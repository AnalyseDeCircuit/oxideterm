use super::*;
use flate2::{Compression, write::ZlibEncoder};
use std::io::Cursor;

fn only_frame_update(change: Option<VncFramebufferChange>) -> RemoteDesktopFrameUpdate {
    let Some(VncFramebufferChange::Updates(mut updates)) = change else {
        panic!("expected one incremental framebuffer update");
    };
    assert_eq!(updates.len(), 1);
    updates.remove(0)
}

fn apply_frame_updates(target: &mut [u8], target_width: u32, updates: &[RemoteDesktopFrameUpdate]) {
    for update in updates {
        for row in 0..update.rect.height as usize {
            let source_start = row * update.rect.width as usize * 4;
            let source_end = source_start + update.rect.width as usize * 4;
            let target_start = ((update.rect.y as usize + row) * target_width as usize
                + update.rect.x as usize)
                * 4;
            let target_end = target_start + update.rect.width as usize * 4;
            target[target_start..target_end]
                .copy_from_slice(&update.bytes[source_start..source_end]);
        }
    }
}

#[test]
fn framebuffer_draws_bgra_rect() {
    let mut framebuffer = VncFramebuffer::new(2, 2);
    let rect = RfbRect {
        x: 1,
        y: 0,
        width: 1,
        height: 2,
    };

    let update = only_frame_update(framebuffer.apply(VncServerEvent::RawImage(
        rect,
        vec![1, 2, 3, 255, 4, 5, 6, 255],
    )));

    assert_eq!(update.rect, RemoteDesktopRect::new(1, 0, 1, 2));
    assert_eq!(update.bytes, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    assert_eq!(
        framebuffer.frame().bytes,
        vec![0, 0, 0, 255, 1, 2, 3, 255, 0, 0, 0, 255, 4, 5, 6, 255]
    );
}

#[test]
fn framebuffer_treats_raw_padding_as_opaque_alpha() {
    let mut framebuffer = VncFramebuffer::new(1, 1);
    let rect = RfbRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };

    let _ = framebuffer.apply(VncServerEvent::RawImage(rect, vec![1, 2, 3, 0]));

    assert_eq!(framebuffer.frame().bytes, vec![1, 2, 3, 255]);
    assert_eq!(
        framebuffer.frame_update(rect).unwrap().bytes,
        vec![1, 2, 3, 255]
    );
}

#[test]
fn framebuffer_copies_rect_without_overlapping_corruption() {
    let mut framebuffer = VncFramebuffer::new(3, 1);
    let _ = framebuffer.apply(VncServerEvent::RawImage(
        RfbRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        },
        vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255],
    ));
    let dst = RfbRect {
        x: 1,
        y: 0,
        width: 2,
        height: 1,
    };

    let update = only_frame_update(framebuffer.apply(VncServerEvent::CopyRect {
        dst,
        src_x: 0,
        src_y: 0,
    }));

    assert_eq!(update.rect, RemoteDesktopRect::new(1, 0, 2, 1));
    assert_eq!(update.bytes, vec![1, 0, 0, 255, 2, 0, 0, 255]);
    assert_eq!(
        framebuffer.frame().bytes,
        vec![1, 0, 0, 255, 1, 0, 0, 255, 2, 0, 0, 255]
    );
}

#[test]
fn framebuffer_update_contains_only_changed_rect() {
    let mut framebuffer = VncFramebuffer::new(3, 2);
    let rect = RfbRect {
        x: 1,
        y: 1,
        width: 2,
        height: 1,
    };

    let update = only_frame_update(framebuffer.apply(VncServerEvent::RawImage(
        rect,
        vec![7, 8, 9, 255, 10, 11, 12, 255],
    )));
    assert_eq!(
        update.size,
        RemoteDesktopSize {
            width: 3,
            height: 2,
        }
    );
    assert_eq!(update.rect, RemoteDesktopRect::new(1, 1, 2, 1));
    assert_eq!(update.bytes, vec![7, 8, 9, 255, 10, 11, 12, 255]);
}

#[test]
fn framebuffer_keeps_sparse_batch_regions_separate() {
    let mut framebuffer = VncFramebuffer::new(100, 100);
    let change = framebuffer
        .apply(VncServerEvent::Batch(vec![
            VncServerEvent::RawImage(
                RfbRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                vec![1, 2, 3, 255],
            ),
            VncServerEvent::RawImage(
                RfbRect {
                    x: 99,
                    y: 99,
                    width: 1,
                    height: 1,
                },
                vec![4, 5, 6, 255],
            ),
        ]))
        .unwrap();
    let mut sent_initial_frame = true;

    let event = vnc_frame_event_for_change(&framebuffer, change, &mut sent_initial_frame, false);

    let RemoteDesktopHelperEvent::FrameUpdateBatch { batch } = event else {
        panic!("expected sparse framebuffer update batch");
    };
    assert_eq!(batch.updates.len(), 2);
    assert_eq!(batch.byte_len(), 8);
    assert_eq!(batch.updates[0].rect, RemoteDesktopRect::new(0, 0, 1, 1));
    assert_eq!(batch.updates[1].rect, RemoteDesktopRect::new(99, 99, 1, 1));
}

