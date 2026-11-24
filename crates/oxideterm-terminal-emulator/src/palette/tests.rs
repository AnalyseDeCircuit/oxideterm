use super::*;

fn solarized_light_palette() -> TerminalPalette {
    let hex = |value: u32| TerminalColor::rgb((value >> 16) as u8, (value >> 8) as u8, value as u8);
    TerminalPalette::new(
        hex(0x657b83),
        hex(0xfdf6e3),
        hex(0x586e75),
        [
            0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ]
        .map(hex),
    )
}

#[test]
fn color_requests_follow_palette_and_runtime_overrides() {
    let light = solarized_light_palette();
    let rgb = |color: Rgb| TerminalColor::rgb(color.r, color.g, color.b);
    for palette in [OXIDETERM_DARK_THEME, light] {
        let overrides = Colors::default();
        for (index, expected) in [
            (1, palette.ansi[1]),
            (256, palette.foreground),
            (257, palette.background),
            (258, palette.cursor),
            (268, palette.dim_foreground),
            (999, TerminalColor::rgb(0, 0, 0)),
        ] {
            assert_eq!(
                rgb(color_for_alacritty_request(index, &palette, &overrides)),
                expected,
                "slot {index} of {palette:?}"
            );
        }
    }

    let mut overrides = Colors::default();
    overrides[257] = Some(Rgb {
        r: 12,
        g: 34,
        b: 56,
    });
    assert_eq!(
        rgb(color_for_alacritty_request(257, &light, &overrides)),
        TerminalColor::rgb(12, 34, 56)
    );
    let (_, bg) = style_colors_for_cell(
        Color::Named(NamedColor::Foreground),
        Color::Named(NamedColor::Background),
        'x',
        TerminalAttrs::default(),
        &light,
        &overrides,
    );
    assert_eq!(bg, TerminalColor::rgb(12, 34, 56));
}

#[test]
fn cell_colors_follow_palette_only_for_palette_sourced_colors() {
    let light = solarized_light_palette();
    let overrides = Colors::default();
    // Decoration glyphs bypass contrast adjustment so the resolved palette slot is visible.
    for (fg, expected) in [
        (
            Color::Named(NamedColor::Red),
            TerminalColor::rgb(0xdc, 0x32, 0x2f),
        ),
        (Color::Indexed(1), TerminalColor::rgb(0xdc, 0x32, 0x2f)),
        (Color::Indexed(196), TerminalColor::rgb(255, 0, 0)),
        (
            Color::Spec(Rgb { r: 1, g: 2, b: 3 }),
            TerminalColor::rgb(1, 2, 3),
        ),
    ] {
        let colors = style_colors_for_cell(
            fg,
            Color::Named(NamedColor::Background),
            '\u{2500}',
            TerminalAttrs::default(),
            &light,
            &overrides,
        );
        assert_eq!(
            colors,
            (expected, TerminalColor::rgb(0xfd, 0xf6, 0xe3)),
            "{fg:?}"
        );
    }
}

#[test]
fn minimum_contrast_uses_resolved_palette_background() {
    let overrides = Colors::default();
    for palette in [OXIDETERM_DARK_THEME, solarized_light_palette()] {
        // ANSI white is nearly invisible on a light background until contrast adjusts it.
        let (fg, bg) = style_colors_for_cell(
            Color::Named(NamedColor::White),
            Color::Named(NamedColor::Background),
            'x',
            TerminalAttrs::default(),
            &palette,
            &overrides,
        );

        assert_eq!(bg, palette.background);
        assert!(
            perceptual_contrast_score(fg, bg).abs() >= DEFAULT_MINIMUM_CONTRAST_SCORE,
            "{fg:?} on {bg:?}"
        );
    }
}

#[test]
fn app_chosen_truecolor_and_256_colors_bypass_contrast_adjustment() {
    // Both colors are too dark for the default background and would otherwise be lifted.
    for (fg, expected) in [
        (
            Color::Spec(Rgb {
                r: 20,
                g: 20,
                b: 20,
            }),
            TerminalColor::rgb(20, 20, 20),
        ),
        (Color::Indexed(233), TerminalColor::rgb(18, 18, 18)),
    ] {
        let (resolved, _) = style_colors_for_cell(
            fg,
            Color::Named(NamedColor::Background),
            'x',
            TerminalAttrs::default(),
            &TerminalPalette::default(),
            &Colors::default(),
        );
        assert_eq!(resolved, expected, "{fg:?}");
    }
}

#[test]
fn decorative_characters_bypass_contrast_adjustment() {
    let (fg, bg) = style_colors_for_cell(
        Color::Named(NamedColor::White),
        Color::Indexed(15),
        '\u{e0b0}',
        TerminalAttrs::default(),
        &TerminalPalette::default(),
        &Colors::default(),
    );

    assert_eq!(fg, OXIDETERM_DARK_THEME.ansi[7]);
    assert_eq!(bg, OXIDETERM_DARK_THEME.ansi[15]);
}
