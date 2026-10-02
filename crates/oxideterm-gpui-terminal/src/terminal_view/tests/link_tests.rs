use super::*;

#[test]
fn link_detection_preserves_ranges_and_prefers_explicit_targets() {
    for (source, explicit, kind, start, end, target) in [
        (
            "open https://example.com/docs).",
            None,
            TerminalLinkKind::Url,
            5,
            29,
            "https://example.com/docs",
        ),
        (
            "see ./crates/oxideterm-gpui-app/src/main.rs",
            None,
            TerminalLinkKind::Path,
            4,
            43,
            "./crates/oxideterm-gpui-app/src/main.rs",
        ),
        (
            "click",
            Some("https://example.com/osc8"),
            TerminalLinkKind::Url,
            0,
            5,
            "https://example.com/osc8",
        ),
        (
            "https://example.com",
            Some("https://example.com/osc8"),
            TerminalLinkKind::Url,
            0,
            19,
            "https://example.com/osc8",
        ),
    ] {
        let mut snapshot = selection_snapshot(source);
        if let Some(uri) = explicit {
            for cell in &mut snapshot.lines[0].cells_mut()[..source.len()] {
                cell.set_hyperlink(Some(uri.to_string()));
            }
            snapshot.lines[0].refresh_signature();
        }
        let links = super::super::links::detect_link_ranges_for_rows_with_path_detection(
            &snapshot,
            0..snapshot.lines.len(),
            true,
        );
        assert_eq!(
            links
                .iter()
                .map(|link| (
                    link.row,
                    link.kind,
                    link.start_col,
                    link.end_col,
                    link.target.as_ref(),
                ))
                .collect::<Vec<_>>(),
            [(0, kind, start, end, target)],
            "{source}"
        );
    }
}

#[test]
fn disabling_path_detection_preserves_urls_and_osc8_links() {
    let source = "./logs/server.log https://example.com/docs click";
    let osc_start = source.find("click").unwrap();
    let mut snapshot = selection_snapshot(source);
    for cell in &mut snapshot.lines[0].cells_mut()[osc_start..osc_start + "click".len()] {
        cell.set_hyperlink(Some("file:///tmp/report.txt".to_string()));
    }
    snapshot.lines[0].refresh_signature();

    let links = display_link_ranges_with_path_detection(&snapshot, false);

    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|link| link.target == "https://example.com/docs")
    );
    assert!(
        links
            .iter()
            .any(|link| link.target == "file:///tmp/report.txt")
    );
    assert!(!links.iter().any(|link| link.target == "./logs/server.log"));
}

#[test]
fn active_input_paths_are_hidden_while_completed_output_paths_remain_visible() {
    let mut snapshot = multirow_snapshot(&["cd ../", "echo ./src/", "main.rs", "./completed.log"]);
    for row in &mut snapshot.lines[..3] {
        row.active_input = true;
        row.refresh_signature();
    }
    let links = super::super::links::detect_link_ranges_for_rows_with_path_detection(
        &snapshot,
        0..snapshot.lines.len(),
        true,
    );
    assert_eq!(
        links
            .iter()
            .map(|link| (link.row, link.target.as_ref()))
            .collect::<Vec<_>>(),
        vec![(0, "../"), (1, "./src/"), (3, "./completed.log")]
    );
    let displayed = display_link_ranges_with_path_detection(&snapshot, true);
    assert_eq!(
        displayed
            .iter()
            .map(|link| (link.row, link.target.as_ref()))
            .collect::<Vec<_>>(),
        vec![(3, "./completed.log")]
    );
}

#[test]
fn link_styling_respects_explicit_links_application_colors_and_hover() {
    let mut explicit = selection_snapshot("click");
    for cell in &mut explicit.lines[0].cells_mut()[..5] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.set_hyperlink(Some("https://example.com/osc8".to_string()));
    }
    explicit.lines[0].refresh_signature();
    let mut suggestion = selection_snapshot("https://example.com");
    for cell in &mut suggestion.lines[0].cells_mut()[..19] {
        cell.fg = TerminalColor::rgb(0x68, 0x70, 0x78);
        cell.style_origin = oxideterm_terminal::TerminalStyleOrigin::new(true, false);
    }
    suggestion.lines[0].active_input = true;
    suggestion.lines[0].refresh_signature();
    let mut prompt = selection_snapshot("~/Documents/OxideTerm");
    for cell in &mut prompt.lines[0].cells_mut()[..21] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.fg = TerminalColor::rgb(0xff, 0xff, 0xff);
    }
    prompt.lines[0].refresh_signature();
    let path = selection_snapshot("open ./crates/oxideterm-gpui-app/src/main.rs");

    for (case, snapshot, hover, text, color, underline) in [
        ("OSC8 on color", explicit, false, "click", None, true),
        (
            "detected URL",
            selection_snapshot("open https://example.com"),
            false,
            "https://example.com",
            None,
            true,
        ),
        (
            "application foreground",
            suggestion,
            false,
            "https://example.com",
            Some(0x687078),
            false,
        ),
        (
            "path without hover",
            path.clone(),
            false,
            "./crates/oxideterm-gpui-app/src/main.rs",
            None,
            false,
        ),
        (
            "hovered path",
            path,
            true,
            "./crates/oxideterm-gpui-app/src/main.rs",
            None,
            true,
        ),
        (
            "painted prompt",
            prompt,
            false,
            "~/Documents/OxideTerm",
            Some(0xffffff),
            false,
        ),
    ] {
        let hovered = hover.then(|| {
            display_link_ranges_with_path_detection(&snapshot, true)
                .into_iter()
                .next()
                .expect("path link")
        });
        let layout = TerminalElement::new(
            snapshot,
            None,
            test_metrics(),
            true,
            None,
            None,
            Vec::new(),
            None,
            hovered,
            None,
        )
        .layout();
        let run = layout
            .text_runs
            .iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("{case}: missing {text}"));
        assert_eq!(run.style.underline.is_some(), underline, "{case}");
        if let Some(color) = color {
            assert_eq!(run.style.color, rgb(color).into_color(), "{case}");
        }
    }
}

#[test]
fn path_links_resolve_and_percent_encode_file_urls() {
    for (path, expected) in [
        ("./src/main.rs", "file:///tmp/Oxide%20Term/./src/main.rs"),
        (
            "/tmp/a b/中文.rs",
            "file:///tmp/a%20b/%E4%B8%AD%E6%96%87.rs",
        ),
    ] {
        assert_eq!(
            path_link_to_file_url(path, Path::new("/tmp/Oxide Term")).unwrap(),
            expected
        );
    }
}
