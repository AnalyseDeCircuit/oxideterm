use super::*;

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
        let Some((category, legacy_compatibility, preferences)) =
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
        let report = oxideterm_ssh::ssh_capability_report();
        let custom = preference_algorithms(&preferences, category);
        let inherited = custom.is_empty();
        let enabled = if inherited {
            baseline_algorithms(legacy_compatibility, category)
        } else {
            custom.to_vec()
        };
        let available = available_algorithms(&report, category, &enabled);
        let modern = offer_algorithms(&report.default_offer, category);

        let mut enabled_list = div().flex().flex_col().gap(px(self.tokens.spacing.one));
        let enabled_count = enabled.len();
        for (index, algorithm) in enabled.iter().enumerate() {
            let algorithm_for_up = algorithm.clone();
            let algorithm_for_down = algorithm.clone();
            let algorithm_for_remove = algorithm.clone();
            let weak = !modern.contains(algorithm);
            enabled_list = enabled_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.two))
                    .child(
                        div()
                            .w(px(22.0))
                            .flex_none()
                            .text_align(gpui::TextAlign::Center)
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(algorithm.clone()),
                    )
                    .when(weak, |row| {
                        row.child(status_pill(
                            &self.tokens,
                            self.i18n.t("ssh.form.ssh_algorithms_legacy"),
                            StatusPillOptions::new(StatusTone::Warning).compact(),
                        ))
                    })
                    .child(self.ssh_algorithm_icon_action(
                        self.i18n.t("ssh.form.ssh_algorithms_move_up"),
                        LucideIcon::ArrowUp,
                        index == 0,
                        cx.listener(move |this, _event, _window, cx| {
                            this.move_ssh_algorithm(category, algorithm_for_up.clone(), -1, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.ssh_algorithm_icon_action(
                        self.i18n.t("ssh.form.ssh_algorithms_move_down"),
                        LucideIcon::ArrowDown,
                        index + 1 == enabled_count,
                        cx.listener(move |this, _event, _window, cx| {
                            this.move_ssh_algorithm(category, algorithm_for_down.clone(), 1, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.ssh_algorithm_icon_action(
                        self.i18n.t("ssh.form.ssh_algorithms_remove"),
                        LucideIcon::X,
                        enabled_count <= 1,
                        cx.listener(move |this, _event, _window, cx| {
                            this.remove_ssh_algorithm(category, algorithm_for_remove.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )),
            );
        }

        let has_available = !available.is_empty();
        let mut available_list = div().flex().flex_col().gap(px(self.tokens.spacing.one));
        for algorithm in available {
            let algorithm_for_add = algorithm.clone();
            let weak = !modern.contains(&algorithm);
            available_list = available_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .rounded(px(self.tokens.radii.sm))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.two))
                    .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_secondary))
                            .child(algorithm),
                    )
                    .when(weak, |row| {
                        row.child(status_pill(
                            &self.tokens,
                            self.i18n.t("ssh.form.ssh_algorithms_legacy"),
                            StatusPillOptions::new(StatusTone::Warning).compact(),
                        ))
                    })
                    .child(self.ssh_algorithm_icon_action(
                        self.i18n.t("ssh.form.ssh_algorithms_add"),
                        LucideIcon::Plus,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.add_ssh_algorithm(category, algorithm_for_add.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )),
            );
        }

        div()
            .w(px(SSH_ALGORITHM_DETAIL_COLUMN_WIDTH))
            .h_full()
            .min_h(px(0.0))
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(self.tokens.ui.border))
            .pl(px(self.tokens.metrics.modal_section_gap))
            .pr(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(self.tokens.spacing.three))
                    .pb(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.spacing.one))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t(category_i18n_key(category))),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t(if inherited {
                                        "ssh.form.ssh_algorithms_inherited_hint"
                                    } else {
                                        "ssh.form.ssh_algorithms_custom_hint"
                                    })),
                            ),
                    )
                    .child(self.ssh_algorithm_icon_action(
                        self.i18n.t("ssh.form.ssh_algorithms_close"),
                        LucideIcon::X,
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ssh_algorithm_editor_open = false;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .pr(px(self.tokens.spacing.one))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.spacing.three))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(self.i18n.t("ssh.form.ssh_algorithms_enabled")),
                                    )
                                    .child(
                                        action_chip(
                                            &self.tokens,
                                            self.i18n.t("ssh.form.ssh_algorithms_reset"),
                                            Some(Self::render_lucide_icon(
                                                LucideIcon::RotateCcw,
                                                13.0,
                                                rgb(self.tokens.ui.text_muted),
                                            )),
                                            ActionChipOptions::new().disabled(inherited),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _event, _window, cx| {
                                                if !inherited {
                                                    this.reset_ssh_algorithm_category(category, cx);
                                                }
                                                cx.stop_propagation();
                                            }),
                                        ),
                                    ),
                            )
                            .child(enabled_list)
                            .when(has_available, |content| {
                                content
                                    .child(
                                        div()
                                            .pt(px(self.tokens.spacing.two))
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(
                                                self.i18n.t("ssh.form.ssh_algorithms_available"),
                                            ),
                                    )
                                    .child(available_list)
                            })
                            .child(
                                div()
                                    .pt(px(self.tokens.spacing.two))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(
                                        self.i18n
                                            .t("ssh.form.ssh_algorithms_negotiation_order_hint"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
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

    fn reset_ssh_algorithm_category(
        &mut self,
        category: SshAlgorithmCategory,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                preference_algorithms_mut(&mut form.ssh_algorithms, category).clear();
            }
        });
        cx.notify();
    }
}
