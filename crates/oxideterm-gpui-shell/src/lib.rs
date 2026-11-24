//! Platform-independent workspace chrome. Hosts provide feature content and own
//! window services; the shell owns frame layout, activity rails and drawer geometry.
mod assets;
mod frame;
mod host;
mod rails;
pub mod zen;

pub use assets::PortableAssets;
pub use frame::WorkspaceShell;
pub use host::{ShellContent, ShellEnvironment, ShellFrameSlots, ShellHost, ShellRequest};
pub use rails::{
    ContextSidebarPanel, RailElements, RailSide, RailState, SidebarSection, WorkspaceRails,
    activity_rail, clamp_responsive_sidebar_width, rail_resize_capture,
    sidebar_resize_hotzone_origin, workspace_sidebar_resize_cursor,
};
