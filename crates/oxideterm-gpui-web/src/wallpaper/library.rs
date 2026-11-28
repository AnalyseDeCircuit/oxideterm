use crate::host::Workspace;
use gpui::{
    App, Asset, AsyncApp, Context, Image, ImageAssetLoader, ImageFormat, ImageSource, Resource,
    Task,
};
use js_sys::{Array, Promise, Reflect, Uint8Array};
use oxideterm_gpui_settings_view::surface::{WallpaperAction, WallpaperTile};
use oxideterm_settings_model::browser_preferences::{WallpaperId, WallpaperSettings};
use std::sync::Arc;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(module = "/src/wallpaper_gallery.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=listWallpapers)]
    async fn list_media() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=loadWallpaper)]
    async fn read_image(id: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=removeWallpapers)]
    async fn remove_media(ids: Array) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=importWallpapers)]
    async fn import_media(files: Array, limit: u32) -> Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/src/upload.mjs")]
extern "C" {
    #[wasm_bindgen(js_name=chooseUploads)]
    fn choose_images(accept: &str) -> Promise;
}

const STORAGE: &str = "settings_view.terminal.bg_operation_failed";
const DECODE: &str = "settings_view.terminal.bg_decode_failed";

fn error(err: JsValue) -> &'static str {
    match err.as_string().as_deref() {
        Some("unsupported") => "settings_view.terminal.bg_unsupported",
        Some("size") => "sftp.preview.too_large",
        Some("decode") => DECODE,
        _ => STORAGE,
    }
}
fn string(v: &JsValue, key: &str) -> Result<String, &'static str> {
    Reflect::get(v, &key.into())
        .ok()
        .and_then(|v| v.as_string())
        .ok_or(STORAGE)
}
fn encoded(v: &JsValue, format: ImageFormat) -> Result<Arc<Image>, &'static str> {
    let bytes = Reflect::get(v, &"bytes".into())
        .ok()
        .and_then(|v| v.dyn_into::<Uint8Array>().ok())
        .ok_or(DECODE)?;
    Ok(Arc::new(Image::from_bytes(format, bytes.to_vec())))
}

