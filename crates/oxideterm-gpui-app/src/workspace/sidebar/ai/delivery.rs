impl WorkspaceApp {
    pub(in crate::workspace) fn schedule_ai_delivery(
        &self,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let delivery_wake = self.ai.delivery_wake.clone();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // AI runtimes finish through their own lifecycle; this only stops the UI waiter.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |weak, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if !should_drain {
                    if stopped {
                        break;
                    }
                    continue;
                }
                let Ok(Ok(backlog_remaining)) = cx.update_window(
                    window_handle,
                    |_, window, cx| {
                        weak.update(cx, |workspace, cx| {
                            let stream_backlog =
                                workspace.poll_ai_chat_stream_events(Some(window), cx);
                            let compaction_backlog = workspace.poll_ai_compaction_results(cx);
                            let inline_backlog =
                                workspace.poll_terminal_ai_inline_delivery(cx);
                            stream_backlog || compaction_backlog || inline_backlog
                        })
                    },
                ) else {
                    break;
                };
                if backlog_remaining {
                    // Preserve one continuation permit across independently bounded AI queues.
                    delivery_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();
    }
}
