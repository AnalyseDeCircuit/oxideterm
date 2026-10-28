use crate::{
    AnyElement, AnyImageCache, App, Asset, AssetLogger, Bounds, DefiniteLength, Element, ElementId,
    Entity, GlobalElementId, Hitbox, Image, ImageCache, InspectorElementId, InteractiveElement,
    Interactivity, IntoElement, LayoutId, Length, ObjectFit, Pixels, RenderImage, Resource,
    SharedString, SharedUri, StyleRefinement, Styled, Task, Window, decode_static_image,
    decode_static_image_from_decoder, px,
};
use anyhow::Result;

use futures::Future;
use gpui_util::ResultExt;
use image::{
    AnimationDecoder, ImageError, ImageFormat, Rgba,
    codecs::{gif::GifDecoder, webp::WebPDecoder},
};
use scheduler::Instant;
use smallvec::SmallVec;
use std::{
    fs,
    io::{self, Cursor},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

use super::{Stateful, StatefulInteractiveElement};

mod decoded_size;
pub(crate) mod sized;
mod streaming;
pub(crate) use streaming::{AnimationInput, AnimationStream};

/// The delay before showing the loading state.
pub const LOADING_DELAY: Duration = Duration::from_millis(200);

/// A type alias to the resource loader that the `img()` element uses.
///
/// Note: that this is only for Resources, like URLs or file paths.
/// Custom loaders, or external images will not use this asset loader
pub type ImgResourceLoader = AssetLogger<ImageAssetLoader>;

/// A source of image content.
#[derive(Clone)]
pub enum ImageSource {
    /// The image content will be loaded from some resource location
    Resource(Resource),
    /// Cached image data
    Render(Arc<RenderImage>),
    /// Cached image data
    Image(Arc<Image>),
    /// A custom loading function to use
    Custom(Arc<dyn Fn(&mut Window, &mut App) -> Option<Result<Arc<RenderImage>, ImageCacheError>>>),
}

fn is_uri(uri: &str) -> bool {
    url::Url::from_str(uri).is_ok()
}

impl From<SharedUri> for ImageSource {
    fn from(value: SharedUri) -> Self {
        Self::Resource(Resource::Uri(value))
    }
}

impl<'a> From<&'a str> for ImageSource {
    fn from(s: &'a str) -> Self {
        if is_uri(s) {
            Self::Resource(Resource::Uri(s.to_string().into()))
        } else {
            Self::Resource(Resource::Embedded(s.to_string().into()))
        }
    }
}

impl From<String> for ImageSource {
    fn from(s: String) -> Self {
        if is_uri(&s) {
            Self::Resource(Resource::Uri(s.into()))
        } else {
            Self::Resource(Resource::Embedded(s.into()))
        }
    }
}

impl From<SharedString> for ImageSource {
    fn from(s: SharedString) -> Self {
        s.as_ref().into()
    }
}

impl From<&Path> for ImageSource {
    fn from(value: &Path) -> Self {
        Self::Resource(value.to_path_buf().into())
    }
}

impl From<Arc<Path>> for ImageSource {
    fn from(value: Arc<Path>) -> Self {
        Self::Resource(value.into())
    }
}

impl From<PathBuf> for ImageSource {
    fn from(value: PathBuf) -> Self {
        Self::Resource(value.into())
    }
}

impl From<Arc<RenderImage>> for ImageSource {
    fn from(value: Arc<RenderImage>) -> Self {
        Self::Render(value)
    }
}

impl From<Arc<Image>> for ImageSource {
    fn from(value: Arc<Image>) -> Self {
        Self::Image(value)
    }
}

impl<F> From<F> for ImageSource
where
    F: Fn(&mut Window, &mut App) -> Option<Result<Arc<RenderImage>, ImageCacheError>> + 'static,
{
    fn from(value: F) -> Self {
        Self::Custom(Arc::new(value))
    }
}

/// The style of an image element.
pub struct ImageStyle {
    grayscale: bool,
    object_fit: ObjectFit,
    object_position: (f32, f32),
    loading: Option<Box<dyn Fn() -> AnyElement>>,
    fallback: Option<Box<dyn Fn() -> AnyElement>>,
}

impl Default for ImageStyle {
    fn default() -> Self {
        Self {
            grayscale: false,
            object_fit: ObjectFit::Contain,
            object_position: (0.5, 0.5),
            loading: None,
            fallback: None,
        }
    }
}

/// Style an image element.
pub trait StyledImage: Sized {
    /// Position the fitted image from the top-left (0, 0) to bottom-right (1, 1).
    fn object_position(mut self, position: (f32, f32)) -> Self {
        self.image_style().object_position = position;
        self
    }
    /// Get a mutable [ImageStyle] from the element.
    fn image_style(&mut self) -> &mut ImageStyle;

    /// Set the image to be displayed in grayscale.
    fn grayscale(mut self, grayscale: bool) -> Self {
        self.image_style().grayscale = grayscale;
        self
    }

    /// Set the object fit for the image.
    fn object_fit(mut self, object_fit: ObjectFit) -> Self {
        self.image_style().object_fit = object_fit;
        self
    }

    /// Set a fallback function that will be invoked to render an error view should
    /// the image fail to load.
    fn with_fallback(mut self, fallback: impl Fn() -> AnyElement + 'static) -> Self {
        self.image_style().fallback = Some(Box::new(fallback));
        self
    }

    /// Set a fallback function that will be invoked to render a view while the image
    /// is still being loaded.
    fn with_loading(mut self, loading: impl Fn() -> AnyElement + 'static) -> Self {
        self.image_style().loading = Some(Box::new(loading));
        self
    }
}

impl StyledImage for Img {
    fn image_style(&mut self) -> &mut ImageStyle {
        &mut self.style
    }
}

impl StyledImage for Stateful<Img> {
    fn image_style(&mut self) -> &mut ImageStyle {
        &mut self.element.style
    }
}

/// An image element.
pub struct Img {
    interactivity: Interactivity,
    source: ImageSource,
    style: ImageStyle,
    image_cache: Option<AnyImageCache>,
}

/// Create a new image element.
#[track_caller]
pub fn img(source: impl Into<ImageSource>) -> Img {
    Img {
        interactivity: Interactivity::new(),
        source: source.into(),
        style: ImageStyle::default(),
        image_cache: None,
    }
}

impl Img {
    /// A list of all format extensions currently supported by this img element
    pub fn extensions() -> &'static [&'static str] {
        // This is the list in [image::ImageFormat::from_extension] + `svg`
        &[
            "avif", "jpg", "jpeg", "png", "gif", "webp", "tif", "tiff", "tga", "dds", "bmp", "ico",
            "hdr", "exr", "pbm", "pam", "ppm", "pgm", "ff", "farbfeld", "qoi", "svg",
        ]
    }

    /// Sets the image cache for the current node.
    ///
    /// If the `image_cache` is not explicitly provided, the function will determine the image cache by:
    ///
    /// 1. Checking if any ancestor node of the current node contains an `ImageCacheElement`, If such a node exists, the image cache specified by that ancestor will be used.
    /// 2. If no ancestor node contains an `ImageCacheElement`, the global image cache will be used as a fallback.
    ///
    /// This mechanism provides a flexible way to manage image caching, allowing precise control when needed,
    /// while ensuring a default behavior when no cache is explicitly specified.
    #[inline]
    pub fn image_cache<I: ImageCache>(self, image_cache: &Entity<I>) -> Self {
        Self {
            image_cache: Some(image_cache.clone().into()),
            ..self
        }
    }
}