#[test]
fn framebuffer_bounds_large_sparse_batches_without_losing_pixels() {
    let mut framebuffer = VncFramebuffer::new(64, 4);
    let events = (0..=VNC_MAX_FRAME_UPDATE_REGIONS)
        .map(|index| {
            VncServerEvent::RawImage(
                RfbRect {
                    x: (index * 3) as u16,
                    y: 1,
                    width: 1,
                    height: 1,
                },
                vec![index as u8, 20, 30, 255],
            )
        })
        .collect();

    let Some(VncFramebufferChange::Updates(updates)) =
        framebuffer.apply(VncServerEvent::Batch(events))
    else {
        panic!("expected bounded incremental updates");
    };

    assert!(updates.len() <= VNC_MAX_FRAME_UPDATE_REGIONS);
    let mut reconstructed = VncFramebuffer::new(64, 4).frame().bytes;
    apply_frame_updates(&mut reconstructed, 64, &updates);
    assert_eq!(reconstructed, framebuffer.frame().bytes);
}

#[test]
fn set_encodings_prefers_zrle_and_hextile_before_raw() {
    let preferences = VncEncodingPreferences::default();
    let advertised = advertised_vnc_encodings(preferences, false);
    let message = set_encodings_message(preferences, false);
    assert_eq!(
        &message[0..4],
        &[2, 0, (advertised.len() >> 8) as u8, advertised.len() as u8,]
    );

    let encodings = message[4..].chunks_exact(4).map(be_i32).collect::<Vec<_>>();
    assert_eq!(encodings, advertised);
    assert!(encodings.contains(&VNC_ENCODING_EXTENDED_DESKTOP_SIZE));
    assert!(encodings.contains(&VNC_ENCODING_QEMU_AUDIO));
    assert!(encodings.contains(&VNC_ENCODING_QEMU_EXTENDED_KEY_EVENT));
    assert!(encodings.contains(&VNC_ENCODING_QEMU_LED_STATE));
    assert!(encodings.contains(&VNC_ENCODING_VMWARE_LED_STATE));
    assert!(encodings.contains(&VNC_ENCODING_EXTENDED_MOUSE_BUTTONS));
    let zrle = encodings
        .iter()
        .position(|encoding| *encoding == VNC_ENCODING_ZRLE)
        .unwrap();
    let hextile = encodings
        .iter()
        .position(|encoding| *encoding == VNC_ENCODING_HEXTILE)
        .unwrap();
    let raw = encodings
        .iter()
        .position(|encoding| *encoding == VNC_ENCODING_RAW)
        .unwrap();
    assert!(zrle < hextile && hextile < raw);
}

#[test]
fn last_rect_stops_an_unknown_length_framebuffer_update() {
    let mut payload = vec![0, 0xff, 0xff];
    payload.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    push_be_i32(&mut payload, VNC_ENCODING_LAST_RECT);
    payload.push(0xaa);
    let mut reader = Cursor::new(payload);

    let event = read_framebuffer_update(&mut reader, &mut VncDecodeState::default()).unwrap();

    assert_eq!(
        event,
        VncServerEvent::Batch(vec![VncServerEvent::ObservedCapability(
            VncObservedCapability::LastRect
        )])
    );
    assert_eq!(read_u8(&mut reader).unwrap(), 0xaa);
}

#[test]
fn hextile_background_and_colored_subrect_decode_to_raw_rect() {
    let mut payload = vec![
        VNC_HEXTILE_BACKGROUND_SPECIFIED | VNC_HEXTILE_ANY_SUBRECTS | VNC_HEXTILE_SUBRECTS_COLORED,
        1,
        2,
        3,
        0,
        1,
        9,
        8,
        7,
        0,
        0x10,
        0x00,
    ];
    let mut reader = Cursor::new(payload.split_off(0));

    let bytes = read_hextile_rect(
        &mut reader,
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
    )
    .unwrap();

    assert_eq!(bytes, vec![1, 2, 3, 0, 9, 8, 7, 0, 1, 2, 3, 0, 1, 2, 3, 0]);
}

#[test]
fn hextile_raw_tile_decodes_without_background_state() {
    let mut payload = vec![VNC_HEXTILE_RAW, 1, 2, 3, 0, 4, 5, 6, 0];
    let mut reader = Cursor::new(payload.split_off(0));

    let bytes = read_hextile_rect(
        &mut reader,
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    )
    .unwrap();

    assert_eq!(bytes, vec![1, 2, 3, 0, 4, 5, 6, 0]);
}

#[test]
fn hextile_rejects_out_of_bounds_subrect() {
    let mut payload = vec![
        VNC_HEXTILE_BACKGROUND_SPECIFIED | VNC_HEXTILE_ANY_SUBRECTS | VNC_HEXTILE_SUBRECTS_COLORED,
        1,
        2,
        3,
        0,
        1,
        9,
        8,
        7,
        0,
        0x10,
        0x10,
    ];
    let mut reader = Cursor::new(payload.split_off(0));

    let error = read_hextile_rect(
        &mut reader,
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    )
    .unwrap_err();

    assert!(error.contains("subrect exceeds"));
}

#[test]
fn zrle_raw_tile_decodes_compact_pixels() {
    let bytes = decode_trle_rect(
        &[VNC_TRLE_RAW, 1, 2, 3, 4, 5, 6],
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    )
    .unwrap();

    assert_eq!(bytes, vec![1, 2, 3, 0, 4, 5, 6, 0]);
}

#[test]
fn zrle_packed_palette_decodes_bit_indices() {
    let bytes = decode_trle_rect(
        &[2, 1, 2, 3, 9, 8, 7, 0b0100_0000],
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    )
    .unwrap();

    assert_eq!(bytes, vec![1, 2, 3, 0, 9, 8, 7, 0]);
}

