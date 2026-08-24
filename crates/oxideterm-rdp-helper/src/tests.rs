use std::fmt;

use ironrdp::pdu::{geometry::InclusiveRectangle, input::fast_path::SynchronizeFlags};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopMouseButton, RemoteDesktopMouseButtonState, RemoteDesktopRect,
    RemoteDesktopWheelDelta,
};

use super::*;

fn tiny_png_bytes() -> Vec<u8> {
    vec![
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
        0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
    ]
}

#[derive(Debug)]
struct StaticConnectorSource(&'static str);

impl fmt::Display for StaticConnectorSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for StaticConnectorSource {}

#[test]
fn wheel_conversion_preserves_units_and_emits_both_axes() {
    for (delta, expected) in [(1.0, 120), (-1.0, -120), (240.0, 240)] {
        assert_eq!(rdp_wheel_units(delta), expected);
    }

    let operations = rdp_wheel_operations(RemoteDesktopWheelDelta { x: 1.0, y: -240.0 });

    assert_eq!(operations.len(), 2);
    match &operations[0] {
        RdpInputOperation::WheelRotations(rotations) => {
            assert!(!rotations.is_vertical);
            assert_eq!(rotations.rotation_units, 120);
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
    match &operations[1] {
        RdpInputOperation::WheelRotations(rotations) => {
            assert!(rotations.is_vertical);
            assert_eq!(rotations.rotation_units, -240);
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
}

#[test]
fn client_request_coalescer_keeps_latest_mouse_move_before_clicks() {
    let mut coalescer = ClientRdpRequestCoalescer::default();
    let mut output = Vec::new();

    coalescer.push(
        RemoteDesktopHelperRequest::MouseMove { x: 10, y: 20 },
        &mut output,
    );
    coalescer.push(
        RemoteDesktopHelperRequest::MouseMove { x: 30, y: 40 },
        &mut output,
    );
    assert!(output.is_empty());

    coalescer.push(
        RemoteDesktopHelperRequest::MouseButton {
            button: RemoteDesktopMouseButton::Left,
            state: RemoteDesktopMouseButtonState::Pressed,
        },
        &mut output,
    );
    coalescer.push(
        RemoteDesktopHelperRequest::MouseMove { x: 50, y: 60 },
        &mut output,
    );
    coalescer.flush(&mut output);

    assert_eq!(
        output,
        vec![
            RemoteDesktopHelperRequest::MouseMove { x: 30, y: 40 },
            RemoteDesktopHelperRequest::MouseButton {
                button: RemoteDesktopMouseButton::Left,
                state: RemoteDesktopMouseButtonState::Pressed,
            },
            RemoteDesktopHelperRequest::MouseMove { x: 50, y: 60 },
        ]
    );
}

#[test]
fn client_output_drain_yields_after_budget() {
    let writer = SharedEventWriter::inert_for_tests();
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    for index in 0..=RDP_CLIENT_OUTPUT_DRAIN_LIMIT {
        output_tx
            .try_send_graphics(ClientRdpOutput::Event(
                RemoteDesktopHelperEvent::FrameUpdate {
                    update: RemoteDesktopFrameUpdate::new(
                        RemoteDesktopSize {
                            width: 128,
                            height: 1,
                        },
                        RemoteDesktopRect::new(index as u32, 0, 1, 1),
                        RemoteDesktopFrameFormat::Rgba8,
                        vec![index as u8, 0, 0, 0xff],
                    ),
                },
            ))
            .unwrap();
    }

    let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

    assert_eq!(drain.drained, RDP_CLIENT_OUTPUT_DRAIN_LIMIT);
    assert!(drain.exit.is_none());
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(_))
    ));
}

#[test]
fn saturated_graphics_queue_defers_latest_base_frame_without_dropping_state() {
    let (output_tx, output_rx) = client_rdp_output_channel(1);
    output_tx
        .try_send_graphics(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    RemoteDesktopSize {
                        width: 2,
                        height: 2,
                    },
                    RemoteDesktopRect::new(0, 0, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![9, 9, 9, 0xff],
                ),
            },
        ))
        .unwrap();
    let image = DecodedImage::new(PixelFormat::RgbA32, 2, 2);
    let mut frame_state = ClientRdpFrameState {
        graphics_sync: RdpGraphicsSyncState::Synced,
        ..ClientRdpFrameState::default()
    };

    send_client_rdp_graphics_event(
        &output_tx,
        RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                RemoteDesktopSize {
                    width: 2,
                    height: 2,
                },
                RemoteDesktopRect::new(0, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![1, 1, 1, 0xff],
            ),
        },
        &mut frame_state,
    )
    .unwrap();

    assert!(frame_state.pending_base_frame);
    assert!(frame_state.graphics_sync.needs_base());
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::FrameUpdate { .. }
        ))
    ));

    flush_pending_rdp_base_frame(&output_tx, &image, &mut frame_state).unwrap();

    assert!(!frame_state.pending_base_frame);
    assert_eq!(frame_state.graphics_sync, RdpGraphicsSyncState::Synced);
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));
}

