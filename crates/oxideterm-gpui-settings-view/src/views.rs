use std::rc::Rc;

use oxideterm_gpui_markdown::MarkdownVirtualListScrollHandle;

use gpui::{App, AppContext, Entity, IntoElement, Window};

use crate::{
    SettingsInput, SettingsSelect,
    command_specs::{CommandSpecsBindings, CommandSpecsDialog},
    connections_page::ConnectionDialog,
    entity::SettingsWorkspaceEntity,
    highlight_page::HighlightEditor,
    mcp_page::McpClientDialog,
    navigation_editor::{SettingsNavigationEditor, SettingsNavigationEditorFrame},
    portable_runtime::dialog::{PortablePasswordDialog, PortablePasswordFrame},
    surface::{SettingsSurfaceBindings, SettingsSurfaceView, pages::SettingsDialog},
    terminal_triggers::TerminalTriggerEditor,
    theme_editor::{ThemeEditorBindings, ThemeEditorDialog},
    theme_picker::{SettingsThemePicker, SettingsThemePickerFrame},
    update_view::{NativeUpdateNoticeBindings, NativeUpdateNoticeView},
    window_chrome::SettingsWindowChromeEntity,
};

type FinishDialog = Rc<dyn Fn(bool, &mut App)>;

pub struct SettingsViewBindings {
    pub surface: SettingsSurfaceBindings,
    pub native_update_notice: NativeUpdateNoticeBindings,
    pub theme_editor: ThemeEditorBindings,
    pub command_specs: CommandSpecsBindings,
    pub navigation_frame: Rc<dyn Fn(&App) -> Option<SettingsNavigationEditorFrame>>,
    pub finish_navigation: FinishDialog,
    pub portable_frame: Rc<dyn Fn(&App) -> Option<PortablePasswordFrame>>,
    pub portable_controls: crate::components::SettingsControlsSource,
    pub finish_portable: FinishDialog,
    pub theme_picker_frame: Rc<dyn Fn(&Window, &App) -> Option<SettingsThemePickerFrame>>,
    pub apply_theme: Rc<dyn Fn(SettingsSelect, String, &mut App)>,
}

/// Owns the mounted settings surface and its window-level views.
pub struct SettingsViews {
    pub window_chrome: Entity<SettingsWindowChromeEntity>,
    pub surface: Entity<SettingsSurfaceView>,
    pub legal_notice_scroll: MarkdownVirtualListScrollHandle,
    pub release_notes_scroll: MarkdownVirtualListScrollHandle,
    pub native_update_notice: Entity<NativeUpdateNoticeView>,
    pub terminal_trigger_editor: Entity<TerminalTriggerEditor>,
    pub highlight_editor: Entity<HighlightEditor>,
    pub theme_editor_dialog: Entity<ThemeEditorDialog>,
    pub ssh_import_dialog_view: Entity<ConnectionDialog>,
    pub managed_key_dialog_view: Entity<ConnectionDialog>,
    pub mcp_client_dialog: Entity<McpClientDialog>,
    pub command_specs_dialog: Entity<CommandSpecsDialog>,
    pub navigation_editor: Entity<SettingsNavigationEditor>,
    pub portable_password_dialog: Entity<PortablePasswordDialog>,
    pub theme_picker: Entity<SettingsThemePicker>,
}

impl SettingsViews {
    /// Returns false when the input belongs to a different workspace surface.
    pub fn notify_input(&self, input: SettingsInput, cx: &mut App) -> bool {
        if crate::terminal_triggers::is_terminal_trigger_input(input) {
            self.terminal_trigger_editor.update(cx, |_, cx| cx.notify());
        } else if crate::highlight_page::is_highlight_input(input) {
            self.highlight_editor.update(cx, |_, cx| cx.notify());
            self.surface.update(cx, |_, cx| cx.notify());
        } else if crate::theme_editor::is_theme_editor_input(input) {
            self.theme_editor_dialog.update(cx, |_, cx| cx.notify());
        } else if crate::components::is_connection_key_input(input) {
            self.managed_key_dialog_view.update(cx, |_, cx| cx.notify());
        } else if input == SettingsInput::TerminalCommandSpecsJson {
            self.command_specs_dialog.update(cx, |_, cx| cx.notify());
        } else if crate::components::is_surface_input(input) {
            self.surface.update(cx, |_, cx| cx.notify());
        } else if matches!(
            input,
            SettingsInput::PortableCurrentPassword
                | SettingsInput::PortableNewPassword
                | SettingsInput::PortableConfirmPassword
        ) {
            self.portable_password_dialog
                .update(cx, |_, cx| cx.notify());
        } else {
            return false;
        }
        true
    }

