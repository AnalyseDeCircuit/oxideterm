use std::borrow::Cow;

use crate::bundled_fonts::BundledTerminalFace;
use anyhow::Result;
use gpui::{AssetSource, SharedString};
use oxideterm_gpui_shell::PortableAssets;

// SVG has its own font database; only regular faces are needed for its bundled fallbacks.
const SVG_FONTS: &[(&str, BundledTerminalFace)] = &[
    (
        "fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Subset-Regular.ttf",
        BundledTerminalFace::JetBrainsRegular,
    ),
    (
        "fonts/MapleMono/MapleMono-NF-CN-Subset-Regular.ttf",
        BundledTerminalFace::MapleRegular,
    ),
];

pub(crate) use oxideterm_gpui_ui::icons::LucideIcon;

pub(crate) struct NativeAssets;

impl AssetSource for NativeAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, face)) = SVG_FONTS.iter().find(|(name, _)| *name == path) {
            return face.load().map(|bytes| Some(Cow::Owned(bytes)));
        }
        let distro: Option<&'static [u8]> = match path {
            "distro-icons/ubuntu.svg" => {
                Some(include_bytes!("../resources/distro-icons/ubuntu.svg"))
            }
            "distro-icons/archlinux.svg" => {
                Some(include_bytes!("../resources/distro-icons/archlinux.svg"))
            }
            "distro-icons/debian.svg" => {
                Some(include_bytes!("../resources/distro-icons/debian.svg"))
            }
            "distro-icons/gentoo.svg" => {
                Some(include_bytes!("../resources/distro-icons/gentoo.svg"))
            }
            "distro-icons/nixos.svg" => Some(include_bytes!("../resources/distro-icons/nixos.svg")),
            "distro-icons/rocky.svg" => Some(include_bytes!("../resources/distro-icons/rocky.svg")),
            "distro-icons/linuxmint.svg" => {
                Some(include_bytes!("../resources/distro-icons/linuxmint.svg"))
            }
            _ => None,
        };
        if let Some(bytes) = distro {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        let svg = match path {
            "window-controls/minimize.svg" => WINDOW_CONTROL_MINIMIZE,
            "window-controls/maximize.svg" => WINDOW_CONTROL_MAXIMIZE,
            "window-controls/restore.svg" => WINDOW_CONTROL_RESTORE,
            "window-controls/close.svg" => WINDOW_CONTROL_CLOSE,
            _ => return PortableAssets.load(path),
        };
        Ok(Some(Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path == "fonts" {
            return Ok(SVG_FONTS
                .iter()
                .map(|(path, _)| SharedString::from(*path))
                .collect());
        }
        PortableAssets.list(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_gpui_connections::icons::DISTRO_ICONS;

    #[test]
    fn distribution_assets_render_in_color_through_the_native_image_pipeline() {
        let assets = NativeAssets;
        let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(NativeAssets));
        for icon in DISTRO_ICONS {
            let bytes = assets
                .load(icon.path)
                .unwrap()
                .expect("bundled distribution icon");
            let svg = renderer.parse_svg(&bytes).unwrap();
            let image = renderer
                .render_parsed(
                    &svg,
                    gpui::SvgSize::Size(gpui::Size::new(
                        gpui::DevicePixels(24),
                        gpui::DevicePixels(24),
                    )),
                )
                .unwrap();
            assert!(
                image.as_bytes(0).unwrap().chunks_exact(4).any(|pixel| {
                    let brightest = pixel[0].max(pixel[1]).max(pixel[2]);
                    let darkest = pixel[0].min(pixel[1]).min(pixel[2]);
                    pixel[3] > 128 && brightest - darkest > 20
                }),
                "{} lost its brand colors",
                icon.id
            );
        }
    }

    #[test]
    fn svg_font_assets_are_loadable_and_keep_latin_and_cjk_coverage() {
        let assets = NativeAssets;
        let paths = assets.list("fonts").unwrap();
        let expected = [
            (
                "fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Subset-Regular.ttf",
                oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY,
                'A',
            ),
            (
                "fonts/MapleMono/MapleMono-NF-CN-Subset-Regular.ttf",
                oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY,
                '中',
            ),
        ];
        assert_eq!(
            paths
                .iter()
                .map(|path| path.as_ref())
                .collect::<Vec<&str>>(),
            expected
                .iter()
                .map(|(path, _, _)| *path)
                .collect::<Vec<_>>()
        );
        for (path, family, glyph) in expected {
            let data = assets.load(path).unwrap().unwrap();
            let face = ttf_parser::Face::parse(&data, 0).unwrap();
            assert!(
                face.names()
                    .into_iter()
                    .any(|name| name.name_id == ttf_parser::name_id::FAMILY
                        && name.to_string().as_deref() == Some(family))
            );
            assert!(
                face.glyph_index(glyph).is_some(),
                "{family} must cover {glyph}"
            );
        }
    }
}

const WINDOW_CONTROL_MINIMIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="square"><path d="M2.5 6.5h7"/></svg>"#;
const WINDOW_CONTROL_MAXIMIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1"><rect x="2.5" y="2.5" width="7" height="7"/></svg>"#;
const WINDOW_CONTROL_RESTORE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1" stroke-linejoin="miter"><path d="M4.5 2.5h5v5"/><rect x="2.5" y="4.5" width="5" height="5"/></svg>"#;
const WINDOW_CONTROL_CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="square"><path d="m2.5 2.5 7 7M9.5 2.5l-7 7"/></svg>"#;
