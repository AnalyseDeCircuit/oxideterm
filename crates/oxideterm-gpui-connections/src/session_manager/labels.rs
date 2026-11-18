// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use chrono::{DateTime, Datelike, Local, Utc};
use gpui::Rgba;
use oxideterm_connections::AuthType;
use oxideterm_gpui_ui::surface::{color_for_background, color_for_background_or_alpha};
use oxideterm_i18n::I18n;
use std::collections::HashSet;

pub fn auth_label(auth_type: AuthType) -> String {
    match auth_type {
        AuthType::Password => "Password",
        AuthType::Key => "Key",
        AuthType::ManagedKey => "Managed Key",
        AuthType::Certificate => "Certificate",
        AuthType::KeyboardInteractive => "Keyboard Interactive",
        AuthType::Agent => "Agent",
    }
    .to_string()
}

pub fn add_group_path_segments(group: &str, paths: &mut HashSet<String>) {
    if group.trim().is_empty() {
        return;
    }
    let parts = group
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for index in 1..=parts.len() {
        paths.insert(parts[..index].join("/"));
    }
}

pub fn expand_group_path(group: &str, expanded_groups: &mut HashSet<String>) {
    let parts = group
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        return;
    }
    for index in 1..parts.len() {
        expanded_groups.insert(parts[..index].join("/"));
    }
}

/// Splits a canonical group path into its parent path and editable leaf name.
pub fn split_session_group_path(group: &str) -> (Option<&str>, &str) {
    group
        .rsplit_once('/')
        .map_or((None, group), |(parent, leaf)| (Some(parent), leaf))
}

/// Builds a canonical path while keeping contextual editors leaf-name only.
pub fn session_group_path_from_leaf(parent_path: Option<&str>, leaf_name: &str) -> Option<String> {
    let leaf_name = leaf_name.trim();
    if leaf_name.is_empty() || leaf_name.contains('/') {
        return None;
    }
    Some(match parent_path.filter(|parent| !parent.is_empty()) {
        Some(parent) => format!("{parent}/{leaf_name}"),
        None => leaf_name.to_string(),
    })
}

/// Returns whether a path is the selected group or one of its descendants.
pub fn session_group_path_is_within(candidate: &str, group: &str) -> bool {
    candidate == group
        || candidate
            .strip_prefix(group)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

/// Maps a row-menu target onto its batch-selection counterpart so a row menu
/// can act on the whole active selection. Group and SSH-config rows are not
/// selectable, so they never join multi-row operations.
pub fn session_manager_row_menu_selection_target(
    target: &SessionManagerRowActionTarget,
) -> Option<SessionManagerSelectionTarget> {
    match target {
        SessionManagerRowActionTarget::Tunnel(id) => {
            Some(SessionManagerSelectionTarget::Tunnel(id.clone()))
        }
        SessionManagerRowActionTarget::Plugin(id) => {
            Some(SessionManagerSelectionTarget::Plugin(id.clone()))
        }
        SessionManagerRowActionTarget::Connection(id) => {
            Some(SessionManagerSelectionTarget::Connection(id.clone()))
        }
        SessionManagerRowActionTarget::Serial(id) => {
            Some(SessionManagerSelectionTarget::Serial(id.clone()))
        }
        SessionManagerRowActionTarget::Ble(id) => {
            Some(SessionManagerSelectionTarget::Ble(id.clone()))
        }
        SessionManagerRowActionTarget::LocalTerminal(id) => {
            Some(SessionManagerSelectionTarget::LocalTerminal(id.clone()))
        }
        SessionManagerRowActionTarget::Telnet(id) => {
            Some(SessionManagerSelectionTarget::Telnet(id.clone()))
        }
        SessionManagerRowActionTarget::Ftp(id) => {
            Some(SessionManagerSelectionTarget::Ftp(id.clone()))
        }
        SessionManagerRowActionTarget::Mosh(id) => {
            Some(SessionManagerSelectionTarget::Mosh(id.clone()))
        }
        SessionManagerRowActionTarget::StandaloneSftp(id) => {
            Some(SessionManagerSelectionTarget::StandaloneSftp(id.clone()))
        }
        SessionManagerRowActionTarget::RemoteDesktop(id) => {
            Some(SessionManagerSelectionTarget::RemoteDesktop(id.clone()))
        }
        SessionManagerRowActionTarget::SshConfig(_)
        | SessionManagerRowActionTarget::GroupRoot
        | SessionManagerRowActionTarget::Group(_) => None,
    }
}

/// Rewrites UI group state after a persisted subtree rename.
pub fn renamed_session_group_path(
    candidate: &str,
    old_group: &str,
    new_group: &str,
) -> Option<String> {
    session_group_path_is_within(candidate, old_group).then(|| {
        let suffix = &candidate[old_group.len()..];
        format!("{new_group}{suffix}")
    })
}

pub fn format_last_used(last_used: Option<&str>, i18n: &I18n) -> String {
    let Some(last_used) = last_used else {
        return i18n.t("sessionManager.table.never_used");
    };
    let Ok(date) = DateTime::parse_from_rfc3339(last_used) else {
        return last_used.to_string();
    };
    let date = date.with_timezone(&Utc);
    let now = Utc::now();
    let diff = now.signed_duration_since(date);
    let diff_mins = diff.num_minutes();
    let diff_hours = diff.num_hours();
    let diff_days = diff.num_days();

    if diff_mins < 1 {
        return i18n.t("sessionManager.time.just_now");
    }
    if diff_mins < 60 {
        return i18n
            .t("sessionManager.time.minutes_ago")
            .replace("{{count}}", &diff_mins.to_string());
    }
    if diff_hours < 24 {
        return i18n
            .t("sessionManager.time.hours_ago")
            .replace("{{count}}", &diff_hours.to_string());
    }
    if diff_days < 7 {
        return i18n
            .t("sessionManager.time.days_ago")
            .replace("{{count}}", &diff_days.to_string());
    }

    let local = date.with_timezone(&Local);
    format!("{}/{}/{}", local.year(), local.month(), local.day())
}

pub fn theme_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_THEME_ALPHA)
}

