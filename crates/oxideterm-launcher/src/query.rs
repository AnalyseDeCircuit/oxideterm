// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::LauncherAppEntry;

pub fn count_label(filtered: usize, total: usize) -> String {
    if filtered != total {
        format!("{filtered}/{total}")
    } else {
        total.to_string()
    }
}

pub fn filter_apps(apps: &[LauncherAppEntry], query: &str) -> Vec<LauncherAppEntry> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return apps.to_vec();
    }
    apps.iter()
        .filter(|app| {
            app.name.to_ascii_lowercase().contains(&query)
                || app
                    .bundle_id
                    .as_ref()
                    .is_some_and(|bundle_id| bundle_id.to_ascii_lowercase().contains(&query))
        })
        .cloned()
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
}
