use super::*;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TablePage {
    kind: String,
    pub page: u32,
    pub page_count: u32,
    pub tables: Vec<String>,
    pub selected_table: Option<String>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    row_count: u64,
}

impl TablePage {
    pub(super) fn parse(
        value: serde_json::Value,
        page: u32,
        selected: Option<&str>,
    ) -> Result<Self, ()> {
        let data: Self = serde_json::from_value(value).map_err(|_| ())?;
        let unique = data.tables.iter().collect::<std::collections::HashSet<_>>();
        if data.kind != "table"
            || data.page != page
            || data.page_count == 0
            || data.page_count > 100_000
            || page >= data.page_count
            || data.page_count as u64 != data.row_count.div_ceil(50).max(1)
            || data.tables.len() > 256
            || unique.len() != data.tables.len()
            || data.columns.len() > 64
            || data.rows.len() > 50
            || data
                .tables
                .iter()
                .chain(&data.columns)
                .any(|text| text.len() > 1024)
            || data.rows.iter().any(|row| {
                row.len() != data.columns.len()
                    || row.iter().flatten().any(|cell| cell.len() > 4096)
            })
            || data
                .rows
                .iter()
                .flatten()
                .flatten()
                .map(String::len)
                .sum::<usize>()
                > 4 * 1024 * 1024
            || selected.is_some_and(|name| data.selected_table.as_deref() != Some(name))
        {
            return Err(());
        }
        match &data.selected_table {
            Some(name) if data.tables.contains(name) && !data.columns.is_empty() => {}
            None if data.tables.is_empty()
                && data.columns.is_empty()
                && data.rows.is_empty()
                && data.row_count == 0 => {}
            _ => return Err(()),
        }
        let expected = data.row_count.saturating_sub(u64::from(page) * 50).min(50) as usize;
        if data.rows.len() != expected {
            return Err(());
        }
        Ok(data)
    }
}

impl PluginFilePreview {
    pub(super) fn render_database(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .p_3()
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(self.tokens.metrics.ui_text_sm));
        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(self.button(
                self.i18n.t("file_preview.previous"),
                self.loading || self.page == 0,
                |this, cx| {
                    this.page -= 1;
                    this.render_page(cx);
                },
                cx,
            ))
            .child(format!(
                "{} / {}",
                if self.page_count == 0 {
                    0
                } else {
                    self.page + 1
                },
                self.page_count
            ))
            .child(self.button(
                self.i18n.t("file_preview.next"),
                self.loading || self.page + 1 >= self.page_count,
                |this, cx| {
                    this.page += 1;
                    this.render_page(cx);
                },
                cx,
            ))
            .child(self.button(
                self.i18n.t("file_preview.retry"),
                self.loading,
                |this, cx| {
                    this.page = 0;
                    this.selected_table = None;
                    this.render_page(cx);
                },
                cx,
            ));
        body = body.child(toolbar);
        if let Some(key) = self.error_key {
            return body.child(div().py_4().child(self.i18n.t(key)));
        }
        if self.loading {
            body = body.child(self.i18n.t("file_preview.loading"));
        }
        let Some(table) = self.table.clone() else {
            return body;
        };
        if table.tables.is_empty() {
            return body.child(self.i18n.t("file_preview.no_tables"));
        }
        let mut tables = div()
            .id("database-tables")
            .w(px(180.0))
            .flex_none()
            .h(px(420.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.i18n.t("file_preview.tables"));
        for (index, name) in table.tables.iter().enumerate() {
            let selected = table.selected_table.as_ref() == Some(name);
            let name = name.clone();
            tables = tables.child(
                div()
                    .id(("database-table", index))
                    .w_full()
                    .min_w_0()
                    .bg(if selected {
                        rgb(self.tokens.ui.bg_active)
                    } else {
                        rgba(0)
                    })
                    .child(
                        self.button(
                            name.clone(),
                            self.loading,
                            move |this, cx| {
                                this.selected_table = Some(name.clone());
                                this.page = 0;
                                this.render_page(cx);
                            },
                            cx,
                        )
                        .w_full()
                        .overflow_hidden(),
                    ),
            );
        }
        let tokens = self.tokens;
        let metrics = oxideterm_gpui_ui::TauriTableMetrics::from_tokens(&tokens);
        let colors = oxideterm_gpui_ui::TauriTableColors {
            header_border: rgb(tokens.ui.border),
            header_bg: rgb(tokens.ui.bg_sunken),
            row_border: rgb(tokens.ui.border),
            row_hover_bg: rgb(tokens.ui.bg_hover),
            row_selected_bg: rgb(tokens.ui.bg_active),
        };
        let width = (table.columns.len() as f32 * 180.0).max(180.0) + metrics.padding_x * 2.0;
        let header = oxideterm_gpui_ui::tauri_table_header(&tokens, colors, metrics).children(
            table.columns.iter().map(|name| {
                div()
                    .w(px(180.0))
                    .flex_none()
                    .px_2()
                    .truncate()
                    .child(name.clone())
            }),
        );
        let rows = table.clone();
        let spec = TauriVirtualListSpec::new(px(metrics.row_min_height), 4);
        let state = self
            .table_scroll
            .clone()
            .unwrap_or_else(|| tauri_virtual_list_state(0, ListAlignment::Top, spec));
        let grid = div()
            .id("database-grid")
            .min_w_0()
            .w_full()
            .overflow_x_scroll()
            .child(
                div()
                    .w(px(width))
                    .h(px(420.0))
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(tauri_virtual_list(state, spec, move |index, _, _| {
                        let mut row = oxideterm_gpui_ui::tauri_table_row(colors, metrics, false);
                        if let Some(cells) = rows.rows.get(index) {
                            for cell in cells {
                                row = row.child(
                                    div()
                                        .w(px(180.0))
                                        .flex_none()
                                        .px_2()
                                        .truncate()
                                        .text_color(rgb(if cell.is_some() {
                                            tokens.ui.text
                                        } else {
                                            tokens.ui.text_muted
                                        }))
                                        .child(cell.clone().unwrap_or_else(|| "NULL".into())),
                                );
                            }
                        }
                        row.into_any_element()
                    })),
            );
        body.child(
            self.i18n
                .t("file_preview.rows")
                .replace("{{count}}", &table.row_count.to_string()),
        )
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .gap_3()
                .child(tables)
                .child(div().flex_1().min_w_0().child(grid)),
        )
        .child(
            div()
                .text_color(rgb(tokens.ui.text_muted))
                .child(self.i18n.t("file_preview.database_hint")),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn table_preview_validates_plugin_rows_and_requested_table() {
        let value = json!({"kind":"table","page":0,"pageCount":1,"tables":["items"],"selectedTable":"items","columns":["id","name"],"rows":[["42",null]],"rowCount":1});
        let page = TablePage::parse(value.clone(), 0, Some("items"))
            .unwrap_or_else(|_| panic!("valid page"));
        assert_eq!(page.columns, ["id", "name"]);
        assert_eq!(page.rows, vec![vec![Some("42".into()), None]]);
        for (key, invalid) in [
            ("rows", json!([["42"]])),
            ("rows", json!([["42", "a".repeat(4097)]])),
            ("rowCount", json!(51)),
            ("tables", json!(["items", "items"])),
            ("selectedTable", json!("different")),
            ("page", json!(1)),
        ] {
            let mut malformed = value.clone();
            malformed[key] = invalid;
            assert!(
                TablePage::parse(malformed, 0, Some("items")).is_err(),
                "{key}"
            );
        }
    }
}
