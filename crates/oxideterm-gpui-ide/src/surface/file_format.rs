use oxideterm_ide_core::{LineEnding, TextFileFormat};

#[derive(Clone, Copy)]
enum FileFormatAction {
    Encoding,
    Reopen,
    SaveEncoding,
    LineEnding,
}

#[derive(Clone, Copy)]
struct FileFormatMenu {
    tab_id: EditorTabId,
    bounds: Bounds<Pixels>,
    action: FileFormatAction,
}

// These are the encodings supported by the existing preview detector and encoder.
const FILE_ENCODINGS: &[(&str, bool)] = &[
    ("UTF-8", false),
    ("UTF-8", true),
    ("UTF-16LE", true),
    ("UTF-16BE", true),
    ("GBK", false),
    ("GB18030", false),
    ("Big5", false),
    ("Shift_JIS", false),
    ("EUC-KR", false),
    ("windows-1252", false),
    ("windows-1251", false),
];

impl IdeSurface {
    fn file_error_message(&self, error: &IdeFileError) -> String {
        match error.kind {
            IdeFileErrorKind::TooLarge => self.labels.file_too_large.clone(),
            IdeFileErrorKind::InvalidEncoding => self.labels.invalid_encoding.clone(),
            IdeFileErrorKind::UnrepresentableText => self.labels.unrepresentable_text.clone(),
            _ => error.message.clone(),
        }
    }

