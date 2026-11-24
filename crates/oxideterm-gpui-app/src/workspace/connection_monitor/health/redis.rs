use super::*;
use oxideterm_gpui_host_tools::*;
use oxideterm_gpui_ui::scroll::{Scrollbar, ScrollbarAxis};
use oxideterm_gpui_ui::text_input::text_input_anchor_probe;
/// The value editor draws 12px mono text; the scroll band is measured with
/// the same size so horizontal scrolling never clips a line.
const HOST_REDIS_VALUE_EDITOR_TEXT_SIZE: f32 = 12.0;
/// Mono width of the widest editor line. The band scrolls horizontally instead
/// of clipping the document at the dialog edge.
fn redis_value_editor_content_width(value: &str) -> f32 {
    value
        .lines()
        .map(|line| oxideterm_gpui_ui::estimate_text_width(line, HOST_REDIS_VALUE_EDITOR_TEXT_SIZE))
        .fold(0.0_f32, f32::max)
        + HOST_REDIS_VALUE_SCROLL_SLACK
}

impl WorkspaceApp {
    pub(super) fn render_host_redis_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let password_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::RedisPassword, cx)
            .expect("redis password is a registered Host Tools input");
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::RedisSearch, cx)
            .expect("redis key search is a registered Host Tools input");
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_redis_panel(
                password_ime,
                search_ime,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_redis_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let escape = key == "escape" && !event.keystroke.modifiers.platform;
        let submit = matches!(key, "enter" | "return");

        if self.host_tools.read(cx).redis_delete_open() {
            return match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_redis_delete(cx);
                    });
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.clear_standard_confirm_focus();
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.confirm_redis_delete(cx);
                    });
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            };
        }

        let host_tools = self.host_tools.read(cx);
        let create_key_focused = host_tools
            .ui
            .input_is_focused(HostToolsTextInput::RedisCreateKeyName)
            || host_tools
                .ui
                .input_is_focused(HostToolsTextInput::RedisCreateKeyValue)
            || host_tools
                .ui
                .input_is_focused(HostToolsTextInput::RedisCreateKeyField)
            || host_tools
                .ui
                .input_is_focused(HostToolsTextInput::RedisCreateKeyTtl);
        let set_value_focused = host_tools
            .ui
            .input_is_focused(HostToolsTextInput::RedisSetValue);
        let rename_focused = host_tools
            .ui
            .input_is_focused(HostToolsTextInput::RedisRename);
        let ttl_focused = host_tools.ui.input_is_focused(HostToolsTextInput::RedisTtl);
        let search_focused = host_tools
            .ui
            .input_is_focused(HostToolsTextInput::RedisSearch);
        let password_focused = host_tools
            .ui
            .input_is_focused(HostToolsTextInput::RedisPassword);
        let value_open = host_tools.redis_value_dialog_open();
        let rename_open = host_tools.redis_rename_open();
        let ttl_open = host_tools.redis_ttl_edit_open();
        let set_value_open = host_tools.redis_set_value_open();
        let create_key_open = host_tools.redis_create_key_open();

        if search_focused {
            if submit {
                // The field filters what is loaded; Enter asks the server for
                // the full match set instead of only the capped scan.
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.request_redis_keys_for_selected(cx);
                });
                return true;
            }
            if escape {
                self.host_tools.update(cx, |host_tools, _cx| {
                    host_tools.ui.clear_input_focus();
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                return true;
            }
            return false;
        }

        if create_key_focused {
            if escape {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_redis_create_key(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                return true;
            }
            if submit {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.submit_redis_create_key(cx);
                });
                return true;
            }
            return false;
        }

        if set_value_focused {
            if escape {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_redis_set_value(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                return true;
            }
            // The value editor is multi-line: Enter belongs to the text, and
            // the footer button saves. (Enter was submitted here when the
            // editor was a single-line field.)
            return false;
        }

        if ttl_focused {
            if escape {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_redis_ttl_edit(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                return true;
            }
            if submit {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.submit_redis_ttl(cx);
                });
                return true;
            }
            return false;
        }

        if rename_focused {
            if escape {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_redis_rename(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                return true;
            }
            if submit {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.submit_redis_rename(cx);
                });
                return true;
            }
            return false;
        }
        if password_focused {
            if escape {
                self.host_tools.update(cx, |host_tools, _cx| {
                    host_tools.ui.clear_input_focus();
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                return true;
            }
            if submit {
                // Enter re-reads the selected endpoint with the password
                // currently in the field, so typing a password does not
                // require a pointer.
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.request_redis_keys_for_selected(cx);
                });
                return true;
            }
            return false;
        }
        if escape && create_key_open {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_redis_create_key(cx);
            });
            return true;
        }
        // Inline editors close before the viewer they live in.
        if escape && ttl_open {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_redis_ttl_edit(cx);
            });
            return true;
        }
        if escape && rename_open {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_redis_rename(cx);
            });
            return true;
        }
        if escape && set_value_open {
            // The editor lives inside the value viewer, so Escape closes what
            // the user sees: the viewer. A focused value input cancels just
            // the edit through the branch above.
            self.host_tools.update(cx, |host_tools, cx| {
                if host_tools.redis_value_dialog_open() {
                    host_tools.dismiss_redis_value(cx);
                } else {
                    host_tools.dismiss_redis_set_value(cx);
                }
            });
            return true;
        }
        if escape && value_open {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_redis_value(cx);
            });
            return true;
        }
        false
    }
    fn render_host_redis_create_input(
        &self,
        target: WorkspaceImeTarget,
        input: HostToolsTextInput,
        value: String,
        placeholder: String,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.host_tools.read(cx).ui.input_is_focused(input);
        let workspace = cx.entity();
        let control = self
            .workspace_text_input(
                target,
                TextInputView {
                    value: value.as_str(),
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(height))
            .cursor(CursorStyle::IBeam);
        text_input_anchor_probe(
            target.anchor_id(),
            control
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            host_tools.ui.focus_input(input);
                        });
                        this.ime_marked_text = None;
                        this.show_active_input_caret(cx);
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn scalar_value_input(
        &self,
        value: &str,
        editor_height: f32,
        editor_scroll: &gpui::ScrollHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::HostRedisSetValue;
        let line_height = crate::workspace::ime::HOST_REDIS_VALUE_EDITOR_LINE_HEIGHT;
        let focused = self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::RedisSetValue);
        let workspace = cx.entity();
        let content_width = redis_value_editor_content_width(&value);
        let editor_scroll = editor_scroll.clone();
        let lines = self.render_settings_multiline_textarea_lines(
            div().flex().flex_col().items_start().gap(px(0.0)),
            target,
            &value,
            false,
            line_height,
            cx,
        );
        // Frame, padding and focus ring belong to the scroll viewport
        // so they stay put while the lines scroll underneath; the bars
        // are siblings so they cannot scroll out of view either.
        let viewport = div()
            .relative()
            .w_full()
            .min_w_0()
            .h(px(editor_height))
            .child(
                div()
                    .id("host-redis-value-editor-scroll")
                    .size_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba(if focused {
                        (theme.accent << 8) | 0x99
                    } else {
                        (theme.border << 8) | 0xFF
                    }))
                    .bg(rgb(theme.bg))
                    .px_3()
                    .py_2()
                    .cursor(CursorStyle::IBeam)
                    .text_size(px(HOST_REDIS_VALUE_EDITOR_TEXT_SIZE))
                    .line_height(px(line_height))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text))
                    .overflow_scroll()
                    .track_scroll(&editor_scroll)
                    .child(
                        text_input_anchor_probe(
                            target.anchor_id(),
                            lines.min_w(px(content_width)),
                            move |anchor, _window, cx| {
                                let _ = workspace.update(cx, |this, cx| {
                                    this.update_text_input_anchor(anchor, cx);
                                });
                            },
                        )
                        .into_any_element(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.host_tools.update(cx, |host_tools, _cx| {
                                host_tools.ui.focus_input(HostToolsTextInput::RedisSetValue);
                            });
                            this.ime_marked_text = None;
                            this.show_active_input_caret(cx);
                            window.focus(&this.focus_handle, cx);
                            this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    })),
            )
            .child(
                Scrollbar::new(&editor_scroll)
                    .id("host-redis-value-editor-scrollbar")
                    .axis(ScrollbarAxis::Both),
            );

        viewport.into_any_element()
    }

    pub(in crate::workspace) fn mount_host_tool_dialogs(
        host: Entity<HostToolsEntity>,
        tokens: ThemeTokens,
        i18n: I18n,
        font: SharedString,
        cx: &mut Context<Self>,
    ) -> Entity<HostToolsDialogs> {
        let owner = DialogWindow(cx.weak_entity());
        cx.new(|cx| HostToolsDialogs::new(host, Box::new(owner), tokens, i18n, font, cx))
    }
}

