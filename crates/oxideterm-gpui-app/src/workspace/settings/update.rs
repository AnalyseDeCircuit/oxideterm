#[derive(Clone, Debug)]
pub(in crate::workspace) enum NativeUpdateUiState {
    Idle,
    Checking,
    UpToDate,
    Available(oxideterm_update::NativeUpdatePackage),
    Downloading,
    Downloaded(oxideterm_update::NativeUpdateDownload),
    Error(String),
}

impl WorkspaceApp {
    fn check_native_update(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.native_update_state,
            NativeUpdateUiState::Checking | NativeUpdateUiState::Downloading
        ) {
            return;
        }

        self.native_update_state = NativeUpdateUiState::Checking;
        let channel = self.settings_store.settings().general.update_channel;
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let runtime = self.forwarding_runtime.clone();

        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn(async move {
                    let client = oxideterm_update::NativeUpdateClient::new()?;
                    client
                        .check(oxideterm_update::NativeUpdateRequest::current(
                            channel,
                            current_version,
                        ))
                        .await
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));

            let _ = weak.update(cx, |this, cx| {
                this.native_update_state = match result {
                    Ok(oxideterm_update::NativeUpdateStatus::UpToDate) => {
                        NativeUpdateUiState::UpToDate
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::Available(package)) => {
                        NativeUpdateUiState::Available(package)
                    }
                    Err(error) => NativeUpdateUiState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn download_native_update(&mut self, cx: &mut Context<Self>) {
        let package = match &self.native_update_state {
            NativeUpdateUiState::Available(package) => package.clone(),
            NativeUpdateUiState::Downloaded(download) => {
                let path = download.path.clone();
                self.open_native_update_download(&path, cx);
                return;
            }
            _ => return,
        };

        self.native_update_state = NativeUpdateUiState::Downloading;
        let directory = self.native_update_download_directory();
        let runtime = self.forwarding_runtime.clone();

        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn(async move {
                    let client = oxideterm_update::NativeUpdateClient::new()?;
                    // The native updater deliberately downloads and opens the
                    // package instead of self-replacing the running binary.
                    // That keeps the GPUI preview lane compatible with Tauri
                    // manifests while avoiding platform-specific installer
                    // side effects inside the app process.
                    client
                        .download_package(package, &directory, |_| {})
                        .await
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));

            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(download) => {
                        let path = download.path.clone();
                        this.native_update_state = NativeUpdateUiState::Downloaded(download);
                        this.open_native_update_download(&path, cx);
                    }
                    Err(error) => {
                        this.native_update_state = NativeUpdateUiState::Error(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_native_update_download(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if let Err(error) = open_path_external(path) {
            self.push_ai_settings_toast(error.to_string(), TerminalNoticeVariant::Error);
        }
        cx.notify();
    }

    fn native_update_download_directory(&self) -> std::path::PathBuf {
        self.settings_store
            .path()
            .parent()
            .map(|parent| parent.join("updates"))
            .unwrap_or_else(|| std::path::PathBuf::from("updates"))
    }
}
