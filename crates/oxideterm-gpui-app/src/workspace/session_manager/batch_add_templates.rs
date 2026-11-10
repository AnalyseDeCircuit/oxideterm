use super::*;

use oxideterm_atomic_file::durable_write;

pub(super) const BATCH_TEMPLATES_FILENAME: &str = "oxideterm-batch-templates.json";
/// Factory templates are never persisted and never deletable: every load
/// re-seeds them so the dialog always has usable starting points.
pub(super) const DEFAULT_BATCH_TEMPLATE_ID: &str = "preset-default";
pub(super) const DEFAULT_AWS_BATCH_TEMPLATE_ID: &str = "preset-aws";

/// Factory templates share the `preset-` id prefix; the prefix is the guard
/// used by rename and delete.
pub(super) fn is_factory_batch_template_id(id: &str) -> bool {
    id.starts_with("preset-")
}

/// A saved template for the batch-add dialog, mirroring the SFTP path bookmark
/// persistence shape (id/name/createdAt next to the settings file).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub(super) struct BatchTemplatePreset {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) template: String,
    #[serde(rename = "createdAt")]
    pub(super) created_at: u64,
}

fn batch_templates_path(settings_path: &std::path::Path) -> std::path::PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(BATCH_TEMPLATES_FILENAME)
}

fn load_batch_templates(
    settings_path: &std::path::Path,
) -> (Vec<BatchTemplatePreset>, Option<String>) {
    let path = batch_templates_path(settings_path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // No catalog yet is the normal first-run state, not a failure.
            return (Vec::new(), None);
        }
        // An unreadable catalog must not look empty: a later save would
        // overwrite presets the user cannot see.
        Err(error) => {
            return (Vec::new(), Some(format!("{}: {error}", path.display())));
        }
    };
    match serde_json::from_slice::<Vec<BatchTemplatePreset>>(&bytes) {
        Ok(templates) => (templates, None),
        Err(error) => {
            // Keep the unreadable catalog next to the original instead of
            // letting the next save overwrite the only copy of the user's
            // templates, and report it so the dialog can say so.
            let backup =
                path.with_extension(format!("json.corrupt-{}", oxideterm_local_files::now_ms()));
            let backup_note = match std::fs::copy(&path, &backup) {
                Ok(_) => format!("{}", backup.display()),
                Err(copy_error) => format!("{} ({copy_error})", path.display()),
            };
            (Vec::new(), Some(format!("{error} [{backup_note}]")))
        }
    }
}

pub(super) fn save_batch_templates(
    settings_path: &std::path::Path,
    templates: &[BatchTemplatePreset],
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(templates).map_err(|error| error.to_string())?;
    durable_write(&batch_templates_path(settings_path), &bytes)
        .map_err(|error| format!("{}: {error}", BATCH_TEMPLATES_FILENAME))
}

