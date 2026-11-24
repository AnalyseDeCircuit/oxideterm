use super::*;

impl HostToolsDialogs {
    pub(super) fn render_host_redis_create_key_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (name, kind, ttl_never, ttl, value, field, entries, database, error, loading) = {
            let host_tools = self.host_tools.read(cx);
            let dialog = host_tools.ui.host_redis_create_key_dialog.as_ref()?;
            (
                dialog.name.to_string(),
                dialog.kind,
                dialog.ttl_never,
                dialog.ttl.to_string(),
                dialog.value.to_string(),
                dialog.field.to_string(),
                dialog
                    .entries
                    .iter()
                    .map(|entry| entry.to_string())
                    .collect::<Vec<_>>(),
                dialog.database,
                dialog.error.clone(),
                dialog.loading,
            )
        };
        let theme = self.tokens.ui;
        let i18n = &self.i18n;
        let mut body = div()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("db{database}")),
            )
            .child(self.input(
                HostToolsTextInput::RedisCreateKeyName,
                name,
                i18n.t("sidebar.host_redis.create_key.name_placeholder"),
                32.0,
                cx,
            ))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(i18n.t("sidebar.host_redis.create_key.type")),
            );

        // Type selector: one chip per writable shape.
        let mut type_row = div().flex().flex_wrap().gap_1();
        for (candidate, label_key) in [
            (
                RedisCreateKind::String,
                "sidebar.host_redis.create_key.types.string",
            ),
            (
                RedisCreateKind::Hash,
                "sidebar.host_redis.create_key.types.hash",
            ),
            (
                RedisCreateKind::List,
                "sidebar.host_redis.create_key.types.list",
            ),
            (
                RedisCreateKind::Set,
                "sidebar.host_redis.create_key.types.set",
            ),
            (
                RedisCreateKind::SortedSet,
                "sidebar.host_redis.create_key.types.sorted_set",
            ),
            (
                RedisCreateKind::Json,
                "sidebar.host_redis.create_key.types.json",
            ),
        ] {
            let active = candidate == kind;
            type_row = type_row.child(
                div()
                    .id(("host-redis-create-kind", candidate as usize))
                    .h(px(24.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .when(active, |chip| {
                        chip.bg(rgb(theme.bg_panel))
                            .border_1()
                            .border_color(rgba((theme.accent << 8) | MONITOR_BORDER_ALPHA))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |chip| {
                        chip.text_color(rgb(theme.text_muted))
                            .hover(|chip| chip.bg(rgb(theme.bg_hover)))
                    })
                    .child(i18n.t(label_key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.set_redis_create_kind(candidate, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        body = body.child(type_row);

        // TTL has its own title with two explicit choices; the seconds field
        // only appears for an expiring key.
        body = body.child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme.text_muted))
                .child(i18n.t("sidebar.host_redis.create_key.ttl")),
        );
        let mut ttl_row = div().flex().items_center().gap_2();
        for (never, id, label_key) in [
            (
                true,
                "host-redis-create-ttl-never",
                "sidebar.host_redis.create_key.ttl_never",
            ),
            (
                false,
                "host-redis-create-ttl-expires",
                "sidebar.host_redis.create_key.ttl_expires",
            ),
        ] {
            let active = ttl_never == never;
            ttl_row = ttl_row.child(
                div()
                    .id(id)
                    .h(px(24.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .when(active, |chip| {
                        chip.bg(rgb(theme.bg_panel))
                            .border_1()
                            .border_color(rgba((theme.accent << 8) | MONITOR_BORDER_ALPHA))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |chip| {
                        chip.text_color(rgb(theme.text_muted))
                            .hover(|chip| chip.bg(rgb(theme.bg_hover)))
                    })
                    .child(i18n.t(label_key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.set_redis_create_ttl_never(never, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        if !ttl_never {
            ttl_row = ttl_row.child(div().flex_1().min_w_0().child(self.input(
                HostToolsTextInput::RedisCreateKeyTtl,
                ttl,
                i18n.t("sidebar.host_redis.create_key.ttl_placeholder"),
                32.0,
                cx,
            )));
        }
        body = body.child(ttl_row);

        // Value editor per shape.
        match kind {
            RedisCreateKind::String | RedisCreateKind::Json => {
                body = body.child(self.input(
                    HostToolsTextInput::RedisCreateKeyValue,
                    value,
                    i18n.t("sidebar.host_redis.create_key.value_placeholder"),
                    HOST_REDIS_VALUE_INPUT_HEIGHT,
                    cx,
                ));
                body = body.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(if kind == RedisCreateKind::Json {
                            i18n.t("sidebar.host_redis.create_key.json_hint")
                        } else {
                            i18n.t("sidebar.host_redis.create_key.value_hint")
                        }),
                );
            }
            RedisCreateKind::Hash
            | RedisCreateKind::List
            | RedisCreateKind::Set
            | RedisCreateKind::SortedSet => {
                let mut entry_row = div().flex().items_center().gap_2();
                if matches!(kind, RedisCreateKind::Hash | RedisCreateKind::SortedSet) {
                    entry_row = entry_row.child(div().flex_1().min_w_0().child(self.input(
                        HostToolsTextInput::RedisCreateKeyField,
                        field,
                        if kind == RedisCreateKind::SortedSet {
                            i18n.t("sidebar.host_redis.create_key.member_placeholder")
                        } else {
                            i18n.t("sidebar.host_redis.create_key.field_placeholder")
                        },
                        32.0,
                        cx,
                    )));
                }
                entry_row = entry_row.child(div().flex_1().min_w_0().child(self.input(
                    HostToolsTextInput::RedisCreateKeyValue,
                    value,
                    if kind == RedisCreateKind::SortedSet {
                        i18n.t("sidebar.host_redis.create_key.score_placeholder")
                    } else {
                        i18n.t("sidebar.host_redis.create_key.element_placeholder")
                    },
                    32.0,
                    cx,
                )));
                entry_row = entry_row.child(
                    div()
                        .id("host-redis-create-add-entry")
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
                                    host_tools.add_redis_create_entry(cx);
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
                            RedisCreateKind::Hash => {
                                i18n.t("sidebar.host_redis.create_key.hash_hint")
                            }
                            RedisCreateKind::List => {
                                i18n.t("sidebar.host_redis.create_key.list_hint")
                            }
                            RedisCreateKind::Set => {
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
                        .id("host-redis-create-entries")
                        .max_h(px(140.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1();
                    for (index, entry) in entries.iter().enumerate() {
                        let display = entry.replace(
                            '\t',
                            if kind == RedisCreateKind::Hash {
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
                                        .id(("host-redis-create-entry-remove", index))
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
                                                    host_tools.remove_redis_create_entry(index, cx);
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

        let cancel_action = self.footer_button(
            i18n.t("sidebar.host_redis.create_key.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_redis_create_key(cx);
                });
                this.window.clear_composition(cx);
                cx.notify();
            },
            cx,
        );
        let submit_action = self.footer_button(
            i18n.t("sidebar.host_redis.create_key.confirm"),
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            loading,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.submit_redis_create_key(cx);
                });
                cx.notify();
            },
            cx,
        );
        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_redis_create_key(cx);
                        });
                        this.window.clear_composition(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(560.0))
                        .max_h(px(620.0))
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .child(i18n.t("sidebar.host_redis.create_key.title")),
                        )
                        .child(body)
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .border_t_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(cancel_action)
                                .child(submit_action),
                        ),
                ))
                .into_any_element(),
        )
    }
}
