use super::*;
use crate::form_state::NewConnectionTransport;
use gpui::{Context, ParentElement, Render, Styled, px};
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
    let rail = view.read_with(cx, |view, cx| {
        view.views
            .form_scroll("new-connection-transport-scroll", cx)
    });
    assert_eq!(
        rail.max_offset().x,
        px(0.0),
        "protocol rail lays out vertically without horizontal scroll",
    );
    assert_eq!(
        rail.max_offset().y,
        px(0.0),
        "all built-in protocols fit in the vertical rail at desktop height",
    );
    assert!(
        cx.debug_bounds("studio-category-None").is_none(),
        "SSH modal replaces the 6-tab bar with inline summary disclosure rows",
    );
    let ssh_route_row = cx
        .debug_bounds("ssh-summary-Route")
        .expect("SSH modal renders Connection Route summary disclosure row");
    let ssh_security_row = cx
        .debug_bounds("ssh-summary-SshSecurity")
        .expect("SSH modal renders SSH Options summary disclosure row");
    assert!(
        ssh_route_row.left() >= rail.bounds().right(),
        "SSH summary rows live in the right pane beside the vertical protocol rail",
    );
    assert!(
        ssh_security_row.top() > ssh_route_row.top(),
        "SSH summary rows stack vertically as a hairline disclosure list",
    );
    cx.simulate_click(ssh_security_row.center(), gpui::Modifiers::default());
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
            Some(crate::form_state::ConnectionFormDrawer::SshSecurity),
        );
    });
    let ssh_back = cx
        .debug_bounds("ssh-subpage-back")
        .expect("SSH subpage renders a back header bar");
    cx.simulate_click(ssh_back.center(), gpui::Modifiers::default());
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
            None,
        );
    });

    // Narrow split-screen viewport collapses the protocol sidebar to a compact
    // icon rail and folds SSH controls cleanly without horizontal overflow.
    cx.simulate_resize(gpui::size(px(540.0), px(480.0)));
    draw(cx);
    let (narrow_rail, narrow_form) = view.read_with(cx, |view, cx| {
        (
            view.views
                .form_scroll("new-connection-transport-scroll", cx),
            view.views
                .form_scroll("new-connection-modal-form-scroll", cx),
        )
    });
    assert!(
        narrow_rail.bounds().size.width <= px(60.0),
        "protocol sidebar collapses to a compact icon rail on narrow viewports",
    );
    assert_eq!(
        narrow_form.max_offset().x,
        px(0.0),
        "narrow SSH form folds cleanly without horizontal scrolling",
    );
    let narrow_route_row = cx
        .debug_bounds("ssh-summary-Route")
        .expect("SSH summary disclosure row remains mounted on narrow viewports");
    assert!(
        narrow_route_row.left() >= narrow_rail.bounds().right()
            && narrow_route_row.right() <= px(540.0),
        "SSH summary rows stay inside the narrow viewport bounds",
    );

    // Large desktop viewport restores the full labeled rail with zero horizontal overflow.
    cx.simulate_resize(gpui::size(px(1920.0), px(1080.0)));
    draw(cx);
    let (wide_rail, wide_form) = view.read_with(cx, |view, cx| {
        (
            view.views
                .form_scroll("new-connection-transport-scroll", cx),
            view.views
                .form_scroll("new-connection-modal-form-scroll", cx),
        )
    });
    assert!(
        wide_rail.bounds().size.width >= px(160.0),
        "protocol sidebar expands to its full labeled width on desktop viewports",
    );
    assert_eq!(
        wide_form.max_offset().x,
        px(0.0),
        "wide SSH form has zero horizontal overflow",
    );

    // Restore standard desktop size and verify non-SSH transports share summary-row navigation.
    cx.simulate_resize(gpui::size(px(1000.0), px(750.0)));
    draw(cx);
    let ftp_nav = cx
        .debug_bounds("new-connection-transport-Ftp")
        .expect("vertical protocol rail contains FTP / FTPS");
    cx.simulate_click(ftp_nav.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(
        cx.debug_bounds("studio-category-None").is_none(),
        "FTP modal uses summary disclosure rows instead of horizontal top tabs",
    );
    let ftp_options_row = cx
        .debug_bounds("ftp-summary-FtpOptions")
        .expect("FTP modal renders Connection Route summary disclosure row");
    cx.simulate_click(ftp_options_row.center(), gpui::Modifiers::default());
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
            Some(crate::form_state::ConnectionFormDrawer::FtpOptions),
        );
    });
    let ftp_back = cx
        .debug_bounds("ssh-subpage-back")
        .expect("FTP subpage renders a back header bar");
    cx.simulate_click(ftp_back.center(), gpui::Modifiers::default());
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
            None,
        );
    });
    let sftp_selector = "new-connection-transport-StandaloneSftp";
    let sftp = cx
        .debug_bounds(sftp_selector)
        .expect("vertical protocol rail contains standalone SFTP");
    assert!(
        rail.bounds().contains(&sftp.center()),
        "standalone SFTP is immediately reachable in the vertical rail without scrolling",
    );
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
        .expect("standalone SFTP keeps Remote Host 1 in its horizontal section tab bar");
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
        .expect("vertical protocol rail remains pinned outside scrolling fields");
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
        "the vertical protocol rail stays pinned outside the scrolling fields",
    );

    let secondary_nav = cx
        .debug_bounds("studio-category-Some(StandaloneSftpSecondary)")
        .expect("remote-to-remote mode shows Remote Host 2 in section tab bar");
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
