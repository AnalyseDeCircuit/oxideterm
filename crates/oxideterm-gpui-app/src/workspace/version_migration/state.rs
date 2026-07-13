use super::*;

pub(in crate::workspace) const VERSION_MIGRATION_DIALOG_MAX_WIDTH: f32 = 800.0;
pub(in crate::workspace) const VERSION_MIGRATION_DIALOG_MAX_HEIGHT: f32 = 720.0;
pub(in crate::workspace) const VERSION_MIGRATION_VIEWPORT_MARGIN: f32 = 24.0;
pub(in crate::workspace) const VERSION_MIGRATION_COMPACT_WIDTH: f32 = 660.0;
pub(in crate::workspace) const VERSION_MIGRATION_TOTAL_STEPS: usize = 6;
pub(in crate::workspace) const VERSION_MIGRATION_PROGRESS_STEP_SIZE: f32 = 36.0;
pub(in crate::workspace) const VERSION_MIGRATION_PAGE_RAIL_WIDTH: f32 = 224.0;
pub(in crate::workspace) const VERSION_MIGRATION_CLI_COMMAND_WIDTH: f32 = 96.0;
pub(in crate::workspace) const VERSION_MIGRATION_CLI_PATH_MIN_WIDTH: f32 = 120.0;

#[derive(Clone)]
pub(in crate::workspace) struct VersionMigrationState {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) step: usize,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) scroll_handle: ScrollHandle,
}

impl VersionMigrationState {
    pub(in crate::workspace) fn from_settings_path(
        settings_path: &std::path::Path,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            open: crate::migration_snapshot::pre_2_0_migration_notice_pending(settings_path)?,
            step: 0,
            error: None,
            scroll_handle: ScrollHandle::new(),
        })
    }

    pub(in crate::workspace) fn reset_for_open(&mut self) {
        // Manual reopening starts from the overview without changing the
        // acknowledgement marker that controls automatic presentation.
        self.open = true;
        self.step = 0;
        self.error = None;
        self.scroll_handle = ScrollHandle::new();
    }
}
