mod editor_actions;
mod ssh;

use super::*;
use oxideterm_plugin_manifest::discovery::{secret_key, secret_scope};
use oxideterm_plugin_manifest::{
    NativePluginDeclarativeUiControl,
    discovery::{
        DiscoveryConfigUse, DiscoveryLaunchKind, DiscoverySourceAction,
        NativePluginDiscoveryProviderDef,
    },
};
use oxideterm_settings::{DiscoverySource, DiscoverySshAuth};
use serde_json::{Value, json};
use std::hash::{Hash, Hasher};
use zeroize::Zeroizing;

const DEFAULT_REFRESH_INTERVAL_MINUTES: u32 = 5;

#[derive(Clone, PartialEq)]
pub struct DiscoveryProviderChoice {
    plugin_id: String,
    title: String,
    definition: NativePluginDiscoveryProviderDef,
}

impl DiscoveryProviderChoice {
    pub fn new(
        plugin_id: String,
        title: String,
        definition: NativePluginDiscoveryProviderDef,
    ) -> Self {
        Self {
            plugin_id,
            title,
            definition,
        }
    }

    pub(crate) fn new_source(&self) -> DiscoverySource {
        DiscoverySource {
            name: self.title.clone(),
            plugin_id: Some(self.plugin_id.clone()),
            provider_id: self.definition.id.clone(),
            ..Default::default()
        }
    }

    fn actions_at<'a>(
        &'a self,
        field: Option<&'a str>,
    ) -> impl Iterator<Item = &'a DiscoverySourceAction> {
        self.definition
            .source_actions
            .iter()
            .filter(move |action| action.field.as_deref() == field)
    }

    fn editor_choices(&self, store: &ConnectionStore) -> EditorChoices {
        if !self
            .definition
            .launch_kinds
            .contains(&DiscoveryLaunchKind::Ssh)
        {
            return EditorChoices::default();
        }
        EditorChoices {
            keys: store
                .managed_ssh_keys()
                .into_iter()
                .map(|key| (key.name, key.id))
                .collect(),
            totp: store
                .totp_credentials()
                .iter()
                .filter(|item| item.enabled)
                .map(|item| (item.name.clone(), json!(item.id)))
                .collect(),
        }
    }

    pub fn matches(&self, source: &DiscoverySource) -> bool {
        source.plugin_id.as_ref() == Some(&self.plugin_id)
            && self.definition.id == source.provider_id
    }
}

#[derive(Default)]
pub(crate) struct DiscoverySettingsState {
    pub providers: Vec<DiscoveryProviderChoice>,
    pub editor: Option<SourceEditor>,
    pub feedback: std::collections::HashMap<String, DiscoverySourceFeedback>,
}

#[derive(Default, Hash)]
pub(crate) struct DiscoverySourceFeedback {
    pub(super) catalog: Option<String>,
    refreshing: bool,
    action: Option<SourceRequestFeedback>,
}

#[derive(Hash, PartialEq)]
enum SourceRequestFeedback {
    Pending(uuid::Uuid),
    Finished(String),
}

impl SourceRequestFeedback {
    fn message(&self) -> Option<&str> {
        match self {
            Self::Pending(_) => None,
            Self::Finished(message) => Some(message),
        }
    }
}

impl DiscoverySettingsState {
    pub(crate) fn hash_controls(&self, hasher: &mut impl Hasher) {
        for provider in &self.providers {
            provider.title.hash(hasher);
            for action in &provider.definition.source_actions {
                action.title.hash(hasher);
                action.field.hash(hasher);
            }
        }
        if let Some(editor) = &self.editor {
            editor.source.id.hash(hasher);
            editor.fields.len().hash(hasher);
            for field in &editor.fields {
                if matches!(
                    field.slot,
                    Slot::ScheduledRefresh | Slot::SshAuth | Slot::KeySource
                ) {
                    field.value.hash(hasher);
                }
            }
        }
    }

    fn source_detail(
        &self,
        source: &DiscoverySource,
        provider: Option<&DiscoveryProviderChoice>,
        i18n: &I18n,
    ) -> String {
        if source.removal_pending {
            return i18n.t("settings_view.discovery.removal_pending");
        }
        let Some(provider) = provider else {
            return i18n.t("plugin.connections.unavailable");
        };
        if !source.enabled {
            return i18n.t("common.disabled");
        }
        let Some(feedback) = self.feedback.get(&source.id) else {
            return provider.title.clone();
        };
        let messages: Vec<_> = [
            feedback.catalog.as_deref(),
            feedback
                .action
                .as_ref()
                .and_then(SourceRequestFeedback::message),
        ]
        .into_iter()
        .flatten()
        .collect();
        if messages.is_empty() {
            provider.title.clone()
        } else {
            messages.join("\n")
        }
    }
}

impl SettingsWorkspaceEntity {
    pub fn set_discovery_providers(
        &mut self,
        providers: Vec<DiscoveryProviderChoice>,
        cx: &mut Context<Self>,
    ) {
        if self.discovery.providers != providers {
            if let Some(editor) = &self.discovery.editor
                && self
                    .discovery
                    .providers
                    .iter()
                    .find(|provider| provider.matches(&editor.source))
                    != providers
                        .iter()
                        .find(|provider| provider.matches(&editor.source))
            {
                self.blur_settings_entity_input(cx);
                self.discovery.editor = None;
            }
            self.discovery.providers = providers;
            cx.notify();
        }
    }

