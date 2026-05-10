use std::collections::HashMap;

use serde_json::Value;

const EN_PARTS: &[&str] = &[
    include_str!("../locales/en/common.json"),
    include_str!("../locales/en/menu.json"),
    include_str!("../locales/en/sidebar.json"),
    include_str!("../locales/en/settings.json"),
    include_str!("../locales/en/settings_view.json"),
    include_str!("../locales/en/sessionManager.json"),
    include_str!("../locales/en/forwards.json"),
    include_str!("../locales/en/sftp.json"),
    include_str!("../locales/en/ssh.json"),
    include_str!("../locales/en/terminal.json"),
    include_str!("../locales/en/ide.json"),
    include_str!("../locales/en/fileManager.json"),
];
const DE_PARTS: &[&str] = &[
    include_str!("../locales/de/common.json"),
    include_str!("../locales/de/menu.json"),
    include_str!("../locales/de/sidebar.json"),
    include_str!("../locales/de/settings.json"),
    include_str!("../locales/de/settings_view.json"),
    include_str!("../locales/de/sessionManager.json"),
    include_str!("../locales/de/forwards.json"),
    include_str!("../locales/de/sftp.json"),
    include_str!("../locales/de/ssh.json"),
    include_str!("../locales/de/terminal.json"),
    include_str!("../locales/de/ide.json"),
    include_str!("../locales/de/fileManager.json"),
];
const ES_ES_PARTS: &[&str] = &[
    include_str!("../locales/es-ES/common.json"),
    include_str!("../locales/es-ES/menu.json"),
    include_str!("../locales/es-ES/sidebar.json"),
    include_str!("../locales/es-ES/settings.json"),
    include_str!("../locales/es-ES/settings_view.json"),
    include_str!("../locales/es-ES/sessionManager.json"),
    include_str!("../locales/es-ES/forwards.json"),
    include_str!("../locales/es-ES/sftp.json"),
    include_str!("../locales/es-ES/ssh.json"),
    include_str!("../locales/es-ES/terminal.json"),
    include_str!("../locales/es-ES/ide.json"),
    include_str!("../locales/es-ES/fileManager.json"),
];
const FR_FR_PARTS: &[&str] = &[
    include_str!("../locales/fr-FR/common.json"),
    include_str!("../locales/fr-FR/menu.json"),
    include_str!("../locales/fr-FR/sidebar.json"),
    include_str!("../locales/fr-FR/settings.json"),
    include_str!("../locales/fr-FR/settings_view.json"),
    include_str!("../locales/fr-FR/sessionManager.json"),
    include_str!("../locales/fr-FR/forwards.json"),
    include_str!("../locales/fr-FR/sftp.json"),
    include_str!("../locales/fr-FR/ssh.json"),
    include_str!("../locales/fr-FR/terminal.json"),
    include_str!("../locales/fr-FR/ide.json"),
    include_str!("../locales/fr-FR/fileManager.json"),
];
const IT_PARTS: &[&str] = &[
    include_str!("../locales/it/common.json"),
    include_str!("../locales/it/menu.json"),
    include_str!("../locales/it/sidebar.json"),
    include_str!("../locales/it/settings.json"),
    include_str!("../locales/it/settings_view.json"),
    include_str!("../locales/it/sessionManager.json"),
    include_str!("../locales/it/forwards.json"),
    include_str!("../locales/it/sftp.json"),
    include_str!("../locales/it/ssh.json"),
    include_str!("../locales/it/terminal.json"),
    include_str!("../locales/it/ide.json"),
    include_str!("../locales/it/fileManager.json"),
];
const JA_PARTS: &[&str] = &[
    include_str!("../locales/ja/common.json"),
    include_str!("../locales/ja/menu.json"),
    include_str!("../locales/ja/sidebar.json"),
    include_str!("../locales/ja/settings.json"),
    include_str!("../locales/ja/settings_view.json"),
    include_str!("../locales/ja/sessionManager.json"),
    include_str!("../locales/ja/forwards.json"),
    include_str!("../locales/ja/sftp.json"),
    include_str!("../locales/ja/ssh.json"),
    include_str!("../locales/ja/terminal.json"),
    include_str!("../locales/ja/ide.json"),
    include_str!("../locales/ja/fileManager.json"),
];
const KO_PARTS: &[&str] = &[
    include_str!("../locales/ko/common.json"),
    include_str!("../locales/ko/menu.json"),
    include_str!("../locales/ko/sidebar.json"),
    include_str!("../locales/ko/settings.json"),
    include_str!("../locales/ko/settings_view.json"),
    include_str!("../locales/ko/sessionManager.json"),
    include_str!("../locales/ko/forwards.json"),
    include_str!("../locales/ko/sftp.json"),
    include_str!("../locales/ko/ssh.json"),
    include_str!("../locales/ko/terminal.json"),
    include_str!("../locales/ko/ide.json"),
    include_str!("../locales/ko/fileManager.json"),
];
const PT_BR_PARTS: &[&str] = &[
    include_str!("../locales/pt-BR/common.json"),
    include_str!("../locales/pt-BR/menu.json"),
    include_str!("../locales/pt-BR/sidebar.json"),
    include_str!("../locales/pt-BR/settings.json"),
    include_str!("../locales/pt-BR/settings_view.json"),
    include_str!("../locales/pt-BR/sessionManager.json"),
    include_str!("../locales/pt-BR/forwards.json"),
    include_str!("../locales/pt-BR/sftp.json"),
    include_str!("../locales/pt-BR/ssh.json"),
    include_str!("../locales/pt-BR/terminal.json"),
    include_str!("../locales/pt-BR/ide.json"),
    include_str!("../locales/pt-BR/fileManager.json"),
];
const VI_PARTS: &[&str] = &[
    include_str!("../locales/vi/common.json"),
    include_str!("../locales/vi/menu.json"),
    include_str!("../locales/vi/sidebar.json"),
    include_str!("../locales/vi/settings.json"),
    include_str!("../locales/vi/settings_view.json"),
    include_str!("../locales/vi/sessionManager.json"),
    include_str!("../locales/vi/forwards.json"),
    include_str!("../locales/vi/sftp.json"),
    include_str!("../locales/vi/ssh.json"),
    include_str!("../locales/vi/terminal.json"),
    include_str!("../locales/vi/ide.json"),
    include_str!("../locales/vi/fileManager.json"),
];
const ZH_CN_PARTS: &[&str] = &[
    include_str!("../locales/zh-CN/common.json"),
    include_str!("../locales/zh-CN/menu.json"),
    include_str!("../locales/zh-CN/sidebar.json"),
    include_str!("../locales/zh-CN/settings.json"),
    include_str!("../locales/zh-CN/settings_view.json"),
    include_str!("../locales/zh-CN/sessionManager.json"),
    include_str!("../locales/zh-CN/forwards.json"),
    include_str!("../locales/zh-CN/sftp.json"),
    include_str!("../locales/zh-CN/ssh.json"),
    include_str!("../locales/zh-CN/terminal.json"),
    include_str!("../locales/zh-CN/ide.json"),
    include_str!("../locales/zh-CN/fileManager.json"),
];
const ZH_TW_PARTS: &[&str] = &[
    include_str!("../locales/zh-TW/common.json"),
    include_str!("../locales/zh-TW/menu.json"),
    include_str!("../locales/zh-TW/sidebar.json"),
    include_str!("../locales/zh-TW/settings.json"),
    include_str!("../locales/zh-TW/settings_view.json"),
    include_str!("../locales/zh-TW/sessionManager.json"),
    include_str!("../locales/zh-TW/forwards.json"),
    include_str!("../locales/zh-TW/sftp.json"),
    include_str!("../locales/zh-TW/ssh.json"),
    include_str!("../locales/zh-TW/terminal.json"),
    include_str!("../locales/zh-TW/ide.json"),
    include_str!("../locales/zh-TW/fileManager.json"),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Locale {
    De,
    En,
    EsEs,
    FrFr,
    It,
    Ja,
    Ko,
    PtBr,
    Vi,
    ZhCn,
    ZhTw,
}

#[derive(Clone, Debug)]
pub struct I18n {
    locale: Locale,
    fallback_locale: Locale,
    catalogs: HashMap<Locale, LocaleCatalog>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let mut catalogs = HashMap::new();
        catalogs.insert(Locale::De, LocaleCatalog::from_json_parts(DE_PARTS));
        catalogs.insert(Locale::En, LocaleCatalog::from_json_parts(EN_PARTS));
        catalogs.insert(Locale::EsEs, LocaleCatalog::from_json_parts(ES_ES_PARTS));
        catalogs.insert(Locale::FrFr, LocaleCatalog::from_json_parts(FR_FR_PARTS));
        catalogs.insert(Locale::It, LocaleCatalog::from_json_parts(IT_PARTS));
        catalogs.insert(Locale::Ja, LocaleCatalog::from_json_parts(JA_PARTS));
        catalogs.insert(Locale::Ko, LocaleCatalog::from_json_parts(KO_PARTS));
        catalogs.insert(Locale::PtBr, LocaleCatalog::from_json_parts(PT_BR_PARTS));
        catalogs.insert(Locale::Vi, LocaleCatalog::from_json_parts(VI_PARTS));
        catalogs.insert(Locale::ZhCn, LocaleCatalog::from_json_parts(ZH_CN_PARTS));
        catalogs.insert(Locale::ZhTw, LocaleCatalog::from_json_parts(ZH_TW_PARTS));

        Self {
            locale,
            fallback_locale: Locale::En,
            catalogs,
        }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    pub fn t(&self, key: &str) -> String {
        self.catalogs
            .get(&self.locale)
            .and_then(|catalog| catalog.get(key))
            .or_else(|| {
                self.catalogs
                    .get(&self.fallback_locale)
                    .and_then(|catalog| catalog.get(key))
            })
            .unwrap_or(key)
            .to_string()
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Locale::ZhCn)
    }
}

