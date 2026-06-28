use gpui::{AnyElement, Div, ParentElement, Styled, div, px};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionSlotRowAlignment {
    Start,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionSlotRowOptions {
    pub alignment: ActionSlotRowAlignment,
    pub gap: Option<f32>,
    pub trailing_gap: Option<f32>,
}

impl ActionSlotRowOptions {
    pub const fn new() -> Self {
        Self {
            alignment: ActionSlotRowAlignment::Center,
            gap: None,
            trailing_gap: None,
        }
    }

    pub const fn align_start(mut self) -> Self {
        self.alignment = ActionSlotRowAlignment::Start;
        self
    }

    pub const fn align_center(mut self) -> Self {
        self.alignment = ActionSlotRowAlignment::Center;
        self
    }

    pub const fn gap(mut self, gap: f32) -> Self {
        self.gap = Some(gap);
        self
    }

    pub const fn trailing_gap(mut self, gap: f32) -> Self {
        self.trailing_gap = Some(gap);
        self
    }
}

impl Default for ActionSlotRowOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn action_slot_row(
    tokens: &ThemeTokens,
    options: ActionSlotRowOptions,
    leading: Option<AnyElement>,
    body: AnyElement,
    trailing: Vec<AnyElement>,
) -> Div {
    let gap = options.gap.unwrap_or(tokens.spacing.two);
    let trailing_gap = options.trailing_gap.unwrap_or(tokens.spacing.two);

    let row = div().w_full().min_w_0().flex().gap(px(gap));
    let mut row = match options.alignment {
        ActionSlotRowAlignment::Start => row.items_start(),
        ActionSlotRowAlignment::Center => row.items_center(),
    };

    if let Some(leading) = leading {
        row = row.child(div().flex_none().child(leading));
    }

    // The body is the only flexible slot. Keeping this rule centralized avoids
    // min-content measurement bugs where text wraps as one character per line.
    row = row.child(div().min_w_0().flex_1().child(body));

    if !trailing.is_empty() {
        row = row.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(trailing_gap))
                .children(trailing),
        );
    }

    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_slot_row_options_default_to_center_alignment() {
        let options = ActionSlotRowOptions::default();

        assert_eq!(options.alignment, ActionSlotRowAlignment::Center);
        assert_eq!(options.gap, None);
        assert_eq!(options.trailing_gap, None);
    }

    #[test]
    fn action_slot_row_options_are_chainable() {
        let options = ActionSlotRowOptions::new()
            .align_start()
            .gap(10.0)
            .trailing_gap(8.0);

        assert_eq!(options.alignment, ActionSlotRowAlignment::Start);
        assert_eq!(options.gap, Some(10.0));
        assert_eq!(options.trailing_gap, Some(8.0));
    }
}
