use super::*;
use oxideterm_gpui_editor::{EditorPresentation, TextEditorView};
use oxideterm_gpui_ui::button::{ButtonOptions, ButtonSize, ButtonVariant};
#[cfg(feature = "plugin-wasm-runtime")]
use std::time::Duration;

const MAX_TEXT_BYTES: usize = 256 * 1024;

/// Editors belong to the open surface; raw text never enters contribution metadata.
pub(super) struct PluginTextWorkbench {
    plugin: plugin_host::NativePluginInfo,
    control: plugin_host::NativePluginDeclarativeUiControl,
    runtime: Arc<tokio::runtime::Runtime>,
    tokens: ThemeTokens,
    i18n: I18n,
    input: Entity<TextEditorView>,
    output: Entity<TextEditorView>,
    parameter: Entity<TextEditorView>,
    selected: usize,
    error: Option<String>,
    loading: bool,
    job: Option<tokio::task::AbortHandle>,
    delivery: Option<gpui::Task<()>>,
}

impl PluginTextWorkbench {
    pub(super) fn update_definition(
        &mut self,
        plugin: plugin_host::NativePluginInfo,
        control: plugin_host::NativePluginDeclarativeUiControl,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) {
        if self.tokens != tokens || self.i18n.locale() != i18n.locale() {
            for editor in [&self.input, &self.output, &self.parameter] {
                editor.update(cx, |editor, cx| {
                    editor.set_context_menu_labels(Self::editor_labels(&i18n));
                    editor.apply_runtime_settings(
                        &tokens,
                        tokens.metrics.markdown_code_font_family.to_string(),
                        tokens.metrics.markdown_body_font_size
                            * tokens.metrics.markdown_code_font_scale,
                        1.5,
                        false,
                        false,
                        cx,
                    );
                });
            }
            self.tokens = tokens;
            self.i18n = i18n;
        }
        self.plugin = plugin;
        self.control = control;
        self.selected = self.selected.min(
            self.control
                .options
                .as_ref()
                .map_or(0, |items| items.len().saturating_sub(1)),
        );
    }
    fn editor(
        text: String,
        read_only: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Entity<TextEditorView> {
        cx.new(|cx| {
            let mut editor = TextEditorView::new_verbatim(text, tokens, cx);
            editor.set_presentation(EditorPresentation::Inline, cx);
            editor.set_read_only(read_only);
            editor.set_context_menu_labels(Self::editor_labels(i18n));
            editor
        })
    }

    fn editor_labels(i18n: &I18n) -> oxideterm_gpui_editor::EditorContextMenuLabels {
        oxideterm_gpui_editor::EditorContextMenuLabels {
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        }
    }

    pub(super) fn new(
        plugin: plugin_host::NativePluginInfo,
        control: plugin_host::NativePluginDeclarativeUiControl,
        runtime: Arc<tokio::runtime::Runtime>,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            plugin,
            control,
            runtime,
            tokens,
            input: Self::editor(String::new(), false, &tokens, &i18n, cx),
            output: Self::editor(String::new(), true, &tokens, &i18n, cx),
            parameter: Self::editor(String::new(), false, &tokens, &i18n, cx),
            i18n,
            selected: 0,
            error: None,
            loading: false,
            job: None,
            delivery: None,
        }
    }

    fn label(&self, key: &str) -> String {
        self.control
            .value
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(key)
            .to_string()
    }

    pub(super) fn set_input(&mut self, text: Zeroizing<String>, cx: &mut Context<Self>) {
        if text.len() > MAX_TEXT_BYTES {
            self.error = Some(self.label("limit"));
        } else {
            if let Some(job) = self.job.take() {
                job.abort();
            }
            self.delivery = None;
            self.loading = false;
            // Replacing the editor drops the old draft and undo history together.
            self.input = Self::editor(text.to_string(), false, &self.tokens, &self.i18n, cx);
            self.error = None;
        }
        cx.notify();
    }