impl Deref for Stateful<Img> {
    type Target = Img;

    fn deref(&self) -> &Self::Target {
        &self.element
    }
}

impl DerefMut for Stateful<Img> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.element
    }
}

/// The image state between frames
struct ImgState {
    frame_index: usize,
    last_frame_time: Option<Instant>,
    started_loading: Option<(Instant, Task<()>)>,
    playback: Arc<()>,
    resource: Option<Resource>,
}

/// The image layout state between frames
pub struct ImgLayoutState {
    frame_index: usize,
    intrinsic_size: crate::Size<crate::DevicePixels>,
    replacement: Option<AnyElement>,
    playback: Option<Arc<()>>,
}

impl Element for Img {
    type RequestLayoutState = ImgLayoutState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        self.interactivity.element_id.clone()
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.interactivity.source_location()
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut layout_state = ImgLayoutState {
            frame_index: 0,
            intrinsic_size: Default::default(),
            replacement: None,
            playback: None,
        };

        let resource = match &self.source {
            ImageSource::Resource(source) => Some(source.clone()),
            ImageSource::Image(image) => Some(Resource::Image(image.clone())),
            _ => None,
        };

        window.with_optional_element_state(global_id, |state, window| {
            let mut state = state.map(|state| {
                let mut state = state.unwrap_or_else(|| ImgState {
                    frame_index: 0,
                    last_frame_time: None,
                    started_loading: None,
                    playback: Arc::new(()),
                    resource: resource.clone(),
                });
                if state.resource != resource {
                    state.resource = resource.clone();
                    state.playback = Arc::new(());
                }
                state
            });
            layout_state.playback = state.as_ref().map(|state| state.playback.clone());

            let mut frame_index = state.as_ref().map(|state| state.frame_index).unwrap_or(0);

            let layout_id = self.interactivity.request_layout(
                global_id,
                inspector_id,
                window,
                cx,
                |mut style, window, cx| {
                    let mut replacement_id = None;
                    let mut loading = false;

                    match self.source.use_frame_at_size(
                        self.image_cache
                            .clone()
                            .or_else(|| window.image_cache_stack.last().cloned()),
                        None,
                        crate::ImageFrameRequest {
                            index: frame_index,
                            playback: layout_state.playback.clone(),
                        },
                        window,
                        cx,
                    ) {
                        Some(Ok(data)) => {
                            layout_state.intrinsic_size = data.intrinsic_size(0);
                            let frame_count = data.frame_count();
                            loading = frame_count == 0;
                            let max_frame_index = frame_count.saturating_sub(1);

                            if let Some(state) = &mut state {
                                state.frame_index = state.frame_index.min(max_frame_index);
                                if frame_count > 1 && !cx.reduce_motion() {
                                    if window.is_window_active() {
                                        let current_time = Instant::now();
                                        if let Some(last_frame_time) = state.last_frame_time {
                                            let elapsed = current_time - last_frame_time;
                                            let frame_duration =
                                                Duration::from(data.delay(state.frame_index));

                                            let next = (state.frame_index + 1) % frame_count;
                                            if elapsed >= frame_duration && data.frame_ready(next) {
                                                state.frame_index = next;
                                                state.last_frame_time =
                                                    Some(current_time - (elapsed - frame_duration));
                                            } else if elapsed >= frame_duration {
                                                data.prefetch_frame(next);
                                            }
                                        } else {
                                            state.last_frame_time = Some(current_time);
                                        }
                                    } else {
                                        state.last_frame_time = None;
                                    }
                                } else {
                                    state.last_frame_time = None;
                                }
                                if !loading {
                                    state.started_loading = None;
                                }
                                frame_index = state.frame_index;
                            }

                            let image_size = data.render_size(frame_index);

                            if style.aspect_ratio.is_none() {
                                style.aspect_ratio = Some(image_size.width / image_size.height);
                            }

                            if let Length::Auto = style.size.width {
                                style.size.width = match style.size.height {
                                    Length::Definite(DefiniteLength::Absolute(abs_length)) => {
                                        let height_px = abs_length.to_pixels(window.rem_size());
                                        Length::Definite(
                                            px(image_size.width.0 * height_px.0
                                                / image_size.height.0)
                                            .into(),
                                        )
                                    }
                                    _ => Length::Definite(image_size.width.into()),
                                };
                            }

                            if let Length::Auto = style.size.height {
                                style.size.height = match style.size.width {
                                    Length::Definite(DefiniteLength::Absolute(abs_length)) => {
                                        let width_px = abs_length.to_pixels(window.rem_size());
                                        Length::Definite(
                                            px(image_size.height.0 * width_px.0
                                                / image_size.width.0)
                                            .into(),
                                        )
                                    }
                                    _ => Length::Definite(image_size.height.into()),
                                };
                            }

                            if global_id.is_some()
                                && data.frame_count() > 1
                                && window.is_window_active()
                                && !cx.reduce_motion()
                            {
                                window.request_animation_frame();
                            }
                        }
                        Some(_err) => {
                            if let Some(fallback) = self.style.fallback.as_ref() {
                                let mut element = fallback();
                                replacement_id = Some(element.request_layout(window, cx));
                                layout_state.replacement = Some(element);
                            }
                            if let Some(state) = &mut state {
                                state.started_loading = None;
                            }
                        }
                        None => loading = true,
                    }
                    if loading {
                        if let Some(state) = &mut state {
                            if let Some((started_loading, _)) = state.started_loading {
                                if started_loading.elapsed() > LOADING_DELAY
                                    && let Some(loading) = self.style.loading.as_ref()
                                {
                                    let mut element = loading();
                                    replacement_id = Some(element.request_layout(window, cx));
                                    layout_state.replacement = Some(element);
                                }
                            } else {
                                let current_view = window.current_view();
                                let task = window.spawn(cx, async move |cx| {
                                    cx.background_executor().timer(LOADING_DELAY).await;
                                    cx.update(move |_, cx| {
                                        cx.notify(current_view);
                                    })
                                    .ok();
                                });
                                state.started_loading = Some((Instant::now(), task));
                            }
                        }
                    }

                    window.request_layout(style, replacement_id, cx)
                },
            );

            layout_state.frame_index = frame_index;

            ((layout_id, layout_state), state)
        })
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.interactivity.prepaint(
            global_id,
            inspector_id,
            bounds,
            bounds.size,
            window,
            cx,
            |_, _, hitbox, window, cx| {
                if let Some(replacement) = &mut request_layout.replacement {
                    replacement.prepaint(window, cx);
                }

                hitbox
            },
        )
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout_state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let source = self.source.clone();
        self.interactivity.paint(
            global_id,
            inspector_id,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |style, window, cx| {
                if layout_state.intrinsic_size.width.0 == 0
                    || layout_state.intrinsic_size.height.0 == 0
                {
                    if let Some(replacement) = &mut layout_state.replacement {
                        replacement.paint(window, cx);
                    }
                    return;
                }
                let image_bounds = self.style.object_fit.get_aligned_bounds(
                    bounds,
                    layout_state.intrinsic_size,
                    self.style.object_position,
                );
                if image_bounds.size.width <= px(0.0) || image_bounds.size.height <= px(0.0) {
                    return;
                }
                let target = crate::size(
                    ((f32::from(image_bounds.size.width) * window.scale_factor()).ceil() as u32)
                        .into(),
                    ((f32::from(image_bounds.size.height) * window.scale_factor()).ceil() as u32)
                        .into(),
                );
                if let Some(Ok(data)) = source.use_frame_at_size(
                    self.image_cache
                        .clone()
                        .or_else(|| window.image_cache_stack.last().cloned()),
                    Some(target),
                    crate::ImageFrameRequest {
                        index: layout_state.frame_index,
                        playback: layout_state.playback.clone(),
                    },
                    window,
                    cx,
                ) {
                    if data.frame_count() == 0 {
                        return;
                    }
                    let new_bounds = self.style.object_fit.get_aligned_bounds(
                        bounds,
                        data.intrinsic_size(layout_state.frame_index),
                        self.style.object_position,
                    );
                    let corner_radii = style.corner_radii.to_pixels(window.rem_size());
                    window
                        .paint_image(
                            bounds,
                            new_bounds,
                            corner_radii,
                            data,
                            layout_state.frame_index,
                            self.style.grayscale,
                        )
                        .log_err();
                } else if let Some(replacement) = &mut layout_state.replacement {
                    replacement.paint(window, cx);
                }
            },
        )
    }
}

