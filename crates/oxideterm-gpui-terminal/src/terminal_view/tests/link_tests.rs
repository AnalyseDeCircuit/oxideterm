use super::*;

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
    let path_hover = display_link_ranges_with_path_detection(&path, true)
        .into_iter()
        .next()
        .expect("path link");

    for (case, snapshot, hover, paths_enabled, text, color, underline) in [
        ("OSC8 on color", explicit, false, false, "click", None, true),
        (
            "detected URL",
            selection_snapshot("open https://example.com"),
            false,
            false,
            "https://example.com",
            None,
            true,
        ),
        (
            "application foreground",
            suggestion,
            false,
            true,
            "https://example.com",
            Some(0x687078),
            false,
        ),
        (
            "path without hover",
            path.clone(),
            false,
            true,
            "./crates/oxideterm-gpui-app/src/main.rs",
            None,
            false,
        ),
        (
            "hovered path",
            path.clone(),
            true,
            true,
            "./crates/oxideterm-gpui-app/src/main.rs",
            None,
            true,
        ),
        (
            "path disabled after hover",
            path,
            true,
            false,
            "./crates/oxideterm-gpui-app/src/main.rs",
            None,
            false,
        ),
        (
            "row changed after hover",
            selection_snapshot("open ./crates/oxideterm-gpui-app/src/test.rs"),
            true,
            true,
            "./crates/oxideterm-gpui-app/src/test.rs",
            None,
            false,
        ),
        (
            "painted prompt",
            prompt,
            false,
            true,
            "~/Documents/OxideTerm",
            Some(0xffffff),
            false,
        ),
    ] {
        let hovered = hover.then(|| path_hover.clone());
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
        .detect_file_paths_as_links(paths_enabled)
        .layout();
        let run = layout
            .text_runs
            .iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("{case}: missing {text}"));
        assert_eq!(run.style.underline.is_some(), underline, "{case}");
        if let Some(color) = color {
            assert_eq!(run.style.color, rgb(color).into(), "{case}");
        }
    }
}
