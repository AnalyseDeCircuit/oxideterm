// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSettings {
    pub tab_size: usize,
    pub insert_spaces: bool,
    pub soft_wrap: bool,
    pub soft_wrap_column: usize,
    pub find_case_sensitive: bool,
    pub find_whole_word: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            soft_wrap: false,
            soft_wrap_column: 120,
            find_case_sensitive: false,
            find_whole_word: false,
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