    fn render_file_format_triggers(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab_id) = self.workspace.active_tab() else {
            return div().into_any_element();
        };
        let Some(buffer) = self.workspace.buffer(tab_id) else {
            return div().into_any_element();
        };
        let format = &buffer.format;
        let mut row = div().flex().items_center().gap_2().flex_shrink_0();
        if self
            .editors
            .get(&tab_id)
            .is_some_and(|editor| editor.read(cx).is_large_file())
        {
            row = row.child(self.labels.large_file.clone());
        }
        for (index, label, action, anchor) in [
            (
                0,
                format!(
                    "{}{}",
                    format.encoding,
                    if format.has_bom { " BOM" } else { "" }
                ),
                FileFormatAction::Encoding,
                SelectAnchorId::IdeFileEncoding,
            ),
            (
                1,
                format.line_ending.label().to_string(),
                FileFormatAction::LineEnding,
                SelectAnchorId::IdeFileLineEnding,
            ),
        ] {
            let entity = cx.entity();
            let disabled =
                self.loading_file_tabs.contains(&tab_id) || self.saving_tabs.contains(&tab_id);
            let trigger = div()
                .px_1()
                .rounded(px(self.tokens.radii.xs))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                .when(disabled, |this| this.opacity(0.5).cursor_default())
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if this.loading_file_tabs.contains(&tab_id)
                            || this.saving_tabs.contains(&tab_id)
                        {
                            return;
                        }
                        if let Some(bounds) = this.file_format_bounds[index] {
                            this.file_format_menu = Some(FileFormatMenu {
                                tab_id,
                                bounds,
                                action,
                            });
                            this.agent_status_menu = None;
                            this.tab_context_menu = None;
                            this.tree_context_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                );
            row = row.child(select_anchor_probe(
                anchor,
                trigger,
                move |anchor, _, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        if this.file_format_bounds[index] == Some(anchor.bounds) {
                            return;
                        }
                        this.file_format_bounds[index] = Some(anchor.bounds);
                        if let Some(menu) = this.file_format_menu.as_mut() {
                            let menu_index =
                                usize::from(matches!(menu.action, FileFormatAction::LineEnding));
                            if menu_index == index && menu.tab_id == tab_id {
                                menu.bounds = anchor.bounds;
                                cx.notify();
                            }
                        }
                    });
                },
            ));
        }
        row.into_any_element()
    }

    fn render_file_format_menu(
        &self,
        menu: FileFormatMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut popup = div()
            .w(px(250.0))
            .py_1()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel))
            .shadow_lg()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .occlude();
        let title = match menu.action {
            FileFormatAction::Encoding => self.labels.file_encoding.clone(),
            FileFormatAction::Reopen => self.labels.reopen_encoding.clone(),
            FileFormatAction::SaveEncoding => self.labels.save_encoding.clone(),
            FileFormatAction::LineEnding => self.labels.file_line_ending.clone(),
        };
        popup = popup.child(
            div()
                .px_2()
                .py_1()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(title),
        );
        let mut row_count = 1;
        match menu.action {
            FileFormatAction::Encoding => {
                for (label, action) in [
                    (
                        self.labels.reopen_encoding.clone(),
                        FileFormatAction::Reopen,
                    ),
                    (
                        self.labels.save_encoding.clone(),
                        FileFormatAction::SaveEncoding,
                    ),
                ] {
                    popup = popup.child(self.render_agent_status_menu_item(
                        self.icon("lucide/chevron-right.svg", 12.0, self.tokens.ui.text_muted),
                        label,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            if matches!(action, FileFormatAction::Reopen)
                                && this.is_tab_dirty(menu.tab_id, cx)
                            {
                                this.last_error = Some(this.labels.reopen_dirty.clone());
                                this.file_format_menu = None;
                            } else {
                                this.file_format_menu = Some(FileFormatMenu { action, ..menu });
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                    row_count += 1;
                }
            }
            FileFormatAction::LineEnding => {
                for ending in [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr] {
                    let selected = self
                        .workspace
                        .buffer(menu.tab_id)
                        .is_some_and(|b| b.format.line_ending == ending);
                    popup = popup.child(self.render_agent_status_menu_item(
                        self.icon(
                            if selected {
                                "lucide/check.svg"
                            } else {
                                "lucide/minus.svg"
                            },
                            12.0,
                            self.tokens.ui.text_muted,
                        ),
                        ending.label().to_string(),
                        false,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(buffer) = this.workspace.buffer(menu.tab_id) {
                                let mut format = buffer.format.clone();
                                format.line_ending = ending;
                                let _ = this.workspace.set_file_format(menu.tab_id, format);
                            }
                            this.file_format_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                    row_count += 1;
                }
            }
            FileFormatAction::Reopen | FileFormatAction::SaveEncoding => {
                for &(encoding, has_bom) in FILE_ENCODINGS {
                    if matches!(menu.action, FileFormatAction::Reopen)
                        && encoding == "UTF-8"
                        && has_bom
                    {
                        continue;
                    }
                    let selected = self.workspace.buffer(menu.tab_id).is_some_and(|b| {
                        b.format.encoding == encoding && b.format.has_bom == has_bom
                    });
                    let label = format!(
                        "{encoding}{}",
                        if has_bom && matches!(menu.action, FileFormatAction::SaveEncoding) {
                            " BOM"
                        } else {
                            ""
                        }
                    );
                    popup = popup.child(self.render_agent_status_menu_item(
                        self.icon(
                            if selected {
                                "lucide/check.svg"
                            } else {
                                "lucide/minus.svg"
                            },
                            12.0,
                            self.tokens.ui.text_muted,
                        ),
                        label,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.file_format_menu = None;
                            if matches!(menu.action, FileFormatAction::Reopen) {
                                this.reopen_with_encoding(menu.tab_id, encoding, cx);
                            } else if let Some(buffer) = this.workspace.buffer(menu.tab_id) {
                                let format = TextFileFormat {
                                    encoding: encoding.into(),
                                    has_bom,
                                    line_ending: buffer.format.line_ending,
                                };
                                let _ = this.workspace.set_file_format(menu.tab_id, format);
                                this.save_tab(menu.tab_id, cx);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                    row_count += 1;
                }
            }
        }
        let height = (row_count as f32 * IDE_AGENT_MENU_ITEM_HEIGHT + 12.0)
            .min(f32::from(window.viewport_size().height) - 16.0);
        let x = f32::from(menu.bounds.right()).min(f32::from(window.viewport_size().width) - 8.0)
            - 250.0;
        let y = (f32::from(menu.bounds.top()) - height - 6.0).max(8.0);
        let popup = popup
            .id("ide-file-format-options")
            .max_h(px(height))
            .overflow_y_scroll()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        popover_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.file_format_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.file_format_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position(gpui::point(px(x.max(8.0)), px(y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(popup),
                )
                .with_priority(IDE_AGENT_MENU_Z),
            )
            .into_any_element()
    }

    fn reopen_with_encoding(
        &mut self,
        tab_id: EditorTabId,
        encoding: &'static str,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_remote_actions_ready(cx)
            || self.loading_file_tabs.contains(&tab_id)
            || self.saving_tabs.contains(&tab_id)
        {
            return;
        }
        if self.is_tab_dirty(tab_id, cx) {
            self.last_error = Some(self.labels.reopen_dirty.clone());
            return;
        }
        let Some(buffer) = self.workspace.buffer(tab_id) else {
            return;
        };
        let location = buffer.location.clone();
        let revision = buffer.revision;
        let generation = self.generation;
        let fs = self.fs.clone();
        let runtime = self.backend_runtime.clone();
        self.loading_file_tabs.insert(tab_id);
        cx.spawn(async move |weak, cx| {
            let read_location = location.clone();
            let result = await_ide_backend(
                runtime.spawn(async move { fs.read_file(&read_location, Some(encoding)).await }),
            )
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_file_tabs.remove(&tab_id);
                if this
                    .workspace
                    .buffer(tab_id)
                    .is_none_or(|b| b.location != location || b.revision != revision)
                    || this.is_tab_dirty(tab_id, cx)
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(data) => {
                        let _ = this
                            .workspace
                            .replace_buffer_text(tab_id, data.text.clone());
                        let _ = this.workspace.set_file_format(tab_id, data.format);
                        let _ = this.workspace.mark_saved(tab_id, data.version);
                        this.create_editor(tab_id, &location, data.text, cx);
                    }
                    Err(error) => this.last_error = Some(this.file_error_message(&error)),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_theme::default_tokens;

    #[gpui::test]
    fn pasted_crlf_is_normalized_in_the_editor(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| TextEditorView::new("", &default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                "first\r\nsecond\r\n".into(),
            ));
            editor.paste_from_clipboard(cx);
            assert_eq!(editor.buffer().text(), "first\nsecond\n");
        });
    }

    #[gpui::test]
    fn format_only_edits_are_dirty_and_survive_reconnect_snapshot(cx: &mut TestAppContext) {
        let router =
            oxideterm_ssh::NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::default());
        let backend = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(),
        );
        let surface = cx.new(|cx| {
            IdeSurface::new(
                NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled),
                default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend,
                cx,
            )
        });
        surface.update(cx, |surface, cx| {
            surface.node_id = Some("format-test".into());
            surface.root_path = Some("/repo".into());
            surface.load_state = IdeLoadState::Ready;
            surface
                .workspace
                .open_project(IdeLocation::remote("format-test", "/repo"), "repo");
            let location = IdeLocation::remote("format-test", "/repo/file.txt");
            surface
                .workspace
                .open_file(location.clone(), "text\n", SavedFileVersion::unknown())
                .unwrap();
            let tab = surface.workspace.active_tab().unwrap();
            surface.create_editor(tab, &location, "text\n".into(), cx);
            surface
                .workspace
                .set_file_format(
                    tab,
                    TextFileFormat {
                        line_ending: LineEnding::CrLf,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(surface.is_tab_dirty(tab, cx));
            let snapshot = surface.reconnect_snapshot(cx).unwrap();
            assert_eq!(snapshot.file_formats["/repo/file.txt"].line_ending, "CRLF");
            assert_eq!(snapshot.dirty_contents["/repo/file.txt"], "text\n");
            surface.reopen_with_encoding(tab, "GBK", cx);
            assert_eq!(
                surface.last_error.as_deref(),
                Some(surface.labels.reopen_dirty.as_str())
            );
            surface.close_tab(tab, cx);
            assert!(surface.workspace.pending_close().is_some());
        });
    }
}