#[derive(Clone, Debug)]
struct LocaleCatalog {
    messages: HashMap<String, String>,
}

impl LocaleCatalog {
    fn from_json_parts(parts: &[&str]) -> Self {
        let mut messages = HashMap::new();
        for source in parts {
            let value: Value =
                serde_json::from_str(source).expect("invalid native locale catalog part");
            flatten_json("", &value, &mut messages);
        }
        Self { messages }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }
}

fn flatten_json(prefix: &str, value: &Value, messages: &mut HashMap<String, String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let key = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_json(&key, child, messages);
            }
        }
        Value::String(message) => {
            let previous = messages.insert(prefix.to_string(), message.clone());
            assert!(previous.is_none(), "duplicate native locale key: {prefix}");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_active_locale() {
        let mut i18n = I18n::default();
        assert_eq!(i18n.t("menu.new_terminal"), "新建终端");

        i18n.set_locale(Locale::En);
        assert_eq!(i18n.t("menu.new_terminal"), "New Terminal");
    }

    #[test]
    fn falls_back_to_english_then_key() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("missing.key"), "missing.key");
    }

    #[test]
    fn split_catalogs_keep_expected_domains() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("ssh.form.title"), "新建连接");
        assert_eq!(i18n.t("sidebar.panels.sessions"), "活动会话");
        assert_eq!(i18n.t("terminal.local_terminal"), "本地终端");
        assert_eq!(i18n.t("terminal.trzsz.completed_title"), "传输已完成");
    }

    #[test]
    #[should_panic(expected = "duplicate native locale key")]
    fn duplicate_keys_are_rejected() {
        let _ = LocaleCatalog::from_json_parts(&[
            r#"{"menu":{"copy":"Copy"}}"#,
            r#"{"menu":{"copy":"Duplicate"}}"#,
        ]);
    }

    #[test]
    fn all_eleven_locales_load_without_panic() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        for locale in locales {
            let i18n = I18n::new(locale);
            let name = i18n.t("app.name");
            assert_eq!(name, "OxideTerm");
        }
    }

    #[test]
    fn ssh_connection_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "ssh.form.checking_host_key",
            "ssh.form.test_running",
            "ssh.form.test_success",
            "ssh.form.default_key_desc",
            "ssh.form.passphrase",
            "ssh.form.passphrase_placeholder",
            "ssh.form.key_file",
            "ssh.form.certificate_note",
            "ssh.form.private_key",
            "ssh.form.certificate",
            "ssh.form.agent_desc",
            "ssh.form.two_factor_desc",
            "ssh.form.key_path_required",
            "ssh.form.certificate_paths_required",
            "ssh.form.key_path_not_ready",
            "ssh.form.certificate_not_ready",
            "ssh.form.keyboard_interactive_not_ready",
            "ssh.host_key.title_unknown",
            "ssh.host_key.title_changed",
            "ssh.host_key.title_error",
            "ssh.host_key.unknown_message",
            "ssh.host_key.changed_warning",
            "ssh.host_key.key_type_label",
            "ssh.host_key.fingerprint_label",
            "ssh.host_key.expected_fingerprint",
            "ssh.host_key.actual_fingerprint",
            "ssh.host_key.cancelled",
            "ssh.host_key.changed_requires_remove",
            "ssh.host_key.actions.cancel",
            "ssh.host_key.actions.trust_once",
            "ssh.host_key.actions.trust_save",
            "ssh.kbi.title",
            "ssh.kbi.continue",
            "ssh.kbi.cancelled",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn sidebar_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "sidebar.active_sessions",
            "sidebar.no_sessions",
            "sidebar.add_new_connection",
            "sidebar.actions.expand",
            "sidebar.actions.collapse",
            "sidebar.actions.new_local_terminal",
            "sidebar.actions.open_pylance_guide",
            "sidebar.panels.sessions",
            "sidebar.panels.saved",
            "sidebar.panels.saved_connections",
            "sidebar.panels.saved_title",
            "sidebar.panels.sftp",
            "sidebar.panels.sftp_sessions",
            "sidebar.panels.files",
            "sidebar.panels.forwards",
            "sidebar.panels.forwards_title",
            "sidebar.panels.connection_pool",
            "sidebar.panels.connection_monitor",
            "sidebar.panels.connection_matrix",
            "sidebar.panels.ai",
            "sidebar.panels.ai_assistant",
            "sidebar.panels.settings",
            "sidebar.panels.no_saved_connections",
            "sidebar.panels.no_active_sessions",
            "sidebar.panels.no_connected_sessions",
            "sidebar.panels.import_tooltip",
            "sidebar.panels.export_tooltip",
            "sidebar.panels.system_health",
            "sidebar.panels.session_manager",
            "sidebar.panels.open_session_manager",
            "sidebar.panels.plugins",
            "sidebar.panels.activity",
            "sidebar.panels.event_log",
            "sidebar.panels.notifications",
            "sidebar.tooltips.settings",
            "sidebar.tooltips.ai_hint",
            "sidebar.tooltips.switch_tree",
            "sidebar.tooltips.switch_focus",
            "sidebar.tooltips.auto_route",
            "sidebar.tooltips.new_connection",
            "sidebar.tooltips.collapse",
            "sessions.tree.no_sessions",
            "sessions.tree.click_to_add",
            "sessions.tree.actions.new_terminal",
            "sessions.tree.actions.sftp",
            "sessions.tree.actions.port_forwarding",
            "sessions.tree.actions.disconnect",
            "sessions.tree.actions.reconnect",
            "sessions.tree.actions.drill_in",
            "sessions.focused_list.terminal",
            "ssh.drill_down.title",
            "ssh.drill_down.description",
            "ssh.drill_down.target_host",
            "ssh.drill_down.port",
            "ssh.drill_down.username",
            "ssh.drill_down.auth_method",
            "ssh.drill_down.connect",
            "ssh.drill_down.connecting",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn forwarding_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "copy_address",
            "copied",
            "forwards.quick.title",
            "forwards.quick.jupyter",
            "forwards.quick.tensorboard",
            "forwards.quick.vscode",
            "forwards.table.title",
            "forwards.table.type",
            "forwards.table.local_address",
            "forwards.table.remote_address",
            "forwards.table.status",
            "forwards.table.actions",
            "forwards.table.no_forwards",
            "forwards.type.local",
            "forwards.type.remote",
            "forwards.type.dynamic",
            "forwards.status.active",
            "forwards.status.stopped",
            "forwards.status.failed",
            "forwards.status.starting",
            "forwards.status.error",
            "forwards.status.suspended",
            "forwards.status.suspended_hint",
            "forwards.actions.new_forward",
            "forwards.actions.stop",
            "forwards.actions.restart",
            "forwards.actions.edit",
            "forwards.actions.delete",
            "forwards.actions.will_recover",
            "forwards.actions.confirm_delete_title",
            "forwards.actions.confirm_delete_desc",
            "forwards.form.new_title",
            "forwards.form.edit_title",
            "forwards.form.cancel",
            "forwards.form.type",
            "forwards.form.create_forward",
            "forwards.form.creating",
            "forwards.form.save_changes",
            "forwards.form.saving",
            "forwards.form.type_local",
            "forwards.form.type_remote",
            "forwards.form.type_dynamic",
            "forwards.form.local_client",
            "forwards.form.remote_server",
            "forwards.form.bind_address",
            "forwards.form.target_address",
            "forwards.form.socks5_mode",
            "forwards.form.host_placeholder",
            "forwards.form.port_placeholder",
            "forwards.form.skip_check",
            "forwards.form.checking_port",
            "forwards.form.port_required",
            "forwards.form.port_invalid",
            "forwards.form.desc",
            "forwards.toast.jupyter_created",
            "forwards.toast.jupyter_desc",
            "forwards.toast.tensorboard_created",
            "forwards.toast.tensorboard_desc",
            "forwards.toast.vscode_created",
            "forwards.toast.vscode_desc",
            "forwards.toast.create_failed",
            "forwards.toast.forward_updated",
            "forwards.toast.update_failed",
            "forwards.toast.suspended_title",
            "forwards.toast.suspended_desc",
            "forwards.toast.error_title",
            "forwards.toast.session_suspended_title",
            "forwards.toast.session_suspended_desc",
            "forwards.messages.node_not_ready",
            "forwards.messages.connection_not_ready",
            "forwards.messages.created",
            "forwards.messages.updated",
            "forwards.messages.stopped",
            "forwards.messages.restarted",
            "forwards.messages.deleted",
            "forwards.detection.detected",
            "forwards.detection.port",
            "forwards.detection.forward",
            "forwards.detection.auto",
            "forwards.detection.forwarded",
            "forwards.detection.forwardError",
            "forwards.detection.remotePorts",
            "forwards.detection.bindAddr",
            "forwards.detection.process",
            "forwards.detection.action",
            "forwards.detection.scanning",
            "forwards.detection.noPorts",
            "forwards.detection.alreadyForwarded",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn ide_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "ide.loading_project",
            "ide.open_failed",
            "ide.retry",
            "ide.disconnected_overlay",
            "ide.no_open_files",
            "ide.click_to_open",
            "ide.loading_file",
            "ide.save_failed",
            "ide.unsaved_changes",
            "ide.unsaved_changes_desc",
            "ide.save",
            "ide.discard",
            "ide.cancel",
            "ide.agent_status_sftp",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn sftp_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "sftp.file_list.local",
            "sftp.file_list.remote",
            "sftp.file_list.filter_placeholder",
            "sftp.file_list.path_placeholder",
            "sftp.file_list.col_name",
            "sftp.file_list.col_size",
            "sftp.file_list.col_modified",
            "sftp.toolbar.show_drives",
            "sftp.toolbar.go_up",
            "sftp.toolbar.home",
            "sftp.toolbar.refresh",
            "sftp.toolbar.upload_count",
            "sftp.toolbar.download_count",
            "sftp.context.preview",
            "sftp.context.rename",
            "sftp.context.delete",
            "sftp.context.new_folder",
            "sftp.dialogs.select_drive",
            "sftp.dialogs.rename",
            "sftp.dialogs.new_folder",
            "sftp.dialogs.delete",
            "sftp.conflict.title",
            "sftp.conflict.overwrite",
            "sftp.diff.title",
            "sftp.preview.description",
            "sftp.queue.title",
            "sftp.queue.clear_done",
            "sftp.queue.incomplete_title",
            "sftp.queue.status_waiting",
            "sftp.queue.status_paused",
            "sftp.queue.status_completed",
            "sftp.queue.status_cancelled",
            "sftp.queue.status_error",
            "sftp.queue.empty",
            "sftp.errors.connection_lost",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn file_manager_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "fileManager.title",
            "fileManager.pathPlaceholder",
            "fileManager.showDrives",
            "fileManager.newFolder",
            "fileManager.newFile",
            "fileManager.openTerminalHere",
            "fileManager.compress",
            "fileManager.extract",
            "fileManager.favorites",
            "fileManager.addBookmark",
            "fileManager.removeBookmark",
            "fileManager.properties",
            "fileManager.confirmDelete",
            "fileManager.preview",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn terminal_trzsz_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "terminal.trzsz.select_upload_directory_title",
            "terminal.trzsz.select_upload_directory_description",
            "terminal.trzsz.select_upload_files_title",
            "terminal.trzsz.select_upload_files_description",
            "terminal.trzsz.select_download_directory_title",
            "terminal.trzsz.select_download_directory_description",
            "terminal.trzsz.cancelled_title",
            "terminal.trzsz.cancelled_description",
            "terminal.trzsz.completed_title",
            "terminal.trzsz.completed_description",
            "terminal.trzsz.failed_title",
            "terminal.trzsz.failed_description",
            "terminal.trzsz.version_mismatch_title",
            "terminal.trzsz.path_invalid_title",
            "terminal.trzsz.symlink_not_supported_title",
            "terminal.trzsz.conflict_detected_title",
            "terminal.trzsz.directory_not_allowed_title",
            "terminal.trzsz.max_file_count_title",
            "terminal.trzsz.max_total_bytes_title",
            "terminal.trzsz.disabled_title",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn language_names_are_autonyms_in_every_locale() {
        let expected = [
            ("language.english", "English"),
            ("language.simplified_chinese", "简体中文"),
            ("language.traditional_chinese", "繁體中文"),
            ("language.german", "Deutsch"),
            ("language.spanish", "Español"),
            ("language.french", "Français"),
            ("language.italian", "Italiano"),
            ("language.japanese", "日本語"),
            ("language.korean", "한국어"),
            ("language.portuguese_brazil", "Português (Brasil)"),
            ("language.vietnamese", "Tiếng Việt"),
        ];
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for (key, value) in expected {
                assert_eq!(i18n.t(key), value);
            }
        }
    }
}
