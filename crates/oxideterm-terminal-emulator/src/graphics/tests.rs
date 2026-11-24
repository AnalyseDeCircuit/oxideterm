use super::*;
use crate::{GraphicsOptions, TerminalImageAnimationState, convert::snapshot_from_term};
use alacritty_terminal::{
    event::VoidListener,
    index::Line,
    term::Config,
    vte::ansi::{Processor, StdSyncHandler},
};
use oxideterm_terminal_graphics::{GraphicsIngress, TerminalGraphicsSegment};

#[test]
fn scrolled_grid_lines_map_into_viewport_rows() {
    assert_eq!(viewport_row_for_grid_line(-10, 10), Some(0));
    assert_eq!(viewport_row_for_grid_line(-1, 10), Some(9));
    assert_eq!(viewport_row_for_grid_line(0, 10), Some(10));
    assert_eq!(viewport_row_for_grid_line(-11, 10), None);
}

#[test]
fn graphics_budget_eviction_releases_snapshot_pixels_and_placements() {
    let mut graphics = TerminalGraphicsState::default();
    let mut ingress = GraphicsIngress::new(GraphicsOptions {
        storage_limit_mb: 1,
        ..GraphicsOptions::default()
    });
    let cursor = GraphicsCursor {
        line: 0,
        row: 0,
        col: 0,
        cols: 80,
        rows: 24,
        cell_width: 10,
        cell_height: 20,
    };
    let mut previous = None;
    for id in 1..=2 {
        let payload = format!("{}AA==", "AAAA".repeat((512 * 512 * 4) / 3));
        let bytes = format!("\x1b_Ga=T,i={id},f=32,s=512,v=512,C=1,q=2;{payload}\x1b\\");
        for event in ingress.advance(bytes.as_bytes(), cursor).events {
            graphics.handle_event(event);
        }
        if id == 1 {
            previous = Some(Arc::downgrade(&graphics.images[&TerminalImageId(1)].rgba));
        }
    }
    assert!(previous.unwrap().upgrade().is_none());
    assert_eq!(
        graphics
            .visible_images(0, 24)
            .iter()
            .map(|i| i.id.0)
            .collect::<Vec<_>>(),
        [2]
    );
    assert_eq!(graphics.images[&TerminalImageId(2)].rgba[0], 0);
}

#[test]
fn graphics_state_retransmission_removes_placements_but_updates_preserve_them() {
    let image = |rgba: [u8; 4]| TerminalImageData {
        memory: None,
        id: TerminalImageId(7),
        protocol: TerminalImageProtocol::Kitty,
        version: 0,
        width: 1,
        height: 1,
        rgba: Vec::from(rgba).into(),
        frames: Vec::new(),
        animation: TerminalImageAnimationState::default(),
        name: None,
    };
    let placement = TerminalImagePlacement {
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
        kitty: None,
        sixel: None,
        alternate_screen: false,
    };
    for (event, expected_placements) in [
        (TerminalGraphicsEvent::ImageReady(image([255; 4])), vec![]),
        (
            TerminalGraphicsEvent::ImageUpdated(image([255; 4])),
            vec![placement.clone()],
        ),
    ] {
        let mut graphics = TerminalGraphicsState::default();
        graphics.handle_event(TerminalGraphicsEvent::ImageReady(image([0, 0, 0, 255])));
        graphics.handle_event(TerminalGraphicsEvent::Place(placement.clone()));
        graphics.handle_event(event);

        assert_eq!(
            graphics.images[&TerminalImageId(7)].rgba.as_ref(),
            &[255; 4]
        );
        assert_eq!(graphics.placements, expected_placements);
    }
}

#[test]
fn graphics_state_uses_monotonic_versions_across_deleted_image_ids() {
    let mut graphics = TerminalGraphicsState::default();
    let image = |id| TerminalImageData {
        memory: None,
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
        memory: None,
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
        kitty: None,
        sixel: None,
        alternate_screen: false,
    }));

    let first = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    let second = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
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

    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(snapshot.images.len(), 1);
    let image = &snapshot.images[0];
    assert_eq!(image.row, 5);
    assert_eq!(image.col, 40);
    assert_eq!(image.cols, 1);
    assert_eq!(image.rows, 1);
    assert!(image.data.is_some());
}