#[test]
fn zrle_plain_rle_decodes_run_lengths() {
    let bytes = decode_trle_rect(
        &[VNC_TRLE_PLAIN_RLE, 7, 8, 9, 2],
        RfbRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        },
    )
    .unwrap();

    assert_eq!(bytes, vec![7, 8, 9, 0, 7, 8, 9, 0, 7, 8, 9, 0]);
}

#[test]
fn zrle_palette_rle_decodes_single_pixels_and_runs() {
    let bytes = decode_trle_rect(
        &[130, 1, 2, 3, 9, 8, 7, 0, 0x81, 1],
        RfbRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        },
    )
    .unwrap();

    assert_eq!(bytes, vec![1, 2, 3, 0, 9, 8, 7, 0, 9, 8, 7, 0]);
}

#[test]
fn zrle_rectangle_inflates_persistent_zlib_stream() {
    let trle = [VNC_TRLE_RAW, 1, 2, 3, 4, 5, 6];
    let compressed = zlib_payload(&trle);
    let mut payload = Vec::new();
    push_be_u32(&mut payload, compressed.len() as u32);
    payload.extend_from_slice(&compressed);
    let mut reader = Cursor::new(payload);
    let mut decode_state = VncDecodeState::default();

    let bytes = read_zrle_rect(
        &mut reader,
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        &mut decode_state,
    )
    .unwrap();

    assert_eq!(bytes, vec![1, 2, 3, 0, 4, 5, 6, 0]);
}

#[test]
fn zrle_inflater_grows_beyond_initial_output_capacity() {
    // Large ZRLE rectangles can legitimately expand beyond the inflater's
    // small initial allocation even when the compressed payload is tiny.
    let expanded = vec![0x5a; 96 * 1024];
    let compressed = zlib_payload(&expanded);

    let output =
        inflate_zrle_payload(&mut Decompress::new(true), &compressed, expanded.len()).unwrap();

    assert_eq!(output.len(), expanded.len());
    assert!(output.iter().all(|byte| *byte == 0x5a));
}

#[test]
fn zrle_inflater_rejects_output_beyond_limit() {
    let expanded = vec![0x3c; 96 * 1024];
    let compressed = zlib_payload(&expanded);

    let error = inflate_zrle_payload(&mut Decompress::new(true), &compressed, expanded.len() - 1)
        .unwrap_err();

    assert!(error.contains("expanded beyond"));
}

#[test]
fn framebuffer_update_request_message_uses_incremental_flag() {
    assert_eq!(
        framebuffer_update_request_message(false, 800, 600),
        vec![3, 0, 0, 0, 0, 0, 3, 32, 2, 88]
    );
    assert_eq!(
        framebuffer_update_request_message(true, 800, 600),
        vec![3, 1, 0, 0, 0, 0, 3, 32, 2, 88]
    );
}

#[test]
fn set_desktop_size_message_encodes_complete_screen_topology() {
    let layout = VncDesktopLayout {
        width: 1920,
        height: 1080,
        screens: vec![
            VncDesktopScreen {
                id: 1,
                x: 0,
                y: 0,
                width: 1280,
                height: 1024,
                flags: 0,
            },
            VncDesktopScreen {
                id: 2,
                x: 1280,
                y: 0,
                width: 640,
                height: 1080,
                flags: 0,
            },
        ],
    };

    assert_eq!(
        set_desktop_size_message(&layout).unwrap(),
        vec![
            251, 0, 7, 128, 4, 56, 2, 0, // SetDesktopSize header.
            0, 0, 0, 1, 0, 0, 0, 0, 5, 0, 4, 0, 0, 0, 0, 0, // First screen.
            0, 0, 0, 2, 5, 0, 0, 0, 2, 128, 4, 56, 0, 0, 0, 0, // Second screen.
        ]
    );
}

#[test]
fn extended_desktop_size_decodes_result_and_server_layout() {
    let mut payload = Cursor::new(vec![
        2, 0, 0, 0, // Screen count and padding.
        0, 0, 0, 1, 0, 0, 0, 0, 5, 0, 4, 0, 0, 0, 0, 0, // First screen.
        0, 0, 0, 2, 5, 0, 0, 0, 2, 128, 4, 56, 0, 0, 0, 0, // Second screen.
    ]);

    let update = read_extended_desktop_size(
        &mut payload,
        RfbRect {
            x: 1,
            y: 3,
            width: 1920,
            height: 1080,
        },
    )
    .unwrap();

    assert_eq!(update.reason, VncDesktopSizeReason::Client);
    assert_eq!(update.result, VncDesktopSizeResult::InvalidScreenLayout);
    assert_eq!(update.layout.screens.len(), 2);
    assert!(!update.applies_layout());
}

#[test]
fn framebuffer_update_decodes_extended_desktop_size_pseudo_rectangle() {
    let mut payload = vec![
        0, 0, 1, // FramebufferUpdate padding and rectangle count.
        0, 0, 0, 0, 3, 32, 2, 88, // Reason, result, width, and height.
    ];
    payload.extend_from_slice(&VNC_ENCODING_EXTENDED_DESKTOP_SIZE.to_be_bytes());
    payload.extend_from_slice(&[
        1, 0, 0, 0, // Screen count and padding.
        0, 0, 0, 7, 0, 0, 0, 0, 3, 32, 2, 88, 0, 0, 0, 0,
    ]);

    let event =
        read_framebuffer_update(&mut Cursor::new(payload), &mut VncDecodeState::default()).unwrap();

    let VncServerEvent::Batch(events) = event else {
        panic!("expected framebuffer update batch");
    };
    let VncServerEvent::ExtendedDesktopSize(update) = &events[0] else {
        panic!("expected ExtendedDesktopSize event");
    };
    assert_eq!(update.reason, VncDesktopSizeReason::Server);
    assert_eq!(update.result, VncDesktopSizeResult::Success);
    assert_eq!((update.layout.width, update.layout.height), (800, 600));
    assert_eq!(update.layout.screens[0].id, 7);
}

