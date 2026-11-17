// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::remote_desktop_surface_with_geometry;
use gpui::{TestAppContext, VisualContext, point, size};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate, RemoteDesktopRect,
};
use oxideterm_theme::theme_by_id;

struct RemoteDesktopSessionTestRoot;

#[gpui::test]
fn display_topology_owns_size_until_the_last_external_screen_is_removed(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| RemoteDesktopSessionTestRoot);
    let mut profile = preview_remote_desktop_profile(RemoteDesktopProtocol::Rdp);
    profile.session_options.display.use_all_monitors = true;
    let mut provider = builtin_preview_provider_registry()
        .unwrap()
        .get_for_protocol(RemoteDesktopProtocol::Rdp)
        .unwrap()
        .clone();
    provider.capabilities.multi_monitor = true;
    let (tx, rx) = mpsc::channel();
    let directory = tempfile::tempdir().unwrap();
    let session = cx.new(|cx| {
        let mut session = RemoteDesktopSessionEntity::new(
            TabId(900),
            profile,
            provider,
            None,
            directory.path().join("certificates.json"),
            RemoteDesktopFrameDeliverySlot::new(),
            window.into(),
        );
        session.worker = Some(RemoteDesktopWorkerOwner::new(tx, thread::spawn(|| {})));
        session.last_viewport_size = Some(RemoteDesktopSize {
            width: 1000,
            height: 600,
        });
        session.last_viewport_scale_factor = Some(150);
        session
            .state
            .apply_event(RemoteDesktopHelperEvent::Connected {
                size: RemoteDesktopSize {
                    width: 1500,
                    height: 900,
                },
            });
        session
            .state
            .apply_event(RemoteDesktopHelperEvent::CapabilitiesNegotiated {
                capabilities: NegotiatedCapabilities {
                    resize: NegotiatedCapabilityStatus::Supported,
                    ..Default::default()
                },
            });
        session.install_release_handler(cx);
        session
    });
    let primary = RemoteDesktopMonitor {
        stable_id: "external".into(),
        left: 0,
        top: 0,
        width: 3840,
        height: 2160,
        primary: true,
        desktop_scale_factor: 150,
        device_scale_factor: 140,
        physical_width_mm: None,
        physical_height_mm: None,
        orientation: RemoteDesktopMonitorOrientation::Landscape,
    };
    let secondary = RemoteDesktopMonitor {
        stable_id: "left".into(),
        left: -1920,
        width: 1920,
        height: 1080,
        primary: false,
        desktop_scale_factor: 100,
        device_scale_factor: 100,
        ..primary.clone()
    };
    let dual = RemoteDesktopMonitorLayout {
        monitors: vec![primary.clone(), secondary],
    };
    session.update(cx, |session, cx| {
        session.apply_monitor_layout(dual.clone(), cx);
        assert!(!session.request_preset_size(
            RemoteDesktopSize {
                width: 1280,
                height: 720
            },
            150,
            cx
        ));
        assert!(!session.set_follow_window_size(false, 150, cx));
        assert!(!session.send_resize_immediately(
            RemoteDesktopSize {
                width: 1600,
                height: 900
            },
            Some(150),
            cx
        ));
    });
    match rx.try_recv().unwrap() {
        RemoteDesktopHelperRequest::UpdateDisplayLayout { layout } => assert_eq!(layout, dual),
        _ => panic!("multi-monitor layout must be the only outgoing size request"),
    }
    assert!(rx.try_recv().is_err());
    session.update(cx, |session, cx| {
        session.apply_monitor_layout(
            RemoteDesktopMonitorLayout {
                monitors: vec![primary],
            },
            cx,
        );
    });
    let RemoteDesktopHelperRequest::UpdateDisplayLayout { layout } = rx.try_recv().unwrap() else {
        panic!("missing return to single-monitor layout");
    };
    assert_eq!(
        layout
            .monitors
            .iter()
            .map(|m| (m.left, m.top, m.width, m.height, m.primary))
            .collect::<Vec<_>>(),
        vec![(0, 0, 1500, 900, true)]
    );
    session.update(cx, |session, cx| {
        assert!(session.request_preset_size(
            RemoteDesktopSize {
                width: 1280,
                height: 720
            },
            150,
            cx
        ));
    });
    assert!(matches!(
        rx.try_recv().unwrap(),
        RemoteDesktopHelperRequest::Resize {
            size: RemoteDesktopSize {
                width: 1280,
                height: 720
            },
            scale_factor: Some(150)
        }
    ));
    drop(session);
    cx.run_until_parked();
}

