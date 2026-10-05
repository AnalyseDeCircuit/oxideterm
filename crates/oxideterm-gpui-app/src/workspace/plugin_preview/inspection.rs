use super::*;

#[derive(serde::Deserialize)]
pub(super) struct InspectionField {
    key: String,
    value: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BinarySection {
    name: String,
    offset: Option<u64>,
    size: u64,
    address: u64,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InspectionPage {
    kind: String,
    index: u32,
    pub objects: Vec<String>,
    fields: Vec<InspectionField>,
    #[serde(default)]
    details: Vec<InspectionField>,
    #[serde(default)]
    sections: Vec<BinarySection>,
    size: Option<u64>,
    not_before: Option<i64>,
    not_after: Option<i64>,
}

impl InspectionPage {
    fn validity_status(&self, now: i64) -> &'static str {
        if now < self.not_before.unwrap_or(i64::MAX) {
            "not_yet_valid"
        } else if now > self.not_after.unwrap_or(i64::MIN) {
            "expired"
        } else {
            "within_validity"
        }
    }

    pub(super) fn parse(
        value: serde_json::Value,
        index: u32,
        certificate: bool,
    ) -> Result<Self, ()> {
        let page: Self = serde_json::from_value(value).map_err(|_| ())?;
        const KEYS: &[&str] = &[
            "subject",
            "issuer",
            "not_before",
            "not_after",
            "domains",
            "public_key_algorithm",
            "signature_algorithm",
            "sha256",
            "sha1",
            "key_usage",
            "extended_key_usage",
            "version",
            "serial",
            "constraints",
            "extensions",
            "critical_extensions",
            "format",
            "architecture",
            "bitness",
            "byte_order",
            "entry_point",
            "entry_file_offset",
            "file_size",
        ];
        if page.kind != if certificate { "certificate" } else { "binary" }
            || page.index != index
            || page.objects.is_empty()
            || page.objects.len() > 64
            || index as usize >= page.objects.len()
            || page.objects.iter().any(|name| name.len() > 1024)
            || page.fields.len() + page.details.len() > 64
            || page
                .fields
                .iter()
                .chain(&page.details)
                .any(|field| !KEYS.contains(&field.key.as_str()) || field.value.len() > 16384)
            || page.sections.len() > 1024
            || page
                .sections
                .iter()
                .any(|section| section.name.len() > 1024)
        {
            return Err(());
        }
        if certificate {
            if page
                .not_before
                .zip(page.not_after)
                .is_none_or(|(before, after)| before > after)
                || !page.sections.is_empty()
                || page.size.is_some()
            {
                return Err(());
            }
        } else {
            let size = page
                .size
                .filter(|size| *size <= 10 * 1024 * 1024)
                .ok_or(())?;
            if page.sections.iter().any(|section| {
                section
                    .offset
                    .is_some_and(|offset| offset > size || section.size > size - offset)
            }) {
                return Err(());
            }
        }
        Ok(page)
    }
}

impl PluginFilePreview {
    fn inspection_fields(&self, fields: &[InspectionField], cx: &Context<Self>) -> gpui::Div {
        let mut content = div().w_full().min_w_0().flex().flex_col().gap_3();
        for field in fields {
            let label = self.i18n.t(&format!("file_preview.{}", field.key));
            let value = if (field.key == "byte_order"
                && matches!(field.value.as_str(), "little_endian" | "big_endian"))
                || (field.key == "format" && field.value == "unknown_format")
            {
                self.i18n.t(&format!("file_preview.{}", field.value))
            } else {
                field.value.clone()
            };
            let copy = value.clone();
            content = content.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .w(px(135.0))
                            .flex_none()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(label),
                    )
                    .child(div().flex_1().min_w_0().overflow_hidden().child(value))
                    .child(self.button(
                        self.i18n.t("menu.copy"),
                        false,
                        move |_, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()))
                        },
                        cx,
                    )),
            );
        }
        content
    }

    pub(super) fn render_inspection(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text))
            .child(self.button(
                self.i18n.t("file_preview.retry"),
                self.loading,
                |this, cx| {
                    this.page = 0;
                    this.render_page(cx);
                },
                cx,
            ));
        if let Some(error) = self.error_key {
            return body.child(self.i18n.t(error));
        }
        if self.loading {
            body = body.child(self.i18n.t("file_preview.loading"));
        }
        let Some(page) = self.inspection.clone() else {
            return body;
        };
        let certificate = page.kind == "certificate";
        let mut objects = div()
            .id("inspection-objects")
            .w(px(190.0))
            .flex_none()
            .max_h(px(520.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for (index, label) in page.objects.iter().enumerate() {
            let label = if label == "unknown_format" {
                self.i18n.t("file_preview.unknown_format")
            } else {
                label.clone()
            };
            objects = objects.child(
                div()
                    .id(("inspection-object", index))
                    .w_full()
                    .min_w_0()
                    .bg(if self.page as usize == index {
                        rgb(self.tokens.ui.bg_active)
                    } else {
                        rgba(0)
                    })
                    .child(
                        self.button(
                            format!("{}. {}", index + 1, label),
                            self.loading,
                            move |this, cx| {
                                this.page = index as u32;
                                this.render_page(cx);
                            },
                            cx,
                        )
                        .w_full()
                        .overflow_hidden(),
                    ),
            );
        }
        let labels = if certificate {
            vec!["basic_info", "details"]
        } else {
            vec!["overview", "sections", "hex"]
        };
        let mut tabs = oxideterm_gpui_ui::tabs::tabs_list(&self.tokens);
        for (index, key) in labels.iter().enumerate() {
            tabs = tabs.child(
                oxideterm_gpui_ui::tabs::tabs_trigger(
                    &self.tokens,
                    self.i18n.t(&format!("file_preview.{key}")),
                    self.inspection_tab == index,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if !this.loading {
                            this.inspection_tab = index;
                            if index == 2 {
                                this.load_inspection_hex(this.hex_offset, cx);
                            }
                            cx.notify();
                        }
                    }),
                ),
            );
        }
        let mut content = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(tabs);
        if certificate {
            let now = chrono::Utc::now().timestamp();
            let status = page.validity_status(now);
            content = content
                .child(self.i18n.t(&format!("file_preview.{status}")))
                .child(self.i18n.t("file_preview.certificate_scope"))
                .child(self.inspection_fields(
                    if self.inspection_tab == 0 {
                        &page.fields
                    } else {
                        &page.details
                    },
                    cx,
                ));
        } else if self.inspection_tab == 0 {
            content = content.child(self.inspection_fields(&page.fields, cx));
        } else if self.inspection_tab == 1 {
            let mut sections = div()
                .id("inspection-sections")
                .w_full()
                .max_h(px(480.0))
                .overflow_y_scroll()
                .flex()
                .flex_col();
            for section in &page.sections {
                let offset = section.offset;
                sections = sections.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(section.name.clone()),
                        )
                        .child(format!("0x{:X} · {} B", section.address, section.size))
                        .child(
                            self.button(
                                offset
                                    .map(|value| format!("0x{value:X}"))
                                    .unwrap_or_else(|| "—".into()),
                                self.loading || offset.is_none() || section.size == 0,
                                move |this, cx| {
                                    if let Some(offset) = offset {
                                        this.inspection_tab = 2;
                                        this.load_inspection_hex(offset, cx);
                                    }
                                },
                                cx,
                            ),
                        ),
                );
            }
            content = content
                .child(self.i18n.t("file_preview.sections_hint"))
                .child(sections);
        } else {
            let mut controls = div().w_full().flex().items_center().gap_2();
            if let Some(editor) = &self.offset_editor {
                controls = controls.child(
                    div()
                        .w(px(170.0))
                        .h(px(self.tokens.metrics.ui_control_height))
                        .child(editor.clone()),
                );
            }
            controls = controls.child(self.button(
                self.i18n.t("file_preview.jump"),
                self.loading,
                |this, cx| {
                    let text = this
                        .offset_editor
                        .as_ref()
                        .map(|editor| editor.read(cx).buffer().text())
                        .unwrap_or_default();
                    let text = text.trim();
                    let offset = if let Some(value) =
                        text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
                    {
                        u64::from_str_radix(value, 16)
                    } else {
                        text.parse::<u64>()
                    };
                    match offset {
                        Ok(offset) => this.load_inspection_hex(offset, cx),
                        Err(_) => {
                            this.offset_error = true;
                            cx.notify();
                        }
                    }
                },
                cx,
            ));
            controls = controls
                .child(self.button(
                    self.i18n.t("file_preview.previous"),
                    self.loading || self.hex_offset == 0,
                    |this, cx| this.load_inspection_hex(this.hex_offset.saturating_sub(512), cx),
                    cx,
                ))
                .child(self.button(
                    self.i18n.t("file_preview.next"),
                    self.loading || self.hex_offset + 512 >= page.size.unwrap_or(0),
                    |this, cx| this.load_inspection_hex(this.hex_offset + 512, cx),
                    cx,
                ));
            content = content.child(controls);
            if self.offset_error {
                content = content.child(self.i18n.t("file_preview.invalid_offset"));
            }
            if let Some(editor) = &self.hex_editor {
                content = content.child(div().w_full().h(px(480.0)).child(editor.clone()));
            }
        }
        body.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .gap_4()
                .child(objects)
                .child(content),
        )
    }

    fn load_inspection_hex(&mut self, offset: u64, cx: &mut Context<Self>) {
        let Some(page) = &self.inspection else { return };
        if page.kind != "binary" {
            return;
        }
        let size = page.size.unwrap_or(0);
        if offset >= size && !(size == 0 && offset == 0) {
            self.offset_error = true;
            cx.notify();
            return;
        }
        let Some(source) = self.source.clone() else {
            return;
        };
        self.cancel();
        self.loading = true;
        self.offset_error = false;
        self.hex_offset = offset;
        let tokens = self.tokens;
        let input = format!("0x{offset:X}");
        if let Some(editor) = &self.offset_editor {
            editor.update(cx, |editor, cx| editor.replace_text_external(input, cx));
        } else {
            self.offset_editor = Some(cx.new(|cx| {
                let mut editor = TextEditorView::new(input, &tokens, cx);
                editor.set_presentation(oxideterm_gpui_editor::EditorPresentation::Inline, cx);
                editor
            }));
        }
        let task = self.runtime.spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            let mut file = tokio::fs::File::open(source.path()).await.map_err(|_| ())?;
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|_| ())?;
            let mut bytes = zeroize::Zeroizing::new(Vec::new());
            file.take(512)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| ())?;
            Ok::<_, ()>(oxideterm_preview::generate_hex_dump(&bytes, offset))
        });
        self.job = Some(task.abort_handle());
        self.delivery = Some(cx.spawn(async move |view, cx| {
            let result = task.await.unwrap_or(Err(()));
            let _ = view.update(cx, |this, cx| {
                this.job = None;
                this.loading = false;
                match result {
                    Ok(text) => {
                        if let Some(editor) = &this.hex_editor {
                            editor.update(cx, |editor, cx| editor.replace_text_external(text, cx));
                        } else {
                            this.hex_editor = Some(cx.new(|cx| {
                                let mut editor = TextEditorView::new(text, &tokens, cx);
                                editor.set_read_only(true);
                                editor.set_presentation(
                                    oxideterm_gpui_editor::EditorPresentation::Inline,
                                    cx,
                                );
                                editor
                            }));
                        }
                    }
                    Err(()) => this.offset_error = true,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inspection_payload_bounds_match_the_selected_object_and_source() {
        let certificate = json!({"kind":"certificate","index":0,"objects":["Example"],"fields":[{"key":"subject","value":"CN=Example"}],"notBefore":100,"notAfter":200});
        let page = InspectionPage::parse(certificate.clone(), 0, true)
            .unwrap_or_else(|_| panic!("valid certificate"));
        assert_eq!(page.fields[0].value, "CN=Example");
        for (now, status) in [
            (99, "not_yet_valid"),
            (100, "within_validity"),
            (200, "within_validity"),
            (201, "expired"),
        ] {
            assert_eq!(page.validity_status(now), status);
        }
        for (key, value) in [
            ("notAfter", json!(99)),
            ("notBefore", json!(null)),
            ("index", json!(1)),
            ("size", json!(100)),
            ("fields", json!([{"key":"private_key","value":"rejected"}])),
        ] {
            let mut invalid = certificate.clone();
            invalid[key] = value;
            assert!(InspectionPage::parse(invalid, 0, true).is_err(), "{key}");
        }
        let binary = json!({"kind":"binary","index":0,"objects":["X86_64"],"fields":[{"key":"format","value":"ELF"}],"size":512,"sections":[{"name":".text","offset":256,"size":256,"address":4194304},{"name":".bss","offset":null,"size":4096,"address":4194560}]});
        let page = InspectionPage::parse(binary.clone(), 0, false)
            .unwrap_or_else(|_| panic!("valid binary"));
        assert_eq!(page.sections[0].offset, Some(256));
        assert_eq!(page.sections[1].name, ".bss");
        for offset in [257, u64::MAX] {
            let mut invalid = binary.clone();
            invalid["sections"][0]["offset"] = json!(offset);
            assert!(InspectionPage::parse(invalid, 0, false).is_err());
        }
    }
}