#[test]
fn remote_monitor_layout_normalizes_negative_coordinates_for_rfb() {
    let layout = oxideterm_remote_desktop::RemoteDesktopMonitorLayout {
        monitors: vec![
            remote_monitor("primary", 0, 0, 1920, 1080, true),
            remote_monitor("left", -1280, 0, 1280, 1024, false),
        ],
    };

    let normalized = VncDesktopLayout::from_remote_layout(&layout).unwrap();

    assert_eq!((normalized.width, normalized.height), (3200, 1080));
    assert_eq!(
        (normalized.screens[0].x, normalized.screens[0].y),
        (1280, 0)
    );
    assert_eq!((normalized.screens[1].x, normalized.screens[1].y), (0, 0));
    assert_eq!(normalized.screens[0].flags, 0);
}

#[test]
fn initial_desktop_layout_honors_use_all_monitors() {
    let size = RemoteDesktopSize {
        width: 1024,
        height: 768,
    };
    let monitors = oxideterm_remote_desktop::RemoteDesktopMonitorLayout {
        monitors: vec![
            remote_monitor("primary", 0, 0, 3840, 2160, true),
            remote_monitor("right", 3840, 0, 3840, 2160, false),
            remote_monitor("far-right", 7680, 0, 3840, 2160, false),
        ],
    };

    let single = initial_vnc_desktop_layout(size, false, &monitors).unwrap();
    let extended = initial_vnc_desktop_layout(size, true, &monitors).unwrap();

    assert_eq!(
        (single.width, single.height, single.screens.len()),
        (1024, 768, 1)
    );
    assert_eq!(
        (extended.width, extended.height, extended.screens.len()),
        (11520, 2160, 3)
    );
}

#[test]
fn remote_monitor_layout_rejects_primary_overlap_and_framebuffer_limit() {
    let no_primary = oxideterm_remote_desktop::RemoteDesktopMonitorLayout {
        monitors: vec![remote_monitor("only", 0, 0, 800, 600, false)],
    };
    assert!(
        VncDesktopLayout::from_remote_layout(&no_primary)
            .unwrap_err()
            .contains("exactly one primary")
    );

    let multiple_primary = oxideterm_remote_desktop::RemoteDesktopMonitorLayout {
        monitors: vec![
            remote_monitor("first", 0, 0, 800, 600, true),
            remote_monitor("second", 800, 0, 800, 600, true),
        ],
    };
    assert!(
        VncDesktopLayout::from_remote_layout(&multiple_primary)
            .unwrap_err()
            .contains("exactly one primary")
    );

    let overlapping = oxideterm_remote_desktop::RemoteDesktopMonitorLayout {
        monitors: vec![
            remote_monitor("primary", 0, 0, 800, 600, true),
            remote_monitor("overlap", 400, 0, 800, 600, false),
        ],
    };
    assert!(
        VncDesktopLayout::from_remote_layout(&overlapping)
            .unwrap_err()
            .contains("must not overlap")
    );

    assert!(
        VncDesktopLayout::single(RemoteDesktopSize {
            width: u16::MAX.into(),
            height: u16::MAX.into(),
        })
        .unwrap_err()
        .contains("memory limit")
    );
}

#[test]
fn wire_desktop_layout_rejects_duplicate_ids_and_out_of_bounds_screens() {
    let duplicate_ids = VncDesktopLayout {
        width: 1600,
        height: 600,
        screens: vec![
            VncDesktopScreen {
                id: 7,
                x: 0,
                y: 0,
                width: 800,
                height: 600,
                flags: 0,
            },
            VncDesktopScreen {
                id: 7,
                x: 800,
                y: 0,
                width: 800,
                height: 600,
                flags: 0,
            },
        ],
    };
    assert!(
        duplicate_ids
            .validate_wire_layout()
            .unwrap_err()
            .contains("identifiers must be unique")
    );

    let out_of_bounds = VncDesktopLayout {
        width: 800,
        height: 600,
        screens: vec![VncDesktopScreen {
            id: 1,
            x: 400,
            y: 0,
            width: 800,
            height: 600,
            flags: 0,
        }],
    };
    assert!(
        out_of_bounds
            .validate_wire_layout()
            .unwrap_err()
            .contains("outside the framebuffer")
    );
}