#[test]
fn graphics_base_frame_replaces_queued_dirty_updates() {
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    output_tx
        .try_send_graphics(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    RemoteDesktopSize {
                        width: 2,
                        height: 1,
                    },
                    RemoteDesktopRect::new(1, 0, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![1, 1, 1, 0xff],
                ),
            },
        ))
        .unwrap();

    output_tx
        .try_send_graphics(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame {
            frame: test_frame(),
        }))
        .unwrap();

    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn client_output_drain_prioritizes_control_events_over_saturated_graphics() {
    let writer = SharedEventWriter::inert_for_tests();
    let (output_tx, output_rx) = client_rdp_output_channel(1);
    output_tx
        .try_send_graphics(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame {
            frame: test_frame(),
        }))
        .unwrap();
    output_tx
        .send_control(ClientRdpOutput::ConnectionFailure(
            connector::ConnectorError::new("Authentication", ConnectorErrorKind::AccessDenied),
        ))
        .unwrap();

    let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

    match drain.exit {
        Some(ClientRdpSessionExit::ConnectionFailed { category, .. }) => {
            assert_eq!(category, RemoteDesktopErrorCategory::Authentication);
        }
        other => panic!("expected control failure before graphics, got {other:?}"),
    }
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));
}

#[test]
fn egfx_protocol_failure_preserves_protocol_category() {
    let writer = SharedEventWriter::inert_for_tests();
    let (output_tx, output_rx) = client_rdp_output_channel(1);
    output_tx
        .send_control(ClientRdpOutput::ProtocolFailure(
            "RDP Progressive decode failed.".to_string(),
        ))
        .unwrap();

    let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

    match drain.exit {
        Some(ClientRdpSessionExit::ConnectionFailed { message, category }) => {
            assert_eq!(message, "RDP Progressive decode failed.");
            assert_eq!(category, RemoteDesktopErrorCategory::Protocol);
        }
        other => panic!("expected an EGFX protocol failure, got {other:?}"),
    }
}

#[test]
fn cursor_position_can_drop_under_backpressure_but_shape_is_preserved() {
    let (output_tx, output_rx) = client_rdp_output_channel(1);
    output_tx
        .try_send_graphics(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame {
            frame: test_frame(),
        }))
        .unwrap();

    send_client_rdp_event(
        &output_tx,
        RemoteDesktopHelperEvent::Cursor {
            x: 10,
            y: 20,
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    send_client_rdp_event(
        &output_tx,
        RemoteDesktopHelperEvent::CursorShape {
            shape: RemoteDesktopCursorShape::new(
                RemoteDesktopSize {
                    width: 1,
                    height: 1,
                },
                0,
                0,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xff, 0xff, 0xff, 0xff],
            ),
        },
    )
    .unwrap();

    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        output_rx.control_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::CursorShape { .. }
        ))
    ));
}

#[test]
fn client_loop_prioritizes_queued_close_over_pending_output_error() {
    let writer = SharedEventWriter::inert_for_tests();
    let (request_tx, request_rx) = mpsc::channel();
    let (input_tx, _input_rx) = tokio_mpsc::unbounded_channel();
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    output_tx
        .send_control(ClientRdpOutput::ConnectionFailure(
            connector::ConnectorError::new(
                "test",
                ConnectorErrorKind::Reason("queued failure".to_string()),
            ),
        ))
        .unwrap();
    request_tx.send(RemoteDesktopHelperRequest::Close).unwrap();
    let mut config = RdpWorkerConfig {
        endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
        transport_endpoint: None,
        size: RemoteDesktopSize {
            width: 1280,
            height: 720,
        },
        scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
        graphics_epoch: 0,
        read_only: false,
        session_options: RemoteDesktopSessionOptions::default(),
        monitor_layout: RemoteDesktopMonitorLayout::default(),
    };

    let exit = run_client_rdp_loop(
        &writer,
        &request_rx,
        &input_tx,
        output_rx,
        &mut config,
        false,
    )
    .unwrap();

    assert!(matches!(exit, ClientRdpSessionExit::Closed));
}

