use super::*;
use oxideterm_settings::{TerminalBackspaceSequence, TerminalDeleteSequence};

#[test]
fn win32_input_preserves_control_key_identity_and_enter_modifiers() {
    let mode = TermMode::default() | TermMode::WIN32_INPUT;
    for (key, control, shift, alt, expected) in [
        ("j", true, false, false, "\x1b[74;0;10;1;8;1_"),
        ("i", true, false, false, "\x1b[73;0;9;1;8;1_"),
        ("m", true, false, false, "\x1b[77;0;13;1;8;1_"),
        ("h", true, false, false, "\x1b[72;0;8;1;8;1_"),
        ("c", true, false, false, "\x1b[67;0;3;1;8;1_"),
        ("enter", false, false, false, "\x1b[13;0;13;1;0;1_"),
        ("enter", true, false, false, "\x1b[13;0;10;1;8;1_"),
        ("enter", false, true, false, "\x1b[13;0;13;1;16;1_"),
        ("enter", false, false, true, "\x1b[13;0;13;1;2;1_"),
        ("space", true, false, false, "\x1b[32;0;0;1;8;1_"),
        ("left", true, false, false, "\x1b[37;0;0;1;8;1_"),
    ] {
        let keystroke = Keystroke {
            key: key.into(),
            modifiers: Modifiers {
                control,
                shift,
                alt,
                ..Default::default()
            },
            ..Default::default()
        };
        for event in [KittyKeyEventType::Press, KittyKeyEventType::Repeat] {
            assert_eq!(
                oxideterm_key_escape_sequence(&keystroke, &mode, false, event).as_deref(),
                Some(expected),
                "{key}"
            );
        }
        assert_eq!(
            oxideterm_key_escape_sequence(&keystroke, &mode, false, KittyKeyEventType::Release)
                .as_deref(),
            Some(expected.replace(";1;", ";0;").as_str()),
            "{key} release",
        );
    }
    let ctrl_j = Keystroke::parse("ctrl-j").unwrap();
    assert_eq!(
        oxideterm_key_escape_sequence(
            &ctrl_j,
            &TermMode::default(),
            false,
            KittyKeyEventType::Press
        )
        .as_deref(),
        Some("\n")
    );
    assert_eq!(
        oxideterm_key_escape_sequence(
            &ctrl_j,
            &(mode | TermMode::DISAMBIGUATE_ESC_CODES),
            false,
            KittyKeyEventType::Press
        )
        .as_deref(),
        Some("\x1b[74;0;10;1;8;1_")
    );
}

