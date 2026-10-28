use crate::{
    AnyElement, AnyEntity, App, AppContext, Asset, AssetLogger, Bounds, Element, ElementId, Entity,
    GlobalElementId, ImageAssetLoader, ImageCacheError, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, RenderImage, Resource, Style, StyleRefinement, Styled, Window, hash,
};

use crate::asset_cache::CachedLoad;
use refineable::Refineable;
use smallvec::SmallVec;
use std::{collections::HashMap, fmt, sync::Arc};

mod bounded;
pub(crate) use bounded::RecentImageCache;
pub use bounded::retain_recent;

/// An image cache element, all its child img elements will use the cache specified by this element.
/// Note that this could as simple as passing an `Entity<T: ImageCache>`
pub fn image_cache(image_cache_provider: impl ImageCacheProvider) -> ImageCacheElement {
    ImageCacheElement {
        image_cache_provider: Box::new(image_cache_provider),
        style: StyleRefinement::default(),
        children: SmallVec::default(),
    }
}

/// A dynamically typed image cache, which can be used to store any image cache
#[derive(Clone)]
pub struct AnyImageCache {
    image_cache: AnyEntity,
    load_fn: fn(
        image_cache: &AnyEntity,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>>,
    sized_fn: fn(
        &AnyEntity,
        &Resource,
        Option<crate::Size<crate::DevicePixels>>,
        ImageFrameRequest,
        &mut Window,
        &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>>,
}

impl<I: ImageCache> From<Entity<I>> for AnyImageCache {
    fn from(image_cache: Entity<I>) -> Self {
        Self {
            image_cache: image_cache.into_any(),
            load_fn: any_image_cache::load::<I>,
            sized_fn: any_image_cache::load_at_size::<I>,
        }
    }
}

impl AnyImageCache {
    pub(crate) fn load_at_size(
        &self,
        source: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        frame: ImageFrameRequest,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        (self.sized_fn)(&self.image_cache, source, bounds, frame, window, cx)
    }
    /// Load an image given a resource
    /// returns the result of loading the image if it has finished loading, or None if it is still loading
    pub fn load(
        &self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        (self.load_fn)(&self.image_cache, resource, window, cx)
    }
}

mod any_image_cache {
    use super::*;

    pub(crate) fn load_at_size<I: 'static + ImageCache>(
        cache: &AnyEntity,
        source: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        frame: ImageFrameRequest,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        cache
            .clone()
            .downcast::<I>()
            .unwrap()
            .update(cx, |cache, cx| {
                cache.load_frame_at_size(source, bounds, frame, window, cx)
            })
    }

    pub(crate) fn load<I: 'static + ImageCache>(
        image_cache: &AnyEntity,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let image_cache = image_cache.clone().downcast::<I>().unwrap();
        image_cache.update(cx, |image_cache, cx| image_cache.load(resource, window, cx))
    }
}

