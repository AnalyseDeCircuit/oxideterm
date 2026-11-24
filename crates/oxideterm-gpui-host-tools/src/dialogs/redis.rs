use super::*;
use oxideterm_connection_monitor::redis_value_json;
use oxideterm_gpui_ui::scroll::{Scrollbar, ScrollbarAxis};
const HOST_REDIS_VALUE_EDITOR_CHROME_HEIGHT: f32 = 48.0;
impl HostToolsDialogs {
    fn render_redis_inline_chip(
        &self,
        id: &'static str,
        label: String,
        accent: bool,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
        listener: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .id(id)
            .h(px(22.0))
            .px_2()
            .flex_none()
            .flex()
            .items_center()
            .rounded(px(tokens.radii.sm))
            .cursor_pointer()
            .text_size(px(10.0))
            .text_color(if accent {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .bg(if accent {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    listener(this, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    /// Key title, inline rename and the metadata line with the editable TTL
    /// chip. Dialog actions stay on the right; rename and set-value moved here
    /// so neither needs a popup of its own.
    fn render_redis_value_header(
        &self,
        dialog: &HostRedisValueDialogView,
        editor_open: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: gpui::SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let mut metadata = format!("db{}", dialog.database);
        if let Some(kind) = dialog.kind_label {
            metadata.push_str("  ·  ");
            metadata.push_str(&i18n.t("sidebar.host_redis.value.type"));
            metadata.push(' ');
            metadata.push_str(kind);
        }
        if let Some(size) = dialog.size_text.as_deref() {
            metadata.push_str("  ·  ");
            metadata.push_str(&i18n.t("sidebar.host_redis.value.size"));
            metadata.push(' ');
            metadata.push_str(size);
        }

        let key_row = if dialog.renaming {
            div()
                .min_w_0()
                .h(px(22.0))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div().flex_1().min_w_0().child(
                        self.input(
                            HostToolsTextInput::RedisRename,
                            self.host_tools
                                .read(cx)
                                .ui
                                .input_value(HostToolsTextInput::RedisRename)
                                .unwrap_or_default()
                                .to_string(),
                            i18n.t("sidebar.host_redis.rename.placeholder"),
                            22.0,
                            cx,
                        ),
                    ),
                )
                .child(self.render_redis_inline_chip(
                    "host-redis-rename-confirm",
                    i18n.t("sidebar.host_redis.value.confirm"),
                    true,
                    tokens,
                    cx,
                    |this, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.submit_redis_rename(cx);
                        });
                        cx.notify();
                    },
                ))
                .child(self.render_redis_inline_chip(
                    "host-redis-rename-cancel",
                    i18n.t("sidebar.host_redis.value.cancel"),
                    false,
                    tokens,
                    cx,
                    |this, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_redis_rename(cx);
                        });
                        this.window.clear_composition(cx);
                        cx.notify();
                    },
                ))
        } else {
            let key = dialog.key.clone();
            let database = dialog.database;
            div()
                .min_w_0()
                .h(px(22.0))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(mono_font_family)
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(dialog.key.clone()),
                )
                .child(host_tools_tooltip_icon_button(
                    tokens,
                    "lucide/circle.svg",
                    11.0,
                    rgb(theme.text_muted),
                    IconButtonOptions {
                        size: 18.0,
                        has_background: false,
                        idle_opacity: 1.0,
                        ..IconButtonOptions::compact(18.0)
                    },
                    i18n.t("sidebar.host_redis.actions.rename"),
                    "host-redis-key-rename",
                    true,
                    cx.listener(move |this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.open_redis_rename(database, key.clone(), cx);
                        });
                        cx.stop_propagation();
                    }),
                ))
        };

        let ttl_segment = if dialog.ttl_editing {
            div()
                .id("host-redis-ttl-editor")
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(i18n.t("sidebar.host_redis.value.ttl")),
                )
                .child(
                    div().w(px(64.0)).child(
                        self.input(
                            HostToolsTextInput::RedisTtl,
                            self.host_tools
                                .read(cx)
                                .ui
                                .input_value(HostToolsTextInput::RedisTtl)
                                .unwrap_or_default()
                                .to_string(),
                            i18n.t("sidebar.host_redis.value.ttl_placeholder"),
                            22.0,
                            cx,
                        ),
                    ),
                )
                .child(self.render_redis_inline_chip(
                    "host-redis-ttl-save",
                    i18n.t("sidebar.host_redis.value.confirm"),
                    true,
                    tokens,
                    cx,
                    |this, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.submit_redis_ttl(cx);
                        });
                        cx.notify();
                    },
                ))
                .child(self.render_redis_inline_chip(
                    "host-redis-ttl-never",
                    i18n.t("sidebar.host_redis.value.ttl_never_action"),
                    false,
                    tokens,
                    cx,
                    |this, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.submit_redis_ttl_never(cx);
                        });
                        cx.notify();
                    },
                ))
                .child(self.render_redis_inline_chip(
                    "host-redis-ttl-cancel",
                    i18n.t("sidebar.host_redis.value.cancel"),
                    false,
                    tokens,
                    cx,
                    |this, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_redis_ttl_edit(cx);
                        });
                        this.window.clear_composition(cx);
                        cx.notify();
                    },
                ))
        } else {
            let ttl_text = dialog
                .ttl
                .map(|ttl| match ttl {
                    oxideterm_connection_monitor::ResourceRedisTtl::Never => {
                        i18n.t("sidebar.host_redis.value.ttl_never")
                    }
                    oxideterm_connection_monitor::ResourceRedisTtl::Missing => {
                        i18n.t("sidebar.host_redis.value.ttl_missing")
                    }
                    oxideterm_connection_monitor::ResourceRedisTtl::Seconds(seconds) => i18n
                        .t("sidebar.host_redis.value.ttl_seconds")
                        .replace("{{seconds}}", &seconds.to_string()),
                    oxideterm_connection_monitor::ResourceRedisTtl::Unknown => "-".to_string(),
                })
                .unwrap_or_else(|| "-".to_string());
            div()
                .id("host-redis-ttl-chip")
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .py(px(1.0))
                .rounded(px(tokens.radii.sm))
                .cursor_pointer()
                .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
                .text_size(px(10.0))
                .text_color(rgb(theme.text_muted))
                .child(format!(
                    "{} {}",
                    i18n.t("sidebar.host_redis.value.ttl"),
                    ttl_text
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.open_redis_ttl_edit(cx);
                        });
                        cx.stop_propagation();
                    }),
                )
        };

        let (copy_key, copy_value_key, delete_key, edit_key) = (
            dialog.key.clone(),
            dialog.key.clone(),
            dialog.key.clone(),
            dialog.key.clone(),
        );
        let database = dialog.database;
        let json_view = dialog.json_view;
        let json_available = dialog.json_available;
        let refresh_disabled = dialog.loading;
        let collection_editable = dialog.can_set_value
            && !matches!(
                dialog.kind,
                Some(oxideterm_connection_monitor::ResourceRedisValueKind::String)
                    | Some(oxideterm_connection_monitor::ResourceRedisValueKind::Json)
            );

        // The header keeps a fixed height: inline editors must not resize the
        // dialog while it is centred. Errors therefore replace the metadata
        // text instead of adding a line of their own.
        let (meta_text, meta_color) = match (dialog.rename_error.clone(), dialog.ttl_error.clone())
        {
            (Some(error), _) => (error, MONITOR_RED),
            (None, Some(error)) => (error, MONITOR_RED),
            (None, None) => (metadata, theme.text_muted),
        };
        div()
            .flex_none()
            .h(px(72.0))
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .overflow_hidden()
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(key_row)
                    .child(
                        div()
                            .min_w_0()
                            .h(px(22.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(10.0))
                            .text_color(rgb(meta_color))
                            .child(div().flex_none().truncate().child(meta_text))
                            .child(ttl_segment),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(json_available, |row| {
                        row.child(self.render_redis_value_view_toggle(json_view, tokens, i18n, cx))
                    })
                    .when(collection_editable && !editor_open, |row| {
                        row.child(host_tools_tooltip_icon_button(
                            tokens,
                            "lucide/pencil.svg",
                            13.0,
                            rgb(theme.text),
                            IconButtonOptions {
                                size: 24.0,
                                has_background: true,
                                background: Some(rgb(theme.bg_hover)),
                                hover_background: Some(rgb(theme.bg_panel)),
                                idle_opacity: 1.0,
                                ..IconButtonOptions::compact(24.0)
                            },
                            i18n.t("sidebar.host_redis.actions.set_value"),
                            "host-redis-value-set",
                            true,
                            cx.listener(move |this, _event, _window, cx| {
                                let key = edit_key.clone();
                                this.host_tools.update(cx, |host_tools, cx| {
                                    host_tools.open_redis_set_value(database, key, cx);
                                });
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/copy.svg",
                        13.0,
                        rgb(theme.text),
                        IconButtonOptions {
                            size: 24.0,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_redis.actions.copy_key"),
                        "host-redis-value-copy-key",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            let key = copy_key.clone();
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.copy_redis_key(key, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/file-text.svg",
                        13.0,
                        rgb(theme.text),
                        IconButtonOptions {
                            size: 24.0,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_redis.actions.copy_value"),
                        "host-redis-value-copy-value",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            let key = copy_value_key.clone();
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.copy_redis_value(database, key, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/trash-2.svg",
                        13.0,
                        rgb(MONITOR_RED),
                        IconButtonOptions {
                            size: 24.0,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_redis.actions.delete"),
                        "host-redis-value-delete",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            let key = delete_key.clone();
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.request_redis_delete(database, key, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/refresh-cw.svg",
                        13.0,
                        rgb(theme.text),
                        IconButtonOptions {
                            size: 24.0,
                            disabled: refresh_disabled,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_redis.actions.refresh"),
                        "host-redis-value-refresh",
                        true,
                        cx.listener(|this, _event, _window, cx| {
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.refresh_redis_value(cx);
                            });
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }
    /// The set-value editor rendered in place of the value box: no dialog of
    /// its own, so the value stays where the user is looking.
    fn render_redis_value_inline_editor(
        &self,
        dialog: &HostRedisValueDialogView,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (kind, value, field, entries, error, loading, json_view) = {
            let host_tools = self.host_tools.read(cx);
            let draft = host_tools.ui.host_redis_set_value_dialog.as_ref()?;
            // A draft belongs to one target; showing it under another key would
            // let a save write the wrong key.
            let viewer = host_tools.host_redis.value_dialog.as_ref()?;
            if viewer.connection_id != draft.connection_id
                || viewer.endpoint != draft.endpoint
                || viewer.database != draft.database
                || viewer.key != draft.key
            {
                return None;
            }
            (
                draft.kind,
                draft.value.to_string(),
                draft.field.to_string(),
                draft
                    .entries
                    .iter()
                    .map(|entry| entry.to_string())
                    .collect::<Vec<_>>(),
                draft.error.clone(),
                draft.loading,
                viewer.json_view,
            )
        };
        let theme = self.tokens.ui;
        let i18n = &self.i18n;
        let edit_unsupported =
            redis_set_value_entries_unsupported(kind, entries.iter().map(String::as_str));
        let unchanged = self
            .host_tools
            .read(cx)
            .ui
            .host_redis_set_value_dialog
            .as_ref()
            .is_some_and(|dialog| match (kind, dialog.base.as_ref()) {
                (RedisValueWriteKind::String, Some(base)) => base.body == value,
                (RedisValueWriteKind::Json, Some(base)) => {
                    redis_value_json(base).is_some_and(|json| json == value)
                }
                _ => false,
            });
        // The editor's own padding and hint row spend part of the dialog body;
        // the text band takes the rest so viewer and editor keep one size.
        let editor_height = (dialog.size.1 - HOST_REDIS_VALUE_EDITOR_CHROME_HEIGHT).max(120.0);
        let mut body = div().w_full().min_w_0().p_4().flex().flex_col().gap_2();
        match kind {
            RedisValueWriteKind::String | RedisValueWriteKind::Json => {
                body = body.child(self.window.scalar_editor(
                    &value,
                    editor_height,
                    &dialog.editor_scroll,
                    cx,
                ));
                let mut hint_row = div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(if kind == RedisValueWriteKind::Json {
                        i18n.t("sidebar.host_redis.set_value.json_hint")
                    } else {
                        i18n.t("sidebar.host_redis.set_value.text_hint")
                    });
                if kind == RedisValueWriteKind::Json
                    || (kind == RedisValueWriteKind::String && json_view)
                {
                    // Editing compact JSON by hand is painful; format it in
                    // place before saving (the save itself minifies again).
                    hint_row = hint_row.child(self.render_redis_inline_chip(
                        "host-redis-json-format",
                        i18n.t("sidebar.host_redis.set_value.format_json"),
                        false,
                        &self.tokens,
                        cx,
                        |this, cx| {
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.format_redis_json_value(cx);
                            });
                            cx.notify();
                        },
                    ));
                }
                body = body.child(hint_row);
            }
            RedisValueWriteKind::Hash
            | RedisValueWriteKind::List
            | RedisValueWriteKind::Set
            | RedisValueWriteKind::SortedSet => {
                let mut entry_row = div().flex().items_center().gap_2();
                if kind == RedisValueWriteKind::Hash {
                    entry_row = entry_row.child(div().flex_1().min_w_0().child(self.input(
                        HostToolsTextInput::RedisSetValueField,
                        field,
                        i18n.t("sidebar.host_redis.create_key.field_placeholder"),
                        32.0,
                        cx,
                    )));
                }
                entry_row = entry_row.child(div().flex_1().min_w_0().child(self.input(
                    HostToolsTextInput::RedisSetValue,
                    value,
                    i18n.t("sidebar.host_redis.create_key.element_placeholder"),
                    32.0,
                    cx,
                )));
                entry_row = entry_row.child(
                    div()
                        .id("host-redis-set-value-add")
                        .h(px(32.0))
                        .px_3()
                        .flex()
                        .items_center()
                        .rounded(px(self.tokens.radii.md))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.text))
                        .bg(rgb(theme.bg_hover))
                        .hover(|button| button.bg(rgb(theme.bg_panel)))
                        .child(i18n.t("sidebar.host_redis.create_key.add_entry"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.host_tools.update(cx, |host_tools, cx| {
                                    host_tools.add_redis_set_value_entry(cx);
                                });
                                cx.stop_propagation();
                            }),
                        ),
                );
                body = body.child(entry_row);
                body = body.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(match kind {
                            RedisValueWriteKind::Hash => {
                                i18n.t("sidebar.host_redis.create_key.hash_hint")
                            }
                            RedisValueWriteKind::List => {
                                i18n.t("sidebar.host_redis.create_key.list_hint")
                            }
                            RedisValueWriteKind::Set => {
                                i18n.t("sidebar.host_redis.create_key.set_hint")
                            }
                            _ => i18n.t("sidebar.host_redis.create_key.zset_hint"),
                        }),
                );
                if entries.is_empty() {
                    body = body.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(i18n.t("sidebar.host_redis.create_key.entries_empty")),
                    );
                } else {
                    let mut list = div()
                        .id("host-redis-set-value-entries")
                        .max_h(px(160.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1();
                    for (index, entry) in entries.iter().enumerate() {
                        let display = entry.replace(
                            '\t',
                            if kind == RedisValueWriteKind::Hash {
                                " = "
                            } else {
                                " · "
                            },
                        );
                        list = list.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1()
                                .rounded(px(self.tokens.radii.md))
                                .bg(rgb(theme.bg_hover))
                                .text_size(px(11.0))
                                .text_color(rgb(theme.text))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(self.mono_font.clone())
                                        .child(display),
                                )
                                .child(
                                    div()
                                        .id(("host-redis-set-value-remove", index))
                                        .flex_none()
                                        .cursor_pointer()
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme.text_muted))
                                        .hover(|button| button.text_color(rgb(MONITOR_RED)))
                                        .child(i18n.t("sidebar.host_redis.create_key.remove_entry"))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.host_tools.update(cx, |host_tools, cx| {
                                                    host_tools
                                                        .remove_redis_set_value_entry(index, cx);
                                                });
                                                cx.stop_propagation();
                                            }),
                                        ),
                                ),
                        );
                    }
                    body = body.child(list);
                }
            }
        }
        body = body.when_some(error, |body, error| {
            body.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(MONITOR_RED))
                    .child(error),
            )
        });
        if edit_unsupported {
            body = body.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(MONITOR_RED))
                    .child(i18n.t("sidebar.host_redis.value.edit_unsupported")),
            );
        }
        let footer =
            div()
                .w_full()
                .flex()
                .justify_end()
                .gap_2()
                .child(host_tools_tooltip_icon_button(
                    &self.tokens,
                    "lucide/check.svg",
                    12.0,
                    rgb(theme.text),
                    IconButtonOptions {
                        size: 24.0,
                        disabled: loading || edit_unsupported || unchanged,
                        has_background: true,
                        background: Some(rgb(theme.bg_hover)),
                        hover_background: Some(rgb(theme.bg_panel)),
                        idle_opacity: 1.0,
                        ..IconButtonOptions::compact(24.0)
                    },
                    i18n.t("sidebar.host_redis.set_value.confirm"),
                    "host-redis-value-editor-save",
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.submit_redis_set_value(cx);
                        });
                        cx.notify();
                    }),
                ));
        Some(body.child(footer).into_any_element())
    }

    pub(super) fn render_host_redis_value_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Every size below is clamped to this window: the dialog body scrolls,
        // it never grows past the screen edges.
        let viewport = window.viewport_size();
        let viewport = (f32::from(viewport.width), f32::from(viewport.height));
        let dialog = self.host_tools.read(cx).redis_value_dialog_view(viewport)?;
        let theme = self.tokens.ui;
        let i18n = &self.i18n;
        let mono_font_family = self.mono_font.clone();
        let (dialog_width, dialog_height) = dialog.size;
        let group_id = selection_group(
            "host-redis-value",
            (&dialog.key, dialog.database, dialog.json_view),
        );
        let editor = self.render_redis_value_inline_editor(&dialog, cx);
        let body = match (editor, &dialog.body) {
            (Some(editor), _) => editor,
            (None, body) => match body {
                HostRedisValueBody::Loading => div()
                    .p_4()
                    .text_color(rgb(theme.text_muted))
                    .child(i18n.t("sidebar.host_redis.value.loading"))
                    .into_any_element(),
                HostRedisValueBody::Error(error) => div()
                    .p_4()
                    .text_color(rgb(MONITOR_RED))
                    .child(self.window.selection(
                        group_id,
                        "host-redis-value-error",
                        0,
                        error.clone(),
                        MONITOR_RED,
                        None,
                        cx,
                    ))
                    .into_any_element(),
                HostRedisValueBody::Unsupported(raw_type) => div()
                    .p_4()
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.window.selection(
                            group_id,
                            "host-redis-value-unsupported",
                            0,
                            i18n.t("sidebar.host_redis.value.unsupported")
                                .replace("{{type}}", raw_type),
                            theme.text_muted,
                            None,
                            cx,
                        ),
                    )
                    .into_any_element(),
                HostRedisValueBody::Empty => div()
                    .p_4()
                    .text_color(rgb(theme.text_muted))
                    .child(i18n.t("sidebar.host_redis.value.empty"))
                    .into_any_element(),
                HostRedisValueBody::Rows { two_column } => {
                    let two_column = *two_column;
                    let rows = dialog.rows.clone();
                    let list_state = dialog.list_state.clone();
                    let list_spec = TauriVirtualListSpec::new(
                        px(HOST_REDIS_VALUE_ROW_HEIGHT),
                        HOST_REDIS_VALUE_ROW_OVERSCAN,
                    );
                    let workspace = cx.entity();
                    let row_color = theme.text;
                    let row_muted = theme.text_muted;
                    let row_mono = mono_font_family.clone();
                    // The band is sized from the widest row, so long JSON lines
                    // scroll into view instead of being clipped at the edge.
                    let content_width = dialog.content_width.max(dialog_width);
                    let h_scroll = dialog.h_scroll.clone();
                    div()
                        .relative()
                        .w_full()
                        .min_w_0()
                        .h(px(dialog_height))
                        .child(
                            div()
                                .id("host-redis-value-body-scroll")
                                .size_full()
                                .overflow_x_scroll()
                                .track_scroll(&h_scroll)
                                .child(div().h_full().min_w(px(content_width)).child(
                                    tauri_virtual_list(
                                        list_state,
                                        list_spec,
                                        move |index, _window, cx| {
                                            let rows = Arc::clone(&rows);
                                            let mono = row_mono.clone();
                                            workspace.update(cx, |this, cx| {
                                                let Some(row) = rows.get(index) else {
                                                    return div().into_any_element();
                                                };
                                                this.render_host_redis_value_row(
                                                    row.clone(),
                                                    index,
                                                    two_column,
                                                    group_id,
                                                    row_color,
                                                    row_muted,
                                                    mono,
                                                    cx,
                                                )
                                            })
                                        },
                                    ),
                                )),
                        )
                        .child(
                            Scrollbar::new(&h_scroll)
                                .id("host-redis-value-h-scrollbar")
                                .axis(ScrollbarAxis::Horizontal),
                        )
                        .into_any_element()
                }
            },
        };
        let editor_open = self
            .host_tools
            .read(cx)
            .ui
            .host_redis_set_value_dialog
            .is_some();

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_redis_value(cx);
                        });
                        cx.stop_propagation();
                    }),
                )
                // Resize continuation lives on the backdrop so the corner drag
                // never depends on the pointer staying inside the handle.
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    let viewport = window.viewport_size();
                    let viewport = (f32::from(viewport.width), f32::from(viewport.height));
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.update_redis_value_dialog_resize(event, viewport, cx);
                    });
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.finish_redis_value_dialog_resize(cx);
                        });
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .relative()
                        .w(px(dialog_width))
                        .child(self.render_redis_value_header(
                            &dialog,
                            editor_open,
                            &self.tokens,
                            i18n,
                            mono_font_family.clone(),
                            cx,
                        ))
                        .child(body)
                        .when(dialog.omitted_rows > 0, |panel| {
                            panel.child(
                                div()
                                    .px_4()
                                    .pb_3()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(
                                        i18n.t("sidebar.host_redis.value.truncated")
                                            .replace("{{count}}", &dialog.rows.len().to_string()),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .border_t_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .justify_end()
                                .child(
                                    div()
                                        .id("host-redis-value-close")
                                        .cursor_pointer()
                                        .px_3()
                                        .py_1()
                                        .rounded(px(self.tokens.radii.md))
                                        .hover(|button| button.bg(rgb(theme.bg_hover)))
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text))
                                        .child(i18n.t("sidebar.host_redis.actions.close"))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.host_tools.update(cx, |host_tools, cx| {
                                                    host_tools.dismiss_redis_value(cx);
                                                });
                                                cx.stop_propagation();
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            // Invisible corner hotzone: drag resizes, double
                            // click restores the default size.
                            div()
                                .id("host-redis-value-resize")
                                .absolute()
                                .right(px(2.0))
                                .bottom(px(2.0))
                                .w(px(14.0))
                                .h(px(14.0))
                                .border_r_2()
                                .border_b_2()
                                .border_color(rgba((theme.text_muted << 8) | 0x66))
                                .cursor(CursorStyle::ResizeUpLeftDownRight)
                                .occlude()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                        let viewport = window.viewport_size();
                                        let viewport =
                                            (f32::from(viewport.width), f32::from(viewport.height));
                                        this.host_tools.update(cx, |host_tools, cx| {
                                            host_tools.start_redis_value_dialog_resize(
                                                event, viewport, cx,
                                            );
                                        });
                                        cx.stop_propagation();
                                    }),
                                ),
                        ),
                ))
                .into_any_element(),
        )
    }

    fn render_redis_value_view_toggle(
        &self,
        json_view: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let mut row = div().flex_none().flex().items_center().gap_1().mr_1();
        for (json, label_key) in [
            (false, "sidebar.host_redis.value.view_text"),
            (true, "sidebar.host_redis.value.view_json"),
        ] {
            let active = json == json_view;
            row = row.child(
                div()
                    .id(("host-redis-view-toggle", usize::from(json)))
                    .px_2()
                    .h(px(22.0))
                    .flex()
                    .items_center()
                    .rounded(px(tokens.radii.md))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .when(active, |tab| {
                        tab.bg(rgb(theme.bg_panel)).text_color(rgb(theme.text))
                    })
                    .when(!active, |tab| tab.text_color(rgb(theme.text_muted)))
                    .hover(|tab| tab.bg(rgb(theme.bg_hover)))
                    .child(i18n.t(label_key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.set_redis_value_view(json, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        row.into_any_element()
    }

    /// One value row: both columns are real selectable fragments so a drag
    /// across the two-column layout copies what the viewer shows.
    #[allow(clippy::too_many_arguments)]
    fn render_host_redis_value_row(
        &self,
        row: HostRedisValueRow,
        index: usize,
        two_column: bool,
        group_id: u64,
        color: u32,
        muted: u32,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut line = div()
            .w_full()
            .min_w_0()
            .h(px(HOST_REDIS_VALUE_ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(HOST_REDIS_VALUE_ROW_GAP))
            .text_size(px(HOST_REDIS_VALUE_TEXT_SIZE))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.window.selection(
                        group_id,
                        "host-redis-value-left",
                        index * 2,
                        row.left,
                        color,
                        Some(mono_font_family.clone()),
                        cx,
                    )),
            );
        if two_column && let Some(right) = row.right {
            line = line.child(div().flex_1().min_w_0().overflow_hidden().child(
                self.window.selection(
                    group_id,
                    "host-redis-value-right",
                    index * 2 + 1,
                    right,
                    muted,
                    Some(mono_font_family),
                    cx,
                ),
            ));
        }
        line.into_any_element()
    }
}
