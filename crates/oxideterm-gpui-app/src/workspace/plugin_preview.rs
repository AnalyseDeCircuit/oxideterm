use super::*;
use base64::Engine;
use oxideterm_gpui_editor::TextEditorView;
use oxideterm_gpui_ui::button::{ButtonOptions, ButtonSize, ButtonVariant};
use oxideterm_plugin_host_api::runtime::{NativeProcessPluginRuntime, PluginRuntimeBridge};
use oxideterm_preview::PreviewAssetOwner;
use std::{io::Cursor, time::Duration};
mod table;
use table::TablePage;
mod inspection;
use inspection::InspectionPage;

pub(super) fn local_inspection_preview(path: &str) -> Option<oxideterm_local_files::LocalPreview> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = zeroize::Zeroizing::new([0; 64]);
    let count = file.read(&mut header[..]).ok()?;
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = oxideterm_preview::inspection_mime_type(&extension, &header[..count])?;
    let size = file.metadata().ok()?.len();
    Some(if size > oxideterm_local_files::MAX_PREVIEW_SIZE {
        oxideterm_local_files::LocalPreview::TooLarge { size }
    } else {
        oxideterm_local_files::LocalPreview::Document {
            path: path.into(),
            mime_type: mime.into(),
        }
    })
}

enum RenderedPage {
    Image(Arc<RenderImage>, u32, f32),
    Table(TablePage),
    Inspection(InspectionPage),
}

/// The preview owns both its source lease and its cancellable renderer job.
pub(super) struct PluginFilePreview {
    source: Option<PreviewAssetOwner>,
    plugins: Entity<plugin_entity::PluginWorkspaceEntity>,
    runtime: Arc<tokio::runtime::Runtime>,
    tokens: ThemeTokens,
    i18n: I18n,
    page: u32,
    page_count: u32,
    zoom: f32,
    width: f32,
    image: Option<Arc<RenderImage>>,
    table: Option<Arc<TablePage>>,
    selected_table: Option<String>,
    table_scroll: Option<gpui::ListState>,
    inspection: Option<Arc<InspectionPage>>,
    inspection_tab: usize,
    hex_editor: Option<Entity<TextEditorView>>,
    offset_editor: Option<Entity<TextEditorView>>,
    hex_offset: u64,
    offset_error: bool,
    aspect: f32,
    error_key: Option<&'static str>,
    loading: bool,
    job: Option<tokio::task::AbortHandle>,
    delivery: Option<gpui::Task<()>>,
    _registry_subscription: gpui::Subscription,
}

impl PluginFilePreview {
    pub(super) fn new(
        plugins: Entity<plugin_entity::PluginWorkspaceEntity>,
        runtime: Arc<tokio::runtime::Runtime>,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&plugins, |this, _, cx| {
            if this.source.is_some() && this.provider(cx).is_none() {
                this.cancel();
                if let Some(image) = this.image.take() {
                    cx.drop_image(image, None);
                }
                this.error_key = Some("file_preview.plugin_required");
                this.table = None;
                this.table_scroll = None;
                this.inspection = None;
                this.hex_editor = None;
                this.offset_editor = None;
                cx.notify();
            } else if this.source.is_some()
                && this.error_key == Some("file_preview.plugin_required")
            {
                this.render_page(cx);
            }
        });
        cx.on_release(|this, cx| {
            if let Some(image) = this.image.take() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            source: None,
            plugins,
            runtime,
            tokens,
            i18n,
            page: 0,
            page_count: 0,
            zoom: 1.0,
            width: 800.0,
            image: None,
            table: None,
            selected_table: None,
            table_scroll: None,
            inspection: None,
            inspection_tab: 0,
            hex_editor: None,
            offset_editor: None,
            hex_offset: 0,
            offset_error: false,
            aspect: 1.0,
            error_key: None,
            loading: false,
            job: None,
            delivery: None,
            _registry_subscription: subscription,
        }
    }

    pub(super) fn set_source(&mut self, source: PreviewAssetOwner, cx: &mut Context<Self>) {
        self.source = Some(source);
        self.page = 0;
        self.selected_table = None;
        self.inspection = None;
        self.hex_editor = None;
        self.offset_editor = None;
        self.hex_offset = 0;
        self.offset_error = false;
        self.render_page(cx);
    }

    fn provider(&self, cx: &App) -> Option<(plugin_host::NativePluginInfo, String)> {
        let mime = self.source.as_ref()?.mime_type();
        self.plugins.read(cx).registry().file_preview_provider(mime)
    }

    fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.abort();
        }
        self.delivery = None;
        self.loading = false;
    }

    fn render_page(&mut self, cx: &mut Context<Self>) {
        self.cancel();
        let Some(source) = self.source.clone() else {
            return;
        };
        let Some((plugin, command)) = self.provider(cx) else {
            self.error_key = Some("file_preview.plugin_required");
            cx.notify();
            return;
        };
        self.error_key = None;
        self.loading = true;
        let page = self.page;
        let width = (self.width * self.zoom * 2.0).clamp(256.0, 2048.0) as u32;
        let task = self.runtime.spawn(render_page(
            plugin,
            command,
            source,
            page,
            width,
            self.selected_table.clone(),
        ));
        self.job = Some(task.abort_handle());
        self.delivery = Some(cx.spawn(async move |view, cx| {
            let result = task.await.unwrap_or(Err("file_preview.failed"));
            let _ = view.update(cx, |this, cx| {
                this.loading = false;
                this.job = None;
                if let Some(image) = this.image.take() {
                    cx.drop_image(image, None);
                }
                this.table = None;
                this.table_scroll = None;
                this.inspection = None;
                match result {
                    Ok(RenderedPage::Image(image, page_count, aspect)) => {
                        this.image = Some(image);
                        this.page_count = page_count;
                        this.aspect = aspect;
                    }
                    Ok(RenderedPage::Table(table)) => {
                        this.selected_table = table.selected_table.clone();
                        this.page_count = table.page_count;
                        let metrics =
                            oxideterm_gpui_ui::TauriTableMetrics::from_tokens(&this.tokens);
                        this.table_scroll = Some(tauri_virtual_list_state(
                            table.rows.len(),
                            ListAlignment::Top,
                            TauriVirtualListSpec::new(px(metrics.row_min_height), 4),
                        ));
                        this.table = Some(Arc::new(table));
                    }
                    Ok(RenderedPage::Inspection(page)) => {
                        this.page_count = page.objects.len() as u32;
                        this.inspection = Some(Arc::new(page));
                        this.inspection_tab = 0;
                        this.hex_editor = None;
                        this.offset_editor = None;
                    }
                    Err(key) => {
                        this.image = None;
                        this.error_key = Some(key);
                    }
                }
                cx.notify();
            });
        }));
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
}

impl Drop for PluginFilePreview {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Render for PluginFilePreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.source.as_ref().is_some_and(|source| {
            matches!(
                source.mime_type(),
                "application/pkix-cert" | "application/x-oxideterm-binary"
            )
        }) {
            return self.render_inspection(cx);
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| source.mime_type() != "application/pdf")
        {
            return self.render_database(cx);
        }
        let view = cx.entity();
        let width_probe = gpui::canvas(
            move |bounds, _, cx| {
                view.update(cx, |this, cx| {
                    let width = f32::from(bounds.size.width).max(100.0);
                    if (this.width - width).abs() > 1.0 {
                        this.width = width;
                        cx.notify();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .w_full()
        .h(px(0.0));
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .p_3()
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(width_probe)
            .child(
                div()
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
                        "−".into(),
                        self.loading || self.zoom <= 0.5,
                        |this, cx| {
                            this.zoom -= 0.25;
                            this.render_page(cx);
                        },
                        cx,
                    ))
                    .child(format!("{}%", (self.zoom * 100.0) as u32))
                    .child(self.button(
                        "+".into(),
                        self.loading || self.zoom >= 3.0,
                        |this, cx| {
                            this.zoom += 0.25;
                            this.render_page(cx);
                        },
                        cx,
                    ))
                    .child(self.button(
                        self.i18n.t("file_preview.fit_width"),
                        self.loading,
                        |this, cx| {
                            this.zoom = 1.0;
                            this.render_page(cx);
                        },
                        cx,
                    ))
                    .child(self.button(
                        self.i18n.t("file_preview.retry"),
                        self.loading,
                        |this, cx| this.render_page(cx),
                        cx,
                    )),
            );
        if self.page_count > 1 {
            let view = cx.entity();
            let bounds = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
            let read_bounds = bounds.clone();
            let slider = oxideterm_gpui_ui::slider::slider(
                &self.tokens,
                oxideterm_gpui_ui::slider::SliderView {
                    min: 0.0,
                    max: (self.page_count - 1) as f32,
                    value: self.page as f32,
                    disabled: self.loading,
                },
            )
            .h(px(self.tokens.metrics.ui_control_height))
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                let bounds = read_bounds.get();
                view.update(cx, |this, cx| {
                    if !this.loading {
                        let ratio = ((event.position.x - bounds.origin.x) / bounds.size.width)
                            .clamp(0.0, 1.0);
                        this.page = (ratio * (this.page_count - 1) as f32).round() as u32;
                        this.render_page(cx);
                    }
                });
                cx.stop_propagation();
            });
            body = body.child(oxideterm_gpui_ui::text_input::text_input_anchor_probe(
                oxideterm_gpui_ui::text_input::TextInputAnchorId(0),
                slider,
                move |anchor, _, _| bounds.set(anchor.bounds),
            ));
        }
        if let Some(key) = self.error_key {
            body = body.child(
                div()
                    .py_4()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(key)),
            );
        } else if self.loading {
            body = body.child(self.i18n.t("file_preview.loading"));
        }
        if let Some(image) = self.image.clone() {
            let width = self.width * self.zoom;
            body = body.child(
                div()
                    .id("plugin-document-image")
                    .w_full()
                    .overflow_x_scroll()
                    .child(gpui::img(image).w(px(width)).h(px(width * self.aspect))),
            );
        }
        body
    }
}

