use super::*;

impl HostToolsEntity {
    pub(in crate::health) fn package_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<&ResourcePackageSnapshot> {
        self.host_packages
            .snapshot
            .as_ref()
            .filter(|_| self.host_packages.snapshot_connection_id.as_deref() == Some(connection_id))
    }

    pub(in crate::health) fn package_snapshot_in_flight(&self) -> bool {
        self.host_packages.snapshot_in_flight
    }

    pub(crate) fn package_filter(&self) -> PackageFilter {
        self.host_packages.filter
    }

    pub(in crate::health) fn package_list_state(&self) -> ListState {
        self.host_packages.list_state.clone()
    }

    pub(crate) fn package_expanded_index(&self) -> Option<usize> {
        self.host_packages.expanded_index
    }

    pub(crate) fn select_package_filter(
        &mut self,
        filter: PackageFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_packages.filter == filter {
            return false;
        }
        self.host_packages.filter = filter;
        self.host_packages.expanded_index = None;
        cx.notify();
        true
    }

    pub(crate) fn toggle_package_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        self.host_packages.expanded_index =
            (self.host_packages.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(in crate::health) fn sync_package_list_signatures(
        &self,
        identity: &str,
        signatures: &[u64],
    ) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_packages.list_state,
            &mut self.host_packages.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(in crate::health) fn package_inspect_command(
        &self,
        connection_id: &str,
        manager: &str,
        package_name: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_package_inspect_command(&os_type, manager, package_name)
            .map(|command| command.command)
    }

    pub(crate) fn request_package_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        monitoring_enabled: bool,
        runtime: tokio::runtime::Handle,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if !monitoring_enabled {
            return Vec::new();
        }
        if self.host_packages.snapshot_in_flight {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PackageSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PackageConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_package_snapshot_command(&os_type);
        let request = HostPackageSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            failure_fallback,
        };
        self.host_packages.snapshot_connection_id = Some(connection_id);
        self.host_packages.running = Some(request.clone());
        self.host_packages.snapshot_in_flight = true;
        // Package inventory is read-only manual work, not a periodic sampler.
        let spawned = self.spawn_package_snapshot_capture(
            command.command,
            request,
            HOST_PACKAGE_SNAPSHOT_TIMEOUT,
            HOST_PACKAGE_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_packages.snapshot_in_flight = false;
            self.host_packages.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PackageConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        Vec::new()
    }

    pub(crate) fn finish_host_packages_snapshot(
        &mut self,
        mut delivery: HostPackageSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_packages.running.as_ref() != Some(&delivery.request) {
            if let Ok(output) = delivery.result.as_mut() {
                zeroize_host_snapshot_output(output);
            }
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_packages.snapshot_in_flight = false;
        self.host_packages.running = None;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                // The error-marker payload quotes raw host output, which can
                // carry credentials; only the request's fallback copy may
                // reach renderable state.
                let mut snapshot = parse_package_snapshot(&output.stdout);
                if matches!(&snapshot.status, ResourcePackageStatus::Error { .. }) {
                    snapshot.status = ResourcePackageStatus::Error {
                        message: failure_fallback,
                    };
                }
                zeroize_host_snapshot_output(&mut output);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourcePackageStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PackageSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourcePackageStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PackageUnavailable,
                            ));
                        }
                        ResourcePackageStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PackageSnapshotFailed,
                            ));
                        }
                        ResourcePackageStatus::Unknown => {}
                    }
                }
                self.host_packages.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_packages.snapshot = Some(snapshot);
            }
            Ok(mut output) => {
                zeroize_host_snapshot_output(&mut output);
                self.host_packages.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_packages.snapshot = Some(ResourcePackageSnapshot {
                    status: ResourcePackageStatus::Error {
                        message: failure_fallback,
                    },
                    managers: Vec::new(),
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::PackageSnapshotFailed,
                    ));
                }
            }
            Err(()) => {
                self.host_packages.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_packages.snapshot = Some(ResourcePackageSnapshot {
                    status: ResourcePackageStatus::Error {
                        message: failure_fallback,
                    },
                    managers: Vec::new(),
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::PackageSnapshotFailed,
                    ));
                }
            }
        }
        // A manager filter only exists while the host reports that manager;
        // otherwise the list would be empty with no chip to switch back.
        let filter_manager = package_filter_manager(self.host_packages.filter);
        let manager_present = filter_manager.is_some_and(|manager| {
            self.host_packages
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.managers.iter().any(|entry| entry.name == manager))
        });
        if filter_manager.is_some() && !manager_present {
            self.host_packages.filter = PackageFilter::All;
            self.host_packages.expanded_index = None;
        }
        cx.notify();
    }
}
