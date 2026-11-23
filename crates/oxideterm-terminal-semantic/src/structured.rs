// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic semantic highlighters for common operational output formats
//! (HTTP access logs, JSON/YAML keys, and multi-language stack traces).

use std::ops::Range;

use crate::{SemanticClass, SemanticLineRole, scheme::Candidate};

const HTTP_PRIORITY: u8 = 104;
const HTTP_KV_STATUS_PRIORITY: u8 = 92;
const JSON_KEY_PRIORITY: u8 = 102;
const YAML_KEY_PRIORITY: u8 = 88;
const STACK_PATH_PRIORITY: u8 = 103;
const STACK_FRAME_PRIORITY: u8 = 96;

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    if role == SemanticLineRole::Command || text.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let bytes = text.as_bytes();
    let has_colon = bytes.contains(&b':');
    let has_equals = bytes.contains(&b'=');

    if text.contains("HTTP/") || has_equals || has_colon {
        push_http_candidates(text, &allows_class, &mut candidates);
    }
    if has_colon {
        push_json_key_candidates(text, &allows_class, &mut candidates);
        if matches!(role, SemanticLineRole::Output | SemanticLineRole::Unknown) {
            push_yaml_key_candidates(text, &allows_class, &mut candidates);
        }
    }
    // Python frame lines (`File "...", line 42, in fn`) have no colon, so check before gating on `:`.
    if has_colon || text.contains("File \"") {
        push_stack_trace_candidates(text, &allows_class, &mut candidates);
    }
    candidates
}

fn push_http_candidates(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find("HTTP/") {
        let http_start = search_from + rel;
        let http_end = scan_http_version_end(text, http_start);
        search_from = http_end;
        if http_end <= http_start + 5 {
            continue;
        }

        if allows_class(SemanticClass::Info) {
            candidates.push(Candidate::new(
                http_start..http_end,
                SemanticClass::Info,
                HTTP_PRIORITY - 2,
            ));
        }

        // Look backward for `METHOD /path` right before `HTTP/x.y`.
        push_http_method_and_target(text, http_start, allows_class, candidates);

        // Look forward across optional closing quote and spaces for a 3-digit HTTP status code.
        let mut after = http_end;
        let bytes = text.as_bytes();
        if after < bytes.len() && matches!(bytes[after], b'"' | b'\'') {
            after += 1;
        }
        while after < bytes.len() && bytes[after] == b' ' {
            after += 1;
        }
        if let Some((status_range, status_class)) = parse_http_status_token(text, after)
            && allows_class(status_class)
        {
            candidates.push(Candidate::new(status_range, status_class, HTTP_PRIORITY));
        }
    }

    // Structured log fields such as `status=502` or `status_code=404`.
    push_http_kv_status_candidates(text, allows_class, candidates);
}

fn scan_http_version_end(text: &str, http_start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = http_start + 5;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    end
}

fn push_http_method_and_target(
    text: &str,
    http_start: usize,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let before = text[..http_start].trim_end_matches(' ');
    if before.len() == http_start {
        return;
    }
    let Some(target_start) = before
        .rfind(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '"' | '\''))
        .map(|idx| idx + 1)
    else {
        return;
    };
    let target = &before[target_start..];
    if target.is_empty() {
        return;
    }
    let before_target = before[..target_start].trim_end_matches(' ');
    let method_start = before_target
        .rfind(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '"' | '\'' | '['))
        .map_or(0, |idx| idx + 1);
    let method = &before_target[method_start..];
    if !is_http_method(method) {
        return;
    }
    if allows_class(SemanticClass::Keyword) {
        candidates.push(Candidate::new(
            method_start..before_target.len(),
            SemanticClass::Keyword,
            HTTP_PRIORITY,
        ));
    }
    if (target.starts_with('/') || target == "*") && allows_class(SemanticClass::Path) {
        candidates.push(Candidate::new(
            target_start..before.len(),
            SemanticClass::Path,
            HTTP_PRIORITY - 1,
        ));
    }
}