struct AuditKeys;

impl oxideterm_audit::AuditKeyProvider for AuditKeys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
        Ok(Zeroizing::new(vec![3; 32]))
    }

    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
        self.load(id)
    }
}

#[gpui::test]
fn desktop_helper_results_audit_connection_and_redact_clipboard(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    oxideterm_audit::AuditStore::open(&dir.path().join("audit.db"), &AuditKeys)
        .unwrap()
        .set_policy(oxideterm_audit::AuditPolicy {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let audit_service =
        oxideterm_audit::AuditService::with_key_provider(dir.path().join("audit.db"), AuditKeys)
            .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut connections = Vec::new();
    let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
    for (tab_id, protocol, succeeds) in [
        (81, RemoteDesktopProtocol::Rdp, true),
        (82, RemoteDesktopProtocol::Vnc, false),
    ] {
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let audit_context = oxideterm_audit::AuditContext::new(
            audit_service.client(),
            oxideterm_audit::AuditSource::User,
        )
        .session(protocol.provider_id(), &format!("desktop-{tab_id}.test"));
        let connect =
            desktop_connection_audit(Some(&audit_context), "desktop_connect", None).unwrap();
        let operation_id = connect.id().unwrap().to_owned();
        connections.push((
            operation_id,
            audit_context.session_id.clone(),
            if succeeds {
                oxideterm_audit::AuditOutcome::Succeeded
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
        ));
        let pending = PendingRemoteDesktopSshTunnel::new(
            || {},
            runtime.spawn(async { Ok(RemoteDesktopEndpoint::new("127.0.0.1", 3390)) }),
            Some(audit_context.clone()),
            Some(connect),
        );
        let (_, lease) = runtime.block_on(pending.finish()).unwrap();
        let session = cx.new(|cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(tab_id),
                profile,
                provider,
                None,
                dir.path().join(format!("certificates-{tab_id}.json")),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.audit_context = Some(audit_context.clone());
            session.ssh_tunnel = Some(lease);
            session.start_connection_audit("desktop_connect", None);
            session.worker_generation = 1;
            session.install_release_handler(cx);
            session
        });
        window
            .update(cx, |_root, window, cx| {
                session.update(cx, |session, cx| {
                    let event = if succeeds {
                        RemoteDesktopHelperEvent::Connected {
                            size: RemoteDesktopSize::clamped(640, 480),
                        }
                    } else {
                        RemoteDesktopHelperEvent::ConnectionFailure {
                            message: "private helper diagnostic".into(),
                            category: Some(RemoteDesktopErrorCategory::Network),
                        }
                    };
                    session
                        .delivery_tx
                        .send(RemoteDesktopWorkerDelivery::Event {
                            tab_id: TabId(tab_id),
                            generation: 1,
                            event,
                        })
                        .unwrap();
                    if succeeds {
                        session
                            .delivery_tx
                            .send(RemoteDesktopWorkerDelivery::Event {
                                tab_id: TabId(tab_id),
                                generation: 1,
                                event: RemoteDesktopHelperEvent::ClipboardText {
                                    text: "private clipboard payload".into(),
                                },
                            })
                            .unwrap();
                    }
                    session.poll_deliveries(window, cx);
                });
            })
            .unwrap();
    }
    let page = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(audit_service.client().query(oxideterm_audit::AuditQuery {
            limit: 20,
            ..Default::default()
        }))
        .unwrap();
    let results = page
        .records
        .iter()
        .filter_map(|record| {
            let op = record.details.operation.as_ref()?;
            (op.phase == Some(oxideterm_audit::AuditPhase::Result)).then_some((
                op.action.as_str(),
                op.outcome,
                op.bytes,
            ))
        })
        .collect::<Vec<_>>();
    assert!(results.contains(&(
        "desktop_connect",
        oxideterm_audit::AuditOutcome::Succeeded,
        None
    )));
    assert!(results.contains(&(
        "desktop_connect",
        oxideterm_audit::AuditOutcome::Failed,
        None
    )));
    assert!(results.contains(&(
        "desktop_clipboard_transfer",
        oxideterm_audit::AuditOutcome::Succeeded,
        Some(25)
    )));
    for (id, session_id, expected) in connections {
        let phases = page
            .records
            .iter()
            .filter_map(|record| record.details.operation.as_ref())
            .filter(|operation| operation.action == "desktop_connect" && operation.id == id)
            .collect::<Vec<_>>();
        assert_eq!(
            phases
                .iter()
                .filter(|operation| operation.phase == Some(oxideterm_audit::AuditPhase::Start))
                .count(),
            1
        );
        assert_eq!(
            phases
                .iter()
                .filter(|operation| operation.phase == Some(oxideterm_audit::AuditPhase::Result))
                .count(),
            1
        );
        assert_eq!(
            phases
                .iter()
                .find(|operation| operation.phase == Some(oxideterm_audit::AuditPhase::Result))
                .unwrap()
                .outcome,
            expected
        );
        assert!(
            phases
                .iter()
                .all(|operation| operation.session_id == session_id)
        );
    }
    let serialized = serde_json::to_string(&page.records).unwrap();
    assert!(!serialized.contains("private clipboard payload"));
    assert!(!serialized.contains("private helper diagnostic"));
}

impl Render for RemoteDesktopSessionTestRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn missing_remote_desktop_password_requests_credentials_once_per_attempt(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| RemoteDesktopSessionTestRoot);
    for (protocol, category, supplied, expected) in [
        (
            RemoteDesktopProtocol::Vnc,
            RemoteDesktopErrorCategory::Authentication,
            false,
            true,
        ),
        (
            RemoteDesktopProtocol::Rdp,
            RemoteDesktopErrorCategory::Authentication,
            false,
            true,
        ),
        (
            RemoteDesktopProtocol::Vnc,
            RemoteDesktopErrorCategory::Network,
            false,
            false,
        ),
        (
            RemoteDesktopProtocol::Vnc,
            RemoteDesktopErrorCategory::LegacySecurity,
            false,
            false,
        ),
        (
            RemoteDesktopProtocol::Vnc,
            RemoteDesktopErrorCategory::Authentication,
            true,
            false,
        ),
    ] {
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let session = cx.new(|_| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(45),
                preview_remote_desktop_profile(protocol),
                provider,
                supplied.then(|| RemoteDesktopSecret::from("test-password")),
                std::env::temp_dir().join("unused-auth-test-certs.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.worker_generation = 1;
            session
        });
        window
            .update(cx, |_, window, cx| {
                session.update(cx, |session, cx| {
                    for first in [true, false] {
                        session
                            .delivery_tx
                            .send(RemoteDesktopWorkerDelivery::Event {
                                tab_id: TabId(45),
                                generation: 1,
                                event: RemoteDesktopHelperEvent::ConnectionFailure {
                                    message: "authentication test".into(),
                                    category: Some(category),
                                },
                            })
                            .unwrap();
                        let outcome = session.poll_deliveries(window, cx);
                        assert_eq!(
                            outcome.intents.iter().any(|intent| matches!(
                                intent,
                                RemoteDesktopDeliveryIntent::CredentialsRequired { generation: 1 }
                            )),
                            expected && first
                        );
                    }
                })
            })
            .unwrap();
    }
}

#[gpui::test]
fn initial_layout_probe_starts_rdp_and_vnc_before_first_delivery(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_window, _cx| RemoteDesktopSessionTestRoot);
    let tokens = ThemeTokens::from_builtin(theme_by_id("default"));
    let viewport_width = 960;
    let viewport_height = 540;
    let initial_scale_factor = 200;

    for (tab_number, protocol) in [
        (21, RemoteDesktopProtocol::Rdp),
        (22, RemoteDesktopProtocol::Vnc),
    ] {
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let window_handle = cx.window_handle();
        let session = cx.new(|cx| {
            let session = RemoteDesktopSessionEntity::new(
                TabId(tab_number),
                profile,
                provider,
                None,
                std::env::temp_dir().join(format!(
                    "oxideterm-initial-layout-{}-test-certificates.json",
                    protocol.provider_id()
                )),
                RemoteDesktopFrameDeliverySlot::new(),
                window_handle,
            );
            session.install_release_handler(cx);
            session
        });

        // The placeholder canvas is the only source of the initial
        // viewport before either protocol can produce a worker delivery.
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(viewport_width as f32), px(viewport_height as f32)),
            |_window, cx| {
                let session = session.read(cx);
                remote_desktop_surface_with_geometry(
                    &tokens,
                    &session.state,
                    Some(session.geometry.clone()),
                )
            },
        );
        assert_eq!(
            session.read_with(cx, |session, _cx| session.geometry.viewport_size()),
            Some(RemoteDesktopSize::clamped(viewport_width, viewport_height))
        );
        session.update(cx, |session, cx| {
            session.schedule_initial_layout_probe(initial_scale_factor, cx);
        });
        cx.cx.run_until_parked();

        let (worker_started, measured_viewport, recorded_scale_factor) =
            session.read_with(cx, |session, _cx| {
                (
                    session.worker.is_some(),
                    session.last_viewport_size,
                    session.last_viewport_scale_factor,
                )
            });
        assert!(worker_started, "{protocol:?} worker did not start");
        assert_eq!(
            measured_viewport,
            Some(RemoteDesktopSize::clamped(viewport_width, viewport_height))
        );
        assert_eq!(recorded_scale_factor, Some(initial_scale_factor));

        drop(session);
        cx.cx.run_until_parked();
    }
}

