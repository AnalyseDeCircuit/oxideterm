use super::*;
use crate::terminal_ui::{
    TerminalHighlightMatchScope, TerminalHighlightRenderMode, TerminalHighlightRule,
};
use gpui::{AnyElement, SharedString, div, prelude::*, rgb, rgba};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonSize, ButtonVariant, IconButtonOptions, button_with, icon_button,
};
use oxideterm_gpui_ui::entity_row::{EntityListRowOptions, entity_list_row};
use oxideterm_gpui_ui::modal::{
    dialog_content, dialog_footer, dialog_header, dialog_overlay, modal_body,
    overlay_content_boundary,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;

pub(super) struct TemporaryMarker {
    text: Zeroizing<String>,
    color: usize,
}
#[derive(Default)]
pub(super) struct PaneMarkers {
    items: Vec<TemporaryMarker>,
    pub open: bool,
    base: Option<Arc<[TerminalHighlightRule]>>,
    palette: [u32; 6],
    rules: Arc<Vec<crate::terminal_view::highlight::RuntimeHighlightRule>>,
    pub pending_save: Option<(Zeroizing<String>, String, String)>,
}

fn marker_foreground(background: u32, dark: u32, light: u32) -> u32 {
    let luminance = |color: u32| {
        [16, 8, 0]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(shift, weight)| {
                let channel = ((color >> shift) & 0xff) as f32 / 255.0;
                let linear = if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                };
                linear * weight
            })
            .sum::<f32>()
    };
    let background = luminance(background);
    let contrast = |color| {
        let text = luminance(color);
        (background.max(text) + 0.05) / (background.min(text) + 0.05)
    };
    if contrast(dark) >= contrast(light) {
        dark
    } else {
        light
    }
}

impl TerminalPane {
    fn marker_palette(&self) -> [u32; 6] {
        let theme = self.theme.tokens;
        [
            theme.terminal.yellow,
            theme.terminal.blue,
            theme.terminal.green,
            theme.terminal.magenta,
            theme.terminal.black,
            theme.terminal.bright_white,
        ]
    }

    fn set_temporary_marker(&mut self, text: &str, color: usize, cx: &mut Context<Self>) {
        if text.is_empty() || text.contains(['\r', '\n']) || color >= 4 {
            return;
        }
        if let Some(marker) = self
            .markers
            .items
            .iter_mut()
            .find(|marker| marker.text.as_str() == text)
        {
            marker.color = color;
        } else {
            self.markers.items.push(TemporaryMarker {
                text: Zeroizing::new(text.to_string()),
                color,
            });
        }
        self.markers.base = None;
        cx.notify();
    }

