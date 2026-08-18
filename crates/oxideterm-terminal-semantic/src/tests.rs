// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    SemanticClass, SemanticLineRole, SemanticScheme, classify_line, classify_line_with_scheme,
};

fn matched_texts(text: &str) -> Vec<(&str, SemanticClass)> {
    classify_line(text, SemanticLineRole::Output)
        .into_iter()
        .map(|span| (&text[span.range], span.class))
        .collect()
}

#[test]
fn ubuntu_motd_status_phrases_receive_semantic_roles() {
    let error = "Expanded Security Maintenance is not enabled.";
    let success = "247 additional security updates can be applied immediately.";

    assert!(matched_texts(error).contains(&("not enabled", SemanticClass::Error)));
    assert!(matched_texts(success).contains(&("247", SemanticClass::Number)));
    assert!(matched_texts(success).contains(&("can be applied", SemanticClass::Success)));
}

#[test]
fn structured_terminal_values_are_classified_without_overlap() {
    let text = "2026-08-18 15:20:06 host 192.168.1.52 mac 02:3B:4C:5D:6E:7F /var/log/app.log";
    let spans = classify_line(text, SemanticLineRole::Output);
    let matches = spans
        .iter()
        .map(|span| (&text[span.range.clone()], span.class))
        .collect::<Vec<_>>();

    assert!(matches.contains(&("2026-08-18 15:20:06", SemanticClass::Timestamp)));
    assert!(matches.contains(&("192.168.1.52", SemanticClass::Address)));
    assert!(matches.contains(&("02:3B:4C:5D:6E:7F", SemanticClass::Address)));
    assert!(matches.contains(&("/var/log/app.log", SemanticClass::Path)));
    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start);
    }
}

#[test]
fn quoted_text_wins_over_nested_status_words_and_numbers() {
    let text = "message \"error 500\" returned";

    assert_eq!(
        matched_texts(text),
        vec![("\"error 500\"", SemanticClass::String)]
    );
}

#[test]
fn warning_terms_use_the_warning_class() {
    assert_eq!(
        matched_texts("Warning: update skipped"),
        vec![
            ("Warning", SemanticClass::Warning),
            ("skipped", SemanticClass::Warning),
        ]
    );
}

#[test]
fn conservative_scheme_omits_noisy_classes_but_keeps_structured_values() {
    let text = "Info: 247 updates on 192.168.1.52 failed";
    let matches =
        classify_line_with_scheme(text, SemanticLineRole::Output, SemanticScheme::Conservative)
            .into_iter()
            .map(|span| (&text[span.range], span.class))
            .collect::<Vec<_>>();

    assert!(!matches.contains(&("Info", SemanticClass::Info)));
    assert!(!matches.contains(&("247", SemanticClass::Number)));
    assert!(matches.contains(&("192.168.1.52", SemanticClass::Address)));
    assert!(matches.contains(&("failed", SemanticClass::Error)));
}

#[test]
fn command_role_colors_only_the_leading_command_token() {
    let text = "user@host:~$ sudo apt update --assume-yes";
    let command = classify_line(text, SemanticLineRole::Command);
    let output = classify_line(text, SemanticLineRole::Output);

    assert!(command.iter().any(|span| {
        &text[span.range.clone()] == "sudo" && span.class == SemanticClass::Command
    }));
    assert!(command.iter().any(|span| {
        &text[span.range.clone()] == "--assume-yes" && span.class == SemanticClass::Option
    }));
    assert!(
        !output
            .iter()
            .any(|span| span.class == SemanticClass::Command)
    );
}

#[test]
fn every_span_uses_valid_utf8_boundaries() {
    let text = "连接 10.0.0.1 成功 true";

    for span in classify_line(text, SemanticLineRole::Output) {
        assert!(text.is_char_boundary(span.range.start));
        assert!(text.is_char_boundary(span.range.end));
        assert!(span.range.start < span.range.end);
    }
}
