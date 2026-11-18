#[test]
fn workspace_drop_zeroizes_the_transient_settings_ime_draft() {
    let workspace_source = include_str!("../../workspace.rs");

    assert!(workspace_source.contains("impl Drop for WorkspaceApp"));
    assert!(workspace_source.contains("zeroize::Zeroize::zeroize(&mut self.settings_input_draft)"));
}