/// An image cache element.
pub struct ImageCacheElement {
    image_cache_provider: Box<dyn ImageCacheProvider>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl ParentElement for ImageCacheElement {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Styled for ImageCacheElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for ImageCacheElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ImageCacheElement {
    type RequestLayoutState = SmallVec<[LayoutId; 4]>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let image_cache = self.image_cache_provider.provide(window, cx);
        window.with_image_cache(Some(image_cache), |window| {
            let child_layout_ids = self
                .children
                .iter_mut()
                .map(|child| child.request_layout(window, cx))
                .collect::<SmallVec<_>>();
            let mut style = Style::default();
            style.refine(&self.style);
            let layout_id = window.request_layout(style, child_layout_ids.iter().copied(), cx);
            (layout_id, child_layout_ids)
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        for child in &mut self.children {
            child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let image_cache = self.image_cache_provider.provide(window, cx);
        window.with_image_cache(Some(image_cache), |window| {
            for child in &mut self.children {
                child.paint(window, cx);
            }
        })
    }
}

/// Owns an image load and its cached result.
///
/// Dropping the entry cancels a pending load. Image caches must remove loaded
/// images from windows before discarding the entry.
pub struct ImageCacheItem {
    load: ImageCacheLoad,
    players: std::cell::RefCell<HashMap<usize, ImagePlayer>>,
    last_player: std::cell::Cell<Option<usize>>,
}

struct ImagePlayer {
    owner: std::sync::Weak<()>,
    image: super::img::sized::SizedImage,
}

/// The requested frame and the lifetime of its independent playback cursor.
#[derive(Clone, Default)]
pub struct ImageFrameRequest {
    /// Logical frame number, independent of decode resolution.
    pub index: usize,
    pub(crate) playback: Option<Arc<()>>,
}

enum ImageCacheLoad {
    Full(CachedLoad<Result<Arc<RenderImage>, ImageCacheError>>),
    Sized(std::cell::RefCell<super::img::sized::SizedImage>),
}

impl std::fmt::Debug for ImageCacheItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageCacheItem")
            .field("result", &self.get())
            .finish()
    }
}

impl ImageCacheItem {
    fn from_load(load: ImageCacheLoad) -> Self {
        Self {
            load,
            players: Default::default(),
            last_player: Default::default(),
        }
    }

    fn prune_players(&self) {
        self.players
            .borrow_mut()
            .retain(|_, player| player.owner.strong_count() > 0);
    }

    fn base_image(&self) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        match &self.load {
            ImageCacheLoad::Full(load) => load.get(),
            ImageCacheLoad::Sized(load) => load.borrow().get(),
        }
    }

    fn contains_image(&self, painted: &std::collections::HashSet<crate::ImageId>) -> bool {
        self.prune_players();
        let base = match &self.load {
            ImageCacheLoad::Full(load) => load
                .get()
                .and_then(Result::ok)
                .is_some_and(|image| painted.contains(&image.id)),
            ImageCacheLoad::Sized(load) => load.borrow().contains_image(painted),
        };
        base || self
            .players
            .borrow()
            .values()
            .any(|player| player.image.contains_image(painted))
    }

    /// Starts loading an image into a new cache entry.
    pub fn new(source: &Resource, cx: &mut App) -> Self {
        Self::from_load(ImageCacheLoad::Full(CachedLoad::new(
            AssetLogger::<ImageAssetLoader>::load(source.clone(), cx),
            cx,
        )))
    }

    /// Returns a cached result without subscribing to completion notifications.
    pub fn get(&self) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.prune_players();
        let players = self.players.borrow();
        self.last_player
            .get()
            .and_then(|id| players.get(&id))
            .and_then(|player| player.image.get())
            .or_else(|| self.base_image())
            .or_else(|| players.values().find_map(|player| player.image.get()))
    }

    /// Returns the cached result or subscribes the current view to completion.
    pub fn use_image(&self, window: &Window) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        match &self.load {
            ImageCacheLoad::Full(load) => load.use_by(window.current_view()),
            ImageCacheLoad::Sized(_) => self.get(),
        }
    }

    pub(crate) fn new_sized(source: &Resource, cx: &App) -> Self {
        Self::from_load(ImageCacheLoad::Sized(std::cell::RefCell::new(
            super::img::sized::SizedImage::new(source.clone(), cx),
        )))
    }

    pub(crate) fn use_at_size(
        &self,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        window: &Window,
        cx: &App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.use_frame_at_size(bounds, ImageFrameRequest::default(), window, cx)
    }

    fn use_frame_at_size(
        &self,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        frame: ImageFrameRequest,
        window: &Window,
        cx: &App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.prune_players();
        if let ImageCacheLoad::Sized(load) = &self.load
            && let Some(owner) = frame.playback
        {
            let animated = load.borrow().is_animation(window)?;
            if animated {
                let id = Arc::as_ptr(&owner) as usize;
                self.last_player.set(Some(id));
                let mut players = self.players.borrow_mut();
                let player = players.entry(id).or_insert_with(|| ImagePlayer {
                    owner: Arc::downgrade(&owner),
                    image: load.borrow().fork_player(),
                });
                return player.image.use_image(bounds, frame.index, window, cx);
            }
        }
        self.last_player.set(None);
        match &self.load {
            ImageCacheLoad::Full(_) => self.use_image(window),
            ImageCacheLoad::Sized(load) => {
                load.borrow_mut().use_image(bounds, frame.index, window, cx)
            }
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.prune_players();
        let base = match &self.load {
            ImageCacheLoad::Full(_) => self
                .base_image()
                .and_then(Result::ok)
                .map_or(0, |image| image.retained_bytes()),
            ImageCacheLoad::Sized(load) => load.borrow().retained_bytes(),
        };
        // Players share the source load, so count encoded bytes once.
        base + self
            .players
            .borrow()
            .values()
            .map(|player| player.image.image_bytes())
            .sum::<usize>()
    }

    fn estimated_render_bytes(&self) -> usize {
        fn cost(image: Option<Result<Arc<RenderImage>, ImageCacheError>>) -> usize {
            image.and_then(Result::ok).map_or(0, |image| {
                let texture = (0..image.frame_count())
                    .map(|index| {
                        let size = image.size(index);
                        (size.width.0 as usize)
                            .saturating_mul(size.height.0 as usize)
                            .saturating_mul(4)
                    })
                    .max()
                    .unwrap_or(0);
                texture.saturating_add(image.estimated_decode_working_bytes())
            })
        }
        self.prune_players();
        cost(self.base_image())
            + self
                .players
                .borrow()
                .values()
                .map(|player| cost(player.image.get()))
                .sum::<usize>()
    }
}