impl Styled for Img {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl InteractiveElement for Img {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

impl IntoElement for Img {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl StatefulInteractiveElement for Img {}

impl ImageSource {
    #[cfg(test)]
    fn use_data_at_size(
        &self,
        cache: Option<AnyImageCache>,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.use_frame_at_size(
            cache,
            bounds,
            crate::ImageFrameRequest::default(),
            window,
            cx,
        )
    }

    fn use_frame_at_size(
        &self,
        cache: Option<AnyImageCache>,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        frame: crate::ImageFrameRequest,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let resource = match self {
            Self::Resource(resource) => resource.clone(),
            Self::Image(image) => Resource::Image(image.clone()),
            _ => return self.use_data(cache, window, cx),
        };
        let cache = cache.unwrap_or_else(|| {
            if cx.try_global::<DefaultImageCache>().is_none() {
                let cache = crate::RecentImageCache::shared(cx);
                cx.set_global(DefaultImageCache(cache));
            }
            cx.global::<DefaultImageCache>().0.clone().into()
        });
        cache.load_at_size(&resource, bounds, frame, window, cx)
    }
    pub(crate) fn use_data(
        &self,
        cache: Option<AnyImageCache>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        match self {
            ImageSource::Resource(resource) => {
                if let Some(cache) = cache {
                    cache.load(resource, window, cx)
                } else {
                    window.use_asset::<ImgResourceLoader>(resource, cx)
                }
            }
            ImageSource::Custom(loading_fn) => loading_fn(window, cx),
            ImageSource::Render(data) => Some(Ok(data.to_owned())),
            ImageSource::Image(data) => window.use_asset::<AssetLogger<ImageDecoder>>(data, cx),
        }
    }