#[gpui::test]
fn rdp_activation_announces_files_without_paste_and_does_not_echo_remote_files(
    cx: &mut TestAppContext,
) {
    let window = cx.add_empty_window();
    window.deactivate_window();
    window.run_until_parked();
    let (tx, rx) = mpsc::channel();
    let local_path = PathBuf::from("/tmp/local clipboard file.txt");
    let session = window.update(|window, cx| {
        let mut profile = preview_remote_desktop_profile(RemoteDesktopProtocol::Rdp);
        profile.session_options.clipboard.files = true;
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(RemoteDesktopProtocol::Rdp)
            .unwrap()
            .clone();
        let session = cx.new(|_| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(71),
                profile,
                provider,
                None,
                PathBuf::new(),
                RemoteDesktopFrameDeliverySlot::new(),
                window.window_handle(),
            );
            session.worker = Some(RemoteDesktopWorkerOwner::new(tx, thread::spawn(|| {})));
            session.ui_frame_visible = true;
            session.state.apply_event(RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Connected,
                message: None,
            });
            session
        });
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                vec![local_path.clone()].into(),
            ))],
        });
        session.update(cx, |session, cx| session.bind_file_clipboard(window, cx));
        session
    });
    assert!(
        rx.try_recv().is_err(),
        "an inactive window must not publish local files"
    );
    window.update(|window, _| window.activate_window());
    window.run_until_parked();
    match rx.try_recv().unwrap() {
        RemoteDesktopHelperRequest::ClipboardFiles { paths, .. } => {
            assert_eq!(paths, [local_path])
        }
        _ => panic!("activation must advertise files without injecting a paste shortcut"),
    }
    window.update(|_, cx| session.update(cx, |session, cx| session.sync_local_file_clipboard(cx)));
    assert!(
        rx.try_recv().is_err(),
        "the same file selection must not be announced twice"
    );

    let remote_path = PathBuf::from("/tmp/downloaded remote file.txt");
    window.update(|window, cx| {
        session.update(cx, |session, cx| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::Event {
                    tab_id: session.tab_id,
                    generation: session.worker_generation,
                    event: RemoteDesktopHelperEvent::ClipboardFilesReady {
                        transfer_id: "remote-files".into(),
                        paths: vec![remote_path.clone()],
                    },
                })
                .unwrap();
            session.poll_deliveries(window, cx);
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| remote_desktop_clipboard_paths_from_item(&item)),
                Some(vec![remote_path])
            );
            session.sync_local_file_clipboard(cx);
        })
    });
    assert!(
        rx.try_recv().is_err(),
        "downloaded files must not be echoed to their source"
    );
    window.update(|_, cx| {
        session.update(cx, |session, cx| {
            session.profile.session_options.clipboard.files = false;
            cx.write_to_clipboard(ClipboardItem {
                entries: vec![ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                    vec![PathBuf::from("/tmp/not-shared.txt")].into(),
                ))],
            });
            session.sync_local_file_clipboard(cx);
        })
    });
    assert!(
        rx.try_recv().is_err(),
        "disabled file sharing must block automatic announcements"
    );
}

