use super::*;
use oxideterm_gpui_ui::scroll::ScrollableElement;
use zeroize::Zeroizing;

const NATIVE_PLUGIN_UI_LIST_OVERSCAN: usize = 8;
const NATIVE_PLUGIN_UI_MAX_VISIBLE_ROWS: usize = 8;
const NATIVE_PLUGIN_UI_TABLE_COLUMN_WIDTH: f32 = 120.0;
const NATIVE_PLUGIN_UI_TABLE_COLUMN_MIN_WIDTH: f32 = 64.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativePluginUiControlContext {
    pub plugin_id: String,
    pub surface_kind: String,
    pub surface_id: String,
    pub section_id: String,
    pub control_id: String,
    pub control_kind: String,
}

enum NativePluginUiControlDraft {
    Text(Zeroizing<String>),
    Value(serde_json::Value),
}

struct NativePluginUiControlState {
    context: NativePluginUiControlContext,
    draft: NativePluginUiControlDraft,
    source_signature: u64,
    render_generation: u64,
}

#[derive(Default)]
pub(super) struct NativePluginUiState {
    controls: HashMap<u64, NativePluginUiControlState>,
    pub focused_input: Option<u64>,
    open_select: Option<u64>,
    render_generation: u64,
}

impl NativePluginUiState {
    fn begin_surface_render(&mut self) -> u64 {
        self.render_generation = self.render_generation.wrapping_add(1);
        self.render_generation
    }

    fn sync_control(
        &mut self,
        context: NativePluginUiControlContext,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
    ) -> u64 {
        let key = native_plugin_ui_control_key(&context);
        let source_signature = native_plugin_ui_source_signature(control.value.as_ref());
        match self.controls.get_mut(&key) {
            Some(state) => {
                state.context = context;
                state.render_generation = render_generation;
                if state.source_signature != source_signature {
                    state.draft = native_plugin_ui_control_source_draft(control);
                    state.source_signature = source_signature;
                }
            }
            None => {
                self.controls.insert(
                    key,
                    NativePluginUiControlState {
                        context,
                        draft: native_plugin_ui_control_source_draft(control),
                        source_signature,
                        render_generation,
                    },
                );
            }
        }
        key
    }

    fn finish_surface_render(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        render_generation: u64,
    ) {
        self.controls.retain(|_, state| {
            state.context.plugin_id != plugin_id
                || state.context.surface_kind != surface_kind
                || state.context.surface_id != surface_id
                || state.render_generation == render_generation
        });
        if self
            .focused_input
            .is_some_and(|key| !self.controls.contains_key(&key))
        {
            self.focused_input = None;
        }
        if self
            .open_select
            .is_some_and(|key| !self.controls.contains_key(&key))
        {
            self.open_select = None;
        }
    }

    pub(super) fn context(&self, key: u64) -> Option<&NativePluginUiControlContext> {
        self.controls.get(&key).map(|state| &state.context)
    }

    pub(super) fn text(&self, key: u64) -> Option<&str> {
        match &self.controls.get(&key)?.draft {
            NativePluginUiControlDraft::Text(value) => Some(value.as_str()),
            NativePluginUiControlDraft::Value(_) => None,
        }
    }

    fn value(&self, key: u64) -> Option<serde_json::Value> {
        match &self.controls.get(&key)?.draft {
            NativePluginUiControlDraft::Text(value) => {
                Some(serde_json::Value::String(value.to_string()))
            }
            NativePluginUiControlDraft::Value(value) => Some(value.clone()),
        }
    }

    pub(super) fn text_mut(&mut self, key: u64) -> Option<&mut String> {
        match &mut self.controls.get_mut(&key)?.draft {
            NativePluginUiControlDraft::Text(value) => Some(value),
            NativePluginUiControlDraft::Value(_) => None,
        }
    }

    fn set_value(&mut self, key: u64, value: serde_json::Value) -> bool {
        let Some(state) = self.controls.get_mut(&key) else {
            return false;
        };
        state.draft = NativePluginUiControlDraft::Value(value);
        true
    }
}

fn native_plugin_ui_control_key(context: &NativePluginUiControlContext) -> u64 {
    let mut hasher = DefaultHasher::new();
    context.plugin_id.hash(&mut hasher);
    context.surface_kind.hash(&mut hasher);
    context.surface_id.hash(&mut hasher);
    context.section_id.hash(&mut hasher);
    context.control_id.hash(&mut hasher);
    hasher.finish()
}

fn native_plugin_ui_source_signature(value: Option<&serde_json::Value>) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(value.unwrap_or(&serde_json::Value::Null))
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