    pub(crate) fn get_data(
        &self,
        cache: Option<AnyImageCache>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        match self {
            ImageSource::Resource(resource) => {
                if let Some(cache) = cache {
                    cache.load(resource, window, cx)
                } else {
                    window.get_asset::<ImgResourceLoader>(resource, cx)
                }
            }
            ImageSource::Custom(loading_fn) => loading_fn(window, cx),
            ImageSource::Render(data) => Some(Ok(data.to_owned())),
            ImageSource::Image(data) => window.get_asset::<AssetLogger<ImageDecoder>>(data, cx),
        }
    }

    /// Remove this image source from the asset system
    pub fn remove_asset(&self, cx: &mut App) {
        let resource = match self {
            Self::Resource(resource) => Some(resource.clone()),
            Self::Image(image) => Some(Resource::Image(image.clone())),
            _ => None,
        };
        if let Some(resource) = resource
            && let Some(cache) = cx
                .try_global::<DefaultImageCache>()
                .map(|cache| cache.0.clone())
        {
            cache.update(cx, |cache, cx| cache.remove_from_all_windows(&resource, cx));
        }
        match self {
            ImageSource::Resource(resource) => {
                cx.remove_asset::<ImgResourceLoader>(resource);
            }
            ImageSource::Custom(_) | ImageSource::Render(_) => {}
            ImageSource::Image(data) => cx.remove_asset::<AssetLogger<ImageDecoder>>(data),
        }
    }

    /// Check whether this image source is present in the asset system (loading
    /// or loaded), without fetching it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_asset_cached(&self, cx: &App) -> bool {
        match self {
            ImageSource::Resource(resource) => cx.has_asset::<ImgResourceLoader>(resource),
            ImageSource::Custom(_) | ImageSource::Render(_) => false,
            ImageSource::Image(data) => cx.has_asset::<AssetLogger<ImageDecoder>>(data),
        }
    }
}

struct DefaultImageCache(Entity<crate::RecentImageCache>);
impl crate::Global for DefaultImageCache {}

pub(crate) fn finish_default_image_cache(
    painted: &std::collections::HashSet<crate::ImageId>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(cache) = cx
        .try_global::<DefaultImageCache>()
        .map(|cache| cache.0.clone())
    {
        cache.update(cx, |cache, cx| {
            cache.finish_window_frame(painted, window, cx)
        });
    }
}

#[derive(Clone)]
enum ImageDecoder {}

impl Asset for ImageDecoder {
    type Source = Arc<Image>;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let renderer = cx.svg_renderer();
        async move { source.to_image_data(renderer).map_err(Into::into) }
    }
}

/// An image loader for the GPUI asset system
#[derive(Clone)]
pub enum ImageAssetLoader {}

impl Asset for ImageAssetLoader {
    type Source = Resource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let bytes = Self::read_bytes(source.clone(), cx);
        let renderer = cx.svg_renderer();
        async move {
            let bytes = bytes.await?;
            if matches!(source, Resource::RotatedImage { .. }) {
                sized::SourceInfo::new(source, Some(bytes))?.decode((u32::MAX, u32::MAX), renderer)
            } else {
                Self::decode_bytes(&bytes, renderer)
            }
        }
    }
}

