mod tests {
    use super::*;
    use std::path::PathBuf;

    #[cfg(unix)]
    use std::time::Duration;

    use alacritty_terminal::{
        event::VoidListener,
        term::Config,
        vte::ansi::{Color, NamedColor, Processor, Rgb, StdSyncHandler},
    };
    use oxideterm_terminal_graphics::{GraphicsIngress, TerminalGraphicsSegment};

    use crate::{
        color::{
            DEFAULT_MINIMUM_CONTRAST_SCORE, OXIDETERM_DARK_THEME,
            color_for_alacritty_request_with_override, indexed_color_to_rgb,
            perceptual_contrast_score, style_colors_for_cell,
        },
        process::{parse_lsof_cwd, parse_process_table_for_group},
        search::search_line_matches,
    };

    #[test]
    fn focus_reports_are_gated_by_terminal_mode() {
        assert_eq!(focus_report_sequence(false, true), None);
        assert_eq!(focus_report_sequence(false, false), None);
        assert_eq!(
            focus_report_sequence(true, true),
            Some(b"\x1b[I".as_slice())
        );
        assert_eq!(
            focus_report_sequence(true, false),
            Some(b"\x1b[O".as_slice())
        );
    }

    #[test]
    fn lifecycle_reports_running_state() {
        assert!(TerminalLifecycle::Running.is_running());
        assert!(!TerminalLifecycle::Exited(Some(0)).is_running());
        assert!(!TerminalLifecycle::Closed.is_running());
    }

    #[test]
    fn terminal_resize_request_clamps_to_minimum_grid() {
        let resize = TerminalResize::new(0, 1, 12, 24);

        assert_eq!(resize.cols, 2);
        assert_eq!(resize.rows, 2);
        assert_eq!(resize.cell_width, 12);
        assert_eq!(resize.cell_height, 24);
    }

    #[test]
    fn ssh_session_config_preserves_connection_identity() {
        let config = SshSessionConfig::new("example.com", 2222, "alice");

        assert_eq!(config.host(), "example.com");
        assert_eq!(config.port(), 2222);
        assert_eq!(config.username(), "alice");
        assert!(!config.defer_pty_until_resize());
        assert!(config.with_deferred_pty(true).defer_pty_until_resize());
    }

    #[test]
    fn ssh_terminal_is_not_interactive_until_shell_channel_is_ready() {
        let session = SshPtySession::new(
            SshSessionConfig::new("127.0.0.1", 9, "nobody"),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            1000,
        );

        assert!(session.lifecycle().is_running());
        assert!(!session.is_interactive());
    }