fn native_plugin_ui_control_source_draft(
    control: &plugin_host::NativePluginDeclarativeUiControl,
) -> NativePluginUiControlDraft {
    if matches!(control.kind.as_str(), "text" | "password" | "number") {
        NativePluginUiControlDraft::Text(Zeroizing::new(native_plugin_control_value_label(control)))
    } else {
        NativePluginUiControlDraft::Value(control.value.clone().unwrap_or(serde_json::Value::Null))
    }
}

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
        self.bootstrap_native_plugin_runtime(cx);
        let contribution = self
            .native_plugin_runtime
            .registry
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
        if self.focus_detached_tab_window(tab_id_value, cx) {
            return Ok(());
        }
        self.main_window_tabs.active_tab_id = Some(tab_id_value);
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
        self.bootstrap_native_plugin_runtime(cx);
        let theme = self.tokens.ui;
        let contribution = self
            .native_plugin_runtime
            .registry
            .contributions()
            .tab_contribution(plugin_id, tab_id);
        let runtime_view = self
            .native_plugin_runtime
            .registry
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
            .bg(self.workspace_chrome_background(theme.bg))
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
        self.bootstrap_native_plugin_runtime(cx);
        let theme = self.tokens.ui;
        let Some(selection) = self.native_plugin_manager.active_sidebar_panel.as_ref() else {
            return self.render_plugin_sidebar_placeholder();
        };
        let panels = self
            .native_plugin_runtime
            .registry
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
        let render_generation = self.native_plugin_ui.begin_surface_render();
        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three));
        if let Some(title) = &schema.title {
            let mut options = oxideterm_gpui_ui::SectionHeaderOptions::new();
            if let Some(description) = &schema.description {
                options = options.description(description.clone());
            }
            body = body.child(oxideterm_gpui_ui::section_header(
                &self.tokens,
                title.clone(),
                options,
                None,
                None,
            ));
        }
        if !schema.controls.is_empty() {
            body = body.child(self.render_native_plugin_declarative_controls(
                plugin_id,
                surface_kind,
                surface_id,
                "root",
                &schema.controls,
                render_generation,
                cx,
            ));
        }
        for section in &schema.sections {
            body = body.child(self.render_native_plugin_declarative_section(
                plugin_id,
                surface_kind,
                surface_id,
                section,
                render_generation,
                cx,
            ));
        }
        self.native_plugin_ui.finish_surface_render(
            plugin_id,
            surface_kind,
            surface_id,
            render_generation,
        );
        body.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.blur_native_plugin_ui_input();
                this.native_plugin_ui.open_select = None;
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_native_plugin_declarative_section(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section: &plugin_host::NativePluginDeclarativeUiSection,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut section_el = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two));
        if let Some(title) = &section.title {
            let mut options = oxideterm_gpui_ui::SectionHeaderOptions::new().compact();
            if let Some(description) = &section.description {
                options = options.description(description.clone());
            }
            section_el = section_el.child(oxideterm_gpui_ui::section_header(
                &self.tokens,
                title.clone(),
                options,
                None,
                None,
            ));
        }
        section_el
            .child(self.render_native_plugin_declarative_controls(
                plugin_id,
                surface_kind,
                surface_id,
                &section.id,
                &section.controls,
                render_generation,
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
        render_generation: u64,
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
                render_generation,
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
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match control.kind.as_str() {
            "stack" | "row" | "card" | "toolbar" => self.render_native_plugin_layout_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                render_generation,
                cx,
            ),
            "button" | "iconButton" | "icon-button" => self.render_native_plugin_button_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                cx,
            ),
            "text" | "password" | "number" => self.render_native_plugin_input_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                render_generation,
                cx,
            ),
            "checkbox" => self.render_native_plugin_checkbox_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                render_generation,
                cx,
            ),
            "select" => self.render_native_plugin_select_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                render_generation,
                cx,
            ),
            "radioGroup" | "radio-group" => self.render_native_plugin_radio_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                render_generation,
                cx,
            ),
            "segmentedControl" | "segmented-control" => self
                .render_native_plugin_segmented_control(
                    plugin_id,
                    surface_kind,
                    surface_id,
                    section_id,
                    control,
                    render_generation,
                    cx,
                ),
            "slider" => self.render_native_plugin_slider_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                control,
                render_generation,
                cx,
            ),
            "alert" => self.render_native_plugin_alert_control(control),
            "divider" => self.render_native_plugin_divider_control(),
            "markdown" => self.render_native_plugin_text_block_control(control, false),
            "code" | "codeBlock" | "code-block" => {
                self.render_native_plugin_text_block_control(control, true)
            }
            "statusBadge" | "status-badge" | "badge" => {
                self.render_native_plugin_status_badge(control)
            }
            "progress" => self.render_native_plugin_progress_control(control),
            "table" => self.render_native_plugin_table_control(control),
            "list" => self.render_native_plugin_list_control(control),
            "emptyState" | "empty-state" => self.render_native_plugin_empty_state_control(control),
            "keyValue" | "key-value" | "keyValueRow" | "key-value-row" => {
                self.render_native_plugin_key_value_control(control)
            }
            _ => self.render_native_plugin_text_block_control(control, false),
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
        let actionable = plugin_host::native_plugin_declarative_control_is_actionable(control);
        let label = if control.loading {
            format!("{}…", native_plugin_control_label(control, ""))
        } else {
            native_plugin_control_label(control, "")
        };
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let value = control.value.clone().unwrap_or(serde_json::Value::Null);
        let icon_name = control.icon.as_deref().map(LucideIcon::from_plugin_name);
        if matches!(control.kind.as_str(), "iconButton" | "icon-button") {
            let tooltip = label;
            return self.workspace_tooltip_icon_button(
                icon_name.unwrap_or(LucideIcon::Puzzle),
                self.tokens.metrics.ui_menu_icon_size,
                rgb(self.tokens.ui.text),
                oxideterm_gpui_ui::IconButtonOptions {
                    size: native_plugin_button_icon_size(&self.tokens, control.size.as_deref()),
                    disabled: !actionable,
                    loading: control.loading,
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::IconButtonOptions::compact(native_plugin_button_icon_size(
                        &self.tokens,
                        control.size.as_deref(),
                    ))
                },
                tooltip,
                "native-plugin-icon-button",
                false,
                cx.listener(move |this, _event, _window, cx| {
                    this.blur_native_plugin_ui_input();
                    this.dispatch_native_plugin_ui_control_event(
                        context.clone(),
                        "click",
                        value.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }),
                cx.entity(),
            );
        }
        let icon = icon_name.map(|icon| {
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.ui_menu_icon_size,
                rgb(self.tokens.ui.text),
            )
        });
        let button = oxideterm_gpui_ui::toolbar_button(
            &self.tokens,
            label,
            icon,
            oxideterm_gpui_ui::ToolbarButtonOptions {
                button: native_plugin_button_options(control),
                has_background: true,
                loading: control.loading,
                ..oxideterm_gpui_ui::ToolbarButtonOptions::default()
            },
        );

        if actionable {
            button
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.blur_native_plugin_ui_input();
                        // Button activation is delivered as data to the plugin
                        // runtime; plugin code never runs on the GPUI event stack.
                        this.dispatch_native_plugin_ui_control_event(
                            context.clone(),
                            "click",
                            value.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        } else {
            button.into_any_element()
        }
    }

    fn render_native_plugin_input_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let key = self
            .native_plugin_ui
            .sync_control(context, control, render_generation);
        let target = WorkspaceImeTarget::PluginControl(key);
        let value = self
            .native_plugin_ui
            .text(key)
            .unwrap_or_default()
            .to_string();
        let focused = self.native_plugin_ui.focused_input == Some(key);
        let marked = self.marked_text_for_target(target).map(str::to_string);
        let selected_range = self.ime_selected_range_for_target(target);
        let input = oxideterm_gpui_ui::input::input(
            &self.tokens,
            oxideterm_gpui_ui::input::InputView {
                value: &value,
                placeholder: control.placeholder.clone().unwrap_or_default(),
                focused,
                caret_visible: self.new_connection_caret_visible,
                input_type: if control.kind == "password" {
                    oxideterm_gpui_ui::input::InputType::Password
                } else {
                    oxideterm_gpui_ui::input::InputType::Text
                },
                selected_all: self.selected_ime_target == Some(target),
                selected_range,
                marked_text: marked.as_deref(),
                disabled: control.disabled,
            },
        )
        .id(("native-plugin-input", key));
        let input = if control.disabled {
            input
        } else {
            input
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.native_plugin_ui.focused_input = Some(key);
                        this.native_plugin_ui.open_select = None;
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        this.new_connection_caret_visible = true;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                        if this.native_plugin_ui.focused_input == Some(key) {
                            this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                        }
                    }),
                )
        };
        let view = cx.entity();
        let anchored = oxideterm_gpui_ui::text_input::text_input_anchor_probe(
            target.anchor_id(),
            input,
            move |anchor, _window, cx| {
                let _ = view.update(cx, |this, cx| this.update_text_input_anchor(anchor, cx));
            },
        );
        self.render_native_plugin_form_field(control, anchored)
    }

    fn render_native_plugin_checkbox_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let key = self
            .native_plugin_ui
            .sync_control(context, control, render_generation);
        let checked = self
            .native_plugin_ui
            .value(key)
            .as_ref()
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let checkbox = oxideterm_gpui_ui::checkbox_with(
            &self.tokens,
            native_plugin_control_label(control, ""),
            checked,
            oxideterm_gpui_ui::CheckboxOptions {
                focused: false,
                disabled: control.disabled,
            },
        );
        if control.disabled {
            checkbox.into_any_element()
        } else {
            checkbox
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_native_plugin_ui_control_value(
                            key,
                            serde_json::Value::Bool(!checked),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        }
    }

    fn render_native_plugin_select_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let key = self
            .native_plugin_ui
            .sync_control(context, control, render_generation);
        let value = self
            .native_plugin_ui
            .value(key)
            .unwrap_or(serde_json::Value::Null);
        let options = control.options.clone().unwrap_or_default();
        let selected_label = options
            .iter()
            .find(|option| option.value == value)
            .map(|option| option.label.clone());
        let open = self.native_plugin_ui.open_select == Some(key);
        let trigger = oxideterm_gpui_ui::select::select_trigger_with_focus_visible(
            &self.tokens,
            selected_label
                .clone()
                .or_else(|| control.placeholder.clone())
                .unwrap_or_default(),
            selected_label.is_none(),
            control.disabled,
            open,
        );
        let trigger = if control.disabled {
            trigger
        } else {
            trigger.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.blur_native_plugin_ui_input();
                    this.native_plugin_ui.open_select =
                        if this.native_plugin_ui.open_select == Some(key) {
                            None
                        } else {
                            Some(key)
                        };
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
        };
        let mut select = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one))
            .child(trigger);
        if open {
            let mut menu = oxideterm_gpui_ui::select::select_inline_menu(&self.tokens);
            for option in options {
                let selected = option.value == value;
                let option_value = option.value.clone();
                menu = menu.child(
                    oxideterm_gpui_ui::select::select_inline_option_row(
                        &self.tokens,
                        selected,
                        false,
                    )
                    .child(option.label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.native_plugin_ui.open_select = None;
                            this.set_native_plugin_ui_control_value(key, option_value.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
            select = select.child(menu);
        }
        self.render_native_plugin_form_field(control, select)
    }

    fn render_native_plugin_radio_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let key = self
            .native_plugin_ui
            .sync_control(context, control, render_generation);
        let value = self
            .native_plugin_ui
            .value(key)
            .unwrap_or(serde_json::Value::Null);
        let mut group = oxideterm_gpui_ui::radio_group::radio_group(&self.tokens);
        for option in control.options.clone().unwrap_or_default() {
            let selected = option.value == value;
            let option_value = option.value.clone();
            let row = div()
                .flex()
                .items_center()
                .gap(px(self.tokens.spacing.two))
                .child(oxideterm_gpui_ui::radio_group::radio_group_item(
                    &self.tokens,
                    selected,
                    control.disabled,
                ))
                .child(option.label)
                .when(!control.disabled, |row| {
                    row.cursor_pointer().on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_native_plugin_ui_control_value(key, option_value.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
                });
            group = group.child(row);
        }
        self.render_native_plugin_form_field(control, group)
    }

    fn render_native_plugin_segmented_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let key = self
            .native_plugin_ui
            .sync_control(context, control, render_generation);
        let value = self
            .native_plugin_ui
            .value(key)
            .unwrap_or(serde_json::Value::Null);
        let options = control.options.clone().unwrap_or_default();
        let active_index = options
            .iter()
            .position(|option| option.value == value)
            .unwrap_or(0);
        let mut items = Vec::with_capacity(options.len());
        for (index, option) in options.into_iter().enumerate() {
            let option_value = option.value.clone();
            let item = oxideterm_gpui_ui::segmented_control_item(
                &self.tokens,
                option.label,
                index == active_index,
            );
            let item = if control.disabled {
                item.opacity(0.5)
            } else {
                item.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_native_plugin_ui_control_value(key, option_value.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
            };
            items.push(item.into_any_element());
        }
        let segmented = oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            ("native-plugin-segmented", key),
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                active_index,
                items.len(),
            ),
            items,
        );
        self.render_native_plugin_form_field(control, segmented)
    }

    fn render_native_plugin_slider_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = native_plugin_ui_control_context(
            plugin_id,
            surface_kind,
            surface_id,
            section_id,
            control,
        );
        let key = self
            .native_plugin_ui
            .sync_control(context, control, render_generation);
        let min = control.min.unwrap_or(0.0) as f32;
        let max = control.max.unwrap_or(100.0).max(f64::from(min)) as f32;
        let step = control.step.unwrap_or(1.0).max(f64::EPSILON) as f32;
        let value = self
            .native_plugin_ui
            .value(key)
            .as_ref()
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(f64::from(min)) as f32;
        let target = WorkspaceImeTarget::PluginControl(key);
        let slider = oxideterm_gpui_ui::slider::slider(
            &self.tokens,
            oxideterm_gpui_ui::slider::SliderView {
                min,
                max,
                value,
                disabled: control.disabled,
            },
        )
        .h(px(self.tokens.metrics.ui_control_height));
        let slider = if control.disabled {
            slider
        } else {
            slider
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        this.set_native_plugin_slider_from_pointer(
                            key,
                            target,
                            event.position.x,
                            min,
                            max,
                            step,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.set_native_plugin_slider_from_pointer(
                                key,
                                target,
                                event.position.x,
                                min,
                                max,
                                step,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    }),
                )
        };
        let view = cx.entity();
        let anchored = oxideterm_gpui_ui::text_input::text_input_anchor_probe(
            target.anchor_id(),
            slider,
            move |anchor, _window, cx| {
                let _ = view.update(cx, |this, cx| this.update_text_input_anchor(anchor, cx));
            },
        );
        self.render_native_plugin_form_field(control, anchored)
    }

    fn render_native_plugin_layout_control(
        &mut self,
        plugin_id: &str,
        surface_kind: &str,
        surface_id: &str,
        section_id: &str,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        render_generation: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = native_plugin_layout_gap(&self.tokens, control.gap.as_deref());
        let mut children = Vec::with_capacity(control.children.len());
        for child in &control.children {
            children.push(self.render_native_plugin_declarative_control(
                plugin_id,
                surface_kind,
                surface_id,
                section_id,
                child,
                render_generation,
                cx,
            ));
        }
        match control.kind.as_str() {
            "row" => div()
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(gap))
                .children(children)
                .into_any_element(),
            "card" => oxideterm_gpui_ui::semantic_surface(
                &self.tokens,
                native_plugin_card_surface_options(control),
            )
            .w_full()
            .flex()
            .flex_col()
            .gap(px(gap))
            .children(children)
            .into_any_element(),
            "toolbar" => oxideterm_gpui_ui::semantic_surface(
                &self.tokens,
                oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
                    .padding(oxideterm_gpui_ui::SurfacePadding::Compact),
            )
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(gap))
            .children(children)
            .into_any_element(),
            _ => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(gap))
                .children(children)
                .into_any_element(),
        }
    }

    fn render_native_plugin_alert_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let tone = native_plugin_ui_state_tone(control.tone.as_deref());
        oxideterm_gpui_ui::state_notice(
            &self.tokens,
            tone,
            Self::render_lucide_icon(
                LucideIcon::from_plugin_name(control.icon.as_deref().unwrap_or("info")),
                self.tokens.metrics.ui_menu_icon_size,
                rgb(native_plugin_ui_tone_color(
                    &self.tokens,
                    control.tone.as_deref(),
                )),
            ),
            native_plugin_control_label(control, ""),
            control.description.clone(),
        )
        .into_any_element()
    }

    fn render_native_plugin_form_field(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        input: impl IntoElement,
    ) -> AnyElement {
        let mut field = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one));
        if let Some(label) = control.label.clone() {
            field = field.child(oxideterm_gpui_ui::form_field(&self.tokens, label, input));
        } else {
            field = field.child(input);
        }
        field
            .when_some(control.description.clone(), |field, description| {
                field.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(description),
                )
            })
            .into_any_element()
    }

    fn set_native_plugin_ui_control_value(
        &mut self,
        key: u64,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        self.blur_native_plugin_ui_input();
        let Some(context) = self.native_plugin_ui.context(key).cloned() else {
            return;
        };
        if self.native_plugin_ui.set_value(key, value.clone()) {
            self.dispatch_native_plugin_ui_control_event(context, "change", value, cx);
            cx.notify();
        }
    }

    fn blur_native_plugin_ui_input(&mut self) {
        if self.native_plugin_ui.focused_input.take().is_some() {
            self.clear_ime_selection();
        }
    }

    pub(super) fn dispatch_native_plugin_ui_input_event(
        &mut self,
        key: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(context) = self.native_plugin_ui.context(key).cloned() else {
            return;
        };
        let value = if context.control_kind == "password" {
            let Some(password) = self.native_plugin_ui.text(key) else {
                return;
            };
            native_plugin_password_event_value(
                password,
                self.native_plugin_ui_secret_event_is_approved(&context.plugin_id),
            )
        } else {
            let Some(value) = self.native_plugin_ui.value(key) else {
                return;
            };
            value
        };
        // The event is sent only to the plugin that owns this explicitly
        // focused surface; host-owned password drafts remain zeroizing.
        self.dispatch_native_plugin_ui_control_event(context, "input", value, cx);
    }

    fn native_plugin_ui_secret_event_is_approved(&self, plugin_id: &str) -> bool {
        self.native_plugin_runtime
            .registry
            .plugins()
            .iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .is_some_and(|plugin| {
                plugin
                    .config
                    .approved_capabilities
                    .iter()
                    .any(|capability| {
                        capability
                            == oxideterm_plugin_host_api::capabilities::NATIVE_PLUGIN_CAPABILITY_CREDENTIALS_RAW_READ
                    })
            })
    }

    pub(super) fn native_plugin_ui_control_is_visible(&self, key: u64) -> bool {
        let Some(context) = self.native_plugin_ui.context(key) else {
            return false;
        };
        match context.surface_kind.as_str() {
            "tab" => self.active_tab().is_some_and(|tab| {
                matches!(
                    &tab.kind,
                    TabKind::Plugin { plugin_id, tab_id }
                        if plugin_id == &context.plugin_id && tab_id == &context.surface_id
                )
            }),
            "sidebarPanel" => {
                self.effective_sidebar_panel_section() == SidebarSection::Extensions
                    && self
                        .native_plugin_manager
                        .active_sidebar_panel
                        .as_ref()
                        .is_some_and(|selection| {
                            selection.plugin_id == context.plugin_id
                                && selection.panel_id == context.surface_id
                        })
            }
            _ => false,
        }
    }

    fn dispatch_native_plugin_ui_control_event(
        &mut self,
        context: NativePluginUiControlContext,
        event_type: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_native_plugin_event(
            context.plugin_id,
            plugin_host::NATIVE_PLUGIN_UI_EVENT,
            serde_json::json!({
                "type": event_type,
                "componentKind": context.control_kind,
                "surfaceKind": context.surface_kind,
                "surfaceId": context.surface_id,
                "sectionId": context.section_id,
                "controlId": context.control_id,
                "value": value,
            }),
            cx,
        );
    }

    fn set_native_plugin_slider_from_pointer(
        &mut self,
        key: u64,
        target: WorkspaceImeTarget,
        pointer_x: Pixels,
        min: f32,
        max: f32,
        step: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.text_input_anchors.get(&target.anchor_id()) else {
            return;
        };
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let fraction = (f32::from(pointer_x - anchor.bounds.left()) / width).clamp(0.0, 1.0);
        let raw = min + (max - min) * fraction;
        let value = min + ((raw - min) / step).round() * step;
        self.set_native_plugin_ui_control_value(
            key,
            serde_json::json!(f64::from(value.clamp(min, max))),
            cx,
        );
    }

    fn render_native_plugin_divider_control(&self) -> AnyElement {
        oxideterm_gpui_ui::separator::separator(
            &self.tokens,
            oxideterm_gpui_ui::separator::SeparatorOrientation::Horizontal,
        )
        .into_any_element()
    }

    fn render_native_plugin_text_block_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        code: bool,
    ) -> AnyElement {
        let text = native_plugin_control_text(control);
        if !code {
            let mut options = MarkdownOptions::from_theme(&self.tokens);
            // Plugin markdown may format supplied text but cannot use the host
            // renderer to read local files or trigger background image fetches.
            options.enable_async_images = false;
            options.allowed_image_schemes.clear();
            options.allowed_link_schemes = vec!["http", "https", "mailto"];
            return oxideterm_gpui_markdown::markdown_with_options(&self.tokens, &text, &options);
        }
        oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
                .padding(oxideterm_gpui_ui::SurfacePadding::Compact),
        )
        .w_full()
        .text_size(px(self.tokens.metrics.ui_text_sm))
        .text_color(rgb(self.tokens.ui.text))
        .font_family(settings_mono_font_family(self.settings_store.settings()))
        .child(text)
        .into_any_element()
    }

    fn render_native_plugin_status_badge(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let mut options = oxideterm_gpui_ui::StatusPillOptions::new(native_plugin_ui_status_tone(
            control.tone.as_deref(),
        ));
        if control.size.as_deref() == Some("small") {
            options = options.compact();
        }
        if control.strong {
            options = options.strong();
        }
        oxideterm_gpui_ui::status_pill(
            &self.tokens,
            native_plugin_control_label(control, ""),
            options,
        )
        .into_any_element()
    }

    fn render_native_plugin_progress_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let raw_value = control
            .value
            .as_ref()
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;
        let progress = if raw_value <= 1.0 {
            raw_value * 100.0
        } else {
            raw_value
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one))
            .when_some(
                self.render_native_plugin_control_label(control),
                |view, label| view.child(label),
            )
            .child(oxideterm_gpui_ui::progress::progress(
                &self.tokens,
                Some(progress),
                control.indeterminate,
            ))
            .into_any_element()
    }

    fn render_native_plugin_table_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let rows = Arc::new(control.rows.clone().unwrap_or_default());
        let row_count = rows.len();
        let metrics = oxideterm_gpui_ui::TauriTableMetrics::from_tokens(&self.tokens);
        let spec =
            TauriVirtualListSpec::new(px(metrics.row_min_height), NATIVE_PLUGIN_UI_LIST_OVERSCAN);
        let state = tauri_virtual_list_state(row_count, ListAlignment::Top, spec);
        let height = native_plugin_virtual_list_height(row_count, metrics.row_min_height)
            + metrics.header_min_height;
        let columns = Arc::new(native_plugin_table_columns(control));
        let colors = native_plugin_table_colors(&self.tokens);
        let tokens = self.tokens;
        let mut header = oxideterm_gpui_ui::tauri_table_header(&self.tokens, colors, metrics);
        if columns.is_empty() {
            header = header.child(
                div()
                    .flex_1()
                    .child(native_plugin_control_label(control, "")),
            );
        } else {
            for (_, label, _) in columns.iter() {
                header = header.child(div().min_w_0().flex_1().truncate().child(label.clone()));
            }
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when_some(
                self.render_native_plugin_control_label(control),
                |view, label| view.child(label),
            )
            .child(
                div()
                    .h(px(height))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .overflow_hidden()
                    .child(header)
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, _cx| {
                            let row = rows.get(index).cloned().unwrap_or(serde_json::Value::Null);
                            native_plugin_table_row_element(
                                row,
                                columns.clone(),
                                tokens,
                                colors,
                                metrics,
                            )
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_native_plugin_list_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        let rows = Arc::new(control.rows.clone().unwrap_or_default());
        let row_count = rows.len();
        let row_height = self.tokens.metrics.ui_button_sm_height + self.tokens.spacing.one;
        let spec = TauriVirtualListSpec::new(px(row_height), NATIVE_PLUGIN_UI_LIST_OVERSCAN);
        let state = tauri_virtual_list_state(row_count, ListAlignment::Top, spec);
        let height = native_plugin_virtual_list_height(row_count, row_height);
        let tokens = self.tokens;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when_some(
                self.render_native_plugin_control_label(control),
                |view, label| view.child(label),
            )
            .child(
                div()
                    .h(px(height))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, _cx| {
                            let row = rows.get(index).cloned().unwrap_or(serde_json::Value::Null);
                            native_plugin_list_row_element(row, tokens)
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_native_plugin_empty_state_control(
        &self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
    ) -> AnyElement {
        oxideterm_gpui_ui::empty_state(
            &self.tokens,
            Self::render_lucide_icon(
                LucideIcon::from_plugin_name(control.icon.as_deref().unwrap_or("inbox")),
                self.tokens.metrics.activity_icon_glyph_size,
                rgb(self.tokens.ui.accent),
            ),
            native_plugin_control_label(control, ""),
            control.description.clone(),
            None,
        )
        .min_h(px(self.tokens.metrics.ui_button_lg_height * 2.0))
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
                    .child(native_plugin_control_label(control, "")),
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
    ) -> Option<AnyElement> {
        let label = control.label.clone()?;
        Some(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(label)
                .into_any_element(),
        )
    }
}

fn native_plugin_ui_control_context(
    plugin_id: &str,
    surface_kind: &str,
    surface_id: &str,
    section_id: &str,
    control: &plugin_host::NativePluginDeclarativeUiControl,
) -> NativePluginUiControlContext {
    NativePluginUiControlContext {
        plugin_id: plugin_id.to_string(),
        surface_kind: surface_kind.to_string(),
        surface_id: surface_id.to_string(),
        section_id: section_id.to_string(),
        control_id: control.id.clone().unwrap_or_else(|| control.kind.clone()),
        control_kind: control.kind.clone(),
    }
}

fn native_plugin_button_options(
    control: &plugin_host::NativePluginDeclarativeUiControl,
) -> oxideterm_gpui_ui::button::ButtonOptions {
    let variant = match control.variant.as_deref() {
        Some("secondary") => oxideterm_gpui_ui::button::ButtonVariant::Secondary,
        Some("outline") => oxideterm_gpui_ui::button::ButtonVariant::Outline,
        Some("ghost") => oxideterm_gpui_ui::button::ButtonVariant::Ghost,
        Some("destructive") => oxideterm_gpui_ui::button::ButtonVariant::Destructive,
        Some("link") => oxideterm_gpui_ui::button::ButtonVariant::Link,
        _ => oxideterm_gpui_ui::button::ButtonVariant::Default,
    };
    let size = match control.size.as_deref() {
        Some("small") => oxideterm_gpui_ui::button::ButtonSize::Sm,
        Some("large") => oxideterm_gpui_ui::button::ButtonSize::Lg,
        Some("icon") => oxideterm_gpui_ui::button::ButtonSize::Icon,
        _ => oxideterm_gpui_ui::button::ButtonSize::Default,
    };
    oxideterm_gpui_ui::button::ButtonOptions {
        variant,
        size,
        radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
        disabled: control.disabled || control.loading,
    }
}

fn native_plugin_button_icon_size(tokens: &ThemeTokens, size: Option<&str>) -> f32 {
    match size {
        Some("small") => tokens.metrics.ui_button_sm_height,
        Some("large") => tokens.metrics.ui_button_lg_height,
        _ => tokens.metrics.ui_button_icon_size,
    }
}

fn native_plugin_layout_gap(tokens: &ThemeTokens, gap: Option<&str>) -> f32 {
    match gap {
        Some("none") => 0.0,
        Some("compact") => tokens.spacing.one,
        Some("spacious") => tokens.spacing.three * 2.0,
        _ => tokens.spacing.three,
    }
}

fn native_plugin_card_surface_options(
    control: &plugin_host::NativePluginDeclarativeUiControl,
) -> oxideterm_gpui_ui::SurfaceOptions {
    let kind = match control.variant.as_deref() {
        Some("inset") => oxideterm_gpui_ui::SurfaceKind::InsetGroup,
        Some("inspector") => oxideterm_gpui_ui::SurfaceKind::Inspector,
        _ => oxideterm_gpui_ui::SurfaceKind::Panel,
    };
    oxideterm_gpui_ui::SurfaceOptions::new(kind)
}

fn native_plugin_ui_state_tone(tone: Option<&str>) -> oxideterm_gpui_ui::UiStateTone {
    match tone {
        Some("success") => oxideterm_gpui_ui::UiStateTone::Success,
        Some("warning") => oxideterm_gpui_ui::UiStateTone::Warning,
        Some("error") => oxideterm_gpui_ui::UiStateTone::Error,
        Some("neutral") => oxideterm_gpui_ui::UiStateTone::Muted,
        _ => oxideterm_gpui_ui::UiStateTone::Accent,
    }
}

fn native_plugin_ui_status_tone(tone: Option<&str>) -> oxideterm_gpui_ui::StatusTone {
    match tone {
        Some("accent") => oxideterm_gpui_ui::StatusTone::Accent,
        Some("success") => oxideterm_gpui_ui::StatusTone::Success,
        Some("warning") => oxideterm_gpui_ui::StatusTone::Warning,
        Some("error") => oxideterm_gpui_ui::StatusTone::Error,
        Some("info") => oxideterm_gpui_ui::StatusTone::Info,
        _ => oxideterm_gpui_ui::StatusTone::Neutral,
    }
}

fn native_plugin_ui_tone_color(tokens: &ThemeTokens, tone: Option<&str>) -> u32 {
    match tone {
        Some("success") => tokens.ui.success,
        Some("warning") => tokens.ui.warning,
        Some("error") => tokens.ui.error,
        Some("info") => tokens.ui.info,
        Some("neutral") => tokens.ui.text_muted,
        _ => tokens.ui.accent,
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

fn native_plugin_virtual_list_height(row_count: usize, row_height: f32) -> f32 {
    let visible_rows = row_count.clamp(1, NATIVE_PLUGIN_UI_MAX_VISIBLE_ROWS);
    visible_rows as f32 * row_height
}

fn native_plugin_list_row_element(value: serde_json::Value, tokens: ThemeTokens) -> AnyElement {
    oxideterm_gpui_ui::entity_list_row(
        &tokens,
        oxideterm_gpui_ui::EntityListRowOptions::new().compact(),
        None,
        div()
            .min_w_0()
            .truncate()
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .child(native_plugin_ui_value_label(&value))
            .into_any_element(),
        None,
        Vec::new(),
        Vec::new(),
    )
    .into_any_element()
}

fn native_plugin_table_row_element(
    value: serde_json::Value,
    columns: Arc<Vec<(String, String, String)>>,
    tokens: ThemeTokens,
    colors: oxideterm_gpui_ui::TauriTableColors,
    metrics: oxideterm_gpui_ui::TauriTableMetrics,
) -> AnyElement {
    let mut row = oxideterm_gpui_ui::tauri_table_row(colors, metrics, false);
    if columns.is_empty() {
        return row
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(tokens.metrics.ui_text_sm))
                    .text_color(rgb(tokens.ui.text))
                    .child(native_plugin_ui_value_label(&value)),
            )
            .into_any_element();
    }
    for (index, (column, _, style)) in columns.iter().enumerate() {
        let label = value
            .get(column)
            .map(native_plugin_ui_value_label)
            .unwrap_or_default();
        let cell_style = match style.as_str() {
            "mono" => oxideterm_gpui_ui::TauriTableCellStyle::MetaMono,
            "meta" => oxideterm_gpui_ui::TauriTableCellStyle::Meta,
            _ if index == 0 => oxideterm_gpui_ui::TauriTableCellStyle::Primary,
            _ => oxideterm_gpui_ui::TauriTableCellStyle::Meta,
        };
        row = row.child(oxideterm_gpui_ui::tauri_table_cell(
            &tokens,
            &oxideterm_gpui_ui::TauriTableCellOptions {
                width: NATIVE_PLUGIN_UI_TABLE_COLUMN_WIDTH,
                min_width: NATIVE_PLUGIN_UI_TABLE_COLUMN_MIN_WIDTH,
                flexible: true,
                padding_left: 0.0,
                primary_text_size: tokens.metrics.ui_text_sm,
                meta_text_size: tokens.metrics.ui_text_xs,
                mono_font: Some(SharedString::from(tokens.metrics.font_family)),
            },
            cell_style,
            label,
        ));
    }
    row.into_any_element()
}

fn native_plugin_table_columns(
    control: &plugin_host::NativePluginDeclarativeUiControl,
) -> Vec<(String, String, String)> {
    if let Some(columns) = &control.column_defs {
        return columns
            .iter()
            .map(|column| {
                (
                    column.key.clone(),
                    column.label.clone(),
                    column.style.clone().unwrap_or_else(|| "meta".to_string()),
                )
            })
            .collect();
    }
    control
        .columns
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|column| (column.clone(), column, "meta".to_string()))
        .collect()
}

fn native_plugin_table_colors(tokens: &ThemeTokens) -> oxideterm_gpui_ui::TauriTableColors {
    oxideterm_gpui_ui::TauriTableColors {
        header_border: rgb(tokens.ui.border),
        header_bg: rgb(tokens.ui.bg_sunken),
        row_border: rgba((tokens.ui.border << 8) | 0x66),
        row_hover_bg: rgb(tokens.ui.bg_hover),
        row_selected_bg: rgb(tokens.ui.bg_active),
    }
}

fn native_plugin_password_event_value(
    password: &str,
    raw_access_approved: bool,
) -> serde_json::Value {
    if raw_access_approved {
        // The owning plugin explicitly requested and received raw-credential
        // approval; this is the single plaintext handoff out of host UI state.
        serde_json::Value::String(password.to_string())
    } else {
        serde_json::json!({
            "present": !password.is_empty(),
            "redacted": true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::native_plugin_password_event_value;

    #[test]
    fn password_event_is_redacted_without_sensitive_approval() {
        let representative_secret = "plugin-secret-value";
        let payload = native_plugin_password_event_value(representative_secret, false);

        assert_eq!(payload["present"], true);
        assert_eq!(payload["redacted"], true);
        assert!(!payload.to_string().contains(representative_secret));
    }
}
