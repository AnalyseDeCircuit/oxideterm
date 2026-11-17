use super::*;

pub(super) fn monitor_center_state(
    app: &WorkspaceApp,
    icon: LucideIcon,
    color: u32,
    label: String,
    cx: &mut Context<WorkspaceApp>,
) -> AnyElement {
    let selectable_text = app.host_tools_selectable_text(cx);
    host_tools_center_state(icon.path(), color, label, &selectable_text, cx)
}
