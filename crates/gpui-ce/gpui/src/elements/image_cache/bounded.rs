use super::*;
use std::collections::HashSet;

struct Entry {
    image: ImageCacheItem,
    used: u64,
}

pub(crate) struct RecentImageCache {
    entries: HashMap<u64, Entry>,
    visible: HashSet<u64>,
    clock: u64,
    budget: usize,
    windows: Option<HashMap<crate::WindowId, WindowImages>>,
}

#[derive(Default)]
struct WindowImages {
    requested: HashSet<u64>,
    retained: HashSet<u64>,
}

impl RecentImageCache {
    fn new(budget: usize, cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_| Self {
            entries: HashMap::new(),
            visible: HashSet::new(),
            clock: 0,
            budget,
            windows: None,
        });
        cx.observe_release(&entity, |cache, cx| {
            for entry in cache.entries.values() {
                if let Some(Ok(image)) = entry.image.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        entity
    }

    fn begin_frame(&mut self, window: &mut Window, cx: &mut App) {
        self.evict(window, cx);
        self.visible.clear();
    }

    pub(crate) fn shared(cx: &mut App) -> Entity<Self> {
        let cache = Self::new(64 * 1024 * 1024, cx);
        cache.update(cx, |cache, _| cache.windows = Some(HashMap::new()));
        cache
    }

    pub(crate) fn finish_window_frame(
        &mut self,
        painted: &HashSet<crate::ImageId>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let windows = self.windows.as_mut().expect("shared cache");
        let live: HashSet<_> = cx
            .windows()
            .into_iter()
            .map(|window| window.window_id())
            .collect();
        windows.retain(|id, _| live.contains(id));
        let usage = windows
            .entry(window.window_handle().window_id())
            .or_default();
        usage.retained = std::mem::take(&mut usage.requested);
        // Cached paint subtrees do not call load again; their image IDs still pin the source.
        for (&key, entry) in &self.entries {
            if entry.image.contains_image(painted) {
                usage.retained.insert(key);
            }
        }
        self.visible = windows
            .values()
            .flat_map(|usage| usage.retained.iter().chain(&usage.requested))
            .copied()
            .collect();
        self.evict(window, cx);
    }

    fn mark_used(&mut self, key: u64, window: &Window) {
        self.visible.insert(key);
        if let Some(windows) = &mut self.windows {
            windows
                .entry(window.window_handle().window_id())
                .or_default()
                .requested
                .insert(key);
        }
    }

    pub(crate) fn remove_from_all_windows(&mut self, source: &Resource, cx: &mut App) {
        if let Some(entry) = self.entries.remove(&hash(source))
            && let Some(Ok(image)) = entry.image.get()
        {
            cx.drop_image(image, None);
        }
    }

    fn evict(&mut self, window: &mut Window, cx: &mut App) {
        let mut bytes = 0usize;
        let mut cold = Vec::new();
        for (&key, entry) in &self.entries {
            let cost = entry
                .image
                .retained_bytes()
                .saturating_add(entry.image.estimated_render_bytes());
            bytes = bytes.saturating_add(cost);
            if !self.visible.contains(&key) {
                cold.push((
                    entry.used,
                    key,
                    cost,
                    !matches!(entry.image.get(), Some(Ok(_))),
                ));
            }
        }
        cold.sort_unstable();
        for (_, key, size, unfinished) in cold {
            if !unfinished && self.budget != 0 && bytes <= self.budget {
                continue;
            }
            if let Some(entry) = self.entries.remove(&key) {
                if let Some(Ok(image)) = entry.image.get() {
                    cx.drop_image(image, Some(window));
                }
                bytes = bytes.saturating_sub(size);
            }
        }
        // Visible images may exceed the budget; evicting them during layout
        // would continuously reload the same images instead of displaying them.
    }
}

impl ImageCache for RecentImageCache {
    fn load_at_size(
        &mut self,
        resource: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.load_frame_at_size(resource, bounds, ImageFrameRequest::default(), window, cx)
    }

    fn load_frame_at_size(
        &mut self,
        resource: &Resource,
        bounds: Option<crate::Size<crate::DevicePixels>>,
        frame: ImageFrameRequest,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);
        self.clock += 1;
        self.mark_used(key, window);
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            image: ImageCacheItem::new_sized(resource, cx),
            used: 0,
        });
        entry.used = self.clock;
        entry.image.use_frame_at_size(bounds, frame, window, cx)
    }
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);
        self.clock += 1;
        self.mark_used(key, window);
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            image: ImageCacheItem::new(resource, cx),
            used: 0,
        });
        entry.used = self.clock;
        entry.image.use_at_size(
            Some(crate::size(i32::MAX.into(), i32::MAX.into())),
            window,
            cx,
        )
    }
}