/// An object that can handle the caching and unloading of images.
/// Implementations of this trait should ensure that images are removed from all windows when they are no longer needed.
pub trait ImageCache: 'static {
    /// Request a playback frame without expanding the whole animation.
    fn load_frame_at_size(
        &mut self,
        resource: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        _frame: ImageFrameRequest,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.load_at_size(resource, bounds, window, cx)
    }
    /// Layout passes None; painting supplies the required device-pixel dimensions.
    /// Existing cache implementations keep their original loading behavior by default.
    fn load_at_size(
        &mut self,
        resource: &Resource,
        _bounds: Option<crate::Size<crate::DevicePixels>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.load(resource, window, cx)
    }
    /// Load an image given a resource
    /// returns the result of loading the image if it has finished loading, or None if it is still loading
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>>;
}

/// An object that can create an ImageCache during the render phase.
/// See the ImageCache trait for more information.
pub trait ImageCacheProvider: 'static {
    /// Called during the request_layout phase to create an ImageCache.
    fn provide(&mut self, _window: &mut Window, _cx: &mut App) -> AnyImageCache;
}

impl<T: ImageCache> ImageCacheProvider for Entity<T> {
    fn provide(&mut self, _window: &mut Window, _cx: &mut App) -> AnyImageCache {
        self.clone().into()
    }
}

/// An implementation of ImageCache, that uses an LRU caching strategy to unload images when the cache is full
pub struct RetainAllImageCache(HashMap<u64, ImageCacheItem>);

impl fmt::Debug for RetainAllImageCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashMapImageCache")
            .field("num_images", &self.0.len())
            .finish()
    }
}

impl RetainAllImageCache {
    /// Create a new image cache.
    #[inline]
    pub fn new(cx: &mut App) -> Entity<Self> {
        let e = cx.new(|_cx| RetainAllImageCache(HashMap::new()));
        cx.observe_release(&e, |image_cache, cx| {
            for (_, item) in std::mem::replace(&mut image_cache.0, HashMap::new()) {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        e
    }

    /// Load an image from the given source.
    ///
    /// Returns `None` if the image is loading.
    pub fn load(
        &mut self,
        source: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let hash = hash(source);

        self.0
            .entry(hash)
            .or_insert_with(|| ImageCacheItem::new(source, cx))
            .use_at_size(
                Some(crate::size(i32::MAX.into(), i32::MAX.into())),
                window,
                cx,
            )
    }

    /// Clear the image cache.
    pub fn clear(&mut self, window: &mut Window, cx: &mut App) {
        for (_, item) in std::mem::replace(&mut self.0, HashMap::new()) {
            if let Some(Ok(image)) = item.get() {
                cx.drop_image(image, Some(window));
            }
        }
    }

    /// Remove the image from the cache by the given source.
    pub fn remove(&mut self, source: &Resource, window: &mut Window, cx: &mut App) {
        let hash = hash(source);
        if let Some(item) = self.0.remove(&hash)
            && let Some(Ok(image)) = item.get()
        {
            cx.drop_image(image, Some(window));
        }
    }

    /// Returns the number of images in the cache.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl ImageCache for RetainAllImageCache {
    fn load_at_size(
        &mut self,
        source: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.load_frame_at_size(source, bounds, ImageFrameRequest::default(), window, cx)
    }

    fn load_frame_at_size(
        &mut self,
        source: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        frame: ImageFrameRequest,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.0
            .entry(hash(source))
            .or_insert_with(|| ImageCacheItem::new_sized(source, cx))
            .use_frame_at_size(bounds, frame, window, cx)
    }
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        RetainAllImageCache::load(self, resource, window, cx)
    }
}

/// Constructs a retain-all image cache that uses the element state associated with the given ID.
pub fn retain_all(id: impl Into<ElementId>) -> RetainAllImageCacheProvider {
    RetainAllImageCacheProvider { id: id.into() }
}

/// A provider struct for creating a retain-all image cache inline
pub struct RetainAllImageCacheProvider {
    id: ElementId,
}

impl ImageCacheProvider for RetainAllImageCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> AnyImageCache {
        window
            .with_global_id(self.id.clone(), |global_id, window| {
                window.with_element_state::<Entity<RetainAllImageCache>, _>(
                    global_id,
                    |cache, _window| {
                        let mut cache = cache.unwrap_or_else(|| RetainAllImageCache::new(cx));
                        (cache.clone(), cache)
                    },
                )
            })
            .into()
    }
}