    fn button(
        &self,
        label: String,
        disabled: bool,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> gpui::Div {
        oxideterm_gpui_ui::button::button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                disabled,
                ..Default::default()
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                if !disabled {
                    action(this, cx);
                }
                cx.stop_propagation();
            }),
        )
    }

    fn execute(&mut self, cx: &mut Context<Self>) {
        let Some(tool) = self
            .control
            .options
            .as_ref()
            .and_then(|tools| tools.get(self.selected))
        else {
            return;
        };
        let Some(command) = tool.value["command"].as_str() else {
            return;
        };
        if self.input.read(cx).buffer().len() > MAX_TEXT_BYTES
            || self.parameter.read(cx).buffer().len() > 4096
        {
            self.error = Some(self.label("limit"));
            cx.notify();
            return;
        }
        let command = command.to_string();
        let input = Zeroizing::new(self.input.read(cx).buffer().text());
        let parameter = Zeroizing::new(self.parameter.read(cx).buffer().text());
        let plugin = self.plugin.clone();
        self.error = None;
        self.loading = true;
        let task = self
            .runtime
            .spawn(async move { transform(plugin, command, input, parameter).await });
        self.job = Some(task.abort_handle());
        self.delivery = Some(cx.spawn(async move |view, cx| {
            let result = task.await.unwrap_or(Err(("failed".into(), 0, 0)));
            let _ = view.update(cx, |this, cx| {
                this.loading = false;
                this.job = None;
                match result {
                    Ok(text) => {
                        this.output =
                            Self::editor(text.to_string(), true, &this.tokens, &this.i18n, cx)
                    }
                    Err((code, line, column)) => {
                        let message = this
                            .control
                            .value
                            .as_ref()
                            .and_then(|labels| labels.get(&code))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_else(|| this.label("failed"));
                        this.error = Some(if line > 0 {
                            format!(
                                "{message} · {} {line}, {} {column}",
                                this.label("line"),
                                this.label("column")
                            )
                        } else {
                            message
                        });
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select_tool(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tools) = self.control.options.as_ref() else {
            return;
        };
        let Some(tool) = tools.get(index) else { return };
        let previous = tools
            .get(self.selected)
            .map(|tool| &tool.value["parameterLabel"]);
        if previous != Some(&tool.value["parameterLabel"]) {
            let value = tool.value["parameterDefault"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            self.parameter = Self::editor(value, false, &self.tokens, &self.i18n, cx);
        }
        self.selected = index;
        self.error = None;
        cx.notify();
    }
}

impl Drop for PluginTextWorkbench {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            job.abort();
        }
    }
}

impl Render for PluginTextWorkbench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tools = div().w(px(180.0)).flex_none().flex().flex_col().gap_1();
        let definitions = self.control.options.clone().unwrap_or_default();
        let group = |tool: &plugin_host::NativePluginDeclarativeUiOption| {
            tool.value["group"]
                .as_str()
                .unwrap_or(&tool.label)
                .to_string()
        };
        let active_group = definitions
            .get(self.selected)
            .map(&group)
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut operations = div().w_full().flex().flex_wrap().gap_2();
        for (index, tool) in definitions.iter().enumerate() {
            let name = group(tool);
            if name == active_group {
                operations = operations.child(
                    div()
                        .bg(if self.selected == index {
                            rgb(self.tokens.ui.bg_active)
                        } else {
                            rgba(0)
                        })
                        .child(self.button(
                            tool.label.clone(),
                            self.loading,
                            move |this, cx| this.select_tool(index, cx),
                            cx,
                        )),
                );
            }
            if !seen.insert(name.clone()) {
                continue;
            }
            tools = tools.child(
                div()
                    .bg(if active_group == name {
                        rgb(self.tokens.ui.bg_active)
                    } else {
                        rgba(0)
                    })
                    .child(
                        self.button(
                            name,
                            self.loading,
                            move |this, cx| {
                                this.select_tool(index, cx);
                            },
                            cx,
                        )
                        .w_full()
                        .justify_start(),
                    ),
            );
        }
        let mut options = div().w_full().flex().flex_wrap().items_center().gap_3();
        if let Some(label) = self
            .control
            .options
            .as_ref()
            .and_then(|tools| tools.get(self.selected))
            .and_then(|tool| tool.value["parameterLabel"].as_str())
        {
            options = options.child(label.to_string()).child(
                div()
                    .w(px(240.0))
                    .h(px(self.tokens.metrics.ui_control_height))
                    .child(self.parameter.clone()),
            );
        }
        options = options.child(self.button(
            self.label(if self.loading { "working" } else { "execute" }),
            self.loading,
            |this, cx| this.execute(cx),
            cx,
        ));
        let input = div()
            .flex_1()
            .min_w_0()
            .flex_basis(px(320.0))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.label("input"))
            .child(
                div()
                    .w_full()
                    .h(px(360.0))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(self.input.clone()),
            );
        let output_actions = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(self.button(
                self.label("copy"),
                false,
                |this, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        this.output.read(cx).buffer().text(),
                    ));
                },
                cx,
            ))
            .child(self.button(
                self.label("useInput"),
                self.loading,
                |this, cx| {
                    let text = Zeroizing::new(this.output.read(cx).buffer().text());
                    this.set_input(text, cx);
                },
                cx,
            ))
            .child(self.button(
                self.label("clear"),
                self.loading,
                |this, cx| {
                    this.output = Self::editor(String::new(), true, &this.tokens, &this.i18n, cx);
                    this.error = None;
                    cx.notify();
                },
                cx,
            ));
        let output = div()
            .flex_1()
            .min_w_0()
            .flex_basis(px(320.0))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.label("output"))
            .child(
                div()
                    .w_full()
                    .h(px(360.0))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(self.output.clone()),
            )
            .child(output_actions);
        let mut main = div()
            .flex_1()
            .min_w_0()
            .flex_basis(px(360.0))
            .flex()
            .flex_col()
            .gap_3()
            .child(operations)
            .child(options);
        if let Some(description) = definitions
            .get(self.selected)
            .and_then(|tool| tool.value["description"].as_str())
        {
            main = main.child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(description.to_string()),
            );
        }
        if let Some(error) = &self.error {
            main = main.child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(error.clone()),
            );
        }
        main = main.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .items_start()
                .gap_3()
                .child(input)
                .child(output),
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_start()
            .gap_4()
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(tools)
            .child(main)
    }
}

