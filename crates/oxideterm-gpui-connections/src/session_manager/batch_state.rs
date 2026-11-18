// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use super::SessionManagerState;
use gpui::{App, Context, Entity, Subscription, Task, Timer};
use oxideterm_connections::BatchIssueKind;
use oxideterm_connections::{
    BatchDuplicateNamePolicy, BatchIssue, BatchPreview, BatchTemplate, SavedAuth, SecretString,
};
use oxideterm_gpui_editor::TextEditorView;
use zeroize::Zeroizing;

pub fn batch_i18n(
    i18n: &oxideterm_i18n::I18n,
    key: &str,
    replacements: &[(&str, String)],
) -> String {
    let mut text = i18n.t(key);
    for (name, value) in replacements {
        text = text.replace(&format!("{{{{{name}}}}}"), value);
    }
    text
}

pub fn batch_issue_text(i18n: &oxideterm_i18n::I18n, issue: &BatchIssue) -> String {
    let kind = &issue.kind;
    let key = format!("sessionManager.batchAdd.errors.{}", kind.message_key());
    let line = issue.line.to_string();
    let text = match kind {
        BatchIssueKind::DirectiveUnsupported { keyword } => {
            batch_i18n(i18n, &key, &[("keyword", keyword.clone())])
        }
        BatchIssueKind::PlaceholderUnknown { placeholder } => {
            batch_i18n(i18n, &key, &[("placeholder", placeholder.clone())])
        }
        BatchIssueKind::QueryKeyUnknown { key } => batch_i18n(i18n, &key, &[("key", key.clone())]),
        BatchIssueKind::QueryValueEmpty { key } => batch_i18n(i18n, &key, &[("key", key.clone())]),
        BatchIssueKind::RowLimitExceeded { limit } => {
            batch_i18n(i18n, &key, &[("limit", limit.to_string())])
        }
        _ => i18n.t(&key),
    };
    batch_i18n(
        i18n,
        "sessionManager.batchAdd.issue_line",
        &[("line", line)],
    ) + &text
}

/// Template contents can include proxy credentials and share the catalog's protected storage.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct BatchTemplatePreset {
    pub id: String,
    pub name: String,
    pub template: SecretString,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BatchAddAuthKind {
    #[default]
    Password,
    Key,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchAddEditorKind {
    Template,
    Data,
}

pub struct BatchAddDialogState {
    pub(super) existing_connections:
        Option<std::rc::Rc<dyn Fn(&App) -> Vec<oxideterm_connections::BatchExistingConnection>>>,
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub template_editor: Option<Entity<TextEditorView>>,
    pub data_editor: Option<Entity<TextEditorView>>,
    pub template_editor_subscription: Option<Subscription>,
    pub data_editor_subscription: Option<Subscription>,
    pub focused_editor: Option<BatchAddEditorKind>,
    pub password: Zeroizing<String>,
    pub key_path: String,
    pub auth_kind: BatchAddAuthKind,
    pub duplicate: BatchDuplicateNamePolicy,
    pub template: Option<BatchTemplate>,
    pub template_issue: Option<BatchIssue>,
    pub preview: BatchPreview,
    pub error: Option<String>,
    pub templates: Vec<BatchTemplatePreset>,
    pub templates_loaded: bool,
    pub selected_template_id: Option<String>,
    pub renaming_id: Option<String>,
    pub rename_draft: String,
    pub new_template_naming: bool,
    pub template_menu_open: bool,
    pub key_menu_open: bool,
    pub data_file_picker_task: Option<Task<()>>,
}

impl Default for BatchAddDialogState {
    fn default() -> Self {
        Self {
            existing_connections: None,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            template_editor: None,
            data_editor: None,
            template_editor_subscription: None,
            data_editor_subscription: None,
            focused_editor: None,
            password: Zeroizing::new(String::new()),
            key_path: String::new(),
            auth_kind: BatchAddAuthKind::default(),
            duplicate: BatchDuplicateNamePolicy::default(),
            template: None,
            template_issue: None,
            preview: BatchPreview::default(),
            error: None,
            templates: Vec::new(),
            templates_loaded: false,
            selected_template_id: None,
            renaming_id: None,
            rename_draft: String::new(),
            new_template_naming: false,
            template_menu_open: false,
            key_menu_open: false,
            data_file_picker_task: None,
        }
    }
}

impl BatchAddDialogState {
    /// The dialog authentication is only a fallback; the template identity file
    /// wins whenever it is present, so this stays `None` for key templates.
    pub fn default_auth(&self) -> Option<SavedAuth> {
        match self.auth_kind {
            BatchAddAuthKind::Password if !self.password.is_empty() => Some(SavedAuth::Password {
                empty_password: false,
                keychain_id: None,
                plaintext_password: Some(SecretString::from(self.password.as_str())),
            }),
            BatchAddAuthKind::Key if !self.key_path.trim().is_empty() => Some(SavedAuth::Key {
                key_path: self.key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            }),
            _ => None,
        }
    }

    /// Whether the fallback authentication row set is relevant for the current
    /// template; a template with an identity path owns authentication itself.
    pub fn needs_fallback_auth(&self) -> bool {
        self.template.as_ref().is_none_or(|template| {
            template.identity_file.is_none() && template.certificate_file.is_none()
        })
    }

    pub fn editor_text(&self, kind: BatchAddEditorKind, cx: &App) -> String {
        let editor = match kind {
            BatchAddEditorKind::Template => &self.template_editor,
            BatchAddEditorKind::Data => &self.data_editor,
        };
        editor
            .as_ref()
            .map(|editor| editor.read(cx).buffer().text())
            .unwrap_or_default()
    }
}

impl SessionManagerState {
    pub fn set_batch_template_menu(&mut self, open: Option<bool>, cx: &mut Context<Self>) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        let open = open.unwrap_or(!dialog.template_menu_open);
        if dialog.template_menu_open == open {
            return;
        }
        dialog.template_menu_open = open;
        if open {
            dialog.key_menu_open = false;
        }
        dialog.renaming_id = None;
        dialog.new_template_naming = false;
        dialog.rename_draft.clear();
        cx.notify();
    }

    pub fn set_batch_key_menu(&mut self, open: Option<bool>, cx: &mut Context<Self>) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        let open = open.unwrap_or(!dialog.key_menu_open);
        if dialog.key_menu_open == open {
            return;
        }
        dialog.key_menu_open = open;
        if open {
            dialog.template_menu_open = false;
        }
        cx.notify();
    }

    pub fn begin_batch_add_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.batch_add.as_mut() else {
            return false;
        };
        let Some(generation) = dialog.presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
        if delay.is_zero() {
            self.finish_batch_add_dialog_exit(generation, cx);
            return true;
        }
        self.batch_add_exit_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.finish_batch_add_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn finish_batch_add_dialog_exit(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.presence.finish_exit(generation))
        {
            return false;
        }
        // Dropping the state is the final boundary for the password draft;
        // Zeroizing clears it on drop together with both editors.
        self.batch_add = None;
        self.batch_add_exit_task = None;
        cx.emit(super::SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
        true
    }
}
