// Keep action groups included in the parent SFTP module. This is a mechanical
// split: behavior stays in the original private module scope while the file
// boundaries now mirror the Tauri feature areas we keep porting.
include!("actions/navigation.rs");
include!("actions/preview_editor.rs");
include!("actions/menus_conflicts.rs");
include!("actions/transfers.rs");
include!("actions/dialog_lifecycle.rs");
include!("actions/external.rs");
