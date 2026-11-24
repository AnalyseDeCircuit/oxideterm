impl IdeSurface {
    fn go_to_symbol_definition(&mut self, word: String, cx: &mut Context<Self>) {
        if !self.fs.supports_symbols() {
            return;
        }
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let Some(buffer) = self
            .workspace
            .active_tab()
            .and_then(|tab_id| self.workspace.buffer(tab_id))
            .cloned()
        else {
            return;
        };
        let IdeLocation::Remote { node_id, .. } = buffer.location else {
            return;
        };
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;

        cx.spawn(async move |weak, cx| {
            let result = backend_runtime
                .spawn(async move {
                    // Tauri runs symbol lookup as an agent-side best-effort
                    // RPC. Keep failures non-fatal so modifier-click remains
                    // a lightweight editor gesture.
                    fs.node_agent_symbol_definitions(node_id, root_path, word)
                        .await
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(definitions) => {
                        if let Some(definition) = definitions.into_iter().next() {
                            this.open_symbol_definition(definition, cx);
                        }
                    }
                    Err(_error) => {
                        // Tauri treats modifier-click symbol lookup as a
                        // best-effort editor gesture, so failed lookups should
                        // not interrupt normal editing.
                    }
                }
            });
        })
        .detach();
    }

    fn open_symbol_definition(
        &mut self,
        definition: oxideterm_ide_fs::NodeAgentSymbolInfo,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.pending_editor_reveals.insert(
            definition.path.clone(),
            PendingEditorReveal {
                line: definition.line,
                column: definition.column,
            },
        );
        self.open_remote_file(
            IdeLocation::remote(node_id, definition.path),
            Some(oxideterm_audit::AuditSource::User),
            cx,
        );
    }
}