#[test]
fn kitty_synchronized_redraw_commits_pixels_at_the_frame_cursor() {
    let size = TerminalSize {
        cols: 20,
        rows: 10,
        cell_width: 10,
        cell_height: 20,
    };
    for chunk_size in [1, 7, 4096] {
        let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let mut graphics = TerminalGraphicsState::default();
        parser.advance(&mut *term.borrow_mut(), b"\x1b[9;12H");
        for (frame, expected) in [
            (
                "\x1b[?2026h\x1b[3;4H\x1b[2K\x1b_Ga=T,f=32,s=1,v=1,c=3,r=2,i=42,C=1;AP8A/w==\x1b\\\x1b[4;1H\x1b[2K",
                (2, 3),
            ),
            (
                "\x1b[?2026h\x1b_Ga=d,d=a,q=2\x1b\\\x1b[5;2H\x1b[2K\x1b_Ga=p,i=42,c=3,r=2,C=1,q=2\x1b\\\x1b[6;1H\x1b[2K",
                (4, 1),
            ),
        ] {
            let before = graphics.visible_images(0, size.rows);
            let frame = format!("{frame}\x1b[?2026l");
            for (index, chunk) in frame.as_bytes().chunks(chunk_size).enumerate() {
                ingress.advance_ordered(
                    chunk,
                    |segment| match segment {
                        TerminalGraphicsSegment::Terminal(bytes) => {
                            parser.advance(&mut *term.borrow_mut(), &bytes)
                        }
                        TerminalGraphicsSegment::Event(event) => {
                            graphics.handle_event(event);
                        }
                    },
                    || graphics_cursor_from_term(&term.borrow(), size),
                );
                if (index + 1) * chunk_size < frame.len() {
                    assert_eq!(
                        graphics.visible_images(0, size.rows),
                        before,
                        "unfinished frame, chunk size {chunk_size}"
                    );
                }
            }
            let visible = graphics.visible_images(0, size.rows);
            assert_eq!(
                visible
                    .iter()
                    .map(|image| (image.id, image.row, image.col, image.cols, image.rows))
                    .collect::<Vec<_>>(),
                [(TerminalImageId(42), expected.0, expected.1, 3, 2)]
            );
            assert_eq!(
                visible[0].data.as_ref().unwrap().rgba.as_ref(),
                &[0, 255, 0, 255]
            );
        }
    }
}

#[test]
fn kitty_multiple_placements_share_pixels_and_delete_independently() {
    let size = TerminalSize {
        cols: 20,
        rows: 10,
        cell_width: 10,
        cell_height: 20,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    let bytes = b"\x1b_Ga=t,i=42,f=32,s=1,v=1,q=2;/wAA/w==\x1b\\\x1b[2;3H\x1b_Ga=p,i=42,p=1,C=1,q=2\x1b\\\x1b[5;6H\x1b_Ga=p,i=42,p=2,C=1,q=2\x1b\\";
    for chunk in bytes.chunks(3) {
        ingress.advance_ordered(
            chunk,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(bytes) => {
                    parser.advance(&mut *term.borrow_mut(), &bytes)
                }
                TerminalGraphicsSegment::Event(event) => {
                    graphics.handle_event(event);
                }
            },
            || graphics_cursor_from_term(&term.borrow(), size),
        );
    }
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(
        snapshot
            .images
            .iter()
            .map(|image| (image.row, image.col))
            .collect::<Vec<_>>(),
        [(1, 2), (4, 5)]
    );
    assert_eq!(
        snapshot.images[0].data.as_ref().unwrap().rgba.as_ref(),
        &[255, 0, 0, 255]
    );
    assert!(Arc::ptr_eq(
        snapshot.images[0].data.as_ref().unwrap(),
        snapshot.images[1].data.as_ref().unwrap()
    ));
    for event in ingress
        .advance(
            b"\x1b_Ga=d,d=i,i=42,p=1,q=2\x1b\\",
            graphics_cursor_from_term(&term.borrow(), size),
        )
        .events
    {
        graphics.handle_event(event);
    }
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(
        snapshot
            .images
            .iter()
            .map(|image| (image.row, image.col))
            .collect::<Vec<_>>(),
        [(4, 5)]
    );
    assert_eq!(term.borrow().grid().cursor.point.column.0, 5);
}