    #[test]
    fn ssh_resize_resets_command_mark_coordinates_only_when_grid_changes() {
        let mut session = SshPtySession::new(
            SshSessionConfig::new("127.0.0.1", 9, "nobody"),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            1000,
        );

        session
            .resize_with_cell_size(TerminalResize::new(80, 24, 8, 16))
            .expect("cell-only resize should succeed");
        assert!(!session.take_events().iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Reset)
        )));

        session
            .resize_with_cell_size(TerminalResize::new(100, 24, 8, 16))
            .expect("grid resize should succeed");
        assert!(session.take_events().iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Reset)
        )));
    }

    #[test]
    fn process_group_parser_ignores_zombies_and_picks_latest_pid() {
        let ps_output = "\
          100   42 S\n\
          101   42 Z\n\
          205   99 S\n\
          103   42 S+\n";

        assert_eq!(parse_process_table_for_group(ps_output, 42), Some(103));
        assert_eq!(parse_process_table_for_group(ps_output, 123), None);
    }

    #[test]
    fn lsof_cwd_parser_reads_name_record() {
        let lsof_output = "p12345\nn/Users/dominical/Documents/OxideTerm\n";
        assert_eq!(
            parse_lsof_cwd(lsof_output),
            Some(PathBuf::from("/Users/dominical/Documents/OxideTerm"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_pty_shutdown_cleans_background_child_processes() {
        let marker_path = std::env::temp_dir().join(format!(
            "oxideterm-pty-child-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let script = r#"
marker=$1
( trap "" TERM; while :; do sleep 5; done ) &
child=$!
printf '%s\n' "$child" > "$marker"
wait
"#;
        let mut config = LocalPtyConfig::default();
        config.shell = Some(
            ShellInfo::new("test-sh", "Test sh", "/bin/sh").with_args(vec![
                "-c".to_string(),
                script.to_string(),
                "oxideterm-pty-test".to_string(),
                marker_path.display().to_string(),
            ]),
        );
        config.load_profile = false;

        let mut session = LocalPtySession::spawn_with_config_graphics_and_encoding(
            80,
            24,
            config,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        )
        .expect("spawn local PTY");

        let child_pid = wait_for_child_pid(&marker_path);
        assert!(
            unix_process_is_running(child_pid),
            "test child should be running before PTY shutdown"
        );

        session.shutdown();

        assert_eventually(
            Duration::from_secs(3),
            || !unix_process_is_running(child_pid),
            "background child should stop after PTY shutdown",
        );
        let _ = std::fs::remove_file(marker_path);
    }

    #[cfg(unix)]
    #[test]
    fn local_available_shell_integrations_report_initial_cwd() {
        let expected_cwd = std::env::temp_dir();
        for shell_id in ["bash", "zsh", "fish", "pwsh"] {
            let Some(shell_path) = find_test_executable(shell_id) else {
                continue;
            };
            assert_local_shell_reports_initial_cwd(shell_id, shell_path, &expected_cwd);
        }
    }

    #[cfg(unix)]
    #[test]
    fn integrated_zsh_loads_history_from_user_config_in_a_real_pty() {
        let Some(shell_path) = find_test_executable("zsh") else {
            return;
        };
        let user_config = tempfile::tempdir().expect("temporary user Zsh directory");
        std::fs::write(
            user_config.path().join(".zshrc"),
            "HISTFILE=\"$ZDOTDIR/.zsh_history\"\nHISTSIZE=100\nSAVEHIST=100\nsetopt share_history\n",
        )
        .expect("write user Zsh config");
        std::fs::write(
            user_config.path().join(".zsh_history"),
            "oxideterm-history-probe\n",
        )
        .expect("write user Zsh history");
        let config = LocalPtyConfig {
            shell: Some(ShellInfo::new("zsh", "Zsh", shell_path)),
            env: HashMap::from([(
                "ZDOTDIR".to_string(),
                user_config.path().display().to_string(),
            )]),
            current_directory_shell_integration: true,
            ..LocalPtyConfig::default()
        };
        let mut session = LocalPtySession::spawn_with_config_graphics_and_encoding(
            80,
            24,
            config,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        )
        .expect("spawn integrated Zsh PTY");

        std::thread::sleep(Duration::from_secs(1));
        session.drain_output();
        session.take_events();
        session
            .write_text("print -r -- OXIDETERM_HISTORY_COUNT=${#history[@]}\n")
            .expect("query Zsh history count");

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut screen = String::new();
        let parse_history_count = |output: &str| {
            output.rsplit("OXIDETERM_HISTORY_COUNT=").find_map(|value| {
                let digits = value
                    .trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>();
                (!digits.is_empty())
                    .then(|| digits.parse::<usize>().ok())
                    .flatten()
            })
        };
        while std::time::Instant::now() < deadline && parse_history_count(&screen).is_none() {
            session.drain_output();
            screen = session
                .snapshot()
                .lines
                .iter()
                .map(|row| row.cells.iter().map(|cell| cell.ch).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            std::thread::sleep(Duration::from_millis(10));
        }
        session.shutdown();

        let count = parse_history_count(&screen).expect("history count response");
        assert!(
            count > 0,
            "integrated Zsh PTY did not load configured history"
        );
    }

    #[cfg(unix)]
    fn assert_local_shell_reports_initial_cwd(
        shell_id: &str,
        shell_path: PathBuf,
        expected_cwd: &std::path::Path,
    ) {
        let config = LocalPtyConfig {
            shell: Some(ShellInfo::new(shell_id, shell_id, shell_path)),
            cwd: Some(expected_cwd.to_path_buf()),
            load_profile: false,
            current_directory_shell_integration: true,
            ..LocalPtyConfig::default()
        };
        let mut session = LocalPtySession::spawn_with_config_graphics_and_encoding(
            80,
            24,
            config,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        )
        .unwrap_or_else(|error| panic!("spawn integrated local {shell_id} PTY: {error}"));
        assert_eq!(
            session.shell_integration_launch_state(),
            TerminalCwdIntegrationLaunchState::Prepared
        );

        let deadline = std::time::Instant::now() + local_shell_cwd_report_timeout(shell_id);
        let mut reported_cwd = None;
        while std::time::Instant::now() < deadline && reported_cwd.is_none() {
            session.drain_output();
            reported_cwd = session
                .take_events()
                .into_iter()
                .find_map(|event| match event {
                    TerminalEvent::CwdChanged { cwd, .. } => Some(cwd),
                    _ => None,
                });
            std::thread::sleep(Duration::from_millis(10));
        }
        session.shutdown();

        assert_eq!(
            reported_cwd.map(PathBuf::from),
            Some(expected_cwd.canonicalize().unwrap()),
            "{shell_id} did not report its initial cwd"
        );
    }

    #[cfg(unix)]
    fn local_shell_cwd_report_timeout(shell_id: &str) -> Duration {
        // PowerShell's managed runtime can take longer to start on a contended CI runner.
        if shell_id == "pwsh" {
            Duration::from_secs(15)
        } else {
            Duration::from_secs(5)
        }
    }

    #[cfg(unix)]
    fn find_test_executable(name: &str) -> Option<PathBuf> {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn search_line_matches_reports_terminal_range_columns() {
        let matches = search_line_matches(-3, "cargo test cargo", "cargo", 80);

        assert_eq!(
            matches,
            vec![
                TerminalSearchMatch {
                    line: -3,
                    start_col: 0,
                    end_col: 5,
                    ranges: vec![TerminalSearchRange {
                        line: -3,
                        start_col: 0,
                        end_col: 5,
                    }],
                },
                TerminalSearchMatch {
                    line: -3,
                    start_col: 11,
                    end_col: 16,
                    ranges: vec![TerminalSearchRange {
                        line: -3,
                        start_col: 11,
                        end_col: 16,
                    }],
                },
            ]
        );
    }

    #[test]
    fn search_line_matches_clips_to_terminal_columns() {
        let matches = search_line_matches(0, "abcde", "cde", 4);

        assert_eq!(
            matches,
            vec![TerminalSearchMatch {
                line: 0,
                start_col: 2,
                end_col: 4,
                ranges: vec![TerminalSearchRange {
                    line: 0,
                    start_col: 2,
                    end_col: 4,
                }],
            }]
        );
    }

    #[test]
    fn logical_search_splits_matches_across_wrapped_rows() {
        let cell_map = vec![(-1, 0), (-1, 1), (-1, 2), (0, 0), (0, 1), (0, 2)];
        let matches = search_logical_line_matches("abcdef", &cell_map, "cde", 80);

        assert_eq!(
            matches,
            vec![TerminalSearchMatch {
                line: -1,
                start_col: 2,
                end_col: 3,
                ranges: vec![
                    TerminalSearchRange {
                        line: -1,
                        start_col: 2,
                        end_col: 3,
                    },
                    TerminalSearchRange {
                        line: 0,
                        start_col: 0,
                        end_col: 2,
                    },
                ],
            }]
        );
    }

    #[test]
    fn scrolled_grid_lines_map_into_viewport_rows() {
        assert_eq!(viewport_row_for_grid_line(-10, 10), Some(0));
        assert_eq!(viewport_row_for_grid_line(-1, 10), Some(9));
        assert_eq!(viewport_row_for_grid_line(0, 10), Some(10));
        assert_eq!(viewport_row_for_grid_line(-11, 10), None);
    }

    #[test]
    fn graphics_state_evicts_images_and_placements_over_budget() {
        let mut graphics = TerminalGraphicsState {
            storage_limit_bytes: 4,
            ..TerminalGraphicsState::default()
        };

        graphics.handle_event(TerminalGraphicsEvent::ImageReady(TerminalImageData {
            id: TerminalImageId(1),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));
        graphics.handle_event(TerminalGraphicsEvent::Place(TerminalImagePlacement {
            id: TerminalImageId(1),
            protocol: TerminalImageProtocol::Kitty,
            line: 0,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
        }));
        graphics.handle_event(TerminalGraphicsEvent::ImageReady(TerminalImageData {
            id: TerminalImageId(2),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));

        assert!(!graphics.images.contains_key(&TerminalImageId(1)));
        assert!(graphics.images.contains_key(&TerminalImageId(2)));
        assert!(graphics.placements.is_empty());
    }

    #[test]
    fn graphics_state_removes_existing_placements_when_image_id_is_retransmitted() {
        let mut graphics = TerminalGraphicsState::default();

        graphics.handle_event(TerminalGraphicsEvent::ImageReady(TerminalImageData {
            id: TerminalImageId(7),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));
        graphics.handle_event(TerminalGraphicsEvent::Place(TerminalImagePlacement {
            id: TerminalImageId(7),
            protocol: TerminalImageProtocol::Kitty,
            line: 0,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
        }));
        graphics.handle_event(TerminalGraphicsEvent::ImageReady(TerminalImageData {
            id: TerminalImageId(7),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));

        assert!(graphics.images.contains_key(&TerminalImageId(7)));
        assert!(graphics.placements.is_empty());
    }

    #[test]
    fn graphics_state_uses_monotonic_versions_across_deleted_image_ids() {
        let mut graphics = TerminalGraphicsState::default();
        let image = |id| TerminalImageData {
            id: TerminalImageId(id),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        };

        graphics.handle_event(TerminalGraphicsEvent::ImageReady(image(100)));
        let first_version = graphics.images[&TerminalImageId(100)].version;
        graphics.handle_event(TerminalGraphicsEvent::Delete {
            id: Some(TerminalImageId(100)),
        });
        graphics.handle_event(TerminalGraphicsEvent::ImageReady(image(200)));
        let second_version = graphics.images[&TerminalImageId(200)].version;

        assert!(second_version > first_version);
        assert_eq!(graphics.images.len(), 1);
    }

    #[test]
    fn graphics_state_preserves_placements_when_image_is_updated() {
        let mut graphics = TerminalGraphicsState::default();

        graphics.handle_event(TerminalGraphicsEvent::ImageReady(TerminalImageData {
            id: TerminalImageId(8),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));
        graphics.handle_event(TerminalGraphicsEvent::Place(TerminalImagePlacement {
            id: TerminalImageId(8),
            protocol: TerminalImageProtocol::Kitty,
            line: 0,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
        }));
        graphics.handle_event(TerminalGraphicsEvent::ImageUpdated(TerminalImageData {
            id: TerminalImageId(8),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));

        assert!(graphics.images.contains_key(&TerminalImageId(8)));
        assert_eq!(graphics.placements.len(), 1);
    }

    #[test]
    fn image_snapshots_share_terminal_image_data() {
        let size = TerminalSize {
            cols: 4,
            rows: 4,
            cell_width: 8,
            cell_height: 17,
        };
        let term = Term::new(Config::default(), &size, VoidListener);
        let mut graphics = TerminalGraphicsState::default();
        graphics.handle_event(TerminalGraphicsEvent::ImageReady(TerminalImageData {
            id: TerminalImageId(9),
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255].into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        }));
        graphics.handle_event(TerminalGraphicsEvent::Place(TerminalImagePlacement {
            id: TerminalImageId(9),
            protocol: TerminalImageProtocol::Kitty,
            line: 0,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: false,
        }));

        let first = snapshot_from_term(&term, size, &graphics);
        let second = snapshot_from_term(&term, size, &graphics);
        let first_data = first.images[0].data.as_ref().expect("image data");
        let second_data = second.images[0].data.as_ref().expect("image data");

        // Snapshot construction runs every changed tick; image payloads must stay
        // shared so ordinary terminal output does not clone frame metadata.
        assert!(Arc::ptr_eq(first_data, second_data));
    }

    #[test]
    fn yazi_kgp_old_sequence_anchors_image_at_moved_cursor_in_snapshot() {
        let size = TerminalSize {
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        };
        let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let mut graphics = TerminalGraphicsState::default();
        let payload = "AAAA/////wAAAP8A";
        let sequence =
            format!("\x1b7\x1b[6;41H\x1b_Gq=2,a=T,z=-1,C=1,f=24,s=2,v=2,m=0;{payload}\x1b\\\x1b8");

        let events = ingress.advance_with(
            sequence.as_bytes(),
            |bytes| {
                let mut term = term.borrow_mut();
                parser.advance(&mut *term, bytes);
            },
            || graphics_cursor_from_term(&term.borrow(), size),
        );
        for event in events {
            graphics.handle_event(event);
        }

        let snapshot = snapshot_from_term(&term.borrow(), size, &graphics);
        assert_eq!(snapshot.images.len(), 1);
        let image = &snapshot.images[0];
        assert_eq!(image.row, 5);
        assert_eq!(image.col, 40);
        assert_eq!(image.cols, 1);
        assert_eq!(image.rows, 1);
        assert!(image.data.is_some());
    }

    #[test]
    fn yazi_kgp_old_image_is_cleared_after_alt_screen_exit() {
        let size = TerminalSize {
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        };
        let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let mut graphics = TerminalGraphicsState::default();
        let mut alt_screen_active = false;
        let cursor = std::cell::Cell::new(graphics_cursor_from_term(&term.borrow(), size));
        let payload = "AAAA/////wAAAP8A";
        let sequence = format!(
            "\x1b[?1049h\x1b7\x1b[6;41H\x1b_Gq=2,a=T,z=-1,C=1,f=24,s=2,v=2,m=0;{payload}\x1b\\\x1b8\x1b[?1049l"
        );

        ingress.advance_ordered(
            sequence.as_bytes(),
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(bytes) => {
                    let mut term = term.borrow_mut();
                    parser.advance(&mut *term, &bytes);
                    graphics.clear_for_alt_screen_transition(&term, &mut alt_screen_active);
                    cursor.set(graphics_cursor_from_term(&term, size));
                }
                TerminalGraphicsSegment::Event(event) => {
                    graphics.handle_event(event);
                }
            },
            || cursor.get(),
        );

        let term = term.borrow();
        let snapshot = snapshot_from_term(&term, size, &graphics);
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
        assert!(snapshot.images.is_empty());
    }

    #[test]
    fn snapshot_preserves_soft_wrapped_visual_rows() {
        let size = TerminalSize {
            cols: 10,
            rows: 6,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"012345678901234567890123456789X");

        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let row_text = |row: usize| -> String {
            snapshot.lines[row]
                .cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>()
        };

        assert_eq!(row_text(0), "0123456789");
        assert_eq!(row_text(1), "0123456789");
        assert_eq!(row_text(2), "0123456789");
        assert_eq!(&row_text(3)[..1], "X");
        assert!(snapshot.lines[0].wrapped);
        assert!(snapshot.lines[1].wrapped);
        assert!(snapshot.lines[2].wrapped);
        assert!(!snapshot.lines[3].wrapped);
        assert!(snapshot.lines[0].active_input);
        assert!(snapshot.lines[1].active_input);
        assert!(snapshot.lines[2].active_input);
        assert!(snapshot.lines[3].active_input);
    }

    #[test]
    fn snapshot_with_display_offset_can_include_paint_overscan_rows() {
        let size = TerminalSize {
            cols: 12,
            rows: 3,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(
            &mut term,
            b"alpha\r\nbravo\r\ncharlie\r\ndelta\r\necho\r\nfoxtrot",
        );

        let snapshot = snapshot_from_term_with_display_offset(
            &term,
            size,
            &TerminalGraphicsState::default(),
            1,
            4,
        );

        assert_eq!(snapshot.display_offset, 1);
        assert_eq!(snapshot.rows, 3);
        assert_eq!(snapshot.lines.len(), 4);
        assert_eq!(snapshot.lines[0].absolute_line, -1);
        assert_eq!(snapshot.lines[3].absolute_line, 2);
        assert_eq!(snapshot.lines[3].text().trim_end(), "foxtrot");
    }

    #[test]
    fn shell_integration_osc633_creates_and_closes_command_mark() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]633;A\x07$ \x1b]633;B\x07echo hi\r\n\x1b]633;E;echo%20hi\x07hi\r\n\x1b]633;D;0\x07",
            |event| events.push(event),
        );

        let marks = integration.command_marks();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].command.as_deref(), Some("echo hi"));
        assert!(marks[0].is_closed);
        assert_eq!(
            marks[0].closed_by,
            Some(TerminalCommandMarkClosedBy::ShellIntegration)
        );
        assert_eq!(marks[0].exit_code, Some(0));
        assert!(matches!(
            integration.status().state,
            ShellIntegrationLifecycleState::Closed
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Created(_))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Closed(_))
        )));
        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let visible_text = snapshot
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!visible_text.contains("633;"));
        assert!(!visible_text.contains("echo%20hi"));
    }

    #[test]
    fn shell_integration_osc133_clear_saved_history_resets_command_mark_coordinates() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]133;A;click_events=1\x07$ ls\r\n\x1b]133;C;cmdline_url=ls\x07file\r\n\x1b]133;D;0\x07",
            |event| events.push(event),
        );
        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]133;A;click_events=1\x07$ clear\r\n\x1b]133;C;cmdline_url=clear\x07\x1b[H\x1b[2J\x1b[3",
            |event| events.push(event),
        );
        integration.advance(
            &mut parser,
            &mut term,
            b"J\x1b]133;D;0\x07\x1b]133;A\x07$ ",
            |event| events.push(event),
        );

        assert!(integration.command_marks().is_empty());
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Closed(mark))
                if mark.closed_by == Some(TerminalCommandMarkClosedBy::TerminalReset)
                    && mark.command.as_deref() == Some("clear")
                    && mark.stale
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Reset)
        )));

        integration.advance(
            &mut parser,
            &mut term,
            b"pwd\r\n\x1b]133;C;cmdline_url=pwd\x07/tmp\r\n\x1b]133;D;0\x07",
            |event| events.push(event),
        );
        let marks = integration.command_marks();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].command.as_deref(), Some("pwd"));
        assert!(marks[0].is_closed);
    }

    #[test]
    fn shell_integration_grid_reflow_closes_and_clears_active_command_mark() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]133;A;click_events=1\x07$ long-command\r\n\x1b]133;C;cmdline_url=long-command\x07output",
            |event| events.push(event),
        );
        assert_eq!(integration.command_marks().len(), 1);

        integration.reset_command_marks_for_grid_reflow(|event| events.push(event));

        assert!(integration.command_marks().is_empty());
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Closed(mark))
                if mark.command.as_deref() == Some("long-command")
                    && mark.closed_by == Some(TerminalCommandMarkClosedBy::TerminalReset)
                    && mark.stale
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Reset)
        )));

        integration.advance(
            &mut parser,
            &mut term,
            b"\r\n$ next-command\r\n\x1b]133;C;cmdline_url=next-command\x07done\r\n\x1b]133;D;0\x07",
            |event| events.push(event),
        );
        let marks = integration.command_marks();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].command.as_deref(), Some("next-command"));
        assert_eq!(marks[0].command_line, marks[0].start_line);
        assert!(marks[0].is_closed);
    }

    #[test]
    fn shell_integration_osc633_clear_saved_history_uses_shared_reset_path() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]633;A\x07PS> \x1b]633;B\x07Clear-Host\r\n\x1b]633;E;Clear-Host\x07\x1b[2J\x1b[3J",
            |event| events.push(event),
        );

        assert!(integration.command_marks().is_empty());
        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CommandMark(TerminalCommandMarkEvent::Reset)
        )));
    }

    #[test]
    fn shell_integration_osc7_emits_cwd_and_host() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]7;file://build-host/home/dev/Oxide%20Term\x07$ ",
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged {
                cwd,
                host: Some(host),
            } if cwd == "/home/dev/Oxide Term" && host == "build-host"
        )));
        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let visible_text = snapshot
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!visible_text.contains("file://build-host"));
        assert!(visible_text.contains("$"));
    }

    #[test]
    fn shell_integration_private_remote_metadata_accepts_version_two() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]7719;v=1;cwd=%2fwrong;host=%62%61%64\x07\x1b]7719;v=2;cwd=%2fhome%2fdev%2fAstrBot;host=%62%75%69%6c%64\x07$ ",
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged {
                cwd,
                host: Some(host),
            } if cwd == "/home/dev/AstrBot" && host == "build"
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged { cwd, .. } if cwd == "/wrong"
        )));
        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let visible_text = snapshot
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!visible_text.contains("7719;"));
        assert!(visible_text.contains("$"));
    }

    #[test]
    fn shell_integration_private_remote_metadata_accepts_windows_paths() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]7719;v=2;cwd=C%3a%5cUsers%5calice;host=desktop\x07PS> ",
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged {
                cwd,
                host: Some(host),
            } if cwd == "C:\\Users\\alice" && host == "desktop"
        )));
    }

    #[test]
    fn shell_integration_osc7_accepts_raw_path_compatibility() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]7;/tmp/Oxide%20Term\x07$ ",
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged { cwd, host: None }
                if cwd == "/tmp/Oxide Term"
        )));
    }

    #[test]
    fn shell_integration_osc633_property_can_update_cwd() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]633;P;Cwd=/work/Oxide%20Term\x07$ ",
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged { cwd, host: None }
                if cwd == "/work/Oxide Term"
        )));
        assert!(integration.command_marks().is_empty());
        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let visible_text = snapshot
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!visible_text.contains("633;P"));
    }

    #[test]
    fn shell_integration_osc1337_current_dir_can_update_cwd() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]1337;CurrentDir=/srv/Oxide%20Term\x07$ ",
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            TerminalEvent::CwdChanged { cwd, host: None }
                if cwd == "/srv/Oxide Term"
        )));
        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let visible_text = snapshot
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!visible_text.contains("CurrentDir="));
    }

    #[test]
    fn shell_integration_scanner_waits_for_split_osc_terminator() {
        let size = TerminalSize {
            cols: 80,
            rows: 8,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut integration = crate::shell_integration::TerminalShellIntegration::default();
        let mut events = Vec::new();

        integration.advance(
            &mut parser,
            &mut term,
            b"\x1b]633;A\x07$ \x1b]633;B\x07",
            |event| events.push(event),
        );
        integration.advance(&mut parser, &mut term, b"\x1b]633;E;pwd", |event| {
            events.push(event)
        });
        assert!(integration.command_marks().is_empty());
        integration.advance(&mut parser, &mut term, b"\x07/home\r\n", |event| {
            events.push(event)
        });

        let marks = integration.command_marks();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].command.as_deref(), Some("pwd"));
        assert!(!marks[0].is_closed);
        let snapshot = snapshot_from_term(&term, size, &TerminalGraphicsState::default());
        let visible_text = snapshot
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!visible_text.contains("633;"));
        assert!(!visible_text.contains("pwd\x07"));
    }

    #[test]
    fn color_request_uses_oxideterm_terminal_palette_indices() {
        let dim_background = color_for_alacritty_request_with_override(268, None);
        assert_eq!(dim_background.r, OXIDETERM_DARK_THEME.ansi[0].r);
        assert_eq!(dim_background.g, OXIDETERM_DARK_THEME.ansi[0].g);
        assert_eq!(dim_background.b, OXIDETERM_DARK_THEME.ansi[0].b);

        let out_of_range = color_for_alacritty_request_with_override(999, None);
        assert_eq!((out_of_range.r, out_of_range.g, out_of_range.b), (0, 0, 0));
    }

    #[test]
    fn window_size_preserves_physical_cell_dimensions() {
        let size = TerminalSize {
            cols: 97,
            rows: 42,
            cell_width: 16,
            cell_height: 34,
        };

        let window = window_size(size);
        assert_eq!(window.num_cols, 97);
        assert_eq!(window.num_lines, 42);
        assert_eq!(window.cell_width, 16);
        assert_eq!(window.cell_height, 34);
    }

    #[test]
    fn color_request_prefers_alacritty_runtime_overrides() {
        let override_color = Rgb {
            r: 12,
            g: 34,
            b: 56,
        };

        let color = color_for_alacritty_request_with_override(4, Some(override_color));
        assert_eq!((color.r, color.g, color.b), (12, 34, 56));
    }

    #[test]
    fn minimum_contrast_adjusts_theme_defined_ansi_colors() {
        let (fg, bg) = style_colors_for_cell(
            Color::Named(NamedColor::White),
            Color::Indexed(15),
            'x',
            TerminalAttrs::default(),
        );

        assert_ne!(fg, OXIDETERM_DARK_THEME.ansi[7]);
        assert_eq!(bg, OXIDETERM_DARK_THEME.ansi[15]);
        assert!(perceptual_contrast_score(fg, bg).abs() >= DEFAULT_MINIMUM_CONTRAST_SCORE);
    }

    #[test]
    fn app_chosen_truecolor_and_256_colors_bypass_contrast_adjustment() {
        let red_rgb = Rgb { r: 255, g: 0, b: 0 };
        let (truecolor_fg, _) = style_colors_for_cell(
            Color::Spec(red_rgb),
            Color::Named(NamedColor::Background),
            'x',
            TerminalAttrs::default(),
        );
        assert_eq!(truecolor_fg, TerminalColor::rgb(255, 0, 0));

        let (indexed_fg, _) = style_colors_for_cell(
            Color::Indexed(196),
            Color::Named(NamedColor::Background),
            'x',
            TerminalAttrs::default(),
        );
        assert_eq!(indexed_fg, indexed_color_to_rgb(196));
    }

    #[test]
    fn decorative_characters_bypass_contrast_adjustment() {
        let (fg, bg) = style_colors_for_cell(
            Color::Named(NamedColor::White),
            Color::Indexed(15),
            '\u{e0b0}',
            TerminalAttrs::default(),
        );

        assert_eq!(fg, OXIDETERM_DARK_THEME.ansi[7]);
        assert_eq!(bg, OXIDETERM_DARK_THEME.ansi[15]);
    }

    #[cfg(unix)]
    fn wait_for_child_pid(marker_path: &std::path::Path) -> u32 {
        let mut pid = None;
        assert_eventually(
            Duration::from_secs(3),
            || {
                pid = std::fs::read_to_string(marker_path)
                    .ok()
                    .and_then(|value| value.trim().parse::<u32>().ok());
                pid.is_some()
            },
            "PTY script should write background child PID",
        );
        pid.unwrap()
    }

    #[cfg(unix)]
    fn unix_process_is_running(pid: u32) -> bool {
        let status = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if status != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EPERM) {
            return false;
        }

        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output();
        let Ok(output) = output else {
            return true;
        };
        if !output.status.success() {
            return false;
        }

        !String::from_utf8_lossy(&output.stdout).contains('Z')
    }

    #[cfg(unix)]
    fn assert_eventually(timeout: Duration, mut predicate: impl FnMut() -> bool, message: &str) {
        let started = std::time::Instant::now();
        while started.elapsed() < timeout {
            if predicate() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(predicate(), "{message}");
    }
}