#[gpui::test]
fn hidden_rdp_and_vnc_sessions_apply_control_deliveries(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
    for (tab_number, protocol) in [
        (31, RemoteDesktopProtocol::Rdp),
        (32, RemoteDesktopProtocol::Vnc),
    ] {
        let tab_id = TabId(tab_number);
        let mut profile = preview_remote_desktop_profile(protocol);
        profile.session_options.clipboard.images = true;
        profile.session_options.clipboard.files = true;
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let session = cx.new(|_cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                tab_id,
                profile,
                provider,
                None,
                std::env::temp_dir().join(format!(
                    "oxideterm-hidden-control-{}-test-certificates.json",
                    protocol.provider_id()
                )),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.worker_generation = 1;
            session
        });
        let mut events = cx.events(&session);

        // Clipboard and lifecycle messages remain reliable while frame
        // presentation is suspended.
        session.update(cx, |session, cx| {
            session.set_frame_visibility(false, cx);
            for event in [
                RemoteDesktopHelperEvent::ClipboardText {
                    text: format!("hidden-{protocol:?}"),
                },
                RemoteDesktopHelperEvent::ClipboardTransferFailed {
                    transfer_id: "hidden-transfer".to_string(),
                    message: "content-free test failure".to_string(),
                },
            ] {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation: 1,
                        event,
                    })
                    .unwrap();
            }
            session.schedule_window_event(DesktopWindowDelivery::Mailbox(1), cx);
        });
        cx.run_until_parked();

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(format!("hidden-{protocol:?}"))
        );
        assert_eq!(
            events.try_recv().unwrap(),
            RemoteDesktopSessionEvent::ClipboardTransferFailed
        );

        session.update(cx, |session, cx| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation: 1,
                    message: "hidden transport failed".to_string(),
                })
                .unwrap();
            session.schedule_window_event(DesktopWindowDelivery::Mailbox(1), cx);
        });
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| session.read(cx).state.snapshot().status),
            RemoteDesktopSessionStatus::Failed
        );

        session.update(cx, |session, cx| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation: 1,
                    event: RemoteDesktopHelperEvent::Disconnected { reason: None },
                })
                .unwrap();
            session.schedule_window_event(DesktopWindowDelivery::Mailbox(1), cx);
        });
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| session.read(cx).state.snapshot().status),
            RemoteDesktopSessionStatus::Disconnected
        );

        let clipboard_path =
            std::env::temp_dir().join(format!("hidden-{protocol:?}-clipboard.txt"));
        session.update(cx, |session, cx| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation: 1,
                    event: RemoteDesktopHelperEvent::ClipboardFilesReady {
                        transfer_id: "hidden-files".to_string(),
                        paths: vec![clipboard_path.clone()],
                    },
                })
                .unwrap();
            session.schedule_window_event(DesktopWindowDelivery::Mailbox(1), cx);
        });
        cx.run_until_parked();
        let clipboard_item = cx.read_from_clipboard().unwrap();
        assert!(matches!(
            clipboard_item.entries().first(),
            Some(ClipboardEntry::ExternalPaths(paths))
                if paths.paths() == std::slice::from_ref(&clipboard_path)
        ));

        session.update(cx, |session, cx| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation: 1,
                    event: RemoteDesktopHelperEvent::ClipboardData {
                        data: RemoteDesktopClipboardData::new(
                            RemoteDesktopClipboardFormat::ImagePng,
                            vec![1, 2, 3, 4],
                        ),
                    },
                })
                .unwrap();
            session.schedule_window_event(DesktopWindowDelivery::Mailbox(1), cx);
        });
        cx.run_until_parked();
        assert!(matches!(
            cx.read_from_clipboard()
                .and_then(|item| item.entries.into_iter().next()),
            Some(ClipboardEntry::Image(_))
        ));
    }
}