    pub fn edit_discovery_source(
        &mut self,
        source: DiscoverySource,
        store: &ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self
            .discovery
            .providers
            .iter()
            .find(|provider| provider.matches(&source))
        else {
            return;
        };
        let editor = SourceEditor::new(source, provider, &provider.editor_choices(store), i18n);
        self.blur_settings_entity_input(cx);
        self.discovery.editor = Some(editor);
        cx.notify();
    }

    pub fn set_discovery_catalog(
        &mut self,
        source_id: String,
        refreshing: bool,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let feedback = self.discovery.feedback.entry(source_id).or_default();
        let changed = feedback.catalog != status || feedback.refreshing != refreshing;

        feedback.catalog = status;
        feedback.refreshing = refreshing;
        if changed {
            cx.notify();
        }
    }

    pub fn begin_discovery_action(&mut self, id: &str, cx: &mut Context<Self>) -> uuid::Uuid {
        let request = uuid::Uuid::new_v4();
        let feedback = self.discovery.feedback.entry(id.to_owned()).or_default();
        feedback.action = Some(SourceRequestFeedback::Pending(request));
        cx.notify();
        request
    }

    pub fn finish_discovery_action(
        &mut self,
        id: &str,
        request: uuid::Uuid,
        message: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(feedback) = self.discovery.feedback.get_mut(id)
            && feedback.action == Some(SourceRequestFeedback::Pending(request))
        {
            feedback.action = message.map(SourceRequestFeedback::Finished);
            cx.notify();
        }
    }

    pub fn clear_discovery_feedback(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.discovery.feedback.remove(id).is_some() {
            cx.notify();
        }
    }
}

pub(crate) struct SourceEditor {
    editor_id: uuid::Uuid,
    pub source: DiscoverySource,
    pub fields: Vec<EditorField>,
}

#[derive(Default)]
struct EditorChoices {
    keys: Vec<(String, String)>,
    totp: Vec<(String, Value)>,
}

pub(crate) struct EditorField {
    slot: Slot,
    label: String,
    placeholder: String,
    kind: String,
    pub value: String,
    secret: Option<SecretDraft>,
    pub options: Vec<(String, Value)>,
    min: Option<f64>,
    max: Option<f64>,
    disabled: bool,
}

struct SecretDraft {
    scope: Vec<String>,
    revision: uuid::Uuid,
    value: Zeroizing<String>,
}

#[derive(PartialEq, Eq)]
enum Slot {
    Name,
    ScheduledRefresh,
    Minutes,
    SshAuth,
    IdentityAgent,
    KeySource,
    KeyPath,
    CertPath,
    ManagedKey,
    Passphrase,
    Totp,
    ConfirmForm,
    Config(String),
    Note,
}

impl EditorField {
    fn new(slot: Slot, label: impl Into<String>, kind: &str, value: impl ToString) -> Self {
        Self {
            slot,
            label: label.into(),
            placeholder: String::new(),
            kind: kind.into(),
            value: value.to_string(),
            secret: None,
            options: Vec::new(),
            min: None,
            max: None,
            disabled: false,
        }
    }

    pub(crate) fn secret_value(&self) -> Option<&str> {
        Some(self.secret.as_ref()?.value.as_str())
    }

    pub(crate) fn secret_value_mut(&mut self) -> Option<&mut String> {
        Some(&mut self.secret.as_mut()?.value)
    }

    pub(crate) fn select(&mut self, index: usize) {
        if let Some((_, value)) = self.options.get(index) {
            self.value = value.to_string();
        }
    }
}

impl SourceEditor {
    fn new(
        mut source: DiscoverySource,
        provider: &DiscoveryProviderChoice,
        choices: &EditorChoices,
        i18n: &I18n,
    ) -> Self {
        source.apply_ssh_policy(&provider.definition);
        let mut fields = vec![
            EditorField::new(Slot::Name, "plugin.connections.name", "text", &source.name),
            EditorField::new(
                Slot::ScheduledRefresh,
                "settings_view.discovery.scheduled_refresh",
                "checkbox",
                source.refresh_minutes.is_some(),
            ),
            EditorField::new(
                Slot::Minutes,
                "settings_view.discovery.interval",
                "number",
                source
                    .refresh_minutes
                    .or(source.last_refresh_minutes)
                    .or(provider.definition.recommended_refresh_interval_minutes)
                    .map_or(DEFAULT_REFRESH_INTERVAL_MINUTES, |minutes| minutes.get()),
            ),
        ];
        let def = &provider.definition;
        collect_controls(&mut fields, &def.form.controls, "root", &source.config);
        for section in &def.form.sections {
            collect_controls(&mut fields, &section.controls, &section.id, &source.config);
        }
        if def.launch_kinds.contains(&DiscoveryLaunchKind::Ssh) {
            let prefs = &source.ssh_preferences;
            fields.extend(ssh::auth_fields(&source, choices, i18n));
            let mut totp = EditorField::new(
                Slot::Totp,
                "settings_view.totp.connection_label",
                "select",
                json!(prefs.totp_credential_id),
            );
            totp.options
                .push((i18n.t("settings_view.totp.none"), Value::Null));
            totp.options.extend_from_slice(&choices.totp);
            fields.push(totp);
            fields.push(EditorField::new(
                Slot::ConfirmForm,
                "settings_view.discovery.confirm_ssh_form",
                "checkbox",
                !source.ssh_preferences.skip_connection_form,
            ));
        }
        Self {
            editor_id: uuid::Uuid::new_v4(),
            source,
            fields,
        }
    }

