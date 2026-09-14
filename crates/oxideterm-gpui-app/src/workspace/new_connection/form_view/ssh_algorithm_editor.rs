use super::*;
use gpui::StatefulInteractiveElement;

use oxideterm_connections::SshAlgorithmPreferences;
use oxideterm_ssh::{SshAlgorithmCategory, SshAlgorithmOffer};

fn category_i18n_key(category: SshAlgorithmCategory) -> &'static str {
    match category {
        SshAlgorithmCategory::Kex => "ssh.form.ssh_algorithms_category_kex",
        SshAlgorithmCategory::HostKey => "ssh.form.ssh_algorithms_category_host_key",
        SshAlgorithmCategory::Cipher => "ssh.form.ssh_algorithms_category_cipher",
        SshAlgorithmCategory::Mac => "ssh.form.ssh_algorithms_category_mac",
        SshAlgorithmCategory::Compression => "ssh.form.ssh_algorithms_category_compression",
    }
}

fn offer_algorithms(offer: &SshAlgorithmOffer, category: SshAlgorithmCategory) -> &[String] {
    match category {
        SshAlgorithmCategory::Kex => &offer.kex,
        SshAlgorithmCategory::HostKey => &offer.host_key_algorithms,
        SshAlgorithmCategory::Cipher => &offer.ciphers,
        SshAlgorithmCategory::Mac => &offer.macs,
        SshAlgorithmCategory::Compression => &offer.compression,
    }
}

fn preference_algorithms(
    preferences: &SshAlgorithmPreferences,
    category: SshAlgorithmCategory,
) -> &[String] {
    match category {
        SshAlgorithmCategory::Kex => &preferences.kex,
        SshAlgorithmCategory::HostKey => &preferences.host_key,
        SshAlgorithmCategory::Cipher => &preferences.cipher,
        SshAlgorithmCategory::Mac => &preferences.mac,
        SshAlgorithmCategory::Compression => &preferences.compression,
    }
}

fn preference_algorithms_mut(
    preferences: &mut SshAlgorithmPreferences,
    category: SshAlgorithmCategory,
) -> &mut Vec<String> {
    match category {
        SshAlgorithmCategory::Kex => &mut preferences.kex,
        SshAlgorithmCategory::HostKey => &mut preferences.host_key,
        SshAlgorithmCategory::Cipher => &mut preferences.cipher,
        SshAlgorithmCategory::Mac => &mut preferences.mac,
        SshAlgorithmCategory::Compression => &mut preferences.compression,
    }
}

fn customized_category_count(preferences: &SshAlgorithmPreferences) -> usize {
    SshAlgorithmCategory::ALL
        .into_iter()
        .filter(|category| !preference_algorithms(preferences, *category).is_empty())
        .count()
}

fn baseline_algorithms(legacy_compatibility: bool, category: SshAlgorithmCategory) -> Vec<String> {
    let report = oxideterm_ssh::ssh_capability_report();
    let offer = if legacy_compatibility {
        &report.legacy_compatibility_offer
    } else {
        &report.default_offer
    };
    offer_algorithms(offer, category).to_vec()
}

fn available_algorithms(
    report: &oxideterm_ssh::SshCapabilityReport,
    category: SshAlgorithmCategory,
    enabled: &[String],
) -> Vec<String> {
    let mut available = Vec::new();
    for algorithm in offer_algorithms(&report.legacy_compatibility_offer, category)
        .iter()
        .chain(offer_algorithms(&report.default_offer, category))
    {
        if !enabled.contains(algorithm) && !available.contains(algorithm) {
            available.push(algorithm.clone());
        }
    }
    available
}

#[derive(Clone)]
struct AlgorithmDrag {
    category: SshAlgorithmCategory,
    name: String,
    background: gpui::Rgba,
    foreground: gpui::Rgba,
}

impl Render for AlgorithmDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .bg(self.background)
            .text_color(self.foreground)
            .text_size(px(12.0))
            .child(self.name.clone())
    }
}