pub fn theme_secondary_bg(color: u32, has_background: bool) -> Rgba {
    theme_bg(color, has_background)
}

pub fn theme_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_HOVER_ALPHA)
}

pub fn theme_row_hover_bg(color: u32, has_background: bool) -> Rgba {
    // Full-width rows need a lower-contrast hover than compact buttons and menus.
    color_for_background_or_alpha(
        color,
        has_background,
        BG_ACTIVE_ROW_HOVER_ALPHA,
        ROW_HOVER_ALPHA,
    )
}

pub fn theme_input_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background_or_alpha(color, has_background, BG_ACTIVE_THEME_ALPHA / 2, 0x80)
}

pub fn theme_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_BORDER_ALPHA)
}

pub fn theme_border_half(color: u32, has_background: bool) -> Rgba {
    color_for_background_or_alpha(color, has_background, BG_ACTIVE_BORDER_HALF_ALPHA, 0x80)
}

pub fn parse_hex_color(value: &str) -> Option<u32> {
    let hex = value.trim().strip_prefix('#')?;
    let expanded;
    let hex = match hex.len() {
        3 => {
            expanded = hex.chars().flat_map(|ch| [ch, ch]).collect::<String>();
            expanded.as_str()
        }
        6 | 8 => hex,
        _ => return None,
    };
    u32::from_str_radix(&hex[..6], 16).ok()
}

pub fn group_label(i18n: &I18n, group: Option<&str>) -> String {
    group
        .filter(|group| !group.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| i18n.t("sessionManager.folder_tree.ungrouped"))
}

pub fn selected_count_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.table.selected_count")
        .replace("{{count}}", &count.to_string())
}

pub fn confirm_delete_connection_label(i18n: &I18n, name: &str) -> String {
    i18n.t("sessionManager.actions.confirm_delete")
        .replace("{{name}}", name)
}

pub fn confirm_batch_delete_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.actions.confirm_batch_delete")
        .replace("{{count}}", &count.to_string())
}

pub fn connections_deleted_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.toast.connections_deleted")
        .replace("{{count}}", &count.to_string())
}

pub fn connections_moved_label(i18n: &I18n, count: usize, group: String) -> String {
    i18n.t("sessionManager.toast.connections_moved")
        .replace("{{count}}", &count.to_string())
        .replace("{{group}}", &group)
}
