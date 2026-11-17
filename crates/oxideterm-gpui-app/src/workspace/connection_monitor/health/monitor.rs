use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::connection_monitor) fn monitor_render_context(
        &self,
        cx: &mut Context<Self>,
    ) -> MonitorRenderContext {
        // This snapshot is frame-scoped. Cloning I18n shares its catalog Arc
        // and does not duplicate the locale tables.
        MonitorRenderContext {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            mono_font_family: settings_mono_font_family(self.settings_store.settings()),
            selectable_text: self.host_tools_selectable_text(cx),
            sidebar_width: self.ai_entity.read(cx).chat_ui().sidebar_width,
        }
    }

    pub(in crate::workspace::connection_monitor) fn render_host_monitor_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let render = self.monitor_render_context(cx);
        let monitor_enabled = self.settings_store.settings().host_tools.monitor_enabled;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_monitor_panel(&render, monitor_enabled, cx)
        })
    }
}
