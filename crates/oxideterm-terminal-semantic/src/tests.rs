// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    SemanticClass, SemanticLineRole, SemanticScheme, classify_line, classify_line_with_scheme,
    semantic_output_role_for_command,
};
#[cfg(feature = "shell-syntax")]
use crate::{
    SemanticShellDialect, classify_line_with_compiled_scheme_and_shell, compiled_builtin_scheme,
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
    let text = "2026-08-18 15:20:06 host 192.168.1.52 mac 02:3B:4C:5D:6E:7F /var/log/app.log temperature -12";
    let spans = classify_line(text, SemanticLineRole::Output);
    let matches = spans
        .iter()
        .map(|span| (&text[span.range.clone()], span.class))
        .collect::<Vec<_>>();

    assert!(matches.contains(&("2026-08-18 15:20:06", SemanticClass::Timestamp)));
    assert!(matches.contains(&("192.168.1.52", SemanticClass::Address)));
    assert!(matches.contains(&("02:3B:4C:5D:6E:7F", SemanticClass::Address)));
    assert!(matches.contains(&("/var/log/app.log", SemanticClass::Path)));
    assert!(!matches.contains(&("-12", SemanticClass::Option)));
    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start);
    }
}

#[test]
fn ipv6_and_windows_paths_are_classified_as_structured_values() {
    let text =
        r"host 2001:db8::1 loopback ::1 paths C:\Users\alice\app.log \\server\share\report.txt";
    let matches = matched_texts(text);

    assert!(matches.contains(&("2001:db8::1", SemanticClass::Address)));
    assert!(matches.contains(&("::1", SemanticClass::Address)));
    assert!(matches.contains(&(r"C:\Users\alice\app.log", SemanticClass::Path)));
    assert!(matches.contains(&(r"\\server\share\report.txt", SemanticClass::Path)));
}

#[test]
fn process_table_time_formats_are_classified_as_complete_timestamps() {
    let text = "Sun03PM Mon07PM 6:52PM 11:04 PM 16Aug26 6月22 7月03 0:00.82 154:04 2255:55 19:21:57 2026-08-19 19:21:57";
    let matches = matched_texts(text);

    for timestamp in [
        "Sun03PM",
        "Mon07PM",
        "6:52PM",
        "11:04 PM",
        "16Aug26",
        "6月22",
        "7月03",
        "0:00.82",
        "154:04",
        "2255:55",
        "19:21:57",
        "2026-08-19 19:21:57",
    ] {
        assert!(
            matches.contains(&(timestamp, SemanticClass::Timestamp)),
            "missing timestamp {timestamp:?} in {matches:?}"
        );
    }
}

#[test]
fn option_and_variable_assignments_keep_separate_semantic_parts() {
    let text = "grep --color=auto NODE_ENV=production rw,fsname=portal,subtype=fuse";
    let matches = matched_texts(text);

    for expected in [
        ("--color", SemanticClass::Option),
        ("NODE_ENV", SemanticClass::Variable),
        ("fsname", SemanticClass::Variable),
        ("subtype", SemanticClass::Variable),
        ("=", SemanticClass::Operator),
        ("auto", SemanticClass::String),
        ("production", SemanticClass::String),
        ("portal", SemanticClass::String),
        ("fuse", SemanticClass::String),
    ] {
        assert!(
            matches.contains(&expected),
            "missing {expected:?} in {matches:?}"
        );
    }
}