#[test]
fn win32_input_leaves_printable_and_ime_text_on_the_platform_commit_path() {
    for (key, key_char, modifiers) in [
        ("a", "a", Modifiers::default()),
        (
            "a",
            "A",
            Modifiers {
                shift: true,
                ..Default::default()
            },
        ),
        ("中", "中", Modifiers::default()),
        (
            "q",
            "@",
            Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
        ),
    ] {
        let keystroke = Keystroke {
            key: key.into(),
            key_char: Some(key_char.into()),
            modifiers,
        };
        assert_eq!(
            oxideterm_key_escape_sequence(
                &keystroke,
                &(TermMode::default() | TermMode::WIN32_INPUT),
                false,
                KittyKeyEventType::Press
            ),
            None
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "requires ConPTY staged beside the test executable; see local-development.md"]
fn bundled_conpty_delivers_distinct_ctrl_j_and_enter_events() {
    use crate::TerminalPane;
    use oxideterm_terminal::{LocalPtyConfig, ShellInfo};
    use std::{
        fs,
        time::{Duration, Instant},
    };

    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("input.ps1");
    let ready = directory.path().join("ready");
    let result = directory.path().join("result.json");
    fs::write(
        &script,
        r#"
param($Ready, $Result)
$ErrorActionPreference = 'Stop'
[Console]::TreatControlCAsInput = $true
[IO.File]::WriteAllText($Ready, 'ready')
$events = @(for ($i = 0; $i -lt 7; $i++) {
    $key = [Console]::ReadKey($true)
    @{ key = [int]$key.Key; character = [int]$key.KeyChar; modifiers = $key.Modifiers.ToString() }
})
[IO.File]::WriteAllText($Result, (ConvertTo-Json -InputObject $events -Compress))
"#,
    )
    .unwrap();
    let powershell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let config = LocalPtyConfig {
        shell: Some(
            ShellInfo::new("input-probe", "Input probe", powershell).with_args(vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                script.to_string_lossy().into_owned(),
                ready.to_string_lossy().into_owned(),
                result.to_string_lossy().into_owned(),
            ]),
        ),
        ..Default::default()
    };
    let terminal =
        TerminalPane::local_shared_session(config, &TerminalUiPreferences::default()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        terminal.lock().read_pending();
        if ready.exists() && terminal.lock().mode().contains(TermMode::WIN32_INPUT) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ready.exists(), "console reader did not start");
    let mode = terminal.lock().mode();
    assert!(
        mode.contains(TermMode::WIN32_INPUT),
        "bundled ConPTY did not negotiate mode 9001"
    );
    for key in ["ctrl-j", "ctrl-enter", "shift-enter", "enter", "ctrl-i"] {
        for event in [KittyKeyEventType::Press, KittyKeyEventType::Release] {
            let sequence =
                oxideterm_key_escape_sequence(&Keystroke::parse(key).unwrap(), &mode, false, event)
                    .unwrap();
            terminal
                .lock()
                .write_protocol_bytes(sequence.as_bytes())
                .unwrap();
        }
    }
    terminal
        .lock()
        .write_protocol_bytes("中文".as_bytes())
        .unwrap();
    while Instant::now() < deadline && !result.exists() {
        terminal.lock().read_pending();
        std::thread::sleep(Duration::from_millis(10));
    }
    terminal.lock().shutdown();
    let events: serde_json::Value =
        serde_json::from_slice(&fs::read(result).expect("console reader result")).unwrap();
    assert_eq!(
        events,
        serde_json::json!([
            {"key": 74, "character": 10, "modifiers": "Control"},
            {"key": 13, "character": 10, "modifiers": "Control"},
            {"key": 13, "character": 13, "modifiers": "Shift"},
        {"key": 13, "character": 13, "modifiers": "0"},
            {"key": 73, "character": 9, "modifiers": "Control"},
        {"key": 0, "character": 20013, "modifiers": "0"},
        {"key": 0, "character": 25991, "modifiers": "0"},
        ])
    );
}

#[test]
fn legacy_navigation_emits_normal_application_and_modified_sequences() {
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

    let cases = [
        ("up", "\x1b[A", "\x1bOA"),
        ("down", "\x1b[B", "\x1bOB"),
        ("right", "\x1b[C", "\x1bOC"),
        ("left", "\x1b[D", "\x1bOD"),
        ("home", "\x1b[H", "\x1bOH"),
        ("end", "\x1b[F", "\x1bOF"),
    ];

    for (key, normal, app_cursor) in cases {
        let keystroke = Keystroke {
            key: key.to_string(),
            ..Default::default()
        };
        for event_type in [KittyKeyEventType::Press, KittyKeyEventType::Repeat] {
            let normal_sequence =
                oxideterm_key_escape_sequence(&keystroke, &TermMode::default(), false, event_type);
            assert_eq!(
                normal_sequence.as_deref(),
                Some(normal),
                "{key} {event_type:?}"
            );

            let app_cursor_sequence = oxideterm_key_escape_sequence(
                &keystroke,
                &(TermMode::default() | TermMode::APP_CURSOR),
                false,
                event_type,
            );
            assert_eq!(
                app_cursor_sequence.as_deref(),
                Some(app_cursor),
                "{key} {event_type:?}"
            );
        }
    }
}

#[test]
fn plain_text_input_and_tab_follow_separate_paths() {
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
fn legacy_backspace_and_delete_sequences_are_configurable() {
    let key = Keystroke {
        key: "backspace".to_string(),
        ..Default::default()
    };

    let delete = configurable_key_escape_sequence(
        &key,
        &TermMode::default(),
        false,
        TerminalBackspaceSequence::Delete,
        TerminalDeleteSequence::Csi3Tilde,
        KittyKeyEventType::Press,
    );
    let control_h = configurable_key_escape_sequence(
        &key,
        &TermMode::default(),
        false,
        TerminalBackspaceSequence::ControlH,
        TerminalDeleteSequence::Csi3Tilde,
        KittyKeyEventType::Press,
    );

    assert_eq!(delete.as_deref(), Some("\x7f"));
    assert_eq!(control_h.as_deref(), Some("\x08"));

    let key = Keystroke {
        key: "delete".to_string(),
        ..Default::default()
    };

    for (sequence, expected) in [
        (TerminalDeleteSequence::Csi3Tilde, "\x1b[3~"),
        (TerminalDeleteSequence::Delete, "\x7f"),
        (TerminalDeleteSequence::ControlH, "\x08"),
    ] {
        let encoded = configurable_key_escape_sequence(
            &key,
            &TermMode::default(),
            false,
            TerminalBackspaceSequence::Delete,
            sequence,
            KittyKeyEventType::Press,
        );

        assert_eq!(encoded.as_deref(), Some(expected));
    }
}

#[test]
fn kitty_keyboard_protocol_encodes_modes_modifiers_and_event_types() {
    let sequence = configurable_key_escape_sequence(
        &Keystroke {
            key: "backspace".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::REPORT_ALL_KEYS_AS_ESC),
        false,
        TerminalBackspaceSequence::ControlH,
        TerminalDeleteSequence::ControlH,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[127;1u"));

    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "l".to_string(),
            key_char: Some("l".to_string()),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        },
        &(TermMode::default() | TermMode::DISAMBIGUATE_ESC_CODES),
        false,
        KittyKeyEventType::Press,
    );
    assert_eq!(sequence.as_deref(), Some("\x1b[108;5u"));

    let mode = TermMode::default()
        | TermMode::DISAMBIGUATE_ESC_CODES
        | TermMode::REPORT_EVENT_TYPES
        | TermMode::REPORT_ALTERNATE_KEYS;
    for (key, shifted_text) in [("a", "A"), (";", ":")] {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                key_char: Some(shifted_text.to_string()),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
            &mode,
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(sequence, None);
    }

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
fn ctrl_keys_emit_terminal_control_codes() {
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
        },
        &TermMode::default(),
        true,
        KittyKeyEventType::Press,
    );
    assert_eq!(alt_shift_x.as_deref(), Some("\x1bX"));
}

#[test]
fn mouse_reports_preserve_protocol_coordinates_buttons_and_release_encoding() {
    let sgr_mode = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
    let cases = [
        (
            TerminalPoint { row: 4, col: 7 },
            MouseButton::Left,
            true,
            sgr_mode,
            b"\x1b[<0;8;5M".as_slice(),
        ),
        (
            TerminalPoint { row: 0, col: 0 },
            MouseButton::Middle,
            true,
            sgr_mode,
            b"\x1b[<1;1;1M".as_slice(),
        ),
        (
            TerminalPoint { row: 0, col: 0 },
            MouseButton::Right,
            true,
            sgr_mode,
            b"\x1b[<2;1;1M".as_slice(),
        ),
        (
            TerminalPoint { row: 0, col: 0 },
            MouseButton::Left,
            false,
            TermMode::MOUSE_REPORT_CLICK,
            b"\x1b[M#!!".as_slice(),
        ),
    ];

    for (point, button, pressed, mode, expected) in cases {
        let report = mouse_button_report(point, button, Modifiers::default(), pressed, mode);
        assert_eq!(report.as_deref(), Some(expected));
    }
}
