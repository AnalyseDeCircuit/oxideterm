// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Bundled terminal font registration for the native GPUI app.
//!
//! MapleMono faces are embedded as independent Zstd frames and decompressed
//! only when registered. GPUI/font-kit still receives the original SFNT bytes.
//! Registration stays lazy: startup and terminal-open paths load only the
//! selected font's critical faces, matching Tauri's fontLoader strategy.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context as _, Result};
use gpui::TextSystem;
use oxideterm_settings::{FontFamily, PersistedSettings};

const JETBRAINS_REGULAR: &[u8] =
    include_bytes!("../resources/fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Subset-Regular.ttf");
const JETBRAINS_BOLD: &[u8] =
    include_bytes!("../resources/fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Subset-Bold.ttf");
const JETBRAINS_ITALIC: &[u8] =
    include_bytes!("../resources/fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Subset-Italic.ttf");
const JETBRAINS_BOLD_ITALIC: &[u8] = include_bytes!(
    "../resources/fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Subset-BoldItalic.ttf"
);
const MESLO_REGULAR: &[u8] =
    include_bytes!("../resources/fonts/Meslo/MesloLGMNerdFontMono-Subset-Regular.ttf");
const MESLO_BOLD: &[u8] =
    include_bytes!("../resources/fonts/Meslo/MesloLGMNerdFontMono-Subset-Bold.ttf");
const MESLO_ITALIC: &[u8] =
    include_bytes!("../resources/fonts/Meslo/MesloLGMNerdFontMono-Subset-Italic.ttf");
const MESLO_BOLD_ITALIC: &[u8] =
    include_bytes!("../resources/fonts/Meslo/MesloLGMNerdFontMono-Subset-BoldItalic.ttf");
const MAPLE_REGULAR: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/MapleMono-NF-CN-Subset-Regular.ttf.zst"
));
const MAPLE_BOLD: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/MapleMono-NF-CN-Subset-Bold.ttf.zst"
));
const MAPLE_ITALIC: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/MapleMono-NF-CN-Subset-Italic.ttf.zst"
));
const MAPLE_BOLD_ITALIC: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/MapleMono-NF-CN-Subset-BoldItalic.ttf.zst"
));

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BundledTerminalFace {
    JetBrainsRegular,
    JetBrainsBold,
    JetBrainsItalic,
    JetBrainsBoldItalic,
    MesloRegular,
    MesloBold,
    MesloItalic,
    MesloBoldItalic,
    MapleRegular,
    MapleBold,
    MapleItalic,
    MapleBoldItalic,
}

impl BundledTerminalFace {
    fn embedded_bytes(self) -> &'static [u8] {
        match self {
            Self::JetBrainsRegular => JETBRAINS_REGULAR,
            Self::JetBrainsBold => JETBRAINS_BOLD,
            Self::JetBrainsItalic => JETBRAINS_ITALIC,
            Self::JetBrainsBoldItalic => JETBRAINS_BOLD_ITALIC,
            Self::MesloRegular => MESLO_REGULAR,
            Self::MesloBold => MESLO_BOLD,
            Self::MesloItalic => MESLO_ITALIC,
            Self::MesloBoldItalic => MESLO_BOLD_ITALIC,
            Self::MapleRegular => MAPLE_REGULAR,
            Self::MapleBold => MAPLE_BOLD,
            Self::MapleItalic => MAPLE_ITALIC,
            Self::MapleBoldItalic => MAPLE_BOLD_ITALIC,
        }
    }

    pub(crate) fn load(self) -> Result<Vec<u8>> {
        let bytes = self.embedded_bytes();
        if matches!(
            self,
            Self::MapleRegular | Self::MapleBold | Self::MapleItalic | Self::MapleBoldItalic
        ) {
            // The build script writes frames with their original length, allowing one allocation.
            let size = zstd::zstd_safe::get_frame_content_size(bytes)
                .map_err(|error| anyhow::anyhow!("invalid bundled font {self:?}: {error}"))?
                .context("bundled font frame has no content size")?;
            zstd::bulk::decompress(bytes, usize::try_from(size)?)
                .with_context(|| format!("failed to decompress bundled font {self:?}"))
        } else {
            Ok(bytes.to_vec())
        }
    }
}

