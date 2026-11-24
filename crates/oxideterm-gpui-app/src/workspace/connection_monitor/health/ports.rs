use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_ports_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::PortSearch, cx)
            .expect("port search is a non-secret Host Tools input");
        let sidebar_width = self.rails.right.width();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_ports_panel(
                search_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_port_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::PortSearch)
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.host_tools.update(cx, |host_tools, _cx| {
                host_tools.ui.clear_input_focus();
            });
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }
}
