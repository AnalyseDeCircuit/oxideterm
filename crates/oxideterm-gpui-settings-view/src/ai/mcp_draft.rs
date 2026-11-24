use std::time::Duration;

use gpui::{Context, Task};
use oxideterm_gpui_ui::motion::ExitPresence;
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{AiMcpServerDraft, ai_mcp_draft_valid};
use zeroize::Zeroize;

use crate::{
    SettingsInput,
    entity::{SettingsWorkspaceEntity, SettingsWorkspaceEvent},
};

pub struct McpDraftState {
    draft: Option<AiMcpServerDraft>,
    presence: ExitPresence,
    exit: Option<Task<()>>,
    pending: Option<AiMcpServerDraft>,
    saving: bool,
}

impl Default for McpDraftState {
    fn default() -> Self {
        Self {
            draft: None,
            presence: ExitPresence::visible(),
            exit: None,
            pending: None,
            saving: false,
        }
    }
}

impl McpDraftState {
    pub fn draft(&self) -> Option<&AiMcpServerDraft> {
        self.draft.as_ref()
    }

    pub fn phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.presence.phase()
    }

    pub fn valid(&self, settings: &PersistedSettings) -> bool {
        self.draft()
            .is_some_and(|draft| ai_mcp_draft_valid(draft, settings))
    }

    pub(crate) fn input(&self, input: SettingsInput) -> Option<&str> {
        let draft = self.draft()?;
        Some(match input {
            SettingsInput::AiMcpName => &draft.name,
            SettingsInput::AiMcpCommand => &draft.command,
            SettingsInput::AiMcpArgs => &draft.args,
            SettingsInput::AiMcpUrl => &draft.url,
            SettingsInput::AiMcpAuthHeaderName => &draft.auth_header_name,
            SettingsInput::AiMcpAuthToken => &draft.auth_token,
            SettingsInput::AiMcpEnvKey(index) => &draft.env.get(index)?.0,
            SettingsInput::AiMcpEnvValue(index) => &draft.env.get(index)?.1,
            SettingsInput::AiMcpHeaderKey(index) => &draft.headers.get(index)?.0,
            SettingsInput::AiMcpHeaderValue(index) => &draft.headers.get(index)?.1,
            _ => return None,
        })
    }

    pub(crate) fn input_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        let draft = self.draft.as_mut()?;
        Some(match input {
            SettingsInput::AiMcpName => &mut draft.name,
            SettingsInput::AiMcpCommand => &mut draft.command,
            SettingsInput::AiMcpArgs => &mut draft.args,
            SettingsInput::AiMcpUrl => &mut draft.url,
            SettingsInput::AiMcpAuthHeaderName => &mut draft.auth_header_name,
            SettingsInput::AiMcpAuthToken => &mut draft.auth_token,
            SettingsInput::AiMcpEnvKey(index) => &mut draft.env.get_mut(index)?.0,
            SettingsInput::AiMcpEnvValue(index) => &mut draft.env.get_mut(index)?.1,
            SettingsInput::AiMcpHeaderKey(index) => &mut draft.headers.get_mut(index)?.0,
            SettingsInput::AiMcpHeaderValue(index) => &mut draft.headers.get_mut(index)?.1,
            _ => return None,
        })
    }
}

impl SettingsWorkspaceEntity {
    pub fn open_ai_mcp(&mut self, cx: &mut Context<Self>) {
        if self.ai_mcp.saving {
            return;
        }
        self.ai_mcp.exit = None;
        self.ai_mcp.presence.reopen();
        self.ai_mcp.draft = Some(AiMcpServerDraft::default());
        self.blur_settings_entity_input(cx);
        cx.notify();
    }

    pub fn update_ai_mcp(
        &mut self,
        edit: impl FnOnce(&mut AiMcpServerDraft),
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.ai_mcp.draft.as_mut() {
            edit(draft);
            cx.notify();
        }
    }

    pub fn begin_ai_mcp_exit(&mut self, submit: bool, wait: Duration, cx: &mut Context<Self>) {
        if self.ai_mcp.draft.is_none() {
            return;
        }
        let Some(generation) = self.ai_mcp.presence.begin_exit() else {
            return;
        };
        self.blur_settings_entity_input(cx);
        if wait.is_zero() {
            self.finish_ai_mcp_exit(submit, generation, cx);
        } else {
            self.ai_mcp.exit = Some(cx.spawn(async move |entity, cx| {
                cx.background_executor().timer(wait).await;
                let _ = entity.update(cx, |entity, cx| {
                    entity.ai_mcp.exit = None;
                    entity.finish_ai_mcp_exit(submit, generation, cx);
                });
            }));
            cx.notify();
        }
    }

    fn finish_ai_mcp_exit(&mut self, submit: bool, generation: u64, cx: &mut Context<Self>) {
        if !self.ai_mcp.presence.finish_exit(generation) {
            return;
        }
        let draft = self.ai_mcp.draft.take();
        self.ai_mcp.presence.reopen();
        if submit {
            self.ai_mcp.pending = draft;
            self.ai_mcp.saving = true;
            cx.emit(SettingsWorkspaceEvent::SubmitAiMcp);
        }
        cx.notify();
    }

    pub fn take_ai_mcp_submission(&mut self) -> Option<AiMcpServerDraft> {
        self.ai_mcp.pending.take()
    }

    pub fn complete_ai_mcp_submission(
        &mut self,
        restore: Option<AiMcpServerDraft>,
        cx: &mut Context<Self>,
    ) {
        self.ai_mcp.saving = false;
        self.ai_mcp.draft = restore;
        self.ai_mcp.presence.reopen();
        cx.notify();
    }

    pub(crate) fn remove_ai_mcp_row(&mut self, env: bool, index: usize, cx: &mut Context<Self>) {
        if let Some(draft) = self.ai_mcp.draft.as_mut() {
            let rows = if env {
                &mut draft.env
            } else {
                &mut draft.headers
            };
            if index < rows.len() {
                let (mut key, mut value) = rows.remove(index);
                key.zeroize();
                value.zeroize();
                if self
                    .settings_entity_focused_input()
                    .is_some_and(|input| match input {
                        SettingsInput::AiMcpEnvKey(i) | SettingsInput::AiMcpEnvValue(i) => {
                            env && i >= index
                        }
                        SettingsInput::AiMcpHeaderKey(i) | SettingsInput::AiMcpHeaderValue(i) => {
                            !env && i >= index
                        }
                        _ => false,
                    })
                {
                    self.blur_settings_entity_input(cx);
                }
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[gpui::test]
    fn reopening_cancels_the_exit_and_submits_only_the_current_draft(
        cx: &mut gpui::TestAppContext,
    ) {
        let state = cx.new(SettingsWorkspaceEntity::new);
        state.update(cx, |state, cx| {
            state.open_ai_mcp(cx);
            state.update_ai_mcp(
                |draft| {
                    draft.name = "obsolete".into();
                    draft.auth_token = "obsolete-token".into();
                },
                cx,
            );
            state.begin_ai_mcp_exit(true, Duration::from_millis(10), cx);
            state.open_ai_mcp(cx);
            state.update_ai_mcp(|draft| draft.name = "current".into(), cx);
        });
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        state.update(cx, |state, cx| {
            assert!(state.take_ai_mcp_submission().is_none());
            state.begin_ai_mcp_exit(true, Duration::ZERO, cx);
            let draft = state.take_ai_mcp_submission().expect("current draft");
            assert_eq!(draft.name, "current");
            assert!(draft.auth_token.is_empty());
        });
    }
}
