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
        gpui::div()
            .relative()
            .size_full()
            .child(self.views.form_body())
            .child(self.views.form_overlays())
    }
}

fn mount_form(
    cx: &mut Context<FormPreview>,
    setup: impl FnOnce(&mut NewConnectionForm),
) -> FormPreview {
    let theme = || {
        let mut tokens = oxideterm_theme::default_tokens();
        tokens.apply_motion(oxideterm_theme::UiMotionProfile::Off);
        ConnectionsEnvironment {
            tokens,
            i18n: I18n::new(Locale::En),
        }
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
fn standalone_sftp_keeps_transport_switcher_and_scrolls_vertically(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        mount_form(cx, |form| {
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
        .expect("protocol rail contains standalone SFTP");
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
    let primary_nav = cx
        .debug_bounds("studio-category-Some(StandaloneSftpPrimary)")
        .expect("standalone SFTP keeps Remote Host 1 in its own category");
    cx.simulate_click(primary_nav.center(), gpui::Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.views
                .flow
                .read(cx)
                .form
                .form
                .as_ref()
                .unwrap()
                .active_drawer,
            Some(crate::form_state::ConnectionFormDrawer::StandaloneSftpPrimary),
        );
    });
    let rail_item = cx
        .debug_bounds("new-connection-transport-Ssh")
        .expect("protocol rail remains pinned outside scrolling fields");
    let viewport = fields.bounds();
    assert!(viewport.size.height > px(0.0) && viewport.size.height < px(460.0));
    assert!(fields.max_offset().y > px(0.0));
    assert_eq!(fields.max_offset().x, px(0.0));

    cx.simulate_event(gpui::ScrollEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-4000.0))),
        ..Default::default()
    });
    draw(cx);
    assert_eq!(
        fields.offset(),
        gpui::point(px(0.0), -fields.max_offset().y)
    );
    assert_eq!(
        cx.debug_bounds("new-connection-transport-Ssh").unwrap(),
        rail_item,
        "the protocol rail stays pinned outside the scrolling fields",
    );

    let secondary_nav = cx
        .debug_bounds("studio-category-Some(StandaloneSftpSecondary)")
        .expect("remote-to-remote mode shows Remote Host 2 in category sidebar");
    cx.simulate_click(secondary_nav.center(), gpui::Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.views
                .flow
                .read(cx)
                .form
                .form
                .as_ref()
                .unwrap()
                .active_drawer,
            Some(crate::form_state::ConnectionFormDrawer::StandaloneSftpSecondary),
        );
    });
}
