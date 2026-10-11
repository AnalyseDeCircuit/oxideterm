use super::*;
use futures_util::FutureExt;

#[derive(Default)]
struct HistoryQuit {
    task: Option<gpui::Task<()>>,
}
impl gpui::Global for HistoryQuit {}

/// All explicit exit paths leave the UI running while the database acknowledges its final changes.
pub(crate) fn request_app_quit(cx: &mut App) {
    // Quit can originate inside a workspace update; release its lease before reading it.
    cx.defer(start_app_quit);
}

fn start_app_quit(cx: &mut App) {
    if !cx.has_global::<HistoryQuit>() {
        cx.set_global(HistoryQuit::default());
    }
    if cx.global::<HistoryQuit>().task.is_some() {
        return;
    }
    let sessions: Vec<_> = cx
        .windows()
        .into_iter()
        .filter_map(|window| {
            window
                .downcast::<WorkspaceWindowShell>()
                .and_then(|window| window.read(cx).ok().map(|shell| shell.session_entity()))
        })
        .collect();
    let Some(first) = sessions.first() else {
        oxideterm_desktop_presence::request_quit();
        cx.quit();
        return;
    };
    let i18n = first.read(cx).i18n.clone();
    for session in &sessions {
        session.update(cx, |workspace, cx| workspace.stop_ai_for_quit(cx));
    }
    let task = cx.spawn(async move |cx| {
        loop {
            let waits = cx.update(|cx| {
                sessions
                    .iter()
                    .map(|session| {
                        session.update(cx, |workspace, cx| {
                            workspace.ai_entity.update(cx, |ai, _| {
                                ai.persist_chat_state();
                                let (sender, receiver) = tokio::sync::oneshot::channel();
                                ai.history_barrier(sender);
                                receiver
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            });
            let saved = futures_util::future::join_all(
                waits
                    .into_iter()
                    .map(|wait| async move { wait.await.unwrap_or(false) }),
            )
            .boxed_local();
            let timeout = Timer::after(Duration::from_secs(5)).boxed_local();
            if let futures_util::future::Either::Left((saved, _)) =
                futures_util::future::select(saved, timeout).await
            {
                if saved.iter().all(|saved| *saved) {
                    cx.update(|cx| {
                        oxideterm_desktop_presence::request_quit();
                        cx.quit();
                    });
                    return;
                }
            }
            let prompt = cx.update(|cx| {
                let window = cx
                    .active_window()
                    .or_else(|| cx.windows().first().copied())?;
                window
                    .update(cx, |_, window, cx| {
                        window.prompt(
                            gpui::PromptLevel::Warning,
                            &i18n.t("ai.history.quit_title"),
                            Some(&i18n.t("ai.history.quit_detail")),
                            &[
                                i18n.t("common.actions.retry").as_str(),
                                i18n.t("ai.history.cancel_quit").as_str(),
                                i18n.t("ai.history.discard_quit").as_str(),
                            ],
                            cx,
                        )
                    })
                    .ok()
            });
            match prompt {
                Some(prompt) => match prompt.await {
                    Ok(0) => {
                        cx.update(|cx| {
                            for session in &sessions {
                                session.update(cx, |workspace, cx| {
                                    workspace
                                        .ai_entity
                                        .update(cx, |ai, _| ai.retry_history_write())
                                });
                            }
                        });
                    }
                    Ok(2) => {
                        cx.update(|cx| {
                            oxideterm_desktop_presence::request_quit();
                            cx.quit();
                        });
                        return;
                    }
                    _ => break,
                },
                None => break,
            }
        }
        cx.update(|cx| {
            for session in &sessions {
                session.update(cx, |workspace, cx| {
                    workspace.ai_entity.update(cx, |ai, cx| {
                        ai.history.quitting = false;
                        cx.notify();
                    })
                });
            }
            cx.global_mut::<HistoryQuit>().task.take();
        });
    });
    cx.global_mut::<HistoryQuit>().task = Some(task);
}

impl WorkspaceApp {
    fn stop_ai_for_quit(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _| {
            ai.history.quitting = true;
            ai.history
                .migration_cancel
                .store(true, std::sync::atomic::Ordering::Release);
            let ids: Vec<_> = ai
                .conversation_state()
                .conversations
                .iter()
                .map(|conversation| conversation.id.clone())
                .collect();
            for id in ids {
                ai.cancel_chat_stream_for(&id);
                let stopped = ai
                    .conversation_state_mut()
                    .conversations
                    .iter_mut()
                    .find(|conversation| conversation.id == id)
                    .map(oxideterm_ai::stream_state::finalize_streaming_ai_messages_on_cancel)
                    .unwrap_or_default();
                for message in stopped {
                    if message.retained {
                        ai.history_message_changed(&id, &message.message_id);
                    } else {
                        ai.history_message_deleted(&id, &message.message_id);
                    }
                }
            }
            ai.persist_chat_state();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn quit_requested_from_workspace_update_finishes_without_reentry(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_HISTORY_QUIT_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; isolate it from the user's installation.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(PathBuf::from(fixture_dir)));
        let mut store = SettingsStore::load_from_path(&settings_path).unwrap();
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();

        // Update completion requests quit while the workspace entity is still leased.
        workspace.update(cx, |workspace, cx| workspace.request_application_quit(cx));
        let task = cx.update(|_, cx| cx.global_mut::<HistoryQuit>().task.take().unwrap());
        cx.foreground_executor().block_test(task);
        workspace.read_with(cx, |workspace, cx| {
            assert!(workspace.ai_entity.read(cx).history.quitting);
        });
    }
}
