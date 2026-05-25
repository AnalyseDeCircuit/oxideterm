use super::*;
use gpui_component::scroll::ScrollableElement;

const NATIVE_PLUGIN_UI_LIST_ROW_HEIGHT: f32 = 34.0;
const NATIVE_PLUGIN_UI_LIST_OVERSCAN: usize = 8;
const NATIVE_PLUGIN_UI_MAX_VISIBLE_ROWS: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativePluginSidebarPanelSelection {
    // Native keeps the selected plugin panel as data instead of encoding it in
    // a string key, but it represents Tauri's `plugin:<pluginId>:<panelId>`.
    pub plugin_id: String,
    pub panel_id: String,
}

impl WorkspaceApp {
    pub(super) fn open_native_plugin_tab(
        &mut self,
        plugin_id: &str,
        tab_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .plugin_registry
            .contributions()
            .tab_contribution(plugin_id, tab_id)
            .ok_or_else(|| format!("Plugin tab \"{plugin_id}:{tab_id}\" is not declared"))?;
        let existing_tab_id = self.tabs.iter().find_map(|tab| match &tab.kind {
            TabKind::Plugin {
                plugin_id: existing_plugin_id,
                tab_id: existing_tab_id,
            } if existing_plugin_id == plugin_id && existing_tab_id == tab_id => Some(tab.id),
            _ => None,
        });
        let tab_id_value = if let Some(existing_tab_id) = existing_tab_id {
            existing_tab_id
        } else {
            let tab_id_value = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id_value,
                kind: TabKind::Plugin {
                    plugin_id: plugin_id.to_string(),
                    tab_id: tab_id.to_string(),
                },
                title: contribution.definition.title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            tab_id_value
        };
        self.active_tab_id = Some(tab_id_value);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.persist_sidebar_settings();
        cx.notify();
        Ok(())
    }

    pub(super) fn render_native_plugin_tab_surface(
        &mut self,
        plugin_id: &str,
        tab_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let contribution = self
            .plugin_registry
            .contributions()
            .tab_contribution(plugin_id, tab_id);
        let runtime_view = self
            .plugin_registry
            .contributions()
            .runtime_tab_view(plugin_id, tab_id);
        let title = runtime_view
            .as_ref()
            .map(|view| view.title.clone())
            .or_else(|| {
                contribution
                    .as_ref()
                    .map(|entry| entry.definition.title.clone())
            })
            .unwrap_or_else(|| tab_id.to_string());

        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                self.render_native_plugin_surface_header(
                    plugin_id,
                    &title,
                    contribution
                        .as_ref()
                        .map(|entry| entry.plugin_name.as_str())
                        .unwrap_or(plugin_id),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px(px(self.tokens.metrics.settings_content_padding))
                    .py(px(self.tokens.metrics.settings_page_gap))
                    .child(match runtime_view {
                        Some(view) => self.render_native_plugin_declarative_schema(
                            plugin_id,
                            "tab",
                            &view.tab_id,
                            &view.schema,
                            cx,
                        ),
                        None => self.render_native_plugin_missing_view(
                            "Register a declarative tab schema before opening this plugin tab.",
                        ),
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_native_plugin_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(selection) = self.active_native_plugin_sidebar_panel.as_ref() else {
            return self.render_plugin_sidebar_placeholder();
        };
        let panels = self
            .plugin_registry
            .contributions()
            .runtime_sidebar_panels();
        let Some(panel) = panels.iter().find(|panel| {
            panel.plugin_id == selection.plugin_id && panel.panel_id == selection.panel_id
        }) else {
            return self.render_plugin_sidebar_placeholder();
        };

        // Tauri renders exactly the selected plugin panel component under
        // `sidebarActiveSection === "plugin:<pluginId>:<panelId>"`. Do not add
        // a native panel header here; the plugin-provided schema owns its body.
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scrollbar()
            .px_2()
            .py_2()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .bg(rgb(theme.bg_panel))
            .child(self.render_native_plugin_declarative_schema(
                &panel.plugin_id,
                "sidebarPanel",
                &panel.panel_id,
                &panel.schema,
                cx,
            ))
            .into_any_element()
    }

    fn render_native_plugin_surface_header(
        &self,
        plugin_id: &str,
        title: &str,
        plugin_name: &str,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(52.0))
            .flex()
            .items_center()
            .justify_between()
            .px(px(self.tokens.metrics.settings_content_padding))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{plugin_name} · {plugin_id}")),
                    ),
            )
            .into_any_element()
    }

