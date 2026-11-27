use std::{cell::RefCell, collections::HashMap, rc::Rc};

use oxideterm_gpui_markdown::MarkdownVirtualListScrollHandle;

use gpui::{App, AppContext, Entity, IntoElement, WeakEntity, Window};

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
    pub acp_probe_status: crate::ai::acp::AcpProbeStatus,
    pub ai_mcp: crate::ai::mcp::AiMcpBindings,
    pub ai_providers: crate::ai::providers::AiProviderBindings,
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
    pub acp_settings: Entity<crate::ai::acp::AcpSettingsView>,
    acp_plugin_views: RefCell<HashMap<String, WeakEntity<crate::ai::acp::AcpSettingsView>>>,
    pub ai_mcp_dialog: Entity<crate::ai::mcp_dialog::AiMcpDialogView>,
    pub ai_mcp: Entity<crate::ai::mcp::AiMcpSettingsView>,
    pub ai_providers: Entity<crate::ai::providers::AiProviderSettingsView>,
    pub ai_confirm: Entity<crate::ai::confirm::AiSettingsConfirm>,
    pub ai_text_editor: Entity<crate::ai::text_editor::AiTextEditor>,
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
    pub fn acp_plugin_settings(
        &self,
        plugin_id: &str,
        cx: &mut App,
    ) -> Entity<crate::ai::acp::AcpSettingsView> {
        let mut views = self.acp_plugin_views.borrow_mut();
        views.retain(|_, view| view.upgrade().is_some());
        if let Some(view) = views.get(plugin_id).and_then(WeakEntity::upgrade) {
            return view;
        }
        let build = self
            .acp_settings
            .read(cx)
            .plugin_builder(plugin_id.to_owned());
        let view = cx.new(build);
        views.insert(plugin_id.to_owned(), view.downgrade());
        view
    }

    pub fn notify_acp(&self, cx: &mut App) {
        cx.notify(self.acp_settings.entity_id());
        for view in self
            .acp_plugin_views
            .borrow()
            .values()
            .filter_map(WeakEntity::upgrade)
        {
            cx.notify(view.entity_id());
        }
    }

    /// Returns false when the input belongs to a different workspace surface.
    pub fn notify_input(&self, input: SettingsInput, cx: &mut App) -> bool {
        if input.is_ai_mcp() {
            self.ai_mcp_dialog.update(cx, |_, cx| cx.notify());
        } else if matches!(
            input,
            SettingsInput::AiProviderApiKey(_)
                | SettingsInput::AiProviderName(_)
                | SettingsInput::AiProviderBaseUrl(_)
                | SettingsInput::AiProviderNewModel(_)
        ) {
            self.ai_providers.update(cx, |_, cx| cx.notify());
        } else if crate::terminal_triggers::is_terminal_trigger_input(input) {
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
        let acp_controls = bindings.ai_providers.controls.clone();
        let ai_providers = cx.new(|cx| {
            cx.observe(settings, |_, _, cx| cx.notify()).detach();
            crate::ai::providers::AiProviderSettingsView::new(bindings.ai_providers)
        });
        let surface = cx.new(|cx| {
            cx.subscribe(
                &ai_providers,
                |_, _, _: &crate::ai::providers::ProviderLayoutChanged, cx| cx.notify(),
            )
            .detach();
            SettingsSurfaceView::new(settings.clone(), bindings.surface, cx)
        });
        Self {
            ai_confirm: cx.new(|_| {
                crate::ai::confirm::AiSettingsConfirm::new(
                    oxideterm_theme::default_tokens(),
                    oxideterm_i18n::I18n::default(),
                )
            }),
            ai_providers,
            ai_text_editor: cx.new(|_| crate::ai::text_editor::AiTextEditor::default()),
            legal_notice_scroll: MarkdownVirtualListScrollHandle::new(),
            acp_settings: cx.new(|cx| {
                crate::ai::acp::AcpSettingsView::new(
                    settings.clone(),
                    acp_controls,
                    bindings.acp_probe_status,
                    None,
                    cx,
                )
            }),
            acp_plugin_views: Default::default(),
            ai_mcp_dialog: cx.new(|cx| {
                crate::ai::mcp_dialog::AiMcpDialogView::new(
                    settings.clone(),
                    bindings.ai_mcp.controls.clone(),
                    bindings.ai_mcp.request.clone(),
                    cx,
                )
            }),
            ai_mcp: cx.new(|_| crate::ai::mcp::AiMcpSettingsView::new(bindings.ai_mcp)),
            release_notes_scroll: MarkdownVirtualListScrollHandle::new(),
            native_update_notice: cx
                .new(|cx| NativeUpdateNoticeView::new(settings, bindings.native_update_notice, cx)),
            window_chrome: cx.new(|cx| SettingsWindowChromeEntity::new(settings, cx)),
            surface,
            terminal_trigger_editor: cx.new(|cx| {
                TerminalTriggerEditor::new(settings, dialog(SettingsDialog::Trigger), cx)
            }),
            highlight_editor: cx
                .new(|cx| HighlightEditor::new(settings, dialog(SettingsDialog::Highlight), cx)),
            theme_editor_dialog: cx
                .new(|cx| ThemeEditorDialog::new(settings, bindings.theme_editor, background, cx)),
            ssh_import_dialog_view: cx.new(|cx| {
                ConnectionDialog::new(
                    settings,
                    SettingsWorkspaceEntity::ssh_config_import_dialog_open,
                    dialog(SettingsDialog::SshImport),
                    cx,
                )
            }),
            managed_key_dialog_view: cx.new(|cx| {
                ConnectionDialog::new(
                    settings,
                    SettingsWorkspaceEntity::managed_key_dialog_open,
                    dialog(SettingsDialog::ManagedKeys),
                    cx,
                )
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
