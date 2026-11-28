use super::*;

fn select_field(
    slot: Slot,
    label: &str,
    selected: &str,
    options: &[(&str, &str)],
    i18n: &I18n,
) -> EditorField {
    let mut field = EditorField::new(slot, label, "select", json!(selected));
    field.options = options
        .iter()
        .map(|(value, key)| (i18n.t(key), json!(value)))
        .collect();
    field
}

pub(super) fn auth_fields(
    source: &DiscoverySource,
    choices: &EditorChoices,
    i18n: &I18n,
) -> Vec<EditorField> {
    let auth = &source.ssh_preferences.auth;
    let family = match auth {
        DiscoverySshAuth::Password => "password",
        DiscoverySshAuth::Agent => "agent",
        DiscoverySshAuth::TwoFactor => "twoFactor",
        _ => "key",
    };
    let (key_source, key_path, cert_path, key_id) = match auth {
        DiscoverySshAuth::DefaultKey => ("default", "", "", ""),
        DiscoverySshAuth::KeyFile(path) => ("file", path.as_str(), "", ""),
        DiscoverySshAuth::Certificate {
            key_path,
            cert_path,
        } => ("certificate", key_path.as_str(), cert_path.as_str(), ""),
        DiscoverySshAuth::ManagedKey(id) => ("managed", "", "", id.as_str()),
        _ => ("file", "", "", ""),
    };
    let mut managed = EditorField::new(
        Slot::ManagedKey,
        "ssh.form.managed_key",
        "select",
        json!(key_id),
    );
    managed.options = choices
        .keys
        .iter()
        .map(|(name, id)| (name.clone(), json!(id)))
        .collect();
    let mut passphrase = EditorField::new(Slot::Passphrase, "ssh.form.passphrase", "password", "");
    passphrase.secret = Some(SecretDraft {
        scope: Vec::new(),
        revision: uuid::Uuid::new_v4(),
        value: Zeroizing::default(),
    });
    vec![
        select_field(
            Slot::SshAuth,
            "ssh.form.authentication",
            family,
            &[
                ("password", "ssh.auth.password"),
                ("key", "ssh.auth.key"),
                ("agent", "ssh.auth.agent"),
                ("twoFactor", "ssh.auth.two_factor"),
            ],
            i18n,
        ),
        select_field(
            Slot::KeySource,
            "ssh.auth.key_source",
            key_source,
            &[
                ("default", "ssh.auth.key_source_default"),
                ("file", "ssh.auth.key_source_file"),
                ("managed", "ssh.auth.key_source_managed"),
                ("certificate", "ssh.auth.key_source_certificate"),
            ],
            i18n,
        ),
        EditorField::new(Slot::KeyPath, "ssh.form.key_file", "path", key_path),
        EditorField::new(Slot::CertPath, "ssh.form.certificate", "path", cert_path),
        managed,
        passphrase,
        EditorField::new(
            Slot::IdentityAgent,
            "ssh.form.agent_endpoint",
            "text",
            source
                .ssh_preferences
                .identity_agent
                .as_deref()
                .unwrap_or_default(),
        ),
    ]
}

impl SourceEditor {
    fn ssh_field(&self, slot: Slot) -> Option<&EditorField> {
        self.fields.iter().find(|field| field.slot == slot)
    }

    pub(super) fn ssh_choice(&self, slot: Slot) -> Option<&str> {
        let field = self.ssh_field(slot)?;
        field
            .options
            .iter()
            .find(|(_, value)| value.to_string() == field.value)?
            .1
            .as_str()
    }

    pub(super) fn ssh_auth(&self) -> Option<DiscoverySshAuth> {
        let path = |slot| {
            let value = self.ssh_field(slot)?.value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        };
        Some(match self.ssh_choice(Slot::SshAuth)? {
            "password" => DiscoverySshAuth::Password,
            "agent" => DiscoverySshAuth::Agent,
            "twoFactor" => DiscoverySshAuth::TwoFactor,
            "key" => match self.ssh_choice(Slot::KeySource)? {
                "default" => DiscoverySshAuth::DefaultKey,
                "file" => DiscoverySshAuth::KeyFile(path(Slot::KeyPath)?),
                "managed" => {
                    DiscoverySshAuth::ManagedKey(self.ssh_choice(Slot::ManagedKey)?.to_owned())
                }
                "certificate" => DiscoverySshAuth::Certificate {
                    key_path: path(Slot::KeyPath)?,
                    cert_path: path(Slot::CertPath)?,
                },
                _ => return None,
            },
            _ => return None,
        })
    }
}

impl ConnectionPage<'_> {
    pub(super) fn discovery_key_path(
        &self,
        editor: &SourceEditor,
        index: usize,
        cx: &App,
    ) -> AnyElement {
        let field = &editor.fields[index];
        let editor_id = editor.editor_id;
        let input = self.settings_text_input_control(
            SettingsInput::DiscoveryField(index),
            &field.value,
            if field.slot == Slot::CertPath {
                "~/.ssh/id_ed25519-cert.pub"
            } else {
                "~/.ssh/id_ed25519"
            }
            .into(),
            self.tokens.metrics.settings_select_width,
            cx,
        );
        let label = self.i18n.t("ssh.drill_down.browse");
        let browse = self.workspace_toolbar_action_button(
            label.clone(),
            None,
            self.connection_import_secondary_button_options(false),
            self.select(move |_, cx| {
                let selection = crate::entity::picker::pick_path(cx, label.clone(), false);
                cx.spawn(async move |state, cx| {
                    if let Some(path) = selection.await {
                        let _ = state.update(cx, |state, cx| {
                            let Some(editor) = state
                                .discovery
                                .editor
                                .as_mut()
                                .filter(|e| e.editor_id == editor_id)
                            else {
                                return;
                            };
                            if editor.field_visible(&editor.fields[index]) {
                                editor.fields[index].value = path.display().to_string();
                                editor.clear_dependent_secrets(index);
                                cx.notify();
                            }
                        });
                    }
                })
                .detach();
            }),
        );
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(input)
            .child(browse)
            .into_any_element()
    }
}
