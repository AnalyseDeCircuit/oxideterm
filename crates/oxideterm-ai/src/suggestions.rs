use regex::Regex;

use crate::AiFollowUpSuggestion;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiSuggestionParseResult {
    pub clean_content: String,
    pub suggestions: Vec<AiFollowUpSuggestion>,
}

const MAX_SUGGESTIONS: usize = 5;
const MAX_SUGGESTION_TEXT_LEN: usize = 200;
const MAX_SUGGESTION_ICON_LEN: usize = 30;

pub fn parse_ai_suggestions(content: &str) -> AiSuggestionParseResult {
    let Ok(block_re) = Regex::new(r"(?s)<suggestions>\s*(.*?)\s*</suggestions>\s*$") else {
        return AiSuggestionParseResult {
            clean_content: content.to_string(),
            suggestions: Vec::new(),
        };
    };
    let Some(block_match) = block_re.captures(content) else {
        return AiSuggestionParseResult {
            clean_content: content.to_string(),
            suggestions: Vec::new(),
        };
    };
    let Some(whole_block) = block_match.get(0) else {
        return AiSuggestionParseResult {
            clean_content: content.to_string(),
            suggestions: Vec::new(),
        };
    };
    let block_inner = block_match
        .get(1)
        .map(|matched| matched.as_str())
        .unwrap_or_default();
    let Ok(item_re) = Regex::new(r#"(?s)<s\s+icon="([^"]+)">(.*?)</s>"#) else {
        return AiSuggestionParseResult {
            clean_content: content[..whole_block.start()].trim_end().to_string(),
            suggestions: Vec::new(),
        };
    };

    let suggestions = item_re
        .captures_iter(block_inner)
        .filter_map(|captures| {
            let icon = captures.get(1)?.as_str().trim();
            let text = captures.get(2)?.as_str().trim();
            (!text.is_empty()
                && text.len() <= MAX_SUGGESTION_TEXT_LEN
                && icon.len() <= MAX_SUGGESTION_ICON_LEN)
                .then(|| AiFollowUpSuggestion {
                    icon: icon.to_string(),
                    text: text.to_string(),
                })
        })
        .take(MAX_SUGGESTIONS)
        .collect::<Vec<_>>();

    AiSuggestionParseResult {
        clean_content: content[..whole_block.start()].trim_end().to_string(),
        suggestions,
    }
}

pub fn ai_has_partial_suggestions_block(content: &str) -> bool {
    content.contains("<suggestions>") && !content.contains("</suggestions>")
}

pub fn ai_visible_suggestion_content(content: &str) -> String {
    let parsed = parse_ai_suggestions(content);
    if !parsed.suggestions.is_empty() {
        return parsed.clean_content;
    }
    if ai_has_partial_suggestions_block(content)
        && let Some(start) = content.find("<suggestions>")
    {
        return content[..start].trim_end().to_string();
    }
    content.to_string()
}
