use crate::host::Workspace;
use gpui::{AppContext, WindowKind, WindowOptions};
use oxideterm_gpui_shell::{PortableAssets, WorkspaceShell, shortcuts::bindings::KeybindingSide};
use std::borrow::Cow;

pub fn run() {
    let browser = web_sys::window().expect("browser window");
    let navigator = browser.navigator();
    let platform = navigator.platform().unwrap_or_default();
    let apple = if platform.is_empty() {
        navigator.user_agent().unwrap_or_default().contains("Mac")
    } else {
        ["Mac", "iPhone", "iPad", "iPod"]
            .iter()
            .any(|p| platform.starts_with(p))
    };
    let side = if apple {
        KeybindingSide::Mac
    } else {
        KeybindingSide::Other
    };
    side.install_browser_side();
    let gateway = web_sys::UrlSearchParams::new_with_str(
        &browser.location().search().expect("browser location"),
    )
    .expect("query string")
    .get("gateway");
    wasm_bindgen_futures::spawn_local(async move {
        let saved = crate::connections::storage::Snapshot::load(gateway.is_none()).await;
        open_workspace(side, gateway, saved);
    });
}

fn open_workspace(
    side: KeybindingSide,
    gateway: Option<String>,
    saved: Result<crate::connections::storage::Snapshot, ()>,
) {
    oxideterm_gpui_platform::application()
        .with_assets(PortableAssets)
        .run(move |cx| {
            cx.text_system()
                .add_fonts(vec![
                    Cow::Borrowed(include_bytes!(
                        "../../../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"
                    )),
                    Cow::Borrowed(include_bytes!(
                        "../../../assets/fonts/lilex/Lilex-Regular.ttf"
                    )),
                ])
                .expect("bundled workspace fonts");
            // Hosts serving CJK fonts can fetch their own licensed subset asynchronously,
            // register the bytes here via add_fonts, then refresh the window. Demo stays offline.
            cx.open_window(
                WindowOptions {
                    kind: WindowKind::Normal,
                    ..Default::default()
                },
                move |window, cx| {
                    let host = cx.new(|cx| Workspace::new(side, gateway, saved, window, cx));
                    host.update(cx, |h, cx| h.start(window, cx));
                    let focus = host.read(cx).focus.clone();
                    cx.new(|cx| {
                        cx.observe(&host, |_, _, cx| cx.notify()).detach();
                        WorkspaceShell::new(focus, move |w, cx| {
                            host.update(cx, |h, cx| h.frame(w, cx))
                        })
                    })
                },
            )
            .expect("browser workspace");
        });
}
