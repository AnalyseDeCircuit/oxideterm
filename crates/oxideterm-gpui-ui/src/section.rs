use gpui::{AnyElement, Div, FontWeight, ParentElement, Styled, div, prelude::*, px, rgb};
use oxideterm_theme::ThemeTokens;

use crate::{StatusPillOptions, StatusTone, status_pill};

pub fn page_header(
    tokens: &ThemeTokens,
    title: impl Into<String>,
    description: Option<String>,
    trailing: Option<AnyElement>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_start()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .min_w(px(280.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(tokens.spacing.two))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_2xl))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(rgb(tokens.ui.text_heading))
                        .child(title.into()),
                )
                .when_some(description, |header, description| {
                    header.child(
                        div()
                            .max_w(px(680.0))
                            .text_size(px(tokens.metrics.ui_text_base))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(description),
                    )
                }),
        )
        .when_some(trailing, |header, trailing| header.child(trailing))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SectionHeaderOptions {
    pub description: Option<String>,
    pub count: Option<String>,
    pub compact: bool,
}

impl SectionHeaderOptions {
    pub fn new() -> Self {
        Self {
            description: None,
            count: None,
            compact: false,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn count(mut self, count: impl Into<String>) -> Self {
        self.count = Some(count.into());
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

impl Default for SectionHeaderOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn section_header(
    tokens: &ThemeTokens,
    title: impl Into<String>,
    options: SectionHeaderOptions,
    leading: Option<AnyElement>,
    trailing: Option<AnyElement>,
) -> Div {
    let title_size = if options.compact {
        tokens.metrics.ui_text_sm
    } else {
        tokens.metrics.ui_text_base
    };
    let gap = if options.compact {
        tokens.spacing.two
    } else {
        tokens.spacing.three
    };
    let mut title_row = div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(tokens.spacing.two));
    if let Some(leading) = leading {
        title_row = title_row.child(leading);
    }
    title_row = title_row
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(title_size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(tokens.ui.text_heading))
                .child(title.into()),
        )
        .when_some(options.count.clone(), |row, count| {
            row.child(status_pill(
                tokens,
                count,
                StatusPillOptions::new(StatusTone::Neutral).compact(),
            ))
        });

    let mut header = div()
        .flex()
        .w_full()
        .items_start()
        .justify_between()
        .gap(px(gap))
        .child(
            div()
                .flex()
                .min_w_0()
                .flex_1()
                .flex_col()
                .gap(px(tokens.spacing.one))
                .child(title_row)
                .when_some(options.description, |body, description| {
                    // Descriptions are supporting context, so keep them muted
                    // and below the title instead of competing with actions.
                    body.child(
                        div()
                            .max_w(px(680.0))
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .line_height(px(tokens.metrics.ui_text_sm + 6.0))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(description),
                    )
                }),
        );
    if let Some(trailing) = trailing {
        header = header.child(div().flex_none().child(trailing));
    }
    header
}