    fn scheduled_refresh(&self) -> bool {
        self.fields
            .iter()
            .any(|field| matches!(field.slot, Slot::ScheduledRefresh) && field.value == "true")
    }

    fn field_visible(&self, field: &EditorField) -> bool {
        match field.slot {
            Slot::Minutes => self.scheduled_refresh(),
            Slot::IdentityAgent => self.ssh_choice(Slot::SshAuth) == Some("agent"),
            Slot::KeySource
            | Slot::Passphrase
            | Slot::KeyPath
            | Slot::CertPath
            | Slot::ManagedKey => {
                self.ssh_choice(Slot::SshAuth) == Some("key")
                    && match field.slot {
                        Slot::KeyPath => matches!(
                            self.ssh_choice(Slot::KeySource),
                            Some("file" | "certificate")
                        ),
                        Slot::CertPath => self.ssh_choice(Slot::KeySource) == Some("certificate"),
                        Slot::ManagedKey => self.ssh_choice(Slot::KeySource) == Some("managed"),
                        _ => true,
                    }
            }
            _ => true,
        }
    }

    pub(crate) fn clear_dependent_secrets(&mut self, index: usize) {
        if self.fields.get(index).is_some_and(|field| {
            matches!(
                field.slot,
                Slot::SshAuth | Slot::KeySource | Slot::KeyPath | Slot::CertPath | Slot::ManagedKey
            )
        }) && let Some(draft) = self
            .fields
            .iter_mut()
            .find(|f| f.slot == Slot::Passphrase)
            .and_then(|f| f.secret.as_mut())
        {
            draft.value = Zeroizing::default();
        }
        let Some(EditorField {
            slot: Slot::Config(key),
            ..
        }) = self.fields.get(index)
        else {
            return;
        };
        let key = key.clone();
        for field in &mut self.fields {
            if let Some(secret) = &mut field.secret
                && !secret.value.is_empty()
                && secret.scope.contains(&key)
            {
                secret.value = Zeroizing::default();
            }
        }
    }

    pub(crate) fn select_choice(&mut self, field_index: usize, option_index: usize) {
        let Some(field) = self.fields.get_mut(field_index) else {
            return;
        };
        let changed = field
            .options
            .get(option_index)
            .is_some_and(|(_, v)| v.to_string() != field.value);
        field.select(option_index);
        if changed {
            self.clear_dependent_secrets(field_index);
        }
    }

    fn validated(&self) -> Option<DiscoverySource> {
        let mut source = self.source.clone();
        for field in &self.fields {
            let text = field.value.trim();
            match &field.slot {
                Slot::Name if !text.is_empty() => source.name = text.into(),
                Slot::Name => return None,
                Slot::ScheduledRefresh => {}
                Slot::Minutes if self.scheduled_refresh() => {
                    source.refresh_minutes = Some(text.parse().ok()?);
                    source.last_refresh_minutes = None;
                }
                Slot::Minutes => {
                    source.last_refresh_minutes = source
                        .refresh_minutes
                        .take()
                        .or(source.last_refresh_minutes);
                }
                Slot::Config(_) if field.secret.is_some() => continue,
                Slot::Config(key) => {
                    let value = match field.kind.as_str() {
                        "checkbox" => Value::Bool(text == "true"),
                        "select" if text.is_empty() => continue,
                        "select" => field
                            .options
                            .iter()
                            .find(|(_, v)| v.to_string() == text)?
                            .1
                            .clone(),
                        "number" if text.is_empty() => {
                            source.config.as_object_mut()?.remove(key);
                            continue;
                        }
                        "number" => {
                            let n: serde_json::Number = text.parse().ok()?;
                            let v = n.as_f64()?;
                            if field.min.is_some_and(|min| v < min)
                                || field.max.is_some_and(|max| v > max)
                            {
                                return None;
                            }
                            Value::Number(n)
                        }
                        _ => Value::String(field.value.clone()),
                    };
                    source.config[key] = value;
                }
                Slot::ConfirmForm => source.ssh_preferences.skip_connection_form = text != "true",
                Slot::IdentityAgent => {
                    source.ssh_preferences.identity_agent =
                        (self.field_visible(field) && !text.is_empty()).then(|| text.to_owned());
                }
                Slot::SshAuth => source.ssh_preferences.auth = self.ssh_auth()?,
                Slot::Totp => {
                    let value = &field.options.iter().find(|(_, v)| v.to_string() == text)?.1;
                    source.ssh_preferences.totp_credential_id = value.as_str().map(str::to_owned);
                }
                Slot::Passphrase => {
                    source.ssh_preferences.passphrase_revision = if !self.field_visible(field) {
                        None
                    } else if let Some(secret) =
                        field.secret.as_ref().filter(|s| !s.value.is_empty())
                    {
                        Some(secret.revision)
                    } else if source.ssh_preferences.auth == self.source.ssh_preferences.auth {
                        self.source.ssh_preferences.passphrase_revision
                    } else {
                        None
                    };
                }
                Slot::KeyPath | Slot::CertPath | Slot::KeySource | Slot::ManagedKey => {}
                Slot::Note => {}
            }
        }
        // Resolve bindings after all public fields, including fields declared later in the form.
        for field in &self.fields {
            if let (Slot::Config(key), Some(secret)) = (&field.slot, &field.secret) {
                let scope = secret_scope(&source.config, &secret.scope);
                if !secret.value.is_empty() {
                    source.config[key] =
                        json!({"discoverySecret": secret.revision, "scope": scope});
                } else if source.config[key].get("scope") != Some(&scope)
                    || secret_key(&source.id, key, &source.config[key]).is_none()
                {
                    source.config.as_object_mut()?.remove(key);
                }
            }
        }
        Some(source)
    }