/// Two saves within the same millisecond would otherwise share an id, and every
/// later lookup is by id.
fn new_batch_template_id(templates: &[BatchTemplatePreset]) -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let mut candidate = format!("template-{millis}");
    let mut suffix = 1usize;
    while templates.iter().any(|template| template.id == candidate) {
        candidate = format!("template-{millis}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn default_batch_template_preset(id: &str, name: String) -> BatchTemplatePreset {
    let template = match id {
        DEFAULT_AWS_BATCH_TEMPLATE_ID => concat!(
            "Host --\n",
            "  HostName {host}\n",
            "  User user.name\n",
            "  Port 22\n",
            "  IdentityFile ~/.ssh/key\n",
            // The commented variant documents the Windows alternative; the
            // parser drops comment lines, so only the sh form stays active.
            "  ProxyCommand sh -c \"aws ssm start-session --profile {profile} --target %h --document-name AWS-StartSSHSession --parameters 'portNumber=%p'\"\n",
            "  # ProxyCommand powershell.exe \"aws ssm start-session --profile {profile} --target %h --document-name AWS-StartSSHSession --parameters 'portNumber=%p'\"\n",
        ),
        _ => concat!(
            "Host xx\n",
            "  HostName {host}\n",
            "  User user.name\n",
            "  Port 22\n",
            "  IdentityFile ~/.ssh/key\n",
        ),
    };
    BatchTemplatePreset {
        id: id.to_owned(),
        name,
        template: template.to_owned(),
        created_at: 0,
    }
}

fn default_batch_template_presets(
    default_name: String,
    aws_name: String,
) -> Vec<BatchTemplatePreset> {
    vec![
        default_batch_template_preset(DEFAULT_BATCH_TEMPLATE_ID, default_name),
        default_batch_template_preset(DEFAULT_AWS_BATCH_TEMPLATE_ID, aws_name),
    ]
}

/// Factory presets always sit in front of the persisted user templates.
fn batch_templates_with_presets(
    mut user_templates: Vec<BatchTemplatePreset>,
    presets: Vec<BatchTemplatePreset>,
) -> Vec<BatchTemplatePreset> {
    user_templates.sort_by_key(|template| template.created_at);
    for (index, preset) in presets.into_iter().enumerate() {
        user_templates.insert(index, preset);
    }
    user_templates
}

impl WorkspaceApp {
    fn batch_template_presets(&self) -> Vec<BatchTemplatePreset> {
        default_batch_template_presets(
            self.i18n.t("sessionManager.batchAdd.preset_name"),
            self.i18n.t("sessionManager.batchAdd.preset_aws_name"),
        )
    }

    /// Loads the persisted user templates plus the always-present factory
    /// preset, then preloads the preset into the editor so the dialog opens
    /// with a usable starting point.
    pub(in crate::workspace) fn ensure_batch_templates_loaded(&mut self, cx: &mut Context<Self>) {
        let already_loaded = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.templates_loaded);
        if already_loaded {
            return;
        }
        let presets = self.batch_template_presets();
        let first_template = presets[0].template.clone();
        let (user_templates, load_error) = load_batch_templates(self.settings_store.path());
        let load_error = load_error.map(|error| {
            format!(
                "{}: {error}",
                self.i18n.t("sessionManager.batchAdd.templates_load_failed")
            )
        });
        let templates = batch_templates_with_presets(user_templates, presets);
        let editor = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return None;
            };
            dialog.templates = templates;
            dialog.templates_loaded = true;
            dialog.selected_template_id = Some(DEFAULT_BATCH_TEMPLATE_ID.to_owned());
            dialog.renaming_id = None;
            dialog.error = load_error;
            cx.notify();
            dialog.template_editor.clone()
        });
        if let Some(editor) = editor {
            editor.update(cx, |editor, cx| {
                editor.replace_text_external(first_template, cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn select_batch_add_template(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let template = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return None;
            };
            let template = dialog
                .templates
                .iter()
                .find(|template| template.id == id)
                .cloned()?;
            dialog.selected_template_id = Some(template.id.clone());
            dialog.renaming_id = None;
            dialog.error = None;
            cx.notify();
            Some(template)
        });
        let Some(template) = template else {
            return;
        };
        let editor = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .and_then(|dialog| dialog.template_editor.clone());
        if let Some(editor) = editor {
            editor.update(cx, |editor, cx| {
                editor.replace_text_external(template.template, cx);
            });
        }
        cx.notify();
    }

    /// Overwrites the currently selected user template with the editor
    /// content. The factory preset is never overwritten.
    pub(in crate::workspace) fn save_batch_template_into_selected(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let template_text = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .and_then(|dialog| dialog.template_editor.as_ref())
            .map(|editor| editor.read(cx).buffer().text())
            .unwrap_or_default();
        if template_text.trim().is_empty() {
            let empty_error = self.i18n.t("sessionManager.batchAdd.template_empty_save");
            self.session_manager.update(cx, |session_manager, cx| {
                if let Some(dialog) = session_manager.batch_add.as_mut() {
                    dialog.error = Some(empty_error);
                    cx.notify();
                }
            });
            return;
        }
        if let Err(issue) = oxideterm_connections::parse_batch_template(&template_text) {
            // A saved preset that never parses would fail on every open, so
            // the parse error is reported instead of persisted.
            let text = self.batch_issue_text(&issue);
            self.session_manager.update(cx, |session_manager, cx| {
                if let Some(dialog) = session_manager.batch_add.as_mut() {
                    dialog.template_issue = Some(issue.clone());
                    dialog.error = Some(text);
                    cx.notify();
                }
            });
            return;
        }
        let user_templates = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return None;
            };
            let Some(selected_id) = dialog.selected_template_id.clone() else {
                return None;
            };
            if is_factory_batch_template_id(&selected_id) {
                return None;
            }
            let Some(template) = dialog
                .templates
                .iter_mut()
                .find(|template| template.id == selected_id)
            else {
                return None;
            };
            template.template = template_text;
            dialog.error = None;
            cx.notify();
            Some(
                dialog
                    .templates
                    .iter()
                    .filter(|template| !is_factory_batch_template_id(&template.id))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        });
        let Some(user_templates) = user_templates else {
            return;
        };
        self.report_batch_template_save_result(
            save_batch_templates(self.settings_store.path(), &user_templates),
            cx,
        );
        cx.notify();
    }

    /// Opens the template dropdown with a name prompt on top; committing
    /// creates a new template from the editor content under that name.
    pub(in crate::workspace) fn begin_batch_template_naming(&mut self, cx: &mut Context<Self>) {
        let template_text = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .and_then(|dialog| dialog.template_editor.as_ref())
            .map(|editor| editor.read(cx).buffer().text())
            .unwrap_or_default();
        if template_text.trim().is_empty() {
            let empty_error = self.i18n.t("sessionManager.batchAdd.template_empty_save");
            self.session_manager.update(cx, |session_manager, cx| {
                if let Some(dialog) = session_manager.batch_add.as_mut() {
                    dialog.error = Some(empty_error);
                    cx.notify();
                }
            });
            return;
        }
        let existing_names: Vec<String> = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .map(|dialog| {
                dialog
                    .templates
                    .iter()
                    .map(|template| template.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let suggested = (1usize..)
            .map(|index| {
                self.i18n_with(
                    "sessionManager.batchAdd.template_name_template",
                    &[("index", index.to_string())],
                )
            })
            .find(|candidate| !existing_names.contains(candidate))
            .expect("unbounded name search always finds a free name");
        self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return;
            };
            dialog.new_template_naming = true;
            dialog.template_menu_open = true;
            dialog.renaming_id = None;
            dialog.rename_draft = suggested;
            dialog.error = None;
            session_manager.focused_input = Some(SessionManagerInput::BatchTemplateName);
            cx.notify();
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        cx.notify();
    }

    /// Commits the pending name prompt: renames the pending template, or
    /// creates a new one from the editor content.
    pub(in crate::workspace) fn commit_batch_template_name(&mut self, cx: &mut Context<Self>) {
        let template_text = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .and_then(|dialog| dialog.template_editor.as_ref())
            .map(|editor| editor.read(cx).buffer().text())
            .unwrap_or_default();
        let name = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .map(|dialog| dialog.rename_draft.trim().to_owned())
            .unwrap_or_default();
        if name.is_empty() {
            let required = self
                .i18n
                .t("sessionManager.batchAdd.template_name_required");
            self.session_manager.update(cx, |session_manager, cx| {
                if let Some(dialog) = session_manager.batch_add.as_mut() {
                    dialog.error = Some(required);
                    cx.notify();
                }
            });
            return;
        }
        if self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.new_template_naming)
        {
            if let Err(issue) = oxideterm_connections::parse_batch_template(&template_text) {
                let text = self.batch_issue_text(&issue);
                self.session_manager.update(cx, |session_manager, cx| {
                    if let Some(dialog) = session_manager.batch_add.as_mut() {
                        dialog.template_issue = Some(issue.clone());
                        dialog.error = Some(text);
                        cx.notify();
                    }
                });
                return;
            }
        }
        let save_result = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return None;
            };
            if dialog.new_template_naming {
                if template_text.trim().is_empty() {
                    return None;
                }
                let preset = BatchTemplatePreset {
                    id: new_batch_template_id(&dialog.templates),
                    name: name.clone(),
                    template: template_text,
                    created_at: oxideterm_local_files::now_ms(),
                };
                let id = preset.id.clone();
                dialog.templates.push(preset);
                dialog.selected_template_id = Some(id);
                dialog.new_template_naming = false;
                dialog.template_menu_open = false;
                dialog.rename_draft.clear();
                session_manager.focused_input = None;
                dialog.error = None;
                cx.notify();
                Some(
                    dialog
                        .templates
                        .iter()
                        .filter(|template| !is_factory_batch_template_id(&template.id))
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            } else if let Some(id) = dialog.renaming_id.clone() {
                let Some(template) = dialog
                    .templates
                    .iter_mut()
                    .find(|template| template.id == id)
                else {
                    return None;
                };
                template.name = name.clone();
                dialog.renaming_id = None;
                dialog.rename_draft.clear();
                session_manager.focused_input = None;
                cx.notify();
                Some(
                    dialog
                        .templates
                        .iter()
                        .filter(|template| !is_factory_batch_template_id(&template.id))
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        });
        let Some(user_templates) = save_result else {
            return;
        };
        self.report_batch_template_save_result(
            save_batch_templates(self.settings_store.path(), &user_templates),
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_batch_template_name(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return;
            };
            if dialog.renaming_id.take().is_some() || dialog.new_template_naming {
                dialog.new_template_naming = false;
                dialog.rename_draft.clear();
                session_manager.focused_input = None;
                cx.notify();
            }
        });
    }

    fn report_batch_template_save_result(
        &self,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let Err(error) = result else {
            return;
        };
        let message = format!(
            "{}: {error}",
            self.i18n.t("sessionManager.batchAdd.templates_save_failed")
        );
        // The in-memory list has already been mutated, so a failed write would
        // otherwise show a rename or deletion that never reached the disk.
        let presets = self.batch_template_presets();
        let (user_templates, load_error) = load_batch_templates(self.settings_store.path());
        let templates = batch_templates_with_presets(user_templates, presets);
        self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return;
            };
            dialog.templates = templates;
            let selection_is_known = dialog
                .selected_template_id
                .as_deref()
                .is_some_and(|id| dialog.templates.iter().any(|template| template.id == id));
            if !selection_is_known {
                dialog.selected_template_id = Some(DEFAULT_BATCH_TEMPLATE_ID.to_owned());
            }
            dialog.error = Some(match load_error {
                Some(load_error) => format!("{message}; {load_error}"),
                None => message,
            });
            cx.notify();
        });
    }

    pub(in crate::workspace) fn begin_batch_template_rename(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let started = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return false;
            };
            let Some(template) = dialog
                .templates
                .iter()
                .find(|template| template.id == id)
                .cloned()
            else {
                return false;
            };
            if is_factory_batch_template_id(&template.id) {
                return false;
            }
            dialog.renaming_id = Some(template.id.clone());
            dialog.rename_draft = template.name;
            dialog.error = None;
            session_manager.focused_input = Some(SessionManagerInput::BatchTemplateName);
            cx.notify();
            true
        });
        if started {
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
        cx.notify();
    }

    pub(in crate::workspace) fn delete_batch_template(&mut self, id: &str, cx: &mut Context<Self>) {
        let user_templates = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return None;
            };
            if is_factory_batch_template_id(id) {
                return None;
            }
            dialog.templates.retain(|template| template.id != id);
            if dialog.selected_template_id.as_deref() == Some(id) {
                dialog.selected_template_id = Some(DEFAULT_BATCH_TEMPLATE_ID.to_owned());
            }
            dialog.renaming_id = None;
            dialog.error = None;
            cx.notify();
            Some(
                dialog
                    .templates
                    .iter()
                    .filter(|template| !is_factory_batch_template_id(&template.id))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        });
        let Some(user_templates) = user_templates else {
            return;
        };
        self.report_batch_template_save_result(
            save_batch_templates(self.settings_store.path(), &user_templates),
            cx,
        );
        cx.notify();
    }
}
