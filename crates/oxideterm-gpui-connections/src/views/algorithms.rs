use gpui::{
    AnyElement, App, AppContext, Context, Entity, KeyDownEvent, MouseButton, MouseDownEvent,
    Render, SharedString, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_connections::SshAlgorithmPreferences;
use oxideterm_gpui_ui::{
    StatusPillOptions, StatusTone,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_action_button,
    },
    segmented_tab, segmented_tabs, status_pill,
};
use oxideterm_i18n::I18n;
use oxideterm_ssh::{SshAlgorithmCategory, SshAlgorithmOffer};
use oxideterm_theme::ThemeTokens;

use super::{ConnectionsDispatch, ConnectionsWindowRequest};
use crate::{ConnectionFormState, entity::ConnectionFlowEntity, form_state::ConnectionFormDrawer};

fn render_lucide_icon(icon_path: &'static str, size: f32, color: gpui::Rgba) -> AnyElement {
    svg()
        .path(icon_path)
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

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
    border: gpui::Rgba,
    foreground: gpui::Rgba,
    mono_font: SharedString,
}

impl Render for AlgorithmDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(self.border)
            .bg(self.background)
            .font_family(self.mono_font.clone())
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

pub struct SshAlgorithmEditor<'a> {
    flow: &'a Entity<ConnectionFlowEntity>,
    tokens: &'a ThemeTokens,
    i18n: &'a I18n,
    dispatch: &'a ConnectionsDispatch,
}

impl<'a> SshAlgorithmEditor<'a> {
    pub(super) fn new(
        flow: &'a Entity<ConnectionFlowEntity>,
        tokens: &'a ThemeTokens,
        i18n: &'a I18n,
        dispatch: &'a ConnectionsDispatch,
    ) -> Self {
        Self {
            flow,
            tokens,
            i18n,
            dispatch,
        }
    }