#[test]
fn connector_failure_exit_preserves_structured_category() {
    let writer = SharedEventWriter::inert_for_tests();
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    output_tx
        .send_control(ClientRdpOutput::ConnectionFailure(
            connector::ConnectorError::new("Authentication", ConnectorErrorKind::AccessDenied),
        ))
        .unwrap();

    let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

    // The helper event should classify from the connector error kind, not
    // from a localized display string.
    match drain.exit {
        Some(ClientRdpSessionExit::ConnectionFailed { message, category }) => {
            assert_eq!(category, RemoteDesktopErrorCategory::Authentication);
            assert!(message.contains("access denied"));
        }
        other => panic!("unexpected drain exit: {other:?}"),
    }
}

#[test]
fn active_session_failure_exit_preserves_structured_category() {
    let writer = SharedEventWriter::inert_for_tests();
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    output_tx
        .send_control(ClientRdpOutput::SessionFailure {
            message: "RDP session ended after transport loss.".to_string(),
            category: RemoteDesktopErrorCategory::Network,
        })
        .unwrap();

    let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

    assert!(matches!(
        drain.exit,
        Some(ClientRdpSessionExit::ConnectionFailed {
            category: RemoteDesktopErrorCategory::Network,
            ..
        })
    ));
}

#[test]
fn clipboard_formats_prefer_unicode_text() {
    let formats = text_clipboard_formats();

    assert_eq!(
        preferred_text_clipboard_format(&formats),
        Some(ClipboardFormatId::CF_UNICODETEXT)
    );
}