#[gpui::test]
fn established_network_failure_preserves_frame_until_retry_is_cancelled(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
    let tab_id = TabId(37);
    let profile = preview_remote_desktop_profile(RemoteDesktopProtocol::Rdp);
    let provider = builtin_preview_provider_registry()
        .unwrap()
        .get_for_protocol(RemoteDesktopProtocol::Rdp)
        .cloned()
        .unwrap();
    let session = cx.new(|_cx| {
        let mut session = RemoteDesktopSessionEntity::new(
            tab_id,
            profile,
            provider,
            None,
            std::env::temp_dir().join("oxideterm-reconnect-test-certificates.json"),
            RemoteDesktopFrameDeliverySlot::new(),
            window.into(),
        );
        session.worker_generation = 1;
        session.has_connected = true;
        session.state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                RemoteDesktopSize {
                    width: 2,
                    height: 1,
                },
                RemoteDesktopFrameFormat::Bgra8,
                vec![1, 2, 3, 0xff, 4, 5, 6, 0xff],
            ),
        });
        session
    });

    window
        .update(cx, |_root, window, cx| {
            session.update(cx, |session, cx| {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation: 1,
                        event: RemoteDesktopHelperEvent::ConnectionFailure {
                            message: "network interrupted".to_string(),
                            category: Some(RemoteDesktopErrorCategory::Network),
                        },
                    })
                    .unwrap();
                session.poll_deliveries(window, cx);
            });
        })
        .unwrap();

    let reconnect_snapshot = cx.read(|cx| session.read(cx).state.snapshot());
    assert_eq!(
        reconnect_snapshot.status,
        RemoteDesktopSessionStatus::Reconnecting
    );
    assert!(reconnect_snapshot.has_frame);
    assert!(cx.read(|cx| {
        session
            .read(cx)
            .automatic_reconnect_worker_generation
            .is_some()
    }));

    window
        .update(cx, |_root, window, cx| {
            session.update(cx, |session, cx| session.shutdown(Some(window), cx));
        })
        .unwrap();

    assert!(cx.read(|cx| {
        let session = session.read(cx);
        session.automatic_reconnect_worker_generation.is_none()
            && session.automatic_reconnect_task.is_none()
    }));
}

