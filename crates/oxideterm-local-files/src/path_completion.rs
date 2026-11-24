#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathCompletionRequest {
    pub parent_path: String,
    pub prefix: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathCompletionCandidate {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

const PATH_COMPLETION_MAX_MATCHES: usize = 512;

impl PathCompletionRequest {
    pub fn matching_candidates(
        &self,
        entries: &[PathCompletionCandidate],
    ) -> Vec<PathCompletionCandidate> {
        let prefix = self.prefix.to_lowercase();
        let mut suggestions = entries
            .iter()
            .filter(|entry| entry.name.to_lowercase().starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        suggestions.sort_by(|left, right| {
            right
                .is_directory
                .cmp(&left.is_directory)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        // Bound pathological directories without confusing viewport height with result count.
        suggestions.truncate(PATH_COMPLETION_MAX_MATCHES);
        suggestions
    }
}

pub fn local_path_completion_request(input: &str) -> Option<PathCompletionRequest> {
    if input.trim().is_empty() {
        return None;
    }
    let normalized = super::normalize_local_path(input);
    split_path_completion_request(
        &normalized,
        input.ends_with(['/', '\\']),
        |character| character == '/' || character == '\\',
        ".",
    )
}

pub fn split_path_completion_request(
    normalized: &str,
    input_ends_with_separator: bool,
    is_separator: impl Fn(char) -> bool,
    relative_parent: &str,
) -> Option<PathCompletionRequest> {
    if normalized.is_empty() {
        return None;
    }
    if input_ends_with_separator {
        let parent_path = trim_completion_trailing_separators(normalized, &is_separator);
        return Some(PathCompletionRequest {
            parent_path,
            prefix: String::new(),
        });
    }

    let separator = normalized
        .char_indices()
        .rev()
        .find(|(_, character)| is_separator(*character));
    let Some((separator_index, separator_character)) = separator else {
        return Some(PathCompletionRequest {
            parent_path: relative_parent.to_string(),
            prefix: normalized.to_string(),
        });
    };
    let prefix_start = separator_index + separator_character.len_utf8();
    let raw_parent = &normalized[..separator_index];
    let parent_path = if raw_parent.is_empty() {
        separator_character.to_string()
    } else if raw_parent.len() == 2 && raw_parent.as_bytes().get(1) == Some(&b':') {
        format!("{raw_parent}{separator_character}")
    } else {
        raw_parent.to_string()
    };
    Some(PathCompletionRequest {
        parent_path,
        prefix: normalized[prefix_start..].to_string(),
    })
}

fn trim_completion_trailing_separators(path: &str, is_separator: &impl Fn(char) -> bool) -> String {
    let trimmed = path.trim_end_matches(is_separator);
    if trimmed.is_empty() {
        path.chars()
            .next()
            .map(|character| character.to_string())
            .unwrap_or_else(|| "/".to_string())
    } else if trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':') {
        let separator = path.chars().last().unwrap_or(std::path::MAIN_SEPARATOR);
        format!("{trimmed}{separator}")
    } else {
        trimmed.to_string()
    }
}
