use crate::{
    docker_action_failure_message, docker_action_succeeded, docker_action_success_message,
    process_action_failure_message, process_action_succeeded, process_action_success_message,
    service_action_failure_message, service_action_succeeded, service_action_success_message,
    tmux_action_failure_message, tmux_action_succeeded, tmux_action_success_message,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostToolActionOutcome {
    Succeeded { message: String },
    Failed { message: String },
}

pub fn interpret_docker_action_output(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> HostToolActionOutcome {
    if docker_action_succeeded(exit_code) {
        HostToolActionOutcome::Succeeded {
            message: docker_action_success_message(stdout, stderr),
        }
    } else {
        HostToolActionOutcome::Failed {
            message: docker_action_failure_message(stdout, stderr, exit_code),
        }
    }
}

pub fn interpret_process_action_output(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> HostToolActionOutcome {
    if process_action_succeeded(exit_code) {
        HostToolActionOutcome::Succeeded {
            message: process_action_success_message(stdout, stderr),
        }
    } else {
        HostToolActionOutcome::Failed {
            message: process_action_failure_message(stdout, stderr, exit_code),
        }
    }
}

pub fn interpret_service_action_output(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> HostToolActionOutcome {
    if service_action_succeeded(exit_code) {
        HostToolActionOutcome::Succeeded {
            message: service_action_success_message(stdout, stderr),
        }
    } else {
        HostToolActionOutcome::Failed {
            message: service_action_failure_message(stdout, stderr, exit_code),
        }
    }
}

pub fn interpret_tmux_action_output(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> HostToolActionOutcome {
    if tmux_action_succeeded(exit_code) {
        HostToolActionOutcome::Succeeded {
            message: tmux_action_success_message(stdout, stderr),
        }
    } else {
        HostToolActionOutcome::Failed {
            message: tmux_action_failure_message(stdout, stderr, exit_code),
        }
    }
}

pub fn interpret_scheduled_task_action_output(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    success_message: String,
    unknown_error_message: &str,
) -> HostToolActionOutcome {
    if exit_code.unwrap_or(0) == 0 {
        return HostToolActionOutcome::Succeeded {
            message: success_message,
        };
    }
    let message =
        host_tool_capture_failure_message(stdout, stderr, exit_code, unknown_error_message);
    HostToolActionOutcome::Failed { message }
}

pub fn host_tool_capture_failure_message(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    unknown_error_message: &str,
) -> String {
    let reason = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(unknown_error_message);
    exit_code
        .map(|code| format!("{reason} (exit {code})"))
        .unwrap_or_else(|| reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_interpreters_preserve_domain_specific_messages() {
        assert_eq!(
            interpret_docker_action_output("container-id\n", "", Some(0)),
            HostToolActionOutcome::Succeeded {
                message: "container-id".to_string()
            }
        );
        assert_eq!(
            interpret_service_action_output("", "permission denied", Some(1)),
            HostToolActionOutcome::Failed {
                message: "permission denied".to_string()
            }
        );
        assert_eq!(
            interpret_tmux_action_output("", "no server running", Some(1)),
            HostToolActionOutcome::Failed {
                message: "no server running (exit 1)".to_string()
            }
        );
        assert_eq!(
            interpret_process_action_output("", "invalid pid", Some(1)),
            HostToolActionOutcome::Failed {
                message: "invalid pid".to_string()
            }
        );
    }

    #[test]
    fn scheduled_task_interpreter_keeps_localized_success_and_exit_context() {
        assert_eq!(
            interpret_scheduled_task_action_output(
                "",
                "",
                Some(0),
                "Started backup".to_string(),
                "Unknown error",
            ),
            HostToolActionOutcome::Succeeded {
                message: "Started backup".to_string()
            }
        );
        assert_eq!(
            interpret_scheduled_task_action_output(
                "",
                "denied",
                Some(5),
                String::new(),
                "Unknown error",
            ),
            HostToolActionOutcome::Failed {
                message: "denied (exit 5)".to_string()
            }
        );
    }
}