#[test]
fn clipboard_ready_advertises_cached_local_text() {
    let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
    let (output_tx, _output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let mut backend = ClientClipboardBackend::new(
        input_tx,
        output_tx,
        RemoteDesktopSessionOptions::default().clipboard,
    );
    backend.set_local_text("hello".to_string());

    backend.on_ready();

    match input_rx.try_recv().unwrap() {
        RdpInputEvent::Clipboard(ClipboardMessage::SendInitiateCopy(formats)) => {
            assert!(
                formats
                    .iter()
                    .any(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
            );
            assert!(
                formats
                    .iter()
                    .any(|format| format.id == ClipboardFormatId::CF_TEXT)
            );
        }
        message => panic!("unexpected clipboard message: {message:?}"),
    }
}

#[test]
fn clipboard_ready_advertises_cached_local_image_data() {
    let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
    let (output_tx, _output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let mut backend = ClientClipboardBackend::new(
        input_tx,
        output_tx,
        RemoteDesktopSessionOptions::default().clipboard,
    );
    backend.set_local_data(RemoteDesktopClipboardData::new(
        RemoteDesktopClipboardFormat::ImagePng,
        vec![1, 2, 3],
    ));

    backend.on_ready();

    match input_rx.try_recv().unwrap() {
        RdpInputEvent::Clipboard(ClipboardMessage::SendInitiateCopy(formats)) => {
            assert!(formats.iter().any(|format| {
                format.id == RDP_CLIPBOARD_FORMAT_IMAGE_PNG
                    && format
                        .name
                        .as_ref()
                        .is_some_and(|name| name.value() == "PNG")
            }));
            assert!(
                !formats
                    .iter()
                    .any(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
            );
        }
        message => panic!("unexpected clipboard message: {message:?}"),
    }
}

#[test]
fn clipboard_image_format_prefers_named_png_before_text() {
    let formats = vec![
        ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ClipboardFormat::new(ClipboardFormatId::new(0xc080))
            .with_name(ClipboardFormatName::new("image/png")),
    ];

    assert_eq!(
        preferred_image_clipboard_format(&formats),
        Some(RdpClipboardDataFormat {
            id: ClipboardFormatId::new(0xc080),
            format: RemoteDesktopClipboardFormat::ImagePng,
            encoding: RdpClipboardDataEncoding::Encoded,
        })
    );
}

#[test]
fn local_png_clipboard_can_answer_dib_requests() {
    let data =
        RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImagePng, tiny_png_bytes());

    assert!(encode_local_clipboard_data(&data, ClipboardFormatId::CF_DIB).is_some());
    assert!(encode_local_clipboard_data(&data, ClipboardFormatId::CF_DIBV5).is_some());
    assert_eq!(
        encode_local_clipboard_data(&data, RDP_CLIPBOARD_FORMAT_IMAGE_PNG),
        Some(data.bytes.clone())
    );
}

#[test]
fn remote_dib_clipboard_decodes_to_png_data() {
    let data =
        RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImagePng, tiny_png_bytes());
    let dib = encode_local_clipboard_data(&data, ClipboardFormatId::CF_DIB).unwrap();

    let decoded = decode_remote_clipboard_data(
        RdpClipboardDataFormat {
            id: ClipboardFormatId::CF_DIB,
            format: RemoteDesktopClipboardFormat::ImagePng,
            encoding: RdpClipboardDataEncoding::Dib,
        },
        dib,
    )
    .unwrap();

    assert_eq!(decoded.format, RemoteDesktopClipboardFormat::ImagePng);
    assert!(decoded.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn clipboard_data_request_enters_client_loop() {
    let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
    let mut input_database = RdpInputDatabase::new();
    let mut keyboard_mapper = RdpKeyboardInputMapper::default();
    let data =
        RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImageGif, vec![9, 8, 7]);

    forward_client_rdp_request(
        &input_tx,
        &mut input_database,
        &mut keyboard_mapper,
        RemoteDesktopHelperRequest::ClipboardData { data: data.clone() },
        false,
    )
    .unwrap();

    match input_rx.try_recv().unwrap() {
        RdpInputEvent::SetClipboardData(received) => assert_eq!(received, data),
        event => panic!("expected clipboard data event, got {event:?}"),
    }
}

#[test]
fn request_frame_request_enters_client_loop() {
    let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
    let mut input_database = RdpInputDatabase::new();
    let mut keyboard_mapper = RdpKeyboardInputMapper::default();

    forward_client_rdp_request(
        &input_tx,
        &mut input_database,
        &mut keyboard_mapper,
        RemoteDesktopHelperRequest::RequestFrame,
        false,
    )
    .unwrap();

    match input_rx.try_recv().unwrap() {
        RdpInputEvent::RequestFrame => {}
        event => panic!("expected request-frame event, got {event:?}"),
    }
}

#[test]
fn lock_key_sync_request_emits_fastpath_sync_event() {
    let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
    let mut input_database = RdpInputDatabase::new();
    let mut keyboard_mapper = RdpKeyboardInputMapper::default();

    forward_client_rdp_request(
        &input_tx,
        &mut input_database,
        &mut keyboard_mapper,
        RemoteDesktopHelperRequest::SynchronizeLockKeys {
            keys: RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: false,
                caps_lock: true,
                kana_lock: false,
            },
        },
        false,
    )
    .unwrap();

    match input_rx.try_recv().unwrap() {
        RdpInputEvent::FastPath(events) => {
            assert_eq!(events.len(), 1);
            let FastPathInputEvent::SyncEvent(flags) = events[0] else {
                panic!("expected synchronize event, got {:?}", events[0]);
            };
            assert!(flags.contains(SynchronizeFlags::SCROLL_LOCK));
            assert!(flags.contains(SynchronizeFlags::CAPS_LOCK));
            assert!(!flags.contains(SynchronizeFlags::NUM_LOCK));
            assert!(!flags.contains(SynchronizeFlags::KANA_LOCK));
        }
        message => panic!("unexpected RDP input message: {message:?}"),
    }
}

#[test]
fn client_config_withholds_credentials_until_certificate_acceptance() {
    let mut config = RdpWorkerConfig {
        endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
        transport_endpoint: Some(RemoteDesktopEndpoint::new("127.0.0.1", 43891)),
        size: RemoteDesktopSize {
            width: 1280,
            height: 720,
        },
        scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
        graphics_epoch: 0,
        read_only: false,
        session_options: RemoteDesktopSessionOptions::default(),
        monitor_layout: RemoteDesktopMonitorLayout::default(),
    };

    let client_config = build_client_rdp_config(&config).unwrap();

    assert_eq!(client_config.destination.host(), "example.test");
    assert_eq!(client_config.destination.port(), 3389);
    assert_eq!(client_config.transport_destination.host(), "127.0.0.1");
    assert_eq!(client_config.transport_destination.port(), 43891);
    assert!(client_config.connector.enable_tls);
    assert!(client_config.connector.enable_credssp);
    assert!(!client_config.connector.autologon);
    assert!(client_config.connector.enable_server_pointer);
    assert!(!client_config.connector.pointer_software_rendering);
    assert!(client_config.connector.support_dyn_vc_gfx_protocol);
    assert_eq!(client_config.connector.desktop_size.width, 1280);
    assert_eq!(client_config.connector.desktop_size.height, 720);
    assert_eq!(
        client_config.connector.desktop_scale_factor,
        RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT
    );
    let bitmap = client_config.connector.bitmap.as_ref().unwrap();
    assert!(bitmap.lossy_compression);
    assert_eq!(bitmap.color_depth, 32);
    assert_eq!(rdp_bitmap_codec_labels(&bitmap.codecs), "remotefx");

    for (profile, expected_connection_type, expected_flags) in [
        (
            RemoteDesktopRdpNetworkProfile::Automatic,
            ConnectionType::Autodetect,
            PerformanceFlags::default(),
        ),
        (
            RemoteDesktopRdpNetworkProfile::Lan,
            ConnectionType::Lan,
            PerformanceFlags::ENABLE_FONT_SMOOTHING | PerformanceFlags::ENABLE_DESKTOP_COMPOSITION,
        ),
        (
            RemoteDesktopRdpNetworkProfile::Broadband,
            ConnectionType::BroadbandHigh,
            PerformanceFlags::DISABLE_WALLPAPER
                | PerformanceFlags::DISABLE_FULLWINDOWDRAG
                | PerformanceFlags::DISABLE_MENUANIMATIONS
                | PerformanceFlags::ENABLE_FONT_SMOOTHING,
        ),
        (
            RemoteDesktopRdpNetworkProfile::LowBandwidth,
            ConnectionType::BroadbandLow,
            PerformanceFlags::DISABLE_WALLPAPER
                | PerformanceFlags::DISABLE_FULLWINDOWDRAG
                | PerformanceFlags::DISABLE_MENUANIMATIONS
                | PerformanceFlags::DISABLE_THEMING
                | PerformanceFlags::DISABLE_CURSOR_SHADOW
                | PerformanceFlags::DISABLE_CURSORSETTINGS,
        ),
    ] {
        assert_eq!(
            rdp_network_profile_config(profile),
            (expected_connection_type, expected_flags)
        );
    }

    config.session_options.rdp.disable_graphics_pipeline = true;
    let compatibility_config = build_client_rdp_config(&config).unwrap();
    assert!(!compatibility_config.connector.support_dyn_vc_gfx_protocol);
}

#[test]
fn rdp_desktop_size_normalization_matches_displaycontrol_bounds() {
    assert_eq!(
        normalized_rdp_desktop_size(RemoteDesktopSize {
            width: 201,
            height: 121,
        }),
        RemoteDesktopSize {
            width: 200,
            height: 200,
        }
    );
    assert_eq!(
        normalized_rdp_desktop_size(RemoteDesktopSize {
            width: 9001,
            height: 9001,
        }),
        RemoteDesktopSize {
            width: 8192,
            height: 8192,
        }
    );
}

#[test]
fn resize_request_enters_client_loop_with_normalized_rdp_size() {
    let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
    let mut input_database = RdpInputDatabase::new();
    let mut keyboard_mapper = RdpKeyboardInputMapper::default();

    forward_client_rdp_request(
        &input_tx,
        &mut input_database,
        &mut keyboard_mapper,
        RemoteDesktopHelperRequest::Resize {
            size: RemoteDesktopSize {
                width: 201,
                height: 121,
            },
            scale_factor: Some(125),
        },
        false,
    )
    .unwrap();

    match input_rx.try_recv().unwrap() {
        RdpInputEvent::Resize {
            width,
            height,
            scale_factor,
            ..
        } => {
            assert_eq!(width, 200);
            assert_eq!(height, 200);
            assert_eq!(scale_factor, 125);
        }
        event => panic!("expected resize event, got {event:?}"),
    }
}

#[test]
fn disconnect_reason_hides_local_dependency_paths() {
    let message = sanitize_rdp_disconnect_reason(Some(
        "RDP session ended: /Users/example/.cargo/git/checkouts/ironrdp/src/lib.rs",
    ));

    assert_eq!(message.as_deref(), Some("RDP session ended."));
}

#[test]
fn established_transport_closure_is_classified_as_network_failure() {
    assert_eq!(
        remote_desktop_error_category_from_message(
            "server closed established RDP session while reading frames: unexpected EOF"
        ),
        RemoteDesktopErrorCategory::Network
    );
}

#[test]
fn native_rdp_desktop_ready_events_report_first_frame_ready() {
    let events = native_rdp_desktop_ready_events(RemoteDesktopSize {
        width: 1280,
        height: 720,
    });

    assert!(matches!(
        events[0],
        RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: 1280,
                height: 720
            }
        }
    ));
    assert!(matches!(
        &events[1],
        RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connected,
            message: Some(message),
        } if message.contains("desktop frame")
    ));
}

