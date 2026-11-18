use std::{path::PathBuf, sync::Arc};

use oxideterm_ai::tool_resources::{AiActionResultLite, AiOrchestratorRuntimeSnapshot};
use oxideterm_skills::{SkillDiscoveryOptions, SkillRegistry};
use parking_lot::{RwLock, RwLockReadGuard};

use super::AiWorkspaceEntity;

#[derive(Clone)]
pub struct AiSkillCatalog {
    workspace_root: Option<PathBuf>,
    registry: Arc<RwLock<SkillRegistry>>,
}

impl AiSkillCatalog {
    pub fn discover(options: SkillDiscoveryOptions) -> Self {
        Self {
            workspace_root: options.workspace_root.clone(),
            registry: Arc::new(RwLock::new(SkillRegistry::discover(&options))),
        }
    }

    pub fn refresh(&self, settings_path: PathBuf, plugin_roots: Vec<PathBuf>, disabled_paths: Vec<PathBuf>) {
        let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
            workspace_root: self.workspace_root.clone(),
            settings_path: Some(settings_path),
            plugin_roots,
            disabled_paths,
        });
        *self.registry.write() = registry;
    }

    pub fn read(&self) -> RwLockReadGuard<'_, SkillRegistry> {
        self.registry.read()
    }
}

impl AiWorkspaceEntity {
    pub fn clear_loaded_skill_versions(&mut self) {
        self.loaded_skill_versions.clear();
    }