fn feed_kitty_grid(
    ingress: &mut GraphicsIngress,
    term: &std::cell::RefCell<Term<VoidListener>>,
    parser: &mut Processor<StdSyncHandler>,
    graphics: &mut TerminalGraphicsState,
    bytes: &[u8],
    size: TerminalSize,
) {
    ingress.attach_grid(term.borrow_mut().graphics_events());
    ingress.advance_ordered(
        bytes,
        |segment| match segment {
            TerminalGraphicsSegment::Terminal(bytes) => {
                parser.advance(&mut *term.borrow_mut(), &bytes)
            }
            TerminalGraphicsSegment::Event(event) => {
                apply_graphics_terminal_event(&mut *term.borrow_mut(), &event);
                graphics.handle_event(event);
            }
        },
        || graphics_cursor_from_term(&term.borrow(), size),
    );
}

#[test]
fn sixel_preserves_text_and_tracks_grid_lifecycle() {
    let mut size = TerminalSize {
        cols: 12,
        rows: 6,
        cell_width: 10,
        cell_height: 6,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"abcdefghijkl\x1b[1;3H\x1bP9;1q#1;2;100;0;0!15~-!15~\x1b\\",
        size,
    );
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(
        snapshot.lines[0]
            .cells
            .iter()
            .map(|c| c.ch)
            .collect::<String>(),
        "abcdefghijkl"
    );
    assert_eq!((snapshot.cursor_row, snapshot.cursor_col), (1, 2));
    assert_eq!(
        (
            snapshot.images[0].row,
            snapshot.images[0].col,
            snapshot.images[0].rows
        ),
        (0, 2, 2)
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1049h\x1bP9;1q#2;2;0;100;0~\x1b\\\x1b[?1049l",
        size,
    );
    assert_eq!(
        graphics.visible_images(0, 6)[0].data.as_ref().unwrap().rgba[..4],
        [255, 0, 0, 255]
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1049h",
        size,
    );
    assert!(graphics.visible_images(0, 6).is_empty());
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1049l\x1b[6;1H\n",
        size,
    );
    assert_eq!(graphics.placements[0].line, -1);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[3J",
        size,
    );
    assert_eq!(graphics.placements[0].line, -1);
    assert_eq!(graphics.visible_images(0, 6)[0].clipped_rows, 1);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[2J",
        size,
    );
    assert!(graphics.images.is_empty());
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[1;1Habcdefghij\x1b[1;5H\x1bP9;1q~\x1b\\",
        size,
    );
    size.cols = 3;
    term.borrow_mut().resize(size);
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"", size);
    assert_eq!(graphics.placements[0].col, 1);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1bc",
        size,
    );
    assert!(graphics.images.is_empty());
}

