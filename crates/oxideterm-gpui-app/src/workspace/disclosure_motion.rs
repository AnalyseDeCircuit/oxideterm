use super::*;
pub(super) use oxideterm_gpui_ui::disclosure_motion::DisclosureMotions;

impl WorkspaceApp {
    pub(super) fn begin_disclosure_motion(
        &mut self,
        key: String,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.tokens.motion.enabled {
            self.disclosure_motions.remove(&key);
            return;
        }
        let generation = self.disclosure_motions.begin(key.clone(), expanded);
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        // Only IDs and scalar motion state survive the transition, never message content.
        // The weak workspace handle prevents completion from retaining a closed window.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.disclosure_motions.finish(&key, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
