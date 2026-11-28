// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Interactive exec dial with remote TTY, normalized to a plain duplex byte
//! stream plus resize control for the terminal session worker.

use std::future::Future;
use std::pin::Pin;

use futures_util::{SinkExt, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status as KubeExitStatus;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::types::{ContainerConnectionConfig, ContainerError, EngineTarget};
use crate::types::{ContainerLaunchMode, ShellFallback, classify_shell_start_error};

/// Shell candidates in fallback order. `/bin/bash` covers interactive
/// distros, `/bin/sh` the POSIX-mandatory symlink, and the bare `sh` defers to
/// the daemon's PATH resolution for minimal images (busybox builds) that ship
/// the binary outside `/bin`. `sh` is POSIX-mandatory in every non-distroless
/// image, so it closes the list before surfacing the failure.
pub const DEFAULT_SHELL_CANDIDATES: [&str; 3] = ["/bin/bash", "/bin/sh", "sh"];
const DUPLEX_BUFFER_BYTES: usize = 64 * 1024;
const TERM_ENV: &str = "TERM=xterm-256color";
/// Some daemon versions upgrade the exec stream (HTTP 200) before the OCI
/// runtime rejects the command, so start_exec never errors; the failure text
/// arrives as the first TTY frames instead.
const OCI_EXEC_FAILURE_MARKER: &str = "oci runtime exec failed";
/// First-frame probe window. Startup failures surface immediately over the
/// socket, while a live shell prints its prompt well within the same window.
const STREAM_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
/// RemoteCommand status probe window. The status rides its own channel and is
/// sent as the process ends, so this only bounds a server that never sends one.
const STATUS_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

type ResizeFuture = Pin<Box<dyn Future<Output = Result<(), ContainerError>> + Send>>;
type ResizeFn = Box<dyn Fn(u16, u16) -> ResizeFuture + Send + Sync>;

pub struct ContainerExecTransport {
    input: Pin<Box<dyn AsyncWrite + Send>>,
    output: Pin<Box<dyn AsyncRead + Send>>,
    resize: Option<ResizeFn>,
}

impl ContainerExecTransport {
    pub async fn write_input(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.input.write_all(bytes).await
    }

    pub async fn read_output(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.output.read(buffer).await
    }

    /// Shuts down the input half; the remote side treats it as the end of the
    /// session and closes the stream, which drains the output to EOF.
    pub async fn close(&mut self) {
        let _ = self.input.shutdown().await;
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), ContainerError> {
        match self.resize.as_ref() {
            Some(resize) => resize(cols, rows).await,
            None => Ok(()),
        }
    }
}

/// Dials the terminal session requested by the launch mode: an interactive
/// shell with candidate fallback, a command line run through the container's
/// POSIX shell, or an attach to the already-running main process TTY.
pub async fn open_container_exec(
    config: &ContainerConnectionConfig,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ContainerError> {
    match &config.launch {
        ContainerLaunchMode::Shell { shell } => {
            let candidates: Vec<Vec<String>> = match shell {
                Some(shell) if !shell.trim().is_empty() => vec![vec![shell.trim().to_string()]],
                _ => DEFAULT_SHELL_CANDIDATES
                    .iter()
                    .map(|shell| vec![shell.to_string()])
                    .collect(),
            };
            run_candidate_execs(config, &candidates, initial_cols, initial_rows).await
        }
        ContainerLaunchMode::Command { command } => {
            let command = command.trim();
            if command.is_empty() {
                // An empty command line carries no intent; fall back to the
                // shell path instead of failing the connection.
                let candidates: Vec<Vec<String>> = DEFAULT_SHELL_CANDIDATES
                    .iter()
                    .map(|shell| vec![shell.to_string()])
                    .collect();
                return run_candidate_execs(config, &candidates, initial_cols, initial_rows).await;
            }
            // The command line is shell-interpreted, so quoting works like an
            // interactive session; the candidate loop keeps the same missing
            // shell fallthrough as the shell mode.
            let candidates: Vec<Vec<String>> = DEFAULT_SHELL_CANDIDATES
                .iter()
                .map(|shell| vec![shell.to_string(), "-c".to_string(), command.to_string()])
                .collect();
            run_candidate_execs(config, &candidates, initial_cols, initial_rows).await
        }
        ContainerLaunchMode::Attach => dial_attach(config, initial_cols, initial_rows).await,
    }
}

/// Engine client built once for the whole candidate loop: kubeconfig loading
/// per candidate would re-run exec credential plugins (EKS/GKE) on every
/// shell fallback, and the docker handle is shared for symmetry.
enum EngineClient {
    Docker(bollard::Docker),
    Kube(kube::Client),
}

/// Tries each argv in order, falling through when the daemon reports the
/// executable is missing; unrelated failures surface immediately.
async fn run_candidate_execs(
    config: &ContainerConnectionConfig,
    candidates: &[Vec<String>],
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ContainerError> {
    let client = match &config.engine {
        EngineTarget::Kube { context, .. } => EngineClient::Kube(
            crate::engine::kube_client_for(context.as_deref())
                .await
                .map(|(client, _)| client)?,
        ),
        EngineTarget::DockerSocket { socket_path } => {
            EngineClient::Docker(crate::engine::connect_daemon(socket_path)?)
        }
    };
    let mut last_missing = None;
    for argv in candidates {
        match dial_exec(config, &client, argv, initial_cols, initial_rows).await {
            Ok(transport) => return Ok(transport),
            Err(ExecAttempt::MissingExecutable(error)) => last_missing = Some(error),
            Err(ExecAttempt::Failed(error)) => return Err(error),
        }
    }
    // The candidate list is a non-empty constant, so the loop always records
    // a missing-executable error before it can exhaust.
    Err(ContainerError::NoShell(
        last_missing.expect("shell candidate list is non-empty"),
    ))
}

enum ExecAttempt {
    /// The daemon accepted the exec but the executable does not exist; the
    /// next shell candidate should be tried.
    MissingExecutable(String),
    Failed(ContainerError),
}

async fn dial_exec(
    config: &ContainerConnectionConfig,
    client: &EngineClient,
    argv: &[String],
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ExecAttempt> {
    match (&config.engine, client) {
        (EngineTarget::DockerSocket { .. }, EngineClient::Docker(docker)) => {
            dial_docker_exec(docker, config, argv, initial_cols, initial_rows).await
        }
        (EngineTarget::Kube { namespace, .. }, EngineClient::Kube(kube_client)) => {
            dial_kube_exec(
                kube_client,
                namespace,
                config,
                argv,
                initial_cols,
                initial_rows,
            )
            .await
        }
        // `run_candidate_execs` builds the client from the same engine target,
        // so a mismatched pair is unreachable.
        _ => unreachable!("the client mirrors the engine target"),
    }
}

async fn dial_docker_exec(
    docker: &bollard::Docker,
    config: &ContainerConnectionConfig,
    argv: &[String],
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ExecAttempt> {
    let options = bollard::exec::CreateExecOptions {
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(true),
        cmd: Some(argv.to_vec()),
        env: Some(vec![TERM_ENV.to_string()]),
        ..Default::default()
    };
    let created = docker
        .create_exec(&config.container_id, options)
        .await
        .map_err(|error| ExecAttempt::Failed(ContainerError::Exec(error.to_string())))?;
    match docker.start_exec(&created.id, None).await {
        Ok(bollard::exec::StartExecResults::Attached { output, input }) => {
            // Some daemon versions report OCI runtime failures over the TTY
            // stream instead of failing start_exec; probe the first frames so
            // the candidate loop can still fall through to the next shell.
            let mut stream = output;
            let mut buffered: Vec<u8> = Vec::new();
            let verdict = match tokio::time::timeout(STREAM_PROBE_TIMEOUT, stream.next()).await {
                Ok(Some(Ok(frame))) => {
                    match &frame {
                        bollard::container::LogOutput::StdOut { message }
                        | bollard::container::LogOutput::StdErr { message }
                        | bollard::container::LogOutput::StdIn { message }
                        | bollard::container::LogOutput::Console { message } => {
                            buffered.extend_from_slice(&message);
                        }
                    }
                    probe_exec_start_failure(&buffered)
                }
                // No early output (or a silent stream error): treat the
                // process as started and continue with the normal transport.
                _ => ExecProbeVerdict::Started,
            };
            match verdict {
                ExecProbeVerdict::Started => {}
                ExecProbeVerdict::MissingExecutable(text) => {
                    return Err(ExecAttempt::MissingExecutable(text));
                }
                ExecProbeVerdict::Failed(text) => {
                    return Err(ExecAttempt::Failed(ContainerError::Exec(text)));
                }
            }
            // The TTY is created at the daemon default size; sync the real pane
            // geometry immediately so top/vi render at the correct columns.
            let _ = docker
                .resize_exec(
                    &created.id,
                    bollard::exec::ResizeExecOptions {
                        width: initial_cols,
                        height: initial_rows,
                    },
                )
                .await;
            // The shared client is borrowed for the dial; the resize closure
            // must own a handle that outlives this call.
            let exec_docker = docker.clone();
            let exec_resize = Box::new(move |cols: u16, rows: u16| {
                let docker = exec_docker.clone();
                let exec_id = created.id.clone();
                Box::pin(async move {
                    docker
                        .resize_exec(
                            &exec_id,
                            bollard::exec::ResizeExecOptions {
                                width: cols,
                                height: rows,
                            },
                        )
                        .await
                        .map_err(|error| ContainerError::Exec(error.to_string()))
                }) as ResizeFuture
            });
            let (resize, output) = pump_docker_output(stream, buffered, exec_resize);
            Ok(ContainerExecTransport {
                input: Box::pin(input),
                output,
                resize: Some(resize),
            })
        }
        Ok(bollard::exec::StartExecResults::Detached) => Err(ExecAttempt::Failed(
            ContainerError::Exec("exec detached unexpectedly".to_string()),
        )),
        Err(error) => {
            if classify_shell_start_error(&error.to_string()) == ShellFallback::MissingExecutable {
                Err(ExecAttempt::MissingExecutable(error.to_string()))
            } else {
                Err(ExecAttempt::Failed(ContainerError::Exec(error.to_string())))
            }
        }
    }
}

/// Decision after probing the first TTY frames of a fresh exec.
#[derive(Debug, Eq, PartialEq)]
enum ExecProbeVerdict {
    /// Not an exec-start failure; the buffered bytes belong to the shell.
    Started,
    MissingExecutable(String),
    Failed(String),
}

/// Pure classifier for the probed startup frames so busybox-style failures
/// that surface over the stream fall through to the next shell candidate.
fn probe_exec_start_failure(buffered: &[u8]) -> ExecProbeVerdict {
    let text = String::from_utf8_lossy(buffered).to_lowercase();
    if text.contains(OCI_EXEC_FAILURE_MARKER) {
        if classify_shell_start_error(&text) == ShellFallback::MissingExecutable {
            ExecProbeVerdict::MissingExecutable(text)
        } else {
            ExecProbeVerdict::Failed(text)
        }
    } else {
        ExecProbeVerdict::Started
    }
}

/// Keeps merging the docker frame stream into a single `AsyncRead` so the
/// terminal worker stays transport-agnostic; probed bytes are replayed ahead
/// of the stream so no early shell output is lost. The caller owns the resize
/// closure because exec and attach resize through different API surfaces.
fn pump_docker_output(
    output: Pin<
        Box<
            dyn futures_util::stream::Stream<
                    Item = Result<bollard::container::LogOutput, bollard::errors::Error>,
                > + Send,
        >,
    >,
    prefix: Vec<u8>,
    resize: ResizeFn,
) -> (ResizeFn, Pin<Box<dyn AsyncRead + Send>>) {
    let (mut duplex_writer, duplex_reader) = tokio::io::duplex(DUPLEX_BUFFER_BYTES);
    tokio::spawn(async move {
        if !prefix.is_empty() && duplex_writer.write_all(&prefix).await.is_err() {
            return;
        }
        let mut output = output;
        while let Some(frame) = output.next().await {
            match frame {
                Ok(bollard::container::LogOutput::StdOut { message })
                | Ok(bollard::container::LogOutput::StdErr { message })
                | Ok(bollard::container::LogOutput::StdIn { message })
                | Ok(bollard::container::LogOutput::Console { message }) => {
                    if duplex_writer.write_all(&message).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = duplex_writer.shutdown().await;
    });
    (resize, Box::pin(duplex_reader))
}

/// Interactive TTY params for one pod container: `pod_container` names the
/// target inside a multi-container pod; `None` lets the API server pick the
/// pod's first container.
fn kube_tty_params(pod_container: Option<&str>) -> kube::api::AttachParams {
    let mut params =
        kube::api::AttachParams::interactive_tty().max_stdout_buf_size(DUPLEX_BUFFER_BYTES);
    params.container = pod_container.map(str::to_string);
    params
}

/// kubelet refuses exec/attach without a container name when the pod has more
/// than one container. The form pins the first spec container for such pods,
/// but a saved profile loses its pin once that container is renamed or
/// removed, so the raw BadRequest would dead-end a target that is still
/// usable. Detecting the response lets the dial retry with the same first
/// container the form would have pinned.
fn needs_default_container(error: &kube::Error) -> bool {
    matches!(
        error,
        kube::Error::Api(response)
            if response.code == 400
                && response.message.contains("a container name must be specified")
    )
}

/// First spec container of the pod, the same default the form pins.
async fn first_pod_container(pods: &kube::Api<Pod>, pod: &str) -> Option<String> {
    let pod = pods.get(pod).await.ok()?;
    pod.spec?
        .containers
        .first()
        .map(|container| container.name.clone())
}

/// Reads the RemoteCommand terminal `Status` after an instant stream EOF.
/// kubelet reports a failed exec start on the status channel (channel 3)
/// instead of stderr, so this is the only signal that separates a missing
/// executable from a command that finished silently. `None` means the server
/// closed the stream without sending a status.
async fn probe_kube_exit_status(
    attached: &mut kube::api::AttachedProcess,
) -> Option<ExecProbeVerdict> {
    let status = attached.take_status()?;
    let status = tokio::time::timeout(STATUS_PROBE_TIMEOUT, status)
        .await
        .ok()??;
    Some(classify_kube_exit_status(&status))
}

/// Maps a terminal `Status` onto the probe verdict. A process that actually
/// ran must not be retried, because its side effects already happened; only a
/// runtime start failure that names a missing executable falls through to the
/// next shell candidate, the same way the docker path classifies its errors.
fn classify_kube_exit_status(status: &KubeExitStatus) -> ExecProbeVerdict {
    let reason = status.reason.as_deref().unwrap_or_default();
    if status.status.as_deref() == Some("Success") || reason == "NonZeroExitCode" {
        return ExecProbeVerdict::Started;
    }
    let message = status.message.as_deref().unwrap_or_default();
    if classify_shell_start_error(message) == ShellFallback::MissingExecutable {
        ExecProbeVerdict::MissingExecutable(message.to_string())
    } else if message.is_empty() {
        ExecProbeVerdict::Failed(format!("exec failed with status {reason}"))
    } else {
        ExecProbeVerdict::Failed(message.to_string())
    }
}

async fn dial_kube_exec(
    client: &kube::Client,
    namespace: &str,
    config: &ContainerConnectionConfig,
    argv: &[String],
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ExecAttempt> {
    let pods: kube::Api<Pod> = kube::Api::namespaced(client.clone(), namespace);
    let mut params = kube_tty_params(config.pod_container.as_deref());
    let mut attached = match pods
        .exec(&config.container_id, argv.to_vec(), &params)
        .await
    {
        Ok(attached) => attached,
        Err(error) if config.pod_container.is_none() && needs_default_container(&error) => {
            let Some(container) = first_pod_container(&pods, &config.container_id).await else {
                return Err(ExecAttempt::Failed(ContainerError::Exec(error.to_string())));
            };
            params.container = Some(container);
            pods.exec(&config.container_id, argv.to_vec(), &params)
                .await
                .map_err(|error| ExecAttempt::Failed(ContainerError::Exec(error.to_string())))?
        }
        Err(error) => {
            return Err(ExecAttempt::Failed(ContainerError::Exec(error.to_string())));
        }
    };
    let input = attached.stdin().ok_or_else(|| {
        ExecAttempt::Failed(ContainerError::Exec("stdin unavailable".to_string()))
    })?;
    let mut stdout = attached.stdout().ok_or_else(|| {
        ExecAttempt::Failed(ContainerError::Exec("stdout unavailable".to_string()))
    })?;
    let resize_tx = attached.terminal_size();
    // The API call succeeds even when the OCI runtime rejects the executable;
    // the failure text arrives as the first TTY frames, so the candidate loop
    // can only fall through on a probed startup verdict (same contract as the
    // docker exec probe).
    let mut probe_buf = vec![0_u8; DUPLEX_BUFFER_BYTES];
    let mut buffered: Vec<u8> = Vec::new();
    let verdict = match tokio::time::timeout(STREAM_PROBE_TIMEOUT, stdout.read(&mut probe_buf))
        .await
    {
        // An instant zero-byte EOF happens both when the process never
        // started and when a command finished without output; the TTY
        // stream cannot tell them apart. kubelet answers the former on the
        // status channel, so the status decides: a start failure naming a
        // missing executable tries the next shell candidate (the process
        // never ran, so nothing is repeated), while Success or
        // NonZeroExitCode means it ran and must not be re-executed.
        Ok(Ok(0)) => match probe_kube_exit_status(&mut attached).await {
            Some(verdict) => verdict,
            // No status (older server): keep the phase check so a dead
            // pod still reports its real state, and stay with the per-mode
            // default otherwise.
            None => {
                let command_launch = matches!(config.launch, ContainerLaunchMode::Command { .. });
                let missing = || ExecProbeVerdict::MissingExecutable(config.container_id.clone());
                match pods.get(&config.container_id).await {
                    Ok(pod) => {
                        let phase = pod
                            .status
                            .and_then(|status| status.phase)
                            .unwrap_or_default();
                        if !phase.is_empty() && phase != "Running" {
                            ExecProbeVerdict::Failed(format!(
                                "pod {} is not running (phase {phase})",
                                config.container_id
                            ))
                        } else if command_launch {
                            ExecProbeVerdict::Started
                        } else {
                            missing()
                        }
                    }
                    // A status read failure (e.g. pods/get is not
                    // granted) keeps the per-mode default: the exec API
                    // call itself already succeeded.
                    Err(_) if command_launch => ExecProbeVerdict::Started,
                    Err(_) => missing(),
                }
            }
        },
        Ok(Ok(read)) => {
            buffered.extend_from_slice(&probe_buf[..read]);
            probe_exec_start_failure(&buffered)
        }
        // No early output: treat the process as started and continue with the
        // normal transport; the shell prompt lands after the probe window.
        Err(_elapsed) => ExecProbeVerdict::Started,
        // A transport error before any byte: the stream itself broke, so
        // the dial fails instead of silently opening a dead terminal.
        Ok(Err(error)) => ExecProbeVerdict::Failed(error.to_string()),
    };
    match verdict {
        ExecProbeVerdict::Started => {
            send_initial_size(&resize_tx, initial_cols, initial_rows).await
        }
        ExecProbeVerdict::MissingExecutable(text) => {
            return Err(ExecAttempt::MissingExecutable(text));
        }
        ExecProbeVerdict::Failed(text) => {
            return Err(ExecAttempt::Failed(ContainerError::Exec(text)));
        }
    }
    let (resize, output) = pump_kube_output(attached, stdout, resize_tx, buffered);
    Ok(ContainerExecTransport {
        input: Box::pin(input),
        output,
        resize: Some(resize),
    })
}

/// Binds the terminal to the container/pod main process TTY instead of
/// spawning a fresh exec. Resizes go through the container-level resize API.
async fn dial_attach(
    config: &ContainerConnectionConfig,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ContainerError> {
    match &config.engine {
        EngineTarget::DockerSocket { socket_path } => {
            dial_docker_attach(socket_path, config, initial_cols, initial_rows).await
        }
        EngineTarget::Kube { context, namespace } => {
            dial_kube_attach(
                context.as_deref(),
                namespace,
                config,
                initial_cols,
                initial_rows,
            )
            .await
        }
    }
}

async fn dial_docker_attach(
    socket_path: &str,
    config: &ContainerConnectionConfig,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ContainerError> {
    let docker = crate::engine::connect_daemon(socket_path)?;
    let options = bollard::query_parameters::AttachContainerOptions {
        stream: true,
        stdin: true,
        stdout: true,
        stderr: true,
        ..Default::default()
    };
    let attached = docker
        .attach_container(&config.container_id, Some(options))
        .await
        .map_err(|error| ContainerError::Exec(error.to_string()))?;
    // The main-process TTY keeps its daemon default size until the container
    // resize endpoint is called; a late failure only distorts the first frame.
    let _ = docker
        .resize_container_tty(
            &config.container_id,
            bollard::query_parameters::ResizeContainerTTYOptions {
                h: i32::from(initial_rows),
                w: i32::from(initial_cols),
            },
        )
        .await;
    // Clone the id out of `config` so the resize closure is 'static.
    let container_id = config.container_id.clone();
    let container_resize = Box::new(move |cols: u16, rows: u16| {
        let docker = docker.clone();
        let container_id = container_id.clone();
        Box::pin(async move {
            docker
                .resize_container_tty(
                    &container_id,
                    bollard::query_parameters::ResizeContainerTTYOptions {
                        h: i32::from(rows),
                        w: i32::from(cols),
                    },
                )
                .await
                .map_err(|error| ContainerError::Exec(error.to_string()))
        }) as ResizeFuture
    });
    let (resize, output) = pump_docker_output(attached.output, Vec::new(), container_resize);
    Ok(ContainerExecTransport {
        input: attached.input,
        output,
        resize: Some(resize),
    })
}

async fn dial_kube_attach(
    context: Option<&str>,
    namespace: &str,
    config: &ContainerConnectionConfig,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<ContainerExecTransport, ContainerError> {
    let (kube_client_handle, _) = crate::engine::kube_client_for(context).await?;
    // Distinguish config failures from attach failures so the form shows
    // the actionable message instead of a bare exec error.
    let pods: kube::Api<Pod> = kube::Api::namespaced(kube_client_handle, namespace);
    let mut params = kube_tty_params(config.pod_container.as_deref());
    let mut attached = match pods.attach(&config.container_id, &params).await {
        Ok(attached) => attached,
        Err(error) if config.pod_container.is_none() && needs_default_container(&error) => {
            let Some(container) = first_pod_container(&pods, &config.container_id).await else {
                return Err(ContainerError::Exec(error.to_string()));
            };
            params.container = Some(container);
            pods.attach(&config.container_id, &params)
                .await
                .map_err(|error| ContainerError::Exec(error.to_string()))?
        }
        Err(error) => return Err(ContainerError::Exec(error.to_string())),
    };
    let input = attached
        .stdin()
        .ok_or_else(|| ContainerError::Exec("stdin unavailable".to_string()))?;
    let stdout = attached
        .stdout()
        .ok_or_else(|| ContainerError::Exec("stdout unavailable".to_string()))?;
    let resize_tx = attached.terminal_size();
    send_initial_size(&resize_tx, initial_cols, initial_rows).await;
    let (resize, output) = pump_kube_output(attached, stdout, resize_tx, Vec::new());
    Ok(ContainerExecTransport {
        input: Box::pin(input),
        output,
        resize: Some(resize),
    })
}

/// Syncs the pane geometry once before the pump takes over the stream. The
/// TTY is created at the API default size, and the pane's debounced resize
/// only fires on later layout changes, so without this the first frames render
/// at the wrong dimensions.
async fn send_initial_size(
    resize_tx: &Option<futures_channel::mpsc::Sender<kube::api::TerminalSize>>,
    cols: u16,
    rows: u16,
) {
    if let Some(mut sender) = resize_tx.clone() {
        let _ = sender
            .send(kube::api::TerminalSize {
                width: cols,
                height: rows,
            })
            .await;
    }
}

/// Owns the `AttachedProcess` for the lifetime of the pump so its background
/// message loop keeps running; dropping it would abort the websocket task and
/// kill the session mid-stream.
fn pump_kube_output(
    attached: kube::api::AttachedProcess,
    stdout: tokio::io::ReadHalf<tokio::io::SimplexStream>,
    resize_tx: Option<futures_channel::mpsc::Sender<kube::api::TerminalSize>>,
    prefix: Vec<u8>,
) -> (ResizeFn, Pin<Box<dyn AsyncRead + Send>>) {
    let (mut duplex_writer, duplex_reader) = tokio::io::duplex(DUPLEX_BUFFER_BYTES);
    tokio::spawn(async move {
        // Keep the attached process alive until the server closes the stream;
        // its Drop impl aborts the websocket task.
        let _attached = attached;
        if !prefix.is_empty() && duplex_writer.write_all(&prefix).await.is_err() {
            return;
        }
        let mut stdout = stdout;
        let mut buffer = vec![0_u8; DUPLEX_BUFFER_BYTES];
        loop {
            match stdout.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    if duplex_writer.write_all(&buffer[..read]).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = duplex_writer.shutdown().await;
    });
    let resize = Box::new(move |cols: u16, rows: u16| {
        match &resize_tx {
            Some(sender) => {
                let mut sender = sender.clone();
                Box::pin(async move {
                    sender
                        .send(kube::api::TerminalSize {
                            width: cols,
                            height: rows,
                        })
                        .await
                        // The channel dies only with the session itself; a
                        // late resize on a closing session is a no-op.
                        .map_err(|_| {
                            ContainerError::Exec("terminal size channel closed".to_string())
                        })
                }) as ResizeFuture
            }
            None => Box::pin(async { Ok(()) }) as ResizeFuture,
        }
    });
    (resize, Box::pin(duplex_reader))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_shell_takes_priority_over_candidates() {
        // Candidate ordering drives the fallback loop; explicit configuration
        // must not grow the candidate list.
        let config = ContainerConnectionConfig {
            engine: EngineTarget::DockerSocket {
                socket_path: "/var/run/docker.sock".into(),
            },
            container_id: "abc123".into(),
            label: "web".into(),
            namespace: None,
            pod_container: None,
            launch: ContainerLaunchMode::Shell {
                shell: Some("/usr/bin/zsh".into()),
            },
        };
        let candidates: Vec<Vec<String>> = match &config.launch {
            ContainerLaunchMode::Shell { shell: Some(shell) } if !shell.trim().is_empty() => {
                vec![vec![shell.trim().to_string()]]
            }
            _ => DEFAULT_SHELL_CANDIDATES
                .iter()
                .map(|shell| vec![shell.to_string()])
                .collect(),
        };
        assert_eq!(
            candidates,
            vec![vec!["/usr/bin/zsh".to_string()]],
            "explicit shell yields exactly one candidate"
        );
        // A command line is shell-interpreted; every shell candidate wraps it.
        let command_config = ContainerConnectionConfig {
            launch: ContainerLaunchMode::Command {
                command: "top -d 1".into(),
            },
            ..config.clone()
        };
        if let ContainerLaunchMode::Command { command } = &command_config.launch {
            let wrapped: Vec<Vec<String>> = DEFAULT_SHELL_CANDIDATES
                .iter()
                .map(|shell| vec![shell.to_string(), "-c".to_string(), command.clone()])
                .collect();
            assert_eq!(wrapped.len(), DEFAULT_SHELL_CANDIDATES.len());
            assert_eq!(
                wrapped[0],
                vec![
                    "/bin/bash".to_string(),
                    "-c".to_string(),
                    "top -d 1".to_string()
                ]
            );
        } else {
            panic!("command mode must stay a Command variant");
        }
    }

    #[test]
    fn default_candidates_are_ordered_posix_fallbacks() {
        assert_eq!(
            DEFAULT_SHELL_CANDIDATES,
            ["/bin/bash", "/bin/sh", "sh"],
            "bash first, POSIX sh paths, then the PATH-resolved bare sh"
        );
    }

    #[test]
    fn stream_probe_classifies_oci_failures_that_skip_start_exec() {
        // Daemons that upgrade the stream before the runtime rejects the
        // command emit exactly this text over the TTY; it must fall through
        // to the next shell candidate instead of failing the connection.
        let missing = b"OCI runtime exec failed: exec failed: unable to start \
container process: exec: \"/bin/bash\": stat /bin/bash: no such file or directory";
        assert!(matches!(
            probe_exec_start_failure(missing),
            ExecProbeVerdict::MissingExecutable(_)
        ));
        // Runtime rejections without a missing executable are hard failures.
        assert!(matches!(
            probe_exec_start_failure(b"OCI runtime exec failed: permission denied"),
            ExecProbeVerdict::Failed(_)
        ));
        // Normal first frames (prompt, banner, MOTD) keep the transport.
        assert!(matches!(
            probe_exec_start_failure(b"# \r\nbusybox shell ready\r\n"),
            ExecProbeVerdict::Started
        ));
    }

    fn exit_status(status: &str, reason: &str, message: &str) -> KubeExitStatus {
        KubeExitStatus {
            status: Some(status.to_string()),
            reason: Some(reason.to_string()),
            message: Some(message.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn exit_status_separates_missing_shell_from_executed_command() {
        // k3s closes the TTY stream with zero bytes in both cases, so the
        // status channel is the only discriminator: a command that ran must
        // never be re-executed by the fallback chain.
        assert_eq!(
            classify_kube_exit_status(&exit_status("Success", "", "")),
            ExecProbeVerdict::Started
        );
        assert_eq!(
            classify_kube_exit_status(&exit_status(
                "Failure",
                "NonZeroExitCode",
                "command terminated with exit code 7"
            )),
            ExecProbeVerdict::Started,
            "a command that exited non-zero already ran"
        );
        match classify_kube_exit_status(&exit_status(
            "Failure",
            "InternalError",
            "OCI runtime exec failed: exec: \"/bin/bash\": stat /bin/bash: \
no such file or directory",
        )) {
            ExecProbeVerdict::MissingExecutable(message) => {
                assert!(message.contains("no such file or directory"))
            }
            other => panic!("expected a missing executable, got {other:?}"),
        }
        // A runtime start failure that is not a missing executable must stop
        // the candidate chain instead of trying another shell.
        match classify_kube_exit_status(&exit_status(
            "Failure",
            "InternalError",
            "OCI runtime exec failed: permission denied",
        )) {
            ExecProbeVerdict::Failed(message) => assert!(message.contains("permission denied")),
            other => panic!("expected a hard failure, got {other:?}"),
        }
    }
}