fn push_http_kv_status_candidates(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let bytes = text.as_bytes();
    for key in ["status_code", "http_status", "status"] {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(key) {
            let start = search_from + rel;
            let after_key = start + key.len();
            search_from = after_key;
            if start > 0 && is_ident_byte(bytes[start - 1]) {
                continue;
            }
            if after_key >= bytes.len() {
                continue;
            }
            let sep = bytes[after_key];
            if sep != b'=' && sep != b':' {
                continue;
            }
            // For `"status": 200`, require opening quote before key so prose like `status: 200` still works.
            let mut val_start = after_key + 1;
            if sep == b':' && start > 0 && bytes[start - 1] == b'"' {
                continue;
            }
            while val_start < bytes.len() && bytes[val_start] == b' ' {
                val_start += 1;
            }
            if let Some((status_range, status_class)) = parse_http_status_token(text, val_start)
                && allows_class(status_class)
            {
                candidates.push(Candidate::new(
                    status_range,
                    status_class,
                    HTTP_KV_STATUS_PRIORITY,
                ));
            }
        }
    }
}

fn parse_http_status_token(text: &str, start: usize) -> Option<(Range<usize>, SemanticClass)> {
    let bytes = text.as_bytes();
    if start + 3 > bytes.len() {
        return None;
    }
    if !bytes[start..start + 3].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let end = start + 3;
    if end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'.' | b'_' | b'-'))
    {
        return None;
    }
    let code: u16 = text[start..end].parse().ok()?;
    let class = match code {
        200..=299 => SemanticClass::Success,
        300..=399 => SemanticClass::Info,
        400..=499 => SemanticClass::Warning,
        500..=599 => SemanticClass::Error,
        _ => return None,
    };
    Some((start..end, class))
}

fn is_http_method(token: &str) -> bool {
    matches!(
        token,
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "CONNECT" | "TRACE"
    )
}

fn push_json_key_candidates(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let bytes = text.as_bytes();
    if !bytes.contains(&b'"') {
        return;
    }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let quote_start = i;
        i += 1;
        let mut escaped = false;
        while i < bytes.len() {
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if bytes[i] == b'\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if bytes[i] == b'"' {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            break;
        }
        let quote_end = i + 1;
        i = quote_end;

        let mut colon = quote_end;
        while colon < bytes.len() && bytes[colon] == b' ' {
            colon += 1;
        }
        if colon >= bytes.len() || bytes[colon] != b':' {
            continue;
        }
        let key_len = quote_end - quote_start;
        if key_len <= 2 || !has_json_key_prefix(bytes, quote_start) {
            continue;
        }
        if allows_class(SemanticClass::Variable) {
            candidates.push(Candidate::new(
                quote_start..quote_end,
                SemanticClass::Variable,
                JSON_KEY_PRIORITY,
            ));
        }
        if allows_class(SemanticClass::Operator) {
            candidates.push(Candidate::new(
                colon..colon + 1,
                SemanticClass::Operator,
                JSON_KEY_PRIORITY,
            ));
        }
    }
}

fn has_json_key_prefix(bytes: &[u8], quote_start: usize) -> bool {
    let mut idx = quote_start;
    while idx > 0 && matches!(bytes[idx - 1], b' ' | b'\t') {
        idx -= 1;
    }
    idx == 0 || matches!(bytes[idx - 1], b'{' | b',')
}

fn push_yaml_key_candidates(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() && matches!(bytes[start], b' ' | b'\t') {
        start += 1;
    }
    if start >= bytes.len() {
        return;
    }
    // Support YAML sequence items like `  - name: web`.
    if bytes[start] == b'-' && start + 1 < bytes.len() && bytes[start + 1] == b' ' {
        start += 2;
        while start < bytes.len() && bytes[start] == b' ' {
            start += 1;
        }
    }
    if start >= bytes.len() || !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_') {
        return;
    }
    let mut end = start + 1;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-' | b'.'))
    {
        end += 1;
    }
    if end >= bytes.len() || bytes[end] != b':' {
        return;
    }
    let after_colon = end + 1;
    if after_colon < bytes.len() && !matches!(bytes[after_colon], b' ' | b'\t') {
        return;
    }
    let key = &text[start..end];
    if is_reserved_non_yaml_label(key) || key.contains('.') && has_source_extension(key) {
        return;
    }
    if allows_class(SemanticClass::Variable) {
        candidates.push(Candidate::new(
            start..end,
            SemanticClass::Variable,
            YAML_KEY_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Operator) {
        candidates.push(Candidate::new(
            end..after_colon,
            SemanticClass::Operator,
            YAML_KEY_PRIORITY,
        ));
    }
}

fn is_reserved_non_yaml_label(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "error"
            | "fatal"
            | "critical"
            | "panic"
            | "warn"
            | "warning"
            | "info"
            | "note"
            | "help"
            | "debug"
            | "trace"
            | "http"
            | "https"
            | "file"
            | "sha256"
            | "sha1"
            | "md5"
    ) || key.ends_with("Error")
        || key.ends_with("Exception")
}