#[test]
fn desktop_resize_waits_for_capability_and_coalesces_requests() {
    let initial = VncDesktopLayout::single(RemoteDesktopSize {
        width: 800,
        height: 600,
    })
    .unwrap();
    let server = VncDesktopLayout::single(RemoteDesktopSize {
        width: 1024,
        height: 768,
    })
    .unwrap();
    let mut state = VncDesktopResizeState::new(initial);
    let capability = VncServerEvent::Batch(vec![VncServerEvent::ExtendedDesktopSize(
        VncExtendedDesktopSize {
            reason: VncDesktopSizeReason::Server,
            result: VncDesktopSizeResult::Success,
            layout: server.clone(),
        },
    )]);

    let transition = state.observe_framebuffer_update(&capability).unwrap();
    assert_eq!(
        transition.capability_changed,
        Some(NegotiatedCapabilityStatus::Supported)
    );
    assert!(transition.next_request.is_some());
    assert_eq!(state.server_layout(), Some(&server));

    let latest = VncDesktopLayout::single(RemoteDesktopSize {
        width: 1280,
        height: 720,
    })
    .unwrap();
    assert!(state.queue_layout(latest.clone()).unwrap().is_none());
    let response = VncServerEvent::Batch(vec![VncServerEvent::ExtendedDesktopSize(
        VncExtendedDesktopSize {
            reason: VncDesktopSizeReason::Client,
            result: VncDesktopSizeResult::Success,
            layout: server,
        },
    )]);

    let transition = state.observe_framebuffer_update(&response).unwrap();
    assert_eq!(
        transition.next_request,
        Some(set_desktop_size_message(&latest).unwrap())
    );
}

#[test]
fn desktop_resize_remains_unknown_without_extension_evidence() {
    let initial = VncDesktopLayout::single(RemoteDesktopSize {
        width: 800,
        height: 600,
    })
    .unwrap();
    let mut state = VncDesktopResizeState::new(initial.clone());

    let transition = state
        .observe_framebuffer_update(&VncServerEvent::Batch(vec![VncServerEvent::Noop]))
        .unwrap();

    assert_eq!(transition.capability_changed, None);
    assert_eq!(transition.rejection, None);
    assert!(
        state
            .queue_layout(initial.clone())
            .unwrap_err()
            .contains("has not negotiated")
    );

    let explicit_refusal = VncServerEvent::Batch(vec![VncServerEvent::ExtendedDesktopSize(
        VncExtendedDesktopSize {
            reason: VncDesktopSizeReason::Client,
            result: VncDesktopSizeResult::ResizeProhibited,
            layout: initial.clone(),
        },
    )]);
    let transition = state.observe_framebuffer_update(&explicit_refusal).unwrap();

    assert_eq!(
        transition.capability_changed,
        Some(NegotiatedCapabilityStatus::Unsupported)
    );
    assert!(transition.rejection.unwrap().contains("does not permit"));
    assert!(
        state
            .queue_layout(initial)
            .unwrap_err()
            .contains("did not negotiate")
    );
}

#[test]
fn forced_vnc_recovery_promotes_dirty_rect_to_base_frame() {
    let mut framebuffer = VncFramebuffer::new(2, 2);
    let rect = RfbRect {
        x: 1,
        y: 1,
        width: 1,
        height: 1,
    };
    let change = framebuffer
        .apply(VncServerEvent::RawImage(rect, vec![9, 8, 7, 255]))
        .unwrap();
    let mut sent_initial_frame = true;

    let event = vnc_frame_event_for_change(&framebuffer, change, &mut sent_initial_frame, true);

    match event {
        RemoteDesktopHelperEvent::Frame { frame } => {
            assert_eq!(
                frame.size,
                RemoteDesktopSize {
                    width: 2,
                    height: 2,
                }
            );
            assert_eq!(frame.bytes.len(), 16);
        }
        other => panic!("expected forced base frame, got {other:?}"),
    }
    assert!(sent_initial_frame);
}

#[test]
fn ordinary_vnc_dirty_rect_stays_incremental_after_base_frame() {
    let mut framebuffer = VncFramebuffer::new(2, 2);
    let rect = RfbRect {
        x: 1,
        y: 1,
        width: 1,
        height: 1,
    };
    let change = framebuffer
        .apply(VncServerEvent::RawImage(rect, vec![9, 8, 7, 255]))
        .unwrap();
    let mut sent_initial_frame = true;

    let event = vnc_frame_event_for_change(&framebuffer, change, &mut sent_initial_frame, false);

    match event {
        RemoteDesktopHelperEvent::FrameUpdate { update } => {
            assert_eq!(update.rect, RemoteDesktopRect::new(1, 1, 1, 1));
            assert_eq!(update.bytes, vec![9, 8, 7, 255]);
        }
        other => panic!("expected dirty update, got {other:?}"),
    }
    assert!(sent_initial_frame);
}

#[test]
fn rich_cursor_applies_visibility_mask_to_alpha() {
    let event = rich_cursor_event(
        RfbRect {
            x: 1,
            y: 0,
            width: 2,
            height: 1,
        },
        vec![10, 20, 30, 0, 40, 50, 60, 0],
        &[0b1000_0000],
    )
    .unwrap();

    let VncServerEvent::CursorShape(shape) = event else {
        panic!("expected cursor shape");
    };
    assert_eq!(
        shape,
        RemoteDesktopCursorShape::new(
            RemoteDesktopSize {
                width: 2,
                height: 1,
            },
            1,
            0,
            RemoteDesktopFrameFormat::Bgra8,
            vec![10, 20, 30, 255, 40, 50, 60, 0],
        )
    );
}

#[test]
fn x_cursor_expands_bitmap_and_mask_to_bgra_pixels() {
    let event = x_cursor_event(
        RfbRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        [0x30, 0x20, 0x10, 0x03, 0x02, 0x01],
        &[0b1000_0000],
        &[0b1100_0000],
    )
    .unwrap();

    let VncServerEvent::CursorShape(shape) = event else {
        panic!("expected cursor shape");
    };
    assert_eq!(
        shape.bytes,
        vec![0x10, 0x20, 0x30, 255, 0x01, 0x02, 0x03, 255]
    );
    assert_eq!(shape.hotspot_x, 0);
    assert_eq!(shape.hotspot_y, 0);
}