#[test]
fn kitty_cursor_advance_scrolls_the_image_above_following_text() {
    let size = TerminalSize {
        cols: 12,
        rows: 6,
        cell_width: 10,
        cell_height: 20,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[5;1Habcdef\x1b[5;2H\x1b_Ga=T,i=42,f=32,s=1,v=1,c=2,r=3,q=2;/wAA/w==\x1b\\Z",
        size,
    );
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(
        (
            snapshot.images[0].row,
            snapshot.images[0].col,
            snapshot.images[0].rows
        ),
        (2, 1, 3)
    );
    assert_eq!(snapshot.scrollback_lines, 2);
    assert_eq!(
        snapshot.lines[2]
            .cells
            .iter()
            .take(6)
            .map(|c| c.ch)
            .collect::<String>(),
        "abcdef"
    );
    assert_eq!(snapshot.lines[5].cells[3].ch, 'Z');
    assert_eq!((snapshot.cursor_row, snapshot.cursor_col), (5, 4));
}

#[test]
fn sixel_modes_shared_colors_and_local_scroll() {
    let size = TerminalSize {
        cols: 12,
        rows: 6,
        cell_width: 10,
        cell_height: 6,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1070l\x1b[3;4H\x1bP9;1q#3;2;100;0;0~-~\x1b\\\x1b[1;1H\x1bP9;1q#3~\x1b\\",
        size,
    );
    assert_eq!(
        graphics.visible_images(0, 6)[1]
            .data
            .as_ref()
            .unwrap()
            .rgba
            .as_ref(),
        [255, 0, 0, 255].repeat(6)
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1070h\x1bP9;1q#3~\x1b\\\x1b[?1070l\x1bP9;1q#3~\x1b\\",
        size,
    );
    let images = graphics.visible_images(0, 6);
    assert_eq!(
        images[2].data.as_ref().unwrap().rgba[..4],
        [51, 204, 51, 255]
    );
    assert_eq!(images[3].data.as_ref().unwrap().rgba[..4], [255, 0, 0, 255]);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?80h\x1b[5;7H\x1bP9;1q#2;2;0;100;0~-~\x1b\\",
        size,
    );
    assert_eq!(
        (
            term.borrow().grid().cursor.point.line.0,
            term.borrow().grid().cursor.point.column.0
        ),
        (4, 6)
    );
    let fixed = graphics
        .visible_images(0, 6)
        .into_iter()
        .find(|p| p.sixel.as_ref().unwrap().fixed)
        .unwrap();
    assert_eq!((fixed.row, fixed.col), (0, 0));
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?80l\x1b[?8452h\x1b[1;1H\x1bP9;1q#1;1;120;50;100!15~\x1b\\",
        size,
    );
    assert_eq!(
        (
            term.borrow().grid().cursor.point.line.0,
            term.borrow().grid().cursor.point.column.0
        ),
        (0, 2)
    );
    assert_eq!(
        graphics
            .visible_images(0, 6)
            .last()
            .unwrap()
            .data
            .as_ref()
            .unwrap()
            .rgba[..4],
        [255, 0, 0, 255]
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1bc\x1b[1;1H\x1bP9;1q#1;2;100;0;0~-~-~-~-~\x1b\\\x1b[2;4r\x1b[4;1H\n",
        size,
    );
    let fragments = graphics
        .placements
        .iter()
        .map(|p| {
            let s = p.sixel.as_ref().unwrap();
            (
                p.line + s.clip_top as i32,
                p.line + p.rows as i32 - s.clip_bottom as i32,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(fragments, [(0, 1), (4, 5), (1, 3)]);
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert!(snapshot.images.iter().all(|p| Arc::ptr_eq(
        p.data.as_ref().unwrap(),
        snapshot.images[0].data.as_ref().unwrap()
    )));
}

#[test]
fn sixel_aspect_and_background_preserve_source_pixels() {
    let size = TerminalSize {
        cols: 12,
        rows: 10,
        cell_width: 10,
        cell_height: 6,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[48;2;12;34;56m\x1bP2;0q#1;2;100;0;0@\x1b\\\x1bP2;1q\"1;1;15;7@\x1b\\",
        size,
    );
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    let opaque = &snapshot.images[0];
    assert_eq!(
        (
            opaque.source_width,
            opaque.source_height,
            opaque.pixel_width,
            opaque.pixel_height,
            opaque.rows
        ),
        (1, 6, 1, 30, 5)
    );
    assert_eq!(
        opaque.sixel.as_ref().unwrap().background_rgb(),
        Some([12, 34, 56])
    );
    assert_eq!(
        &opaque.data.as_ref().unwrap().rgba[..8],
        &[255, 0, 0, 255, 0, 0, 0, 0]
    );
    let transparent = &snapshot.images[1];
    assert_eq!(
        (
            transparent.source_width,
            transparent.source_height,
            transparent.pixel_width,
            transparent.pixel_height
        ),
        (15, 7, 15, 7)
    );
    assert_eq!(transparent.sixel.as_ref().unwrap().background, None);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[1;1H\x1bPq~\x1b\\",
        size,
    );
    assert_eq!(
        graphics.visible_images(0, 10).last().unwrap().pixel_height,
        12
    );
}

#[test]
fn sixel_text_erasure_preserves_surrounding_pixels_and_releases_the_last_fragment() {
    let size = TerminalSize {
        cols: 8,
        rows: 6,
        cell_width: 2,
        cell_height: 2,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"abcdefgh\r\nijklmnop\r\nqrstuvwx\x1b[1;1H\x1bP9;1q#1;2;100;0;0!16~\x1b\\\x1b[2;3H\x1b[2X",
        size,
    );
    let fragments = sixel_fragment_cells(&graphics);
    assert_eq!(
        fragments,
        [(0, 1, 0, 8), (1, 2, 0, 2), (1, 2, 4, 8), (2, 3, 0, 8)]
    );
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(
        snapshot.lines[1]
            .cells
            .iter()
            .map(|c| c.ch)
            .collect::<String>(),
        "ij  mnop"
    );
    let pixels = Arc::downgrade(&snapshot.images[0].data.as_ref().unwrap().rgba);
    assert!(snapshot.images.iter().all(|p| Arc::ptr_eq(
        p.data.as_ref().unwrap(),
        snapshot.images[0].data.as_ref().unwrap()
    )));
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[1;1H\x1b[8X\x1b[2;1H\x1b[8X\x1b[3;1H\x1b[8X",
        size,
    );
    assert!(graphics.images.is_empty());
    assert!(graphics.placements.is_empty());
    drop(snapshot);
    assert!(pixels.upgrade().is_none());
}

fn sixel_fragment_cells(graphics: &TerminalGraphicsState) -> Vec<(i32, i32, i64, i64)> {
    let mut rectangles = graphics
        .placements
        .iter()
        .filter_map(|p| {
            let s = p.sixel.as_ref()?;
            let origin = p.col as i64 + s.column_offset;
            Some((
                p.line + s.clip_top as i32,
                p.line + p.rows as i32 - s.clip_bottom as i32,
                origin + s.clip_left as i64,
                origin + p.cols as i64 - s.clip_right as i64,
            ))
        })
        .collect::<Vec<_>>();
    rectangles.sort_unstable();
    rectangles
}

#[test]
fn sixel_text_commands_update_only_the_affected_cells() {
    let surrounding = [(0, 1, 0, 8), (1, 2, 0, 2), (1, 2, 4, 8), (2, 3, 0, 8)];
    let cases: &[(&str, &[u8], &[(i32, i32, i64, i64)], &str)] = &[
        ("ascii", b"XY", &surrounding, "ijXYmnop"),
        ("wide", "界".as_bytes(), &surrounding, "ij界 mnop"),
        (
            "combining",
            "e\u{301}".as_bytes(),
            &[(0, 1, 0, 8), (1, 2, 0, 2), (1, 2, 3, 8), (2, 3, 0, 8)],
            "ijelmnop",
        ),
        ("erase", b"\x1b[2X", &surrounding, "ij  mnop"),
        (
            "line-right",
            b"\x1b[K",
            &[(0, 1, 0, 8), (1, 2, 0, 2), (2, 3, 0, 8)],
            "ij      ",
        ),
        (
            "line-left",
            b"\x1b[1K",
            &[(0, 1, 0, 8), (1, 2, 3, 8), (2, 3, 0, 8)],
            "   lmnop",
        ),
        (
            "line-all",
            b"\x1b[2K",
            &[(0, 1, 0, 8), (2, 3, 0, 8)],
            "        ",
        ),
        (
            "display-below",
            b"\x1b[J",
            &[(0, 1, 0, 8), (1, 2, 0, 2)],
            "ij      ",
        ),
        (
            "display-above",
            b"\x1b[1J",
            &[(1, 2, 3, 8), (2, 3, 0, 8)],
            "   lmnop",
        ),
        ("insert", b"\x1b[2@", &surrounding, "ij  klmn"),
        (
            "delete",
            b"\x1b[2P",
            &[(0, 1, 0, 8), (1, 2, 0, 2), (1, 2, 2, 6), (2, 3, 0, 8)],
            "ijmnop  ",
        ),
        (
            "delete-to-margin",
            b"\x1b[100P",
            &[(0, 1, 0, 8), (1, 2, 0, 2), (2, 3, 0, 8)],
            "ij      ",
        ),
        (
            "insert-mode",
            b"\x1b[4hX",
            &[(0, 1, 0, 8), (1, 2, 0, 2), (1, 2, 3, 8), (2, 3, 0, 8)],
            "ijXklmno",
        ),
        ("cursor-only", b"\t\x08\r\n", &[(0, 3, 0, 8)], "ijklmnop"),
    ];
    for &(name, command, expected, row) in cases {
        let size = TerminalSize {
            cols: 8,
            rows: 6,
            cell_width: 2,
            cell_height: 2,
        };
        let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
        let mut parser = Processor::<StdSyncHandler>::new();
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let mut graphics = TerminalGraphicsState::default();
        feed_kitty_grid(
            &mut ingress,
            &term,
            &mut parser,
            &mut graphics,
            b"abcdefgh\r\nijklmnop\r\nqrstuvwx\x1b[1;1H\x1bP9;1q#1;2;100;0;0!16~\x1b\\\x1b[2;3H",
            size,
        );
        feed_kitty_grid(
            &mut ingress,
            &term,
            &mut parser,
            &mut graphics,
            command,
            size,
        );
        assert_eq!(sixel_fragment_cells(&graphics), expected, "{name}");
        let snapshot =
            snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
        assert_eq!(
            snapshot.lines[1]
                .cells
                .iter()
                .map(|c| c.ch)
                .collect::<String>(),
            row,
            "{name}"
        );
        if name == "display-above" {
            assert_eq!(
                snapshot.lines[0]
                    .cells
                    .iter()
                    .map(|c| c.ch)
                    .collect::<String>(),
                "        "
            );
        }
        if let Some(shift) = match name {
            "insert" => Some(2),
            "delete" => Some(-2),
            "insert-mode" => Some(1),
            _ => None,
        } {
            let shifted = graphics
                .placements
                .iter()
                .find(|p| p.sixel.as_ref().unwrap().column_offset == shift)
                .unwrap();
            assert_eq!(shifted.line, 0, "{name}");
            assert_eq!(shifted.source_width, 16, "{name}");
        }
    }
}

#[test]
fn fixed_sixel_text_batches_and_reflow_keep_erased_pixels_hidden() {
    let mut size = TerminalSize {
        cols: 8,
        rows: 6,
        cell_width: 2,
        cell_height: 2,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?80h\x1bP9;1q!16~\x1b\\\x1b[?80l\x1b[6;1H",
        size,
    );
    // Exceed the ANSI parser's line-batching threshold through the real input path.
    let output = b"a\r\n".repeat(64);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        &output,
        size,
    );
    assert_eq!(sixel_fragment_cells(&graphics), [(0, 3, 0, 8)]);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1bc\x1bP9;1q!16~\x1b\\\x1b[6;1H",
        size,
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        &output,
        size,
    );
    assert_eq!(sixel_fragment_cells(&graphics), [(-64, -61, 0, 8)]);
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1bcabcdefgh\x1b[1;1H\x1bP9;1q!16~\x1b\\\x1b[1;1H\x1b[4X",
        size,
    );
    size.cols = 4;
    term.borrow_mut().resize(size);
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"", size);
    let rects = sixel_fragment_cells(&graphics);
    assert!(rects.iter().all(|r| r.2 >= 0));
    let right = graphics
        .placements
        .iter()
        .find(|p| p.sixel.as_ref().unwrap().clip_left == 4)
        .unwrap();
    assert_eq!(
        (right.col, right.sixel.as_ref().unwrap().column_offset),
        (0, -4)
    );
}

#[test]
fn kitty_grid_changes_preserve_text_screens_scroll_and_reflow() {
    let mut size = TerminalSize {
        cols: 12,
        rows: 6,
        cell_width: 10,
        cell_height: 20,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"0123456789\x1b[1;1H\x1b_Ga=T,i=42,f=32,s=1,v=1,c=2,r=2,q=2;/wAA/w==\x1b\\X",
        size,
    );
    let snapshot = snapshot_from_term(&term.borrow(), size, &graphics, &TerminalPalette::default());
    assert_eq!(
        snapshot.lines[0]
            .cells
            .iter()
            .take(10)
            .map(|c| c.ch)
            .collect::<String>(),
        "0123456789"
    );
    assert_eq!(snapshot.lines[2].cells[2].ch, 'X');
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1049h\x1b_Ga=T,i=42,f=32,s=1,v=1,C=1,q=2;AP8A/w==\x1b\\",
        size,
    );
    assert_eq!(
        graphics.visible_images(0, 6)[0]
            .data
            .as_ref()
            .unwrap()
            .rgba
            .as_ref(),
        &[0, 255, 0, 255]
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1049l",
        size,
    );
    assert_eq!(
        graphics.visible_images(0, 6)[0]
            .data
            .as_ref()
            .unwrap()
            .rgba
            .as_ref(),
        &[255, 0, 0, 255]
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[?1049h",
        size,
    );
    assert!(graphics.visible_images(0, 6).is_empty());
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"\x1b[?1049l\x1b[2J\x1b[1;1Habcdefghij\x1b[1;5H\x1b_Ga=T,i=43,f=32,s=1,v=1,C=1,q=2;AAD//w==\x1b\\", size);
    size.cols = 3;
    term.borrow_mut().resize(size);
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"", size);
    let anchor = graphics
        .placements
        .iter()
        .find(|p| p.id == TerminalImageId(43))
        .unwrap();
    assert_eq!(anchor.col, 1);
    assert_eq!(
        term.borrow()
            .grid()
            .cell(alacritty_terminal::index::Point::new(
                Line(anchor.line),
                alacritty_terminal::index::Column(anchor.col)
            ))
            .c,
        'e'
    );
    size.cols = 12;
    term.borrow_mut().resize(size);
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"", size);
    let anchor = graphics
        .placements
        .iter()
        .find(|p| p.id == TerminalImageId(43))
        .unwrap();
    assert_eq!(anchor.col, 4);
    assert_eq!(
        term.borrow()
            .grid()
            .cell(alacritty_terminal::index::Point::new(
                Line(anchor.line),
                alacritty_terminal::index::Column(anchor.col)
            ))
            .c,
        'e'
    );
    let previous_line = anchor.line;
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[6;1H\n",
        size,
    );
    let anchor = graphics
        .placements
        .iter()
        .find(|p| p.id == TerminalImageId(43))
        .unwrap();
    assert_eq!(anchor.line, previous_line - 1);
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics,
        b"\x1b[2J\x1b[4;1Hz\x1b[2;1H\x1b_Ga=T,i=44,f=32,s=1,v=1,c=1,r=4,C=1,q=2;/wAA/w==\x1b\\\x1b[2;5r\x1b[5;1H\n\n", size);
    let clipped = graphics
        .placements
        .iter()
        .find(|p| p.id == TerminalImageId(44))
        .unwrap();
    assert_eq!(
        (clipped.line, clipped.kitty.as_ref().unwrap().clip_top),
        (-1, 2)
    );
    size.cols = 6;
    term.borrow_mut().resize(size);
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"", size);
    let clipped = graphics
        .placements
        .iter()
        .find(|p| p.id == TerminalImageId(44))
        .unwrap();
    let visible = clipped.line + clipped.kitty.as_ref().unwrap().clip_top as i32;
    assert_eq!(
        term.borrow()
            .grid()
            .cell(alacritty_terminal::index::Point::new(
                Line(visible),
                alacritty_terminal::index::Column(clipped.col)
            ))
            .c,
        'z'
    );
    term.borrow_mut().set_options(Config {
        scrolling_history: 0,
        ..Default::default()
    });
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics, b"", size);
    assert!(
        !graphics
            .placements
            .iter()
            .any(|p| p.id == TerminalImageId(43))
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[3J",
        size,
    );
    assert!(
        graphics
            .visible_images(0, 6)
            .iter()
            .any(|p| p.id == TerminalImageId(44))
    );
}

#[test]
fn kitty_unicode_rows_drive_virtual_parents_without_changing_text() {
    let size = TerminalSize {
        cols: 12,
        rows: 6,
        cell_width: 10,
        cell_height: 20,
    };
    let term = std::cell::RefCell::new(Term::new(Config::default(), &size, VoidListener));
    let mut parser = Processor::<StdSyncHandler>::new();
    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
    let mut graphics = TerminalGraphicsState::default();
    let text = "\x1b[38;2;0;0;42;58;2;0;0;7m\u{10eeee}\u{305}\u{305}\u{10eeee}\x1b[0m";
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        text.as_bytes(),
        size,
    );
    assert!(graphics.visible_images(0, 6).is_empty());
    feed_kitty_grid(&mut ingress, &term, &mut parser, &mut graphics,
        b"\x1b_Ga=T,i=42,p=7,U=1,c=4,r=2,f=32,s=1,v=1,q=2;/wAA/w==\x1b\\\x1b_Ga=T,i=43,p=8,P=42,Q=7,H=-1,V=1,c=2,r=1,f=32,s=1,v=1,q=2;AP8A/w==\x1b\\", size);
    let images = graphics.visible_images(0, 6);
    let marker = images
        .iter()
        .find(|p| p.kitty.as_ref().unwrap().unicode.is_some())
        .unwrap();
    assert_eq!(
        (
            marker.row,
            marker.col,
            marker
                .kitty
                .as_ref()
                .unwrap()
                .unicode
                .as_ref()
                .unwrap()
                .cols
        ),
        (0, 0, 2)
    );
    let child = images.iter().find(|p| p.id == TerminalImageId(43)).unwrap();
    assert_eq!(
        (
            child.row,
            child.col,
            child.kitty.as_ref().unwrap().column_offset
        ),
        (1, 0, -1)
    );
    let point =
        alacritty_terminal::index::Point::new(Line(0), alacritty_terminal::index::Column(0));
    assert_eq!(term.borrow().grid().cell(point).c, '\u{10eeee}');
    assert_eq!(
        term.borrow().grid().cell(point).zerowidth(),
        Some(&['\u{305}', '\u{305}'][..])
    );
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[1;1H\x1b[2K",
        size,
    );
    assert!(graphics.visible_images(0, 6).is_empty());
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        "\x1b[2;4H\x1b[38;2;0;0;42;58;2;0;0;7m\u{10eeee}\u{305}\u{305}\x1b[0m".as_bytes(),
        size,
    );
    let images = graphics.visible_images(0, 6);
    let child = images.iter().find(|p| p.id == TerminalImageId(43)).unwrap();
    assert_eq!((child.row, child.col), (2, 2));
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b[6;1H\n",
        size,
    );
    let images = graphics.visible_images(0, 6);
    let child = images.iter().find(|p| p.id == TerminalImageId(43)).unwrap();
    assert_eq!((child.row, child.col), (1, 2));
    feed_kitty_grid(
        &mut ingress,
        &term,
        &mut parser,
        &mut graphics,
        b"\x1b_Ga=d,d=i,i=42,p=7,q=2\x1b\\",
        size,
    );
    assert!(graphics.visible_images(0, 6).is_empty());
    assert!(!graphics.images.contains_key(&TerminalImageId(43)));
}