fn push_stack_trace_candidates(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let indent = text.len() - trimmed.len();

    // Python traceback header & frames.
    if trimmed.starts_with("Traceback (most recent call last):") {
        if allows_class(SemanticClass::Error) {
            candidates.push(Candidate::new(
                indent..indent + "Traceback".len(),
                SemanticClass::Error,
                STACK_FRAME_PRIORITY,
            ));
        }
        return;
    }
    if push_python_frame(text, indent, allows_class, candidates) {
        return;
    }
    if push_python_exception_heading(text, indent, allows_class, candidates) {
        return;
    }

    // Go goroutine header & `.go:line +0x...` frame lines.
    if push_go_frame(text, indent, allows_class, candidates) {
        return;
    }

    // Node.js / JS / TS / Java / Rust backtrace `at fn (file:line:col)` or `at file:line:col`.
    push_at_frame(text, indent, allows_class, candidates);
}

fn push_python_frame(
    text: &str,
    indent: usize,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) -> bool {
    let trimmed = &text[indent..];
    let Some(after_file) = trimmed.strip_prefix("File \"") else {
        return false;
    };
    let Some(quote_rel) = after_file.find('"') else {
        return false;
    };
    let path_start = indent + "File ".len();
    let path_end = path_start + 1 + quote_rel + 1;
    let rest_start = path_end;
    let Some(after_line_prefix) = text[rest_start..].strip_prefix(", line ") else {
        return false;
    };
    let num_start = rest_start + ", line ".len();
    let num_len = after_line_prefix
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if num_len == 0 {
        return false;
    }
    let num_end = num_start + num_len;

    if allows_class(SemanticClass::Keyword) {
        candidates.push(Candidate::new(
            indent..indent + 4,
            SemanticClass::Keyword,
            STACK_FRAME_PRIORITY,
        ));
        candidates.push(Candidate::new(
            rest_start + 2..rest_start + 6,
            SemanticClass::Keyword,
            STACK_FRAME_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Path) {
        candidates.push(Candidate::new(
            path_start..path_end,
            SemanticClass::Path,
            STACK_PATH_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Number) {
        candidates.push(Candidate::new(
            num_start..num_end,
            SemanticClass::Number,
            STACK_FRAME_PRIORITY,
        ));
    }
    if let Some(after_in) = text[num_end..].strip_prefix(", in ") {
        let in_start = num_end + 2;
        let fn_start = num_end + ", in ".len();
        let fn_len = after_in
            .bytes()
            .take_while(|byte| !byte.is_ascii_whitespace())
            .count();
        if allows_class(SemanticClass::Keyword) {
            candidates.push(Candidate::new(
                in_start..in_start + 2,
                SemanticClass::Keyword,
                STACK_FRAME_PRIORITY,
            ));
        }
        if fn_len > 0 && allows_class(SemanticClass::Command) {
            candidates.push(Candidate::new(
                fn_start..fn_start + fn_len,
                SemanticClass::Command,
                STACK_FRAME_PRIORITY,
            ));
        }
    }
    true
}

fn push_python_exception_heading(
    text: &str,
    indent: usize,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) -> bool {
    if indent != 0 {
        return false;
    }
    let Some(colon) = text.find(':') else {
        return false;
    };
    let head = &text[..colon];
    if head.is_empty()
        || !head.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
        || !head
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
    {
        return false;
    }
    let leaf = head.rsplit('.').next().unwrap_or(head);
    if !(leaf.ends_with("Error") || leaf.ends_with("Exception") || leaf == "KeyboardInterrupt") {
        return false;
    }
    if allows_class(SemanticClass::Error) {
        candidates.push(Candidate::new(
            0..colon,
            SemanticClass::Error,
            STACK_FRAME_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Operator) {
        candidates.push(Candidate::new(
            colon..colon + 1,
            SemanticClass::Operator,
            STACK_FRAME_PRIORITY,
        ));
    }
    true
}

fn push_go_frame(
    text: &str,
    indent: usize,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) -> bool {
    let trimmed = &text[indent..];
    if let Some(rest) = trimmed.strip_prefix("goroutine ") {
        let num_start = indent + "goroutine ".len();
        let num_len = rest.bytes().take_while(u8::is_ascii_digit).count();
        if num_len > 0 && rest[num_len..].starts_with(" [") {
            if allows_class(SemanticClass::Keyword) {
                candidates.push(Candidate::new(
                    indent..indent + "goroutine".len(),
                    SemanticClass::Keyword,
                    STACK_FRAME_PRIORITY,
                ));
            }
            if allows_class(SemanticClass::Number) {
                candidates.push(Candidate::new(
                    num_start..num_start + num_len,
                    SemanticClass::Number,
                    STACK_FRAME_PRIORITY,
                ));
            }
            return true;
        }
    }

    if indent == 0 {
        return false;
    }
    let token_end = indent
        + trimmed
            .bytes()
            .take_while(|byte| !byte.is_ascii_whitespace())
            .count();
    let token = &text[indent..token_end];
    if !token.contains(".go:") {
        return false;
    }
    if !push_source_location_token(text, indent..token_end, allows_class, candidates) {
        return false;
    }
    let trailing = text[token_end..].trim_start();
    if let Some(hex) = trailing.strip_prefix("+0x") {
        let hex_len = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
        if hex_len > 0 && allows_class(SemanticClass::Comment) {
            let offset_start = text.len() - trailing.len();
            candidates.push(Candidate::new(
                offset_start..offset_start + 3 + hex_len,
                SemanticClass::Comment,
                STACK_FRAME_PRIORITY,
            ));
        }
    }
    true
}

fn push_at_frame(
    text: &str,
    indent: usize,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    if indent == 0 {
        return;
    }
    let trimmed = &text[indent..];
    let Some(rest) = trimmed.strip_prefix("at ") else {
        return;
    };
    let rest_start = indent + 3;
    let (fn_range, loc_range) = if let Some(open_rel) = rest.rfind(" (")
        && rest.ends_with(')')
    {
        let fn_start = rest_start;
        let fn_end = rest_start + open_rel;
        let loc_start = fn_end + 2;
        let loc_end = text.len() - 1;
        (Some(fn_start..fn_end), loc_start..loc_end)
    } else {
        let loc_end = rest_start
            + rest
                .bytes()
                .take_while(|byte| !byte.is_ascii_whitespace())
                .count();
        (None, rest_start..loc_end)
    };

    if !push_source_location_token(text, loc_range, allows_class, candidates) {
        return;
    }
    if allows_class(SemanticClass::Keyword) {
        candidates.push(Candidate::new(
            indent..indent + 2,
            SemanticClass::Keyword,
            STACK_FRAME_PRIORITY,
        ));
    }
    if let Some(fn_range) = fn_range
        && !text[fn_range.clone()].trim().is_empty()
        && allows_class(SemanticClass::Command)
    {
        candidates.push(Candidate::new(
            fn_range,
            SemanticClass::Command,
            STACK_FRAME_PRIORITY,
        ));
    }
}

fn push_source_location_token(
    text: &str,
    range: Range<usize>,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) -> bool {
    let token = &text[range.clone()];
    let Some((before_last, last_part)) = token.rsplit_once(':') else {
        return false;
    };
    if last_part.is_empty() || !last_part.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let last_colon = range.start + before_last.len();
    let (path_end, line_range, col_colon, col_range) = if let Some((path_part, line_part)) =
        before_last.rsplit_once(':')
        && !line_part.is_empty()
        && line_part.bytes().all(|byte| byte.is_ascii_digit())
    {
        let first_colon = range.start + path_part.len();
        (
            first_colon,
            first_colon + 1..last_colon,
            Some(last_colon),
            Some(last_colon + 1..range.end),
        )
    } else {
        (last_colon, last_colon + 1..range.end, None, None)
    };

    let path = &text[range.start..path_end];
    if path.is_empty() || !(has_source_extension(path) || path.contains('/') || path.contains('\\'))
    {
        return false;
    }
    if allows_class(SemanticClass::Path) {
        candidates.push(Candidate::new(
            range.start..path_end,
            SemanticClass::Path,
            STACK_PATH_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Operator) {
        candidates.push(Candidate::new(
            path_end..path_end + 1,
            SemanticClass::Operator,
            STACK_PATH_PRIORITY,
        ));
        if let Some(colon) = col_colon {
            candidates.push(Candidate::new(
                colon..colon + 1,
                SemanticClass::Operator,
                STACK_PATH_PRIORITY,
            ));
        }
    }
    if allows_class(SemanticClass::Number) {
        candidates.push(Candidate::new(
            line_range,
            SemanticClass::Number,
            STACK_PATH_PRIORITY,
        ));
        if let Some(col_range) = col_range {
            candidates.push(Candidate::new(
                col_range,
                SemanticClass::Number,
                STACK_PATH_PRIORITY,
            ));
        }
    }
    true
}

fn has_source_extension(path: &str) -> bool {
    let Some((_, ext)) = path.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "rs" | "py"
            | "go"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "mjs"
            | "cjs"
            | "java"
            | "kt"
            | "rb"
            | "php"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "swift"
            | "cs"
            | "lua"
            | "zig"
    )
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use crate::{SemanticClass, SemanticLineRole, classify_line};

    fn spans_of(text: &str) -> Vec<(&str, SemanticClass)> {
        classify_line(text, SemanticLineRole::Output)
            .into_iter()
            .map(|span| (&text[span.range], span.class))
            .collect()
    }

    #[test]
    fn http_access_logs_classify_method_path_version_and_status_bands() {
        let ok =
            r#"10.0.0.1 - - [22/Nov/2026:13:20:01 +0800] "GET /api/v1/users HTTP/1.1" 200 1842"#;
        let ok_spans = spans_of(ok);
        assert!(ok_spans.contains(&("GET", SemanticClass::Keyword)));
        assert!(ok_spans.contains(&("/api/v1/users", SemanticClass::Path)));
        assert!(ok_spans.contains(&("HTTP/1.1", SemanticClass::Info)));
        assert!(ok_spans.contains(&("200", SemanticClass::Success)));

        let warn = r#""POST /login HTTP/2" 404 92"#;
        let warn_spans = spans_of(warn);
        assert!(warn_spans.contains(&("POST", SemanticClass::Keyword)));
        assert!(warn_spans.contains(&("404", SemanticClass::Warning)));

        let err = "upstream failed method=DELETE status=502 latency=120ms";
        let err_spans = spans_of(err);
        assert!(err_spans.contains(&("502", SemanticClass::Error)));
    }

    #[test]
    fn json_and_yaml_keys_separate_from_values_without_stealing_urls_or_severity() {
        let json = r#"{"service":"api","port":8080,"url":"https://example.com"}"#;
        let json_spans = spans_of(json);
        assert!(json_spans.contains(&("\"service\"", SemanticClass::Variable)));
        assert!(json_spans.contains(&("\"api\"", SemanticClass::String)));
        assert!(json_spans.contains(&("\"port\"", SemanticClass::Variable)));
        assert!(json_spans.contains(&("8080", SemanticClass::Number)));
        assert!(json_spans.contains(&("\"url\"", SemanticClass::Variable)));

        let yaml = "  replicas: 3";
        let yaml_spans = spans_of(yaml);
        assert!(yaml_spans.contains(&("replicas", SemanticClass::Variable)));
        assert!(yaml_spans.contains(&(":", SemanticClass::Operator)));
        assert!(yaml_spans.contains(&("3", SemanticClass::Number)));

        let error_line = "error: connection refused";
        let err_spans = spans_of(error_line);
        assert!(!err_spans.contains(&("error", SemanticClass::Variable)));
        assert!(err_spans.contains(&("error", SemanticClass::Error)));
    }

    #[test]
    fn multi_language_stack_traces_highlight_frames_paths_lines_and_exceptions() {
        let py = r#"  File "/app/worker.py", line 42, in handle_request"#;
        let py_spans = spans_of(py);
        assert!(py_spans.contains(&("File", SemanticClass::Keyword)));
        assert!(py_spans.contains(&("\"/app/worker.py\"", SemanticClass::Path)));
        assert!(py_spans.contains(&("42", SemanticClass::Number)));
        assert!(py_spans.contains(&("handle_request", SemanticClass::Command)));

        let py_err = "ValueError: invalid port";
        assert!(spans_of(py_err).contains(&("ValueError", SemanticClass::Error)));

        let go = "\t/srv/app/server.go:128 +0x1a4";
        let go_spans = spans_of(go);
        assert!(go_spans.contains(&("/srv/app/server.go", SemanticClass::Path)));
        assert!(go_spans.contains(&("128", SemanticClass::Number)));
        assert!(go_spans.contains(&("+0x1a4", SemanticClass::Comment)));

        let node = "    at processRequest (/app/src/server.ts:45:12)";
        let node_spans = spans_of(node);
        assert!(node_spans.contains(&("at", SemanticClass::Keyword)));
        assert!(node_spans.contains(&("processRequest", SemanticClass::Command)));
        assert!(node_spans.contains(&("/app/src/server.ts", SemanticClass::Path)));
        assert!(node_spans.contains(&("45", SemanticClass::Number)));
        assert!(node_spans.contains(&("12", SemanticClass::Number)));
    }
}