    fn render_native_plugin_missing_view(&self, message: &str) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_h(px(180.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_center()
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(
                LucideIcon::Puzzle,
                28.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .max_w(px(420.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(message.to_string()),
            )
            .into_any_element()
    }

    fn render_native_plugin_declarative_schema(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        schema: &plugin_host::NativePluginDeclarativeUiSchema,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut body = div().w_full().flex().flex_col().gap(px(12.0));
        if let Some(title) = &schema.title {
            body = body.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(title.clone()),
            );
        }
        if let Some(description) = &schema.description {
            body = body.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(description.clone()),
            );
        }
        if !schema.controls.is_empty() {
            body = body.child(self.render_native_plugin_declarative_controls(
                plugin_id,
                surface_kind,
                surface_id,
                "root",
                &schema.controls,
                cx,
            ));
        }
        for section in &schema.sections {
            body = body.child(self.render_native_plugin_declarative_section(
                plugin_id,
                surface_kind,
                surface_id,
                section,
                cx,
            ));
        }
        body.into_any_element()
    }

    fn render_native_plugin_declarative_section(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section: &plugin_host::NativePluginDeclarativeUiSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut section_el = div().w_full().flex().flex_col().gap(px(8.0)).py(px(4.0));
        if let Some(title) = &section.title {
            section_el = section_el.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(title.clone()),
            );
        }
        if let Some(description) = &section.description {
            section_el = section_el.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(description.clone()),
            );
        }
        section_el
            .child(self.render_native_plugin_declarative_controls(
                plugin_id,
                surface_kind,
                surface_id,
                &section.id,
                &section.controls,
                cx,
            ))
            .into_any_element()
    }

    fn render_native_plugin_declarative_controls(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        controls: &[plugin_host::NativePluginDeclarativeUiControl],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut group = div().w_full().flex().flex_col().gap(px(8.0));
        for control in controls {
            group = group.child(self.render_native_plugin_declarative_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                cx,
            ));
        }
        group.into_any_element()
    }

    fn render_native_plugin_declarative_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match control.kind.as_str() {
            "button" => self.render_native_plugin_button_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                cx,
            ),
            "checkbox" => self.render_native_plugin_checkbox_control(control),
            "divider" => self.render_native_plugin_divider_control(),
            "markdown" => self.render_native_plugin_text_block_control(control, false),
            "code" | "codeBlock" | "code-block" => {
                self.render_native_plugin_text_block_control(control, true)
            }
            "statusBadge" | "status-badge" => self.render_native_plugin_status_badge(control),
            "progress" => self.render_native_plugin_progress_control(control),
            "table" => self.render_native_plugin_table_control(control),
            "list" => self.render_native_plugin_list_control(control),
            "emptyState" | "empty-state" => self.render_native_plugin_empty_state_control(control),
            "keyValue" | "key-value" | "keyValueRow" | "key-value-row" => {
                self.render_native_plugin_key_value_control(control)
            }
            _ => self.render_native_plugin_field_control(control),
        }
    }

    fn render_native_plugin_button_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let actionable = plugin_host::native_plugin_declarative_control_is_actionable(control);
        let label = native_plugin_control_label(control, "Run");
        let control_id = control.id.clone().unwrap_or_default();
        let plugin_id = plugin_id.to_string();
        let surface_kind = surface_kind.to_string();
        let surface_id = surface_id.to_string();
        let section_id = section_id.to_string();
        let value = control.value.clone().unwrap_or(serde_json::Value::Null);

        let button = div()
            .h(px(30.0))
            .min_w(px(84.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if actionable {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(if actionable {
                rgb(theme.accent)
            } else {
                rgb(theme.bg_secondary)
            })
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(if actionable {
                rgb(theme.bg)
            } else {
                rgb(theme.text_muted)
            })
            .child(if control.loading {
                format!("{label}...")
            } else {
                label
            });

        if actionable {
            button
                .cursor_pointer()
                .hover(move |button| button.opacity(0.9))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        // Button activation is delivered as data to the plugin
                        // runtime; plugin code never runs on the GPUI event stack.
                        this.dispatch_native_plugin_event(
                            plugin_id.clone(),
                            plugin_host::NATIVE_PLUGIN_UI_EVENT,
                            serde_json::json!({
                                "type": "click",
                                "surfaceKind": surface_kind,
                                "surfaceId": surface_id,
                                "sectionId": section_id,
                                "controlId": control_id,
                                "value": value,
                            }),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        } else {
            button.opacity(0.65).into_any_element()
        }
    }

    fn render_native_plugin_field_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(self.render_native_plugin_control_label(control))
            .child(
                div()
                    .h(px(30.0))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(native_plugin_control_value_label(control)),
            )
            .into_any_element()
    }

    fn render_native_plugin_checkbox_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let checked = control
            .value
            .as_ref()
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(rgb(if checked { theme.accent } else { theme.border }))
                    .bg(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.bg_panel)
                    })
                    .when(checked, |box_el| {
                        box_el.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            12.0,
                            rgb(theme.bg),
                        ))
                    }),
            )
            .child(native_plugin_control_label(control, "Checkbox"))
            .into_any_element()
    }

    fn render_native_plugin_divider_control(&self) -> AnyElement {
        div()
            .w_full()
            .h(px(1.0))
            .bg(rgb(self.tokens.ui.border))
            .into_any_element()
    }

    fn render_native_plugin_text_block_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        code: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(if code {
                rgb(theme.bg)
            } else {
                rgb(theme.bg_panel)
            })
            .px(px(10.0))
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .when(code, |block| block.font_family("monospace"))
            .child(native_plugin_control_text(control))
            .into_any_element()
    }

    fn render_native_plugin_status_badge(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .rounded_full()
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_active))
            .px(px(9.0))
            .py(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_heading))
            .child(native_plugin_control_label(control, "Status"))
            .into_any_element()
    }

    fn render_native_plugin_progress_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let progress = control
            .value
            .as_ref()
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0) as f32;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(self.render_native_plugin_control_label(control))
            .child(
                div()
                    .h(px(7.0))
                    .w_full()
                    .rounded_full()
                    .bg(rgb(theme.bg_secondary))
                    .child(
                        div()
                            .h_full()
                            .w(px(160.0 * progress))
                            .rounded_full()
                            .bg(rgb(theme.accent)),
                    ),
            )
            .into_any_element()
    }

    fn render_native_plugin_table_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let rows = Arc::new(control.rows.clone().unwrap_or_default());
        let row_count = rows.len();
        let spec = TauriVirtualListSpec::new(
            px(NATIVE_PLUGIN_UI_LIST_ROW_HEIGHT),
            NATIVE_PLUGIN_UI_LIST_OVERSCAN,
        );
        let state = tauri_virtual_list_state(row_count, ListAlignment::Top, spec);
        let height = native_plugin_virtual_list_height(row_count);
        let columns = Arc::new(control.columns.clone().unwrap_or_default());
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_native_plugin_control_label(control))
            .child(
                div()
                    .h(px(height))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, _cx| {
                            let row = rows.get(index).cloned().unwrap_or(serde_json::Value::Null);
                            native_plugin_table_row_element(row, columns.clone(), theme)
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_native_plugin_list_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let rows = Arc::new(control.rows.clone().unwrap_or_default());
        let row_count = rows.len();
        let spec = TauriVirtualListSpec::new(
            px(NATIVE_PLUGIN_UI_LIST_ROW_HEIGHT),
            NATIVE_PLUGIN_UI_LIST_OVERSCAN,
        );
        let state = tauri_virtual_list_state(row_count, ListAlignment::Top, spec);
        let height = native_plugin_virtual_list_height(row_count);
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_native_plugin_control_label(control))
            .child(
                div()
                    .h(px(height))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, _cx| {
                            let row = rows.get(index).cloned().unwrap_or(serde_json::Value::Null);
                            native_plugin_list_row_element(row, theme)
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_native_plugin_empty_state_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_h(px(90.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .text_center()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(
                LucideIcon::Inbox,
                22.0,
                rgb(theme.text_muted),
            ))
            .child(native_plugin_control_label(control, "No data"))
            .into_any_element()
    }

    fn render_native_plugin_key_value_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(native_plugin_control_label(control, "Key")),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(220.0))
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(native_plugin_control_value_label(control)),
            )
            .into_any_element()
    }

    fn render_native_plugin_control_label(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(native_plugin_control_label(control, "Field"))
            .into_any_element()
    }
}

