use super::*;
use crate::MarkdownOptions;
use oxideterm_theme::default_tokens;

#[test]
fn renders_existing_and_extended_diagram_types() {
    let tokens = default_tokens();
    let opts = MarkdownOptions::from_theme(&tokens);
    for (source, label) in [
        ("flowchart LR\nA[客户端] --> B[服务]", "客户端"),
        (
            "sequenceDiagram\nClient->>Server: Request\nServer-->>Client: Response",
            "Response",
        ),
        ("pie title 状态\n\"完成\" : 8\n\"等待\" : 2", "完成"),
        (
            "gantt\ndateFormat YYYY-MM-DD\nsection Work\nBuild : 2026-01-01, 2d",
            "Build",
        ),
        ("classDiagram\nClient --> Server", "Server"),
        ("stateDiagram-v2\n[*] --> Ready\nReady --> [*]", "Ready"),
        ("erDiagram\nUSER ||--o{ NOTE : owns", "NOTE"),
        (
            "mindmap\n  root((Notes))\n    Tasks\n    Projects",
            "Projects",
        ),
    ] {
        let result = render_mermaid_svg(source, &tokens, &opts).expect(label);
        let svg = std::str::from_utf8(result.image.bytes()).unwrap();
        assert!(svg.contains(label), "missing diagram label: {label}");
        assert!(result.display_width.is_finite() && result.display_width > 0.0);
        assert!(result.display_height.is_finite() && result.display_height > 0.0);
    }
}

#[test]
fn maps_theme_and_transparency_without_sharing_wrong_cache_entries() {
    let mut tokens = default_tokens();
    tokens.ui.text = 0x163254;
    let mut opts = MarkdownOptions::from_theme(&tokens);
    let source = "flowchart LR\nA[Theme] --> B[Preview]";
    let normal = render_mermaid_svg(source, &tokens, &opts).unwrap();
    let svg = std::str::from_utf8(normal.image.bytes()).unwrap();
    assert!(svg.contains("#163254"));
    assert!(svg.contains("fill=\"transparent\""));
    opts.background_surface_active = true;
    let transparent = render_mermaid_svg(source, &tokens, &opts).unwrap();
    assert!(
        std::str::from_utf8(transparent.image.bytes())
            .unwrap()
            .contains("0.65)")
    );
    assert!(!std::sync::Arc::ptr_eq(&normal.image, &transparent.image));
    tokens.ui.text = 0xfedcba;
    let dark = render_mermaid_svg(source, &tokens, &opts).unwrap();
    assert!(
        std::str::from_utf8(dark.image.bytes())
            .unwrap()
            .contains("#fedcba")
    );
}

#[test]
fn rejects_invalid_and_oversized_inputs_and_retains_failures() {
    let tokens = default_tokens();
    let opts = MarkdownOptions::from_theme(&tokens);
    let invalid = MermaidRenderRequest::new("not_a_diagram", &tokens, &opts, 2.0);
    let error = invalid.render().err().expect("invalid syntax must fail");
    assert_eq!(
        invalid.cached().unwrap().err().as_deref(),
        Some(error.as_str())
    );
    let oversized = MermaidRenderRequest::new(&"x".repeat(65 * 1024), &tokens, &opts, 2.0);
    assert_eq!(
        oversized.render().err().as_deref(),
        Some("Mermaid diagram source is too large")
    );
}
