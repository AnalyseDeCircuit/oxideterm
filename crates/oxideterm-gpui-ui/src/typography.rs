use gpui::{Div, FontWeight, ParentElement, SharedString, Styled, div, prelude::*, px, rgb};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonospaceDatumTone {
    Primary,
    Muted,
    Accent,
    Success,
    Warning,
    Error,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonospaceDatumOptions {
    pub tone: MonospaceDatumTone,
    pub text_size: Option<f32>,
    pub max_chars: Option<usize>,
    pub strong: bool,
}

impl MonospaceDatumOptions {
    pub const fn new(tone: MonospaceDatumTone) -> Self {
        Self {
            tone,
            text_size: None,
            max_chars: None,
            strong: false,
        }
    }

    pub const fn max_chars(mut self, max_chars: usize) -> Self {
        self.max_chars = Some(max_chars);
        self
    }

    pub const fn text_size(mut self, text_size: f32) -> Self {
        self.text_size = Some(text_size);
        self
    }

    pub const fn strong(mut self) -> Self {
        self.strong = true;
        self
    }
}

impl Default for MonospaceDatumOptions {
    fn default() -> Self {
        Self::new(MonospaceDatumTone::Primary)
    }
}

pub fn monospace_datum(
    tokens: &ThemeTokens,
    text: impl Into<String>,
    mono_font_family: Option<SharedString>,
    options: MonospaceDatumOptions,
) -> Div {
    let text = text.into();
    let text = if let Some(max_chars) = options.max_chars {
        middle_truncate_text(&text, max_chars)
    } else {
        text
    };
    let text_size = options.text_size.unwrap_or(tokens.metrics.ui_text_sm);

    // Machine-readable data keeps a compact rhythm across Git, filesystem,
    // command, runtime, and network surfaces. Parent rows own flex width; this
    // helper intentionally stays out of min-width decisions so GPUI measures it
    // like the pre-shared Git branch/path text rows.
    div()
        .truncate()
        .text_size(px(text_size))
        .line_height(px(text_size + 4.0))
        .font_weight(if options.strong {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        })
        .text_color(rgb(monospace_datum_color(tokens, options.tone)))
        .when_some(mono_font_family, |datum, family| datum.font_family(family))
        .child(text)
}

pub fn monospace_datum_color(tokens: &ThemeTokens, tone: MonospaceDatumTone) -> u32 {
    match tone {
        MonospaceDatumTone::Primary => tokens.ui.text,
        MonospaceDatumTone::Muted => tokens.ui.text_muted,
        MonospaceDatumTone::Accent => tokens.ui.accent,
        MonospaceDatumTone::Success => tokens.ui.success,
        MonospaceDatumTone::Warning => tokens.ui.warning,
        MonospaceDatumTone::Error => tokens.ui.error,
        MonospaceDatumTone::Info => tokens.ui.info,
    }
}

pub fn middle_truncate_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }

    // Use ASCII "..." instead of a single ellipsis so terminal-style datum
    // widths stay predictable across fonts and CJK fallback.
    let keep = max_chars - 3;
    let front = keep / 2 + keep % 2;
    let back = keep / 2;
    let prefix: String = text.chars().take(front).collect();
    let suffix: String = text
        .chars()
        .rev()
        .take(back)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

pub fn tauri_ui_font_family(configured_family: &str) -> SharedString {
    css_font_family_head(configured_family).unwrap_or_else(tauri_default_ui_font_family)
}

pub fn tauri_cjk_ui_font_family(configured_family: &str) -> SharedString {
    // CJK text follows the same user-selected UI family and platform fallback as other UI text.
    tauri_ui_font_family(configured_family)
}

pub fn css_font_family_head(configured_family: &str) -> Option<SharedString> {
    configured_family
        .split(',')
        .map(|family| family.trim().trim_matches(['"', '\'']))
        .find(|family| !family.is_empty())
        .map(gpui_font_family_name)
}

pub fn gpui_font_family_name(family: &str) -> SharedString {
    SharedString::from(normalize_font_family_name(family))
}

fn normalize_font_family_name(family: &str) -> String {
    match family.trim() {
        // Browsers resolve localized Windows font names in CSS. GPUI/CoreText
        // is more reliable with the canonical family name.
        "等线" => "DengXian".to_string(),
        "微软雅黑" => "Microsoft YaHei".to_string(),
        "黑体" => "SimHei".to_string(),
        "苹方" => "PingFang SC".to_string(),
        "思源黑体" => "Source Han Sans SC".to_string(),
        trimmed => trimmed.to_string(),
    }
}

fn tauri_default_ui_font_family() -> SharedString {
    // GPUI resolves this virtual family to the current platform's system UI font.
    SharedString::from(".SystemUIFont")
}