struct DialogWindow(gpui::WeakEntity<WorkspaceApp>);

impl HostToolDialogWindow for DialogWindow {
    fn input(
        &self,
        input: HostToolsTextInput,
        value: &str,
        placeholder: String,
        height: f32,
        cx: &mut App,
    ) -> AnyElement {
        self.0
            .update(cx, |app, cx| {
                let target =
                    crate::workspace::ime::workspace_ime_target_for_plain_host_tools_input(input)
                        .expect("dialog fields have a workspace IME target");
                let focused = app.host_tools.read(cx).ui.input_is_focused(input);
                match input {
                    HostToolsTextInput::ContainerRunPorts
                    | HostToolsTextInput::ContainerRunVolumes => {
                        let id = if input == HostToolsTextInput::ContainerRunPorts {
                            "host-container-run-ports-scroll"
                        } else {
                            "host-container-run-volumes-scroll"
                        };
                        app.render_container_run_spec_area(
                            target,
                            input,
                            value,
                            placeholder,
                            focused,
                            id,
                            cx,
                        )
                    }
                    HostToolsTextInput::ContainerRunName
                    | HostToolsTextInput::ContainerRunCommand
                    | HostToolsTextInput::ContainerDialog
                    | HostToolsTextInput::ContainerDialogRegistry => app
                        .render_container_dialog_text_field(
                            target,
                            input,
                            value,
                            placeholder,
                            focused,
                            cx,
                        ),
                    _ => app.render_host_redis_create_input(
                        target,
                        input,
                        value.to_owned(),
                        placeholder,
                        height,
                        cx,
                    ),
                }
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
    fn scalar_editor(
        &self,
        value: &str,
        height: f32,
        scroll: &gpui::ScrollHandle,
        cx: &mut App,
    ) -> AnyElement {
        self.0
            .update(cx, |app, cx| {
                app.scalar_value_input(value, height, scroll, cx)
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
    fn selection(
        &self,
        group: u64,
        scope: &'static str,
        order: usize,
        text: String,
        color: u32,
        font: Option<SharedString>,
        cx: &mut App,
    ) -> AnyElement {
        self.0
            .update(cx, |app, cx| {
                app.render_row_safe_selectable_display_text_in_group(
                    group, scope, order, order, text, color, font, cx,
                )
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
    fn clear_composition(&self, cx: &mut App) {
        let _ = self.0.update(cx, |app, cx| {
            app.ime_marked_text = None;
            app.clear_ime_selection();
            cx.notify();
        });
    }
    fn submit_container(&self, window: &mut Window, cx: &mut App) {
        let _ = self.0.update(cx, |app, cx| {
            app.submit_host_container_input_dialog(window, cx)
        });
    }
}
