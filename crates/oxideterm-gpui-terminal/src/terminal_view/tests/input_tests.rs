use super::*;

#[test]
fn app_cursor_navigation_uses_application_sequences() {
    let normal = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "up".to_string(),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );
    assert_eq!(normal.as_deref(), Some("\x1b[A"));

    let app_cursor = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "up".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::APP_CURSOR),
        false,
        KittyKeyEventType::Press,
    );
    assert_eq!(app_cursor.as_deref(), Some("\x1bOA"));
}

#[test]
fn held_navigation_repeats_legacy_arrow_sequences() {
    let normal = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "down".to_string(),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Repeat,
    );
    assert_eq!(normal.as_deref(), Some("\x1b[B"));

    let app_cursor = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "up".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::APP_CURSOR),
        false,
        KittyKeyEventType::Repeat,
    );
    assert_eq!(app_cursor.as_deref(), Some("\x1bOA"));
}

#[test]
fn modified_arrows_include_xterm_modifier_code() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            key: "right".to_string(),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[1;5C"));
}

#[test]
fn plain_printable_keys_are_left_for_gpui_text_input() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "l".to_string(),
            key_char: Some("l".to_string()),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence, None);
}

#[test]
fn plain_tab_emits_tab_character_for_shell_completion() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "tab".to_string(),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\t"));
}

#[test]
fn ctrl_alpha_keys_emit_ascii_control_codes() {
    for byte in b'a'..=b'z' {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: char::from(byte).to_string(),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        )
        .expect("ctrl alpha key should produce a control code");

        assert_eq!(sequence.as_bytes(), &[byte & 0x1f]);
    }

    for byte in b'A'..=b'Z' {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: char::from(byte).to_string(),
                modifiers: Modifiers {
                    shift: true,
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        )
        .expect("ctrl-shift alpha key should produce a control code");

        assert_eq!(sequence.as_bytes(), &[(byte.to_ascii_lowercase()) & 0x1f]);
    }
}

#[test]
fn ctrl_symbol_keys_emit_terminal_control_codes() {
    let cases = [
        ("@", 0x00),
        ("[", 0x1b),
        ("\\", 0x1c),
        ("]", 0x1d),
        ("^", 0x1e),
        ("_", 0x1f),
        ("?", 0x7f),
    ];

    for (key, expected) in cases {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        )
        .expect("ctrl symbol key should produce a control code");

        assert_eq!(sequence.as_bytes(), &[expected]);
    }
}

#[test]
fn function_keys_f1_through_f20_emit_plain_and_modified_sequences() {
    let cases = [
        ("f1", "\x1bOP", "\x1b[1;2P"),
        ("f2", "\x1bOQ", "\x1b[1;2Q"),
        ("f3", "\x1bOR", "\x1b[1;2R"),
        ("f4", "\x1bOS", "\x1b[1;2S"),
        ("f5", "\x1b[15~", "\x1b[15;2~"),
        ("f6", "\x1b[17~", "\x1b[17;2~"),
        ("f7", "\x1b[18~", "\x1b[18;2~"),
        ("f8", "\x1b[19~", "\x1b[19;2~"),
        ("f9", "\x1b[20~", "\x1b[20;2~"),
        ("f10", "\x1b[21~", "\x1b[21;2~"),
        ("f11", "\x1b[23~", "\x1b[23;2~"),
        ("f12", "\x1b[24~", "\x1b[24;2~"),
        ("f13", "\x1b[25~", "\x1b[25;2~"),
        ("f14", "\x1b[26~", "\x1b[26;2~"),
        ("f15", "\x1b[28~", "\x1b[28;2~"),
        ("f16", "\x1b[29~", "\x1b[29;2~"),
        ("f17", "\x1b[31~", "\x1b[31;2~"),
        ("f18", "\x1b[32~", "\x1b[32;2~"),
        ("f19", "\x1b[33~", "\x1b[33;2~"),
        ("f20", "\x1b[34~", "\x1b[34;2~"),
    ];

    for (key, plain, shifted) in cases {
        let plain_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(plain_sequence.as_deref(), Some(plain));

        let shifted_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(shifted_sequence.as_deref(), Some(shifted));
    }
}