#[test]
fn unsupported_resize_reports_existing_framebuffer_size() {
    let image = DecodedImage::new(PixelFormat::RgbA32, 4, 3);

    assert!(matches!(
        unsupported_resize_connected_event(&image),
        RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: 4,
                height: 3
            }
        }
    ));
}

#[test]
fn first_desktop_base_frame_publishes_connected_once() {
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let image = DecodedImage::new(PixelFormat::RgbA32, 4, 3);
    let mut frame_state = ClientRdpFrameState::default();

    send_client_rdp_base_frame(&output_tx, &image, &mut frame_state, true)
        .expect("first desktop frame should queue");
    send_client_rdp_base_frame(&output_tx, &image, &mut frame_state, true)
        .expect("later desktop frame should queue");

    assert!(frame_state.published_first_desktop_frame);
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));
    assert!(matches!(
        output_rx.control_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Connected {
                size: RemoteDesktopSize {
                    width: 4,
                    height: 3
                }
            }
        ))
    ));
    assert!(matches!(
        output_rx.control_rx.try_recv(),
        Ok(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connected,
            ..
        }))
    ));
    assert!(output_rx.control_rx.try_recv().is_err());
}

#[test]
fn graphics_update_uses_dirty_update_after_base_frame_is_synced() {
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let image = DecodedImage::new(PixelFormat::RgbA32, 4, 3);
    let mut frame_state = ClientRdpFrameState::default();

    send_client_rdp_graphics_update(
        &output_tx,
        &image,
        InclusiveRectangle {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        &mut frame_state,
    )
    .expect("first graphics update should establish a base frame");

    assert!(frame_state.published_first_desktop_frame);
    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));

    send_client_rdp_graphics_update(
        &output_tx,
        &image,
        InclusiveRectangle {
            left: 1,
            top: 1,
            right: 1,
            bottom: 1,
        },
        &mut frame_state,
    )
    .expect("synced graphics updates should use dirty rectangles");

    assert!(output_rx.graphics_rx.try_recv().is_err());
    flush_queued_rdp_graphics_updates(&output_tx, &image, &mut frame_state)
        .expect("queued dirty update should flush");

    match output_rx.graphics_rx.try_recv() {
        Ok(ClientRdpOutput::Event(RemoteDesktopHelperEvent::FrameUpdate { update })) => {
            assert_eq!(update.rect, RemoteDesktopRect::new(1, 1, 1, 1));
        }
        other => panic!("expected dirty frame update, got {other:?}"),
    }
}