#[test]
fn batch_exposes_nested_cursor_helper_events() {
    let shape = RemoteDesktopCursorShape::new(
        RemoteDesktopSize {
            width: 1,
            height: 1,
        },
        0,
        0,
        RemoteDesktopFrameFormat::Bgra8,
        vec![1, 2, 3, 255],
    );

    assert_eq!(
        vnc_helper_events(&VncServerEvent::Batch(vec![
            VncServerEvent::ClipboardText("copied".to_string()),
            VncServerEvent::CursorShape(shape.clone()),
            VncServerEvent::CursorHidden,
        ])),
        vec![
            RemoteDesktopHelperEvent::ClipboardText {
                text: "copied".to_string(),
            },
            RemoteDesktopHelperEvent::CursorShape { shape },
            RemoteDesktopHelperEvent::CursorHidden,
        ]
    );
}

#[test]
fn key_mapping_covers_text_physical_keypad_and_special_keys() {
    let cases = [
        ("KeyA", Some("a"), false, 'a' as u32),
        ("KeyV", None, true, 'v' as u32),
        ("Numpad1", Some("1"), false, 0xffb1),
        ("Return", None, false, 0xff0d),
        ("EnterKey", None, false, 0xff0d),
        ("NumpadEnter", None, false, 0xff8d),
        ("KP_Enter", None, false, 0xff8d),
        ("NumpadDivide", None, false, 0xffaf),
        ("Insert", None, false, 0xff63),
        ("ContextMenu", None, false, 0xff67),
        ("PrintScreen", None, false, 0xff61),
        ("NumLock", None, false, 0xff7f),
        ("ScrollLock", None, false, 0xff14),
        ("Pause", None, false, 0xff13),
    ];

    for (code, text, ctrl, expected) in cases {
        let key = RemoteDesktopKey {
            code: code.to_string(),
            text: text.map(str::to_string),
            alt: false,
            ctrl,
            shift: false,
            meta: false,
        };
        assert_eq!(vnc_keysym(&key), Some(expected), "code {code}");
    }
}

#[test]
fn unicode_keysyms_preserve_chinese_and_combining_text() {
    assert_eq!(vnc_unicode_keysym('你'), 0x0100_4f60);
    assert_eq!(
        vnc_text_key_events("你e\u{301}"),
        vec![
            VncKeyEvent {
                keysym: 0x0100_4f60,
                raw_keycode: None,
                down: true,
            },
            VncKeyEvent {
                keysym: 0x0100_4f60,
                raw_keycode: None,
                down: false,
            },
            VncKeyEvent {
                keysym: 'e' as u32,
                raw_keycode: None,
                down: true,
            },
            VncKeyEvent {
                keysym: 'e' as u32,
                raw_keycode: None,
                down: false,
            },
            VncKeyEvent {
                keysym: 0x0100_0301,
                raw_keycode: None,
                down: true,
            },
            VncKeyEvent {
                keysym: 0x0100_0301,
                raw_keycode: None,
                down: false,
            },
        ]
    );
}

#[test]
fn qemu_extended_key_event_uses_physical_xt_keycode() {
    let key = RemoteDesktopKey {
        code: "KeyA".to_string(),
        text: Some("q".to_string()),
        alt: false,
        ctrl: false,
        shift: false,
        meta: false,
    };
    let event = vnc_key_event(&key, true).unwrap();

    assert_eq!(event.keysym, 'q' as u32);
    assert_eq!(event.raw_keycode, Some(0x1e));
    assert_eq!(
        qemu_extended_key_event_message(event).unwrap(),
        vec![255, 0, 0, 1, 0, 0, 0, b'q', 0, 0, 0, 0x1e]
    );
    assert_eq!(vnc_raw_keycode_for_code("ArrowRight"), Some(0xcd));
    assert_eq!(vnc_raw_keycode_for_code("ControlRight"), Some(0x9d));
    assert_eq!(vnc_raw_keycode_for_code("PrintScreen"), Some(0x54));
    assert_eq!(vnc_raw_keycode_for_code("Pause"), Some(0xc6));
}

#[test]
fn unknown_physical_keycode_keeps_standard_keysym_fallback() {
    let event = VncKeyEvent {
        keysym: vnc_unicode_keysym('你'),
        raw_keycode: None,
        down: true,
    };

    assert!(qemu_extended_key_event_message(event).is_none());
    assert_eq!(
        vnc_standard_key_event_message(event.keysym, event.down),
        vec![4, 1, 0, 0, 1, 0, 0x4f, 0x60]
    );
}

#[test]
fn keyboard_mapper_keeps_physical_modifier_pressed_until_release() {
    let mut mapper = VncKeyboardInputMapper::default();
    let control = RemoteDesktopKey {
        code: "ControlRight".to_string(),
        text: None,
        alt: false,
        ctrl: true,
        shift: false,
        meta: false,
    };
    let shortcut = RemoteDesktopKey {
        code: "KeyV".to_string(),
        text: Some("v".to_string()),
        alt: false,
        ctrl: true,
        shift: false,
        meta: false,
    };

    assert_eq!(
        mapper.operations(&control, RemoteDesktopKeyState::Pressed),
        vec![VncKeyEvent {
            keysym: 0xffe4,
            raw_keycode: Some(0x9d),
            down: true,
        }]
    );
    assert_eq!(
        mapper.operations(&shortcut, RemoteDesktopKeyState::Pressed),
        vec![VncKeyEvent {
            keysym: 'v' as u32,
            raw_keycode: Some(0x2f),
            down: true,
        }]
    );
    assert_eq!(
        mapper.operations(&shortcut, RemoteDesktopKeyState::Released),
        vec![VncKeyEvent {
            keysym: 'v' as u32,
            raw_keycode: Some(0x2f),
            down: false,
        }]
    );
}

