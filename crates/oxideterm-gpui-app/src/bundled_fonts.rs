// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Bundled terminal font registration for the native GPUI app.
//!
//! The Tauri app uses web-font subsets for the same families. Native embeds the
//! decompressed TTF subset files because GPUI/font-kit loads SFNT font bytes.
//! Registration stays lazy: startup and terminal-open paths load only the
//! selected font's critical faces, matching Tauri's fontLoader strategy.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use anyhow::Result;
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
const MAPLE_REGULAR: &[u8] =
    include_bytes!("../resources/fonts/MapleMono/MapleMono-NF-CN-Subset-Regular.ttf");
const MAPLE_BOLD: &[u8] =
    include_bytes!("../resources/fonts/MapleMono/MapleMono-NF-CN-Subset-Bold.ttf");
const MAPLE_ITALIC: &[u8] =
    include_bytes!("../resources/fonts/MapleMono/MapleMono-NF-CN-Subset-Italic.ttf");
const MAPLE_BOLD_ITALIC: &[u8] =
    include_bytes!("../resources/fonts/MapleMono/MapleMono-NF-CN-Subset-BoldItalic.ttf");

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum BundledTerminalFace {
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
    fn bytes(self) -> &'static [u8] {
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
    register_faces(
        text_system,
        critical_faces_for_family(settings.terminal.font_family),
    )
}

pub(crate) fn load_terminal_cjk_fallback_regular(text_system: &TextSystem) -> Result<()> {
    register_faces(text_system, &[BundledTerminalFace::MapleRegular])
}

pub(crate) fn load_terminal_cjk_secondary_faces(text_system: &TextSystem) -> Result<()> {
    register_faces(
        text_system,
        &[
            BundledTerminalFace::MapleBold,
            BundledTerminalFace::MapleItalic,
            BundledTerminalFace::MapleBoldItalic,
        ],
    )
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

fn register_faces(text_system: &TextSystem, faces: &[BundledTerminalFace]) -> Result<()> {
    let mut inserted_faces = Vec::new();
    let fonts = {
        let mut loaded = LOADED_TERMINAL_FACES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        faces
            .iter()
            .filter_map(|face| {
                loaded.insert(*face).then(|| {
                    inserted_faces.push(*face);
                    Cow::Owned(face.bytes().to_vec())
                })
            })
            .collect::<Vec<_>>()
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
    fn bundled_terminal_font_manifest_covers_all_subset_styles() {
        assert_eq!(ALL_TERMINAL_FACES.len(), 12);
        assert!(
            ALL_TERMINAL_FACES
                .iter()
                .all(|face| !face.bytes().is_empty())
        );
        assert!(ALL_TERMINAL_FACES.iter().all(|face| is_sfnt(face.bytes())));
    }

    #[test]
    fn critical_faces_match_tauri_lazy_strategy() {
        assert_eq!(
            critical_faces_for_family(FontFamily::Jetbrains),
            &[
                BundledTerminalFace::JetBrainsRegular,
                BundledTerminalFace::JetBrainsBold,
            ]
        );
        assert_eq!(
            critical_faces_for_family(FontFamily::Maple),
            &[BundledTerminalFace::MapleRegular]
        );
        assert!(critical_faces_for_family(FontFamily::Custom).is_empty());
    }

    fn is_sfnt(bytes: &[u8]) -> bool {
        bytes.starts_with(b"\0\x01\0\0") || bytes.starts_with(b"OTTO") || bytes.starts_with(b"ttcf")
    }
}
