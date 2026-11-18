use super::ime;
use super::ime::WorkspaceImeTarget;
use super::sftp::native_video::{SharedSftpNativeVideoSurface, sftp_native_video_element};
use super::*;
use oxideterm_gpui_file_manager::{
    FileManagerBindings, FileManagerEditorFonts, FileManagerEnvironment, FileManagerImeFrame,
    FileManagerNoticeKind, FileManagerPointer, FileManagerRequest, FileManagerSelectableText,
    FileManagerVideo, FileManagerViews,
};

pub(super) use oxideterm_gpui_file_manager::{FileManagerDialog, FileManagerInput};
pub(in crate::workspace) use oxideterm_local_files::local_file_audit_context;

pub(super) fn mount(
    store: &oxideterm_settings::SettingsStore,
    tokens: ThemeTokens,
    i18n: &I18n,
    anchors: &ime::TextInputAnchorStore,
    plugins: &Entity<plugin_entity::PluginWorkspaceEntity>,
    runtime: &Arc<tokio::runtime::Runtime>,
    cx: &mut Context<WorkspaceApp>,
) -> FileManagerViews {
    let owner = cx.entity().downgrade();
    let document_plugins = plugins.clone();
    let document_runtime = runtime.clone();
    let anchors = anchors.clone();
    let bindings = FileManagerBindings {
        frame: Rc::new({
            let owner = owner.clone();
            move |cx| {
                owner
                    .upgrade()
                    .map(|owner| owner.read(cx).file_manager_environment(cx))
            }
        }),
        dispatch: Rc::new({
            let owner = owner.clone();
            move |request, cx| {
                let _ = owner.update(cx, |app, cx| app.handle_file_manager_request(request, cx));
            }
        }),
        pointer: Rc::new({
            let owner = owner.clone();
            move |request, window, cx| {
                let _ = owner.update(cx, |app, cx| {
                    app.handle_file_manager_pointer(request, window, cx)
                });
            }
        }),
        anchor: Rc::new(move |anchor| anchors.update(anchor)),
        selectable: Rc::new({
            let owner = owner.clone();
            move |text, cx| {
                owner
                    .update(cx, |app, cx| app.file_manager_selectable(text, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            }
        }),
        document: Rc::new(move |path, mime_type, tokens, i18n, cx| {
            let view = cx.new(|cx| {
                plugin_preview::PluginFilePreview::new(
                    document_plugins.clone(),
                    document_runtime.clone(),
                    tokens,
                    i18n,
                    cx,
                )
            });
            view.update(cx, |view, cx| {
                view.set_source(
                    oxideterm_preview::PreviewAssetOwner::local(
                        path,
                        mime_type,
                        oxideterm_preview::PreviewAssetKind::Document,
                    ),
                    cx,
                );
            });
            Some(view.into())
        }),
        mermaid_zoom: Some(Arc::new(move |source, image, width, height, window, cx| {
            if let Some(workspace) = owner.upgrade() {
                WorkspaceApp::mermaid_zoom_handler_for_workspace(workspace)(
                    source, image, width, height, window, cx,
                );
            }
        })),
    };
    let native_video = SharedSftpNativeVideoSurface::default();
    let video = FileManagerVideo {
        render: Rc::new({
            let surface = native_video.clone();
            move |path, fallback| {
                sftp_native_video_element(path, surface.clone(), fallback).into_any_element()
            }
        }),
        snapshot: Rc::new({
            let surface = native_video.clone();
            move || surface.snapshot()
        }),
        detach: Rc::new(move || native_video.detach()),
    };
    FileManagerViews::new(
        store.path(),
        video,
        bindings,
        file_manager_base_environment(tokens, i18n, store.settings()),
        cx,
    )
}

fn file_manager_base_environment(
    tokens: ThemeTokens,
    i18n: &I18n,
    settings: &PersistedSettings,
) -> FileManagerEnvironment {
    let mono_font_family = settings_mono_font_family(settings);
    FileManagerEnvironment {
        tokens,
        i18n: i18n.clone(),
        background: false,
        modal_background: false,
        chrome_divider: rgb(tokens.ui.border),
        mono_font_family: mono_font_family.clone(),
        mono_font_size: settings.terminal.font_size as f32,
        editor: FileManagerEditorFonts {
            editor_font_family: mono_font_family.to_string(),
            editor_font_weight: settings.terminal.font_weight as f32,
            editor_font_fallback: None,
            editor_font_size: settings.terminal.font_size as f32,
            editor_line_height: settings.terminal.resolved_ide_line_height(),
            word_wrap: settings.ide.word_wrap,
            background_active: false,
        },
        markdown: MarkdownOptions::from_theme(&tokens),
        caret_visible: false,
        ime: Default::default(),
        bookmarks_width: settings.sidebar_ui.file_manager_sidebar_width,
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_file_manager_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_manager
            .body
            .update(cx, |view, cx| view.refresh_file_manager_drives(cx));
        let initial_path = self.active_local_terminal_cwd_path(cx);
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::FileManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::FileManager,
                    title: self.i18n.t("fileManager.title"),
                    title_source: TabTitleSource::I18nKey("fileManager.title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        // Opening a workspace tab is independent from sidebar visibility, so
        // preserve the user's collapsed or expanded state across navigation.
        self.file_manager
            .body
            .update(cx, |view, cx| view.open_location(initial_path, cx));
        self.persist_sidebar_settings(cx);
        self.reveal_active_tab(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_file_manager_tab_at_path(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_manager_tab(window, cx);
        if !path.trim().is_empty() {
            // The cwd picker can browse away from the active terminal's
            // confirmed cwd before opening the surface, so apply that explicit
            // selection after the tab is active.
            self.file_manager
                .body
                .update(cx, |view, cx| view.set_file_manager_path(path, cx));
        }
        cx.notify();
    }

    fn file_manager_environment(&self, cx: &App) -> FileManagerEnvironment {
        let mut frame =
            file_manager_base_environment(self.tokens, &self.i18n, self.settings_store.settings());
        let editor = self.ide_runtime_settings();
        frame.editor = FileManagerEditorFonts {
            editor_font_family: editor.editor_font_family,
            editor_font_weight: editor.editor_font_weight,
            editor_font_fallback: editor.editor_font_fallback,
            editor_font_size: editor.editor_font_size,
            editor_line_height: editor.editor_line_height,
            word_wrap: editor.word_wrap,
            background_active: editor.background_active,
        };
        frame.background = self.background_surface_active("file_manager");
        frame.modal_background = self
            .terminal_background_preferences("file_manager")
            .is_some();
        frame.chrome_divider = self.workspace_chrome_divider();
        frame.markdown = self.localized_markdown_options();
        frame.caret_visible = self.input_caret.visible();
        frame.ime = [
            FileManagerInput::Path,
            FileManagerInput::Filter,
            FileManagerInput::DialogValue,
        ]
        .map(|input| {
            let target = WorkspaceImeTarget::FileManager(input);
            FileManagerImeFrame {
                anchor: self.text_input_anchors.get(target.anchor_id()),
                selection: self.ime_selected_range_for_target(target, cx),
                marked: self.marked_text_for_target(target, cx).map(str::to_owned),
                active_offset: self.ime_active_selection_offset(target),
                viewport: self.text_input_anchors.viewport(target.anchor_id()),
            }
        });
        frame
    }

    fn file_manager_selectable(
        &self,
        text: FileManagerSelectableText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some((group, order, font)) = text.row {
            self.render_row_safe_selectable_display_text_in_group(
                group,
                "local-files",
                text.id,
                order,
                text.text.to_string(),
                text.color,
                font,
                cx,
            )
        } else {
            self.render_selectable_text_scoped("local-files", text.id, text.text, text.color, cx)
        }
    }

    fn handle_file_manager_request(&mut self, request: FileManagerRequest, cx: &mut Context<Self>) {
        match request {
            FileManagerRequest::ClearIme => {
                if self.ime_marked_text.take().is_some() { cx.notify(); }
            }
            FileManagerRequest::ClearImeSelection => {
                if self.clear_ime_selection() {
                    cx.notify();
                }
            }
            FileManagerRequest::WindowInteractionChanged => cx.notify(),
            FileManagerRequest::Notice {
                title,
                description,
                kind,
            } => {
                let variant = match kind {
                    FileManagerNoticeKind::Error => TerminalNoticeVariant::Error,
                    FileManagerNoticeKind::Success => TerminalNoticeVariant::Success,
                    FileManagerNoticeKind::Default => TerminalNoticeVariant::Default,
                };
                self.push_file_manager_toast(title, description, variant, cx);
            }
            FileManagerRequest::Tooltip { id, label, x, y } => {
                self.queue_workspace_tooltip(id, label, x, y, cx)
            }
            FileManagerRequest::ClearTooltip(id) => self.clear_workspace_tooltip(&id, cx),
            FileManagerRequest::SaveBookmarksWidth(width) => {
                let previous = self
                    .settings_store
                    .settings()
                    .sidebar_ui
                    .file_manager_sidebar_width;
                if width == previous {
                    return;
                }
                self.settings_store
                    .settings_mut()
                    .sidebar_ui
                    .file_manager_sidebar_width = width;
                match self.settings_store.save() {
                    Ok(_) => self.settings_workspace.update(cx, |settings, _| {
                        settings.acknowledge_external_store_state()
                    }),
                    Err(error) => {
                        self.settings_store
                            .settings_mut()
                            .sidebar_ui
                            .file_manager_sidebar_width = previous;
                        self.push_file_manager_toast(
                            self.i18n.t("fileManager.error"),
                            Some(error.to_string()),
                            TerminalNoticeVariant::Error,
                            cx,
                        );
                    }
                }
                cx.notify();
            }
        }
    }

    fn handle_file_manager_pointer(
        &mut self,
        request: FileManagerPointer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = || {
            window
                .root::<detached_tab_window::DetachedTabWindow>()
                .flatten()
                .map(|root| root.read(cx).focus_handle(cx))
                .unwrap_or_else(|| self.focus_handle.clone())
        };
        match request {
            FileManagerPointer::Focus => {
                let focus = focus();
                window.focus(&focus, cx);
            }
            FileManagerPointer::TextDown(input, event) => {
                let focus = focus();
                self.ime_marked_text = None;
                window.focus(&focus, cx);
                self.begin_ime_selection_from_mouse_down(
                    WorkspaceImeTarget::FileManager(input),
                    &event,
                    window,
                    cx,
                );
                cx.notify();
            }
            FileManagerPointer::TextMove(event) => {
                self.update_ime_selection_drag_from_mouse_move(&event, window, cx)
            }
            FileManagerPointer::DismissTransient => {
                self.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
            }
            FileManagerPointer::OpenTerminal(path) => {
                let mut config = self.local_terminal_config();
                config.cwd = Some(PathBuf::from(path));
                match self.create_local_terminal_tab_with_config(
                    config,
                    self.local_terminal_tab_title(),
                    window,
                    cx,
                ) {
                    Ok(_) => self.push_file_manager_toast(
                        self.i18n.t("fileManager.terminalOpened"),
                        None,
                        TerminalNoticeVariant::Success,
                        cx,
                    ),
                    Err(error) => self.push_file_manager_toast(
                        self.i18n.t("fileManager.error"),
                        Some(error.to_string()),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
                cx.notify();
            }
        }
    }

    fn push_file_manager_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn render_file_manager_surface(
        &self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> AnyElement {
        self.file_manager.body.clone().into_any_element()
    }

    pub(in crate::workspace) fn render_file_manager_dialog(
        &self,
        _: &mut Window,
        _: bool,
        _: &mut Context<Self>,
    ) -> AnyElement {
        self.file_manager.dialog.clone().into_any_element()
    }

    pub(in crate::workspace) fn handle_file_manager_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key == "escape" && self.dismiss_workspace_context_menus(cx) {
            cx.notify();
            return true;
        }
        let view = self.file_manager.body.clone();
        if view.update(cx, |view, cx| {
            view.handle_file_manager_dialog_input_footer_key(event, cx)
        }) {
            return true;
        }
        let focused = self.file_manager.read(cx).focused_input().is_some();
        if focused && self.handle_active_text_input_edit_shortcut(&event.keystroke, cx) {
            return true;
        }
        let overrides = &self.settings_store.settings().keybindings.overrides;
        let scoped =
            |scope| crate::keybindings::matched_scoped_action(&event.keystroke, scope, overrides);
        let file_action = scoped(crate::keybindings::ActionScope::FileManager);
        let preview_action = scoped(crate::keybindings::ActionScope::Preview);
        view.update(cx, |view, cx| {
            view.handle_file_manager_key(event, file_action, preview_action, cx)
        }) || (focused
            && (self.handle_active_text_input_navigation(&event.keystroke, cx)
                || self.handle_active_text_input_delete_selection(&event.keystroke, cx)
                || self.handle_active_text_input_transpose(&event.keystroke, cx)))
    }

    pub(in crate::workspace) fn close_file_manager_dialog(&mut self, cx: &mut Context<Self>) {
        self.file_manager
            .body
            .update(cx, |view, cx| view.close_file_manager_dialog(cx));
    }

    pub(in crate::workspace) fn dismiss_file_manager_context_menu(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.file_manager
            .body
            .update(cx, |view, cx| view.dismiss_file_manager_context_menu(cx))
    }

    pub(in crate::workspace) fn refresh_file_manager_path_completion(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.file_manager
            .body
            .update(cx, |view, cx| view.refresh_file_manager_path_completion(cx));
    }

    pub(in crate::workspace) fn accept_file_manager_path_completion(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.body.update(cx, |view, cx| {
            view.accept_file_manager_path_completion(index, cx)
        });
    }

    pub(in crate::workspace) fn file_manager_bookmarks_resize_active(
        &self,
        window: gpui::WindowId,
        cx: &App,
    ) -> bool {
        self.file_manager
            .body
            .read(cx)
            .file_manager_bookmarks_resize_active(window, cx)
    }

    pub(in crate::workspace) fn update_file_manager_bookmarks_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.body.update(cx, |view, cx| {
            view.update_file_manager_bookmarks_resize(event, window, cx)
        });
    }

    pub(in crate::workspace) fn cancel_file_manager_bookmarks_resize(
        &mut self,
        window: gpui::WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.file_manager.body.update(cx, |view, cx| {
            view.cancel_file_manager_bookmarks_resize(window, cx)
        })
    }

    pub(in crate::workspace) fn finish_file_manager_bookmarks_resize(
        &mut self,
        window: gpui::WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.file_manager.body.update(cx, |view, cx| {
            view.finish_file_manager_bookmarks_resize(window, cx)
        })
    }
}
