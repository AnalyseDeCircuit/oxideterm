// Keep dialog slices included in the parent SFTP module so the extracted
// renderers can share the same private Tauri-port state without widening
// visibility just for the refactor.
include!("dialogs/shell.rs");
include!("dialogs/conflict.rs");
include!("dialogs/diff.rs");
include!("dialogs/preview.rs");
