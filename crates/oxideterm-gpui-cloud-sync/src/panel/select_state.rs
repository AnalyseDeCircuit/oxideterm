use super::*;

impl CloudSyncWorkspaceEntity {
    fn select_settings(&self) -> CloudSyncSettings {
        CloudSyncSettings {
            backend_type: self.view.form.backend_type.clone(),
            auth_mode: self.view.form.auth_mode.clone(),
            default_conflict_strategy: self.view.form.default_conflict_strategy.clone(),
            ..CloudSyncSettings::default()
        }
    }

    pub(super) fn select_options(
        &self,
        i18n: &I18n,
        select: CloudSyncSelect,
    ) -> Vec<CloudSyncSelectOption> {
        let mut options: Vec<_> = cloud_sync_select_options(&self.select_settings(), select)
            .into_iter()
            .map(|option| CloudSyncSelectOption {
                label: i18n.t(cloud_sync_select_label_key(option.label_key)),
                selected: option.selected
                    && !(select == CloudSyncSelect::Backend && self.view.local_file_mode),
                action: option.action,
            })
            .collect();
        if select == CloudSyncSelect::Backend {
            options.push(CloudSyncSelectOption {
                label: i18n.t("plugin.cloud_sync.backend.local_file"),
                selected: self.view.local_file_mode,
                action: CloudSyncSelectAction::LocalFile,
            });
        }
        options
    }

    pub fn handle_select_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let settings = self.select_settings();
        let focusable = if self.view.active_tab != CloudSyncTab::Configure {
            vec![]
        } else if self.view.local_file_mode {
            vec![CloudSyncSelect::Backend, CloudSyncSelect::ConflictStrategy]
        } else {
            cloud_sync_focusable_selects(&settings)
        };
        let effect = crate::handle_cloud_sync_select_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            CloudSyncSelectKeyState {
                open_select: self.view.open_select,
                focused_select: self.view.focused_select,
                highlighted_option: self.view.select_highlighted,
            },
            &focusable,
            |select| {
                if select == CloudSyncSelect::Backend && self.view.local_file_mode {
                    cloud_sync_select_options(&settings, select).len()
                } else {
                    cloud_sync_selected_option_index(&settings, select)
                }
            },
            |select| self.select_options(i18n, select).len(),
        );
        let CloudSyncSelectKeyEffect::Handled {
            state,
            keyboard_focus_origin,
            selected_action_index,
        } = effect
        else {
            return false;
        };
        let previous_open_select = self.view.open_select;
        self.view.open_select = state.open_select;
        self.view.focused_select = state.focused_select;
        self.view.select_highlighted = state.highlighted_option;
        if keyboard_focus_origin {
            self.view.select_focus_origin = Some(BrowserFocusOrigin::Keyboard);
        }
        if previous_open_select.is_some()
            && state.open_select.is_none()
            && event.keystroke.key.as_str() == "escape"
        {
            self.close_select();
        }
        if let (Some(select), Some(index)) = (self.view.focused_select, selected_action_index) {
            if let Some(option) = self.select_options(i18n, select).get(index) {
                self.apply_select_action(option.action.clone(), cx);
            }
        }
        cx.notify();
        true
    }
}
