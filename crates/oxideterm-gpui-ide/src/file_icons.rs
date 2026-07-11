// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct IdeFileIcon {
    pub path: &'static str,
    pub color: u32,
}

// Tauri `src/lib/fileIcons.tsx` uses Tailwind text-* classes for file kinds.
// Keep those literal source colors named here; only default/folder colors flow
// through theme tokens because Tauri uses CSS theme variables for them.
const TAILWIND_BLUE_400: u32 = 0x60a5fa;
const TAILWIND_BLUE_500: u32 = 0x3b82f6;
const TAILWIND_YELLOW_400: u32 = 0xfacc15;
const TAILWIND_YELLOW_500: u32 = 0xeab308;
const TAILWIND_ORANGE_400: u32 = 0xfb923c;
const TAILWIND_ORANGE_500: u32 = 0xf97316;
const TAILWIND_GREEN_400: u32 = 0x4ade80;
const TAILWIND_GREEN_500: u32 = 0x22c55e;
const TAILWIND_CYAN_400: u32 = 0x22d3ee;
const TAILWIND_RED_400: u32 = 0xf87171;
const TAILWIND_RED_500: u32 = 0xef4444;
const TAILWIND_PURPLE_400: u32 = 0xc084fc;
const TAILWIND_PINK_400: u32 = 0xf472b6;
const TAILWIND_SLATE_400: u32 = 0x94a3b8;
const TAILWIND_EMERALD_400: u32 = 0x34d399;

pub(super) fn file_icon(filename: &str, tokens: &ThemeTokens) -> IdeFileIcon {
    let lower_name = filename.to_ascii_lowercase();
    if let Some(icon) = special_file_icon(&lower_name, tokens) {
        return icon;
    }
    let extension = lower_name
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("");

    // Tauri `src/lib/fileIcons.tsx` maps file types to lucide icons and
    // Tailwind semantic colors. Native keeps the same groups, translated to
    // theme/terminal colors so dark/light/custom themes still drive the result.
    let path = match extension {
        "ts" | "tsx" | "mts" | "cts" | "rs" | "vue" | "svelte" | "astro" => {
            "lucide/file-code-2.svg"
        }
        "js" | "jsx" | "mjs" | "cjs" | "py" | "pyw" | "pyi" | "go" | "rb" | "rake" | "java"
        | "kt" | "kts" | "scala" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "swift"
        | "m" | "mm" | "php" | "lua" | "r" | "rmd" | "sql" | "html" | "htm" | "xhtml" | "xml"
        | "xsd" | "xsl" | "css" | "scss" | "sass" | "less" | "diff" | "patch" => {
            "lucide/file-code.svg"
        }
        "json" | "jsonc" => "lucide/file-json-2.svg",
        "json5" => "lucide/file-json.svg",
        "yaml" | "yml" | "toml" | "ini" | "conf" | "cfg" | "env" | "envrc" | "properties" => {
            "lucide/file-cog.svg"
        }
        "md" | "markdown" | "txt" | "text" | "rst" | "adoc" | "org" | "pdf" | "doc" | "docx"
        | "log" | "tex" | "latex" => "lucide/file-text.svg",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "tiff" | "tif" | "svg" => {
            "lucide/file-image.svg"
        }
        "mp4" | "webm" | "mov" | "avi" | "mkv" | "flv" => "lucide/file-video.svg",
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" => "lucide/file-audio.svg",
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => "lucide/file-terminal.svg",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "lucide/file-archive.svg",
        "lock" => "lucide/file-lock.svg",
        "csv" | "tsv" | "xls" | "xlsx" => "lucide/file-spreadsheet.svg",
        _ => "lucide/file.svg",
    };
    IdeFileIcon {
        path,
        color: file_icon_color(extension, tokens),
    }
}

pub(super) fn folder_icon(is_open: bool, is_git: bool, tokens: &ThemeTokens) -> IdeFileIcon {
    IdeFileIcon {
        path: match (is_open, is_git) {
            (true, true) => "lucide/folder-git-2.svg",
            (false, true) => "lucide/folder-git.svg",
            (true, false) => "lucide/folder-open.svg",
            (false, false) => "lucide/folder.svg",
        },
        color: tokens.ui.text_secondary,
    }
}

fn special_file_icon(lower_name: &str, tokens: &ThemeTokens) -> Option<IdeFileIcon> {
    let path = match lower_name {
        "dockerfile" | "makefile" | "cmakelists.txt" => "lucide/file-terminal.svg",
        "docker-compose.yml"
        | "docker-compose.yaml"
        | ".dockerignore"
        | ".gitignore"
        | ".gitattributes"
        | ".gitmodules"
        | ".editorconfig"
        | ".prettierrc"
        | ".eslintrc"
        | ".eslintrc.json"
        | ".eslintrc.js"
        | "cargo.toml" => "lucide/file-cog.svg",
        "tsconfig.json" | "jsconfig.json" => "lucide/file-json-2.svg",
        "cargo.lock" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" => {
            "lucide/file-lock.svg"
        }
        "package.json" => "lucide/file-json-2.svg",
        "license" | "license.md" | "license.txt" | "readme" | "readme.md" | "readme.txt" => {
            "lucide/file-text.svg"
        }
        _ => return None,
    };
    Some(IdeFileIcon {
        path,
        color: special_file_color(lower_name, tokens),
    })
}

fn special_file_color(lower_name: &str, tokens: &ThemeTokens) -> u32 {
    match lower_name {
        "dockerfile" | "tsconfig.json" => TAILWIND_BLUE_400,
        ".gitignore" => TAILWIND_ORANGE_400,
        "cargo.toml" => TAILWIND_ORANGE_500,
        "package.json" => TAILWIND_GREEN_400,
        _ => tokens.ui.text_muted,
    }
}

fn file_icon_color(extension: &str, tokens: &ThemeTokens) -> u32 {
    match extension {
        "ts" | "tsx" | "mts" | "cts" => TAILWIND_BLUE_400,
        "js" | "jsx" | "mjs" | "cjs" | "svg" => TAILWIND_YELLOW_400,
        "rs" | "html" | "htm" | "svelte" => TAILWIND_ORANGE_500,
        "py" | "pyw" | "pyi" => TAILWIND_GREEN_400,
        "go" => TAILWIND_CYAN_400,
        "rb" | "rake" => TAILWIND_RED_400,
        "java" | "toml" => TAILWIND_ORANGE_400,
        "kt" | "kts" | "yaml" | "yml" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp"
        | "tiff" | "tif" => TAILWIND_PURPLE_400,
        "scala" => TAILWIND_RED_500,
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "css" => TAILWIND_BLUE_500,
        "json" | "jsonc" | "json5" => TAILWIND_YELLOW_500,
        "vue" => TAILWIND_EMERALD_400,
        "scss" | "sass" => TAILWIND_PINK_400,
        "sh" | "bash" | "zsh" => TAILWIND_GREEN_500,
        "md" | "markdown" => TAILWIND_SLATE_400,
        "lock" => tokens.ui.text_muted,
        _ => tokens.ui.text_muted,
    }
}
