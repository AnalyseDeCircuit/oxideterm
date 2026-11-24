//! Window overlays for the container and Redis panels.
use super::*;
use gpui::{Entity, Render};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
    ToolbarButtonOptions, toolbar_action_button,
};
use std::hash::{Hash, Hasher};

mod container;
mod redis;
mod redis_create;

pub trait HostToolDialogWindow {
    fn input(
        &self,
        input: HostToolsTextInput,
        value: &str,
        placeholder: String,
        height: f32,
        cx: &mut App,
    ) -> AnyElement;
    fn scalar_editor(
        &self,
        value: &str,
        height: f32,
        scroll: &gpui::ScrollHandle,
        cx: &mut App,
    ) -> AnyElement;
    fn selection(
        &self,
        group: u64,
        scope: &'static str,
        order: usize,
        text: String,
        color: u32,
        font: Option<SharedString>,
        cx: &mut App,
    ) -> AnyElement;
    fn clear_composition(&self, cx: &mut App);
    fn submit_container(&self, window: &mut Window, cx: &mut App);
}

pub struct HostToolsDialogs {
    host_tools: Entity<HostToolsEntity>,
    window: Box<dyn HostToolDialogWindow>,
    tokens: ThemeTokens,
    i18n: I18n,
    mono_font: SharedString,
    focus: Option<ConfirmDialogAction>,
}

impl HostToolsDialogs {
    pub fn new(
        host_tools: Entity<HostToolsEntity>,
        window: Box<dyn HostToolDialogWindow>,
        tokens: ThemeTokens,
        i18n: I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&host_tools, |_, _, cx| cx.notify()).detach();
        Self {
            host_tools,
            window,
            tokens,
            i18n,
            mono_font,
            focus: None,
        }
    }

    pub fn sync_window(
        &mut self,
        tokens: ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        focus: Option<ConfirmDialogAction>,
    ) {
        self.tokens = tokens;
        if self.i18n.locale() != i18n.locale() {
            self.i18n = i18n.clone();
        }
        self.mono_font = mono_font;
        self.focus = focus;
    }

    pub fn visible(host: &HostToolsEntity) -> bool {
        host.ui.host_container_input_dialog.is_some()
            || host.host_container.text_dialog.is_some()
            || host.redis_value_dialog_open()
            || host.redis_delete_open()
            || host.redis_create_key_open()
    }

    fn input(
        &self,
        input: HostToolsTextInput,
        value: impl AsRef<str>,
        placeholder: String,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.window
            .input(input, value.as_ref(), placeholder, height, cx)
    }

    fn footer_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        focused: Option<ConfirmDialogAction>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        toolbar_action_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                focus_visible: focused == Some(action),
                ..Default::default()
            },
            cx.listener(listener),
        )
    }
}

impl Render for HostToolsDialogs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let delete = self.host_tools.update(cx, |host, cx| {
            host.render_host_redis_delete_dialog(&self.tokens, &self.i18n, self.focus, cx)
        });
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(self.render_host_container_text_dialog(cx))
            .children(self.render_host_container_input_dialog(cx))
            .children(self.render_host_redis_value_dialog(window, cx))
            .children(delete)
            .children(self.render_host_redis_create_key_dialog(cx))
    }
}

fn selection_group(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}