impl ImageAssetLoader {
    pub(crate) fn read_bytes(
        source: Resource,
        cx: &App,
    ) -> impl Future<Output = Result<Vec<u8>, ImageCacheError>> + Send + 'static {
        let client = cx.http_client();
        let asset_source = cx.asset_source().clone();
        async move {
            let bytes = match source.clone() {
                Resource::Image(image) => image.bytes.clone(),
                Resource::Path(uri) | Resource::RotatedImage { path: uri, .. } => {
                    fs::read(uri.as_ref())?
                }
                Resource::Uri(uri) => {
                    use anyhow::Context as _;
                    use futures::AsyncReadExt as _;

                    let mut response = client
                        .get(uri.as_ref(), ().into(), true)
                        .await
                        .with_context(|| format!("loading image asset from {uri:?}"))?;
                    let mut body = Vec::new();
                    response.body_mut().read_to_end(&mut body).await?;
                    if !response.status().is_success() {
                        let mut error_body = String::from_utf8_lossy(&body).into_owned();
                        let first_line = error_body.lines().next().unwrap_or("").trim_end();
                        error_body.truncate(first_line.len());
                        return Err(ImageCacheError::BadStatus {
                            uri,
                            status: response.status(),
                            body: error_body,
                        });
                    }
                    body
                }
                Resource::Embedded(path) => {
                    let data = asset_source.load(&path).ok().flatten();
                    if let Some(data) = data {
                        data.to_vec()
                    } else {
                        return Err(ImageCacheError::Asset(
                            format!("Embedded resource not found: {}", path).into(),
                        ));
                    }
                }
            };
            Ok(bytes)
        }
    }

    pub(crate) fn decode_bytes(
        bytes: &[u8],
        svg_renderer: crate::SvgRenderer,
    ) -> Result<Arc<RenderImage>, ImageCacheError> {
        if let Ok(format) = image::guess_format(&bytes) {
            let data = match format {
                ImageFormat::Gif => {
                    let decoder = GifDecoder::new(Cursor::new(&bytes))?;
                    let mut frames = RenderImage::new(SmallVec::<[image::Frame; 1]>::new());

                    for frame in decoder.into_frames() {
                        match frame {
                            Ok(mut frame) => {
                                // Convert from RGBA to BGRA.
                                for pixel in frame.buffer_mut().chunks_exact_mut(4) {
                                    pixel.swap(0, 2);
                                }
                                frames.push_decoded_frame(frame)?;
                            }
                            Err(err) => {
                                log::debug!("Skipping GIF frame due to decode error: {err}");
                            }
                        }
                    }

                    if frames.frame_count() == 0 {
                        return Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                            "GIF could not be decoded: all frames failed"
                        ))));
                    }

                    frames
                }
                ImageFormat::WebP => {
                    let mut decoder = WebPDecoder::new(Cursor::new(&bytes))?;

                    if decoder.has_animation() {
                        let _ = decoder.set_background_color(Rgba([0, 0, 0, 0]));
                        let mut frames = RenderImage::new(SmallVec::<[image::Frame; 1]>::new());

                        for frame in decoder.into_frames() {
                            match frame {
                                Ok(mut frame) => {
                                    // Convert from RGBA to BGRA.
                                    for pixel in frame.buffer_mut().chunks_exact_mut(4) {
                                        pixel.swap(0, 2);
                                    }
                                    frames.push_decoded_frame(frame)?;
                                }
                                Err(err) => {
                                    log::debug!("Skipping WebP frame due to decode error: {err}");
                                }
                            }
                        }

                        if frames.frame_count() == 0 {
                            return Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                                "WebP could not be decoded: all frames failed"
                            ))));
                        }

                        frames
                    } else {
                        RenderImage::from_decoded_frames(decode_static_image_from_decoder(
                            decoder,
                        )?)?
                    }
                }
                _ => RenderImage::from_decoded_frames(decode_static_image(&bytes, format)?)?,
            };

            Ok(Arc::new(data))
        } else {
            svg_renderer
                .render_single_frame(&bytes, 1.0)
                .map_err(Into::into)
        }
    }
}