#[allow(dead_code)]
const ALL_TERMINAL_FACES: &[BundledTerminalFace] = &[
    BundledTerminalFace::JetBrainsRegular,
    BundledTerminalFace::JetBrainsBold,
    BundledTerminalFace::JetBrainsItalic,
    BundledTerminalFace::JetBrainsBoldItalic,
    BundledTerminalFace::MesloRegular,
    BundledTerminalFace::MesloBold,
    BundledTerminalFace::MesloItalic,
    BundledTerminalFace::MesloBoldItalic,
    BundledTerminalFace::MapleRegular,
    BundledTerminalFace::MapleBold,
    BundledTerminalFace::MapleItalic,
    BundledTerminalFace::MapleBoldItalic,
];

static LOADED_TERMINAL_FACES: LazyLock<Mutex<HashSet<BundledTerminalFace>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub(crate) fn load_terminal_font_open_critical(
    settings: &PersistedSettings,
    text_system: &TextSystem,
) -> Result<()> {
    let faces = critical_faces_for_settings(settings);
    register_faces(text_system, &faces)
}

pub(crate) fn load_terminal_cjk_fallback_regular(
    text_system: &TextSystem,
    cjk_font_family: &str,
) -> Result<()> {
    if !should_load_terminal_cjk_fallback(cjk_font_family) {
        return Ok(());
    }
    register_faces(text_system, &[BundledTerminalFace::MapleRegular])
}

pub(crate) fn load_terminal_font_explicit_secondary_faces(
    settings: &PersistedSettings,
    text_system: &TextSystem,
) -> Result<()> {
    let faces = secondary_faces_for_explicit_settings(settings);
    register_faces(text_system, &faces)
}

fn critical_faces_for_family(family: FontFamily) -> &'static [BundledTerminalFace] {
    match family {
        // Tauri prepares regular+bold for Latin bundled fonts before open.
        FontFamily::Jetbrains => &[
            BundledTerminalFace::JetBrainsRegular,
            BundledTerminalFace::JetBrainsBold,
        ],
        FontFamily::Meslo => &[
            BundledTerminalFace::MesloRegular,
            BundledTerminalFace::MesloBold,
        ],
        // Maple is large: regular is the critical path; other weights stay
        // deferred until native grows an idle/background font warmer.
        FontFamily::Maple => &[BundledTerminalFace::MapleRegular],
        FontFamily::Cascadia | FontFamily::Consolas | FontFamily::Menlo | FontFamily::Custom => &[],
    }
}

fn critical_faces_for_settings(settings: &PersistedSettings) -> Vec<BundledTerminalFace> {
    // The editor and Markdown code surfaces always use the bundled JetBrains family,
    // independently of the terminal's selected family.
    let mut faces = vec![BundledTerminalFace::JetBrainsRegular];
    let terminal_faces = critical_faces_for_family(settings.terminal.font_family);
    let terminal_faces = if terminal_faces.is_empty() {
        // System and custom choices still need a bundled monospace fallback if lookup fails.
        critical_faces_for_family(FontFamily::Jetbrains)
    } else {
        terminal_faces
    };
    for face in terminal_faces {
        if !faces.contains(face) {
            faces.push(*face);
        }
    }
    if settings.terminal.cjk_font_family.trim() == oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY
        && !faces.contains(&BundledTerminalFace::MapleRegular)
    {
        // Explicit CJK fallback selection should be ready with the terminal,
        // while Auto keeps the existing delayed fallback warmup path.
        faces.push(BundledTerminalFace::MapleRegular);
    }
    faces
}

fn should_load_terminal_cjk_fallback(cjk_font_family: &str) -> bool {
    let cjk_font_family = cjk_font_family.trim();
    cjk_font_family.is_empty() || cjk_font_family == oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY
}

fn secondary_faces_for_explicit_settings(settings: &PersistedSettings) -> Vec<BundledTerminalFace> {
    let uses_maple_terminal = settings.terminal.font_family == FontFamily::Maple;
    let uses_maple_cjk =
        settings.terminal.cjk_font_family.trim() == oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY;
    if !uses_maple_terminal && !uses_maple_cjk {
        return Vec::new();
    }

    // These CJK faces are large, so idle startup loads only regular fallback.
    // Explicit Maple settings can pay the cost to keep bold and italic text faithful.
    vec![
        BundledTerminalFace::MapleBold,
        BundledTerminalFace::MapleItalic,
        BundledTerminalFace::MapleBoldItalic,
    ]
}

