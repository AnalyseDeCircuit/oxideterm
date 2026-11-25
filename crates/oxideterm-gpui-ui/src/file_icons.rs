use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use gpui::{AnyElement, IntoElement, ParentElement, Styled, StyledImage, div, img, px, rgb, svg};
use oxideterm_theme::ThemeTokens;

#[rustfmt::skip]
mod generated;

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileIconSource {
    Asset(&'static str),
    Image(Arc<gpui::Image>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileIcon {
    path: FileIconSource,
    light: FileIconSource,
    symlink: bool,
}

/// Verified custom file icon contributed by an enabled language plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginFileIconRegistration {
    pub plugin_id: String,
    pub extensions: Vec<String>,
    pub file_names: Vec<String>,
    pub dark_svg: Vec<u8>,
    pub light_svg: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq)]
struct RegisteredPluginFileIcon {
    plugin_id: String,
    file_names: Vec<String>,
    // Pre-prefixed with '.' so file lookups avoid per-row allocations.
    dot_extensions: Vec<(String, usize)>,
    dark: Arc<gpui::Image>,
    light: Arc<gpui::Image>,
}

impl RegisteredPluginFileIcon {
    fn from_registration(registration: PluginFileIconRegistration) -> Self {
        let dark = Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            registration.dark_svg,
        ));
        let light = registration
            .light_svg
            .map(|bytes| Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Svg, bytes)))
            .unwrap_or_else(|| dark.clone());
        let file_names = registration
            .file_names
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        let dot_extensions = registration
            .extensions
            .into_iter()
            .map(|ext| {
                let len = ext.len();
                (format!(".{}", ext.to_ascii_lowercase()), len)
            })
            .collect();
        Self {
            plugin_id: registration.plugin_id,
            file_names,
            dot_extensions,
            dark,
            light,
        }
    }

    /// Matches the same scoring rules as `NativePluginLanguageDefinition::match_path`:
    /// exact file names outrank extensions; compound extensions outrank shorter ones.
    fn match_lowercase_name(&self, name: &str) -> Option<usize> {
        if self.file_names.iter().any(|file| file == name) {
            return Some(usize::MAX);
        }
        self.dot_extensions
            .iter()
            .filter(|(suffix, _)| name.ends_with(suffix.as_str()))
            .map(|(_, len)| *len)
            .max()
    }
}

static PLUGIN_FILE_ICONS: RwLock<Vec<RegisteredPluginFileIcon>> = RwLock::new(Vec::new());
static PLUGIN_FILE_ICON_REVISION: AtomicU64 = AtomicU64::new(0);

/// Replaces the active set of enabled language plugin file icons.
/// Returns `true` when the registered icon set changed.
pub fn set_plugin_file_icons(registrations: Vec<PluginFileIconRegistration>) -> bool {
    let next = registrations
        .into_iter()
        .map(RegisteredPluginFileIcon::from_registration)
        .collect::<Vec<_>>();
    let mut guard = PLUGIN_FILE_ICONS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *guard == next {
        return false;
    }
    *guard = next;
    PLUGIN_FILE_ICON_REVISION.fetch_add(1, Ordering::Relaxed);
    true
}

/// Monotonic revision bumped whenever enabled language plugin file icons change.
pub fn plugin_icon_revision() -> u64 {
    PLUGIN_FILE_ICON_REVISION.load(Ordering::Relaxed)
}

fn lookup_plugin_file_icon(lowercase_name: &str) -> Option<FileIcon> {
    let guard = PLUGIN_FILE_ICONS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_empty() {
        return None;
    }
    guard
        .iter()
        .filter_map(|entry| {
            entry
                .match_lowercase_name(lowercase_name)
                .map(|score| (score, entry))
        })
        .max_by(|(left_score, left), (right_score, right)| {
            left_score
                .cmp(right_score)
                .then_with(|| right.plugin_id.cmp(&left.plugin_id))
        })
        .map(|(_, entry)| FileIcon {
            path: FileIconSource::Image(entry.dark.clone()),
            light: FileIconSource::Image(entry.light.clone()),
            symlink: false,
        })
}

impl FileIcon {
    const fn new(path: &'static str, light: &'static str) -> Self {
        Self {
            path: FileIconSource::Asset(path),
            light: FileIconSource::Asset(light),
            symlink: false,
        }
    }

    pub fn with_symlink(mut self, symlink: bool) -> Self {
        self.symlink = symlink;
        self
    }

    pub fn render(&self, size: f32, tokens: &ThemeTokens) -> AnyElement {
        let background: gpui::Hsla = rgb(tokens.ui.bg).into();
        let source = if background.l > 0.5 {
            &self.light
        } else {
            &self.path
        };
        let image = match source {
            FileIconSource::Asset(path) => img(*path),
            FileIconSource::Image(image) => img(image.clone()),
        };
        let mut icon = div()
            .size(px(size))
            .flex_none()
            .relative()
            .child(image.size_full().object_fit(gpui::ObjectFit::Contain));
        if self.symlink {
            icon = icon.child(
                svg()
                    .path("lucide/link-2.svg")
                    .absolute()
                    .bottom_0()
                    .right_0()
                    .size(px(size * 0.6))
                    .text_color(rgb(tokens.ui.accent)),
            );
        }
        icon.into_any_element()
    }
}