#[gpui::test]
fn hidden_rdp_and_vnc_sessions_resume_with_latest_frame(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
    let frame_size = RemoteDesktopSize {
        width: 2,
        height: 1,
    };
    for (tab_number, protocol) in [
        (41, RemoteDesktopProtocol::Rdp),
        (42, RemoteDesktopProtocol::Vnc),
    ] {
        let tab_id = TabId(tab_number);
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        let session = cx.new(|cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                tab_id,
                profile,
                provider,
                None,
                std::env::temp_dir().join(format!(
                    "oxideterm-hidden-frame-{}-test-certificates.json",
                    protocol.provider_id()
                )),
                frame_slot.clone(),
                window.into(),
            );
            session.worker_generation = 1;
            let wake = RemoteDesktopWorkerWake::default();
            session.worker_wake = Some(wake.clone());
            session.schedule_worker_wake(1, wake, cx);
            session.install_release_handler(cx);
            session
        });

        session.update(cx, |session, cx| {
            session.set_frame_visibility(false, cx);
        });
        let frame_decision = frame_slot.push(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(frame_size, RemoteDesktopFrameFormat::Rgba8, vec![0; 8]),
        });
        assert!(frame_decision.frame_ready);
        assert!(
            !frame_slot
                .push(RemoteDesktopHelperEvent::FrameUpdate {
                    update: RemoteDesktopFrameUpdate::new(
                        frame_size,
                        RemoteDesktopRect::new(1, 0, 1, 1),
                        RemoteDesktopFrameFormat::Rgba8,
                        vec![9, 8, 7, 0xff],
                    ),
                })
                .frame_ready
        );
        session.update(cx, |session, _| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::FrameReady {
                    tab_id,
                    generation: 1,
                })
                .unwrap();
            session.worker_wake.as_ref().unwrap().mark();
        });
        cx.run_until_parked();

        let hidden_snapshot = cx.read(|cx| session.read(cx).state.snapshot());
        assert!(
            !hidden_snapshot.has_frame,
            "{protocol:?} uploaded while hidden"
        );
        assert_eq!(cx.read(|cx| session.read(cx).state.texture_generation()), 0);
        assert!(frame_slot.has_queued_frame_events());

        session.update(cx, |session, cx| {
            session.set_frame_visibility(true, cx);
        });
        cx.run_until_parked();

        let visible_snapshot = cx.read(|cx| session.read(cx).state.snapshot());
        assert!(visible_snapshot.has_frame, "{protocol:?} did not resume");
        assert_eq!(visible_snapshot.size, Some(frame_size));
        assert_eq!(cx.read(|cx| session.read(cx).state.texture_generation()), 1);
        assert!(!frame_slot.has_queued_frame_events());
    }
}

