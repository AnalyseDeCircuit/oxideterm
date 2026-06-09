mod tests {
    use super::*;
    use image::{Delay, Frame, RgbaImage};
    use image::codecs::gif::GifEncoder;

    fn cursor() -> GraphicsCursor {
        GraphicsCursor {
            row: 0,
            line: 0,
            col: 0,
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        }
    }

    #[test]
    fn plain_text_passes_through() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"hello", cursor());
        assert_eq!(result.terminal_bytes, b"hello");
        assert!(result.events.is_empty());
    }

    #[test]
    fn split_osc_sequence_is_consumed() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let payload = BASE64.encode(bytes);
        let seq = format!("\x1b]1337;File=inline=1:{payload}\x07");
        let first = ingress_advance_chunks(seq.as_bytes());
        assert!(first.terminal_bytes.contains(&b' '));
        assert!(
            first
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    }

    #[test]
    fn gif_payload_preserves_animation_frames() {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            encoder
                .encode_frames([
                    Frame::from_parts(
                        RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])),
                        0,
                        0,
                        Delay::from_numer_denom_ms(50, 1),
                    ),
                    Frame::from_parts(
                        RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255])),
                        0,
                        0,
                        Delay::from_numer_denom_ms(80, 1),
                    ),
                ])
                .unwrap();
        }
        let payload = BASE64.encode(bytes);
        let seq = format!("\x1b]1337;File=inline=1:{payload}\x07");
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());

        let result = ingress.advance(seq.as_bytes(), cursor());
        let image = result
            .events
            .iter()
            .find_map(|event| match event {
                TerminalGraphicsEvent::ImageReady(image) => Some(image),
                _ => None,
            })
            .expect("gif image event");

        assert_eq!(image.frames.len(), 2);
        assert_eq!(image.frames[0].delay_ms_numerator, 50);
        assert_eq!(image.frames[1].delay_ms_numerator, 80);
    }

    #[test]
    fn invalid_iterm2_base64_does_not_leak_escape_sequence() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"\x1b]1337;File=inline=1:not base64\x07", cursor());

        assert!(result.terminal_bytes.is_empty());
        assert!(
            result
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::Error(_)))
        );
    }

    #[test]
    fn kitty_raw_rgba_image_is_placed_and_respects_no_cursor_move() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let payload = BASE64.encode([0, 255, 0, 255]);
        let seq = format!("\x1b_Ga=T,f=32,s=1,v=1,i=42,C=1;{payload}\x1b\\");
        let result = ingress.advance(seq.as_bytes(), cursor());

        assert!(result.terminal_bytes.is_empty());
        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::ImageReady(TerminalImageData {
                    id: TerminalImageId(42),
                    protocol: TerminalImageProtocol::Kitty,
                    width: 1,
                    height: 1,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_transmit_only_does_not_place_until_put_action() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let payload = BASE64.encode([0, 255, 0, 255]);
        let upload = format!("\x1b_Ga=t,f=32,s=1,v=1,i=42;{payload}\x1b\\");
        let uploaded = ingress.advance(upload.as_bytes(), cursor());

        assert!(uploaded.terminal_bytes.is_empty());
        assert!(uploaded.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::ImageReady(TerminalImageData {
                    id: TerminalImageId(42),
                    ..
                })
            )
        }));
        assert!(!uploaded
            .events
            .iter()
            .any(|event| matches!(event, TerminalGraphicsEvent::Place(_))));

        let placed = ingress.advance(b"\x1b_Ga=p,i=42,c=3,r=2,z=4\x1b\\", cursor());
        assert_eq!(placed.terminal_bytes, b"   \r\n   ");
        assert!(placed.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::Place(TerminalImagePlacement {
                    id: TerminalImageId(42),
                    cols: 3,
                    rows: 2,
                    z_index: 4,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_animation_frame_upload_and_control_update_image() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let base_payload = BASE64.encode([255, 0, 0, 255]);
        let upload = format!("\x1b_Ga=t,f=32,s=1,v=1,i=42;{base_payload}\x1b\\");
        ingress.advance(upload.as_bytes(), cursor());

        let frame_payload = BASE64.encode([0, 255, 0, 255]);
        let frame = format!("\x1b_Ga=f,f=32,s=1,v=1,i=42,z=80;{frame_payload}\x1b\\");
        let frame_result = ingress.advance(frame.as_bytes(), cursor());
        let updated = frame_result
            .events
            .iter()
            .find_map(|event| match event {
                TerminalGraphicsEvent::ImageUpdated(image) => Some(image),
                _ => None,
            })
            .expect("animation frame update");

        assert_eq!(updated.frames.len(), 2);
        assert!(updated.frames[0].gapless);
        assert_eq!(updated.frames[1].rgba.as_ref(), &[0, 255, 0, 255]);
        assert_eq!(updated.frames[1].delay_ms_numerator, 80);

        let control = ingress.advance(b"\x1b_Ga=a,i=42,s=3,v=2\x1b\\", cursor());
        let controlled = control
            .events
            .iter()
            .find_map(|event| match event {
                TerminalGraphicsEvent::ImageUpdated(image) => Some(image),
                _ => None,
            })
            .expect("animation control update");
        assert!(controlled.animation.running);
        assert!(!controlled.animation.loading);
        assert_eq!(controlled.animation.loop_limit, Some(2));
    }

    #[test]
    fn kitty_animation_composes_frame_rectangles() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let base_payload = BASE64.encode([255, 0, 0, 255, 0, 0, 255, 255]);
        let upload = format!("\x1b_Ga=t,f=32,s=2,v=1,i=51;{base_payload}\x1b\\");
        ingress.advance(upload.as_bytes(), cursor());

        let overlay_payload = BASE64.encode([0, 255, 0, 255]);
        let frame = format!("\x1b_Ga=f,f=32,s=1,v=1,i=51,x=1,y=0,c=1,X=1;{overlay_payload}\x1b\\");
        ingress.advance(frame.as_bytes(), cursor());

        let composed = ingress.advance(
            b"\x1b_Ga=c,i=51,r=2,c=1,x=0,y=0,X=0,Y=0,w=2,h=1,C=1\x1b\\",
            cursor(),
        );
        let image = composed
            .events
            .iter()
            .find_map(|event| match event {
                TerminalGraphicsEvent::ImageUpdated(image) => Some(image),
                _ => None,
            })
            .expect("composition update");

        assert_eq!(image.rgba.as_ref(), &[255, 0, 0, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn kitty_yazi_kgp_old_upload_uses_no_cursor_movement() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let payload = BASE64.encode([0, 0, 0, 255, 255, 255]);
        let seq = format!("\x1b_Gq=2,a=T,z=-1,C=1,f=24,s=2,v=1,m=0;{payload}\x1b\\");
        let result = ingress.advance(seq.as_bytes(), cursor());

        assert!(result.terminal_bytes.is_empty());
        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::ImageReady(TerminalImageData {
                    width: 2,
                    height: 1,
                    protocol: TerminalImageProtocol::Kitty,
                    ..
                })
            )
        }));
        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::Place(TerminalImagePlacement {
                    row: 0,
                    col: 0,
                    cols: 1,
                    rows: 1,
                    z_index: -1,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_display_geometry_source_rect_and_z_index_are_preserved() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let payload = BASE64.encode([
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]);
        let seq = format!(
            "\x1b_Ga=T,f=32,s=2,v=2,i=77,x=1,y=0,w=1,h=2,c=4,r=2,z=-3;{payload}\x1b\\"
        );
        let result = ingress.advance(seq.as_bytes(), cursor());

        assert_eq!(result.terminal_bytes, b"    \r\n    ");
        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::Place(TerminalImagePlacement {
                    id: TerminalImageId(77),
                    cols: 4,
                    rows: 2,
                    source_x: 1,
                    source_y: 0,
                    source_width: 1,
                    source_height: 2,
                    z_index: -3,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_yazi_kgp_old_chunked_upload_without_explicit_id_completes() {
        let raw = [0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 255, 0];
        let payload = BASE64.encode(raw);
        let split = payload.len() / 2;
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());

        let first = format!(
            "\x1b_Gq=2,a=T,z=-1,C=1,f=24,s=2,v=2,m=1;{}\x1b\\",
            &payload[..split]
        );
        let second = format!("\x1b_Gm=0;{}\x1b\\", &payload[split..]);

        assert!(
            ingress
                .advance(first.as_bytes(), cursor())
                .events
                .is_empty()
        );
        let result = ingress.advance(second.as_bytes(), cursor());

        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::ImageReady(TerminalImageData {
                    id: TerminalImageId(1),
                    width: 2,
                    height: 2,
                    protocol: TerminalImageProtocol::Kitty,
                    ..
                })
            )
        }));
        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::Place(TerminalImagePlacement {
                    id: TerminalImageId(1),
                    row: 0,
                    col: 0,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_yazi_kgp_old_delete_without_payload_clears_all_images() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"\x1b_Gq=2,a=d,d=A\x1b\\", cursor());

        assert_eq!(
            result.events,
            vec![TerminalGraphicsEvent::Delete { id: None }]
        );
        assert!(result.terminal_bytes.is_empty());
    }

    #[test]
    fn advance_with_anchors_image_after_preceding_terminal_text() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let payload = BASE64.encode([0, 255, 0, 255]);
        let seq = format!("abc\x1b_Ga=T,f=32,s=1,v=1,i=42;{payload}\x1b\\xyz");
        let mut terminal_bytes = Vec::new();
        let col = std::cell::Cell::new(0usize);
        let events = ingress.advance_with(
            seq.as_bytes(),
            |bytes| {
                col.set(
                    col.get()
                        + bytes
                            .iter()
                            .filter(|byte| !matches!(byte, b'\r' | b'\n'))
                            .count(),
                );
                terminal_bytes.extend_from_slice(bytes);
            },
            || GraphicsCursor {
                col: col.get(),
                ..cursor()
            },
        );

        let placement = events
            .iter()
            .find_map(|event| match event {
                TerminalGraphicsEvent::Place(placement) => Some(placement),
                _ => None,
            })
            .expect("image placement");
        assert_eq!(placement.col, 3);
        assert!(terminal_bytes.starts_with(b"abc "));
        assert!(terminal_bytes.ends_with(b"xyz"));
    }

    #[test]
    fn kitty_chunked_png_waits_until_final_chunk() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([0, 0, 255, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let payload = BASE64.encode(bytes);
        let split = payload.len() / 2;
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let first = format!("\x1b_Ga=T,f=100,i=7,m=1;{}\x1b\\", &payload[..split]);
        let second = format!("\x1b_Ga=T,f=100,i=7,m=0;{}\x1b\\", &payload[split..]);

        let first = ingress.advance(first.as_bytes(), cursor());
        assert!(first.events.is_empty());

        let second = ingress.advance(second.as_bytes(), cursor());
        assert!(
            second
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    }

    #[test]
    fn kitty_file_transmission_decodes_image_from_path() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        let path = std::env::temp_dir().join(format!(
            "oxideterm-kitty-file-{}-图片.png",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();

        let payload = BASE64.encode(path.to_string_lossy().as_bytes());
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let seq = format!("\x1b_Ga=T,t=f,f=100,i=12;{payload}\x1b\\");
        let result = ingress.advance(seq.as_bytes(), cursor());

        let _ = std::fs::remove_file(path);

        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::ImageReady(TerminalImageData {
                    id: TerminalImageId(12),
                    protocol: TerminalImageProtocol::Kitty,
                    width: 1,
                    height: 1,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_delete_and_query_emit_control_events() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());

        let delete = ingress.advance(b"\x1b_Ga=d,i=9;\x1b\\", cursor());
        assert_eq!(
            delete.events,
            vec![TerminalGraphicsEvent::Delete {
                id: Some(TerminalImageId(9))
            }]
        );

        let query = ingress.advance(b"\x1b_Ga=q,i=9;\x1b\\", cursor());
        assert_eq!(
            query.events,
            vec![TerminalGraphicsEvent::Respond(
                b"\x1b_Gi=9;OK\x1b\\".to_vec()
            )]
        );
    }

    #[test]
    fn sixel_sequence_is_decoded() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"\x1bPq#0;2;100;0;0#0~-\x1b\\", cursor());

        assert!(result.terminal_bytes.contains(&b' '));
        assert!(
            result
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    }

    #[test]
    fn utf8_continuation_bytes_are_not_treated_as_c1_graphics_controls() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let text = "❯ 2025-2026春季毕设安排.pdf";
        let result = ingress.advance(text.as_bytes(), cursor());

        assert_eq!(result.terminal_bytes, text.as_bytes());
        assert!(result.events.is_empty());
    }

    #[test]
    fn eight_bit_c1_graphics_starters_pass_through_to_terminal_parser() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([255, 255, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let payload = BASE64.encode(bytes);
        let mut seq = b"\x9d1337;File=inline=1:".to_vec();
        seq.extend_from_slice(payload.as_bytes());
        seq.push(0x9c);
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(&seq, cursor());

        assert_eq!(result.terminal_bytes, seq);
        assert!(result.events.is_empty());
    }

    fn ingress_advance_chunks(bytes: &[u8]) -> GraphicsAdvance {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let mid = bytes.len() / 2;
        let mut first = ingress.advance(&bytes[..mid], cursor());
        let second = ingress.advance(&bytes[mid..], cursor());
        first.terminal_bytes.extend(second.terminal_bytes);
        first.events.extend(second.events);
        first
    }
}