#[test]
fn graphics_accumulator_merges_dirty_updates_before_copying_pixels() {
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let image = DecodedImage::new(PixelFormat::RgbA32, 8, 4);
    let mut frame_state = ClientRdpFrameState {
        graphics_sync: RdpGraphicsSyncState::Synced,
        published_first_desktop_frame: true,
        ..ClientRdpFrameState::default()
    };

    send_client_rdp_graphics_update(
        &output_tx,
        &image,
        InclusiveRectangle {
            left: 1,
            top: 1,
            right: 1,
            bottom: 1,
        },
        &mut frame_state,
    )
    .expect("first dirty update should queue");
    send_client_rdp_graphics_update(
        &output_tx,
        &image,
        InclusiveRectangle {
            left: 2,
            top: 1,
            right: 2,
            bottom: 1,
        },
        &mut frame_state,
    )
    .expect("second dirty update should queue");

    assert!(output_rx.graphics_rx.try_recv().is_err());
    flush_queued_rdp_graphics_updates(&output_tx, &image, &mut frame_state)
        .expect("queued dirty updates should flush");

    match output_rx.graphics_rx.try_recv() {
        Ok(ClientRdpOutput::Event(RemoteDesktopHelperEvent::FrameUpdate { update })) => {
            assert_eq!(update.rect, RemoteDesktopRect::new(1, 1, 2, 1));
        }
        other => panic!("expected merged dirty frame update, got {other:?}"),
    }
}