fn register_faces(text_system: &TextSystem, faces: &[BundledTerminalFace]) -> Result<()> {
    let mut inserted_faces = Vec::new();
    let fonts = {
        let mut loaded = LOADED_TERMINAL_FACES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut fonts = Vec::new();
        for face in faces {
            if loaded.contains(face) || inserted_faces.contains(face) {
                continue;
            }
            fonts.push(Cow::Owned(face.load()?));
            inserted_faces.push(*face);
        }
        // A decode failure must leave every face retryable, including earlier faces in this batch.
        loaded.extend(inserted_faces.iter().copied());
        fonts
    };
    if fonts.is_empty() {
        return Ok(());
    }
    if let Err(error) = text_system.add_fonts(fonts) {
        let mut loaded = LOADED_TERMINAL_FACES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for face in inserted_faces {
            loaded.remove(&face);
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_maple_faces_preserve_the_complete_original_fonts() {
        for (face, style) in [
            (BundledTerminalFace::MapleRegular, "Regular"),
            (BundledTerminalFace::MapleBold, "Bold"),
            (BundledTerminalFace::MapleItalic, "Italic"),
            (BundledTerminalFace::MapleBoldItalic, "BoldItalic"),
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "resources/fonts/MapleMono/MapleMono-NF-CN-Subset-{style}.ttf"
            ));
            let original = std::fs::read(path).unwrap();
            let decoded = face.load().unwrap();
            assert!(
                decoded == original,
                "{face:?} must preserve every original byte"
            );
            assert!(
                face.embedded_bytes().len() < original.len(),
                "{face:?} must reduce embedded size"
            );
        }
    }

    #[test]
    fn bundled_terminal_faces_use_one_runtime_family_name() {
        for face in ALL_TERMINAL_FACES {
            let expected_family = match face {
                BundledTerminalFace::JetBrainsRegular
                | BundledTerminalFace::JetBrainsBold
                | BundledTerminalFace::JetBrainsItalic
                | BundledTerminalFace::JetBrainsBoldItalic => {
                    oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY
                }
                BundledTerminalFace::MesloRegular
                | BundledTerminalFace::MesloBold
                | BundledTerminalFace::MesloItalic
                | BundledTerminalFace::MesloBoldItalic => oxideterm_settings::MESLO_SUBSET_FAMILY,
                BundledTerminalFace::MapleRegular
                | BundledTerminalFace::MapleBold
                | BundledTerminalFace::MapleItalic
                | BundledTerminalFace::MapleBoldItalic => {
                    oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY
                }
            };

            let bytes = face.load().unwrap();
            let runtime_family_names = sfnt_runtime_family_names(&bytes);
            assert!(
                !runtime_family_names.is_empty(),
                "{face:?} must declare a runtime family name"
            );
            assert!(
                runtime_family_names
                    .iter()
                    .all(|family| family == expected_family),
                "{face:?} declares conflicting runtime family names: {runtime_family_names:?}"
            );
        }
    }

    #[test]
    fn app_code_font_is_loaded_for_every_terminal_family() {
        for family in [
            FontFamily::Jetbrains,
            FontFamily::Meslo,
            FontFamily::Maple,
            FontFamily::Cascadia,
            FontFamily::Consolas,
            FontFamily::Menlo,
            FontFamily::Custom,
        ] {
            let mut settings = PersistedSettings::default();
            settings.terminal.font_family = family;

            assert!(
                critical_faces_for_settings(&settings)
                    .contains(&BundledTerminalFace::JetBrainsRegular),
                "{family:?} must keep the app code font available"
            );
        }
    }

    fn sfnt_runtime_family_names(bytes: &[u8]) -> Vec<String> {
        let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
            return Vec::new();
        };
        face.names()
            .into_iter()
            .filter(|name| {
                name.name_id == ttf_parser::name_id::FAMILY
                    || name.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY
            })
            .filter_map(|name| name.to_string())
            .collect()
    }
}