    fn matches_action_config(
        &self,
        saved: &DiscoverySource,
        provider: &DiscoveryProviderChoice,
    ) -> bool {
        let config =
            provider
                .definition
                .config_for(&saved.id, &saved.config, DiscoveryConfigUse::Both);
        self.fields.iter().all(|field| match &field.slot {
            Slot::Config(_) if field.secret.is_some() => field.secret_value() == Some(""),
            Slot::Config(key) => field.value == config_field_text(&field.kind, config.get(key)),
            _ => true,
        })
    }

    fn save_request(self) -> Option<ConnectionRequest> {
        let source = self.validated()?;
        let secrets = self
            .fields
            .into_iter()
            .filter_map(|field| {
                let key = match field.slot {
                    Slot::Config(key) => key,
                    Slot::Passphrase if source.ssh_preferences.passphrase_revision.is_some() => {
                        oxideterm_settings::DISCOVERY_SSH_PASSPHRASE_FIELD.into()
                    }
                    _ => return None,
                };
                let secret = field.secret?.value;
                (!secret.is_empty()).then_some((key, secret))
            })
            .collect();
        Some(ConnectionRequest::SaveDiscovery { secrets, source })
    }
}

fn config_field_text(kind: &str, value: Option<&Value>) -> String {
    match (kind, value) {
        ("checkbox", value) => value.and_then(Value::as_bool).unwrap_or(false).to_string(),
        ("select", Some(v)) => v.to_string(),
        (_, Some(Value::String(s))) => s.clone(),
        (_, Some(v)) if !v.is_null() => v.to_string(),
        _ => String::new(),
    }
}

fn collect_controls(
    fields: &mut Vec<EditorField>,
    controls: &[NativePluginDeclarativeUiControl],
    section: &str,
    config: &Value,
) {
    for control in controls {
        if control.kind == "markdown" {
            fields.push(EditorField::new(
                Slot::Note,
                "",
                "markdown",
                control.text.as_deref().unwrap_or_default(),
            ));
        } else if matches!(
            control.kind.as_str(),
            "text" | "textarea" | "number" | "checkbox" | "select" | "password"
        ) && let Some(id) = &control.id
        {
            let key = format!("{section}/{id}");
            let value = config.get(&key).or(control.value.as_ref());
            let text = if control.kind == "password" {
                String::new()
            } else {
                config_field_text(&control.kind, value)
            };
            let mut field = EditorField::new(
                Slot::Config(key),
                control.label.as_deref().unwrap_or(id),
                &control.kind,
                text,
            );
            if control.kind == "password" {
                field.secret = Some(SecretDraft {
                    scope: control.secret_scope.clone().unwrap_or_default(),
                    revision: uuid::Uuid::new_v4(),
                    value: Zeroizing::default(),
                });
            }
            field.placeholder = control.placeholder.clone().unwrap_or_default();
            field.min = control.min;
            field.max = control.max;
            field.disabled = control.disabled;
            if let Some(options) = &control.options {
                field.options = options
                    .iter()
                    .map(|o| (o.label.clone(), o.value.clone()))
                    .collect();
            }
            fields.push(field);
        }
        collect_controls(fields, &control.children, section, config);
    }
}

