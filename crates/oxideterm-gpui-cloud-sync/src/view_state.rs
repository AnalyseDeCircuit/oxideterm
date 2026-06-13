// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync panel view state that is independent from WorkspaceApp.

use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, ConflictStrategy, secrets::backend_uses_auth_mode,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CloudSyncTab {
    Overview,
    Configure,
    History,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CloudSyncSection {
    Header,
    TabBar,
    Guide,
    Status,
    Actions,
    Preview,
    Rollback,
    History,
    RecentHistory,
    ConfigConnection,
    ConfigScope,
    ConfigCoverage,
    ConfigPreflight,
    ConfigHealth,
    ConfigNotes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncSelect {
    Backend,
    AuthMode,
    ConflictStrategy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncSelectAction {
    Backend(BackendType),
    AuthMode(AuthMode),
    ConflictStrategy(ConflictStrategy),
}

#[derive(Clone, Debug)]
pub struct CloudSyncSelectOption {
    pub label: String,
    pub selected: bool,
    pub action: CloudSyncSelectAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncSelectLabelKey {
    BackendWebdav,
    BackendHttpJson,
    BackendDropbox,
    BackendOneDrive,
    BackendGoogleDrive,
    BackendGithubGist,
    BackendGit,
    BackendS3,
    AuthBearer,
    AuthBasic,
    AuthNone,
    ConflictMerge,
    ConflictReplace,
    ConflictSkip,
    ConflictRename,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncSelectOptionSpec {
    pub label_key: CloudSyncSelectLabelKey,
    pub selected: bool,
    pub action: CloudSyncSelectAction,
}

pub fn cloud_sync_select_options(
    settings: &CloudSyncSettings,
    select: CloudSyncSelect,
) -> Vec<CloudSyncSelectOptionSpec> {
    match select {
        CloudSyncSelect::Backend => [
            (BackendType::Webdav, CloudSyncSelectLabelKey::BackendWebdav),
            (
                BackendType::HttpJson,
                CloudSyncSelectLabelKey::BackendHttpJson,
            ),
            (
                BackendType::Dropbox,
                CloudSyncSelectLabelKey::BackendDropbox,
            ),
            (
                BackendType::OneDrive,
                CloudSyncSelectLabelKey::BackendOneDrive,
            ),
            (
                BackendType::GoogleDrive,
                CloudSyncSelectLabelKey::BackendGoogleDrive,
            ),
            (
                BackendType::GithubGist,
                CloudSyncSelectLabelKey::BackendGithubGist,
            ),
            (BackendType::Git, CloudSyncSelectLabelKey::BackendGit),
            (BackendType::S3, CloudSyncSelectLabelKey::BackendS3),
        ]
        .into_iter()
        .map(|(backend, label_key)| CloudSyncSelectOptionSpec {
            label_key,
            selected: settings.backend_type == backend,
            action: CloudSyncSelectAction::Backend(backend),
        })
        .collect(),
        CloudSyncSelect::AuthMode => [
            (AuthMode::Bearer, CloudSyncSelectLabelKey::AuthBearer),
            (AuthMode::Basic, CloudSyncSelectLabelKey::AuthBasic),
            (AuthMode::None, CloudSyncSelectLabelKey::AuthNone),
        ]
        .into_iter()
        .map(|(auth_mode, label_key)| CloudSyncSelectOptionSpec {
            label_key,
            selected: settings.auth_mode == auth_mode,
            action: CloudSyncSelectAction::AuthMode(auth_mode),
        })
        .collect(),
        CloudSyncSelect::ConflictStrategy => [
            (
                ConflictStrategy::Merge,
                CloudSyncSelectLabelKey::ConflictMerge,
            ),
            (
                ConflictStrategy::Replace,
                CloudSyncSelectLabelKey::ConflictReplace,
            ),
            (
                ConflictStrategy::Skip,
                CloudSyncSelectLabelKey::ConflictSkip,
            ),
            (
                ConflictStrategy::Rename,
                CloudSyncSelectLabelKey::ConflictRename,
            ),
        ]
        .into_iter()
        .map(|(strategy, label_key)| CloudSyncSelectOptionSpec {
            label_key,
            selected: settings.default_conflict_strategy == strategy,
            action: CloudSyncSelectAction::ConflictStrategy(strategy),
        })
        .collect(),
    }
}

pub fn cloud_sync_selected_option_index(
    settings: &CloudSyncSettings,
    select: CloudSyncSelect,
) -> usize {
    cloud_sync_select_options(settings, select)
        .iter()
        .position(|option| option.selected)
        .unwrap_or(0)
}

pub fn cloud_sync_focusable_selects(settings: &CloudSyncSettings) -> Vec<CloudSyncSelect> {
    let mut selects = vec![CloudSyncSelect::Backend];
    if backend_uses_auth_mode(&settings.backend_type) {
        selects.push(CloudSyncSelect::AuthMode);
    }
    selects.push(CloudSyncSelect::ConflictStrategy);
    selects
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncSelectKeyState {
    pub open_select: Option<CloudSyncSelect>,
    pub focused_select: Option<CloudSyncSelect>,
    pub highlighted_option: Option<(CloudSyncSelect, usize)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncSelectKeyEffect {
    Ignored,
    Handled {
        state: CloudSyncSelectKeyState,
        keyboard_focus_origin: bool,
        selected_action_index: Option<usize>,
    },
}

pub fn handle_cloud_sync_select_key(
    key: &str,
    shift: bool,
    state: CloudSyncSelectKeyState,
    focusable_selects: &[CloudSyncSelect],
    selected_index: impl Fn(CloudSyncSelect) -> usize,
    option_count: impl Fn(CloudSyncSelect) -> usize,
) -> CloudSyncSelectKeyEffect {
    if let Some(select) = state.open_select {
        return handle_open_cloud_sync_select_key(
            key,
            shift,
            select,
            state,
            focusable_selects,
            selected_index,
            option_count,
        );
    }
    let Some(select) = state.focused_select else {
        return CloudSyncSelectKeyEffect::Ignored;
    };
    match key {
        "escape" => handled_key(
            CloudSyncSelectKeyState {
                focused_select: None,
                ..state
            },
            false,
            None,
        ),
        "tab" => handled_key(
            CloudSyncSelectKeyState {
                focused_select: next_cloud_sync_select_focus(focusable_selects, select, !shift),
                ..state
            },
            true,
            None,
        ),
        "enter" | "space" | " " | "arrowdown" | "down" => handled_key(
            CloudSyncSelectKeyState {
                open_select: Some(select),
                focused_select: Some(select),
                highlighted_option: Some((select, selected_index(select))),
            },
            true,
            None,
        ),
        _ => CloudSyncSelectKeyEffect::Ignored,
    }
}

fn handle_open_cloud_sync_select_key(
    key: &str,
    shift: bool,
    select: CloudSyncSelect,
    state: CloudSyncSelectKeyState,
    focusable_selects: &[CloudSyncSelect],
    selected_index: impl Fn(CloudSyncSelect) -> usize,
    option_count: impl Fn(CloudSyncSelect) -> usize,
) -> CloudSyncSelectKeyEffect {
    let options_len = option_count(select);
    if options_len == 0 {
        return CloudSyncSelectKeyEffect::Ignored;
    }
    let current = state
        .highlighted_option
        .filter(|(highlighted_select, _)| *highlighted_select == select)
        .map(|(_, index)| index)
        .unwrap_or_else(|| selected_index(select));
    match key {
        "escape" => handled_key(
            CloudSyncSelectKeyState {
                open_select: None,
                focused_select: Some(select),
                highlighted_option: None,
            },
            true,
            None,
        ),
        "tab" => handled_key(
            CloudSyncSelectKeyState {
                open_select: None,
                focused_select: next_cloud_sync_select_focus(focusable_selects, select, !shift),
                highlighted_option: None,
            },
            true,
            None,
        ),
        "arrowdown" | "down" => handled_key(
            CloudSyncSelectKeyState {
                highlighted_option: Some((select, next_select_index(current, options_len, true))),
                ..state
            },
            false,
            None,
        ),
        "arrowup" | "up" => handled_key(
            CloudSyncSelectKeyState {
                highlighted_option: Some((select, next_select_index(current, options_len, false))),
                ..state
            },
            false,
            None,
        ),
        "home" => handled_key(
            CloudSyncSelectKeyState {
                highlighted_option: Some((select, 0)),
                ..state
            },
            false,
            None,
        ),
        "end" => handled_key(
            CloudSyncSelectKeyState {
                highlighted_option: Some((select, options_len - 1)),
                ..state
            },
            false,
            None,
        ),
        "enter" | "space" | " " => handled_key(state, true, Some(current.min(options_len - 1))),
        _ => CloudSyncSelectKeyEffect::Ignored,
    }
}

fn handled_key(
    state: CloudSyncSelectKeyState,
    keyboard_focus_origin: bool,
    selected_action_index: Option<usize>,
) -> CloudSyncSelectKeyEffect {
    CloudSyncSelectKeyEffect::Handled {
        state,
        keyboard_focus_origin,
        selected_action_index,
    }
}

fn next_select_index(current: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        (current + 1).min(len - 1)
    } else {
        current.saturating_sub(1)
    }
}

pub fn next_cloud_sync_select_focus(
    selects: &[CloudSyncSelect],
    current: CloudSyncSelect,
    forward: bool,
) -> Option<CloudSyncSelect> {
    let index = selects.iter().position(|candidate| *candidate == current)?;
    if forward {
        selects.get(index + 1).copied()
    } else {
        index
            .checked_sub(1)
            .and_then(|previous| selects.get(previous).copied())
    }
}

pub fn close_cloud_sync_select_on_container_scroll(
    open_select: &mut Option<CloudSyncSelect>,
    focused_select: &mut Option<CloudSyncSelect>,
    highlighted_option: &mut Option<(CloudSyncSelect, usize)>,
) -> bool {
    let Some(closing_select) = open_select.take() else {
        return false;
    };
    *focused_select = Some(closing_select);
    *highlighted_option = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_sync_select_focus_tabs_only_through_visible_controls() {
        let webdav_selects = [
            CloudSyncSelect::Backend,
            CloudSyncSelect::AuthMode,
            CloudSyncSelect::ConflictStrategy,
        ];
        let hidden_auth_selects = [CloudSyncSelect::Backend, CloudSyncSelect::ConflictStrategy];

        assert_eq!(
            next_cloud_sync_select_focus(&webdav_selects, CloudSyncSelect::Backend, true),
            Some(CloudSyncSelect::AuthMode)
        );
        assert_eq!(
            next_cloud_sync_select_focus(&hidden_auth_selects, CloudSyncSelect::Backend, true),
            Some(CloudSyncSelect::ConflictStrategy)
        );
        assert_eq!(
            next_cloud_sync_select_focus(
                &hidden_auth_selects,
                CloudSyncSelect::ConflictStrategy,
                true
            ),
            None
        );
        assert_eq!(
            next_cloud_sync_select_focus(&webdav_selects, CloudSyncSelect::AuthMode, false),
            Some(CloudSyncSelect::Backend)
        );
    }

    #[test]
    fn cloud_sync_select_scroll_close_preserves_trigger_focus() {
        let mut open_select = Some(CloudSyncSelect::ConflictStrategy);
        let mut focused_select = Some(CloudSyncSelect::Backend);
        let mut highlighted_option = Some((CloudSyncSelect::ConflictStrategy, 1));

        assert!(close_cloud_sync_select_on_container_scroll(
            &mut open_select,
            &mut focused_select,
            &mut highlighted_option,
        ));
        assert_eq!(open_select, None);
        assert_eq!(focused_select, Some(CloudSyncSelect::ConflictStrategy));
        assert_eq!(highlighted_option, None);

        assert!(!close_cloud_sync_select_on_container_scroll(
            &mut open_select,
            &mut focused_select,
            &mut highlighted_option,
        ));
        assert_eq!(focused_select, Some(CloudSyncSelect::ConflictStrategy));
    }
}