fn native_plugin_control_label(
    control: &plugin_host::NativePluginDeclarativeUiControl,
    fallback: &str,
) -> String {
    control
        .label
        .clone()
        .or_else(|| control.id.clone())
        .unwrap_or_else(|| fallback.to_string())
}

fn native_plugin_control_text(control: &plugin_host::NativePluginDeclarativeUiControl) -> String {
    control
        .text
        .clone()
        .or_else(|| control.value.as_ref().map(native_plugin_ui_value_label))
        .unwrap_or_default()
}

fn native_plugin_control_value_label(
    control: &plugin_host::NativePluginDeclarativeUiControl,
) -> String {
    control
        .value
        .as_ref()
        .map(native_plugin_ui_value_label)
        .unwrap_or_default()
}

fn native_plugin_ui_value_label(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn native_plugin_virtual_list_height(row_count: usize) -> f32 {
    let visible_rows = row_count.clamp(1, NATIVE_PLUGIN_UI_MAX_VISIBLE_ROWS);
    visible_rows as f32 * NATIVE_PLUGIN_UI_LIST_ROW_HEIGHT
}

fn native_plugin_list_row_element(value: serde_json::Value, theme: AppUiColors) -> AnyElement {
    div()
        .h(px(NATIVE_PLUGIN_UI_LIST_ROW_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba((theme.border << 8) | 0x66))
        .px(px(10.0))
        .text_size(px(12.0))
        .text_color(rgb(theme.text))
        .child(native_plugin_ui_value_label(&value))
        .into_any_element()
}

fn native_plugin_table_row_element(
    value: serde_json::Value,
    columns: Arc<Vec<String>>,
    theme: AppUiColors,
) -> AnyElement {
    let mut row = div()
        .h(px(NATIVE_PLUGIN_UI_LIST_ROW_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba((theme.border << 8) | 0x66))
        .px(px(10.0))
        .gap(px(8.0))
        .text_size(px(12.0))
        .text_color(rgb(theme.text));
    if columns.is_empty() {
        return row
            .child(native_plugin_ui_value_label(&value))
            .into_any_element();
    }
    for column in columns.iter() {
        let label = value
            .get(column)
            .map(native_plugin_ui_value_label)
            .unwrap_or_default();
        row = row.child(div().flex_1().min_w(px(0.0)).truncate().child(label));
    }
    row.into_any_element()
}