impl ConnectionPage<'_> {
    fn source_action_button(
        &self,
        source: &DiscoverySource,
        action: &DiscoverySourceAction,
        disabled: bool,
    ) -> Div {
        self.workspace_toolbar_action_button(
            action.title.clone(),
            None,
            self.connection_import_secondary_button_options(disabled),
            self.press(ConnectionRequest::RunDiscoverySourceAction {
                source_id: source.id.clone(),
                action_id: action.id.clone(),
            }),
        )
    }

    fn source_editor_button(
        &self,
        label: String,
        source: DiscoverySource,
        provider: DiscoveryProviderChoice,
    ) -> Div {
        let state = self.state.downgrade();
        let choices = provider.editor_choices(self.store);
        let i18n = self.i18n.clone();
        self.workspace_toolbar_action_button(
            label,
            None,
            self.connection_import_secondary_button_options(false),
            move |_, _, cx| {
                let draft = source.clone();
                let _ = state.update(cx, |state, cx| {
                    state.blur_settings_entity_input(cx);
                    state.discovery.editor =
                        Some(SourceEditor::new(draft, &provider, &choices, &i18n));
                    cx.notify();
                });
                cx.stop_propagation();
            },
        )
    }

    pub(super) fn discovery_section(&self, cx: &App) -> AnyElement {
        let ui = &self.state.read(cx).discovery;
        if let Some(editor) = &ui.editor {
            let mut rows = Vec::new();
            let field_provider = ui.providers.iter().find(|p| {
                p.matches(&editor.source)
                    && p.definition
                        .source_actions
                        .iter()
                        .any(|a| a.field.is_some())
            });
            let saved = self
                .settings
                .discovery_sources
                .iter()
                .find(|s| s.id == editor.source.id);
            // Source actions execute against persisted settings. Never open a credential
            // editor for the previous site or account while its replacement is on screen.
            let actions_ready = field_provider.zip(saved).is_some_and(|(provider, saved)| {
                saved.enabled
                    && !saved.removal_pending
                    && provider.matches(saved)
                    && editor.matches_action_config(saved, provider)
            });
            let draft = editor.validated();
            for (index, field) in editor.fields.iter().enumerate() {
                if !editor.field_visible(field) {
                    continue;
                }
                let input = SettingsInput::DiscoveryField(index);
                let element = match field.kind.as_str() {
                    "markdown" => {
                        rows.push(div().child(field.value.clone()).into_any_element());
                        continue;
                    }
                    "checkbox" => switch(
                        self.tokens,
                        ("discovery-boolean", index),
                        String::new(),
                        field.value == "true",
                    )
                    .when(!field.disabled, |checkbox| {
                        checkbox.on_mouse_down(
                            MouseButton::Left,
                            self.select(move |state, cx| {
                                if let Some(field) = state
                                    .discovery
                                    .editor
                                    .as_mut()
                                    .and_then(|editor| editor.fields.get_mut(index))
                                {
                                    field.value = (field.value != "true").to_string();
                                    if matches!(field.slot, Slot::ScheduledRefresh) {
                                        state.blur_settings_entity_input(cx);
                                    }
                                    if let Some(editor) = state.discovery.editor.as_mut() {
                                        editor.clear_dependent_secrets(index);
                                    }
                                    cx.notify();
                                }
                            }),
                        )
                    })
                    .into_any_element(),
                    "select" => self.settings_select_control(
                        SettingsSelect::DiscoveryChoice(index),
                        field
                            .options
                            .iter()
                            .find(|(_, v)| v.to_string() == field.value)
                            .map(|o| o.0.clone())
                            .unwrap_or_else(|| match field.slot {
                                Slot::Totp => self.i18n.t("settings_view.totp.unavailable"),
                                Slot::ManagedKey => self.i18n.t("ssh.form.managed_key_required"),
                                _ => String::new(),
                            }),
                        field.disabled,
                        None,
                        cx,
                    ),
                    _ if field.disabled => div().child(field.value.clone()).into_any_element(),
                    "path" => self.discovery_key_path(editor, index, cx),
                    "password" => {
                        let configured = draft.as_ref().is_some_and(|source| match &field.slot {
                            Slot::Config(key) => source.config.get(key).is_some(),
                            Slot::Passphrase => {
                                source.ssh_preferences.passphrase_revision.is_some()
                            }
                            _ => false,
                        });
                        let hint = if configured {
                            self.i18n.t("settings_view.totp.keep_secret")
                        } else {
                            field.placeholder.clone()
                        };
                        self.field(
                            SettingsInput::DiscoverySecret(index),
                            field.secret_value().unwrap_or_default(),
                            hint,
                            Some(self.tokens.metrics.settings_select_width),
                            TextInputContentAlign::Start,
                            true,
                            cx,
                        )
                    }
                    "textarea" => {
                        let input = SettingsInput::DiscoveryMultiline(index);
                        crate::components::settings_multiline_patterns_field(
                            self.tokens,
                            self.ime.frame(input, &field.value, cx),
                            &field.placeholder,
                            &(self.scroll)(&format!("discovery-field-{index}")),
                            self.controls.dispatch.clone(),
                        )
                    }
                    _ => self.settings_text_input_control(
                        input,
                        &field.value,
                        field.placeholder.clone(),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                };
                if let (Some(provider), Slot::Config(key)) = (field_provider, &field.slot) {
                    let mut actions = provider.actions_at(Some(key)).peekable();
                    if actions.peek().is_some() {
                        let controls = div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.))
                            .child(element)
                            .children(actions.map(|action| {
                                self.source_action_button(&editor.source, action, !actions_ready)
                            }));
                        let hint = if actions_ready {
                            ""
                        } else {
                            "settings_view.discovery.save_before_action"
                        };
                        rows.push(self.setting_row(
                            &field.label,
                            hint,
                            controls.into_any_element(),
                            cx,
                        ));
                        continue;
                    }
                }
                rows.push(self.setting_row(&field.label, "", element, cx));
            }
            if field_provider.is_some()
                && let Some(message) = ui
                    .feedback
                    .get(&editor.source.id)
                    .and_then(|feedback| feedback.action.as_ref())
                    .and_then(SourceRequestFeedback::message)
            {
                rows.push(div().child(message.to_owned()).into_any_element());
            }
            let controls = self.discovery_editor_actions(draft);
            let cancel = self.workspace_toolbar_action_button(
                self.i18n.t("common.cancel"),
                None,
                self.connection_import_secondary_button_options(false),
                self.select(|state, cx| {
                    state.blur_settings_entity_input(cx);
                    state.discovery.editor = None;
                    cx.notify();
                }),
            );
            rows.push(controls.child(cancel).into_any_element());
            return self.connection_section(
                "settings_view.discovery.title",
                "",
                self.separated_settings_rows(rows),
            );
        }
        let providers = &ui.providers;
        let mut rows = Vec::new();
        for source in &self.settings.discovery_sources {
            let provider = providers.iter().find(|p| p.matches(source));
            let feedback = ui.feedback.get(&source.id);
            let mut controls = div().flex().flex_wrap().items_center().gap(px(6.));
            if let Some(provider) = provider.filter(|_| !source.removal_pending) {
                controls = controls.child(self.source_editor_button(
                    self.i18n.t("settings_view.custom_theme.edit"),
                    source.clone(),
                    provider.clone(),
                ));
                if source.enabled {
                    let refreshing = feedback.is_some_and(|f| f.refreshing);
                    controls = controls.child(self.workspace_toolbar_action_button(
                        self.i18n.t(if refreshing {
                            "settings_view.discovery.refreshing"
                        } else {
                            "settings_view.discovery.refresh"
                        }),
                        None,
                        ToolbarButtonOptions {
                            loading: refreshing,
                            ..self.connection_import_secondary_button_options(false)
                        },
                        self.press(ConnectionRequest::RefreshDiscovery(source.id.clone())),
                    ));
                    for action in provider.actions_at(None) {
                        controls = controls.child(self.source_action_button(source, action, false));
                    }
                }
            }
            controls = controls.child(self.workspace_toolbar_action_button(
                self.i18n.t("common.delete"),
                None,
                self.connection_import_secondary_button_options(false),
                self.press(ConnectionRequest::DeleteDiscovery(source.id.clone())),
            ));
            let detail = ui.source_detail(source, provider, self.i18n);
            let content = self.setting_row(&source.name, &detail, controls.into_any_element(), cx);
            let mut row = div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(self.tokens.metrics.settings_row_gap))
                .child(div().flex_1().min_w(px(0.)).child(content));
            if !source.removal_pending {
                row = row.child(
                    switch(
                        self.tokens,
                        format!("discovery-enabled-{}", source.id),
                        String::new(),
                        source.enabled,
                    )
                    .flex_none()
                    .debug_selector(|| format!("discovery-enabled-{}", source.id))
                    .on_mouse_down(
                        MouseButton::Left,
                        self.press(ConnectionRequest::SetDiscoveryEnabled(
                            source.id.clone(),
                            !source.enabled,
                        )),
                    ),
                );
            }
            rows.push(row.into_any_element());
        }
        let (hint, action) = if providers.is_empty() {
            let dispatch = self.controls.dispatch.clone();
            let manage = self.workspace_toolbar_action_button(
                self.i18n.t("command_palette.cmd_open_plugin_manager"),
                None,
                self.connection_import_secondary_button_options(false),
                move |_, window, cx| {
                    dispatch(
                        crate::components::SettingsWindowRequest::OpenPluginManager,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                },
            );
            (
                "settings_view.discovery.install_provider_hint",
                manage
                    .debug_selector(|| "discovery-open-plugins".into())
                    .into_any_element(),
            )
        } else {
            (
                "",
                self.settings_select_control(
                    SettingsSelect::DiscoveryProvider,
                    self.i18n.t("settings_view.discovery.add"),
                    false,
                    Some(self.tokens.metrics.ui_select_min_width),
                    cx,
                ),
            )
        };
        self.settings_card_with_header(
            "settings_view.discovery.title",
            hint,
            None,
            Some(action),
            self.separated_settings_rows(rows),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory_fixture() -> (DiscoverySource, DiscoveryProviderChoice) {
        let definition = serde_json::from_value(json!({
            "id": "assets", "title": "Inventory", "icon": "inventory.svg", "protocolVersion": 1,
            "form": {}, "launchKinds": ["ssh"],
        }))
        .unwrap();
        let source = DiscoverySource {
            id: "lab".into(),
            provider_id: "assets".into(),
            plugin_id: Some("com.example.inventory".into()),
            name: "Lab".into(),
            ..Default::default()
        };
        (
            source,
            DiscoveryProviderChoice::new(
                "com.example.inventory".into(),
                "Inventory".into(),
                definition,
            ),
        )
    }

    #[test]
    fn discovery_password_is_write_only_and_bound_to_the_selected_identity() {
        let (mut source, mut provider) = inventory_fixture();
        provider.definition.form = serde_json::from_value(json!({"controls":[
            {"kind":"text","id":"site"}, {"kind":"text","id":"user"},
            {"kind":"password","id":"secret","secretScope":["root/site","root/user"]}
        ]}))
        .unwrap();
        source.config = json!({"root/site":"one", "root/user":"alice"});
        let edit = |source| {
            SourceEditor::new(
                source,
                &provider,
                &EditorChoices::default(),
                &I18n::new(oxideterm_i18n::Locale::En),
            )
        };
        let mut first = edit(source);
        let index = first
            .fields
            .iter()
            .position(|f| f.secret.is_some())
            .unwrap();
        first.fields[index]
            .secret_value_mut()
            .unwrap()
            .push_str("fixture-password");
        let Some(ConnectionRequest::SaveDiscovery {
            source: saved,
            secrets,
            ..
        }) = first.save_request()
        else {
            panic!("expected save")
        };
        assert_eq!(secrets[0].1.as_str(), "fixture-password");
        assert!(
            !serde_json::to_string(&saved)
                .unwrap()
                .contains("fixture-password")
        );
        let wire = |source: &DiscoverySource| {
            provider
                .definition
                .config_for(&source.id, &source.config, DiscoveryConfigUse::Both)
        };
        let reference = wire(&saved)["root/secret"].as_str().unwrap().to_owned();
        assert!(reference.starts_with("discovery:3:lab:root/secret:"));
        let mut other = saved.clone();
        other.id = "other".into();
        assert_ne!(wire(&other)["root/secret"], reference);
        let mut reopened = edit(saved.clone());
        assert_eq!(reopened.fields[index].secret_value(), Some(""));
        assert_eq!(
            wire(&reopened.validated().unwrap())["root/secret"],
            reference
        );
        reopened.fields[index]
            .secret_value_mut()
            .unwrap()
            .push_str("replacement");
        let replacement = reopened.validated().unwrap();
        assert_ne!(wire(&replacement)["root/secret"], reference);
        drop(reopened);
        assert_eq!(wire(&saved)["root/secret"], reference);
        for key in ["root/site", "root/user"] {
            let mut editor = edit(saved.clone());
            let position = editor
                .fields
                .iter()
                .position(|f| matches!(&f.slot, Slot::Config(k) if k == key))
                .unwrap();
            editor.fields[index]
                .secret_value_mut()
                .unwrap()
                .push_str("old-identity-draft");
            editor.fields[position].value = "changed".into();
            editor.clear_dependent_secrets(position);
            let Some(ConnectionRequest::SaveDiscovery {
                source, secrets, ..
            }) = editor.save_request()
            else {
                panic!("expected save")
            };
            assert!(secrets.is_empty());
            assert!(wire(&source).get("root/secret").is_none());
        }
    }

    #[test]
    fn discovery_field_action_uses_the_saved_source_shown_in_the_editor() {
        let (mut source, mut provider) = inventory_fixture();
        provider.definition.form = serde_json::from_value(json!({"controls":[
            {"kind":"text","id":"account"}, {"kind":"text","id":"site"}
        ]}))
        .unwrap();
        provider.definition.source_actions = serde_json::from_value(json!([
            {"id":"configure","title":"Configure","field":"root/account"},
            {"id":"inspect","title":"Inspect"}
        ]))
        .unwrap();
        assert_eq!(
            provider
                .actions_at(None)
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["inspect"]
        );
        assert_eq!(
            provider
                .actions_at(Some("root/account"))
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["configure"]
        );
        assert_eq!(provider.actions_at(Some("root/site")).count(), 0);
        source.config = json!({"root/account":"alpha","root/site":"one.example"});
        let mut editor = SourceEditor::new(
            source.clone(),
            &provider,
            &EditorChoices::default(),
            &I18n::new(oxideterm_i18n::Locale::En),
        );
        assert!(editor.matches_action_config(&source, &provider));
        let site = editor
            .fields
            .iter_mut()
            .find(|f| matches!(&f.slot, Slot::Config(key) if key == "root/site"))
            .unwrap();
        site.value = "two.example".into();
        assert!(!editor.matches_action_config(&source, &provider));
        source.config["root/site"] = json!("two.example");
        assert!(editor.matches_action_config(&source, &provider));
    }

    #[gpui::test]
    fn discovery_action_feedback_survives_refreshes_and_ignores_replaced_requests(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext;
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |s, cx| {
            let i18n = I18n::new(oxideterm_i18n::Locale::En);
            let (_, provider) = inventory_fixture();
            let source = |id: &str| DiscoverySource {
                id: id.into(),
                enabled: true,
                ..Default::default()
            };
            let old = s.begin_discovery_action("west", cx);
            s.clear_discovery_feedback("west", cx);
            let east = s.begin_discovery_action("east", cx);
            s.finish_discovery_action("east", east, Some("Access denied".into()), cx);
            s.finish_discovery_action("west", old, Some("Obsolete failure".into()), cx);

            let first = s.begin_discovery_action("west", cx);
            let second = s.begin_discovery_action("west", cx);
            s.finish_discovery_action("west", second, None, cx);
            s.finish_discovery_action("west", first, Some("Late action failure".into()), cx);
            assert_eq!(
                s.discovery
                    .source_detail(&source("west"), Some(&provider), &i18n),
                "Inventory"
            );
            let current = s.begin_discovery_action("west", cx);
            s.finish_discovery_action("west", current, Some("Could not open settings".into()), cx);
            s.set_discovery_catalog("west".into(), true, None, cx);
            assert!(s.discovery.feedback["west"].refreshing);
            s.set_discovery_catalog("west".into(), false, Some("Found 0 targets".into()), cx);
            assert_eq!(
                s.discovery
                    .source_detail(&source("west"), Some(&provider), &i18n),
                "Found 0 targets\nCould not open settings"
            );
            assert!(!s.discovery.feedback["west"].refreshing);
            assert_eq!(
                s.discovery
                    .source_detail(&source("east"), Some(&provider), &i18n),
                "Access denied"
            );
        });
    }

    #[test]
    fn discovery_key_source_requires_paths_and_keeps_the_passphrase_out_of_plugin_config() {
        let (source, provider) = inventory_fixture();
        let choices = EditorChoices::default();
        let i18n = I18n::new(oxideterm_i18n::Locale::En);
        let edit = |source| SourceEditor::new(source, &provider, &choices, &i18n);
        let mut editor = edit(source);
        let index = |slot| editor.fields.iter().position(|f| f.slot == slot).unwrap();
        let (family, key_source, key, cert, passphrase) = (
            index(Slot::SshAuth),
            index(Slot::KeySource),
            index(Slot::KeyPath),
            index(Slot::CertPath),
            index(Slot::Passphrase),
        );
        editor.select_choice(family, 1);
        assert!(editor.field_visible(&editor.fields[key]));
        assert!(!editor.field_visible(&editor.fields[cert]));
        assert!(
            editor.validated().is_none(),
            "an explicit key file must have a path"
        );
        editor.fields[key].value = "fixture/private-key".into();
        editor.select_choice(key_source, 3);
        assert!(editor.field_visible(&editor.fields[cert]));
        assert!(
            editor.validated().is_none(),
            "a certificate requires both paths"
        );
        editor.fields[cert].value = "fixture/cert.pub".into();
        editor.fields[passphrase]
            .secret_value_mut()
            .unwrap()
            .push_str("fixture-unlock");
        let Some(ConnectionRequest::SaveDiscovery {
            source, secrets, ..
        }) = editor.save_request()
        else {
            panic!("expected save")
        };
        assert_eq!(
            source.ssh_preferences.auth,
            DiscoverySshAuth::Certificate {
                key_path: "fixture/private-key".into(),
                cert_path: "fixture/cert.pub".into(),
            }
        );
        assert_eq!(source.config, json!({}));
        assert!(
            !serde_json::to_string(&source)
                .unwrap()
                .contains("fixture-unlock")
        );
        assert_eq!(
            secrets[0].0,
            oxideterm_settings::DISCOVERY_SSH_PASSPHRASE_FIELD
        );
        assert_eq!(secrets[0].1.as_str(), "fixture-unlock");
        let reference = source.ssh_preferences.passphrase_revision.unwrap();
        let mut editor = edit(source);
        assert_eq!(editor.fields[passphrase].secret_value(), Some(""));
        assert_eq!(
            editor
                .validated()
                .unwrap()
                .ssh_preferences
                .passphrase_revision,
            Some(reference)
        );
        editor.fields[key].value = "fixture/other".into();
        assert_eq!(
            editor
                .validated()
                .unwrap()
                .ssh_preferences
                .passphrase_revision,
            None
        );
    }

    #[test]
    fn discovery_editor_saves_ssh_auth_and_totp_for_the_source() {
        let (source, provider) = inventory_fixture();
        let i18n = I18n::new(oxideterm_i18n::Locale::En);
        let choices = EditorChoices {
            totp: vec![("Production MFA".into(), json!("prod-mfa"))],
            ..Default::default()
        };
        let mut implicit = SourceEditor::new(source.clone(), &provider, &choices, &i18n);
        for field in &mut implicit.fields {
            match field.slot {
                Slot::SshAuth => field.select(3),
                Slot::Totp => field.select(1),
                Slot::ConfirmForm => field.value = "false".into(),
                _ => {}
            }
        }
        let saved = implicit.validated().unwrap();
        assert_eq!(saved.ssh_preferences.auth, DiscoverySshAuth::TwoFactor);
        assert_eq!(
            saved.ssh_preferences.totp_credential_id.as_deref(),
            Some("prod-mfa")
        );
        assert!(saved.ssh_preferences.skip_connection_form);
        assert_eq!(saved.config, source.config);
    }

    #[test]
    fn discovery_refresh_is_opt_in_and_remembers_the_saved_interval_when_reenabled() {
        use std::num::NonZeroU32;

        let i18n = I18n::new(oxideterm_i18n::Locale::En);
        let (source, mut choice) = inventory_fixture();
        let set_scheduled = |editor: &mut SourceEditor, enabled: bool| {
            editor
                .fields
                .iter_mut()
                .find(|f| matches!(f.slot, Slot::ScheduledRefresh))
                .unwrap()
                .value = enabled.to_string();
        };
        for (recommendation, expected) in [(None, 5), (NonZeroU32::new(10), 10)] {
            choice.definition.recommended_refresh_interval_minutes = recommendation;
            let mut editor =
                SourceEditor::new(source.clone(), &choice, &EditorChoices::default(), &i18n);
            let manual = editor.validated().unwrap();
            assert_eq!(manual.refresh_minutes, None);
            set_scheduled(&mut editor, true);
            assert_eq!(
                editor.validated().unwrap().refresh_minutes,
                NonZeroU32::new(expected)
            );
        }

        let mut editor = SourceEditor::new(source, &choice, &EditorChoices::default(), &i18n);
        set_scheduled(&mut editor, true);
        editor
            .fields
            .iter_mut()
            .find(|f| matches!(f.slot, Slot::Minutes))
            .unwrap()
            .value = "2".into();
        let mut editor = SourceEditor::new(
            editor.validated().unwrap(),
            &choice,
            &EditorChoices::default(),
            &i18n,
        );
        set_scheduled(&mut editor, false);
        let paused = editor.validated().unwrap();
        assert_eq!(paused.refresh_minutes, None);
        choice.definition.recommended_refresh_interval_minutes = NonZeroU32::new(15);
        let mut editor = SourceEditor::new(paused, &choice, &EditorChoices::default(), &i18n);
        set_scheduled(&mut editor, true);
        assert_eq!(
            editor.validated().unwrap().refresh_minutes,
            NonZeroU32::new(2)
        );
    }
}