pub fn file_icon(filename: &str) -> FileIcon {
    let name = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .to_lowercase();
    if let Some(plugin_icon) = lookup_plugin_file_icon(&name) {
        return plugin_icon;
    }
    if let Ok(index) = generated::NAMES.binary_search_by_key(&name.as_str(), |(name, _)| *name) {
        return generated::NAMES[index].1.clone();
    }
    // Check compound suffixes first, so declarations and test files retain their icon.
    for (offset, _) in name.match_indices('.') {
        let suffix = &name[offset + 1..];
        if let Ok(index) =
            generated::EXTENSIONS.binary_search_by_key(&suffix, |(extension, _)| *extension)
        {
            return generated::EXTENSIONS[index].1.clone();
        }
    }
    generated::DEFAULT.clone()
}

pub fn folder_icon(name: &str, expanded: bool) -> FileIcon {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    if let Ok(index) = generated::FOLDERS.binary_search_by_key(&name.as_str(), |(name, _, _)| *name)
    {
        let (_, closed, opened) = &generated::FOLDERS[index];
        return if expanded {
            opened.clone()
        } else {
            closed.clone()
        };
    }
    if expanded {
        generated::FOLDER_OPEN.clone()
    } else {
        generated::FOLDER.clone()
    }
}

pub fn load_asset(path: &str) -> Option<&'static [u8]> {
    generated::asset(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ResetPluginIcons;

    impl Drop for ResetPluginIcons {
        fn drop(&mut self) {
            set_plugin_file_icons(Vec::new());
        }
    }

    #[test]
    fn plugin_file_icons_override_builtins_respect_precedence_and_restore_on_clear() {
        let _reset = ResetPluginIcons;
        set_plugin_file_icons(Vec::new());

        let builtin_exs = file_icon("demo.exs");
        assert_eq!(
            builtin_exs.path,
            FileIconSource::Asset("material-icons/elixir.svg")
        );

        let exs_dark = b"<svg id=\"exs-dark\"/>".to_vec();
        let exs_light = b"<svg id=\"exs-light\"/>".to_vec();
        let compound_dark = b"<svg id=\"compound-dark\"/>".to_vec();
        let exact_dark = b"<svg id=\"exact-dark\"/>".to_vec();
        let tie_dark = b"<svg id=\"tie-dark\"/>".to_vec();

        let revision_before = plugin_icon_revision();
        assert!(set_plugin_file_icons(vec![
            PluginFileIconRegistration {
                plugin_id: "com.example.b-exs".into(),
                extensions: vec!["exs".into()],
                file_names: Vec::new(),
                dark_svg: tie_dark,
                light_svg: None,
            },
            PluginFileIconRegistration {
                plugin_id: "com.example.a-exs".into(),
                extensions: vec!["exs".into()],
                file_names: Vec::new(),
                dark_svg: exs_dark.clone(),
                light_svg: Some(exs_light.clone()),
            },
            PluginFileIconRegistration {
                plugin_id: "com.example.compound".into(),
                extensions: vec!["custom.exs".into()],
                file_names: Vec::new(),
                dark_svg: compound_dark.clone(),
                light_svg: None,
            },
            PluginFileIconRegistration {
                plugin_id: "com.example.exact".into(),
                extensions: Vec::new(),
                file_names: vec!["Special.exs".into()],
                dark_svg: exact_dark.clone(),
                light_svg: None,
            },
        ]));
        let revision_after = plugin_icon_revision();
        assert!(revision_after > revision_before);
        // Re-applying identical registrations must not churn cached image revisions.
        assert!(!set_plugin_file_icons(vec![
            PluginFileIconRegistration {
                plugin_id: "com.example.b-exs".into(),
                extensions: vec!["exs".into()],
                file_names: Vec::new(),
                dark_svg: b"<svg id=\"tie-dark\"/>".to_vec(),
                light_svg: None,
            },
            PluginFileIconRegistration {
                plugin_id: "com.example.a-exs".into(),
                extensions: vec!["exs".into()],
                file_names: Vec::new(),
                dark_svg: exs_dark.clone(),
                light_svg: Some(exs_light.clone()),
            },
            PluginFileIconRegistration {
                plugin_id: "com.example.compound".into(),
                extensions: vec!["custom.exs".into()],
                file_names: Vec::new(),
                dark_svg: compound_dark.clone(),
                light_svg: None,
            },
            PluginFileIconRegistration {
                plugin_id: "com.example.exact".into(),
                extensions: Vec::new(),
                file_names: vec!["Special.exs".into()],
                dark_svg: exact_dark.clone(),
                light_svg: None,
            },
        ]));
        assert_eq!(plugin_icon_revision(), revision_after);

        // Extension match overrides built-in Elixir icon and breaks ties by plugin_id ascending.
        let matched = file_icon("dir/script.EXS");
        assert_eq!(
            matched.path,
            FileIconSource::Image(Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                exs_dark,
            )))
        );
        assert_eq!(
            matched.light,
            FileIconSource::Image(Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                exs_light,
            )))
        );

        // Compound extension outranks shorter extension; omitted light icon falls back to dark.
        let compound = file_icon("app.custom.exs");
        let expected_compound = FileIconSource::Image(Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            compound_dark,
        )));
        assert_eq!(compound.path, expected_compound);
        assert_eq!(compound.light, expected_compound);

        // Exact file name outranks extension match.
        let exact = file_icon("/path/to/special.exs");
        assert_eq!(
            exact.path,
            FileIconSource::Image(Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                exact_dark,
            )))
        );

        // Unclaimed files keep built-in icons.
        assert_eq!(
            file_icon("main.rs").path,
            FileIconSource::Asset("material-icons/rust.svg")
        );

        // Clearing plugin icons restores built-in Elixir icon.
        assert!(set_plugin_file_icons(Vec::new()));
        assert_eq!(file_icon("demo.exs"), builtin_exs);
    }
}