async fn render_page(
    plugin: plugin_host::NativePluginInfo,
    command: String,
    source: PreviewAssetOwner,
    page: u32,
    width: u32,
    table: Option<String>,
) -> Result<RenderedPage, &'static str> {
    let inspection = matches!(
        source.mime_type(),
        "application/pkix-cert" | "application/x-oxideterm-binary"
    );
    let certificate = source.mime_type() == "application/pkix-cert";
    let database = source.mime_type() != "application/pdf";
    let fail = if inspection {
        "file_preview.inspection_failed"
    } else if database {
        "file_preview.database_failed"
    } else {
        "file_preview.failed"
    };
    let plugin_host::NativePluginRuntimePlan::Process { entry } = plugin.runtime_plan else {
        return Err(fail);
    };
    let mut runtime = NativeProcessPluginRuntime::new(
        &plugin.manifest.id,
        plugin.install_dir,
        entry,
        Duration::from_secs(15),
    );
    // A dedicated process avoids blocking other plugins behind a long PDF render.
    // Dropping this future kills that process; the source lease outlives the job.
    let activated = runtime
        .activate(plugin_runtime::PluginActivateRequest {
            request_id: "preview-activate".into(),
            manifest: plugin.manifest,
            permissions: plugin_runtime::PluginPermissionSet::default(),
            timeout_ms: 15_000,
        })
        .await
        .map_err(|_| fail)?;
    if !matches!(
        activated.result,
        plugin_runtime::PluginResponseResult::Ok { .. }
    ) {
        return Err(fail);
    }
    let response = runtime
        .call(plugin_runtime::PluginRequest {
            request_id: "preview-render".into(),
            timeout_ms: Some(15_000),
            kind: plugin_runtime::PluginRequestKind::DispatchCommand {
                command,
                args: serde_json::json!({
                    "path": source.path(), "page": page, "width": width, "table": table,
                    "snapshot": source.ownership() == oxideterm_preview::PreviewAssetOwnership::OwnedTemp,
                }),
            },
        })
        .await
        .map_err(|_| fail)?;
    let _ = runtime.kill().await;
    let value = match response.result {
        plugin_runtime::PluginResponseResult::Ok { value } => value,
        plugin_runtime::PluginResponseResult::Error { error }
            if error.code == "database_too_large" =>
        {
            return Err("file_preview.database_limit");
        }
        plugin_runtime::PluginResponseResult::Error { error }
            if error.code == "no_certificates" =>
        {
            return Err("file_preview.no_certificates");
        }
        _ => return Err(fail),
    };
    if inspection {
        return InspectionPage::parse(value, page, certificate)
            .map(RenderedPage::Inspection)
            .map_err(|_| fail);
    }
    if database {
        return TablePage::parse(value, page, table.as_deref())
            .map(RenderedPage::Table)
            .map_err(|_| fail);
    }
    let count = value["pageCount"]
        .as_u64()
        .filter(|count| *count > page as u64 && *count <= 100_000)
        .ok_or(fail)? as u32;
    let png = value["png"]
        .as_str()
        .filter(|data| data.len() <= 16 * 1024 * 1024)
        .ok_or(fail)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png)
        .map_err(|_| fail)?;
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| fail)?.to_rgba8();
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 8_000_000 {
        return Err(fail);
    }
    let mut pixels = image.into_raw();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(w, h, pixels).ok_or(fail)?;
    Ok(RenderedPage::Image(
        Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
        count,
        h as f32 / w as f32,
    ))
}
