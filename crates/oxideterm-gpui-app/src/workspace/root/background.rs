impl WorkspaceApp {
    fn wrap_content_background(
        &mut self,
        content: AnyElement,
        background_key: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(background_key) = background_key else {
            return content;
        };
        if matches!(background_key, "terminal" | "local_terminal") {
            return content;
        }
        let Some(background) = self.terminal_background_preferences(background_key) else {
            return content;
        };
        let blurred_image = self
            .background_image_cache
            .render_blurred_image(&background);
        if self.background_image_cache.has_pending() {
            self.schedule_background_cache_poll(cx);
        }

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(workspace_background_image_layer(background, blurred_image))
            .child(div().relative().size_full().child(content))
            .into_any_element()
    }

    fn schedule_background_cache_poll(&mut self, cx: &mut Context<Self>) {
        if self.settings_page.background_cache_poll_scheduled {
            return;
        }
        self.settings_page
            .set_background_cache_poll_scheduled(true);
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.settings_page
                    .set_background_cache_poll_scheduled(false);
                if this.background_image_cache.drain_completed() {
                    cx.notify();
                }
                if this.background_image_cache.has_pending() {
                    this.schedule_background_cache_poll(cx);
                }
            });
        })
        .detach();
    }
}

fn workspace_background_image_layer(
    background: TerminalBackgroundPreferences,
    blurred_image: Option<Arc<RenderImage>>,
) -> AnyElement {
    let image = if let Some(blurred_image) = blurred_image {
        gpui::img(blurred_image)
            .size_full()
            .object_fit(workspace_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .into_any_element()
    } else {
        gpui::img(background.path)
            .size_full()
            .object_fit(workspace_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .with_fallback(|| div().size_full().into_any_element())
            .into_any_element()
    };

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .overflow_hidden()
        .child(image)
        .into_any_element()
}

fn workspace_background_object_fit(fit: TerminalBackgroundFit) -> ObjectFit {
    match fit {
        TerminalBackgroundFit::Cover => ObjectFit::Cover,
        TerminalBackgroundFit::Contain => ObjectFit::Contain,
        TerminalBackgroundFit::Fill => ObjectFit::Fill,
        TerminalBackgroundFit::Tile => ObjectFit::None,
    }
}

fn default_connections_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("connections.json")
}

fn default_saved_forwards_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forwards.json")
}

fn default_session_tree_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("session_tree.json")
}

fn default_ai_conversations_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("chat_history.redb")
}