#[test]
fn alt_meta_printable_keys_emit_escape_prefixed_ascii_when_enabled() {
    let alt_x = Keystroke {
        key: "x".to_string(),
        key_char: Some("x".to_string()),
        modifiers: Modifiers {
            alt: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let meta_enabled =
        oxideterm_key_escape_sequence(&alt_x, &TermMode::default(), true, KittyKeyEventType::Press);
    assert_eq!(meta_enabled.as_deref(), Some("\x1bx"));

    let meta_disabled = oxideterm_key_escape_sequence(
        &alt_x,
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );
    if cfg!(target_os = "macos") {
        assert_eq!(meta_disabled, None);
    } else {
        assert_eq!(meta_disabled.as_deref(), Some("\x1bx"));
    }

    let alt_shift_x = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "x".to_string(),
            key_char: Some("X".to_string()),
            modifiers: Modifiers {
                alt: true,
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &TermMode::default(),
        true,
        KittyKeyEventType::Press,
    );
    assert_eq!(alt_shift_x.as_deref(), Some("\x1bX"));
}

#[test]
fn app_cursor_navigation_covers_arrows_home_and_end() {
    let cases = [
        ("up", "\x1b[A", "\x1bOA"),
        ("down", "\x1b[B", "\x1bOB"),
        ("right", "\x1b[C", "\x1bOC"),
        ("left", "\x1b[D", "\x1bOD"),
        ("home", "\x1b[H", "\x1bOH"),
        ("end", "\x1b[F", "\x1bOF"),
    ];

    for (key, normal, app_cursor) in cases {
        let normal_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(normal_sequence.as_deref(), Some(normal));

        let app_cursor_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                ..Default::default()
            },
            &(TermMode::default() | TermMode::APP_CURSOR),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(app_cursor_sequence.as_deref(), Some(app_cursor));
    }
}

#[test]
fn kitty_keyboard_reports_modified_printable_keys_as_csi_u() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "l".to_string(),
            key_char: Some("l".to_string()),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &(TermMode::default() | TermMode::DISAMBIGUATE_ESC_CODES),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[108;5u"));
}

#[test]
fn kitty_keyboard_can_report_plain_keys_when_requested() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "enter".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::REPORT_ALL_KEYS_AS_ESC),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[13;1u"));
}

#[test]
fn kitty_keyboard_reports_repeat_and_release_event_types() {
    let mode =
        TermMode::default() | TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_EVENT_TYPES;
    let key = Keystroke {
        key: "a".to_string(),
        key_char: Some("a".to_string()),
        ..Default::default()
    };

    let repeat = oxideterm_key_escape_sequence(&key, &mode, false, KittyKeyEventType::Repeat);
    let release = oxideterm_key_escape_sequence(&key, &mode, false, KittyKeyEventType::Release);

    assert_eq!(repeat.as_deref(), Some("\x1b[97;1:2u"));
    assert_eq!(release.as_deref(), Some("\x1b[97;1:3u"));
}

#[test]
fn kitty_keyboard_reports_function_keys_with_event_types() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "f5".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::REPORT_EVENT_TYPES),
        false,
        KittyKeyEventType::Release,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[15;1:3~"));
}

#[test]
fn bracketed_mouse_reports_use_sgr_coordinates() {
    let report = mouse_button_report(
        TerminalPoint { row: 4, col: 7 },
        MouseButton::Left,
        Modifiers::default(),
        true,
        TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE,
    );

    assert_eq!(report.as_deref(), Some(b"\x1b[<0;8;5M".as_slice()));
}

#[test]
fn middle_and_right_mouse_buttons_use_xterm_button_codes() {
    let mode = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;

    let middle = mouse_button_report(
        TerminalPoint { row: 0, col: 0 },
        MouseButton::Middle,
        Modifiers::default(),
        true,
        mode,
    );
    assert_eq!(middle.as_deref(), Some(b"\x1b[<1;1;1M".as_slice()));

    let right = mouse_button_report(
        TerminalPoint { row: 0, col: 0 },
        MouseButton::Right,
        Modifiers::default(),
        true,
        mode,
    );
    assert_eq!(right.as_deref(), Some(b"\x1b[<2;1;1M".as_slice()));
}

#[test]
fn normal_mouse_reports_are_one_based_and_release_as_button_three() {
    let report = mouse_button_report(
        TerminalPoint { row: 0, col: 0 },
        MouseButton::Left,
        Modifiers::default(),
        false,
        TermMode::MOUSE_REPORT_CLICK,
    );

    assert_eq!(report.as_deref(), Some(b"\x1b[M#!!".as_slice()));
}
