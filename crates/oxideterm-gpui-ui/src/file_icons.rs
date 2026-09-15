use gpui::{
    AnyElement, IntoColor, IntoElement, ParentElement, Styled, StyledImage, div, img, px, rgb, svg,
};
use oxideterm_theme::ThemeTokens;

#[rustfmt::skip]
mod generated;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIcon {
    path: &'static str,
    light: &'static str,
    symlink: bool,
}

impl FileIcon {
    const fn new(path: &'static str, light: &'static str) -> Self {
        Self {
            path,
            light,
            symlink: false,
        }
    }

    pub fn with_symlink(mut self, symlink: bool) -> Self {
        self.symlink = symlink;
        self
    }

    pub fn render(self, size: f32, tokens: &ThemeTokens) -> AnyElement {
        let background: gpui::Hsla = rgb(tokens.ui.bg).into_color();
        let path = if background.lightness > 0.5 {
            self.light
        } else {
            self.path
        };
        let mut icon = div()
            .size(px(size))
            .flex_none()
            .relative()
            .child(img(path).size_full().object_fit(gpui::ObjectFit::Contain));
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
    if let Ok(index) = generated::NAMES.binary_search_by_key(&name.as_str(), |(name, _)| *name) {
        return generated::NAMES[index].1;
    }
    // Check compound suffixes first, so declarations and test files retain their icon.
    for (offset, _) in name.match_indices('.') {
        let suffix = &name[offset + 1..];
        if let Ok(index) =
            generated::EXTENSIONS.binary_search_by_key(&suffix, |(extension, _)| *extension)
        {
            return generated::EXTENSIONS[index].1;
        }
    }
    generated::DEFAULT
}

pub fn folder_icon(name: &str, expanded: bool) -> FileIcon {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    if let Ok(index) = generated::FOLDERS.binary_search_by_key(&name.as_str(), |(name, _, _)| *name)
    {
        let (_, closed, opened) = generated::FOLDERS[index];
        return if expanded { opened } else { closed };
    }
    if expanded {
        generated::FOLDER_OPEN
    } else {
        generated::FOLDER
    }
}

pub fn load_asset(path: &str) -> Option<&'static [u8]> {
    generated::asset(path)
}
