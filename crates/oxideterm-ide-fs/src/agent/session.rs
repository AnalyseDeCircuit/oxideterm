struct AgentSession {
    transport: AgentTransport,
    info: SysInfoResult,
}

impl AgentSession {
    fn new(transport: AgentTransport, info: SysInfoResult) -> Self {
        Self { transport, info }
    }

    fn is_alive(&self) -> bool {
        self.transport.is_alive()
    }

    fn status(&self) -> AgentStatus {
        if self.is_alive() {
            AgentStatus::Ready {
                version: self.info.version.clone(),
                arch: self.info.arch.clone(),
                pid: self.info.pid,
            }
        } else {
            AgentStatus::Failed {
                reason: "Agent channel closed".to_string(),
            }
        }
    }

    fn supports_capability(&self, capability: &str) -> bool {
        self.info
            .capabilities
            .iter()
            .any(|available| available == capability)
    }

    async fn read_file(&self, path: &str) -> Result<ReadFileResult, AgentError> {
        let value = self
            .transport
            .call("fs/readFile", serde_json::json!({ "path": path }))
            .await?;
        let mut result: ReadFileResult = serde_json::from_value(value)
            .map_err(|error| AgentError::Deserialize(error.to_string()))?;
        if result.encoding == "zstd+base64" {
            let compressed = base64::engine::general_purpose::STANDARD
                .decode(&result.content)
                .map_err(|error| {
                    AgentError::Deserialize(format!("Base64 decode error: {error}"))
                })?;
            let decompressed =
                zstd::stream::decode_all(compressed.as_slice()).map_err(|error| {
                    AgentError::Deserialize(format!("Zstd decompress error: {error}"))
                })?;
            result.content = String::from_utf8_lossy(&decompressed).into_owned();
            result.encoding = plain_encoding();
        }
        Ok(result)
    }

    async fn write_file(
        &self,
        path: &str,
        content: &str,
        expect_hash: Option<&str>,
    ) -> Result<WriteFileResult, AgentError> {
        let (content, encoding) =
            if self.supports_capability("zstd") && content.len() > AGENT_COMPRESS_THRESHOLD {
                let compressed = zstd::stream::encode_all(content.as_bytes(), 3)
                    .map_err(|error| AgentError::Serialize(error.to_string()))?;
                if compressed.len() < content.len() {
                    (
                        base64::engine::general_purpose::STANDARD.encode(compressed),
                        "zstd+base64",
                    )
                } else {
                    (content.to_string(), "plain")
                }
            } else {
                (content.to_string(), "plain")
            };

        let mut params = serde_json::json!({
            "path": path,
            "content": content,
            "encoding": encoding,
        });
        if let Some(hash) = expect_hash {
            params["expect_hash"] = serde_json::Value::String(hash.to_string());
        }

        let value = self.transport.call("fs/writeFile", params).await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn stat(&self, path: &str) -> Result<StatResult, AgentError> {
        let value = self
            .transport
            .call("fs/stat", serde_json::json!({ "path": path }))
            .await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, AgentError> {
        let value = self
            .transport
            .call("fs/listDir", serde_json::json!({ "path": path }))
            .await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn grep(
        &self,
        pattern: &str,
        path: &str,
        case_sensitive: bool,
        max_results: u32,
    ) -> Result<Vec<AgentGrepMatch>, AgentError> {
        let value = self
            .transport
            .call(
                "search/grep",
                serde_json::json!({
                    "pattern": pattern,
                    "path": path,
                    "case_sensitive": case_sensitive,
                    "max_results": max_results,
                }),
            )
            .await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn symbol_index(
        &self,
        path: &str,
        max_files: Option<u32>,
    ) -> Result<SymbolIndexResult, AgentError> {
        let mut params = serde_json::json!({ "path": path });
        if let Some(max_files) = max_files {
            params["max_files"] = serde_json::json!(max_files);
        }
        let value = self.transport.call("symbols/index", params).await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn symbol_complete(
        &self,
        path: &str,
        prefix: &str,
        limit: Option<u32>,
    ) -> Result<Vec<SymbolInfo>, AgentError> {
        let mut params = serde_json::json!({ "path": path, "prefix": prefix });
        if let Some(limit) = limit {
            params["limit"] = serde_json::json!(limit);
        }
        let value = self.transport.call("symbols/complete", params).await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn symbol_definitions(
        &self,
        path: &str,
        name: &str,
    ) -> Result<Vec<SymbolInfo>, AgentError> {
        let value = self
            .transport
            .call(
                "symbols/definitions",
                serde_json::json!({ "path": path, "name": name }),
            )
            .await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn watch_start(&self, path: &str, ignore: Vec<String>) -> Result<(), AgentError> {
        self.transport
            .call(
                "watch/start",
                serde_json::json!({ "path": path, "ignore": ignore }),
            )
            .await?;
        Ok(())
    }

    async fn watch_stop(&self, path: &str) -> Result<(), AgentError> {
        self.transport
            .call("watch/stop", serde_json::json!({ "path": path }))
            .await?;
        Ok(())
    }

    fn subscribe_watch_events(&self) -> broadcast::Receiver<AgentWatchEvent> {
        self.transport.subscribe_watch_events()
    }

    async fn shutdown(&self) {
        self.transport.shutdown().await;
    }
}