#[test]
fn graphics_accumulator_preserves_separated_dirty_regions_as_one_batch() {
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let image = DecodedImage::new(PixelFormat::RgbA32, 16, 4);
    let mut frame_state = ClientRdpFrameState {
        graphics_sync: RdpGraphicsSyncState::Synced,
        published_first_desktop_frame: true,
        ..ClientRdpFrameState::default()
    };

    for x in [1, 14] {
        send_client_rdp_graphics_update(
            &output_tx,
            &image,
            InclusiveRectangle {
                left: x,
                top: 1,
                right: x,
                bottom: 1,
            },
            &mut frame_state,
        )
        .expect("separated dirty update should queue");
    }

    flush_queued_rdp_graphics_updates(&output_tx, &image, &mut frame_state)
        .expect("queued dirty regions should flush atomically");

    match output_rx.graphics_rx.try_recv() {
        Ok(ClientRdpOutput::Event(RemoteDesktopHelperEvent::FrameUpdateBatch { batch })) => {
            assert_eq!(batch.updates.len(), 2);
            assert_eq!(batch.byte_len(), 8);
            assert_eq!(batch.updates[0].rect, RemoteDesktopRect::new(1, 1, 1, 1));
            assert_eq!(batch.updates[1].rect, RemoteDesktopRect::new(14, 1, 1, 1));
        }
        other => panic!("expected sparse dirty frame batch, got {other:?}"),
    }
}

#[test]
fn graphics_accumulator_promotes_large_dirty_area_to_base_frame() {
    let (output_tx, output_rx) = client_rdp_output_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
    let image = DecodedImage::new(PixelFormat::RgbA32, 4, 3);
    let mut frame_state = ClientRdpFrameState {
        graphics_sync: RdpGraphicsSyncState::Synced,
        published_first_desktop_frame: true,
        ..ClientRdpFrameState::default()
    };

    send_client_rdp_graphics_update(
        &output_tx,
        &image,
        InclusiveRectangle {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        },
        &mut frame_state,
    )
    .expect("large dirty update should promote to base frame");

    assert!(matches!(
        output_rx.graphics_rx.try_recv(),
        Ok(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::Frame { .. }
        ))
    ));
}

#[test]
fn rdp_frame_read_eof_is_reported_as_established_session_close() {
    assert_eq!(
        rdp_frame_read_error_context(&"unexpected eof while reading"),
        "server closed established RDP session while reading frames"
    );
    assert_eq!(rdp_frame_read_error_context(&"bad pdu"), "read RDP frame");
}