#[gpui::test]
fn remote_desktop_plugin_removal_stops_only_its_sessions_and_blocks_missing_launches(
    cx: &mut TestAppContext,
) {
    let window = cx.add_window(|_, _| RemoteDesktopSessionTestRoot);
    let directory = tempfile::tempdir().unwrap();
    let desktops = cx.new(|_| RemoteDesktopWorkspaceEntity::new());
    let mut sessions = Vec::new();
    let mut completed = Vec::new();
    for (index, protocol) in [RemoteDesktopProtocol::Vnc, RemoteDesktopProtocol::Rdp]
        .into_iter()
        .enumerate()
    {
        let tab_id = TabId(index as u64 + 100);
        let mut provider = oxideterm_remote_desktop::builtin_provider_manifest(protocol);
        provider.id = format!("com.oxideterm.remote-desktop.{}", protocol.provider_id());
        let (request_tx, request_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            while let Ok(request) = request_rx.recv() {
                let close = matches!(request, RemoteDesktopHelperRequest::Close);
                requests.push(request);
                if close {
                    break;
                }
            }
            completed_tx.send(requests).unwrap();
        });
        let session = cx.new(|cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                tab_id,
                preview_remote_desktop_profile(protocol),
                provider,
                Some(RemoteDesktopSecret::from("plugin-session-fixture")),
                directory.path().join("certificates.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.use_plugin_provider(|_, _| None);
            session.worker = Some(RemoteDesktopWorkerOwner::new(request_tx, worker));
            session.worker_generation = 7;
            session.install_release_handler(cx);
            session
        });
        desktops.update(cx, |desktops, _| {
            desktops.insert(tab_id, session.clone(), Vec::new())
        });
        sessions.push(session);
        completed.push(completed_rx);
    }
    let vnc_id = cx.read(|cx| sessions[0].read(cx).provider.id.clone());
    let workers = desktops.update(cx, |desktops, cx| desktops.stop_plugins(Some(&vnc_id), cx));
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        completed[0].recv().unwrap(),
        vec![
            RemoteDesktopHelperRequest::ReleaseAllInputs,
            RemoteDesktopHelperRequest::Close
        ]
    );
    cx.read(|cx| {
        let vnc = sessions[0].read(cx);
        assert_eq!(
            vnc.state.snapshot().status,
            RemoteDesktopSessionStatus::Disconnected
        );
        assert_eq!(vnc.worker_generation, 8);
        assert!(vnc.password.is_none());
        assert!(vnc.worker.is_none());
        assert!(sessions[1].read(cx).worker.is_some());
        assert!(sessions[1].read(cx).password.is_some());
    });
    assert!(completed[1].try_recv().is_err());
    let events = cx.new(|_| Vec::<RemoteDesktopSessionEvent>::new());
    let _subscription = events.update(cx, |_, cx| {
        cx.subscribe(&sessions[0], |events, _session, event, _cx| {
            events.push(*event)
        })
    });
    sessions[0].update(cx, |session, cx| {
        assert!(!session.start_worker(RemoteDesktopSize::clamped(200, 120), None, None, cx));
        assert_eq!(session.worker_generation, 8);
        assert!(session.worker.is_none());
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(
            events.read(cx).as_slice(),
            &[RemoteDesktopSessionEvent::PluginRequired {
                protocol: RemoteDesktopProtocol::Vnc,
            }]
        )
    });
    let workers = desktops.update(cx, |desktops, cx| desktops.stop_plugins(None, cx));
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        completed[1].recv().unwrap(),
        vec![
            RemoteDesktopHelperRequest::ReleaseAllInputs,
            RemoteDesktopHelperRequest::Close
        ]
    );
}

