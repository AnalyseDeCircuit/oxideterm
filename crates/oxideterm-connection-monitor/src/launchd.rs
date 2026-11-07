//! Shared launchd service identities and read-only domain inventory.

use crate::service::ResourceService;
use std::collections::{HashMap, HashSet};

pub(crate) fn valid_target(target: &str) -> bool {
    let Some((domain, label)) = target.rsplit_once('/') else {
        return false;
    };
    valid_domain(domain)
        && !label.is_empty()
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:@-".contains(c))
}

pub(crate) fn valid_domain(domain: &str) -> bool {
    domain == "system"
        || domain.split_once('/').is_some_and(|(kind, uid)| {
            matches!(kind, "gui" | "user")
                && !uid.is_empty()
                && uid.bytes().all(|c| c.is_ascii_digit())
        })
}

pub(crate) fn disabled_overrides(section: &str) -> (HashSet<String>, HashMap<String, bool>) {
    let mut domains = HashSet::new();
    let mut overrides = HashMap::new();
    for line in section.lines() {
        if let Some(domain) = line
            .strip_prefix("LAUNCHD_OVERRIDES\t")
            .filter(|d| valid_domain(d))
        {
            domains.insert(domain.into());
        } else if let Some(body) = line.strip_prefix("LAUNCHD_DISABLED\t")
            && let Some((domain, property)) = body.split_once('\t')
            && valid_domain(domain)
            && let Some((label, disabled)) =
                property.trim().trim_end_matches(',').split_once(" => ")
            && let Ok(label) = serde_json::from_str::<String>(label)
            && let Ok(disabled) = disabled.parse::<bool>()
        {
            overrides.insert(format!("{domain}/{label}"), disabled);
        }
    }
    (domains, overrides)
}

pub(crate) fn parse_services(section: &str) -> Vec<ResourceService> {
    let (domains, overrides) = disabled_overrides(section);
    section
        .lines()
        .filter_map(|line| {
            let fields = line
                .strip_prefix("LAUNCHD_LIVE\t")?
                .splitn(4, '\t')
                .collect::<Vec<_>>();
            let [domain, pid, result, label] = fields.as_slice() else {
                return None;
            };
            let id = format!("{domain}/{label}");
            if !valid_target(&id) {
                return None;
            }
            let pid = pid
                .parse::<u32>()
                .ok()
                .filter(|pid| *pid != 0)
                .map(|pid| pid.to_string());
            let disabled = overrides
                .get(&id)
                .copied()
                .or_else(|| domains.contains(*domain).then_some(false));
            Some(ResourceService {
                id,
                load_state: "loaded".into(),
                active_state: if pid.is_some() { "running" } else { "inactive" }.into(),
                sub_state: if *result == "-" {
                    String::new()
                } else {
                    (*result).into()
                },
                enabled_state: match disabled {
                    Some(true) => "disabled",
                    Some(false) => "enabled",
                    None => "unknown",
                }
                .into(),
                main_pid: pid,
                description: (*label).into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        ServiceActionKind, build_service_action_command, parse_service_snapshot,
        service_action_availability,
    };

    #[test]
    fn domain_inventory_retains_pid_enabled_state_and_scoped_actions() {
        let snapshot = parse_service_snapshot(concat!(
            "===SERVICES===\n",
            "LAUNCHD_OVERRIDES\tsystem\nLAUNCHD_OVERRIDES\tgui/501\n",
            "LAUNCHD_DISABLED\tsystem\t\"com.example.same\" => true\n",
            "LAUNCHD_LIVE\tsystem\t0\t-\tcom.example.same\n",
            "LAUNCHD_LIVE\tgui/501\t123\t0\tcom.example.same\n===SERVICES_END===\n"
        ));
        assert_eq!(
            snapshot
                .services
                .iter()
                .map(|s| (
                    s.id.as_str(),
                    s.main_pid.as_deref(),
                    s.enabled_state.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("gui/501/com.example.same", Some("123"), "enabled"),
                ("system/com.example.same", None, "disabled")
            ]
        );
        assert!(!service_action_availability(&snapshot.services[0]).can_reload);
        for (action, verb) in [
            (ServiceActionKind::Start, "kickstart -p"),
            (ServiceActionKind::Restart, "kickstart -kp"),
            (ServiceActionKind::Stop, "kill TERM"),
        ] {
            let command =
                build_service_action_command("Darwin", "gui/501/com.example.same", action).unwrap();
            assert_eq!(
                crate::shell::decoded_posix_script(&command.command),
                format!("launchctl {verb} 'gui/501/com.example.same'")
            );
        }
    }
}
