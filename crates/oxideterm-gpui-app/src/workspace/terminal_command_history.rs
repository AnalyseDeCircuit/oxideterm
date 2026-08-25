use super::*;

const TERMINAL_COMMAND_HISTORY_KEYCHAIN_SERVICE: &str = "com.oxideterm.command-history";
const TERMINAL_COMMAND_HISTORY_KEYCHAIN_ACCOUNT: &str = "terminal-command-history-v1";
const TERMINAL_COMMAND_HISTORY_SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

pub(super) fn load_terminal_command_history() -> anyhow::Result<SharedTerminalCommandHistory> {
    let store =
        oxideterm_secret_store::NativeSecretStore::new(TERMINAL_COMMAND_HISTORY_KEYCHAIN_SERVICE);
    let Some(document) = store.get(TERMINAL_COMMAND_HISTORY_KEYCHAIN_ACCOUNT)? else {
        return Ok(SharedTerminalCommandHistory::default());
    };
    SharedTerminalCommandHistory::from_protected_json(&document)
}

fn store_terminal_command_history(document: &str) -> anyhow::Result<()> {
    oxideterm_secret_store::NativeSecretStore::new(TERMINAL_COMMAND_HISTORY_KEYCHAIN_SERVICE)
        .store(TERMINAL_COMMAND_HISTORY_KEYCHAIN_ACCOUNT, document)
}

impl WorkspaceApp {
    pub(super) fn schedule_terminal_command_history_save(&mut self, cx: &mut Context<Self>) {
        if self.terminal_command_history_save_scheduled
            || !self.terminal_command_history_persistence_available
        {
            return;
        }
        self.terminal_command_history_save_scheduled = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(TERMINAL_COMMAND_HISTORY_SAVE_DEBOUNCE).await;
                let Some(history) = weak
                    .update(cx, |workspace, _cx| {
                        workspace.terminal_command_history.clone()
                    })
                    .ok()
                else {
                    return;
                };
                let save = cx.background_executor().spawn(async move {
                    let (revision, document) = history.protected_json()?;
                    store_terminal_command_history(&document)?;
                    anyhow::Ok(revision)
                });
                let result = save.await;
                let continue_saving = weak
                    .update(cx, |workspace, _cx| {
                        let revision = match result {
                            Ok(revision) => revision,
                            Err(error) => {
                                // Disable writes after a credential-manager failure so an unreadable
                                // protected history is never replaced by an in-memory snapshot.
                                eprintln!(
                                    "failed to store protected terminal command history: {error}"
                                );
                                workspace.terminal_command_history_persistence_available = false;
                                workspace.terminal_command_history_save_scheduled = false;
                                return false;
                            }
                        };
                        if workspace.terminal_command_history.revision() != revision {
                            return true;
                        }
                        workspace.terminal_command_history_save_scheduled = false;
                        false
                    })
                    .unwrap_or(false);
                if !continue_saving {
                    return;
                }
            }
        })
        .detach();
    }
}
