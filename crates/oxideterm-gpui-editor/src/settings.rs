// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSettings {
    pub tab_size: usize,
    pub insert_spaces: bool,
    pub soft_wrap: bool,
    /// None wraps at the viewport edge without a fixed column limit.
    pub soft_wrap_column: Option<usize>,
    pub find_case_sensitive: bool,
    pub find_whole_word: bool,
    pub highlight_special_chars: bool,
    pub highlight_current_line: bool,
    pub indentation_markers: bool,
    pub placeholder: Option<String>,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            soft_wrap: false,
            soft_wrap_column: Some(120),
            find_case_sensitive: false,
            find_whole_word: false,
            highlight_special_chars: true,
            highlight_current_line: true,
            indentation_markers: true,
            placeholder: Some("...".to_string()),
        }
    }
}

impl EditorSettings {
    pub fn indentation_unit(&self) -> String {
        if self.insert_spaces {
            " ".repeat(self.tab_size.max(1))
        } else {
            "\t".to_string()
        }
    }
}