/// An error that can occur when interacting with the image cache.
#[derive(Debug, Error, Clone)]
pub enum ImageCacheError {
    /// Some other kind of error occurred
    #[error("error: {0}")]
    Other(#[from] Arc<anyhow::Error>),
    /// An error that occurred while reading the image from disk.
    #[error("IO error: {0}")]
    Io(Arc<std::io::Error>),
    /// An error that occurred while processing an image.
    #[error("unexpected http status for {uri}: {status}, body: {body}")]
    BadStatus {
        /// The URI of the image.
        uri: SharedUri,
        /// The HTTP status code.
        status: http::StatusCode,
        /// The HTTP response body.
        body: String,
    },
    /// An error that occurred while processing an asset.
    #[error("asset error: {0}")]
    Asset(SharedString),
    /// An error that occurred while processing an image.
    #[error("image error: {0}")]
    Image(Arc<ImageError>),
    /// An error that occurred while processing an SVG.
    #[error("svg error: {0}")]
    Usvg(Arc<usvg::Error>),
}

impl From<anyhow::Error> for ImageCacheError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(Arc::new(value))
    }
}

impl From<io::Error> for ImageCacheError {
    fn from(value: io::Error) -> Self {
        Self::Io(Arc::new(value))
    }
}

impl From<usvg::Error> for ImageCacheError {
    fn from(value: usvg::Error) -> Self {
        Self::Usvg(Arc::new(value))
    }
}

