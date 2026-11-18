#[test]
fn secret_render_projections_do_not_copy_entity_owned_plaintext() {
    let portable_source =
        include_str!("../../../../oxideterm-gpui-settings-view/src/entity/portable.rs");
    let managed_key_source =
        include_str!("../../../../oxideterm-gpui-settings-view/src/entity/connections.rs");
    let proxy_source =
        include_str!("../../../../oxideterm-gpui-settings-view/src/entity/network.rs");

    for forbidden in [
        concat!("portable_current_password", ".to_string()"),
        concat!("portable_new_password", ".to_string()"),
        concat!("portable_confirm_password", ".to_string()"),
    ] {
        assert!(!portable_source.contains(forbidden), "{forbidden}");
    }
    for forbidden in [
        concat!(
            "file_passphrase: self.managed_key_file_passphrase",
            ".clone()"
        ),
        concat!(
            "private_key: self.managed_key_paste_private_key",
            ".clone()"
        ),
        concat!("passphrase: self.managed_key_paste_passphrase", ".clone()"),
    ] {
        assert!(!managed_key_source.contains(forbidden), "{forbidden}");
    }
    assert!(!proxy_source.contains(concat!("password: self.network_proxy_password", ".clone()")));
}

#[test]
fn workspace_drop_zeroizes_the_transient_settings_ime_draft() {
    let workspace_source = include_str!("../../workspace.rs");

    assert!(workspace_source.contains("impl Drop for WorkspaceApp"));
    assert!(workspace_source.contains("zeroize::Zeroize::zeroize(&mut self.settings_input_draft)"));
}
