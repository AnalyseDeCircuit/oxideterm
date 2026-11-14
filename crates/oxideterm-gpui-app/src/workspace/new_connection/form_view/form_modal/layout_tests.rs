use super::*;
use gpui::{AppContext, Entity, Render};

struct FormPreview(Entity<WorkspaceApp>);

impl Render for FormPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |workspace, cx| {
            workspace.render_new_connection_modal(window, cx)
        })
    }
}

#[gpui::test]
fn standalone_sftp_keeps_advanced_navigation_and_scrolls_both_axes(cx: &mut gpui::TestAppContext) {
    let Some(_) = crate::workspace::session_manager::tests::isolated_workspace_fixture(
        cx,
        "OXIDETERM_STANDALONE_SFTP_LAYOUT_TEST_DIR",
    ) else {
        return;
    };
    cx.executor().allow_parking();
    let (view, cx) = cx.add_window_view(|window, cx| {
        FormPreview(cx.new(|cx| {
            let workspace = WorkspaceApp::new(window, cx, None, None).unwrap();
            workspace.update_connection_form_state(cx, |state| {
                let mut form = NewConnectionForm::default();
                form.advanced_connections_expanded = true;
                form.set_standalone_sftp_transfer_mode(StandaloneSftpTransferMode::RemoteRemote);
                state.replace_with_new_form(form);
            });
            workspace
        }))
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, app| window.draw(app).clear(app));
    };
    cx.simulate_resize(gpui::size(px(1000.0), px(750.0)));
    draw(cx);
    let sftp = cx
        .debug_bounds("new-connection-transport-StandaloneSftp")
        .expect("expanded advanced group contains standalone SFTP");
    cx.simulate_click(sftp.center(), gpui::Modifiers::default());

    let workspace = view.read_with(cx, |view, _| view.0.clone());
    let fields = workspace.read_with(cx, |workspace, cx| {
        assert_eq!(
            workspace
                .connection_form_state(cx)
                .form
                .as_ref()
                .unwrap()
                .transport,
            NewConnectionTransport::StandaloneSftp,
        );
        workspace.selectable_text_scroll_handle("standalone-sftp-form-scroll")
    });
    cx.simulate_resize(gpui::size(px(960.0), px(460.0)));
    draw(cx);
    let advanced = cx.debug_bounds("new-connection-advanced-group").unwrap();
    let viewport = fields.bounds();
    assert!(viewport.size.height > px(0.0) && viewport.size.height < px(460.0));
    assert!(fields.max_offset().y > px(0.0));
    assert!(fields.max_offset().x > px(0.0));

    for (delta, expected) in [
        (
            gpui::point(px(0.0), px(-4000.0)),
            gpui::point(px(0.0), -fields.max_offset().y),
        ),
        (
            gpui::point(px(-4000.0), px(0.0)),
            gpui::point(-fields.max_offset().x, -fields.max_offset().y),
        ),
    ] {
        cx.simulate_event(gpui::ScrollEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(delta),
            ..Default::default()
        });
        draw(cx);
        assert_eq!(fields.offset(), expected);
        assert_eq!(
            cx.debug_bounds("new-connection-advanced-group").unwrap(),
            advanced,
            "the advanced group stays outside the scrolling fields",
        );
    }

    let navigation = workspace.read_with(cx, |workspace, _| {
        workspace.selectable_text_scroll_handle("new-connection-transport-scroll")
    });
    let navigation_viewport = navigation.bounds();
    assert!(!navigation_viewport.contains(&advanced.center()));
    cx.simulate_event(gpui::ScrollEvent {
        position: navigation_viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-4000.0))),
        ..Default::default()
    });
    draw(cx);
    assert!(
        navigation_viewport.contains(
            &cx.debug_bounds("new-connection-advanced-group")
                .unwrap()
                .center()
        ),
        "the advanced group can be reached even when the dialog is short",
    );
}

// Run with APPDATA pointing to an empty directory under target. The benchmark
// mounts the real form without opening any network or terminal session.
#[gpui::test]
#[ignore = "manual connection form layout benchmark with an isolated APPDATA directory"]
fn connection_form_layout_benchmark(cx: &mut gpui::TestAppContext) {
    // Workspace initialization owns real I/O tasks; this manual benchmark uses
    // wall-clock measurements rather than the deterministic scheduler contract.
    cx.executor().allow_parking();
    let settings_path = oxideterm_settings::default_settings_path();
    assert!(
        settings_path.starts_with(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("target")
        ),
        "use isolated APPDATA under target"
    );
    let (view, cx) = cx.add_window_view(|window, cx| {
        FormPreview(cx.new(|cx| {
            let workspace = WorkspaceApp::new(window, cx, None, None).unwrap();
            workspace.update_connection_form_state(cx, |state| {
                state.replace_with_new_form(NewConnectionForm::default())
            });
            workspace
        }))
    });
    cx.simulate_resize(gpui::size(px(1280.), px(1000.)));
    for transport in [
        NewConnectionTransport::Ssh,
        NewConnectionTransport::Mosh,
        NewConnectionTransport::Telnet,
    ] {
        view.update(cx, |view, cx| {
            view.0.update(cx, |workspace, cx| {
                workspace.update_connection_form_state(cx, |state| {
                    state.form.as_mut().unwrap().transport = transport
                });
                cx.notify();
            })
        });
        let mut samples = Vec::new();
        for frame in 0..35 {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(16));
            let start = std::time::Instant::now();
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            if frame >= 5 {
                samples.push(start.elapsed().as_micros());
            }
        }
        samples.sort_unstable();
        println!(
            "form {transport:?} median_us={} p95_us={}",
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100]
        );
    }
}