#[cfg(feature = "plugin-wasm-runtime")]
async fn transform(
    plugin: plugin_host::NativePluginInfo,
    command: String,
    input: Zeroizing<String>,
    parameter: Zeroizing<String>,
) -> Result<Zeroizing<String>, (String, u64, u64)> {
    use oxideterm_plugin_host_api::runtime::{
        NativeWasmPluginRuntime, PluginActivateRequest, PluginPermissionSet, PluginRequest,
        PluginRequestKind, PluginResponseResult,
    };
    let fail = || ("failed".into(), 0, 0);
    let plugin_host::NativePluginRuntimePlan::Wasm { entry } = plugin.runtime_plan else {
        return Err(fail());
    };
    let mut runtime = NativeWasmPluginRuntime::new(
        &plugin.manifest.id,
        plugin.install_dir,
        entry,
        Duration::from_secs(5),
    );
    runtime
        .activate(PluginActivateRequest {
            request_id: "text-activate".into(),
            manifest: plugin.manifest,
            permissions: PluginPermissionSet::default(),
            timeout_ms: 5000,
        })
        .await
        .map_err(|_| fail())?;
    // This isolated invocation has no host resolver, preopened directories, or network access.
    let mut response = runtime
        .call(PluginRequest {
            request_id: "text-transform".into(),
            kind: PluginRequestKind::DispatchCommand {
                command,
                args: serde_json::json!({"input":input.as_str(),"parameter":parameter.as_str()}),
            },
            timeout_ms: Some(5000),
        })
        .await
        .map_err(|_| fail())?;
    runtime.kill().await.map_err(|_| fail())?;
    let result = match &mut response.result {
        PluginResponseResult::Ok { value } => {
            if let Some(output) = value.get_mut("output").and_then(|value| match value {
                serde_json::Value::String(text) => Some(text),
                _ => None,
            }) {
                if output.len() > 2 * 1024 * 1024 {
                    Err(("limit".into(), 0, 0))
                } else {
                    Ok(Zeroizing::new(std::mem::take(output)))
                }
            } else {
                let code = value["error"]["code"].as_str().unwrap_or("failed");
                Err((
                    if code.len() <= 64
                        && code
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    {
                        code
                    } else {
                        "failed"
                    }
                    .into(),
                    value["error"]["line"].as_u64().unwrap_or(0),
                    value["error"]["column"].as_u64().unwrap_or(0),
                ))
            }
        }
        _ => Err(fail()),
    };
    if let PluginResponseResult::Ok { value } = &mut response.result {
        oxideterm_plugin_protocol::zeroize_json_value(value);
    }
    result
}

