impl IdeSurface {
    fn render_status_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let dirty_count = self
            .workspace
            .tabs()
            .iter()
            .filter(|tab| self.is_tab_dirty(tab.id, cx))
            .count();
        let active_path = self
            .workspace
            .active_tab()
            .and_then(|tab_id| self.workspace.buffer(tab_id))
            .map(|buffer| match &buffer.location {
                IdeLocation::Remote { path, .. } => path.clone(),
                IdeLocation::Local { path } => path.display().to_string(),
            })
            .unwrap_or_default();

        let project = div().flex().items_center().gap_2();
        #[cfg(not(target_family = "wasm"))]
        let project = project.child(self.render_agent_status_trigger(cx));
        #[cfg(not(target_family = "wasm"))]
        let project = project.when_some(self.desktop.transfer.as_ref(), |row, transfer| {
            row.child(format!("{} {}%", transfer.label, transfer.percent))
                .child(
                    div()
                        .id("ide-cancel-file-transfer")
                        .cursor_pointer()
                        .child(self.labels.cancel.clone())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_tree_transfer();
                            cx.notify();
                        })),
                )
        });
        div()
            .flex_none()
            .h(px(oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(if self.runtime_settings.background_active {
                rgba((self.tokens.ui.bg_panel << 8) | IDE_BG_ACTIVE_THEME_ALPHA)
            } else {
                rgb(self.tokens.ui.bg_panel)
            })
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                project
                    .when_some(self.git_branch.clone(), |this, branch| {
                        this.child(format!("git: {branch}"))
                    })
                    .when(dirty_count > 0, |this| {
                        this.child(format!("{dirty_count} unsaved"))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().truncate().child(active_path))
                    .child(self.render_file_format_triggers(cx)),
            )
            .into_any_element()
    }

    fn render_status_menu_item(
        &self,
        icon: AnyElement,
        label: String,
        danger: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let text_color = if danger {
            TAILWIND_RED_400
        } else {
            self.tokens.ui.text
        };
        div()
            .h(px(IDE_AGENT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_2()
            .gap_2()
            .cursor_pointer()
            .text_color(rgb(text_color))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(icon)
            .child(div().flex_1().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }
}
