use std::sync::Arc;

use gpui::{Context, Task};

/// Non-secret result produced by the portable runtime status worker.
pub(in crate::workspace) struct PortableStatusRefresh {
    pub(in crate::workspace) status:
        Result<oxideterm_portable_runtime::PortableStatusSnapshot, String>,
    pub(in crate::workspace) exportable_secret_count: usize,
}

/// Read-only projection used after releasing the settings Entity borrow.
#[derive(Clone)]
pub(in crate::workspace) struct PortableStatusSnapshot {
    pub(in crate::workspace) status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) exportable_secret_count: Option<usize>,
    pub(in crate::workspace) refresh_pending: bool,
}

/// Owns settings work that must complete independently from root rendering.
pub(in crate::workspace) struct SettingsWorkspaceEntity {
    portable_status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    portable_status_error: Option<String>,
    portable_exportable_secret_count: Option<usize>,
    portable_refresh_pending: bool,
    portable_refresh_task: Option<Task<()>>,
}

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            portable_status: None,
            portable_status_error: None,
            portable_exportable_secret_count: None,
            portable_refresh_pending: false,
            portable_refresh_task: None,
        }
    }

    pub(in crate::workspace) fn portable_status_snapshot(&self) -> PortableStatusSnapshot {
        PortableStatusSnapshot {
            status: self.portable_status.clone(),
            error: self.portable_status_error.clone(),
            exportable_secret_count: self.portable_exportable_secret_count,
            refresh_pending: self.portable_refresh_pending,
        }
    }

    pub(in crate::workspace) fn portable_mode(&self) -> Option<bool> {
        self.portable_status
            .as_ref()
            .map(|status| status.is_portable)
    }

    pub(in crate::workspace) fn start_portable_status_refresh(
        &mut self,
        force: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        worker: impl FnOnce() -> PortableStatusRefresh + Send + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_refresh_pending {
            return false;
        }
        if !force
            && (self.portable_status.is_some() || self.portable_status_error.is_some())
            && self.portable_exportable_secret_count.is_some()
        {
            return false;
        }

        self.portable_refresh_pending = true;
        self.portable_refresh_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime.spawn_blocking(worker).await;
            let _ = settings.update(cx, |settings, cx| {
                settings
                    .finish_portable_status_refresh(result.map_err(|error| error.to_string()), cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_portable_status_refresh(
        &mut self,
        result: Result<PortableStatusRefresh, String>,
        cx: &mut Context<Self>,
    ) {
        self.portable_refresh_task = None;
        self.portable_refresh_pending = false;
        match result {
            Ok(PortableStatusRefresh {
                status: Ok(status),
                exportable_secret_count,
            }) => {
                self.portable_status = Some(status);
                self.portable_status_error = None;
                self.portable_exportable_secret_count = Some(exportable_secret_count);
            }
            Ok(PortableStatusRefresh {
                status: Err(error),
                exportable_secret_count,
            }) => {
                self.portable_status = None;
                self.portable_status_error = Some(error);
                self.portable_exportable_secret_count = Some(exportable_secret_count);
            }
            Err(error) => {
                self.portable_status = None;
                self.portable_status_error = Some(error);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn invalidate_portable_status(&mut self, cx: &mut Context<Self>) {
        self.portable_status = None;
        self.portable_status_error = None;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext, TestAppContext};

    use super::SettingsWorkspaceEntity;

    #[gpui::test]
    fn portable_status_refresh_is_single_flight_and_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|_| SettingsWorkspaceEntity::new());
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("test runtime"),
        );

        entity.update(cx, |entity, cx| {
            assert!(entity.start_portable_status_refresh(
                false,
                runtime,
                || super::PortableStatusRefresh {
                    status: Err("unavailable".to_string()),
                    exportable_secret_count: 2,
                },
                cx,
            ));
            assert!(
                !entity.start_portable_status_refresh(
                    false,
                    Arc::new(
                        tokio::runtime::Builder::new_multi_thread()
                            .worker_threads(1)
                            .enable_all()
                            .build()
                            .expect("second test runtime"),
                    ),
                    || unreachable!("single-flight worker"),
                    cx,
                )
            );
            entity.portable_refresh_task = None;
            entity.finish_portable_status_refresh(
                Ok(super::PortableStatusRefresh {
                    status: Err("unavailable".to_string()),
                    exportable_secret_count: 2,
                }),
                cx,
            );
        });

        entity.update(cx, |entity, _cx| {
            let snapshot = entity.portable_status_snapshot();
            assert!(!snapshot.refresh_pending);
            assert_eq!(snapshot.error.as_deref(), Some("unavailable"));
            assert_eq!(snapshot.exportable_secret_count, Some(2));
        });
    }
}