    pub fn new(
        settings: &Entity<SettingsWorkspaceEntity>,
        bindings: SettingsViewBindings,
        cx: &mut App,
    ) -> Self {
        let page = bindings.surface.page.clone();
        let background = Rc::new({
            let page = page.clone();
            let prepare = bindings.surface.prepare_page.clone();
            let state = settings.downgrade();
            move |cx: &mut App| {
                let Some(state) = state.upgrade() else {
                    return gpui::div().into_any_element();
                };
                let section = oxideterm_settings_model::SettingsSectionId::AppearanceBackground;
                let prepared = prepare(section, cx);
                page(section, prepared, cx)
                    .map(|frame| frame.render(&state, section, cx))
                    .unwrap_or_else(|| gpui::div().into_any_element())
            }
        });
        let dialog = |dialog: SettingsDialog| {
            let page = page.clone();
            let settings = settings.downgrade();
            Rc::new(move |cx: &App| {
                let state = settings.upgrade()?;
                page(dialog.section(), None, cx)?.render_dialog(&state, dialog, cx)
            })
        };
        Self {
            legal_notice_scroll: MarkdownVirtualListScrollHandle::new(),
            release_notes_scroll: MarkdownVirtualListScrollHandle::new(),
            native_update_notice: cx
                .new(|cx| NativeUpdateNoticeView::new(settings, bindings.native_update_notice, cx)),
            window_chrome: cx.new(|cx| SettingsWindowChromeEntity::new(settings, cx)),
            surface: cx.new(|cx| SettingsSurfaceView::new(settings.clone(), bindings.surface, cx)),
            terminal_trigger_editor: cx.new(|cx| {
                TerminalTriggerEditor::new(settings, dialog(SettingsDialog::Trigger), cx)
            }),
            highlight_editor: cx
                .new(|cx| HighlightEditor::new(settings, dialog(SettingsDialog::Highlight), cx)),
            theme_editor_dialog: cx
                .new(|cx| ThemeEditorDialog::new(settings, bindings.theme_editor, background, cx)),
            ssh_import_dialog_view: cx.new(|cx| {
                ConnectionDialog::new(settings, false, dialog(SettingsDialog::SshImport), cx)
            }),
            managed_key_dialog_view: cx.new(|cx| {
                ConnectionDialog::new(settings, true, dialog(SettingsDialog::ManagedKeys), cx)
            }),
            mcp_client_dialog: cx
                .new(|cx| McpClientDialog::new(settings, dialog(SettingsDialog::McpClient), cx)),
            command_specs_dialog: cx
                .new(|cx| CommandSpecsDialog::new(settings, bindings.command_specs, cx)),
            navigation_editor: cx.new(|_| {
                SettingsNavigationEditor::new(
                    settings.clone(),
                    bindings.navigation_frame,
                    bindings.finish_navigation,
                )
            }),
            portable_password_dialog: cx.new(|cx| {
                PortablePasswordDialog::new(
                    settings.clone(),
                    bindings.portable_frame,
                    bindings.portable_controls,
                    bindings.finish_portable,
                    cx,
                )
            }),
            theme_picker: cx.new(|_| {
                SettingsThemePicker::new(
                    settings.downgrade(),
                    bindings.theme_picker_frame,
                    bindings.apply_theme,
                )
            }),
        }
    }
}
