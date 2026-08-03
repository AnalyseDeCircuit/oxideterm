// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn start_rdp_worker(
    config: RdpWorkerConfig,
    writer: SharedEventWriter,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("oxideterm-rdp-session".to_string())
        .spawn(move || {
            if let Err(error) = run_rdp_worker(config, writer.clone(), request_rx) {
                let _ = send_event(
                    &writer,
                    RemoteDesktopHelperEvent::ConnectionFailure {
                        category: Some(remote_desktop_error_category_from_message(&error)),
                        message: error,
                    },
                );
            }
        })
        .expect("failed to start RDP helper worker")
}

pub(super) fn run_rdp_worker(
    mut config: RdpWorkerConfig,
    writer: SharedEventWriter,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
) -> Result<(), String> {
    let mut reconnecting = false;
    loop {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::Status {
                status: if reconnecting {
                    RemoteDesktopSessionStatus::Reconnecting
                } else {
                    RemoteDesktopSessionStatus::Connecting
                },
                message: Some(if reconnecting {
                    "Reopening RDP session.".to_string()
                } else {
                    "Opening RDP session.".to_string()
                }),
            },
        )?;

        let client = start_client_rdp_session(&config)?;
        let read_only = config.read_only;
        let exit = run_client_rdp_loop(
            &writer,
            &request_rx,
            &client.input_tx,
            client.output_rx,
            &mut config,
            read_only,
        )?;
        let _ = client.input_tx.send(RdpInputEvent::Close);
        let _ = client.join_handle.join();

        match exit {
            ClientRdpSessionExit::Closed => {
                return send_event(
                    &writer,
                    RemoteDesktopHelperEvent::Disconnected {
                        reason: Some("RDP session closed.".to_string()),
                    },
                );
            }
            ClientRdpSessionExit::ReconnectRequested => {
                reconnecting = true;
            }
            ClientRdpSessionExit::RemoteEnded(reason) => {
                return send_event(
                    &writer,
                    RemoteDesktopHelperEvent::Disconnected {
                        reason: sanitize_rdp_disconnect_reason(reason.as_deref()),
                    },
                );
            }
            ClientRdpSessionExit::ConnectionFailed { message, category } => {
                return send_event(
                    &writer,
                    RemoteDesktopHelperEvent::ConnectionFailure {
                        message,
                        category: Some(category),
                    },
                );
            }
        }
    }
}

pub(super) fn start_client_rdp_session(
    config: &RdpWorkerConfig,
) -> Result<ClientRdpSession, String> {
    let client_config = build_client_rdp_config(config)?;
    let (input_tx, input_rx) = tokio_mpsc::unbounded_channel();
    let client_input_tx = input_tx.clone();
    let (client_output_tx, client_output_rx) =
        client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);

    let join_handle = thread::Builder::new()
        .name("oxideterm-rdp-client".to_string())
        .spawn(move || {
            run_client_rdp_thread(client_config, input_rx, client_input_tx, client_output_tx)
        })
        .map_err(|error| format!("RDP client thread startup failed: {error}"))?;

    Ok(ClientRdpSession {
        input_tx,
        output_rx: client_output_rx,
        join_handle,
    })
}

pub(super) fn run_client_rdp_thread(
    config: ClientRdpConfig,
    mut input_rx: tokio_mpsc::UnboundedReceiver<RdpInputEvent>,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    client_output_tx: ClientRdpOutputSender,
) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        let _ = client_output_tx.send_control(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP async runtime startup failed.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Dependency),
            },
        ));
        return;
    };

    runtime.block_on(async move {
        loop {
            let (connection_result, framed, egfx_bridge) = match connect_native_rdp(
                &config,
                &mut input_rx,
                input_tx.clone(),
                client_output_tx.clone(),
            )
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    let _ =
                        client_output_tx.send_control(ClientRdpOutput::ConnectionFailure(error));
                    break;
                }
            };
            match run_native_rdp_active_session(
                framed,
                connection_result,
                config.graphics_epoch,
                &mut input_rx,
                &client_output_tx,
                &egfx_bridge,
            )
            .await
            {
                Ok(ClientRdpControlFlow::TerminatedGracefully(reason)) => {
                    let _ = client_output_tx.send_control(ClientRdpOutput::Terminated(
                        format_graceful_disconnect(reason),
                    ));
                    break;
                }
                Err(error) => {
                    let _ = client_output_tx.send_control(ClientRdpOutput::Terminated(format!(
                        "RDP session ended: {error}"
                    )));
                    break;
                }
            }
        }
        let _ = client_output_tx.send_control(ClientRdpOutput::OutputEnded);
    });
}
