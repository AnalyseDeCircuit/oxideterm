// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Interactive terminal operations.
//!
//! These run inside the user's own terminal tab, so the app neither captures
//! output nor audits the composed arguments. Free-form `run`/`build` argv text
//! is the user's typing; identifiers and paths are validated bounds only, and
//! every interpolated value is single-quote escaped before it reaches a shell.

use crate::container::engine::ContainerEngine;
use crate::container::instance::ContainerInstance;
use crate::container::quote::{
    is_windows, quote_argument, validated_container_id, validated_resource_name,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContainerTerminalOp {
    /// Raw `run` arguments authored by the user in the input dialog.
    Run {
        args: String,
    },
    Attach {
        container_id: String,
    },
    Pull {
        reference: String,
    },
    Push {
        reference: String,
    },
    /// Raw `build` arguments authored by the user in the input dialog.
    Build {
        args: String,
    },
    Save {
        reference: String,
        path: String,
    },
    Load {
        path: String,
    },
    Import {
        path: String,
        reference: String,
    },
    Export {
        container_id: String,
        path: String,
    },
    Copy {
        source: String,
        destination: String,
    },
    Login {
        registry: String,
    },
}

pub fn terminal_command(
    os_type: &str,
    instance: &ContainerInstance,
    op: ContainerTerminalOp,
) -> Result<String, String> {
    let prefix = instance.cli_prefix_for(os_type);
    // Every operation is wrapped in PowerShell on Windows so the line is parsed
    // in one dialect. `run`/`build` still pass the user's argv through verbatim;
    // only the wrapper is added.
    let wrap_for_windows = is_windows(os_type);
    let quote = |value: &str| quote_argument(os_type, value);
    let command = match op {
        ContainerTerminalOp::Run { args } => {
            let args = raw_arguments(&args, "run")?;
            ensure_wrapper_safe(os_type, args)?;
            format!("{prefix} run {args}")
        }
        ContainerTerminalOp::Build { args } => {
            let args = raw_arguments(&args, "build")?;
            ensure_wrapper_safe(os_type, args)?;
            format!("{prefix} build {args}")
        }
        ContainerTerminalOp::Attach { container_id } => {
            let container_id = validated_container_id(&container_id)?;
            format!("{prefix} attach {container_id}")
        }
        ContainerTerminalOp::Pull { reference } => {
            let reference = validated_resource_name(&reference)?;
            format!("{prefix} pull {}", quote(reference))
        }
        ContainerTerminalOp::Push { reference } => {
            let reference = validated_resource_name(&reference)?;
            format!("{prefix} push {}", quote(reference))
        }
        ContainerTerminalOp::Save { reference, path } => {
            let reference = validated_resource_name(&reference)?;
            let path = validated_path(&path)?;
            format!("{prefix} save -o {} {}", quote(path), quote(reference))
        }
        ContainerTerminalOp::Load { path } => {
            let path = validated_path(&path)?;
            format!("{prefix} load -i {}", quote(path))
        }
        ContainerTerminalOp::Import { path, reference } => {
            let path = validated_path(&path)?;
            let reference = validated_resource_name(&reference)?;
            format!("{prefix} import {} {}", quote(path), quote(reference))
        }
        ContainerTerminalOp::Export { container_id, path } => {
            let container_id = validated_container_id(&container_id)?;
            let path = validated_path(&path)?;
            format!("{prefix} export -o {} {container_id}", quote(path))
        }
        ContainerTerminalOp::Copy {
            source,
            destination,
        } => {
            let source = validated_path(&source)?;
            let destination = validated_path(&destination)?;
            format!("{prefix} cp {} {}", quote(source), quote(destination))
        }
        ContainerTerminalOp::Login { registry } => {
            let registry = validated_resource_name(&registry)?;
            format!("{prefix} login {}", quote(registry))
        }
    };
    Ok(if wrap_for_windows {
        crate::container::windows::powershell_command(&command)
    } else {
        command
    })
}

fn raw_arguments<'a>(args: &'a str, verb: &str) -> Result<&'a str, String> {
    let args = args.trim();
    if args.is_empty() {
        return Err(format!("{verb} arguments are required."));
    }
    if args.chars().any(|character| character.is_control()) {
        return Err("Control characters are not allowed.".to_string());
    }
    Ok(args)
}

/// Paths may contain separators, spaces and drive letters but never control
/// characters or double quotes. The Windows wrapper delimits the PowerShell
/// script with `"` at the cmd.exe level, so a value containing one would end
/// the generated line and run the remainder as a host command; Win32 paths
/// cannot contain a double quote anyway.
fn validated_path(path: &str) -> Result<&str, String> {
    let path = path.trim();
    if path.is_empty() || path.len() > 512 {
        return Err("Invalid path.".to_string());
    }
    if path.chars().any(|character| character.is_control()) {
        return Err("Control characters are not allowed in paths.".to_string());
    }
    if path.contains('"') {
        return Err("Double quotes are not allowed in paths.".to_string());
    }
    Ok(path)
}