fn reorder_algorithm(algorithms: &mut Vec<String>, name: &str, destination: usize) {
    let Some(index) = algorithms.iter().position(|entry| entry == name) else {
        return;
    };
    if destination >= algorithms.len() || destination == index {
        return;
    }
    let algorithm = algorithms.remove(index);
    algorithms.insert(destination, algorithm);
}

impl WorkspaceApp {
    pub(super) fn render_ssh_algorithms_navigation_row(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((open, preferences)) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| (form.ssh_algorithm_editor_open, form.ssh_algorithms.clone()))
        else {
            return div().into_any_element();
        };
        let customized = customized_category_count(&preferences);
        let status = if customized == 0 {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        } else {
            self.i18n
                .t("ssh.form.ssh_algorithms_custom_count")
                .replace("{{count}}", &customized.to_string())
        };

        div()
            .id("new-connection-ssh-algorithms")
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.spacing.three))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if open {
                rgb(self.tokens.ui.accent)
            } else {
                rgb(self.tokens.ui.border)
            })
            .bg(if open {
                rgba((self.tokens.ui.accent << 8) | 0x14)
            } else {
                rgba(0x00000000)
            })
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("ssh.form.ssh_algorithms")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("ssh.form.ssh_algorithms_hint")),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .child(status_pill(
                        &self.tokens,
                        status,
                        StatusPillOptions::new(if customized == 0 {
                            StatusTone::Neutral
                        } else {
                            StatusTone::Accent
                        })
                        .compact(),
                    ))
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.ssh_algorithm_editor_open = true;
                            form.field_focused = false;
                        }
                    });
                    this.close_new_connection_select(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_ssh_algorithm_category_column(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((selected, legacy_compatibility, preferences)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.ssh_algorithm_editor_category,
                    form.legacy_ssh_compatibility,
                    form.ssh_algorithms.clone(),
                )
            })
        else {
            return div().into_any_element();
        };

        let mut categories = div().flex().flex_col().gap(px(self.tokens.spacing.one));
        for (category_index, category) in SshAlgorithmCategory::ALL.into_iter().enumerate() {
            let active = selected == category;
            let custom = preference_algorithms(&preferences, category);
            let count = if custom.is_empty() {
                baseline_algorithms(legacy_compatibility, category).len()
            } else {
                custom.len()
            };
            let status = if custom.is_empty() {
                self.i18n.t("ssh.form.ssh_algorithms_default")
            } else {
                self.i18n.t("ssh.form.ssh_algorithms_custom")
            };
            categories = categories.child(
                div()
                    .id(("ssh-algorithm-category", category_index))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(self.tokens.spacing.two))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.two))
                    .cursor_pointer()
                    .bg(if active {
                        rgba((self.tokens.ui.accent << 8) | 0x22)
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(rgb(if active {
                        self.tokens.ui.text
                    } else {
                        self.tokens.ui.text_secondary
                    }))
                    .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.spacing.one))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(self.i18n.t(category_i18n_key(category))),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(format!("{status} · {count}")),
                            ),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ssh_algorithm_editor_category = category;
                                    form.ssh_algorithm_selected = None;
                                    form.ssh_algorithm_menu = None;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        div()
            .w(px(SSH_ALGORITHM_CATEGORY_COLUMN_WIDTH))
            .h_full()
            .min_h(px(0.0))
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(self.tokens.ui.border))
            .pl(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .pb(px(self.tokens.spacing.three))
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("ssh.form.ssh_algorithms_categories")),
            )
            .child(categories)
            .into_any_element()
    }

    pub(super) fn render_ssh_algorithm_detail_column(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return div().into_any_element();
        };
        let category = form.ssh_algorithm_editor_category;
        let custom = preference_algorithms(&form.ssh_algorithms, category);
        let inherited = custom.is_empty();
        let enabled = if inherited {
            baseline_algorithms(form.legacy_ssh_compatibility, category)
        } else {
            custom.to_vec()
        };
        let selected = form.ssh_algorithm_selected.clone();
        let report = oxideterm_ssh::ssh_capability_report();
        let available = available_algorithms(&report, category, &enabled);
        let modern = offer_algorithms(&report.default_offer, category);
        let mut list = div().flex().flex_col();
        for (index, name) in enabled.iter().chain(available.iter()).enumerate() {
            let checked = index < enabled.len();
            if index == enabled.len() && !available.is_empty() {
                list = list.child(
                    div()
                        .pt_3()
                        .pb_1()
                        .text_size(px(12.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("ssh.form.ssh_algorithms_disabled")),
                );
            }
            let disabled = inherited || (checked && enabled.len() == 1);
            let toggle_name = name.clone();
            let select_name = name.clone();
            let menu_name = name.clone();
            let drop_name = name.clone();
            let drag = AlgorithmDrag {
                category,
                name: name.clone(),
                background: rgb(self.tokens.ui.bg_panel),
                foreground: rgb(self.tokens.ui.text),
            };
            let accent = self.tokens.ui.accent;
            let mut row = div()
                .id(("ssh-algorithm-row", index))
                .min_h(px(30.0))
                .flex()
                .items_center()
                .gap_2()
                .px_1()
                .py_1()
                .border_b_1()
                .border_color(rgb(self.tokens.ui.border))
                .when(selected.as_ref() == Some(name), |row| {
                    row.bg(rgb(self.tokens.ui.bg_hover))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.ssh_algorithm_selected = Some(select_name.clone());
                                form.field_focused = false;
                            }
                        });
                        window.focus(&this.focus_handle, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .id(("ssh-algorithm-grip", index))
                        .flex_none()
                        .w(px(18.0))
                        .cursor(if checked && !inherited {
                            gpui::CursorStyle::OpenHand
                        } else {
                            gpui::CursorStyle::Arrow
                        })
                        .child(div().w(px(6.0)).flex().flex_wrap().gap(px(2.0)).children(
                            (0..6).map(|_| div().size(px(2.0)).bg(rgb(self.tokens.ui.text_muted))),
                        ))
                        .when(checked && !inherited, |grip| {
                            grip.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                        }),
                )
                .child(
                    oxideterm_gpui_ui::checkbox::checkbox_with(
                        &self.tokens,
                        String::new(),
                        checked,
                        oxideterm_gpui_ui::checkbox::CheckboxOptions {
                            disabled,
                            ..Default::default()
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if !disabled {
                                if checked {
                                    this.remove_ssh_algorithm(category, toggle_name.clone(), cx);
                                } else {
                                    this.add_ssh_algorithm(category, toggle_name.clone(), cx);
                                }
                            }
                            cx.stop_propagation();
                        }),
                    ),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(px(12.0))
                        .text_color(rgb(if checked {
                            self.tokens.ui.text
                        } else {
                            self.tokens.ui.text_muted
                        }))
                        .child(name.clone()),
                )
                .when(!modern.contains(name), |row| {
                    row.child(status_pill(
                        &self.tokens,
                        self.i18n.t("ssh.form.ssh_algorithms_legacy"),
                        StatusPillOptions::new(StatusTone::Warning).compact(),
                    ))
                });
            if checked && !inherited {
                row = row
                    .drag_over::<AlgorithmDrag>(move |row, drag, _, _| {
                        if drag.category == category {
                            row.bg(rgba((accent << 8) | 0x22))
                        } else {
                            row
                        }
                    })
                    .on_drop(cx.listener(move |this, drag: &AlgorithmDrag, _, cx| {
                        if drag.category != category {
                            return;
                        }
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                let algorithms =
                                    preference_algorithms_mut(&mut form.ssh_algorithms, category);
                                if let Some(destination) =
                                    algorithms.iter().position(|name| name == &drop_name)
                                {
                                    reorder_algorithm(algorithms, &drag.name, destination);
                                }
                                form.ssh_algorithm_selected = Some(drag.name.clone());
                            }
                        });
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ssh_algorithm_menu =
                                        Some((menu_name.clone(), event.position));
                                    form.ssh_algorithm_selected = Some(menu_name.clone());
                                    form.field_focused = false;
                                }
                            });
                            window.focus(&this.focus_handle, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    );
            }
            list = list.child(row);
        }
        div()
            .w(px(SSH_ALGORITHM_DETAIL_COLUMN_WIDTH))
            .h_full()
            .min_h_0()
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(self.tokens.ui.border))
            .px(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.i18n.t(category_i18n_key(category))),
                    )
                    .child(self.ssh_algorithm_icon_action(
                        self.i18n.t("ssh.form.ssh_algorithms_close"),
                        LucideIcon::X,
                        false,
                        cx.listener(|this, _, _, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ssh_algorithm_editor_open = false;
                                    form.ssh_algorithm_menu = None;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .pb_2()
                    .children([true, false].into_iter().map(|follow| {
                        action_chip(
                            &self.tokens,
                            self.i18n.t(if follow {
                                "ssh.form.ssh_algorithms_follow_preset"
                            } else {
                                "ssh.form.ssh_algorithms_custom"
                            }),
                            None,
                            ActionChipOptions::new().active(inherited == follow),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        let baseline = baseline_algorithms(
                                            form.legacy_ssh_compatibility,
                                            category,
                                        );
                                        let algorithms = preference_algorithms_mut(
                                            &mut form.ssh_algorithms,
                                            category,
                                        );
                                        if follow {
                                            algorithms.clear();
                                        } else if algorithms.is_empty() {
                                            *algorithms = baseline;
                                        }
                                        form.field_focused = false;
                                        form.ssh_algorithm_menu = None;
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                    })),
            )
            .child(
                div()
                    .pb_2()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(if inherited {
                        "ssh.form.ssh_algorithms_inherited_hint"
                    } else {
                        "ssh.form.ssh_algorithms_edit_hint"
                    })),
            )
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(list))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ssh_algorithm_context_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use oxideterm_gpui_ui::context_menu::*;
        let form_state = self.connection_form_state(cx);
        let form = form_state.form.as_ref()?;
        if !form.ssh_algorithm_editor_open {
            return None;
        }
        let (name, position) = form.ssh_algorithm_menu.clone()?;
        let category = form.ssh_algorithm_editor_category;
        let algorithms = preference_algorithms(&form.ssh_algorithms, category);
        let index = algorithms.iter().position(|algorithm| algorithm == &name)?;
        let mut menu = context_menu_content(&self.tokens);
        for (offset, key, disabled) in [
            (-1, "ssh.form.ssh_algorithms_move_up", index == 0),
            (
                1,
                "ssh.form.ssh_algorithms_move_down",
                index + 1 == algorithms.len(),
            ),
        ] {
            let name = name.clone();
            menu = menu.child(context_menu_action(
                context_menu_item(
                    &self.tokens,
                    self.i18n.t(key),
                    ContextMenuItemKind::Plain,
                    false,
                    disabled,
                ),
                disabled,
                false,
                cx.listener(move |this, _, _, cx| {
                    this.move_ssh_algorithm(category, name.clone(), offset, cx);
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.ssh_algorithm_menu = None;
                        }
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        Some(
            gpui::deferred(
                context_menu_backdrop()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ssh_algorithm_menu = None;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(
                        gpui::anchored()
                            .position(position)
                            .child(context_menu_event_boundary(menu)),
                    ),
            )
            .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
            .into_any_element(),
        )
    }

    pub(super) fn handle_ssh_algorithm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return false;
        };
        if !form.ssh_algorithm_editor_open || form.field_focused {
            return false;
        }
        let category = form.ssh_algorithm_editor_category;
        let inherited = preference_algorithms(&form.ssh_algorithms, category).is_empty();
        let enabled = if inherited {
            baseline_algorithms(form.legacy_ssh_compatibility, category)
        } else {
            preference_algorithms(&form.ssh_algorithms, category).to_vec()
        };
        let report = oxideterm_ssh::ssh_capability_report();
        let mut names = enabled.clone();
        names.extend(available_algorithms(&report, category, &enabled));
        let index = form
            .ssh_algorithm_selected
            .as_ref()
            .and_then(|name| names.iter().position(|entry| entry == name))
            .unwrap_or(0);
        let menu_open = form.ssh_algorithm_menu.is_some();
        match event.keystroke.key.as_str() {
            "escape" => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        if menu_open {
                            form.ssh_algorithm_menu = None;
                        } else {
                            form.ssh_algorithm_editor_open = false;
                        }
                    }
                });
            }
            "up" | "down" => {
                let offset = if event.keystroke.key == "up" { -1 } else { 1 };
                if event.keystroke.modifiers.alt && !inherited {
                    if let Some(name) = names.get(index) {
                        self.move_ssh_algorithm(category, name.clone(), offset, cx);
                    }
                } else if !names.is_empty() {
                    let next =
                        (index as isize + offset).clamp(0, names.len() as isize - 1) as usize;
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.ssh_algorithm_selected = Some(names[next].clone());
                        }
                    });
                }
            }
            "space" if !inherited => {
                if let Some(name) = names.get(index) {
                    if enabled.contains(name) {
                        self.remove_ssh_algorithm(category, name.clone(), cx);
                    } else {
                        self.add_ssh_algorithm(category, name.clone(), cx);
                    }
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    fn ssh_algorithm_icon_action(
        &self,
        label: String,
        icon: LucideIcon,
        disabled: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                13.0,
                rgb(self.tokens.ui.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    radius: ButtonRadius::Sm,
                    disabled,
                },
                show_label: false,
                height: Some(26.0),
                min_width: Some(26.0),
                padding_x: Some(0.0),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
        .into_any_element()
    }

    fn move_ssh_algorithm(
        &mut self,
        category: SshAlgorithmCategory,
        algorithm: String,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let baseline = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| baseline_algorithms(form.legacy_ssh_compatibility, category))
            .unwrap_or_default();
        self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return;
            };
            let selected = preference_algorithms_mut(&mut form.ssh_algorithms, category);
            if selected.is_empty() {
                *selected = baseline;
            }
            let Some(index) = selected.iter().position(|name| name == &algorithm) else {
                return;
            };
            let target = index as isize + offset;
            if target >= 0 && (target as usize) < selected.len() {
                selected.swap(index, target as usize);
            }
        });
        cx.notify();
    }

    fn remove_ssh_algorithm(
        &mut self,
        category: SshAlgorithmCategory,
        algorithm: String,
        cx: &mut Context<Self>,
    ) {
        let baseline = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| baseline_algorithms(form.legacy_ssh_compatibility, category))
            .unwrap_or_default();
        self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return;
            };
            let selected = preference_algorithms_mut(&mut form.ssh_algorithms, category);
            if selected.is_empty() {
                *selected = baseline;
            }
            if selected.len() > 1 {
                selected.retain(|name| name != &algorithm);
            }
        });
        cx.notify();
    }

    fn add_ssh_algorithm(
        &mut self,
        category: SshAlgorithmCategory,
        algorithm: String,
        cx: &mut Context<Self>,
    ) {
        let baseline = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| baseline_algorithms(form.legacy_ssh_compatibility, category))
            .unwrap_or_default();
        self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return;
            };
            let selected = preference_algorithms_mut(&mut form.ssh_algorithms, category);
            if selected.is_empty() {
                *selected = baseline;
            }
            if !selected.contains(&algorithm) {
                selected.push(algorithm);
            }
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::reorder_algorithm;

    #[test]
    fn drag_reordering_moves_one_algorithm_without_swapping_neighbors() {
        let mut algorithms = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        reorder_algorithm(&mut algorithms, "a", 2);
        assert_eq!(algorithms, ["b", "c", "a", "d"]);
        reorder_algorithm(&mut algorithms, "d", 0);
        assert_eq!(algorithms, ["d", "b", "c", "a"]);
    }
}
