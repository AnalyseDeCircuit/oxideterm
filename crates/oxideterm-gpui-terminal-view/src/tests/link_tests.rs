use super::snapshot::*;
use crate::links::*;

fn is_archive_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".tar.gz")
}

#[test]
fn link_ranges_preserve_wide_combining_and_quoted_archive_columns() {
    let mut snapshot = selection_snapshot("");
    snapshot.lines = vec![row_from_text_with_wide_spacers(
        "e 中文 https://example.com/文). '报告 备份.TAR.GZ' noise",
    )];
    snapshot.cols = snapshot.lines[0].cells.len();
    snapshot.lines[0].cells_mut()[0].set_zerowidth("\u{301}".into());
    snapshot.lines[0].refresh_signature();
    let links =
        display_link_ranges_for_rows(&snapshot, 0..snapshot.lines.len(), Some(is_archive_path));
    assert_eq!(
        links
            .iter()
            .map(|link| (
                link.start_col,
                link.end_col,
                link.kind,
                link.target.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (7, 29, TerminalLinkKind::Url, "https://example.com/文"),
            (32, 50, TerminalLinkKind::Path, "'报告 备份.TAR.GZ'"),
        ]
    );
}

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
        let links =
            detect_link_ranges_for_rows(&snapshot, 0..snapshot.lines.len(), Some(is_archive_path));
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

    let links = display_link_ranges_for_rows(&snapshot, 0..snapshot.lines.len(), None);

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
    let links =
        detect_link_ranges_for_rows(&snapshot, 0..snapshot.lines.len(), Some(is_archive_path));
    assert_eq!(
        links
            .iter()
            .map(|link| (link.row, link.target.as_ref()))
            .collect::<Vec<_>>(),
        vec![(0, "../"), (1, "./src/"), (3, "./completed.log")]
    );
    let displayed =
        display_link_ranges_for_rows(&snapshot, 0..snapshot.lines.len(), Some(is_archive_path));
    assert_eq!(
        displayed
            .iter()
            .map(|link| (link.row, link.target.as_ref()))
            .collect::<Vec<_>>(),
        vec![(3, "./completed.log")]
    );
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn path_links_resolve_and_percent_encode_file_urls() {
    use std::path::Path;

    #[cfg(windows)]
    let (base_dir, cases) = (
        Path::new(r"C:\tmp\Oxide Term"),
        vec![
            ("./src/main.rs", "file:///C:/tmp/Oxide%20Term/src/main.rs"),
            (r".\src\main.rs", "file:///C:/tmp/Oxide%20Term/src/main.rs"),
            (r"D:\a b\中文.rs", "file:///D:/a%20b/%E4%B8%AD%E6%96%87.rs"),
            ("/tmp/report.txt", "file:///C:/tmp/report.txt"),
            (
                r"\\server\share\a b\中文.rs",
                "file://server/share/a%20b/%E4%B8%AD%E6%96%87.rs",
            ),
            (r"\\?\C:\tmp\report.txt", "file:///C:/tmp/report.txt"),
            (
                "'./100%#report.txt'",
                "file:///C:/tmp/Oxide%20Term/100%25%23report.txt",
            ),
        ],
    );
    #[cfg(not(windows))]
    let (base_dir, cases) = (
        Path::new("/tmp/Oxide Term"),
        vec![
            ("./src/main.rs", "file:///tmp/Oxide%20Term/src/main.rs"),
            (
                "/tmp/a b/中文.rs",
                "file:///tmp/a%20b/%E4%B8%AD%E6%96%87.rs",
            ),
            (
                "'./100%#report.txt'",
                "file:///tmp/Oxide%20Term/100%25%23report.txt",
            ),
            (r"./a\b.txt", "file:///tmp/Oxide%20Term/a%5Cb.txt"),
        ],
    );
    for (path, expected) in cases {
        assert_eq!(
            path_link_to_file_url(path, base_dir).as_deref(),
            Some(expected),
            "{path}"
        );
    }
    assert_eq!(
        path_link_to_file_url("./src/main.rs", Path::new("relative")),
        None
    );
}