/// The cmd.exe wrapper delimits the PowerShell line with double quotes, so a
/// user-authored `"` would end the script early. PowerShell literals only need
/// single quotes, so the value is rejected instead of escaped.
fn ensure_wrapper_safe(os_type: &str, args: &str) -> Result<(), String> {
    if is_windows(os_type) && args.contains('"') {
        return Err("Double quotes are not supported in Windows terminal arguments.".to_string());
    }
    Ok(())
}

#[allow(dead_code)]
fn engine_supports_attach(engine: ContainerEngine) -> bool {
    // Attach is available on every engine, including `wslc`.
    matches!(
        engine,
        ContainerEngine::Docker | ContainerEngine::Podman | ContainerEngine::Wslc
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::transport::ContainerTransport;

    fn podman() -> ContainerInstance {
        ContainerInstance::new(
            ContainerEngine::Podman,
            ContainerTransport::Socket {
                endpoint: "unix:///run/user/1000/podman/podman.sock".into(),
            },
            None,
        )
    }

    #[test]
    fn run_and_build_keep_user_arguments_and_prefix() {
        assert_eq!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Run {
                    args: "-d --name web nginx:alpine".into()
                }
            )
            .unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' run -d --name web nginx:alpine"
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Run { args: "  ".into() }
            )
            .is_err()
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Build {
                    args: "x\nrm".into()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn file_operations_quote_paths_and_references() {
        assert_eq!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Save {
                    reference: "docker.io/library/nginx:alpine".into(),
                    path: "/tmp/my image.tar".into()
                }
            )
            .unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' save -o '/tmp/my image.tar' 'docker.io/library/nginx:alpine'"
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Export {
                    container_id: "a1b2c3d4e5f6".into(),
                    path: "/tmp/app.tar".into()
                }
            )
            .unwrap()
            .ends_with("export -o '/tmp/app.tar' a1b2c3d4e5f6")
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Copy {
                    source: "web:/srv".into(),
                    destination: "/tmp/srv".into()
                }
            )
            .is_ok()
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Load {
                    path: String::new()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn windows_paths_use_powershell_quoting_and_a_single_dialect() {
        let wslc = ContainerInstance::new(ContainerEngine::Wslc, ContainerTransport::Cli, None);
        let save = terminal_command(
            "Windows",
            &wslc,
            ContainerTerminalOp::Save {
                reference: "alpine:latest".into(),
                path: r"C:\Users\O'Brien\image.tar".into(),
            },
        )
        .expect("windows save command");
        // cmd.exe only sees the wrapper; the path keeps PowerShell's `''` escape.
        assert!(save.starts_with("powershell -NoProfile -NonInteractive -Command \""));
        assert!(save.ends_with('"'));
        assert!(save.contains(r"'C:\Users\O''Brien\image.tar'"), "{save}");
        // Raw user argv stays verbatim but now runs inside the same wrapper.
        let run = terminal_command(
            "Windows",
            &wslc,
            ContainerTerminalOp::Run {
                args: "-d nginx".into(),
            },
        )
        .expect("windows run command");
        assert_eq!(
            run,
            "powershell -NoProfile -NonInteractive -Command \"wslc run -d nginx\""
        );
        // A double quote would close the cmd.exe wrapper, so argv carrying one
        // is rejected rather than spliced into the script.
        assert!(
            terminal_command(
                "Windows",
                &wslc,
                ContainerTerminalOp::Run {
                    args: "-e \"A=b\" nginx".into(),
                }
            )
            .is_err()
        );
        // Win32 paths cannot contain a double quote; accepting one would let
        // the path close the PowerShell argument.
        assert!(
            terminal_command(
                "Windows",
                &wslc,
                ContainerTerminalOp::Load {
                    path: "C:\\tmp\\a\"b.tar".into(),
                }
            )
            .is_err()
        );
    }

    #[test]
    fn attach_pull_push_login_use_the_instance_prefix() {
        assert_eq!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Attach {
                    container_id: "a1b2c3d4e5f6".into()
                }
            )
            .unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' attach a1b2c3d4e5f6"
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Pull {
                    reference: "nginx:alpine".into(),
                }
            )
            .unwrap()
            .contains("pull 'nginx:alpine'")
        );
        assert!(
            terminal_command(
                "Linux",
                &podman(),
                ContainerTerminalOp::Login {
                    registry: "ghcr.io".into()
                }
            )
            .unwrap()
            .ends_with("login 'ghcr.io'")
        );
    }
}