async fn read_full(id: Uuid, cx: &AsyncApp) -> Result<ImageSource, &'static str> {
    let value = read_image(&id.to_string()).await.map_err(error)?;
    let image = encoded(&value, format(&value)?.0)?;
    let bytes = image.bytes();
    // Declared WebP images are static in GPUI. Use its resource decoder for animated containers.
    if image.format() == ImageFormat::Webp
        && bytes.get(12..16) == Some(b"VP8X")
        && bytes.get(20).is_some_and(|flags| flags & 2 != 0)
    {
        return cx
            .update(|cx| {
                let executor = cx.background_executor().clone();
                executor.spawn(ImageAssetLoader::load(Resource::Image(image), cx))
            })
            .await
            .map(ImageSource::Render)
            .map_err(|_| DECODE);
    }
    Ok(image.into())
}
fn format(v: &JsValue) -> Result<(ImageFormat, &'static str), &'static str> {
    match string(v, "type")?.as_str() {
        "image/png" => Ok((ImageFormat::Png, "settings_view.terminal.bg_format_png")),
        "image/jpeg" => Ok((ImageFormat::Jpeg, "settings_view.terminal.bg_format_jpeg")),
        "image/webp" => Ok((ImageFormat::Webp, "settings_view.terminal.bg_format_webp")),
        "image/gif" => Ok((ImageFormat::Gif, "settings_view.terminal.bg_format_gif")),
        _ => Err(DECODE),
    }
}
fn tiles(v: JsValue) -> Result<Vec<WallpaperTile>, &'static str> {
    v.dyn_into::<Array>()
        .map_err(|_| STORAGE)?
        .iter()
        .map(|v| {
            Ok(WallpaperTile {
                id: WallpaperId::Imported(
                    Uuid::parse_str(&string(&v, "id")?).map_err(|_| STORAGE)?,
                ),
                name: string(&v, "name")?,
                format_key: format(&v)?.1,
                thumbnail: encoded(&v, ImageFormat::Png)?.into(),
            })
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct Library {
    tiles: Vec<WallpaperTile>,
    full: Option<(Uuid, ImageSource)>,
    loading: Option<(Uuid, Task<()>)>,
    editing: Option<Task<()>>,
    failed: Option<Uuid>,
    list_error: Option<&'static str>,
}
impl Library {
    pub(super) fn full_source(&self, id: Uuid) -> Option<ImageSource> {
        self.full
            .as_ref()
            .filter(|(key, _)| *key == id)
            .map(|(_, s)| s.clone())
    }
    fn release_full(&mut self, cx: &mut App) {
        if let Some((_, source)) = self.full.take() {
            match source {
                ImageSource::Render(image) => cx.drop_image(image, None),
                _ => source.remove_asset(cx),
            }
        }
    }
}

impl Workspace {
    fn publish_gallery(&self, cx: &mut Context<Self>) {
        self.settings.update(cx, |s, cx| {
            s.update_wallpaper_gallery(
                self.wallpaper_library.tiles.clone(),
                self.wallpaper_library.editing.is_some(),
                self.wallpaper_library.list_error,
                cx,
            )
        });
    }
    pub(crate) fn load_wallpaper_list(&mut self, cx: &mut Context<Self>) {
        self.wallpaper_library.editing = Some(cx.spawn(async move |owner, cx| {
            let result = list_media().await.map_err(error).and_then(tiles);
            let _ = owner.update(cx, |h, cx| {
                h.wallpaper_library.editing = None;
                match result {
                    Ok(items) => {
                        h.wallpaper_library.list_error = None;
                        h.wallpaper_library.tiles = items;
                    }
                    Err(key) => h.wallpaper_library.list_error = Some(key),
                }
                h.publish_gallery(cx);
            });
        }));
        self.publish_gallery(cx);
    }
    fn selected_wallpaper(&self, cx: &App) -> Option<WallpaperId> {
        WallpaperId::selected(&self.settings.read(cx).settings(cx).terminal)
    }
    fn set_wallpaper(&self, id: Option<WallpaperId>, cx: &mut Context<Self>) {
        self.settings.update(cx, |surface, cx| {
            surface.edit_preferences(cx, |prefs| {
                prefs.terminal.background_image = id.map(WallpaperId::key);
                if id.is_some() {
                    prefs.terminal.background_enabled = true;
                }
            })
        });
    }

    pub(crate) fn sync_wallpaper(&mut self, cx: &mut Context<Self>) {
        let desired = WallpaperSettings::active(&self.settings.read(cx).settings(cx).terminal);
        self.wallpaper = desired;
        if let Some(WallpaperId::Imported(id)) = self.wallpaper.as_ref().map(|w| w.image) {
            if self.wallpaper_library.full_source(id).is_none() {
                self.wallpaper = None;
                if self.wallpaper_library.loading.is_none()
                    && self.wallpaper_library.failed != Some(id)
                {
                    self.load_full_wallpaper(id, false, cx);
                }
            }
        } else {
            self.wallpaper_library.release_full(cx);
            self.settings
                .update(cx, |s, cx| s.set_loaded_wallpaper(None, cx));
        }
    }
    fn load_full_wallpaper(&mut self, id: Uuid, select: bool, cx: &mut Context<Self>) {
        self.wallpaper_library.failed = None;
        self.wallpaper_library.loading = Some((
            id,
            cx.spawn(async move |owner, cx| {
                let result = read_full(id, cx).await;
                let _ = owner.update(cx, |h, cx| {
                    h.wallpaper_library.loading = None;
                    let key = WallpaperId::Imported(id);
                    if !select
                        && (h.selected_wallpaper(cx) != Some(key)
                            || !h.settings.read(cx).settings(cx).terminal.background_enabled)
                    {
                        return;
                    }
                    match result {
                        Ok(image) => {
                            h.wallpaper_library.release_full(cx);
                            h.wallpaper_library.full = Some((id, image));
                            h.settings
                                .update(cx, |s, cx| s.set_loaded_wallpaper(Some(key), cx));
                            if select {
                                h.set_wallpaper(Some(key), cx);
                            }
                            h.sync_wallpaper(cx);
                            for session in h.sessions.values() {
                                let alpha = h.terminal_background_alpha(&session.kind());
                                session
                                    .view
                                    .update(cx, |v, cx| v.set_background_alpha(alpha, cx));
                            }
                            h.refresh_center(cx);
                            h.changed(cx);
                        }
                        Err(key) => {
                            h.wallpaper_library.failed = Some(id);
                            h.notice(key, cx);
                        }
                    }
                });
            }),
        ));
    }
    pub(super) fn wallpaper_action(&mut self, action: WallpaperAction, cx: &mut Context<Self>) {
        match action {
            WallpaperAction::Reload if self.wallpaper_library.editing.is_none() => {
                self.load_wallpaper_list(cx);
            }
            WallpaperAction::Select(key) => {
                self.wallpaper_library.loading = None;
                match key {
                    WallpaperId::Bundled(_) => self.set_wallpaper(Some(key), cx),
                    WallpaperId::Imported(id) => self.load_full_wallpaper(id, true, cx),
                }
            }
            WallpaperAction::Pick if self.wallpaper_library.editing.is_none() => {
                // Start the picker before yielding the click's transient activation.
                let selection =
                    JsFuture::from(choose_images("image/png,image/jpeg,image/webp,image/gif"));
                self.wallpaper_library.editing = Some(cx.spawn(async move |owner, cx| {
                    let result = async {
                        let files = selection
                            .await
                            .map_err(error)?
                            .dyn_into::<Array>()
                            .map_err(|_| STORAGE)?;
                        if files.length() == 0 {
                            return Ok(Vec::new());
                        }
                        tiles(
                            import_media(files, crate::browser_file::MAX_IMPORT_BYTES as u32)
                                .await
                                .map_err(error)?,
                        )
                    }
                    .await;
                    let _ = owner.update(cx, |h, cx| {
                        h.wallpaper_library.editing = None;
                        match result {
                            Ok(added) => {
                                let first = added.first().map(|t| t.id);
                                h.wallpaper_library.tiles.extend(added);
                                if let Some(WallpaperId::Imported(id)) = first
                                    && h.selected_wallpaper(cx).is_none()
                                    && h.wallpaper_library.loading.is_none()
                                {
                                    h.load_full_wallpaper(id, true, cx);
                                }
                            }
                            Err(key) => h.notice(key, cx),
                        }
                        h.publish_gallery(cx);
                    });
                }));
                self.publish_gallery(cx);
            }
            WallpaperAction::Remove(_) | WallpaperAction::Clear
                if self.wallpaper_library.editing.is_none() =>
            {
                let ids: Vec<_> = self
                    .wallpaper_library
                    .tiles
                    .iter()
                    .filter_map(|tile| match tile.id {
                        WallpaperId::Imported(id)
                            if matches!(action, WallpaperAction::Clear)
                                || matches!(action,WallpaperAction::Remove(k) if k==tile.id) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    })
                    .collect();
                if ids.is_empty() {
                    return;
                }
                self.wallpaper_library.editing = Some(cx.spawn(async move |owner, cx| {
                    let keys = ids.iter().map(|id| JsValue::from_str(&id.to_string())).collect::<Array>();
                    let result = remove_media(keys).await;
                    let _ = owner.update(cx, |h, cx| {
                        h.wallpaper_library.editing = None;
                        if result.is_ok() {
                            if h.wallpaper_library.loading.as_ref().is_some_and(|(id, _)| ids.contains(id)) {
                                h.wallpaper_library.loading = None;
                            }
                            h.wallpaper_library.tiles.retain(|tile| {
                                let remove = matches!(tile.id, WallpaperId::Imported(id) if ids.contains(&id));
                                if remove { tile.thumbnail.remove_asset(cx); }
                                !remove
                            });
                            if matches!(h.selected_wallpaper(cx), Some(WallpaperId::Imported(id)) if ids.contains(&id)) {
                                h.set_wallpaper(None, cx);
                            }
                        } else {
                            h.notice(STORAGE, cx);
                        }
                        h.publish_gallery(cx);
                    });
                }));
                self.publish_gallery(cx);
            }
            _ => {}
        }
    }
}
