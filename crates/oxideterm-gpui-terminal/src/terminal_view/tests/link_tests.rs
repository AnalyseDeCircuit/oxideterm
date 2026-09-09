use super::*;

#[test]
fn link_detection_finds_urls_and_trims_trailing_punctuation() {
    let snapshot = selection_snapshot("open https://example.com/docs).");
    let links = super::super::links::detect_link_ranges_for_rows_with_path_detection(
        &snapshot,
        0..snapshot.lines.len(),
        true,
    );

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Url);
    assert_eq!(links[0].start_col, 5);
    assert_eq!(links[0].end_col, 29);
    assert_eq!(links[0].target, "https://example.com/docs");
}

#[test]
fn link_detection_finds_path_like_targets() {
    let snapshot = selection_snapshot("see ./crates/oxideterm-gpui-app/src/main.rs");
    let links = super::super::links::detect_link_ranges_for_rows_with_path_detection(
        &snapshot,
        0..snapshot.lines.len(),
        true,
    );

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Path);
    assert_eq!(links[0].target, "./crates/oxideterm-gpui-app/src/main.rs");
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
fn osc8_targets_take_precedence_over_visible_labels_and_detected_urls() {
    for label in ["click", "https://example.com"] {
        let mut snapshot = selection_snapshot(label);
        for cell in &mut snapshot.lines[0].cells_mut()[..label.len()] {
            cell.set_hyperlink(Some("https://example.com/osc8".to_string()));
        }
        snapshot.lines[0].refresh_signature();
        let links = super::super::links::detect_link_ranges_for_rows_with_path_detection(
            &snapshot,
            0..snapshot.lines.len(),
            true,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, TerminalLinkKind::Url);
        assert_eq!(links[0].start_col, 0);
        assert_eq!(links[0].end_col, label.len());
        assert_eq!(links[0].target, "https://example.com/osc8");
    }
}

#[test]
fn terminal_element_underlines_osc8_links_even_on_colored_cells() {
    let mut snapshot = selection_snapshot("click");
    for cell in &mut snapshot.lines[0].cells_mut()[..5] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.set_hyperlink(Some("https://example.com/osc8".to_string()));
    }
    snapshot.lines[0].refresh_signature();

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();

    let link_run = layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("click"))
        .expect("osc8 link run");
    assert!(link_run.style.underline.is_some());
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

#[test]
fn terminal_element_underlines_detected_links() {
    let snapshot = selection_snapshot("open https://example.com");
    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();
    let link_run = layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("https"))
        .expect("link run");

    assert!(link_run.style.underline.is_some());
}

#[test]
fn terminal_element_preserves_explicit_foreground_for_detected_urls() {
    let suggestion = "https://example.com";
    let mut snapshot = selection_snapshot(suggestion);
    let suggestion_color = TerminalColor::rgb(0x68, 0x70, 0x78);
    for cell in snapshot.lines[0]
        .cells_mut()
        .iter_mut()
        .take(suggestion.chars().count())
    {
        cell.fg = suggestion_color;
        cell.style_origin = oxideterm_terminal::TerminalStyleOrigin::new(true, false);
    }
    snapshot.lines[0].active_input = true;
    snapshot.lines[0].refresh_signature();

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();
    let suggestion_run = layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("https"))
        .expect("suggestion URL run");

    assert_eq!(suggestion_run.style.color, rgb(0x687078).into_color());
    assert!(suggestion_run.style.underline.is_none());
}

#[test]
fn terminal_element_underlines_detected_paths_only_while_hovered() {
    let snapshot = selection_snapshot("open ./crates/oxideterm-gpui-app/src/main.rs");
    let hovered_link = display_link_ranges_with_path_detection(&snapshot, true)
        .into_iter()
        .next()
        .expect("detected path");
    let layout = |hovered_link| {
        TerminalElement::new(
            snapshot.clone(),
            None,
            test_metrics(),
            true,
            None,
            None,
            Vec::new(),
            None,
            hovered_link,
            None,
        )
        .layout()
    };

    let unhovered = layout(None);
    let unhovered_path = unhovered
        .text_runs
        .iter()
        .find(|run| run.text.contains("crates"))
        .expect("unhovered path run");
    assert!(unhovered_path.style.underline.is_none());

    let hovered = layout(Some(hovered_link));
    let hovered_path = hovered
        .text_runs
        .iter()
        .find(|run| run.text.contains("crates"))
        .expect("hovered path run");
    assert!(hovered_path.style.underline.is_some());
}

#[test]
fn terminal_element_does_not_recolor_path_like_prompt_segments() {
    let mut snapshot = selection_snapshot("~/Documents/OxideTerm");
    for cell in &mut snapshot.lines[0].cells_mut()[..21] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.fg = TerminalColor::rgb(0xff, 0xff, 0xff);
    }
    snapshot.lines[0].refresh_signature();

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();

    assert!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.text.contains("Documents") || run.text.contains("OxideTerm"))
            .all(|run| {
                run.style.underline.is_none() && run.style.color == rgb(0xffffff).into_color()
            })
    );
}
