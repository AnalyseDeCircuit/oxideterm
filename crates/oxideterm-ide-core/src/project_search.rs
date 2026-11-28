use crate::{IdeFileError, IdeFileErrorKind, IdeSearchQuery};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeSearchMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub preview: String,
    pub match_start: usize,
    pub match_end: usize,
}

impl IdeSearchQuery {
    pub fn text_matcher(&self) -> Result<regex::Regex, IdeFileError> {
        let pattern = if self.regex {
            self.pattern.clone()
        } else {
            regex::escape(&self.pattern)
        };
        regex::RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build()
            .map_err(|_| IdeFileError::new(IdeFileErrorKind::Other, "Invalid search pattern"))
    }
}

impl IdeSearchMatch {
    /// Columns identify UTF-8 byte positions in the complete line, even when its preview is clipped.
    pub fn from_line(
        path: &str,
        line_number: u32,
        line: &str,
        range: std::ops::Range<usize>,
    ) -> Self {
        let start = line[..range.start]
            .char_indices()
            .rev()
            .nth(79)
            .map_or(0, |(i, _)| i);
        let end = line[start..]
            .char_indices()
            .nth(200)
            .map_or(line.len(), |(i, _)| start + i);
        Self {
            path: path.into(),
            line: line_number,
            column: range.start as u32 + 1,
            preview: line[start..end].into(),
            match_start: range.start - start,
            match_end: range.end.min(end) - start,
        }
    }
}

pub struct SearchPaths {
    root: String,
    hidden: bool,
    includes: regex::RegexSet,
    excludes: regex::RegexSet,
}
impl SearchPaths {
    pub fn new(query: &IdeSearchQuery) -> Result<Self, IdeFileError> {
        fn patterns(globs: &[String]) -> Result<regex::RegexSet, IdeFileError> {
            let patterns = globs.iter().map(|glob| {
                let mut pattern = String::from("^");
                if !glob.contains('/') {
                    pattern.push_str("(?:.*/)?");
                }
                let mut chars = glob.chars().peekable();
                while let Some(ch) = chars.next() {
                    match ch {
                        '*' if chars.peek() == Some(&'*') => {
                            chars.next();
                            if chars.peek() == Some(&'/') {
                                chars.next();
                                pattern.push_str("(?:.*/)?");
                            } else {
                                pattern.push_str(".*");
                            }
                        }
                        '*' => pattern.push_str("[^/]*"),
                        '?' => pattern.push_str("[^/]"),
                        other => pattern.push_str(&regex::escape(&other.to_string())),
                    }
                }
                pattern.push('$');
                pattern
            });
            regex::RegexSet::new(patterns)
                .map_err(|_| IdeFileError::new(IdeFileErrorKind::Other, "Invalid search filter"))
        }
        Ok(Self {
            root: query.root_path.trim_end_matches('/').to_owned(),
            hidden: query.include_hidden,
            includes: patterns(&query.include_globs)?,
            excludes: patterns(&query.exclude_globs)?,
        })
    }
    pub fn allows(&self, path: &str) -> bool {
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .trim_start_matches('/');
        (self.hidden || !relative.split('/').any(|part| part.starts_with('.')))
            && (self.includes.is_empty() || self.includes.is_match(relative))
            && !self.excludes.is_match(relative)
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn search_filters_apply_to_root_nested_excluded_and_hidden_files() {
        let mut query = IdeSearchQuery::tauri_literal_project_search("text", "/work", 20, 0);
        query.include_globs = vec!["**/*.rs".into()];
        query.exclude_globs = vec!["vendor/**".into()];
        let paths = SearchPaths::new(&query).unwrap();
        for (path, expected) in [
            ("/work/main.rs", true),
            ("/work/src/lib.rs", true),
            ("/work/vendor/lib.rs", false),
            ("/work/.hidden/main.rs", false),
            ("/work/src/app.ts", false),
        ] {
            assert_eq!(paths.allows(path), expected, "{path}");
        }
        query.include_hidden = true;
        assert!(
            SearchPaths::new(&query)
                .unwrap()
                .allows("/work/.hidden/main.rs")
        );
    }

    #[test]
    fn regex_search_preserves_the_hit_in_a_clipped_unicode_line() {
        let mut query = IdeSearchQuery::tauri_literal_project_search(r"Δ\d+", "/work", 10, 1);
        let line = format!("{}δ123 end", "中".repeat(210));
        assert!(query.text_matcher().unwrap().find(&line).is_none());
        query.regex = true;
        let found = query.text_matcher().unwrap().find(&line).unwrap().range();
        let hit = IdeSearchMatch::from_line("/work/notes.txt", 4, &line, found);
        assert_eq!((hit.line, hit.column), (4, 631));
        assert_eq!(&hit.preview[hit.match_start..hit.match_end], "δ123");
        assert!(hit.preview.len() < line.len());
        query.case_sensitive = true;
        assert!(query.text_matcher().unwrap().find(&line).is_none());
    }
}
