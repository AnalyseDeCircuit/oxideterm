use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, MouseDownEvent, Window};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
    toolbar_action_button,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

pub(super) struct DialogRenderer<'a, A> {
    pub tokens: &'a ThemeTokens,
    pub i18n: &'a I18n,
    pub dispatch: Rc<dyn Fn(A, &mut Window, &mut App)>,
}

impl<A: Copy + 'static> DialogRenderer<'_, A> {
    pub(super) fn on_action(
        &self,
        action: A,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static + use<A> {
        let dispatch = self.dispatch.clone();
        move |_, window, cx| {
            dispatch(action, window, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn button(
        &self,
        label: String,
        variant: ButtonVariant,
        disabled: bool,
        action: A,
    ) -> AnyElement {
        toolbar_action_button(
            self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                height: Some(self.tokens.metrics.form_button_height),
                padding_x: Some(self.tokens.metrics.form_button_padding_x),
                font_size: Some(self.tokens.metrics.form_text_font_size),
                ..ToolbarButtonOptions::default()
            },
            self.on_action(action),
        )
        .into_any_element()
    }
}