impl From<image::ImageError> for ImageCacheError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(Arc::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParentElement as _, TestAppContext, canvas, div, point, px, size};
    use image::{Frame, ImageBuffer, Rgba};

    const TEST_IMG_ID: &str = "test-img";

    #[gpui::test]
    fn animated_resize_keeps_playback_position_delays_loop_and_pause(cx: &mut TestAppContext) {
        struct View {
            source: ImageSource,
            width: f32,
            displayed: std::rc::Rc<
                std::cell::RefCell<Option<(crate::RenderImageParams, Arc<RenderImage>)>>,
            >,
            elapsed: std::rc::Rc<std::cell::Cell<Option<Duration>>>,
        }
        impl crate::Render for View {
            fn render(&mut self, _: &mut Window, _: &mut crate::Context<Self>) -> impl IntoElement {
                let image = img(self.source.clone())
                    .id(TEST_IMG_ID)
                    .w(px(self.width))
                    .h(px(self.width / 2.0));
                let displayed = self.displayed.clone();
                let source = self.source.clone();
                let elapsed = self.elapsed.clone();
                div().child(image).child(canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        let (frame_index, playback) =
                            window.with_global_id(TEST_IMG_ID.into(), |id, window| {
                                window.with_element_state::<ImgState, _>(id, |state, _| {
                                    let mut state = state.unwrap();
                                    // Img uses wall-clock Instant, not the executor's virtual clock.
                                    // Seed its existing timestamp, then let the next layout advance it.
                                    if let Some(duration) = elapsed.take() {
                                        state.last_frame_time = Some(Instant::now() - duration);
                                    }
                                    ((state.frame_index, state.playback.clone()), state)
                                })
                            });
                        *displayed.borrow_mut() = source
                            .use_frame_at_size(
                                None,
                                None,
                                crate::ImageFrameRequest {
                                    index: frame_index,
                                    playback: Some(playback),
                                },
                                window,
                                cx,
                            )
                            .and_then(Result::ok)
                            .filter(|image| image.frame_count() > 0)
                            .map(|image| {
                                (
                                    crate::RenderImageParams {
                                        image_id: image.id,
                                        frame_index,
                                    },
                                    image,
                                )
                            });
                    },
                ))
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder =
                gif::Encoder::new(&mut bytes, 512, 256, &[255, 0, 0, 0, 255, 0, 0, 0, 255])
                    .unwrap();
            for (color, delay) in [(0, 10), (1, 20), (2, 30)] {
                encoder
                    .write_frame(&gif::Frame {
                        width: 512,
                        height: 256,
                        delay,
                        buffer: std::borrow::Cow::Owned(vec![color; 512 * 256]),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        let displayed = std::rc::Rc::new(std::cell::RefCell::new(None));
        let elapsed = std::rc::Rc::new(std::cell::Cell::new(None));
        let (view, cx) = cx.add_window_view(|_, _| View {
            source: ImageSource::Image(Arc::new(Image::from_bytes(crate::ImageFormat::Gif, bytes))),
            width: 128.0,
            displayed: displayed.clone(),
            elapsed: elapsed.clone(),
        });
        cx.simulate_scale_factor_change(2.0);
        let draw = |cx: &mut crate::VisualTestContext| {
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
            displayed
                .borrow()
                .as_ref()
                .map(|(params, _)| params.clone())
        };
        let native = cx.update(|window, _| window.platform_window.as_test().unwrap().clone());
        native.simulate_active_status_change(true);
        let advance = |cx: &mut crate::VisualTestContext, millis| {
            elapsed.set(Some(Duration::from_millis(millis)));
            draw(cx);
            draw(cx).unwrap()
        };
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        let small = draw(cx).unwrap();
        let old_pixels = Arc::downgrade(&displayed.borrow().as_ref().unwrap().1);
        assert_eq!(
            displayed.borrow().as_ref().unwrap().1.size(0),
            size(256.into(), 128.into())
        );
        assert_eq!(small.frame_index, 0);
        assert_eq!(advance(cx, 100).frame_index, 1);
        view.update(cx, |view, cx| {
            view.width = 512.0;
            cx.notify();
        });
        let pending = draw(cx).unwrap();
        assert_eq!(pending.frame_index, 1);
        assert_eq!(pending.image_id, small.image_id);
        cx.run_until_parked();
        let large = draw(cx).unwrap();
        assert_eq!(
            large.frame_index, 1,
            "a sharper decode must not restart playback"
        );
        assert_ne!(large.image_id, small.image_id);
        assert_eq!(
            displayed.borrow().as_ref().unwrap().1.size(0),
            size(512.into(), 256.into())
        );
        assert!(
            old_pixels.upgrade().is_none(),
            "obsolete animation frame set remained cached"
        );
        let input = view.read_with(cx, |view, _| match &view.source {
            ImageSource::Image(image) => image.clone(),
            _ => unreachable!(),
        });
        let streamed = Arc::new(
            AnimationStream::spawn(
                AnimationInput::Image(input),
                ImageFormat::Gif,
                (512, 256),
                (512, 256),
                0,
                [100, 200, 300]
                    .into_iter()
                    .map(|ms| image::Delay::from_numer_denom_ms(ms, 1))
                    .collect(),
            )
            .unwrap(),
        );
        view.update(cx, |view, cx| {
            view.source = ImageSource::Render(streamed.clone());
            cx.notify();
        });
        assert_eq!(draw(cx).unwrap().frame_index, 1);
        for (millis, index) in [(200, 2), (300, 0)] {
            futures::executor::block_on(streamed.prepare_frame(index)).unwrap();
            assert_eq!(advance(cx, millis).frame_index, index);
        }
        cx.update(|_, cx| cx.set_reduce_motion(true));
        assert_eq!(advance(cx, 1000).frame_index, 0);
        cx.update(|_, cx| cx.set_reduce_motion(false));
        draw(cx);
        assert_eq!(advance(cx, 100).frame_index, 1);
        native.simulate_active_status_change(false);
        draw(cx);
        assert_eq!(advance(cx, 1000).frame_index, 1);
        native.simulate_active_status_change(true);
        assert_eq!(draw(cx).unwrap().frame_index, 1);
        futures::executor::block_on(streamed.prepare_frame(2)).unwrap();
        assert_eq!(advance(cx, 200).frame_index, 2);
    }

    #[gpui::test]
    fn display_size_and_dpi_request_sharper_pixels_without_changing_layout(
        cx: &mut TestAppContext,
    ) {
        use image::ImageEncoder;
        struct View {
            source: ImageSource,
            width: f32,
            displayed: std::rc::Rc<std::cell::RefCell<Option<Arc<RenderImage>>>>,
        }
        impl crate::Render for View {
            fn render(&mut self, _: &mut Window, _: &mut crate::Context<Self>) -> impl IntoElement {
                let image = img(self.source.clone())
                    .id("sized-image")
                    .w(px(self.width))
                    .h(px(self.width / 2.0));
                let displayed = self.displayed.clone();
                let source = self.source.clone();
                div().child(image).child(canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        *displayed.borrow_mut() = source
                            .use_data_at_size(None, None, window, cx)
                            .and_then(Result::ok);
                    },
                ))
            }
        }
        let pixels = image::RgbaImage::from_fn(1024, 512, |x, y| {
            image::Rgba([if (x + y) % 2 == 0 { 0 } else { 255 }, 0, 0, 255])
        });
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&pixels, 1024, 512, image::ExtendedColorType::Rgba8)
            .unwrap();
        let source =
            ImageSource::Image(Arc::new(Image::from_bytes(crate::ImageFormat::Png, bytes)));
        let displayed = std::rc::Rc::new(std::cell::RefCell::new(None));
        let (view, cx) = cx.add_window_view(|window, _| {
            window.set_scale_factor(1.0);
            View {
                source: source.clone(),
                width: 128.0,
                displayed: displayed.clone(),
            }
        });
        let draw = |cx: &mut crate::VisualTestContext| {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let low = displayed.borrow().clone().unwrap();
        assert_eq!(low.size(0), size(128.into(), 64.into()));
        assert_eq!(low.render_size(0), size(px(1024.0), px(512.0)));
        view.update(cx, |view, cx| {
            view.width = 512.0;
            cx.notify();
        });
        draw(cx);
        let pending = displayed.borrow().clone().unwrap();
        assert_eq!(
            pending.id, low.id,
            "keep the old image during the sharper decode"
        );
        cx.run_until_parked();
        draw(cx);
        let larger = displayed.borrow().clone().unwrap();
        assert_eq!(larger.size(0), size(512.into(), 256.into()));
        cx.simulate_scale_factor_change(2.0);
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let native = displayed.borrow().clone().unwrap();
        assert_eq!(native.size(0), size(1024.into(), 512.into()));
        assert_eq!(
            &native.as_bytes(0).unwrap()[..8],
            &[0, 0, 0, 255, 0, 0, 255, 255]
        );
        assert_eq!(native.render_size(0), low.render_size(0));
        let obsolete = Arc::downgrade(&low);
        drop(pending);
        drop(low);
        assert!(
            obsolete.upgrade().is_none(),
            "obsolete decode resolution remained cached"
        );
    }

    fn test_image(frame_count: usize) -> Arc<RenderImage> {
        let frame = Frame::new(ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 0])));
        Arc::new(RenderImage::new(SmallVec::from_iter(
            (0..frame_count).map(|_| frame.clone()),
        )))
    }

    fn test_image_with_size(width: u32, height: u32) -> Arc<RenderImage> {
        let frame = Frame::new(ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 0])));
        Arc::new(RenderImage::new(SmallVec::from_elem(frame, 1)))
    }

    /// Overwrites the cached `frame_index` of the sibling `img` during paint.
    fn seed_frame_index(frame_index: usize) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.with_global_id(TEST_IMG_ID.into(), |id, window| {
                    window.with_element_state::<ImgState, _>(id, |state, _| {
                        let mut state = state.expect("img state should be initialized");
                        state.frame_index = frame_index;
                        ((), state)
                    });
                });
            },
        )
    }

    #[gpui::test]
    fn zero_frame_image_does_not_panic_on_paint(cx: &mut TestAppContext) {
        cx.add_empty_window()
            .draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
                img(ImageSource::Render(test_image(0))).into_any_element()
            });
    }

    #[gpui::test]
    fn image_object_fit_cover_crops_to_element_bounds(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();
        let image = test_image_with_size(200, 100);
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(image.clone()))
                .size_full()
                .object_fit(ObjectFit::Fill)
                .into_any_element()
        });
        let full_tile_bounds = window.update(|window, _| {
            window
                .rendered_frame
                .scene
                .polychrome_sprites
                .last()
                .expect("fill image should paint a sprite")
                .tile
                .bounds
        });

        for (position, crop_x) in [(0.0, 0), (0.5, 50), (1.0, 100)] {
            window.draw(point(px(10.), px(20.)), size(px(100.), px(100.)), |_, _| {
                img(ImageSource::Render(image.clone()))
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .object_position((position, 0.5))
                    .into_any_element()
            });

            let (rendered_bounds, rendered_tile_bounds, scale_factor) =
                window.update(|window, _| {
                    let sprite = window
                        .rendered_frame
                        .scene
                        .polychrome_sprites
                        .last()
                        .expect("cover image should paint a sprite");
                    (sprite.bounds, sprite.tile.bounds, window.scale_factor())
                });
            assert_eq!(
                rendered_bounds,
                Bounds {
                    origin: point(px(10.).scale(scale_factor), px(20.).scale(scale_factor)),
                    size: size(px(100.).scale(scale_factor), px(100.).scale(scale_factor)),
                }
            );
            assert_eq!(
                (
                    rendered_tile_bounds.origin.x.0 - full_tile_bounds.origin.x.0,
                    rendered_tile_bounds.origin.y.0 - full_tile_bounds.origin.y.0,
                    rendered_tile_bounds.size.width.0,
                    rendered_tile_bounds.size.height.0,
                ),
                (crop_x, 0, 100, 100),
            );
        }
    }

    #[gpui::test]
    fn explicit_aspect_ratio_is_not_overridden_by_intrinsic_ratio(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();

        // A portrait image in a square container
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            div()
                .size(px(100.))
                .overflow_hidden()
                .child(
                    img(ImageSource::Render(test_image_with_size(100, 200)))
                        .size_full()
                        .aspect_square()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element()
        });

        let (rendered_bounds, scale_factor) = window.update(|window, _| {
            let sprite = window
                .rendered_frame
                .scene
                .polychrome_sprites
                .last()
                .expect("contained image should paint a sprite");
            (sprite.bounds, window.scale_factor())
        });

        // The element stays 100x100, so the image is letterboxed horizontally
        assert_eq!(
            rendered_bounds,
            Bounds {
                origin: point(px(25.).scale(scale_factor), px(0.).scale(scale_factor)),
                size: size(px(50.).scale(scale_factor), px(100.).scale(scale_factor)),
            }
        );
    }

    #[gpui::test]
    fn image_object_fit_cover_clamps_corner_radii_to_visible_bounds(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(test_image_with_size(200, 100)))
                .size_full()
                .rounded(px(100.))
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        });

        let (corner_radius, expected_corner_radius) = window.update(|window, _| {
            (
                window
                    .rendered_frame
                    .scene
                    .polychrome_sprites
                    .last()
                    .map(|sprite| sprite.corner_radii.top_left),
                px(50.).scale(window.scale_factor()),
            )
        });
        assert_eq!(corner_radius, Some(expected_corner_radius));
    }

    #[gpui::test]
    fn stale_frame_index_is_clamped_when_image_changes(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();

        // Assert that a cached frame_index from a previous multi-frame image
        // does not cause an out-of-bounds panic when the image is replaced
        // with one that has fewer frames.
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            div()
                .child(img(ImageSource::Render(test_image(5))).id(TEST_IMG_ID))
                .child(seed_frame_index(4))
                .into_any_element()
        });
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(test_image(1)))
                .id(TEST_IMG_ID)
                .into_any_element()
        });
    }
}
