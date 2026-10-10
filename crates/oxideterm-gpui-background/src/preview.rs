use crate::{
    BackgroundImageRenderCache, BackgroundImageTargetSize, BackgroundPreferences,
    background_display_target, background_layer,
};
use gpui::{Context, Entity, Render, Subscription, Task, Window, canvas, div, prelude::*};
use std::time::Duration;

/// Owns the same cache and playback path as a workspace background inside a preview.
pub struct BackgroundPreview {
    preview_hour: Option<f32>,
    preferences: Option<BackgroundPreferences>,
    cache: BackgroundImageRenderCache,
    target: Option<BackgroundImageTargetSize>,
    completion: Option<Task<()>>,
    _release: Subscription,
}

impl BackgroundPreview {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let release = cx.on_release(|this, cx| {
            for image in this.cache.take_all_images() {
                cx.drop_image(image, None);
            }
        });
        Self {
            preview_hour: None,
            preferences: None,
            cache: BackgroundImageRenderCache::default(),
            target: None,
            completion: None,
            _release: release,
        }
    }

    pub fn set_preferences(
        &mut self,
        preferences: Option<BackgroundPreferences>,
        cx: &mut Context<Self>,
    ) {
        if preferences.is_none() && self.preferences.is_some() {
            self.completion = None;
            for image in self.cache.take_all_images() {
                cx.drop_image(image, None);
            }
            self.cache = BackgroundImageRenderCache::default();
        }
        self.preferences = preferences;
        if !self
            .preferences
            .as_ref()
            .is_some_and(|preferences| preferences.scene.day_cycle)
        {
            self.preview_hour = None;
        }
    }

    pub fn preview_hour(&self) -> Option<f32> {
        self.preview_hour
    }

    pub fn set_preview_hour(&mut self, hour: Option<f32>, cx: &mut Context<Self>) {
        self.preview_hour = hour;
        cx.notify();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        if self.completion.is_some() || !self.cache.has_pending() {
            return;
        }
        self.completion = Some(cx.spawn(async move |preview, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let _ = preview.update(cx, |preview, cx| {
                preview.completion = None;
                if preview.cache.drain_completed() {
                    for image in preview.cache.take_retired_images() {
                        cx.drop_image(image, None);
                    }
                    cx.notify();
                }
                preview.poll(cx);
            });
        }));
    }
}

impl Render for BackgroundPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(mut preferences) = self.preferences.clone() else {
            return div().into_any_element();
        };
        preferences.scene.preview_hour = self.preview_hour;
        let target = self.target.unwrap_or_else(|| {
            background_display_target(
                gpui::size(gpui::px(640.0), gpui::px(360.0)),
                window.scale_factor(),
            )
        });
        let image = if crate::is_streaming_source(&preferences.path) {
            None
        } else {
            self.cache.render_background_image(&preferences, target)
        };
        self.poll(cx);
        let preview: Entity<Self> = cx.entity();
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let target = background_display_target(bounds.size, window.scale_factor());
                        preview.update(cx, |preview, cx| {
                            if preview.target != Some(target) {
                                preview.target = Some(target);
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(background_layer(preferences, image, window, cx))
            .into_any_element()
    }
}
