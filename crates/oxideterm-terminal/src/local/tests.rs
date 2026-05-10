mod tests {
    use super::*;
    use std::path::PathBuf;

    use alacritty_terminal::{
        event::VoidListener,
        term::Config,
        vte::ansi::{Color, NamedColor, Processor, Rgb, StdSyncHandler},
    };
    use oxideterm_terminal_graphics::GraphicsIngress;

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
            name: None,
        }));

        assert!(!graphics.images.contains_key(&TerminalImageId(1)));
        assert!(graphics.images.contains_key(&TerminalImageId(2)));
        assert!(graphics.placements.is_empty());
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

        integration.advance(&mut parser, &mut term, b"\x1b]633;A\x07$ \x1b]633;B\x07", |event| {
            events.push(event)
        });
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
}
