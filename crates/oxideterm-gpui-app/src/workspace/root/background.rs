use super::super::*;

struct BundledWorkspaceBackground {
    file_name: &'static str,
    bytes: &'static [u8],
}

// Bundled gallery assets are installed on startup and protected from user deletion.
const BUNDLED_WORKSPACE_BACKGROUNDS: &[BundledWorkspaceBackground] = &[
    BundledWorkspaceBackground {
        file_name: "oxide-ambient-v1.png",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-ambient-v1.png"
        )),
    },
    BundledWorkspaceBackground {
        file_name: "oxide-nocturne-v1.webp",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-nocturne-v1.webp"
        )),
    },
    BundledWorkspaceBackground {
        file_name: "oxide-verdant-v1.webp",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-verdant-v1.webp"
        )),
    },
];

pub(in crate::workspace) fn ensure_bundled_workspace_backgrounds(
    settings_path: &Path,
) -> Result<()> {
    for background in BUNDLED_WORKSPACE_BACKGROUNDS {
        ensure_bundled_background_image(settings_path, background.file_name, background.bytes)?;
    }
    Ok(())
}

pub(in crate::workspace) fn is_bundled_workspace_background(
    settings_path: &Path,
    image_path: &Path,
) -> bool {
    let background_directory = background_images_directory(settings_path);
    BUNDLED_WORKSPACE_BACKGROUNDS
        .iter()
        .any(|background| image_path == background_directory.join(background.file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_every_bundled_background_as_protected() {
        let settings_path = Path::new("/profile/settings.json");
        let background_directory = background_images_directory(settings_path);

        for background in BUNDLED_WORKSPACE_BACKGROUNDS {
            assert!(is_bundled_workspace_background(
                settings_path,
                &background_directory.join(background.file_name),
            ));
        }
        assert!(!is_bundled_workspace_background(
            settings_path,
            &background_directory.join("user-background.webp"),
        ));
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_workspace_window_background(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let background = self.window_background_preferences()?;
        Some(self.render_workspace_background_layer(background, window, cx))
    }

    pub(in crate::workspace) fn wrap_content_background(
        &mut self,
        content: AnyElement,
        background_key: Option<&str>,
        window: &mut Window,
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
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(self.render_workspace_background_layer(background, window, cx))
            .child(div().relative().size_full().child(content))
            .into_any_element()
    }

    fn render_workspace_background_layer(
        &mut self,
        background: TerminalBackgroundPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blurred_image = self
            .background_image_cache
            .render_blurred_image(&background);
        self.drop_workspace_background_retired_images(Some(window), cx);
        if self.background_image_cache.has_pending() {
            self.schedule_background_cache_poll(cx);
        }
        workspace_background_image_layer(background, blurred_image)
    }

    pub(in crate::workspace) fn schedule_background_cache_poll(&mut self, cx: &mut Context<Self>) {
        if self.background_cache_poll_task.is_some() {
            return;
        }
        // The render-cache owner retains the one-shot so dropping the workspace
        // also cancels polling for cache entries that can no longer be painted.
        self.background_cache_poll_task = Some(cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.background_cache_poll_task = None;
                if this.background_image_cache.drain_completed() {
                    this.drop_workspace_background_retired_images(None, cx);
                    cx.notify();
                }
                if this.background_image_cache.has_pending() {
                    this.schedule_background_cache_poll(cx);
                }
            });
        }));
    }

    pub(in crate::workspace) fn drop_workspace_background_retired_images(
        &mut self,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        for image in self.background_image_cache.take_retired_images() {
            // RenderImage entries painted by gpui::img also stay in the atlas
            // until the app explicitly drops their image id.
            if let Some(window) = window.as_mut() {
                cx.drop_image(image, Some(*window));
            } else {
                cx.drop_image(image, None);
            }
        }
    }
}

pub(in crate::workspace) fn workspace_background_image_layer(
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

pub(in crate::workspace) fn workspace_background_object_fit(
    fit: TerminalBackgroundFit,
) -> ObjectFit {
    match fit {
        TerminalBackgroundFit::Cover => ObjectFit::Cover,
        TerminalBackgroundFit::Contain => ObjectFit::Contain,
        TerminalBackgroundFit::Fill => ObjectFit::Fill,
        TerminalBackgroundFit::Tile => ObjectFit::None,
    }
}

pub(in crate::workspace) fn default_connections_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("connections.json")
}

pub(in crate::workspace) fn default_saved_forwards_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forwards.json")
}

pub(in crate::workspace) fn default_session_tree_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("session_tree.json")
}

pub(in crate::workspace) fn default_ai_conversations_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("chat_history.redb")
}
