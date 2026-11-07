use super::*;

pub(super) fn action_command(verb: &str, id: &str) -> Result<String, String> {
    let (kind, name) = id
        .strip_prefix("brew:")
        .and_then(|id| id.split_once(':'))
        .filter(|(kind, name)| matches!(*kind, "formula" | "cask") && !name.is_empty())
        .ok_or_else(|| "Refresh Homebrew packages before choosing this action.".to_string())?;
    Ok(posix_shell_command(&format!(
        "HOMEBREW_NO_AUTO_UPDATE=1 brew {verb} --{kind} -- {}",
        shell_quote(name)
    )))
}

pub(super) fn parse_inventory(section: &str) -> Vec<ResourcePackageEntry> {
    let outdated = section
        .split_once("BREW_OUTDATED_BEGIN\n")
        .and_then(|(_, tail)| tail.split_once("BREW_OUTDATED_END"))
        .and_then(|(json, _)| serde_json::from_str::<serde_json::Value>(json).ok());
    section
        .lines()
        .filter_map(|line| {
            let fields = line
                .strip_prefix("BREW_INSTALLED\t")?
                .splitn(3, '\t')
                .collect::<Vec<_>>();
            let [kind, name, versions] = fields.as_slice() else {
                return None;
            };
            let key = match *kind {
                "formula" => "formulae",
                "cask" => "casks",
                _ => return None,
            };
            let update = outdated
                .as_ref()
                .and_then(|value| value.get(key)?.as_array())
                .and_then(|items| {
                    items.iter().find(|item| {
                        ["full_name", "name", "token"]
                            .iter()
                            .any(|field| item.get(field).and_then(|v| v.as_str()) == Some(*name))
                    })
                });
            Some(ResourcePackageEntry {
                id: format!("brew:{kind}:{name}"),
                name: (*name).into(),
                manager: "brew".into(),
                installed_version: (*versions).into(),
                candidate_version: update
                    .and_then(|item| item.get("current_version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                arch: String::new(),
                repository: String::new(),
                status: if update.is_some() {
                    "upgradable"
                } else {
                    "installed"
                }
                .into(),
                summary: String::new(),
                service_units: Vec::new(),
                owner_paths: Vec::new(),
                source: format!("brew_{kind}"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_inventory_preserves_formula_and_cask_identity() {
        let snapshot = parse_package_snapshot(concat!(
            "===PACKAGES===\n__OXIDE_PACKAGE_CAPABILITY__\tpartial\tmacos_brew\n",
            "BREW_INSTALLED\tformula\tsame\t1.0 1.1\nBREW_INSTALLED\tcask\tsame\t2.0\nBREW_INSTALLED\tformula\ttap/tools/other\t3.0\n",
            "BREW_OUTDATED_BEGIN\n{\"formulae\":[{\"name\":\"same\",\"current_version\":\"1.2\"}],\"casks\":[]}\nBREW_OUTDATED_END\n===PACKAGES_END===\n"
        ));
        assert_eq!(
            snapshot
                .entries
                .iter()
                .map(|p| (
                    p.id.as_str(),
                    p.installed_version.as_str(),
                    p.status.as_str(),
                    p.candidate_version.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("brew:formula:same", "1.0 1.1", "upgradable", "1.2"),
                ("brew:cask:same", "2.0", "installed", ""),
                ("brew:formula:tap/tools/other", "3.0", "installed", "")
            ]
        );
        let partial = parse_package_snapshot(
            "===PACKAGES===\nBREW_INSTALLED\tformula\tinstalled\t1.0\n__OXIDE_PACKAGE_WARNING__\tcask permission denied\n===PACKAGES_END===",
        );
        assert_eq!(
            partial.status,
            ResourcePackageStatus::Error {
                message: "cask permission denied".into()
            }
        );
        assert_eq!(
            partial
                .entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["brew:formula:installed"]
        );
        for kind in ["formula", "cask"] {
            let id = format!("brew:{kind}:a'$(touch /tmp/untrusted)");
            for (verb, command) in [
                (
                    "upgrade",
                    build_package_upgrade_command("Darwin", "brew", &id).unwrap(),
                ),
                (
                    "uninstall",
                    build_package_uninstall_command("Darwin", "brew", &id).unwrap(),
                ),
            ] {
                assert_eq!(
                    crate::shell::decoded_posix_script(&command.command),
                    format!(
                        "HOMEBREW_NO_AUTO_UPDATE=1 brew {verb} --{kind} -- 'a'\"'\"'$(touch /tmp/untrusted)'"
                    )
                );
            }
        }
    }
}
