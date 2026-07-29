// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{LauncherAppEntry, WslDistro};

fn normalized_query(query: &str) -> String {
    query.trim().to_ascii_lowercase()
}

fn app_matches_query(app: &LauncherAppEntry, query: &str) -> bool {
    app.name.to_ascii_lowercase().contains(query)
        || app
            .bundle_id
            .as_ref()
            .is_some_and(|bundle_id| bundle_id.to_ascii_lowercase().contains(query))
}

fn wsl_distro_matches_query(distro: &WslDistro, query: &str) -> bool {
    distro.name.to_ascii_lowercase().contains(query)
}

pub fn count_label(filtered: usize, total: usize) -> String {
    if filtered != total {
        format!("{filtered}/{total}")
    } else {
        total.to_string()
    }
}

pub fn filter_apps(apps: &[LauncherAppEntry], query: &str) -> Vec<LauncherAppEntry> {
    let query = normalized_query(query);
    if query.is_empty() {
        return apps.to_vec();
    }
    apps.iter()
        .filter(|app| app_matches_query(app, &query))
        .cloned()
        .collect()
}

pub fn filter_wsl_distros(distros: &[WslDistro], query: &str) -> Vec<WslDistro> {
    let query = normalized_query(query);
    if query.is_empty() {
        return distros.to_vec();
    }
    distros
        .iter()
        .filter(|distro| wsl_distro_matches_query(distro, &query))
        .cloned()
        .collect()
}

/// Returns catalog indices so UI virtualization can clone only visible entries.
pub fn filter_app_indices(apps: &[LauncherAppEntry], query: &str) -> Vec<usize> {
    let query = normalized_query(query);
    apps.iter()
        .enumerate()
        .filter_map(|(index, app)| {
            (query.is_empty() || app_matches_query(app, &query)).then_some(index)
        })
        .collect()
}

/// Returns distro indices so UI virtualization can clone only visible rows.
pub fn filter_wsl_distro_indices(distros: &[WslDistro], query: &str) -> Vec<usize> {
    let query = normalized_query(query);
    distros
        .iter()
        .enumerate()
        .filter_map(|(index, distro)| {
            (query.is_empty() || wsl_distro_matches_query(distro, &query)).then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_tauri_name_or_bundle_id() {
        let apps = vec![
            LauncherAppEntry {
                name: "Safari".to_string(),
                path: "/Applications/Safari.app".to_string(),
                bundle_id: Some("com.apple.Safari".to_string()),
                icon_path: None,
            },
            LauncherAppEntry {
                name: "Calendar".to_string(),
                path: "/System/Applications/Calendar.app".to_string(),
                bundle_id: Some("com.apple.iCal".to_string()),
                icon_path: None,
            },
        ];
        assert_eq!(filter_apps(&apps, "saf").len(), 1);
        assert_eq!(filter_apps(&apps, "ical")[0].name, "Calendar");
        assert_eq!(filter_apps(&apps, "missing").len(), 0);
    }

    #[test]
    fn count_label_matches_tauri_header() {
        assert_eq!(count_label(4, 10), "4/10");
        assert_eq!(count_label(10, 10), "10");
    }

    #[test]
    fn filter_wsl_distros_matches_tauri_name_filter() {
        let distros = vec![
            WslDistro {
                name: "Ubuntu".to_string(),
                is_default: true,
                is_running: true,
            },
            WslDistro {
                name: "Debian".to_string(),
                is_default: false,
                is_running: false,
            },
        ];
        assert_eq!(filter_wsl_distros(&distros, "ubu")[0].name, "Ubuntu");
        assert!(filter_wsl_distros(&distros, "missing").is_empty());
    }

    #[test]
    fn filter_indices_preserve_catalog_order_without_cloning_entries() {
        let apps = vec![
            LauncherAppEntry {
                name: "Terminal".to_string(),
                path: "/Applications/Terminal.app".to_string(),
                bundle_id: Some("com.apple.Terminal".to_string()),
                icon_path: None,
            },
            LauncherAppEntry {
                name: "Calendar".to_string(),
                path: "/Applications/Calendar.app".to_string(),
                bundle_id: Some("com.apple.iCal".to_string()),
                icon_path: None,
            },
        ];
        let distros = vec![
            WslDistro {
                name: "Ubuntu".to_string(),
                is_default: true,
                is_running: true,
            },
            WslDistro {
                name: "Debian".to_string(),
                is_default: false,
                is_running: false,
            },
        ];

        assert_eq!(filter_app_indices(&apps, "ical"), vec![1]);
        assert_eq!(filter_wsl_distro_indices(&distros, "ubu"), vec![0]);
    }
}