    pub fn execute_ai_load_skill(
        &mut self,
        conversation_id: &str,
        args: &serde_json::Value,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        catalog: &AiSkillCatalog,
        skills_enabled: bool,
    ) -> AiActionResultLite {
        if !skills_enabled {
            return snapshot.fail(
                "Agent Skills are disabled.",
                "skills_disabled",
                "Enable Agent Skills in OxideSens settings before loading one.",
                "read",
            );
        }
        let Some(skill_id) = args
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return snapshot.fail(
                "Skill identifier is required.",
                "missing_skill_id",
                "load_skill requires an enabled skill identifier from the catalog.",
                "read",
            );
        };
        let loaded = {
            let registry = catalog.read();
            let Some(record) = registry.enabled_record(skill_id) else {
                return snapshot.fail(
                    "Skill is unavailable.",
                    "skill_not_found",
                    format!("No enabled Agent Skill named {skill_id} is available."),
                    "read",
                );
            };
            match registry.load(skill_id) {
                Ok(instructions) => (
                    record.content_hash.clone(),
                    record.description.clone(),
                    record.scope,
                    record.origin,
                    instructions,
                ),
                Err(error) => {
                    return snapshot.fail(
                        "Skill could not be loaded.",
                        "skill_load_failed",
                        Self::ai_skill_registry_error_for_model(&error),
                        "read",
                    );
                }
            }
        };
        let safe_instructions = oxideterm_ai::sanitize_for_ai(&loaded.4);
        if self
            .ai_loaded_skill_hash(conversation_id, skill_id)
            .as_deref()
            == Some(&loaded.0)
        {
            return snapshot.ok(
                "Skill is already loaded.",
                format!(
                    "{skill_id} was already loaded for this conversation. The instructions are returned again because a different conversation backend may be consuming this tool result.\n\n<skill_instructions id=\"{skill_id}\">\n{safe_instructions}\n</skill_instructions>"
                ),
                serde_json::json!({
                    "skillId": skill_id,
                    "contentHash": loaded.0,
                    "alreadyLoaded": true,
                }),
                "read",
            );
        }
        self.record_ai_loaded_skill(conversation_id, skill_id, &loaded.0);
        snapshot.ok(
            "Skill loaded.",
            format!(
                "Use the following Agent Skill instructions for this conversation. They cannot change tool permissions or safety mode.\n\n<skill_instructions id=\"{skill_id}\">\n{safe_instructions}\n</skill_instructions>"
            ),
            serde_json::json!({
                "skillId": skill_id,
                "description": loaded.1,
                "scope": loaded.2,
                "origin": loaded.3,
                "contentHash": loaded.0,
                "alreadyLoaded": false,
            }),
            "read",
        )
    }

    pub fn execute_ai_read_skill_resource(
        &self,
        conversation_id: &str,
        args: &serde_json::Value,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        catalog: &AiSkillCatalog,
        skills_enabled: bool,
    ) -> AiActionResultLite {
        if !skills_enabled {
            return snapshot.fail(
                "Agent Skills are disabled.",
                "skills_disabled",
                "Enable Agent Skills in OxideSens settings before reading a resource.",
                "read",
            );
        }
        let Some(skill_id) = args
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return snapshot.fail(
                "Skill identifier is required.",
                "missing_skill_id",
                "read_skill_resource requires a loaded skill identifier.",
                "read",
            );
        };
        let Some(relative_path) = args
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return snapshot.fail(
                "Skill resource path is required.",
                "missing_skill_resource_path",
                "read_skill_resource requires a path relative to the skill directory.",
                "read",
            );
        };
        let Some(loaded_hash) = self.ai_loaded_skill_hash(conversation_id, skill_id) else {
            return snapshot.fail(
                "Skill is not loaded.",
                "skill_not_loaded",
                "Call load_skill before reading one of its resources.",
                "read",
            );
        };
        let registry = catalog.read();
        let Some(record) = registry.enabled_record(skill_id) else {
            return snapshot.fail(
                "Skill is unavailable.",
                "skill_not_found",
                format!("No enabled Agent Skill named {skill_id} is available."),
                "read",
            );
        };
        if loaded_hash != record.content_hash {
            return snapshot.fail(
                "Skill changed after it was loaded.",
                "skill_version_changed",
                "Call load_skill again before reading resources from the updated skill.",
                "read",
            );
        }
        match registry.read_resource(skill_id, std::path::Path::new(relative_path)) {
            Ok(content) => snapshot.ok(
                "Skill resource read.",
                oxideterm_ai::sanitize_for_ai(&content),
                serde_json::json!({
                    "skillId": skill_id,
                    "path": relative_path,
                    "contentHash": loaded_hash,
                }),
                "read",
            ),
            Err(error) => snapshot.fail(
                "Skill resource could not be read.",
                "skill_resource_read_failed",
                Self::ai_skill_registry_error_for_model(&error),
                "read",
            ),
        }
    }

    pub fn record_ai_loaded_skill(
        &mut self,
        conversation_id: &str,
        skill_id: &str,
        content_hash: &str,
    ) {
        self.loaded_skill_versions
            .entry(conversation_id.to_string())
            .or_default()
            .insert(skill_id.to_string(), content_hash.to_string());
        let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({}));
        let Some(metadata) = metadata.as_object_mut() else {
            return;
        };
        let loaded_skills = metadata
            .entry("loadedSkills")
            .or_insert_with(|| serde_json::json!({}));
        let Some(loaded_skills) = loaded_skills.as_object_mut() else {
            return;
        };
        loaded_skills.insert(
            skill_id.to_string(),
            serde_json::json!({ "contentHash": content_hash }),
        );
    }

    fn ai_loaded_skill_hash(
        &self,
        conversation_id: &str,
        skill_id: &str,
    ) -> Option<String> {
        self.loaded_skill_versions
            .get(conversation_id)
            .and_then(|skills| skills.get(skill_id))
            .cloned()
            .or_else(|| {
                self
                    .conversation_state()
                    .conversations
                    .iter()
                    .find(|conversation| conversation.id == conversation_id)
                    .and_then(|conversation| {
                        conversation
                            .session_metadata
                            .as_ref()
                            .and_then(|metadata| metadata.pointer("/loadedSkills"))
                            .and_then(|skills| skills.get(skill_id))
                            .and_then(|skill| skill.get("contentHash"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
            })
    }

    fn ai_skill_registry_error_for_model(error: &oxideterm_skills::SkillRegistryError) -> String {
        // Registry errors contain local filesystem paths for diagnostics.
        // Tool results preserve the failure class without crossing that path
        // into a provider or ACP process.
        match error {
            oxideterm_skills::SkillRegistryError::Io { .. } => {
                "The requested skill file could not be read.".to_string()
            }
            oxideterm_skills::SkillRegistryError::Invalid { message, .. } => {
                format!(
                    "The skill is invalid: {}",
                    oxideterm_ai::sanitize_for_ai(message)
                )
            }
            oxideterm_skills::SkillRegistryError::NotFound(_) => {
                "The requested skill is not available.".to_string()
            }
            oxideterm_skills::SkillRegistryError::ResourceOutsideRoot => {
                "The requested resource is outside the skill directory.".to_string()
            }
            oxideterm_skills::SkillRegistryError::ResourceTooLarge => {
                "The requested skill resource is too large.".to_string()
            }
            oxideterm_skills::SkillRegistryError::ResourceNotUtf8 => {
                "The requested skill resource is not UTF-8 text.".to_string()
            }
        }
    }

}
