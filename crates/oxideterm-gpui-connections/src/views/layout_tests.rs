use super::*;
use crate::form_state::NewConnectionTransport;
use gpui::{Context, Render, px};
use oxideterm_i18n::Locale;

struct FormPreview {
    views: ConnectionsViews,
    _runtime: tokio::runtime::Runtime,
}

impl Render for FormPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.views.form_body()
    }
}

fn mount_form(
    cx: &mut Context<FormPreview>,
    setup: impl FnOnce(&mut NewConnectionForm),
) -> FormPreview {
    let theme = || ConnectionsEnvironment {
        tokens: oxideterm_theme::default_tokens(),
        i18n: I18n::new(Locale::En),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let bindings = ConnectionsViewBindings {
        environment: Rc::new(move |_| Some(theme())),
        ime: Rc::new(|input, _| {
            let ConnectionsInput::Form(field) = input else {
                return None;
            };
            Some(ConnectionImeFrame {
                anchor_id: TextInputAnchorId(field as u64),
                viewport: TextInputViewport::default(),
                caret_visible: false,
                selected_range: None,
                active_offset: None,
                marked_text: None,
                marked_range: None,
                notes_with_composition: None,
                update_anchor: Rc::new(|_| {}),
            })
        }),
        selectable_text: Rc::new(|label, value, is_label, _, _| {
            gpui::div()
                .child(if is_label {
                    label.to_owned()
                } else {
                    value.to_owned()
                })
                .into_any_element()
        }),
        dispatch: Rc::new(|_, _, _| {}),
        form_environment: Rc::new(|_| Some(ConnectionFormEnvironment::default())),
        form_selectable_text: Rc::new(|_, _, value, _, _| {
            gpui::div().child(value).into_any_element()
        }),
        manager: crate::session_manager::SessionManagerViewBindings {
            catalog: Rc::new(|_, _| Some(Default::default())),
            oxide_environment: Rc::new(|_| Some(Default::default())),
            ime: Rc::new(|_, _| None),
            text: Rc::new(|_, _, _, value, _, _, _| gpui::div().child(value).into_any_element()),
        },
    };
    let views = ConnectionsViews::new(bindings, theme(), runtime.handle().clone(), cx);
    views.flow.update(cx, |flow, cx| {
        let mut draft = NewConnectionForm::default();
        setup(&mut draft);
        flow.form.replace_with_new_form(draft);
        cx.notify();
    });
    FormPreview {
        views,
        _runtime: runtime,
    }
}

#[gpui::test]
fn standalone_sftp_keeps_advanced_navigation_and_scrolls_both_axes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        mount_form(cx, |form| {
            form.advanced_connections_expanded = true;
            form.set_standalone_sftp_transfer_mode(
                oxideterm_connections::StandaloneSftpTransferMode::RemoteRemote,
            );
        })
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

    let fields = view.read_with(cx, |view, cx| {
        assert_eq!(
            view.views
                .flow
                .read(cx)
                .form
                .form
                .as_ref()
                .unwrap()
                .transport,
            NewConnectionTransport::StandaloneSftp,
        );
        view.views.form_scroll("standalone-sftp-form-scroll", cx)
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

    let navigation = view.read_with(cx, |view, app| {
        view.views
            .form_scroll("new-connection-transport-scroll", app)
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

#[gpui::test]
#[ignore = "manual connection form layout benchmark"]
fn connection_form_layout_benchmark(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| mount_form(cx, |_| {}));
    cx.simulate_resize(gpui::size(px(1280.), px(1000.)));
    for transport in [
        NewConnectionTransport::Ssh,
        NewConnectionTransport::Mosh,
        NewConnectionTransport::Telnet,
    ] {
        view.update(cx, |view, cx| {
            view.views.flow.update(cx, |flow, cx| {
                flow.form.form.as_mut().unwrap().transport = transport;
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
