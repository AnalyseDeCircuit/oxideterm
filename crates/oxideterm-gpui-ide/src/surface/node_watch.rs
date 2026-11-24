impl IdeSurface {
    fn refresh_tree_for_watch_path(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.mount.is_visible() {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let path = normalize_remote_path(&path);
        let root_path = normalize_remote_path(&root_path);
        if path != root_path && !path.starts_with(&format!("{}/", root_path.trim_end_matches('/')))
        {
            return;
        }
        self.load_directory(IdeLocation::remote(node_id, path), None, cx);
    }

    fn start_agent_watch_if_ready(&mut self, cx: &mut Context<Self>) {
        if !self.fs.supports_watching() {
            return;
        }
        if !self.mount.is_visible() {
            self.stop_agent_watch(cx);
            return;
        }
        if self.desktop.agent_watch_stop_in_flight {
            // Re-evaluate readiness after the retained stop operation finishes.
            self.desktop.agent_watch_restart_requested = true;
            return;
        }
        if !matches!(
            self.fs.status_for_node(self.node_id.as_deref()),
            AgentStatus::Ready { .. }
        ) {
            self.stop_agent_watch(cx);
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let root_path = normalize_remote_path(&root_path);
        if self.desktop.watched_root_path.as_deref() == Some(root_path.as_str()) {
            return;
        }
        if self.desktop.watched_root_path.is_some() {
            self.stop_agent_watch_with_restart(true, cx);
            return;
        }
        self.desktop.agent_watch_generation = self.desktop.agent_watch_generation.wrapping_add(1);
        let generation = self.desktop.agent_watch_generation;
        self.desktop.watched_root_path = Some(root_path.clone());
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let watch_backend_task = backend_runtime.spawn({
            let root_path = root_path.clone();
            async move { fs.watch_directory(node_id, root_path, Vec::new()).await }
        });
        self.desktop.agent_watch_backend_abort = Some(watch_backend_task.abort_handle());
        self.desktop.agent_watch_task = Some(cx.spawn(async move |weak, cx| {
            let watch = await_ide_backend(watch_backend_task).await;

            match watch {
                Ok(Some(mut subscription)) => {
                    while let Some(event) = subscription.recv().await {
                        let refresh_path = watch_refresh_path(&root_path, &event.path);
                        let should_continue = weak
                            .update(cx, |this, cx| {
                                if this.desktop.agent_watch_generation != generation
                                    || !this.mount.is_visible()
                                {
                                    return false;
                                }
                                this.clear_search_cache();
                                this.refresh_tree_for_watch_path(refresh_path, cx);
                                true
                            })
                            .unwrap_or(false);
                        if !should_continue {
                            return;
                        }
                    }
                }
                Ok(None) | Err(_) => {}
            }

            let _ = weak.update(cx, |this, cx| {
                if this.desktop.agent_watch_generation == generation
                    && this.desktop.watched_root_path.as_deref() == Some(root_path.as_str())
                    && this.mount.is_visible()
                {
                    this.desktop.agent_watch_backend_abort = None;
                    this.schedule_agent_watch_retry(cx);
                }
            });
        }));
    }

    fn schedule_agent_watch_retry(&mut self, cx: &mut Context<Self>) {
        if !self.mount.is_visible() {
            return;
        }
        let generation = self.desktop.agent_watch_generation;
        self.desktop.agent_watch_retry_task = Some(cx.spawn(async move |weak, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(IDE_AGENT_WATCH_RETRY_SECS))
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.desktop.agent_watch_generation == generation && this.mount.is_visible() {
                    this.desktop.watched_root_path = None;
                    this.start_agent_watch_if_ready(cx);
                }
            });
        }));
    }

    fn stop_agent_watch(&mut self, cx: &mut Context<Self>) {
        self.stop_agent_watch_with_restart(false, cx);
    }

    fn stop_agent_watch_with_restart(&mut self, restart_after_stop: bool, cx: &mut Context<Self>) {
        self.desktop.agent_watch_generation = self.desktop.agent_watch_generation.wrapping_add(1);
        self.desktop.agent_watch_task = None;
        self.desktop.agent_watch_retry_task = None;
        if let Some(abort_handle) = self.desktop.agent_watch_backend_abort.take() {
            // The Tokio acquisition can otherwise outlive the cancelled GPUI
            // listener and register a consumer after the surface is released.
            abort_handle.abort();
        }
        if self.desktop.agent_watch_stop_in_flight {
            self.desktop.agent_watch_restart_requested = restart_after_stop;
            return;
        }
        let Some(root_path) = self.desktop.watched_root_path.take() else {
            return;
        };
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let release_queue = Arc::new(IdeWatchStopReleaseQueue::default());
        self.desktop.agent_watch_stop_release_queue = Some(release_queue.clone());
        self.desktop.agent_watch_stop_generation =
            self.desktop.agent_watch_stop_generation.wrapping_add(1);
        let stop_generation = self.desktop.agent_watch_stop_generation;
        self.desktop.agent_watch_stop_in_flight = true;
        self.desktop.agent_watch_restart_requested = restart_after_stop;
        self.desktop.agent_watch_stop_task = Some(cx.spawn(async move |weak, cx| {
            let _ = backend_runtime
                .spawn(async move {
                    let _ = fs.stop_watch_directory(node_id, root_path).await;
                    // Resource release belongs to this backend completion path,
                    // not the optional GPUI weak update below.
                    release_queue.finish_stop(&fs);
                })
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.desktop.agent_watch_stop_generation != stop_generation {
                    return;
                }
                this.desktop.agent_watch_stop_in_flight = false;
                this.desktop.agent_watch_stop_release_queue = None;
                if std::mem::take(&mut this.desktop.agent_watch_restart_requested) {
                    this.start_agent_watch_if_ready(cx);
                }
            });
        }));
    }
}
