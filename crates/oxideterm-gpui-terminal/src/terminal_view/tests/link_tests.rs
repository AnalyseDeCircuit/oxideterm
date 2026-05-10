use super::*;

#[test]
fn link_detection_finds_urls_and_trims_trailing_punctuation() {
    let snapshot = selection_snapshot("open https://example.com/docs).");
    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Url);
    assert_eq!(links[0].start_col, 5);
    assert_eq!(links[0].end_col, 29);
    assert_eq!(links[0].target, "https://example.com/docs");
}

#[test]
fn link_detection_finds_path_like_targets() {
    let snapshot = selection_snapshot("see ./crates/oxideterm-gpui-app/src/main.rs");
    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Path);
    assert_eq!(links[0].target, "./crates/oxideterm-gpui-app/src/main.rs");
}

#[test]
fn link_detection_preserves_unicode_wide_path_segments() {
    let target = "~/Documents/OxideTerm/tauri版本代码/src";
    let snapshot = wide_snapshot(target);
    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Path);
    assert_eq!(links[0].start_col, 0);
    assert_eq!(
        links[0].end_col,
        target.chars().map(display_cell_width).sum::<usize>()
    );
    assert_eq!(links[0].target, target);
}

fn display_cell_width(ch: char) -> usize {
    if matches!(
        ch as u32,
        0x1100..=0x115f
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
    ) {
        2
    } else {
        1
    }
}

#[test]
fn display_links_skip_path_like_text_on_active_input_row() {
    let mut snapshot = selection_snapshot("cd ../");
    snapshot.lines[0].active_input = true;

    let links = detect_link_ranges(&snapshot);
    let display_links = display_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "../");
    assert!(display_links.is_empty());
}

#[test]
fn display_links_skip_path_like_text_on_wrapped_active_input_rows() {
    let mut snapshot = multirow_snapshot(&["echo ./src/", "main.rs"]);
    snapshot.lines[0].active_input = true;
    snapshot.lines[1].active_input = true;

    let links = detect_link_ranges(&snapshot);
    let display_links = display_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert!(display_links.is_empty());
}

#[test]
fn link_detection_prefers_osc8_hyperlink_ranges() {
    let mut snapshot = selection_snapshot("click");
    for cell in &mut snapshot.lines[0].cells[..5] {
        cell.hyperlink = Some("https://example.com/osc8".to_string());
    }

    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Url);
    assert_eq!(links[0].start_col, 0);
    assert_eq!(links[0].end_col, 5);
    assert_eq!(links[0].target, "https://example.com/osc8");
}

#[test]
fn link_detection_does_not_duplicate_url_inside_osc8_range() {
    let mut snapshot = selection_snapshot("https://example.com");
    for cell in &mut snapshot.lines[0].cells[..19] {
        cell.hyperlink = Some("https://example.com/osc8".to_string());
    }

    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "https://example.com/osc8");
}

#[test]
fn terminal_element_underlines_osc8_links_even_on_colored_cells() {
    let mut snapshot = selection_snapshot("click");
    for cell in &mut snapshot.lines[0].cells[..5] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.hyperlink = Some("https://example.com/osc8".to_string());
    }

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
fn path_links_resolve_relative_paths_to_file_urls() {
    let url = path_link_to_file_url("./src/main.rs", Path::new("/tmp/Oxide Term")).expect("url");

    assert_eq!(url, "file:///tmp/Oxide%20Term/./src/main.rs");
}

#[test]
fn path_links_percent_encode_non_url_safe_characters() {
    let encoded = percent_encode_path(Path::new("/tmp/a b/中文.rs"));

    assert_eq!(encoded, "/tmp/a%20b/%E4%B8%AD%E6%96%87.rs");
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
fn terminal_element_does_not_recolor_path_like_prompt_segments() {
    let mut snapshot = selection_snapshot("~/Documents/OxideTerm");
    for cell in &mut snapshot.lines[0].cells[..21] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.fg = TerminalColor::rgb(0xff, 0xff, 0xff);
    }

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
            .all(|run| run.style.underline.is_none() && run.style.color == rgb(0xffffff).into())
    );
}