#[test]
fn dirty_rect_copy_extracts_only_region_and_sets_alpha() {
    let pixels = [
        [0, 1, 2, 0],
        [10, 11, 12, 0],
        [20, 21, 22, 0],
        [30, 31, 32, 0],
        [40, 41, 42, 0],
        [50, 51, 52, 0],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    let bytes = copy_image_rect(
        &pixels,
        3,
        RemoteDesktopRect {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        },
        RemoteDesktopFrameFormat::Rgba8,
    );

    assert_eq!(
        bytes,
        vec![
            10, 11, 12, 0xff, 20, 21, 22, 0xff, 40, 41, 42, 0xff, 50, 51, 52, 0xff,
        ]
    );
}

#[test]
fn reactivation_resets_graphics_base_without_publishing_empty_image() {
    let mut frame_state = ClientRdpFrameState {
        graphics_sync: RdpGraphicsSyncState::Synced,
        pending_base_frame: true,
        pending_base_frame_can_publish_ready: true,
        published_first_desktop_frame: true,
        ..ClientRdpFrameState::default()
    };

    reset_graphics_base_after_reactivation(&mut frame_state);

    assert!(frame_state.graphics_sync.needs_base());
    assert!(!frame_state.pending_base_frame);
    assert!(!frame_state.pending_base_frame_can_publish_ready);
}

#[test]
fn base_frame_event_uses_current_image_as_complete_backing_frame() {
    let image = DecodedImage::new(PixelFormat::RgbA32, 2, 1);

    match base_frame_event(&image) {
        RemoteDesktopHelperEvent::Frame { frame } => {
            assert_eq!(
                frame.size,
                RemoteDesktopSize {
                    width: 2,
                    height: 1
                }
            );
            assert_eq!(frame.format, RemoteDesktopFrameFormat::Rgba8);
            assert_eq!(frame.bytes, vec![0, 0, 0, 0xff, 0, 0, 0, 0xff]);
        }
        other => panic!("expected base frame, got {other:?}"),
    }
}

#[test]
fn base_frame_event_uses_bgra_for_rdp_decoded_image() {
    let image = DecodedImage::new(RDP_DECODED_FRAME_PIXEL_FORMAT, 2, 1);

    match base_frame_event(&image) {
        RemoteDesktopHelperEvent::Frame { frame } => {
            assert_eq!(frame.format, RemoteDesktopFrameFormat::Bgra8);
            assert_eq!(frame.bytes, vec![0, 0, 0, 0xff, 0, 0, 0, 0xff]);
        }
        other => panic!("expected base frame, got {other:?}"),
    }
}

#[test]
fn full_frame_copy_sets_alpha_opaque() {
    let bytes = opaque_frame_bytes(&[1, 2, 3, 0, 4, 5, 6, 7], RemoteDesktopFrameFormat::Rgba8);

    assert_eq!(bytes, vec![1, 2, 3, 0xff, 4, 5, 6, 0xff]);
}

#[test]
fn standard_security_error_is_actionable_and_path_free() {
    let error = connector::ConnectorError::new(
        "Initiation",
        ConnectorErrorKind::Reason(
            "client advertised SSL | HYBRID | HYBRID_EX, but server selected STANDARD_RDP_SECURITY"
                .to_string(),
        ),
    );

    let message = format_connector_error("RDP negotiation failed", &error);

    assert_eq!(message, LEGACY_RDP_SECURITY_MESSAGE);
    assert_eq!(
        connector_error_category(&error),
        RemoteDesktopErrorCategory::LegacySecurity
    );
    assert!(connector_error_requires_legacy_security(&error));
    assert!(!message.contains("/Users/"));
    assert!(!message.contains(".cargo"));
}

#[test]
fn custom_connector_error_includes_source_without_local_path() {
    let error = connector::ConnectorError::new("Initiation", ConnectorErrorKind::Custom)
            .with_source(StaticConnectorSource(
                "[license verification @ /Users/example/.cargo/git/checkouts/ironrdp/src/lib.rs:42] invalid server license",
            ));

    let message = format_connector_error("RDP negotiation failed", &error);

    assert_eq!(
        message,
        "RDP negotiation failed: [license verification] invalid server license"
    );
    assert!(!message.contains("/Users/"));
    assert!(!message.contains(".cargo"));
}

#[test]
fn custom_standard_security_source_reports_legacy_security() {
    let error = connector::ConnectorError::new("Initiation", ConnectorErrorKind::Custom)
            .with_source(StaticConnectorSource(
                "[Initiation @ /Users/example/.cargo/git/checkouts/ironrdp/src/lib.rs:409] client advertised SSL | HYBRID | HYBRID_EX, but server selected STANDARD_RDP_SECURITY",
            ));

    let message = format_connector_error("RDP negotiation failed", &error);

    assert!(connector_error_requires_legacy_security(&error));
    assert_eq!(message, LEGACY_RDP_SECURITY_MESSAGE);
    assert!(!message.contains("/Users/"));
    assert!(!message.contains(".cargo"));
}

#[test]
fn access_denied_connector_error_is_authentication_category() {
    let error = connector::ConnectorError::new("Authentication", ConnectorErrorKind::AccessDenied);

    // Access denied is a stable authentication failure constructor for tests.
    assert_eq!(
        connector_error_category(&error),
        RemoteDesktopErrorCategory::Authentication
    );
}

fn test_frame() -> RemoteDesktopFrame {
    RemoteDesktopFrame::new(
        RemoteDesktopSize {
            width: 1,
            height: 1,
        },
        RemoteDesktopFrameFormat::Bgra8,
        vec![0, 0, 0, 0xff],
    )
}