#[test]
fn keyboard_mapper_release_all_releases_tracked_inputs() {
    let mut mapper = VncKeyboardInputMapper::default();
    let control = RemoteDesktopKey {
        code: "ControlLeft".to_string(),
        text: None,
        alt: false,
        ctrl: true,
        shift: false,
        meta: false,
    };
    let key = RemoteDesktopKey {
        code: "KeyA".to_string(),
        text: Some("a".to_string()),
        alt: false,
        ctrl: false,
        shift: false,
        meta: false,
    };

    let _ = mapper.operations(&control, RemoteDesktopKeyState::Pressed);
    let _ = mapper.operations(&key, RemoteDesktopKeyState::Pressed);
    let released = mapper.release_all_events();

    assert!(released.contains(&VncKeyEvent {
        keysym: 0xffe3,
        raw_keycode: Some(0x1d),
        down: false,
    }));
    assert!(released.contains(&VncKeyEvent {
        keysym: 'a' as u32,
        raw_keycode: Some(0x1e),
        down: false,
    }));
    assert!(mapper.release_all_events().is_empty());
}

#[test]
fn scroll_masks_include_horizontal_wheel_buttons() {
    assert_eq!(
        vnc_scroll_masks(RemoteDesktopWheelDelta {
            x: 120.0,
            y: -240.0
        }),
        vec![VNC_WHEEL_UP, VNC_WHEEL_UP, VNC_WHEEL_RIGHT]
    );
    assert_eq!(
        vnc_scroll_masks(RemoteDesktopWheelDelta { x: -1.0, y: 0.0 }),
        vec![VNC_WHEEL_LEFT]
    );
}

#[test]
fn extended_pointer_event_carries_back_and_forward_buttons() {
    let pressed = VNC_BUTTON_LEFT | VNC_BUTTON_BACK | VNC_BUTTON_FORWARD;

    assert_eq!(
        vnc_pointer_event_message(300, 200, pressed, true),
        vec![5, 0x81, 1, 44, 0, 200, 0x03]
    );
    assert_eq!(
        vnc_pointer_event_message(300, 200, pressed, false),
        // The marker stays clear until the server confirms support.
        vec![5, 0x01, 1, 44, 0, 200]
    );
    assert_eq!(
        vnc_pointer_event_message(300, 200, VNC_BUTTON_LEFT, true),
        vec![5, 0x01, 1, 44, 0, 200]
    );
}

#[test]
fn led_state_drives_only_changed_remote_lock_keys() {
    let current = vnc_lock_keys_from_bits(0b101);
    let target = RemoteDesktopLockKeys {
        scroll_lock: true,
        num_lock: true,
        caps_lock: false,
        kana_lock: true,
    };

    assert_eq!(
        vnc_lock_key_sync_events(current, target),
        vec![
            VncKeyEvent {
                keysym: 0xff7f,
                raw_keycode: Some(0x45),
                down: true,
            },
            VncKeyEvent {
                keysym: 0xff7f,
                raw_keycode: Some(0x45),
                down: false,
            },
            VncKeyEvent {
                keysym: 0xffe5,
                raw_keycode: Some(0x3a),
                down: true,
            },
            VncKeyEvent {
                keysym: 0xffe5,
                raw_keycode: Some(0x3a),
                down: false,
            },
        ]
    );
}

#[test]
fn lock_key_sync_waits_for_the_first_server_led_state() {
    let state = VncSessionSharedState::new(800, 600);
    let (writer, receiver) = std::sync::mpsc::sync_channel(8);
    let target = RemoteDesktopLockKeys {
        scroll_lock: false,
        num_lock: true,
        caps_lock: true,
        kana_lock: false,
    };
    state.store_pending_lock_keys(target);

    flush_pending_vnc_lock_key_sync(&state, &writer).unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));

    state.observe_input_extensions(&VncServerEvent::LockKeys(vnc_lock_keys_from_bits(0)));
    flush_pending_vnc_lock_key_sync(&state, &writer).unwrap();
    let VncIoCommand::Write(message) = receiver.try_recv().unwrap() else {
        panic!("expected a lock-key write");
    };
    assert_eq!(
        message,
        [
            vnc_standard_key_event_message(0xff7f, true),
            vnc_standard_key_event_message(0xff7f, false),
            vnc_standard_key_event_message(0xffe5, true),
            vnc_standard_key_event_message(0xffe5, false),
        ]
        .concat()
    );
    assert_eq!(state.remote_lock_keys(), Some(target));
}