    fn form_state<'app>(&self, cx: &'app App) -> &'app ConnectionFormState {
        &self.flow.read(cx).form
    }

    fn listener<E: 'static, F>(
        &self,
        update: F,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static + use<E, F>
    where
        F: Fn(&mut ConnectionFlowEntity, &E, &mut Window, &mut Context<ConnectionFlowEntity>)
            + 'static,
    {
        let flow = self.flow.downgrade();
        move |event, window, cx| {
            let _ = flow.update(cx, |flow, cx| update(flow, event, window, cx));
        }
    }

    fn focus_listener<F>(
        &self,
        update: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static + use<F>
    where
        F: Fn(
                &mut ConnectionFlowEntity,
                &MouseDownEvent,
                &mut Window,
                &mut Context<ConnectionFlowEntity>,
            ) + 'static,
    {
        let listen = self.listener(update);
        let dispatch = self.dispatch.clone();
        move |event, window, cx| {
            listen(event, window, cx);
            dispatch(ConnectionsWindowRequest::FocusForm, Some(window), cx);
        }
    }

    /// Unified inline SSH algorithms editor rendered inside the studio's right pane.
    pub fn render_ssh_algorithms_page(&self, cx: &App) -> AnyElement {
        let Some(form) = self.form_state(cx).form.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let category = form.ssh_algorithm_editor_category;
        let preferences = form.ssh_algorithms.clone();
        let legacy_compatibility = form.legacy_ssh_compatibility;
        let custom = preference_algorithms(&preferences, category);
        let inherited = custom.is_empty();
        let enabled = if inherited {
            baseline_algorithms(legacy_compatibility, category)
        } else {
            custom.to_vec()
        };
        let selected = form.ssh_algorithm_selected.clone();
        let report = oxideterm_ssh::ssh_capability_report();
        let available = available_algorithms(&report, category, &enabled);
        let modern = offer_algorithms(&report.default_offer, category);
        let mono_font = SharedString::from(self.tokens.metrics.markdown_code_font_family);
        let row_divider = rgba((theme.border << 8) | 0x45);

        // Top 5-category segmented strip keeps all algorithm groups one click away
        // without expanding the modal width.
        let mut category_tabs = div()
            .w_full()
            .p(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_sunken << 8) | 0xc8))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0));
        for (cat_idx, cat) in SshAlgorithmCategory::ALL.into_iter().enumerate() {
            let active = cat == category;
            let is_custom = !preference_algorithms(&preferences, cat).is_empty();
            category_tabs = category_tabs.child(
                div()
                    .id(("ssh-algorithm-category", cat_idx))
                    .flex_1()
                    .min_w_0()
                    .h(px(28.0))
                    .px(px(6.0))
                    .rounded(px(self.tokens.radii.sm))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.0))
                    .when(active, |tab| {
                        tab.bg(rgb(theme.bg_card))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0xc0))
                    })
                    .when(!active, |tab| {
                        tab.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(if active {
                                gpui::FontWeight::MEDIUM
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .text_color(rgb(if active {
                                theme.text_heading
                            } else {
                                theme.text_muted
                            }))
                            .child(self.i18n.t(category_i18n_key(cat))),
                    )
                    .when(is_custom, |tab| {
                        tab.child(
                            div()
                                .flex_none()
                                .size(px(5.0))
                                .rounded_full()
                                .bg(rgb(theme.accent)),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        self.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.form.form.as_mut() {
                                form.ssh_algorithm_editor_category = cat;
                                form.ssh_algorithm_selected = None;
                                form.ssh_algorithm_menu = None;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        // Compact mode toggle bar: Follow Preset vs Custom. Any direct edit on a row
        // automatically promotes the category to Custom, so users never hit dead controls.
        let mode_toggle = segmented_tabs(self.tokens)
            .h(px(self.tokens.metrics.ui_tabs_list_height + 2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_sunken << 8) | 0xc8))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .p(px(2.0))
            .children([true, false].into_iter().map(|follow| {
                let selected = inherited == follow;
                segmented_tab(
                    self.tokens,
                    self.i18n.t(if follow {
                        "ssh.form.ssh_algorithms_follow_preset"
                    } else {
                        "ssh.form.ssh_algorithms_custom"
                    }),
                    selected,
                )
                .id(SharedString::from(if follow {
                    "ssh-algorithm-mode-preset"
                } else {
                    "ssh-algorithm-mode-custom"
                }))
                .h_full()
                .min_h_0()
                .py_0()
                .rounded(px(self.tokens.radii.sm))
                .bg(if selected {
                    rgb(theme.bg_card)
                } else {
                    rgba(0x00000000)
                })
                .when(selected, |el| {
                    el.border_1().border_color(rgba((theme.border << 8) | 0xc0))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    self.listener(move |this, _, _, cx| {
                        if let Some(form) = this.form.form.as_mut() {
                            let baseline =
                                baseline_algorithms(form.legacy_ssh_compatibility, category);
                            let algorithms =
                                preference_algorithms_mut(&mut form.ssh_algorithms, category);
                            if follow {
                                algorithms.clear();
                            } else if algorithms.is_empty() {
                                *algorithms = baseline;
                            }
                            form.field_focused = false;
                            form.ssh_algorithm_menu = None;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
            }));

        let toolbar = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(if inherited {
                        "ssh.form.ssh_algorithms_inherited_hint"
                    } else {
                        "ssh.form.ssh_algorithms_edit_hint"
                    })),
            )
            .child(div().flex_none().w(px(184.0)).child(mode_toggle));

        let mut list = div().flex().flex_col();
        let enabled_len = enabled.len();
        for (index, name) in enabled.iter().chain(available.iter()).enumerate() {
            let checked = index < enabled_len;
            if index == enabled_len && !available.is_empty() {
                list = list.child(
                    div()
                        .pt(px(14.0))
                        .pb(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text_muted))
                                .child(format!(
                                    "{} · {}",
                                    self.i18n.t("ssh.form.ssh_algorithms_disabled"),
                                    available.len()
                                )),
                        )
                        .child(div().flex_1().h(px(1.0)).bg(row_divider)),
                );
            }

            // Never allow removing the sole remaining enabled algorithm.
            let cannot_disable = checked && enabled_len == 1;
            let toggle_name = name.clone();
            let menu_name = name.clone();
            let drop_name = name.clone();
            let move_up_name = name.clone();
            let move_down_name = name.clone();
            let is_selected = selected.as_ref() == Some(name);
            let drag = AlgorithmDrag {
                category,
                name: name.clone(),
                background: rgb(theme.bg_elevated),
                border: rgb(theme.accent),
                foreground: rgb(theme.text),
                mono_font: mono_font.clone(),
            };
            let accent = theme.accent;
            let can_move_up = checked && index > 0;
            let can_move_down = checked && index + 1 < enabled_len;

            let mut row = div()
                .id(("ssh-algorithm-row", index))
                .min_h(px(34.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(self.tokens.radii.sm))
                .flex()
                .items_center()
                .gap(px(8.0))
                .cursor(if cannot_disable {
                    gpui::CursorStyle::Arrow
                } else {
                    gpui::CursorStyle::PointingHand
                })
                .when(is_selected, |row| row.bg(rgba((theme.accent << 8) | 0x16)))
                .when(!is_selected, |row| {
                    row.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0xaa)))
                })
                .border_b_1()
                .border_color(
                    if index + 1 == enabled_len || index + 1 == enabled_len + available.len() {
                        rgba(0x00000000)
                    } else {
                        row_divider
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    self.focus_listener(move |this, _, _window, cx| {
                        if let Some(form) = this.form.form.as_mut() {
                            form.ssh_algorithm_selected = Some(toggle_name.clone());
                            form.ssh_algorithm_menu = None;
                            form.field_focused = false;
                        }
                        if !cannot_disable {
                            if checked {
                                this.remove_ssh_algorithm(category, toggle_name.clone(), cx);
                            } else {
                                this.add_ssh_algorithm(category, toggle_name.clone(), cx);
                            }
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(if checked { 0.48 } else { 0.16 })
                        .child(div().w(px(6.0)).flex().flex_wrap().gap(px(2.0)).children(
                            (0..6).map(|_| div().size(px(2.0)).bg(rgb(theme.text_muted))),
                        )),
                )
                .child(
                    // Soften inherited preset checkmarks so default rows stay calm
                    // until the user explicitly switches to or edits Custom mode.
                    div().when(inherited, |el| el.opacity(0.48)).child(
                        oxideterm_gpui_ui::checkbox::checkbox_with(
                            self.tokens,
                            format!("ssh-algorithm-{category:?}-{name}"),
                            String::new(),
                            checked,
                            oxideterm_gpui_ui::checkbox::CheckboxOptions {
                                disabled: cannot_disable,
                                ..Default::default()
                            },
                        ),
                    ),
                )
                .when(checked, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .w(px(20.0))
                            .font_family(mono_font.clone())
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{:02}", index + 1)),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .font_family(mono_font.clone())
                        .text_size(px(12.5))
                        .text_color(rgb(if checked {
                            theme.text
                        } else {
                            theme.text_muted
                        }))
                        .child(name.clone()),
                )
                .when(!modern.contains(name), |row| {
                    row.child(status_pill(
                        self.tokens,
                        self.i18n.t("ssh.form.ssh_algorithms_legacy"),
                        StatusPillOptions::new(StatusTone::Warning).compact(),
                    ))
                })
                .when(checked && enabled_len > 1, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(self.ssh_algorithm_icon_action(
                                self.i18n.t("ssh.form.ssh_algorithms_move_up"),
                                "lucide/arrow-up.svg",
                                !can_move_up,
                                self.focus_listener(move |this, _, _, cx| {
                                    if can_move_up {
                                        this.move_ssh_algorithm(
                                            category,
                                            move_up_name.clone(),
                                            -1,
                                            cx,
                                        );
                                        if let Some(form) = this.form.form.as_mut() {
                                            form.ssh_algorithm_selected =
                                                Some(move_up_name.clone());
                                        }
                                    }
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.ssh_algorithm_icon_action(
                                self.i18n.t("ssh.form.ssh_algorithms_move_down"),
                                "lucide/arrow-down.svg",
                                !can_move_down,
                                self.focus_listener(move |this, _, _, cx| {
                                    if can_move_down {
                                        this.move_ssh_algorithm(
                                            category,
                                            move_down_name.clone(),
                                            1,
                                            cx,
                                        );
                                        if let Some(form) = this.form.form.as_mut() {
                                            form.ssh_algorithm_selected =
                                                Some(move_down_name.clone());
                                        }
                                    }
                                    cx.stop_propagation();
                                }),
                            )),
                    )
                });

            if checked && enabled_len > 1 {
                row = row
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .drag_over::<AlgorithmDrag>(move |row, drag, _, _| {
                        if drag.category == category {
                            row.border_t_2()
                                .border_color(rgb(accent))
                                .bg(rgba((accent << 8) | 0x14))
                        } else {
                            row
                        }
                    })
                    .on_drop(self.listener(move |this, drag: &AlgorithmDrag, _, cx| {
                        if drag.category != category {
                            return;
                        }
                        if let Some(form) = this.form.form.as_mut() {
                            let baseline =
                                baseline_algorithms(form.legacy_ssh_compatibility, category);
                            let algorithms =
                                preference_algorithms_mut(&mut form.ssh_algorithms, category);
                            if algorithms.is_empty() {
                                *algorithms = baseline;
                            }
                            if let Some(destination) =
                                algorithms.iter().position(|entry| entry == &drop_name)
                            {
                                reorder_algorithm(algorithms, &drag.name, destination);
                            }
                            form.ssh_algorithm_selected = Some(drag.name.clone());
                        }
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        self.focus_listener(
                            move |this, event: &gpui::MouseDownEvent, _window, cx| {
                                if let Some(form) = this.form.form.as_mut() {
                                    form.ssh_algorithm_menu =
                                        Some((menu_name.clone(), event.position));
                                    form.ssh_algorithm_selected = Some(menu_name.clone());
                                    form.field_focused = false;
                                }
                                cx.stop_propagation();
                                cx.notify();
                            },
                        ),
                    );
            }
            list = list.child(row);
        }

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(category_tabs)
            .child(toolbar)
            .child(div().h(px(1.0)).bg(rgba((theme.border << 8) | 0x55)))
            .child(list)
            .into_any_element()
    }

    pub fn render_ssh_algorithm_context_menu(&self, cx: &App) -> Option<AnyElement> {
        use oxideterm_gpui_ui::context_menu::*;
        let form_state = self.form_state(cx);
        let form = form_state.form.as_ref()?;
        if form.active_drawer != Some(ConnectionFormDrawer::SshAlgorithms) {
            return None;
        }
        let (name, position) = form.ssh_algorithm_menu.clone()?;
        let category = form.ssh_algorithm_editor_category;
        let custom = preference_algorithms(&form.ssh_algorithms, category);
        let algorithms = if custom.is_empty() {
            baseline_algorithms(form.legacy_ssh_compatibility, category)
        } else {
            custom.to_vec()
        };
        let index = algorithms.iter().position(|algorithm| algorithm == &name)?;
        let mut menu = context_menu_content(self.tokens);
        for (offset, icon_path, key, disabled) in [
            (
                -1,
                "lucide/arrow-up.svg",
                "ssh.form.ssh_algorithms_move_up",
                index == 0,
            ),
            (
                1,
                "lucide/arrow-down.svg",
                "ssh.form.ssh_algorithms_move_down",
                index + 1 == algorithms.len(),
            ),
        ] {
            let name = name.clone();
            menu = menu.child(context_menu_action(
                context_menu_item_with_icon(
                    self.tokens,
                    icon_path,
                    self.i18n.t(key),
                    false,
                    disabled,
                ),
                disabled,
                false,
                self.listener(move |this, _, _, cx| {
                    this.move_ssh_algorithm(category, name.clone(), offset, cx);
                    if let Some(form) = this.form.form.as_mut() {
                        form.ssh_algorithm_menu = None;
                    }
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
                        self.listener(|this, _, _, cx| {
                            if let Some(form) = this.form.form.as_mut() {
                                form.ssh_algorithm_menu = None;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        self.listener(|this, _, _, cx| {
                            if let Some(form) = this.form.form.as_mut() {
                                form.ssh_algorithm_menu = None;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(gpui::anchored().position(position).child(
                        oxideterm_gpui_ui::motion::popover_zoom_enter(
                            self.tokens,
                            "ssh-algorithm-menu-motion",
                            div().child(context_menu_event_boundary(menu)),
                            -4.0,
                            gpui::point(0.0, 0.0),
                        ),
                    )),
            )
            .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
            .into_any_element(),
        )
    }

    fn ssh_algorithm_icon_action(
        &self,
        label: String,
        icon: &'static str,
        disabled: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        toolbar_action_button(
            self.tokens,
            label,
            Some(render_lucide_icon(
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
                height: Some(24.0),
                min_width: Some(24.0),
                padding_x: Some(0.0),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
        .into_any_element()
    }
}

impl ConnectionFlowEntity {
    pub fn handle_ssh_algorithm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.form.form.as_ref() else {
            return false;
        };
        if form.active_drawer != Some(ConnectionFormDrawer::SshAlgorithms) || form.field_focused {
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
            "escape" if menu_open => {
                if let Some(form) = self.form.form.as_mut() {
                    form.ssh_algorithm_menu = None;
                }
            }
            "up" | "down" => {
                let offset = if event.keystroke.key == "up" { -1 } else { 1 };
                if event.keystroke.modifiers.alt {
                    if let Some(name) = names.get(index) {
                        self.move_ssh_algorithm(category, name.clone(), offset, cx);
                    }
                } else if !names.is_empty() {
                    let next =
                        (index as isize + offset).clamp(0, names.len() as isize - 1) as usize;
                    if let Some(form) = self.form.form.as_mut() {
                        form.ssh_algorithm_selected = Some(names[next].clone());
                    }
                }
            }
            "space" => {
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

    fn move_ssh_algorithm(
        &mut self,
        category: SshAlgorithmCategory,
        algorithm: String,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let baseline = self
            .form
            .form
            .as_ref()
            .map(|form| baseline_algorithms(form.legacy_ssh_compatibility, category))
            .unwrap_or_default();
        {
            let Some(form) = self.form.form.as_mut() else {
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
        }
        cx.notify();
    }

    fn remove_ssh_algorithm(
        &mut self,
        category: SshAlgorithmCategory,
        algorithm: String,
        cx: &mut Context<Self>,
    ) {
        let baseline = self
            .form
            .form
            .as_ref()
            .map(|form| baseline_algorithms(form.legacy_ssh_compatibility, category))
            .unwrap_or_default();
        {
            let Some(form) = self.form.form.as_mut() else {
                return;
            };
            let selected = preference_algorithms_mut(&mut form.ssh_algorithms, category);
            if selected.is_empty() {
                *selected = baseline;
            }
            if selected.len() > 1 {
                selected.retain(|name| name != &algorithm);
            }
        }
        cx.notify();
    }

    fn add_ssh_algorithm(
        &mut self,
        category: SshAlgorithmCategory,
        algorithm: String,
        cx: &mut Context<Self>,
    ) {
        let baseline = self
            .form
            .form
            .as_ref()
            .map(|form| baseline_algorithms(form.legacy_ssh_compatibility, category))
            .unwrap_or_default();
        {
            let Some(form) = self.form.form.as_mut() else {
                return;
            };
            let selected = preference_algorithms_mut(&mut form.ssh_algorithms, category);
            if selected.is_empty() {
                *selected = baseline;
            }
            if !selected.contains(&algorithm) {
                selected.push(algorithm);
            }
        }
        cx.notify();
    }
}
