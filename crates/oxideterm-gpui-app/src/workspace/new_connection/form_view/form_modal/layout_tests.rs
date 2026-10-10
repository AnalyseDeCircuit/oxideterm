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