#[cfg(not(feature = "plugin-wasm-runtime"))]
async fn transform(
    _: plugin_host::NativePluginInfo,
    _: String,
    _: Zeroizing<String>,
    _: Zeroizing<String>,
) -> Result<Zeroizing<String>, (String, u64, u64)> {
    Err(("failed".into(), 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn tool_options_keep_related_parameters_and_reset_unrelated_defaults(
        cx: &mut gpui::TestAppContext,
    ) {
        let plugin = plugin_host::NativePluginInfo {
            manifest: serde_json::from_value(
                serde_json::json!({"id":"test.tools","name":"Tools","version":"1.0.0"}),
            )
            .unwrap(),
            install_dir: Default::default(),
            runtime_plan: plugin_host::NativePluginRuntimePlan::Wasm {
                entry: "plugin.wasm".into(),
            },
            state: plugin_host::NativePluginState::Active,
            config: Default::default(),
        };
        let control=serde_json::from_value(serde_json::json!({"kind":"textWorkbench","id":"tools","options":[
            {"label":"Hash","value":{"command":"hash.sha256","group":"Hash"}},
            {"label":"Seconds","value":{"command":"time.seconds","group":"Time","parameterLabel":"Zone","parameterDefault":"UTC"}},
            {"label":"Milliseconds","value":{"command":"time.millis","group":"Time","parameterLabel":"Zone","parameterDefault":"UTC"}},
            {"label":"Random","value":{"command":"random.hex","group":"Generate","parameterLabel":"Length","parameterDefault":"32"}}
        ]})).unwrap();
        let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let view = cx.new(|cx| {
            PluginTextWorkbench::new(
                plugin,
                control,
                runtime,
                oxideterm_theme::default_tokens(),
                I18n::new(oxideterm_i18n::Locale::En),
                cx,
            )
        });
        view.update(cx, |view, cx| {
            view.set_input(Zeroizing::new("原文\r\n".into()), cx);
            view.select_tool(1, cx);
            assert_eq!(view.parameter.read(cx).buffer().text(), "UTC");
            view.parameter.update(cx, |editor, cx| {
                editor.replace_text_external("Asia/Shanghai", cx)
            });
            view.select_tool(2, cx);
            assert_eq!(view.parameter.read(cx).buffer().text(), "Asia/Shanghai");
            view.select_tool(3, cx);
            assert_eq!(view.parameter.read(cx).buffer().text(), "32");
            assert_eq!(view.input.read(cx).buffer().text(), "原文\r\n");
        });
    }
}