/// Budget retained CPU storage plus one decoded texture per image, pinning visible images.
/// A zero budget retains only visible entries, including while their loads are pending.
pub fn retain_recent(id: impl Into<ElementId>, bytes: usize) -> impl ImageCacheProvider {
    RecentProvider {
        id: id.into(),
        bytes,
        prepared: false,
    }
}

struct RecentProvider {
    id: ElementId,
    bytes: usize,
    prepared: bool,
}

impl ImageCacheProvider for RecentProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> AnyImageCache {
        let entity = window.with_global_id(self.id.clone(), |id, window| {
            window.with_element_state::<Entity<RecentImageCache>, _>(id, |cached, _| {
                let entity = cached.unwrap_or_else(|| RecentImageCache::new(self.bytes, cx));
                (entity.clone(), entity)
            })
        });
        if !self.prepared {
            entity.update(cx, |cache, cx| {
                cache.budget = self.bytes;
                cache.begin_frame(window, cx);
            });
            self.prepared = true;
        }
        entity.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn shared_animation_keeps_independent_cursors_and_releases_closed_player(
        cx: &mut crate::TestAppContext,
    ) {
        use std::{cell::RefCell, rc::Rc};
        struct View {
            item: Rc<ImageCacheItem>,
            first: Option<Arc<()>>,
            second: Arc<()>,
            indices: [usize; 2],
            images: Rc<RefCell<[Option<Arc<RenderImage>>; 2]>>,
        }
        impl crate::Render for View {
            fn render(&mut self, _: &mut Window, _: &mut crate::Context<Self>) -> impl IntoElement {
                let item = self.item.clone();
                let owners = [self.first.clone(), Some(self.second.clone())];
                let indices = self.indices;
                let images = self.images.clone();
                crate::canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        for (slot, owner) in owners.into_iter().enumerate() {
                            images.borrow_mut()[slot] = owner
                                .and_then(|owner| {
                                    item.use_frame_at_size(
                                        Some(crate::size(640.into(), 360.into())),
                                        ImageFrameRequest {
                                            index: indices[slot],
                                            playback: Some(owner),
                                        },
                                        window,
                                        cx,
                                    )
                                })
                                .and_then(Result::ok)
                                .filter(|image| image.frame_count() > 0);
                        }
                    },
                )
            }
        }
        let bytes = crate::elements::img::sized::tests::animation_bytes(32, 16, 240);
        let resource = Resource::Image(Arc::new(crate::Image::from_bytes(
            crate::ImageFormat::Gif,
            bytes,
        )));
        let item = Rc::new(cx.update(|cx| ImageCacheItem::new_sized(&resource, cx)));
        let images = Rc::new(RefCell::new([None, None]));
        let (view, cx) = cx.add_window_view(|_, _| View {
            item: item.clone(),
            first: Some(Arc::new(())),
            second: Arc::new(()),
            indices: [0, 0],
            images: images.clone(),
        });
        let draw =
            |cx: &mut crate::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let first = images.borrow()[0].clone().unwrap();
        let second = images.borrow()[1].clone().unwrap();
        assert_eq!(first.retained_bytes(), 32 * 16 * 4 * 240);
        assert_eq!(second.retained_bytes(), 32 * 16 * 4 * 240);
        let first_frame = first.frame_bytes(4).unwrap().unwrap();
        assert_eq!(
            &first.frame_bytes(4).unwrap().unwrap()[..4],
            &[196, 84, 28, 255]
        );
        assert_eq!(
            &second.frame_bytes(200).unwrap().unwrap()[..4],
            &[72, 104, 120, 255]
        );
        assert_eq!(
            first.frame_bytes(4).unwrap().unwrap().as_ptr(),
            first_frame.as_ptr()
        );
        drop(first_frame);
        let retired = Arc::downgrade(&first);
        drop(first);
        view.update(cx, |view, cx| {
            view.first = None;
            view.indices = [4, 200];
            cx.notify();
        });
        draw(cx);
        assert!(
            retired.upgrade().is_none(),
            "closing one consumer retained its frames"
        );
        assert_eq!(images.borrow()[1].as_ref().unwrap().id, second.id);
        assert_eq!(
            &second.frame_bytes(200).unwrap().unwrap()[..4],
            &[72, 104, 120, 255]
        );
    }

    #[gpui::test]
    fn shared_budget_preserves_other_windows_and_cached_paints(cx: &mut crate::TestAppContext) {
        let first = cx
            .add_empty_window()
            .update(|window, _| window.window_handle());
        let second = cx
            .add_empty_window()
            .update(|window, _| window.window_handle());
        let cache = cx.update(|cx| RecentImageCache::shared(cx));
        let mut ids = Vec::new();
        cache.update(cx, |cache, cx| {
            cache.budget = 4 * 1024 * 1024;
            for key in 1..=3 {
                let image = Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::from_pixel(1024, 512, image::Rgba([key as u8, 0, 0, 255])),
                )]));
                ids.push(image.id);
                cache.entries.insert(
                    key,
                    Entry {
                        image: ImageCacheItem::from_load(ImageCacheLoad::Full(CachedLoad::new(
                            async move { Ok(image) },
                            cx,
                        ))),
                        used: key,
                    },
                );
            }
            cache
                .windows
                .as_mut()
                .unwrap()
                .entry(first.window_id())
                .or_default()
                .retained
                .insert(1);
        });
        cx.run_until_parked();
        second
            .update(cx, |_, window, cx| {
                cache.update(cx, |cache, cx| {
                    // No load call: a reused paint subtree alone must keep the second image alive.
                    cache.finish_window_frame(&HashSet::from([ids[1]]), window, cx);
                    let mut keys: Vec<_> = cache.entries.keys().copied().collect();
                    keys.sort();
                    assert_eq!(keys, [1, 2]);
                    assert_eq!(
                        &cache.entries[&2]
                            .image
                            .get()
                            .unwrap()
                            .unwrap()
                            .as_bytes(0)
                            .unwrap()[..4],
                        &[2, 0, 0, 255]
                    );
                })
            })
            .unwrap();
        first
            .update(cx, |_, window, cx| {
                cache.update(cx, |cache, cx| {
                    cache.entries.insert(
                        4,
                        Entry {
                            image: ImageCacheItem::from_load(ImageCacheLoad::Full(
                                CachedLoad::new(futures::future::pending(), cx),
                            )),
                            used: 4,
                        },
                    );
                    cache.finish_window_frame(&HashSet::new(), window, cx);
                    assert_eq!(cache.entries.keys().copied().collect::<Vec<_>>(), [2]);
                })
            })
            .unwrap();
    }

    #[gpui::test]
    fn budget_evicts_cold_images_without_reloading_visible_images(cx: &mut crate::TestAppContext) {
        let cx = cx.add_empty_window();
        let cache = cx.update(|_, cx| RecentImageCache::new(8 * 1024 * 1024, cx));
        cache.update(cx, |cache, cx| {
            for key in 1..=3 {
                let image = Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
                    image::RgbaImage::from_pixel(1024, 512, image::Rgba([key as u8, 0, 0, 255])),
                )]));
                cache.entries.insert(
                    key,
                    Entry {
                        image: ImageCacheItem::from_load(ImageCacheLoad::Full(CachedLoad::new(
                            async move { Ok(image) },
                            cx,
                        ))),
                        used: key,
                    },
                );
            }
            cache.visible.insert(1);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            cache.update(cx, |cache, cx| {
                cache.begin_frame(window, cx);
                let mut keys = cache.entries.keys().copied().collect::<Vec<_>>();
                keys.sort_unstable();
                assert_eq!(keys, [1, 3]);
                let image = cache.entries[&1].image.get().unwrap().unwrap();
                assert_eq!(&image.as_bytes(0).unwrap()[..4], &[1, 0, 0, 255]);
                eprintln!(
                    "IMAGE_CACHE_BYTES before=6291456 after={}",
                    cache
                        .entries
                        .values()
                        .map(|entry| entry.image.get().unwrap().unwrap().retained_bytes())
                        .sum::<usize>()
                );
                cache.budget = 0;
                cache.visible.insert(1);
                cache.entries.insert(
                    4,
                    Entry {
                        image: ImageCacheItem::from_load(ImageCacheLoad::Full(CachedLoad::new(
                            futures::future::pending(),
                            cx,
                        ))),
                        used: 4,
                    },
                );
                cache.begin_frame(window, cx);
                assert_eq!(
                    cache.entries.keys().copied().collect::<Vec<_>>(),
                    [1],
                    "a preview cache must discard hidden pending loads as well as pixels"
                );
            })
        });
    }
}