#[gpui::test]
fn shutdown_after_window_close_releases_worker_and_secrets_once(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
    let protocol = RemoteDesktopProtocol::Rdp;
    let profile = preview_remote_desktop_profile(protocol);
    let provider = builtin_preview_provider_registry()
        .unwrap()
        .get_for_protocol(protocol)
        .cloned()
        .unwrap();
    let worker_wake = RemoteDesktopWorkerWake::default();
    let observed_wake = worker_wake.clone();
    let (request_tx, request_rx) = mpsc::channel();
    let session = cx.new(|_cx| {
        let mut session = RemoteDesktopSessionEntity::new(
            TabId(12),
            profile,
            provider,
            Some(RemoteDesktopSecret::from("shutdown-test-secret")),
            std::env::temp_dir().join("oxideterm-shutdown-test-certificates.json"),
            RemoteDesktopFrameDeliverySlot::new(),
            window.into(),
        );
        session.worker_wake = Some(worker_wake);
        session.worker = Some(RemoteDesktopWorkerOwner::new(
            request_tx,
            thread::spawn(|| {}),
        ));
        session
    });

    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    // The native window is gone; repeated cleanup still releases the worker
    // and credentials exactly once without accessing its stale handle.
    session.update(cx, |session, cx| session.shutdown(None, cx));
    session.update(cx, |session, cx| session.shutdown(None, cx));

    assert!(observed_wake.is_stopped());
    assert!(matches!(
        request_rx.recv().unwrap(),
        RemoteDesktopHelperRequest::ReleaseAllInputs
    ));
    assert!(matches!(
        request_rx.recv().unwrap(),
        RemoteDesktopHelperRequest::Close
    ));
    assert!(request_rx.try_recv().is_err());
    let password_consumed = cx.read(|cx| session.read(cx).password.is_none());
    assert!(password_consumed);
}