#[test]
fn ps_output_roles_classify_structured_columns_without_global_sentinels() {
    assert_eq!(
        semantic_output_role_for_command("ps aux | grep node"),
        SemanticLineRole::PsAuxOutput
    );
    assert_eq!(
        semantic_output_role_for_command("sudo /bin/ps -ef"),
        SemanticLineRole::PsFullOutput
    );

    let text = "lips 1172515 0.0 0.1 1159056 23396 ? Ssl 6月22 154:04 node /usr/local/bin/pnpm --filter=server NODE_ENV=production";
    let matches = classify_line(text, SemanticLineRole::PsAuxOutput)
        .into_iter()
        .map(|span| (&text[span.range], span.class))
        .collect::<Vec<_>>();

    for expected in [
        ("1172515", SemanticClass::Number),
        ("?", SemanticClass::Info),
        ("Ssl", SemanticClass::Info),
        ("6月22", SemanticClass::Timestamp),
        ("154:04", SemanticClass::Timestamp),
        ("--filter", SemanticClass::Option),
        ("NODE_ENV", SemanticClass::Variable),
    ] {
        assert!(
            matches.contains(&expected),
            "missing {expected:?} in {matches:?}"
        );
    }

    assert!(
        !matched_texts("question ? remains generic")
            .iter()
            .any(|match_| *match_ == ("?", SemanticClass::Info))
    );

    let full_text = "root 717098 1 0 2025 ? 0:00 fuser -o rw,nosuid";
    let full_matches = classify_line(full_text, SemanticLineRole::PsFullOutput)
        .into_iter()
        .map(|span| (&full_text[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(full_matches.contains(&("2025", SemanticClass::Timestamp)));
    assert!(full_matches.contains(&("?", SemanticClass::Info)));
    assert!(full_matches.contains(&("0:00", SemanticClass::Timestamp)));

    let conservative = classify_line_with_scheme(
        text,
        SemanticLineRole::PsAuxOutput,
        SemanticScheme::Conservative,
    );
    assert!(
        conservative
            .iter()
            .all(|span| { !matches!(span.class, SemanticClass::Number | SemanticClass::Info) })
    );
}

#[cfg(feature = "shell-syntax")]
#[test]
fn ps_command_column_uses_lightweight_semantic_classification() {
    let text = "lips 1172515 0.0 0.1 1159056 23396 ? Ssl 6月22 0:00 node --filter ./apps/server";
    let matches = classify_line_with_compiled_scheme_and_shell(
        text,
        SemanticLineRole::PsAuxOutput,
        compiled_builtin_scheme(SemanticScheme::Balanced),
        SemanticShellDialect::Bash,
    )
    .into_iter()
    .map(|span| (&text[span.range], span.class))
    .collect::<Vec<_>>();

    assert!(matches.contains(&("node", SemanticClass::Command)));
    assert!(matches.contains(&("--filter", SemanticClass::Option)));
}

#[test]
fn nested_ascii_and_unicode_bracket_pairs_are_classified() {
    let text = "outer(({[<value>]})) 【《（内容）》】 ⟦⌈item⌉⟧";
    let spans = classify_line(text, SemanticLineRole::Output);
    let brackets = spans
        .iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &text[span.range.clone()])
        .collect::<String>();

    assert_eq!(brackets, "(({[<>]}))【《（）》】⟦⌈⌉⟧");
    let ascii_variants = spans
        .iter()
        .filter(|span| span.class == SemanticClass::Operator && span.range.start < 20)
        .map(|span| span.style_variant)
        .collect::<Vec<_>>();
    assert_eq!(
        ascii_variants,
        vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(4),
            Some(3),
            Some(2),
            Some(1),
            Some(0),
        ]
    );
}

#[cfg(feature = "shell-syntax")]
#[test]
fn nested_command_brackets_keep_depth_variants_after_shell_parsing() {
    let text = "if ((value + (nested * 2))); then echo ok; fi";
    let spans = classify_line_with_compiled_scheme_and_shell(
        text,
        SemanticLineRole::Command,
        compiled_builtin_scheme(SemanticScheme::Balanced),
        SemanticShellDialect::Bash,
    );
    let variants = spans
        .iter()
        .filter(|span| {
            span.class == SemanticClass::Operator && matches!(&text[span.range.clone()], "(" | ")")
        })
        .map(|span| span.style_variant)
        .collect::<Vec<_>>();

    assert_eq!(
        variants,
        vec![Some(0), Some(1), Some(2), Some(2), Some(1), Some(0),],
        "classified spans: {:?}",
        spans
            .iter()
            .map(|span| (&text[span.range.clone()], span.class, span.style_variant))
            .collect::<Vec<_>>()
    );
}

#[test]
fn bracket_pair_classification_ignores_quotes_escapes_and_mismatches() {
    let text = r#"don't "(ignored)" escaped \[value\] valid(ok) broken([)]"#;
    let brackets = classify_line(text, SemanticLineRole::Output)
        .into_iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &text[span.range])
        .collect::<String>();

    assert_eq!(brackets, "()");

    let option = "--filter=<value>";
    let spans = classify_line(option, SemanticLineRole::Output);
    assert!(spans.iter().any(|span| {
        &option[span.range.clone()] == "--filter" && span.class == SemanticClass::Option
    }));
    let operators = spans
        .iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &option[span.range.clone()])
        .collect::<Vec<_>>();
    assert_eq!(operators, vec!["="]);
}

#[test]
fn standalone_output_markers_are_operators_without_reclassifying_negative_numbers() {
    let text = "* item - divider | pipe = value -- boundary temperature -12";
    let operators = classify_line(text, SemanticLineRole::Output)
        .into_iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &text[span.range])
        .collect::<Vec<_>>();

    assert_eq!(operators, vec!["*", "-", "|", "=", "--"]);
    assert!(!operators.contains(&"-12"));
}

#[test]
fn neutral_modal_and_negation_phrases_are_not_statuses() {
    let text =
        "Users can review changes; not every section has notes; no single format is required.";
    let matches = matched_texts(text);

    assert!(
        matches
            .iter()
            .all(|(_, class)| !matches!(class, SemanticClass::Error | SemanticClass::Success))
    );
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

#[test]
fn multilingual_status_terms_receive_the_same_semantic_classes() {
    let cases = [
        ("连接失败", "失败", SemanticClass::Error),
        ("操作成功", "成功", SemanticClass::Success),
        ("警告：空间不足", "警告", SemanticClass::Warning),
        ("Échec de connexion", "Échec", SemanticClass::Error),
        ("Vorgang erfolgreich", "erfolgreich", SemanticClass::Success),
        ("작업 완료", "완료", SemanticClass::Success),
        ("Cảnh báo dung lượng", "Cảnh báo", SemanticClass::Warning),
    ];

    for (text, expected_text, expected_class) in cases {
        let matches = classify_line(text, SemanticLineRole::Output)
            .into_iter()
            .map(|span| (&text[span.range], span.class))
            .collect::<Vec<_>>();
        assert!(
            matches.contains(&(expected_text, expected_class)),
            "missing {expected_text:?} in {matches:?}"
        );
    }
}