#[test]
fn input_extension_support_stays_unknown_until_server_confirmation() {
    let state = VncSessionSharedState::new(800, 600);
    let capabilities = state.input_extension_capabilities();
    assert_eq!(
        capabilities.extended_key_events,
        NegotiatedCapabilityStatus::Unknown
    );
    assert_eq!(
        capabilities.extended_mouse_buttons,
        NegotiatedCapabilityStatus::Unknown
    );
    assert_eq!(
        capabilities.lock_key_sync,
        NegotiatedCapabilityStatus::Unknown
    );
    assert_eq!(state.remote_lock_keys(), None);

    state.observe_input_extensions(&VncServerEvent::Batch(vec![
        VncServerEvent::QemuExtendedKeyEvents,
        VncServerEvent::ExtendedMouseButtons,
        VncServerEvent::LockKeys(vnc_lock_keys_from_bits(0b010)),
    ]));

    let capabilities = state.input_extension_capabilities();
    assert_eq!(
        capabilities.extended_key_events,
        NegotiatedCapabilityStatus::Supported
    );
    assert_eq!(
        capabilities.extended_mouse_buttons,
        NegotiatedCapabilityStatus::Supported
    );
    assert_eq!(
        capabilities.lock_key_sync,
        NegotiatedCapabilityStatus::Supported
    );
    assert_eq!(
        state.remote_lock_keys(),
        Some(RemoteDesktopLockKeys {
            scroll_lock: false,
            num_lock: true,
            caps_lock: false,
            kana_lock: false,
        })
    );

    let mut cumulative = NegotiatedCapabilities {
        qemu_audio: NegotiatedCapabilityStatus::Supported,
        ..NegotiatedCapabilities::default()
    };
    state.merge_input_extension_capabilities(&mut cumulative);
    assert_eq!(cumulative.qemu_audio, NegotiatedCapabilityStatus::Supported);
}

#[test]
fn framebuffer_update_parses_input_extension_confirmations() {
    let mut payload = vec![0, 0, 4];
    append_rfb_rect_header(
        &mut payload,
        RfbRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        VNC_ENCODING_QEMU_EXTENDED_KEY_EVENT,
    );
    append_rfb_rect_header(
        &mut payload,
        RfbRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        VNC_ENCODING_QEMU_LED_STATE,
    );
    payload.push(0b101);
    append_rfb_rect_header(
        &mut payload,
        RfbRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        VNC_ENCODING_VMWARE_LED_STATE,
    );
    push_be_u32(&mut payload, 0b010);
    append_rfb_rect_header(
        &mut payload,
        RfbRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        VNC_ENCODING_EXTENDED_MOUSE_BUTTONS,
    );
    let mut reader = Cursor::new(payload);

    assert_eq!(
        read_framebuffer_update(&mut reader, &mut VncDecodeState::default()).unwrap(),
        VncServerEvent::Batch(vec![
            VncServerEvent::QemuExtendedKeyEvents,
            VncServerEvent::LockKeys(vnc_lock_keys_from_bits(0b101)),
            VncServerEvent::LockKeys(vnc_lock_keys_from_bits(0b010)),
            VncServerEvent::ExtendedMouseButtons,
        ])
    );
}

#[test]
fn baseline_clipboard_uses_utf8_or_latin1_decode_fallback() {
    assert_eq!(decode_vnc_clipboard_text(b"hello"), "hello");
    assert_eq!(decode_vnc_clipboard_text(&[b'c', b'a', b'f', 0xe9]), "café");
    assert_eq!(decode_vnc_clipboard_text("中文".as_bytes()), "中文");
    assert_eq!(encode_vnc_clipboard_text("café"), b"caf\xe9");
    assert_eq!(encode_vnc_clipboard_text("中文"), b"??");
    assert_eq!(
        client_cut_text_message("café").unwrap(),
        vec![6, 0, 0, 0, 0, 0, 0, 4, b'c', b'a', b'f', 0xe9]
    );
}

#[test]
fn vnc_error_category_identifies_authentication_and_network_errors() {
    assert_eq!(
        VncError::authentication("VNC password authentication failed.").category(),
        RemoteDesktopErrorCategory::Authentication
    );
    assert_eq!(
        VncError::network("VNC TCP connection failed: refused").category(),
        RemoteDesktopErrorCategory::Network
    );
    assert_eq!(
        VncError::network("VNC security list read failed: timed out").category(),
        RemoteDesktopErrorCategory::Network
    );
}

#[test]
fn vnc_error_category_separates_security_configuration_and_protocol_errors() {
    assert_eq!(
        VncError::security("Unsupported VNC security types: [19].").category(),
        RemoteDesktopErrorCategory::LegacySecurity
    );
    assert_eq!(
        VncError::configuration("VNC helper received a non-VNC connect request.").category(),
        RemoteDesktopErrorCategory::Configuration
    );
    assert_eq!(
        VncError::protocol("Unsupported VNC rectangle encoding 99.").category(),
        RemoteDesktopErrorCategory::Protocol
    );
}

#[test]
fn vnc_auth_key_reverses_bits_and_truncates_password() {
    let secret = RemoteDesktopSecret::from("abcdefghijk");

    assert_eq!(
        vnc_auth_key(&secret).as_slice(),
        &[0x86, 0x46, 0xc6, 0x26, 0xa6, 0x66, 0xe6, 0x16]
    );
}

fn remote_monitor(
    stable_id: &str,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    primary: bool,
) -> oxideterm_remote_desktop::RemoteDesktopMonitor {
    oxideterm_remote_desktop::RemoteDesktopMonitor {
        stable_id: stable_id.to_string(),
        left,
        top,
        width,
        height,
        primary,
        desktop_scale_factor: 100,
        device_scale_factor: 100,
        physical_width_mm: None,
        physical_height_mm: None,
        orientation: oxideterm_remote_desktop::RemoteDesktopMonitorOrientation::Landscape,
    }
}

fn append_rfb_rect_header(payload: &mut Vec<u8>, rect: RfbRect, encoding: i32) {
    push_be_u16(payload, rect.x);
    push_be_u16(payload, rect.y);
    push_be_u16(payload, rect.width);
    push_be_u16(payload, rect.height);
    push_be_i32(payload, encoding);
}

fn zlib_payload(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}