    pub(super) fn effective_reading_highlights(
        &mut self,
    ) -> Option<Arc<Vec<crate::terminal_view::highlight::RuntimeHighlightRule>>> {
        let base = &self.preferences.highlight_rules;
        if self.markers.items.is_empty() {
            if !self.markers.rules.is_empty() {
                self.markers.rules = Arc::default();
            }
            self.markers.base = None;
            return None;
        }
        let palette = self.marker_palette();
        if self
            .markers
            .base
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(cached, base))
            && self.markers.palette == palette
        {
            return Some(self.markers.rules.clone());
        }
        let mut rules = Vec::new();
        for (index, marker) in self.markers.items.iter().enumerate() {
            rules.push(TerminalHighlightRule {
                id: format!("pane-marker-{index}"),
                pattern: marker.text.to_string(),
                is_regex: false,
                case_sensitive: true,
                foreground: Some(format!(
                    "#{:06x}",
                    marker_foreground(palette[marker.color], palette[4], palette[5])
                )),
                background: Some(format!("#{:06x}", palette[marker.color])),
                render_mode: TerminalHighlightRenderMode::Background,
                match_scope: TerminalHighlightMatchScope::Match,
                preserve_background: false,
                enabled: true,
                priority: i64::MAX,
            });
        }
        self.markers.base = Some(base.clone());
        self.markers.palette = palette;
        self.markers.rules = crate::terminal_view::highlight::compile_pane_highlights(base, rules);
        Some(self.markers.rules.clone())
    }

    pub fn toggle_temporary_markers(&mut self, cx: &mut Context<Self>) {
        self.markers.open = !self.markers.open;
        cx.notify();
    }
    pub fn take_marker_rule_draft(&mut self) -> Option<(Zeroizing<String>, String, String)> {
        self.markers.pending_save.take()
    }

    pub(super) fn render_marker_colors(
        &self,
        selected: Option<Zeroizing<String>>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let labels = &self.preferences.reading_labels;
        let colors = self.marker_palette();
        let mut row = div().flex().items_center().flex_none().gap(px(self
            .theme
            .tokens
            .spacing
            .one));
        for (index, label) in [
            labels.yellow.clone(),
            labels.blue.clone(),
            labels.green.clone(),
            labels.pink.clone(),
        ]
        .into_iter()
        .enumerate()
        {
            let text = selected.clone();
            let tokens = self.theme.tokens;
            let active = selected.as_ref().is_some_and(|text| {
                self.markers
                    .items
                    .iter()
                    .any(|marker| marker.text.as_str() == text.as_str() && marker.color == index)
            });
            row = row.child(
                oxideterm_gpui_ui::button::icon_button(
                    &tokens,
                    div()
                        .size(px(tokens.metrics.ui_menu_icon_size))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(colors[index]))
                        .when(active, |swatch| {
                            swatch.child(
                                gpui::svg()
                                    .path("lucide/check.svg")
                                    .size(px(tokens.metrics.sidebar_action_icon_size))
                                    .text_color(rgb(marker_foreground(
                                        colors[index],
                                        colors[4],
                                        colors[5],
                                    ))),
                            )
                        })
                        .into_any_element(),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        border: active.then(|| rgb(tokens.ui.accent)),
                        idle_opacity: 1.0,
                        ..IconButtonOptions::compact(tokens.metrics.ui_button_sm_height)
                    },
                )
                .id(SharedString::from(format!("marker-color-{index}")))
                .debug_selector(move || format!("marker-color-{index}"))
                .tooltip(move |_, cx| {
                    oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
                })
                .on_click(cx.listener(move |pane, _, _, cx| {
                    if let Some(text) = &text {
                        pane.set_temporary_marker(text, index, cx);
                        pane.dismiss_terminal_context_menu(cx);
                    }
                })),
            );
        }
        row.into_any_element()
    }

    pub(super) fn render_markers(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let labels = self.preferences.reading_labels.clone();
        let tokens = self.theme.tokens;
        let background = self.reading_background_active();
        let palette = self.marker_palette();
        let mut items = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(tokens.spacing.two));
        for (index, marker) in self.markers.items.iter().enumerate() {
            let remove_label = labels.remove.clone();
            let remove = icon_button(
                &tokens,
                self.reading_icon("trash-2", tokens.metrics.sidebar_action_icon_size),
                IconButtonOptions::compact(tokens.metrics.ui_button_sm_height),
            )
            .id(SharedString::from(format!("marker-remove-{index}")))
            .debug_selector(move || format!("marker-remove-{index}"))
            .tooltip(move |_, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(tokens, remove_label.clone(), None, cx)
            })
            .on_click(cx.listener(move |pane, _, _, cx| {
                if index < pane.markers.items.len() {
                    pane.markers.items.remove(index);
                    pane.markers.base = None;
                    cx.notify();
                }
            }))
            .into_any_element();
            let title = entity_list_row(
                &tokens,
                EntityListRowOptions::new()
                    .compact()
                    .has_background_image(background),
                Some(
                    div()
                        .size(px(tokens.metrics.sidebar_action_icon_size))
                        .flex_none()
                        .rounded_full()
                        .bg(rgb(palette[marker.color]))
                        .into_any_element(),
                ),
                div()
                    .truncate()
                    .font_family(self.preferences.font_family.clone())
                    .child(marker.text.to_string())
                    .into_any_element(),
                None,
                Vec::new(),
                vec![remove],
            );
            let save = button_with(
                &tokens,
                labels.save_rule.clone(),
                ButtonOptions {
                    size: ButtonSize::Sm,
                    ..Default::default()
                },
            )
            .max_w_full()
            .min_w_0()
            .h_auto()
            .min_h(px(tokens.metrics.ui_button_sm_height))
            .whitespace_normal()
            .id(SharedString::from(format!("marker-save-{index}")))
            .debug_selector(move || format!("marker-save-{index}"))
            .on_click(cx.listener(move |pane, _, _, cx| {
                if let Some(marker) = pane.markers.items.get(index) {
                    let palette = pane.marker_palette();
                    pane.markers.pending_save = Some((
                        marker.text.clone(),
                        format!("#{:06x}", palette[marker.color]),
                        format!(
                            "#{:06x}",
                            marker_foreground(palette[marker.color], palette[4], palette[5])
                        ),
                    ));
                    pane.markers.open = false;
                    pane.request_context_action(
                        TerminalContextAction::SaveTemporaryMarker,
                        false,
                        cx,
                    );
                }
            }));
            items = items.child(
                div()
                    .id(SharedString::from(format!("marker-{index}")))
                    .w_full()
                    .min_w_0()
                    .border_1()
                    .border_color(rgb(tokens.ui.border))
                    .rounded(px(tokens.radii.md))
                    .flex()
                    .flex_col()
                    .child(title)
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .px(px(tokens.spacing.two))
                            .pb(px(tokens.spacing.two))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(tokens.spacing.two))
                            .child(self.render_marker_colors(Some(marker.text.clone()), cx))
                            .child(save),
                    ),
            );
        }
        if self.markers.items.is_empty() {
            items = items.child(oxideterm_gpui_ui::state::empty_state(
                &tokens,
                self.reading_icon("edit-3", tokens.metrics.ui_menu_icon_size),
                labels.mark_selection.clone(),
                None,
                None,
            ));
        }
        let header = oxideterm_gpui_ui::section::section_header(
            &tokens,
            labels.markers,
            oxideterm_gpui_ui::section::SectionHeaderOptions::new()
                .count(self.markers.items.len().to_string()),
            None,
            Some(self.reading_close_button(
                "markers-close",
                |pane, _, _, cx| {
                    pane.markers.open = false;
                    cx.notify();
                },
                cx,
            )),
        );
        let clear = button_with(
            &tokens,
            labels.clear_markers,
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                disabled: self.markers.items.is_empty(),
                ..Default::default()
            },
        )
        .max_w_full()
        .min_w_0()
        .h_auto()
        .min_h(px(tokens.metrics.ui_button_default_height))
        .whitespace_normal()
        .text_color(rgb(tokens.ui.error))
        .id("markers-clear")
        .debug_selector(|| "markers-clear".into())
        .on_click(cx.listener(|pane, _, _, cx| {
            pane.markers.items.clear();
            pane.markers.base = None;
            pane.markers.rules = Arc::default();
            cx.notify();
        }));
        let close = button_with(
            &tokens,
            labels.close,
            ButtonOptions {
                variant: ButtonVariant::Default,
                ..Default::default()
            },
        )
        .id("markers-footer-close")
        .debug_selector(|| "markers-footer-close".into())
        .on_click(cx.listener(|pane, _, _, cx| {
            pane.markers.open = false;
            cx.notify();
        }));
        let body = dialog_content(&tokens)
            .debug_selector(|| "markers-dialog".into())
            .bg(oxideterm_gpui_ui::color_for_background(
                tokens.ui.bg_elevated,
                background,
                (tokens.metrics.panel_vibrancy_alpha.clamp(0.0, 1.0) * 255.0).round() as u32,
            ))
            .w(px(520.0_f32.min(
                (f32::from(window.viewport_size().width) - 32.0).max(0.0),
            )))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 32.0).max(0.0)
            ))
            .min_w_0()
            .flex()
            .flex_col()
            .font_family(tokens.metrics.font_family)
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .child(dialog_header(&tokens).bg(rgba(0x00000000)).child(header))
            .child(
                modal_body(&tokens)
                    .min_h_0()
                    .max_h(px(
                        360.0_f32.min(f32::from(window.viewport_size().height) * 0.6)
                    ))
                    .overflow_y_scrollbar()
                    .child(items),
            )
            .child(
                dialog_footer(&tokens)
                    .w_full()
                    .min_w_0()
                    .h_auto()
                    .min_h(px(tokens.metrics.modal_footer_height))
                    .py(px(tokens.spacing.two))
                    .flex_none()
                    .flex_wrap()
                    .justify_between()
                    .bg(rgba(0x00000000))
                    .child(clear)
                    .child(close),
            );
        let overlay = dialog_overlay(
            &self.theme.tokens,
            overlay_content_boundary(body).on_key_down(cx.listener(
                |pane, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        pane.markers.open = false;
                        cx.notify();
                    }
                    cx.stop_propagation();
                },
            )),
        );
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::TopLeft)
                .position_mode(gpui::AnchoredPositionMode::Window)
                .position(gpui::point(px(0.0), px(0.0)))
                .child(
                    div()
                        .relative()
                        .w(window.viewport_size().width)
                        .h(window.viewport_size().height)
                        .child(overlay),
                ),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[test]
    fn marker_text_contrasts_with_bright_and_dark_theme_colors() {
        assert_eq!(marker_foreground(0xffeb3b, 0x111111, 0xfafafa), 0x111111);
        assert_eq!(marker_foreground(0x123456, 0x111111, 0xfafafa), 0xfafafa);
    }

    #[gpui::test]
    fn marker_dialog_actions_fit_long_text_and_narrow_windows(cx: &mut TestAppContext) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                80,
                24,
                TerminalUiPreferences::default(),
                window,
                cx,
            )
            .unwrap()
        });
        for (width, chinese, font_size) in [
            (720.0, false, 12.0),
            (320.0, true, 18.0),
            (360.0, false, 18.0),
        ] {
            cx.simulate_resize(gpui::size(px(width), px(600.0)));
            pane.update(cx, |pane, cx| {
                pane.theme
                    .tokens
                    .apply_motion(oxideterm_theme::UiMotionProfile::Off);
                pane.theme.tokens.metrics.ui_text_sm = font_size;
                pane.theme.tokens.metrics.ui_text_xs = font_size;
                pane.preferences.reading_labels = crate::TerminalReadingLabels::default();
                if chinese {
                    pane.preferences.reading_labels.markers = "临时标记".into();
                    pane.preferences.reading_labels.save_rule = "保存为高亮规则".into();
                    pane.preferences.reading_labels.clear_markers = "清除全部标记".into();
                    pane.preferences.reading_labels.close = "关闭".into();
                }
                pane.set_temporary_marker(
                    "request-identifier-with-long-text-0123456789-abcdefghijklmnopqrstuvwxyz",
                    1,
                    cx,
                );
                pane.markers.open = true;
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let dialog = cx.debug_bounds("markers-dialog").expect("marker dialog");
            assert!(dialog.left() >= px(0.0) && dialog.right() <= px(width));
            let mut buttons: Vec<gpui::Bounds<gpui::Pixels>> = Vec::new();
            for selector in [
                "marker-color-0",
                "marker-color-1",
                "marker-color-2",
                "marker-color-3",
                "marker-remove-0",
                "marker-save-0",
                "markers-clear",
                "markers-footer-close",
            ] {
                let button = cx.debug_bounds(selector).expect("marker action");
                assert!(
                    dialog.contains(&button.origin) && dialog.contains(&button.bottom_right()),
                    "{selector} exceeds dialog at {width}px: {button:?} outside {dialog:?}"
                );
                for previous in &buttons {
                    assert!(
                        button.left() >= previous.right()
                            || button.right() <= previous.left()
                            || button.top() >= previous.bottom()
                            || button.bottom() <= previous.top(),
                        "marker actions overlap at {width}px"
                    );
                }
                buttons.push(button);
            }
        }
    }

    #[gpui::test]
    fn temporary_markers_preserve_literals_and_survive_preference_refresh(cx: &mut TestAppContext) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                80,
                24,
                TerminalUiPreferences::default(),
                window,
                cx,
            )
            .unwrap()
        });
        pane.update(cx, |pane, cx| {
            pane.set_temporary_marker(" request-1 ", 0, cx);
            pane.set_temporary_marker("request-2", 1, cx);
            pane.set_temporary_marker(" request-1 ", 3, cx);
            let rules = pane.effective_reading_highlights().unwrap();
            assert_eq!(
                rules
                    .iter()
                    .map(|rule| rule.source.pattern.as_str())
                    .collect::<Vec<_>>(),
                [" request-1 ", "request-2"]
            );
            assert_eq!(
                rules[0].source.background,
                Some(format!("#{:06x}", pane.marker_palette()[3]))
            );
            let preferences = pane.preferences.clone();
            pane.set_preferences(preferences, cx);
            assert_eq!(
                pane.markers
                    .items
                    .iter()
                    .map(|marker| marker.text.as_str())
                    .collect::<Vec<_>>(),
                [" request-1 ", "request-2"]
            );
            assert_eq!(pane.preferences.highlight_rules.len(), 0);
            pane.terminal
                .lock()
                .feed_recording_output(b" request-1  request-2 REQUEST-2");
            let snapshot = pane.terminal.lock().snapshot();
            let compiled = pane.effective_reading_highlights().unwrap();
            let layout =
                crate::terminal_view::highlight::terminal_highlights_for_rows_with_compiled(
                    &snapshot,
                    &[],
                    Some(&compiled),
                    None,
                    0..1,
                );
            assert_eq!(
                layout
                    .backgrounds
                    .iter()
                    .map(|rect| (rect.row, rect.col, rect.cells))
                    .collect::<Vec<_>>(),
                [(0, 0, 11), (0, 12, 9)]
            );

            pane.markers.items.remove(0);
            pane.markers.base = None;
            let rules = pane.effective_reading_highlights().unwrap();
            assert_eq!(
                rules
                    .iter()
                    .map(|rule| rule.source.pattern.as_str())
                    .collect::<Vec<_>>(),
                ["request-2"]
            );
        });
    }
}
