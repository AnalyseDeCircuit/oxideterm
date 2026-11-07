//! Scoped launchd identities, schedule metadata and explicit service actions.

use super::*;
pub(super) use crate::launchd::valid_target;
use crate::launchd::{disabled_overrides, parse_services, valid_domain};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

pub(super) fn action_command(
    action: ScheduledTaskActionKind,
) -> Result<ScheduledTaskActionCommand, String> {
    let (target, verb) = match action {
        ScheduledTaskActionKind::RunNow { id, unit } => {
            if unit != id {
                return Err("The launchd task must be loaded before it can run.".into());
            }
            // No -k: an explicit run must not kill an already running job.
            (id, "kickstart -p")
        }
        ScheduledTaskActionKind::Enable { id, source } if source == "launchd" => (id, "enable"),
        ScheduledTaskActionKind::Disable { id, source } if source == "launchd" => (id, "disable"),
        _ => return Err("This action requires a launchd task.".into()),
    };
    if !valid_target(&target) {
        return Err("The launchd task has no valid service target.".into());
    }
    Ok(ScheduledTaskActionCommand {
        command: posix_shell_command(&format!("launchctl {verb} {}", shell_quote(&target))),
        capability: ScheduledTaskCapability::Partial,
    })
}

pub(super) fn parse_snapshot(section: &str) -> Vec<ResourceScheduledTask> {
    let mut entries = HashMap::new();
    let (known_domains, overrides) = disabled_overrides(section);
    for service in parse_services(section) {
        let domain = service.id.rsplit_once('/').unwrap().0;
        let mut task = empty_task(service.id.clone(), service.description, domain);
        task.active = if service.main_pid.is_some() {
            "active"
        } else {
            "inactive"
        }
        .into();
        task.last_result = service.sub_state;
        task.enabled = service.enabled_state;
        task.unit = task.id.clone();
        entries.insert(task.id.clone(), task);
    }
    for line in section
        .lines()
        .filter_map(|line| line.strip_prefix("LAUNCHD_CONFIG\t"))
    {
        let fields = line.splitn(4, '\t').collect::<Vec<_>>();
        let [domain, loaded, path, json] = fields.as_slice() else {
            continue;
        };
        if !valid_domain(domain) {
            continue;
        }
        // Ignore unrecognized properties; raw plist data never becomes an error or log.
        let Ok(config) = serde_json::from_str::<Value>(json) else {
            continue;
        };
        let Some(label) = config.get("Label").and_then(Value::as_str) else {
            continue;
        };
        let id = format!("{domain}/{label}");
        if !valid_target(&id) {
            continue;
        }
        let task = entries
            .entry(id.clone())
            .or_insert_with(|| empty_task(id, label.into(), domain));
        task.description = STANDARD
            .decode(path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default();
        task.unit = if *loaded == "true" {
            task.id.clone()
        } else {
            String::new()
        };
        let disabled = overrides.get(&task.id).copied().or_else(|| {
            known_domains.contains(*domain).then(|| {
                config
                    .get("Disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        });
        task.enabled = enabled_state(disabled).into();
        task.schedule = schedule(&config);
        task.command = config
            .get("ProgramArguments")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(Value::as_str)
                    .map(shell_quote)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|command| !command.is_empty())
            .or_else(|| {
                decoded_string(&config, "ProgramBase64").map(|program| shell_quote(&program))
            })
            .unwrap_or_default();
        if let Some(user) = decoded_string(&config, "UserNameBase64") {
            task.user = user;
        }
    }
    for task in entries.values_mut() {
        if task.enabled == "unknown" {
            let domain = task
                .id
                .rsplit_once('/')
                .map(|(domain, _)| domain)
                .unwrap_or_default();
            task.enabled = enabled_state(overrides.get(&task.id).copied().or_else(|| {
                (task.unit == task.id && known_domains.contains(domain)).then_some(false)
            }))
            .into();
        }
    }
    entries.into_values().collect()
}

fn enabled_state(disabled: Option<bool>) -> &'static str {
    match disabled {
        Some(true) => "disabled",
        Some(false) => "enabled",
        None => "unknown",
    }
}

fn decoded_string(config: &Value, key: &str) -> Option<String> {
    String::from_utf8(STANDARD.decode(config.get(key)?.as_str()?).ok()?).ok()
}

fn empty_task(id: String, name: String, domain: &str) -> ResourceScheduledTask {
    ResourceScheduledTask {
        id,
        name,
        source: "launchd".into(),
        schedule: String::new(),
        command: String::new(),
        user: domain.into(),
        enabled: "unknown".into(),
        active: "unknown".into(),
        last_run: String::new(),
        next_run: String::new(),
        last_result: String::new(),
        description: String::new(),
        unit: String::new(),
    }
}

fn schedule(config: &Value) -> String {
    // Preserve every calendar alternative and trigger. Interval jobs do not expose
    // a reliable next-run timestamp, so do not manufacture one from sampling time.
    [
        "StartInterval",
        "StartCalendarInterval",
        "RunAtLoad",
        "KeepAlive",
        "StartOnMount",
        "WatchPaths",
        "QueueDirectories",
    ]
    .into_iter()
    .filter_map(|key| {
        config
            .get(key)
            .filter(|value| !value.is_null() && **value != Value::Bool(false))
            .map(|value| format!("{key}={value}"))
    })
    .collect::<Vec<_>>()
    .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchd_scopes_keep_same_labels_distinct_and_merge_live_state() {
        let snapshot = parse_scheduled_task_snapshot(concat!(
            "===SCHEDULED_TASKS===\n__OXIDE_SCHEDULE_CAPABILITY__\tpartial\tmacos_launchd\n",
            "LAUNCHD_OVERRIDES\tsystem\nLAUNCHD_OVERRIDES\tgui/501\n",
            "LAUNCHD_DISABLED\tsystem\t\"com.example.backup\" => true\n",
            "LAUNCHD_DISABLED\tgui/501\t\"com.example.backup\" => false\n",
            "LAUNCHD_LIVE\tgui/501\t123\t0\tcom.example.backup\n",
            "LAUNCHD_CONFIG\tsystem\tfalse\tL0xpYnJhcnkvTGF1bmNoRGFlbW9ucy9iYWNrdXAucGxpc3Q=\t{\"Label\":\"com.example.backup\",\"StartInterval\":300,\"Program\":\"/opt/backup\"}\n",
            "LAUNCHD_CONFIG\tgui/501\ttrue\tL3RtcC9iYWNrdXAucGxpc3Q=\t{\"Label\":\"com.example.backup\",\"Disabled\":true,\"StartCalendarInterval\":[{\"Hour\":2,\"Minute\":15},{\"Weekday\":0,\"Hour\":9}],\"ProgramArguments\":[\"/opt/backup\",\"a b\"]}\n",
            "===SCHEDULED_TASKS_END===\n"
        ));
        let tasks = snapshot.entries;
        assert_eq!(
            tasks
                .iter()
                .map(|task| (
                    task.id.as_str(),
                    task.enabled.as_str(),
                    task.active.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("gui/501/com.example.backup", "enabled", "active"),
                ("system/com.example.backup", "disabled", "unknown")
            ]
        );
        assert_eq!(
            tasks[0].schedule,
            "StartCalendarInterval=[{\"Hour\":2,\"Minute\":15},{\"Weekday\":0,\"Hour\":9}]"
        );
        assert_eq!(tasks[0].command, "'/opt/backup' 'a b'");
        assert_eq!(tasks[1].description, "/Library/LaunchDaemons/backup.plist");
        assert_eq!(tasks[1].schedule, "StartInterval=300");
        assert_eq!(tasks[0].last_result, "0");
        assert_eq!(tasks[0].next_run, "");
        assert!(scheduled_task_action_availability(&tasks[0]).can_run_now);
        assert!(!scheduled_task_action_availability(&tasks[1]).can_run_now);
        assert_eq!(
            scheduled_task_action_availability(&tasks[1]).next_toggle,
            ScheduledTaskToggleAction::Enable
        );
    }

    #[test]
    fn launchd_actions_target_only_the_selected_domain_without_restarting_running_jobs() {
        for target in [
            "system/com.example.backup",
            "gui/501/com.example.backup",
            "user/501/com.example.backup",
        ] {
            for (action, expected) in [
                (
                    ScheduledTaskActionKind::RunNow {
                        id: target.into(),
                        unit: target.into(),
                    },
                    format!("launchctl kickstart -p '{target}'"),
                ),
                (
                    ScheduledTaskActionKind::Enable {
                        id: target.into(),
                        source: "launchd".into(),
                    },
                    format!("launchctl enable '{target}'"),
                ),
                (
                    ScheduledTaskActionKind::Disable {
                        id: target.into(),
                        source: "launchd".into(),
                    },
                    format!("launchctl disable '{target}'"),
                ),
            ] {
                let command = build_scheduled_task_action_command("Darwin", action).unwrap();
                assert_eq!(
                    crate::shell::decoded_posix_script(&command.command),
                    expected
                );
            }
        }
        for target in [
            "com.example.backup",
            "gui/nope/job",
            "system/x;touch /tmp/no",
            "system/x/y",
            "system/-x\n",
        ] {
            assert!(
                build_scheduled_task_action_command(
                    "Darwin",
                    ScheduledTaskActionKind::Disable {
                        id: target.into(),
                        source: "launchd".into()
                    }
                )
                .is_err(),
                "{target}"
            );
        }
        assert!(
            action_command(ScheduledTaskActionKind::RunNow {
                id: "system/com.example.backup".into(),
                unit: String::new()
            })
            .is_err()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_probe_reads_plist_scalars_arrays_and_label_instead_of_filename() {
        let home = std::env::temp_dir().join(format!(
            "oxideterm-launchd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let directory = home.join("Library/LaunchAgents");
        std::fs::create_dir_all(&directory).unwrap();
        let plist = directory.join("different-filename.plist");
        std::fs::write(&plist, r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.oxideterm.test.unloaded-schedule</string>
<key>Program</key><string>/opt/synthetic task</string>
<key>UserName</key><string>synthetic-user</string>
<key>StartInterval</key><integer>300</integer>
<key>Disabled</key><true/>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><false/>
<key>StartCalendarInterval</key><array><dict><key>Hour</key><integer>2</integer><key>Minute</key><integer>15</integer></dict></array>
<key>EnvironmentVariables</key><dict><key>TOKEN</key><string>synthetic-secret-must-not-leave-collector</string></dict>
</dict></plist>"#).unwrap();
        // The fixture is only read, never bootstrapped or registered with launchd.
        let command = build_scheduled_task_snapshot_command("Darwin");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &command.command])
            .env("HOME", &home)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&home).unwrap();
        assert!(output.status.success());
        let output = String::from_utf8(output.stdout).unwrap();
        assert!(!output.contains("synthetic-secret-must-not-leave-collector"));
        let snapshot = parse_scheduled_task_snapshot(&output);
        let task = snapshot
            .entries
            .iter()
            .find(|entry| entry.name == "com.oxideterm.test.unloaded-schedule")
            .expect("fixture task");
        assert!(valid_target(&task.id));
        assert_eq!(task.description, plist.to_string_lossy());
        assert_eq!(task.command, "'/opt/synthetic task'");
        assert_eq!(task.user, "synthetic-user");
        assert_eq!(task.enabled, "disabled");
        assert_eq!(
            task.schedule,
            "StartInterval=300; StartCalendarInterval=[{\"Hour\":2,\"Minute\":15}]; RunAtLoad=true"
        );
        assert_eq!(task.unit, "");
        assert!(!scheduled_task_action_availability(task).can_run_now);
        assert!(scheduled_task_action_availability(task).can_toggle_enabled);
    }
}
