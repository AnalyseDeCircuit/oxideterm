//! Owns the Redis Host Tool: endpoint discovery, optional password entry, and
//! the per-database key tree.
//!
//! Discovery lists the loopback server and every running Redis-family
//! container. Browsing reads one endpoint through `docker exec` or the host
//! `redis-cli`; a password typed in the panel reaches the remote process on the
//! exec channel's stdin, never through the command line.

use super::*;

use oxideterm_connection_monitor::ResourceRedisValue;

/// Rows drawn per frame around the visible band of the value viewer.
const HOST_REDIS_VALUE_ROW_OVERSCAN: usize = 11;

/// Keeps a stored viewer size inside both the dialog maximums and the window,
/// so a shrunk window or an oversized stored size never hides a border.
fn clamped_redis_value_dialog_size(size: (f32, f32), viewport: (f32, f32)) -> (f32, f32) {
    let max_width = (viewport.0 - HOST_REDIS_VALUE_DIALOG_VIEWPORT_MARGIN).clamp(
        HOST_REDIS_VALUE_DIALOG_MIN_WIDTH,
        HOST_REDIS_VALUE_DIALOG_MAX_WIDTH,
    );
    let max_height = (viewport.1 - HOST_REDIS_VALUE_DIALOG_CHROME_HEIGHT).clamp(
        HOST_REDIS_VALUE_DIALOG_MIN_HEIGHT,
        HOST_REDIS_VALUE_DIALOG_MAX_HEIGHT,
    );
    (
        size.0.clamp(HOST_REDIS_VALUE_DIALOG_MIN_WIDTH, max_width),
        size.1.clamp(HOST_REDIS_VALUE_DIALOG_MIN_HEIGHT, max_height),
    )
}

/// Clamped resize for the value viewer's corner handle. The window bounds win
/// over the dialog maximums so dragging can never push a border off screen.
fn resized_redis_value_dialog_size(
    start: (f32, f32),
    delta: (f32, f32),
    viewport: (f32, f32),
) -> (f32, f32) {
    clamped_redis_value_dialog_size((start.0 + delta.0, start.1 + delta.1), viewport)
}

/// Widest prepared row in the mono cell the viewer draws with. The band is
/// sized from this so horizontal scrolling reveals long values instead of
/// clipping them at the dialog edge.
fn redis_value_content_width(rows: &[HostRedisValueRow], two_column: bool) -> f32 {
    let widest = rows.iter().fold(0.0_f32, |widest, row| {
        let left = oxideterm_gpui_ui::estimate_text_width(&row.left, HOST_REDIS_VALUE_TEXT_SIZE);
        let width = if two_column {
            let right = row.right.as_deref().map_or(0.0, |right| {
                oxideterm_gpui_ui::estimate_text_width(right, HOST_REDIS_VALUE_TEXT_SIZE)
            });
            // Both cells split the band evenly, so reserve two copies of the
            // wider one instead of their sum.
            left.max(right) * 2.0 + HOST_REDIS_VALUE_ROW_GAP
        } else {
            left
        };
        widest.max(width)
    });
    widest + HOST_REDIS_VALUE_SCROLL_SLACK
}

/// One prepared viewer row. Collection rows fill both cells; text rows only
/// the left one.
#[derive(Clone)]
pub struct HostRedisValueRow {
    pub left: String,
    pub right: Option<String>,
}

pub enum HostRedisValueBody {
    Loading,
    Error(String),
    Unsupported(String),
    Empty,
    Rows { two_column: bool },
}

/// Read-only snapshot of the value dialog so rendering never borrows the
/// entity while the dialog closure holds it.
pub struct HostRedisValueDialogView {
    pub key: String,
    pub database: u32,
    pub loading: bool,
    pub json_view: bool,
    pub kind: Option<oxideterm_connection_monitor::ResourceRedisValueKind>,
    pub kind_label: Option<&'static str>,
    pub size_text: Option<String>,
    pub ttl: Option<oxideterm_connection_monitor::ResourceRedisTtl>,
    pub body: HostRedisValueBody,
    pub can_set_value: bool,
    pub json_available: bool,
    /// Inline rename state shown next to the key.
    pub renaming: bool,
    pub rename_error: Option<String>,
    /// Inline TTL editor opened from the metadata chip.
    pub ttl_editing: bool,
    pub ttl_error: Option<String>,
    pub size: (f32, f32),
    /// Width of the widest row; the body scrolls horizontally past the dialog.
    pub content_width: f32,
    /// Body horizontal band, so the scrollbar outlives one frame.
    pub h_scroll: gpui::ScrollHandle,
    /// Inline editor band, kept here instead of the app so it survives frames.
    pub editor_scroll: gpui::ScrollHandle,
    /// Rows the viewer cap left out; reported under the list so a long value
    /// never looks complete when only its head is drawn.
    pub omitted_rows: usize,
    pub rows: std::sync::Arc<Vec<HostRedisValueRow>>,
    pub list_state: gpui::ListState,
}

impl HostToolsEntity {
    pub(crate) fn request_redis_snapshot(
        &mut self,
        connection_id: String,
        runtime: HostToolsRuntime,
        connection_fallback: String,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) {
        // A probe must not run while the tool is switched off: a finishing read
        // or a focused password field would otherwise dispatch work the user
        // disabled.
        if !ContextSidebarTool::Redis.monitoring_enabled(&self.monitoring) {
            return;
        }
        let request = HostRedisSnapshotRequest {
            connection_id,
            connection_fallback,
            failure_fallback,
        };
        if self.host_redis.snapshot_in_flight {
            // Keep only the newest selection or refresh while the current
            // bounded probe completes.
            self.host_redis.snapshot_pending = Some(HostRedisSnapshotPending { request, runtime });
            return;
        }
        self.start_redis_snapshot(request, runtime, cx);
    }

    fn start_redis_snapshot(
        &mut self,
        request: HostRedisSnapshotRequest,
        runtime: HostToolsRuntime,
        cx: &mut Context<Self>,
    ) {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            self.host_redis.snapshot_connection_id = Some(request.connection_id);
            self.host_redis.snapshot = Some(ResourceRedisSnapshot {
                status: ResourceRedisStatus::Error {
                    message: request.connection_fallback,
                },
                endpoints: Vec::new(),
            });
            cx.notify();
            return;
        };
        let command = redis_discovery_command(&os_type).to_string();
        if self.host_redis.snapshot_connection_id.as_deref() != Some(request.connection_id.as_str())
        {
            self.host_redis.snapshot = None;
        }
        self.host_redis.snapshot_connection_id = Some(request.connection_id.clone());
        self.host_redis.snapshot_running = Some(request.clone());
        self.host_redis.snapshot_in_flight = true;
        let spawned = self.spawn_redis_snapshot_capture(
            command,
            request.clone(),
            HOST_REDIS_PROBE_TIMEOUT,
            HOST_REDIS_PROBE_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_redis.snapshot_running = None;
            self.host_redis.snapshot_in_flight = false;
            self.host_redis.snapshot = Some(ResourceRedisSnapshot {
                status: ResourceRedisStatus::Error {
                    message: request.connection_fallback,
                },
                endpoints: Vec::new(),
            });
        }
        cx.notify();
    }

    pub(crate) fn finish_host_redis_snapshot(
        &mut self,
        delivery: HostRedisSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_redis.snapshot_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_redis.snapshot_in_flight = false;
        self.host_redis.snapshot_running = None;
        let snapshot = match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_redis_snapshot(&output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                snapshot
            }
            Ok(mut output) => {
                // Probe failures use a localized safe fallback, never the
                // captured remote output.
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                ResourceRedisSnapshot {
                    status: ResourceRedisStatus::Error {
                        message: delivery.request.failure_fallback.clone(),
                    },
                    endpoints: Vec::new(),
                }
            }
            Err(()) => ResourceRedisSnapshot {
                status: ResourceRedisStatus::Error {
                    message: delivery.request.failure_fallback.clone(),
                },
                endpoints: Vec::new(),
            },
        };
        if self.selected_connection_id() == Some(delivery.request.connection_id.as_str())
            && self.active_tool() == ContextSidebarTool::Redis
        {
            self.host_redis.snapshot_connection_id = Some(delivery.request.connection_id.clone());
            self.host_redis.snapshot = Some(snapshot);
            self.sync_redis_endpoint_selection(
                &delivery.request.connection_id,
                delivery.request.failure_fallback,
                cx,
            );
        }
        let pending = self.host_redis.snapshot_pending.take();
        if let Some(pending) = pending {
            self.start_redis_snapshot(pending.request, pending.runtime, cx);
        }
        cx.notify();
    }

    /// Keeps the browsed endpoint inside the freshly discovered list, then reads
    /// it so the tree recovers after a refresh without extra clicks.
    fn sync_redis_endpoint_selection(
        &mut self,
        connection_id: &str,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) {
        let endpoints = self
            .host_redis
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.endpoints.clone())
            .unwrap_or_default();
        let selected_is_live = self
            .host_redis
            .keys_endpoint
            .as_ref()
            .is_some_and(|selected| endpoints.contains(selected));
        if !selected_is_live {
            self.ui.host_redis_password = zeroize::Zeroizing::default();
            self.host_redis.keys_endpoint = endpoints.first().cloned();
            self.clear_redis_keys_cache();
            self.host_redis.collapsed_databases.clear();
            self.host_redis.expanded_segments.clear();
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        self.request_redis_keys(connection_id.to_string(), runtime, failure_fallback, cx);
    }

    pub(crate) fn select_redis_endpoint(
        &mut self,
        endpoint: ResourceRedisEndpoint,
        cx: &mut Context<Self>,
    ) {
        let changed = self.host_redis.keys_endpoint.as_ref() != Some(&endpoint);
        self.host_redis.keys_endpoint = Some(endpoint);
        if changed {
            self.ui.host_redis_password = zeroize::Zeroizing::default();
            self.clear_redis_keys_cache();
            self.host_redis.collapsed_databases.clear();
            self.host_redis.expanded_segments.clear();
        }
        self.request_redis_keys_for_selected(cx);
    }

    /// Drops the browsed key list together with the identity it belongs to.
    /// The cache is only valid for the endpoint that produced it, so every
    /// path that moves the selection resets it here instead of leaving rows to
    /// render under a different endpoint.
    fn clear_redis_keys_cache(&mut self) {
        self.host_redis.keys = None;
        self.host_redis.keys_connection_id = None;
        self.host_redis.keys_cache_endpoint = None;
    }

    /// Connection a key read would address right now, mirroring
    /// [`Self::request_redis_keys_for_selected`].
    fn redis_keys_selection_connection(&self) -> Option<String> {
        self.selected_connection_id_owned()
            .or_else(|| self.host_redis.snapshot_connection_id.clone())
    }

    pub fn request_redis_keys_for_selected(&mut self, cx: &mut Context<Self>) {
        let Some(connection_id) = self
            .selected_connection_id_owned()
            .or_else(|| self.host_redis.snapshot_connection_id.clone())
        else {
            return;
        };
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        let Some(messages) = self.messages.clone() else {
            return;
        };
        self.request_redis_keys(connection_id, runtime, messages.redis_probe_failed, cx);
    }

    fn request_redis_keys(
        &mut self,
        connection_id: String,
        runtime: HostToolsRuntime,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) {
        if !ContextSidebarTool::Redis.monitoring_enabled(&self.monitoring) {
            return;
        }
        let Some(endpoint) = self.host_redis.keys_endpoint.clone() else {
            return;
        };
        let query = self.ui.host_redis_search_query.trim().to_string();
        let request = HostRedisKeysRequest {
            connection_id,
            endpoint,
            password: self.ui.host_redis_password.clone(),
            pattern: (!query.is_empty())
                .then(|| oxideterm_connection_monitor::redis_search_pattern(&query)),
            failure_fallback,
        };
        if self.host_redis.keys_in_flight {
            // Only the newest endpoint or password replaces the running read.
            self.host_redis.keys_pending = Some(HostRedisKeysPending { request, runtime });
            cx.notify();
            return;
        }
        self.start_redis_keys(request, runtime, cx);
    }

    fn start_redis_keys(
        &mut self,
        request: HostRedisKeysRequest,
        runtime: HostToolsRuntime,
        cx: &mut Context<Self>,
    ) {
        // A queued read may have outlived the selection that asked for it.
        let selection_connection = self.redis_keys_selection_connection();
        if request.is_stale_for(
            selection_connection.as_deref(),
            self.host_redis.keys_endpoint.as_ref(),
        ) {
            return;
        }
        let os_type = self.connection_os_type(&request.connection_id);
        let command = os_type
            .as_deref()
            .and_then(|os_type| redis_keys_command(&request.endpoint, os_type));
        let Some(command) = command else {
            // Unsupported host shells and rejected container names land here
            // without spawning a command.
            self.host_redis.keys_connection_id = Some(request.connection_id);
            self.host_redis.keys_cache_endpoint = Some(request.endpoint);
            self.host_redis.keys = Some(ResourceRedisKeys {
                status: ResourceRedisStatus::Error {
                    message: request.failure_fallback,
                },
                databases: Vec::new(),
            });
            cx.notify();
            return;
        };
        if self.host_redis.keys_cache_endpoint.as_ref() != Some(&request.endpoint) {
            self.clear_redis_keys_cache();
        }
        self.host_redis.keys_connection_id = Some(request.connection_id.clone());
        self.host_redis.keys_running = Some(request.clone());
        self.host_redis.keys_in_flight = true;
        let spawned = self.spawn_redis_keys_capture(
            command,
            request.clone(),
            HOST_REDIS_KEYS_TIMEOUT,
            HOST_REDIS_KEYS_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_redis.keys_running = None;
            self.host_redis.keys_in_flight = false;
            self.host_redis.keys_cache_endpoint = Some(request.endpoint);
            self.host_redis.keys = Some(ResourceRedisKeys {
                status: ResourceRedisStatus::Error {
                    message: request.failure_fallback,
                },
                databases: Vec::new(),
            });
        }
        cx.notify();
    }

    pub(crate) fn finish_host_redis_keys(
        &mut self,
        delivery: HostRedisKeysDelivery,
        cx: &mut Context<Self>,
    ) {
        let is_current = self
            .host_redis
            .keys_running
            .as_ref()
            .is_some_and(|running| running.same_target(&delivery.request));
        if !is_current {
            return;
        }
        self.host_redis.keys_in_flight = false;
        self.host_redis.keys_running = None;
        // The delivery belongs to the running read, but the user may have moved
        // to another host or endpoint while it was in flight. Installing it
        // then would show one endpoint's keys under another, and a delete from
        // that stale row would hit a different server.
        let selection_connection = self.redis_keys_selection_connection();
        // The scan pattern is part of the request identity: a result for the
        // previous search must not be installed under the new one.
        let current_pattern = {
            let query = self.ui.host_redis_search_query.trim();
            (!query.is_empty()).then(|| oxideterm_connection_monitor::redis_search_pattern(query))
        };
        let still_selected = delivery.request.pattern == current_pattern
            && !delivery.request.is_stale_for(
                selection_connection.as_deref(),
                self.host_redis.keys_endpoint.as_ref(),
            );
        if still_selected {
            let keys = match delivery.result {
                Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    parse_redis_keys(&text)
                }
                Ok(_) => ResourceRedisKeys {
                    status: ResourceRedisStatus::Error {
                        message: delivery.request.failure_fallback.clone(),
                    },
                    databases: Vec::new(),
                },
                Err(()) => ResourceRedisKeys {
                    status: ResourceRedisStatus::Error {
                        message: delivery.request.failure_fallback.clone(),
                    },
                    databases: Vec::new(),
                },
            };
            self.host_redis.keys = Some(keys);
            self.host_redis.keys_connection_id = Some(delivery.request.connection_id.clone());
            self.host_redis.keys_cache_endpoint = Some(delivery.request.endpoint.clone());
        }
        let pending = self.host_redis.keys_pending.take();
        if let Some(pending) = pending {
            // A queued read is only worth starting while it still addresses the
            // selection the user made after it was queued.
            self.start_redis_keys(pending.request, pending.runtime, cx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_redis_tree_row(
        &mut self,
        path: String,
        is_database: bool,
        cx: &mut Context<Self>,
    ) {
        let target = if is_database {
            &mut self.host_redis.collapsed_databases
        } else {
            &mut self.host_redis.expanded_segments
        };
        if !target.remove(&path) {
            target.insert(path);
        }
        cx.notify();
    }
}

struct HostRedisPanelSnapshot {
    connections: Vec<MonitorConnectionOption>,
    selected_connection_id: String,
    snapshot: Option<ResourceRedisSnapshot>,
    endpoints: Vec<ResourceRedisEndpoint>,
    selected_endpoint: Option<ResourceRedisEndpoint>,
    keys: Option<ResourceRedisKeys>,
    in_flight: bool,
}

struct HostRedisTreeRow {
    path: String,
    label: String,
    depth: usize,
    key_count: usize,
    expandable: bool,
    expanded: bool,
    is_database: bool,
    is_key: bool,
    /// Full key name for rows that address one key.
    key: Option<String>,
    /// Colon path of the tree node; deleting it also deletes its subtree.
    prefix: Option<String>,
    /// Database the row belongs to; keys are only unique within one database.
    database: u32,
    /// Synthetic leaf standing for a prefix that is itself a key.
    is_current_key: bool,
}

impl HostToolsEntity {
    fn redis_panel_snapshot(&self) -> Option<HostRedisPanelSnapshot> {
        let connections = self.monitor_connections();
        let fallback_connection_id = connections.first()?.connection_id.clone();
        let selected_connection_id = self
            .selected_connection_id_owned()
            .filter(|selected_id| {
                connections
                    .iter()
                    .any(|connection| connection.connection_id == *selected_id)
            })
            .unwrap_or(fallback_connection_id);
        let same_connection = self.host_redis.snapshot_connection_id.as_deref()
            == Some(selected_connection_id.as_str());
        let snapshot = same_connection
            .then(|| self.host_redis.snapshot.clone())
            .flatten();
        let endpoints = snapshot
            .as_ref()
            .map(|snapshot| snapshot.endpoints.clone())
            .unwrap_or_default();
        // The cached list is only meaningful for the endpoint that produced it:
        // rows from one endpoint must never render (or be deleted) under
        // another.
        let keys = (self.host_redis.keys_connection_id.as_deref()
            == Some(selected_connection_id.as_str())
            && self.host_redis.keys_cache_endpoint.is_some()
            && self.host_redis.keys_cache_endpoint == self.host_redis.keys_endpoint)
            .then(|| self.host_redis.keys.clone())
            .flatten();

        Some(HostRedisPanelSnapshot {
            connections,
            selected_connection_id,
            snapshot,
            endpoints,
            selected_endpoint: self.host_redis.keys_endpoint.clone(),
            keys,
            in_flight: self.host_redis.snapshot_in_flight || self.host_redis.keys_in_flight,
        })
    }

    fn redis_tree_rows(&self, databases: &[ResourceRedisDatabase]) -> Vec<HostRedisTreeRow> {
        host_redis_tree_rows(
            databases,
            self.ui.host_redis_show_all_databases,
            &self.host_redis.collapsed_databases,
            &self.host_redis.expanded_segments,
            self.ui.host_redis_search_query.trim(),
        )
    }

    pub(crate) fn toggle_redis_database_filter(&mut self, cx: &mut Context<Self>) {
        self.ui.host_redis_show_all_databases = !self.ui.host_redis_show_all_databases;
        cx.notify();
    }

    fn sync_redis_render_rows(&self, rows: &[HostRedisTreeRow], selected_connection_id: &str) {
        let signatures = rows
            .iter()
            .map(|row| {
                let mut hasher = std::hash::DefaultHasher::new();
                std::hash::Hash::hash(&row.path, &mut hasher);
                std::hash::Hash::hash(&row.expanded, &mut hasher);
                std::hash::Hash::hash(&row.key_count, &mut hasher);
                std::hash::Hash::hash(&row.key, &mut hasher);
                std::hash::Hasher::finish(&hasher)
            })
            .collect::<Vec<_>>();
        let identity = format!("host-redis:{selected_connection_id}");
        sync_tauri_variable_list_state_by_signatures(
            &self.host_redis.list_state,
            &mut self.host_redis.list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_REDIS_TREE_ROW_HEIGHT), 8),
        );
    }

    pub fn render_host_redis_panel(
        &self,
        password_ime: HostToolsPlainTextImeFrame,
        search_ime: HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.redis_panel_snapshot() else {
            return host_tools_center_state(
                "lucide/wifi-off.svg",
                tokens.ui.text_muted,
                i18n.t("profiler.panel.no_connection"),
                selectable_text,
                cx,
            );
        };
        let selected_connection_id = snapshot.selected_connection_id.clone();
        let rows = std::sync::Arc::new(
            snapshot
                .keys
                .as_ref()
                .map(|keys| self.redis_tree_rows(&keys.databases))
                .unwrap_or_default(),
        );
        self.sync_redis_render_rows(&rows, &selected_connection_id);
        let body = self.render_host_redis_body(
            snapshot.keys.as_ref(),
            rows.clone(),
            snapshot
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.status.clone()),
            snapshot.endpoints.is_empty(),
            snapshot.in_flight,
            tokens,
            i18n,
            mono_font_family.clone(),
            selectable_text,
            cx,
        );

        div()
            .id("host-redis-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &snapshot.connections,
                        &selected_connection_id,
                        snapshot.snapshot.is_some(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_redis_endpoints(
                        &snapshot.endpoints,
                        snapshot.selected_endpoint.as_ref(),
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(self.render_host_redis_password(&password_ime, tokens, i18n, cx))
                    .child(self.render_host_redis_status_row(
                        &search_ime,
                        snapshot.in_flight,
                        self.ui.host_redis_show_all_databases,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(body),
            )
            .into_any_element()
    }

    fn render_host_redis_endpoints(
        &self,
        endpoints: &[ResourceRedisEndpoint],
        selected: Option<&ResourceRedisEndpoint>,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if endpoints.is_empty() {
            return div().into_any_element();
        }
        let theme = tokens.ui;
        let mut row = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .gap_1()
            .children(endpoints.iter().map(|endpoint| {
                let active = selected == Some(endpoint);
                let endpoint = endpoint.clone();
                let icon = match endpoint.kind {
                    ResourceRedisEndpointKind::Podman | ResourceRedisEndpointKind::Docker => {
                        "lucide/circle.svg"
                    }
                    ResourceRedisEndpointKind::Host => "lucide/server.svg",
                };
                let label = match endpoint.kind {
                    ResourceRedisEndpointKind::Host => endpoint.target.clone(),
                    ResourceRedisEndpointKind::Docker => format!("Docker · {}", endpoint.target),
                    ResourceRedisEndpointKind::Podman => format!("Podman · {}", endpoint.target),
                };
                div()
                    .id(host_tools_row_id(
                        "host-redis-endpoint",
                        &endpoint.target,
                        label.clone(),
                    ))
                    .h(px(HOST_REDIS_ENDPOINT_CHIP_HEIGHT))
                    .px_2()
                    .min_w_0()
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(tokens.radii.md))
                    .border_1()
                    .border_color(rgba(if active {
                        (theme.accent << 8) | MONITOR_BORDER_ALPHA
                    } else {
                        (theme.border << 8) | MONITOR_BORDER_ALPHA
                    }))
                    .when(active, |chip| {
                        chip.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA))
                    })
                    .cursor_pointer()
                    .hover(|chip| chip.bg(rgb(theme.bg_hover)))
                    .child(render_lucide_icon(
                        icon,
                        12.0,
                        rgb(if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                            .child(label),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools.select_redis_endpoint(endpoint.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
            }));
        row = row.child(
            div()
                .h(px(HOST_REDIS_ENDPOINT_CHIP_HEIGHT))
                .flex()
                .items_center()
                .px_1()
                .text_size(px(10.0))
                .text_color(rgb(theme.text_muted))
                .child(i18n.t("sidebar.host_redis.endpoint_hint")),
        );
        row.into_any_element()
    }

    fn render_host_redis_password(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        text_input_anchor_probe(
            ime.anchor_id(),
            ime.text_input(
                tokens,
                TextInputView {
                    value: self.ui.host_redis_password.as_str(),
                    placeholder: i18n.t("sidebar.host_redis.password_placeholder"),
                    focused: self.ui.input_is_focused(input),
                    caret_visible: ime.caret_visible(),
                    // A Redis password is a credential: only the mask is painted.
                    secret: true,
                    selected_all: false,
                    selected_range: ime.selected_range(),
                    marked_text: ime.marked_text(),
                },
            )
            .h(px(30.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, event: &MouseDownEvent, window, cx| {
                    host_tools.ui.focus_input(input);
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::BeginPlainTextImeSelection {
                                input,
                                event: event.clone(),
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }

    fn render_host_redis_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        text_input_anchor_probe(
            ime.anchor_id(),
            ime.text_input(
                tokens,
                TextInputView {
                    value: self.ui.host_redis_search_query.as_str(),
                    placeholder: i18n.t("sidebar.host_redis.search_placeholder"),
                    focused: self.ui.input_is_focused(input),
                    caret_visible: ime.caret_visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: ime.selected_range(),
                    marked_text: ime.marked_text(),
                },
            )
            .h(px(28.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, event: &MouseDownEvent, window, cx| {
                    host_tools.ui.focus_input(input);
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::BeginPlainTextImeSelection {
                                input,
                                event: event.clone(),
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }

    fn render_host_redis_status_row(
        &self,
        search_ime: &HostToolsPlainTextImeFrame,
        in_flight: bool,
        show_all_databases: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(self.render_host_redis_search(search_ime, tokens, i18n, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/list-tree.svg",
                        13.0,
                        if show_all_databases {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text)
                        },
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            has_background: true,
                            background: Some(rgb(if show_all_databases {
                                theme.bg_panel
                            } else {
                                theme.bg_hover
                            })),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        if show_all_databases {
                            i18n.t("sidebar.host_redis.filter_only_with_keys")
                        } else {
                            i18n.t("sidebar.host_redis.filter_all_databases")
                        },
                        "host-redis-database-filter",
                        true,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools.toggle_redis_database_filter(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/refresh-cw.svg",
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: in_flight,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_redis.actions.refresh"),
                        "host-redis-refresh",
                        true,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools
                                .request_active_tool_snapshot(HostSnapshotFeedback::Toast, cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_redis_body(
        &self,
        keys: Option<&ResourceRedisKeys>,
        rows: std::sync::Arc<Vec<HostRedisTreeRow>>,
        discovery_status: Option<ResourceRedisStatus>,
        has_endpoints: bool,
        in_flight: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(keys) = keys {
            match &keys.status {
                ResourceRedisStatus::Available if !rows.is_empty() => {
                    return self.render_host_redis_tree(rows, tokens, i18n, mono_font_family, cx);
                }
                ResourceRedisStatus::Available => {
                    return host_tools_center_state(
                        "lucide/circle.svg",
                        tokens.ui.text_muted,
                        i18n.t("sidebar.host_redis.keys_empty"),
                        selectable_text,
                        cx,
                    );
                }
                status if !in_flight => {
                    return self.render_host_redis_state(status, tokens, i18n, selectable_text, cx);
                }
                _ => {}
            }
        }
        if in_flight {
            return host_tools_center_state(
                "lucide/circle.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.loading"),
                selectable_text,
                cx,
            );
        }
        if discovery_status == Some(ResourceRedisStatus::Available) && !has_endpoints {
            // Only the availability probe ran: this host shell cannot list
            // endpoints or browse keys.
            return host_tools_center_state(
                "lucide/lock.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.browse_unsupported"),
                selectable_text,
                cx,
            );
        }
        self.render_host_redis_state(
            &discovery_status.unwrap_or_default(),
            tokens,
            i18n,
            selectable_text,
            cx,
        )
    }

    fn render_host_redis_state(
        &self,
        status: &ResourceRedisStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match status {
            ResourceRedisStatus::Available => host_tools_center_state(
                "lucide/check-circle.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.keys_empty"),
                selectable_text,
                cx,
            ),
            ResourceRedisStatus::Unavailable => host_tools_center_state(
                "lucide/circle.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.unavailable"),
                selectable_text,
                cx,
            ),
            ResourceRedisStatus::NoServer => host_tools_center_state(
                "lucide/circle.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.no_endpoint"),
                selectable_text,
                cx,
            ),
            ResourceRedisStatus::AuthRequired => host_tools_center_state(
                "lucide/lock.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.auth_required"),
                selectable_text,
                cx,
            ),
            ResourceRedisStatus::Error { message } => host_tools_center_state(
                "lucide/alert-triangle.svg",
                MONITOR_RED,
                i18n.t("sidebar.host_redis.error")
                    .replace("{{error}}", message),
                selectable_text,
                cx,
            ),
            ResourceRedisStatus::Unknown => host_tools_center_state(
                "lucide/circle.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_redis.loading"),
                selectable_text,
                cx,
            ),
        }
    }

    fn render_host_redis_tree(
        &self,
        rows: std::sync::Arc<Vec<HostRedisTreeRow>>,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.host_redis.list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_REDIS_TREE_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let tokens = *tokens;
        let i18n = i18n.clone();
        div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    let rows = rows.clone();
                    let tokens = tokens;
                    let i18n = i18n.clone();
                    let mono_font_family = mono_font_family.clone();
                    host_tools.update(cx, |host_tools, cx| {
                        host_tools.render_host_redis_row(
                            index,
                            rows.as_ref(),
                            &tokens,
                            &i18n,
                            mono_font_family,
                            cx,
                        )
                    })
                },
            ))
            .into_any_element()
    }

    fn render_host_redis_row(
        &self,
        index: usize,
        rows: &[HostRedisTreeRow],
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = rows.get(index) else {
            return div().into_any_element();
        };
        let theme = tokens.ui;
        let indent = 12.0 + row.depth as f32 * 14.0;
        let icon = if row.is_database {
            "lucide/circle.svg"
        } else if row.is_key {
            "lucide/key.svg"
        } else {
            "lucide/folder.svg"
        };
        let chevron = if !row.expandable {
            None
        } else if row.expanded {
            Some("lucide/chevron-down.svg")
        } else {
            Some("lucide/chevron-right.svg")
        };
        let path = row.path.clone();
        let is_database = row.is_database;
        let row_key = row.key.clone();
        let row_database = row.database;
        let count_label = i18n
            .t("sidebar.host_redis.key_count")
            .replace("{{count}}", &row.key_count.to_string());

        let mut element = div()
            .id(host_tools_row_id("host-redis-row", &row.path, path.clone()))
            .w_full()
            .min_w_0()
            .h(px(HOST_REDIS_TREE_ROW_HEIGHT))
            .pl(px(indent))
            .pr_3()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .when(row.expandable || row.is_key, |row| row.cursor_pointer())
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .when_some(chevron, |element, chevron| {
                let toggle_path = path.clone();
                element.child(
                    div()
                        .id(host_tools_row_id(
                            "host-redis-chevron",
                            &toggle_path,
                            toggle_path.clone(),
                        ))
                        .flex_none()
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .child(render_lucide_icon(chevron, 12.0, rgb(theme.text_muted)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |host_tools, _event, _window, cx| {
                                host_tools.toggle_redis_tree_row(
                                    toggle_path.clone(),
                                    is_database,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .when(chevron.is_none(), |element| {
                element.child(div().w(px(12.0)))
            })
            .child(render_lucide_icon(
                icon,
                12.0,
                rgb(if row.is_key {
                    theme.text
                } else {
                    theme.text_muted
                }),
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(11.0))
                    .font_family(mono_font_family)
                    .when(row.is_database, |label| {
                        label.font_weight(gpui::FontWeight::SEMIBOLD)
                    })
                    .text_color(rgb(theme.text))
                    .child(if row.is_current_key {
                        i18n.t("sidebar.host_redis.tree_current")
                    } else {
                        row.label.clone()
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(count_label),
            );

        if row.is_database {
            let create_database = row.database;
            element = element.child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/plus.svg",
                13.0,
                rgb(theme.text_muted),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    has_background: false,
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_redis.actions.new_key"),
                "host-redis-create-key",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.open_redis_create_key(create_database, cx);
                    cx.stop_propagation();
                }),
            ));
        }

        if let Some(key) = row_key {
            let copy_key = key.clone();
            let copy_value = key.clone();
            let delete_key = key.clone();
            element = element
                .child(host_tools_tooltip_icon_button(
                    tokens,
                    "lucide/copy.svg",
                    12.0,
                    rgb(theme.text_muted),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 20.0,
                        has_background: false,
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                    },
                    i18n.t("sidebar.host_redis.actions.copy_key"),
                    "host-redis-copy-key",
                    true,
                    cx.listener(move |host_tools, _event, _window, cx| {
                        host_tools.copy_redis_key(copy_key.clone(), cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(host_tools_tooltip_icon_button(
                    tokens,
                    "lucide/file-text.svg",
                    12.0,
                    rgb(theme.text_muted),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 20.0,
                        has_background: false,
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                    },
                    i18n.t("sidebar.host_redis.actions.copy_value"),
                    "host-redis-copy-value",
                    true,
                    cx.listener(move |host_tools, _event, _window, cx| {
                        host_tools.copy_redis_value(row_database, copy_value.clone(), cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(host_tools_tooltip_icon_button(
                    tokens,
                    "lucide/trash-2.svg",
                    12.0,
                    rgb(MONITOR_RED),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 20.0,
                        has_background: false,
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                    },
                    i18n.t("sidebar.host_redis.actions.delete"),
                    "host-redis-delete-key",
                    true,
                    cx.listener(move |host_tools, _event, _window, cx| {
                        host_tools.request_redis_delete(row_database, delete_key.clone(), cx);
                        cx.stop_propagation();
                    }),
                ));
        }

        if row.key_count > 1
            && !row.is_database
            && let Some(prefix) = row.prefix.clone()
        {
            let delete_prefix = prefix.clone();
            element = element.child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/trash-2.svg",
                12.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    has_background: false,
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_redis.actions.delete_prefix"),
                "host-redis-delete-prefix",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.request_redis_delete_prefix(row_database, delete_prefix.clone(), cx);
                    cx.stop_propagation();
                }),
            ));
        }

        // A key row opens the value viewer; prefix rows only expand.
        if let Some(key) = row.key.clone() {
            element = element.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.open_redis_value(row_database, key.clone(), cx);
                    cx.stop_propagation();
                }),
            );
        } else if row.expandable {
            element = element.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.toggle_redis_tree_row(path.clone(), is_database, cx);
                    cx.stop_propagation();
                }),
            );
        }

        element.into_any_element()
    }
}

impl HostToolsEntity {
    /// Value payload for the WorkspaceApp renderer. Strings are prepared once
    /// per frame here so the dialog never touches the entity while rendering.
    pub fn redis_value_dialog_view(
        &self,
        viewport: (f32, f32),
    ) -> Option<HostRedisValueDialogView> {
        let dialog = self.host_redis.value_dialog.as_ref()?;
        let value = dialog.value.as_ref();
        let kind_label = value.map(|value| match value.kind {
            ResourceRedisValueKind::String => "STRING",
            ResourceRedisValueKind::Json => "JSON",
            ResourceRedisValueKind::Hash => "HASH",
            ResourceRedisValueKind::List => "LIST",
            ResourceRedisValueKind::Set => "SET",
            ResourceRedisValueKind::SortedSet => "ZSET",
            ResourceRedisValueKind::Stream => "STREAM",
            ResourceRedisValueKind::Other => "OTHER",
        });
        let size_text = value.map(|value| {
            if value.kind == ResourceRedisValueKind::String
                && let Some(len) = value.length
                && len > value.elements.first().map_or(0, Vec::len) as u64
            {
                return format!("{} / {} B", value.elements.first().map_or(0, Vec::len), len);
            }
            value
                .memory_usage
                .map(format_bytes)
                .or_else(|| value.length.map(|length| length.to_string()))
                .unwrap_or_else(|| "-".to_string())
        });
        let ttl = value.map(|value| value.ttl);
        let kind = value.map(|value| value.kind);
        // RedisJSON only has the JSON rendering, so the text view toggle would
        // show a second, misleading projection of the same value.
        let json_available = value.is_some_and(|value| {
            redis_value_json(value).is_some()
                && value.kind != oxideterm_connection_monitor::ResourceRedisValueKind::Json
        });
        let can_set_value = value.is_some_and(|value| {
            redis_value_copy_is_complete(value)
                && RedisValueWriteKind::from_value_kind(value.kind).is_some()
        });

        let mut two_column = false;
        let mut omitted_rows = 0usize;
        let (body, rows) = if dialog.loading {
            (HostRedisValueBody::Loading, Vec::new())
        } else if let Some(error) = dialog.error.clone() {
            (HostRedisValueBody::Error(error), Vec::new())
        } else if let Some(value) = value {
            let unsupported = matches!(value.kind, ResourceRedisValueKind::Other);
            if unsupported {
                (
                    HostRedisValueBody::Unsupported(
                        value
                            .raw_type
                            .clone()
                            .unwrap_or_else(|| value.kind.label().to_string()),
                    ),
                    Vec::new(),
                )
            } else if !dialog.json_view && !redis_value_rows(value).is_empty() {
                two_column = matches!(
                    value.kind,
                    ResourceRedisValueKind::Hash
                        | ResourceRedisValueKind::SortedSet
                        | ResourceRedisValueKind::Stream
                );
                let all_rows = redis_value_rows(value);
                // The viewer draws a bounded band per frame; the rest is
                // reported instead of silently disappearing.
                omitted_rows = value
                    .length
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(all_rows.len())
                    .saturating_sub(all_rows.len().min(HOST_REDIS_VALUE_MAX_LINES));
                let rows = all_rows
                    .into_iter()
                    .take(HOST_REDIS_VALUE_MAX_LINES)
                    .map(|(left, right)| HostRedisValueRow { left, right })
                    .collect::<Vec<_>>();
                (HostRedisValueBody::Rows { two_column }, rows)
            } else {
                let text = if dialog.json_view {
                    redis_value_json(value).unwrap_or_default()
                } else {
                    redis_value_text(value)
                };
                if text.is_empty() {
                    (HostRedisValueBody::Empty, Vec::new())
                } else {
                    omitted_rows = text
                        .lines()
                        .count()
                        .saturating_sub(HOST_REDIS_VALUE_MAX_LINES);
                    let rows = text
                        .lines()
                        .take(HOST_REDIS_VALUE_MAX_LINES)
                        .map(|line| HostRedisValueRow {
                            left: if line.is_empty() {
                                " ".to_string()
                            } else {
                                line.to_string()
                            },
                            right: None,
                        })
                        .collect::<Vec<_>>();
                    (HostRedisValueBody::Rows { two_column }, rows)
                }
            }
        } else {
            (HostRedisValueBody::Loading, Vec::new())
        };

        let identity = format!(
            "{}\u{1}{}\u{1}{}",
            dialog.key, dialog.database, dialog.json_view
        );
        let content_width = redis_value_content_width(&rows, two_column);
        let signatures = rows
            .iter()
            .map(|row| {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                row.left.hash(&mut hasher);
                row.right.hash(&mut hasher);
                hasher.finish()
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.host_redis.value_dialog_list,
            &mut self.host_redis.value_dialog_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(
                px(HOST_REDIS_VALUE_ROW_HEIGHT),
                HOST_REDIS_VALUE_ROW_OVERSCAN,
            ),
        );

        Some(HostRedisValueDialogView {
            key: dialog.key.clone(),
            database: dialog.database,
            loading: dialog.loading,
            json_view: dialog.json_view,
            kind,
            kind_label,
            size_text,
            ttl,
            body,
            can_set_value,
            json_available,
            renaming: self.ui.host_redis_rename_value.is_some(),
            rename_error: self.ui.host_redis_rename_error.clone(),
            ttl_editing: self.ui.host_redis_ttl_value.is_some(),
            ttl_error: self.ui.host_redis_ttl_error.clone(),
            size: clamped_redis_value_dialog_size(self.host_redis.value_dialog_size, viewport),
            content_width,
            h_scroll: self.host_redis.value_dialog_h_scroll.clone(),
            editor_scroll: self.host_redis.value_dialog_editor_scroll.clone(),
            omitted_rows,
            rows: Arc::new(rows),
            list_state: self.host_redis.value_dialog_list.clone(),
        })
    }

    pub fn update_redis_value_dialog_resize(
        &mut self,
        event: &MouseMoveEvent,
        viewport: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.host_redis.value_dialog_resize else {
            return;
        };
        let next = resized_redis_value_dialog_size(
            (drag.start_width, drag.start_height),
            (
                f32::from(event.position.x - drag.start_cursor.x),
                f32::from(event.position.y - drag.start_cursor.y),
            ),
            viewport,
        );
        if self.host_redis.value_dialog_size != next {
            self.host_redis.value_dialog_size = next;
            cx.notify();
        }
    }

    pub fn finish_redis_value_dialog_resize(&mut self, cx: &mut Context<Self>) {
        if self.host_redis.value_dialog_resize.take().is_some() {
            cx.notify();
        }
    }

    pub fn start_redis_value_dialog_resize(
        &mut self,
        event: &MouseDownEvent,
        viewport: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        if event.click_count >= 2 {
            self.host_redis.value_dialog_size = clamped_redis_value_dialog_size(
                (
                    HOST_REDIS_VALUE_DIALOG_WIDTH,
                    HOST_REDIS_VALUE_DIALOG_DEFAULT_HEIGHT,
                ),
                viewport,
            );
            self.host_redis.value_dialog_resize = None;
            cx.notify();
            return;
        }
        // A window shrink leaves a larger stored size behind; drag from the
        // clamped size so the handle never jumps on the first move.
        self.host_redis.value_dialog_size =
            clamped_redis_value_dialog_size(self.host_redis.value_dialog_size, viewport);
        let (start_width, start_height) = self.host_redis.value_dialog_size;
        self.host_redis.value_dialog_resize = Some(HostToolsDialogResizeDrag {
            start_cursor: event.position,
            start_width,
            start_height,
        });
        cx.notify();
    }

    pub fn render_host_redis_delete_dialog(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let confirm = self.host_redis.delete_confirm.as_ref()?;
        let key = confirm.key.clone();
        let keys = confirm.keys.clone();
        // The tree only holds the keys the capped browse returned. When the
        // database has more keys than were loaded, the prefix match set may be
        // partial, and the confirmation has to say so instead of implying it
        // removes everything under the prefix.
        let loaded_only = self
            .host_redis
            .keys
            .as_ref()
            .and_then(|keys| {
                keys.databases
                    .iter()
                    .find(|database| database.index == confirm.database)
            })
            .filter(|database| database.key_count > database.keys.len() as u64)
            .map(|database| database.key_count);
        let description: AnyElement = if keys.len() > 1 {
            let heading = i18n
                .t("sidebar.host_redis.delete.prefix_description")
                .replace("{{prefix}}", &key)
                .replace("{{count}}", &keys.len().to_string());
            let visible = keys.iter().take(200);
            let mut list = div().flex().flex_col().gap(px(1.0));
            for (index, name) in visible.enumerate() {
                list = list.child(
                    div()
                        .id(("host-redis-delete-key", index))
                        .flex_none()
                        .truncate()
                        .child(name.clone()),
                );
            }
            if keys.len() > 200 {
                list = list.child(
                    div()
                        .flex_none()
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(
                            i18n.t("sidebar.host_redis.delete.more")
                                .replace("{{count}}", &(keys.len() - 200).to_string()),
                        ),
                );
            }
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(heading)
                .when_some(loaded_only, |body, total| {
                    body.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(
                                i18n.t("sidebar.host_redis.delete.loaded_only")
                                    .replace("{{loaded}}", &keys.len().to_string())
                                    .replace("{{total}}", &total.to_string()),
                            ),
                    )
                })
                .child(
                    div()
                        .id("host-redis-delete-keys")
                        .max_h(px(180.0))
                        .overflow_y_scroll()
                        .text_size(px(11.0))
                        .child(list),
                )
                .into_any_element()
        } else {
            div()
                .child(
                    i18n.t("sidebar.host_redis.delete.description")
                        .replace("{{key}}", &key),
                )
                .into_any_element()
        };
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-redis-delete-confirm",
                confirm.presence.phase(),
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Danger,
                    title: div()
                        .child(i18n.t("sidebar.host_redis.delete.title"))
                        .into_any_element(),
                    description: Some(description),
                    cancel_label: div()
                        .child(i18n.t("sidebar.host_redis.delete.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(i18n.t("sidebar.host_redis.delete.confirm"))
                        .into_any_element(),
                },
                focused_action,
                cx.listener(|host_tools, _event, _window, cx| {
                    host_tools.dismiss_redis_delete(cx);
                }),
                cx.listener(|host_tools, _event, _window, cx| {
                    host_tools.confirm_redis_delete(cx);
                }),
            )
            .into_any_element(),
        )
    }
}

/// Expansion paths address nodes by index instead of by label, so a key segment
/// that itself contains a separator cannot collide with a sibling level.
/// Flattens databases and their key trees into render rows.
///
/// Databases start expanded and deeper segments stay collapsed, so browsing a
/// large database shows its first-level prefixes instead of every key.
fn host_redis_tree_rows(
    databases: &[ResourceRedisDatabase],
    show_empty_databases: bool,
    collapsed_databases: &std::collections::BTreeSet<String>,
    expanded_segments: &std::collections::BTreeSet<String>,
    query: &str,
) -> Vec<HostRedisTreeRow> {
    let query = query.to_lowercase();
    let mut rows = Vec::new();
    for database in databases {
        // The search narrows the loaded keys; a database with no match drops
        // out entirely instead of showing an empty tree.
        let matched: Vec<String> = if query.is_empty() {
            database.keys.clone()
        } else {
            database
                .keys
                .iter()
                .filter(|key| key.to_lowercase().contains(&query))
                .cloned()
                .collect()
        };
        if !query.is_empty() && matched.is_empty() {
            continue;
        }
        if !show_empty_databases && database.key_count == 0 && matched.is_empty() {
            continue;
        }
        let path = format!("db:{}", database.index);
        let expanded = !collapsed_databases.contains(&path);
        rows.push(HostRedisTreeRow {
            path,
            label: format!("db{}", database.index),
            depth: 0,
            key_count: if query.is_empty() {
                usize::try_from(database.key_count).unwrap_or(usize::MAX)
            } else {
                matched.len()
            },
            expandable: !matched.is_empty(),
            expanded,
            is_database: true,
            is_key: false,
            key: None,
            prefix: None,
            database: database.index,
            is_current_key: false,
        });
        if expanded {
            let tree = redis_key_tree(&matched);
            push_redis_tree_rows(
                &tree,
                &format!("db:{}:", database.index),
                database.index,
                1,
                expanded_segments,
                &mut rows,
            );
        }
    }
    rows
}

impl HostToolsEntity {
    fn redis_target_selected(&self, host: &str, endpoint: &ResourceRedisEndpoint) -> bool {
        self.selected_connection_id() == Some(host)
            && self.host_redis.keys_endpoint.as_ref() == Some(endpoint)
    }

    fn redis_value_target(&self) -> Option<(String, ResourceRedisEndpoint)> {
        let connection_id = self
            .selected_connection_id_owned()
            .or_else(|| self.host_redis.snapshot_connection_id.clone())?;
        let endpoint = self.host_redis.keys_endpoint.clone()?;
        Some((connection_id, endpoint))
    }

    pub(crate) fn open_redis_value(&mut self, database: u32, key: String, cx: &mut Context<Self>) {
        let Some((connection_id, endpoint)) = self.redis_value_target() else {
            return;
        };
        self.host_redis.value_dialog = Some(HostRedisValueDialog {
            connection_id: connection_id.clone(),
            endpoint: endpoint.clone(),
            database,
            key: key.clone(),
            value: None,
            loading: true,
            error: None,
            json_view: false,
        });
        // A scalar editor left over from a previous key must not render under
        // the new key's title.
        if self
            .ui
            .host_redis_set_value_dialog
            .as_ref()
            .is_some_and(|editor| editor.key != key || editor.database != database)
        {
            self.ui.host_redis_set_value_dialog = None;
        }
        self.ui.host_redis_rename_value = None;
        self.ui.host_redis_rename_error = None;
        self.ui.host_redis_ttl_value = None;
        self.ui.host_redis_ttl_error = None;
        self.request_redis_value(
            connection_id,
            endpoint,
            database,
            key,
            HostRedisValuePurpose::Open,
            cx,
        );
    }

    pub fn refresh_redis_value(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.host_redis.value_dialog.as_mut() else {
            return;
        };
        dialog.loading = true;
        dialog.error = None;
        // Refresh reads the endpoint the dialog was opened on, not whatever the
        // panel selects now.
        let connection_id = dialog.connection_id.clone();
        let endpoint = dialog.endpoint.clone();
        let database = dialog.database;
        let key = dialog.key.clone();
        self.request_redis_value(
            connection_id,
            endpoint,
            database,
            key,
            HostRedisValuePurpose::Open,
            cx,
        );
    }

    pub fn redis_value_dialog_open(&self) -> bool {
        self.host_redis.value_dialog.is_some()
    }

    pub fn redis_delete_open(&self) -> bool {
        self.host_redis.delete_confirm.is_some()
    }

    pub fn redis_rename_open(&self) -> bool {
        self.ui.host_redis_rename_value.is_some()
    }

    pub fn redis_ttl_edit_open(&self) -> bool {
        self.ui.host_redis_ttl_value.is_some()
    }

    pub fn dismiss_redis_value(&mut self, cx: &mut Context<Self>) {
        if self.host_redis.value_dialog.take().is_some() {
            self.ui.host_redis_set_value_dialog = None;
            self.ui.host_redis_rename_value = None;
            self.ui.host_redis_rename_error = None;
            self.ui.host_redis_ttl_value = None;
            self.ui.host_redis_ttl_error = None;
            self.ui.clear_input_focus();
            cx.notify();
        }
    }

    pub fn set_redis_value_view(&mut self, json_view: bool, cx: &mut Context<Self>) {
        let snapshot = self.host_redis.value_dialog.as_ref().and_then(|dialog| {
            let value = dialog.value.as_ref()?;
            Some((
                dialog.connection_id.clone(),
                dialog.endpoint.clone(),
                dialog.database,
                dialog.key.clone(),
                json_view,
                value.clone(),
            ))
        });
        let Some(snapshot) = snapshot else {
            return;
        };
        let (connection_id, endpoint, database, key, _was_json_view, value) = snapshot;
        // The editor draft is only rewritten when the stored value projection
        // changed under it; see `resync_redis_string_editor`.
        self.resync_redis_string_editor(
            &connection_id,
            &endpoint,
            database,
            &key,
            json_view,
            &value,
            cx,
        );
        cx.notify();
    }

    /// Re-prepares the inline editor after a view switch, but only when the
    /// draft was untouched: a formatted or raw render of the same value is
    /// replaced, a user edit is kept.
    fn resync_redis_string_editor(
        &mut self,
        connection_id: &str,
        endpoint: &oxideterm_connection_monitor::ResourceRedisEndpoint,
        database: u32,
        key: &str,
        json_view: bool,
        value: &oxideterm_connection_monitor::ResourceRedisValue,
        cx: &mut Context<Self>,
    ) {
        if value.kind != oxideterm_connection_monitor::ResourceRedisValueKind::String {
            return;
        }
        let Some(editor) = self.ui.host_redis_set_value_dialog.as_mut() else {
            return;
        };
        if editor.kind != RedisValueWriteKind::String
            || editor.connection_id != connection_id
            || editor.endpoint != *endpoint
            || editor.database != database
            || editor.key != key
        {
            return;
        }
        let current = editor.value.to_string();
        let raw = value.body.clone();
        let pretty = redis_value_json(value).unwrap_or_else(|| raw.clone());
        // Either projection counts as untouched; anything else is a draft.
        if current != raw && current != pretty {
            return;
        }
        let next = if json_view { pretty } else { raw };
        editor.value = zeroize::Zeroizing::new(next);
        cx.notify();
    }

    pub fn redis_create_key_open(&self) -> bool {
        self.ui.host_redis_create_key_dialog.is_some()
    }

    pub(crate) fn open_redis_create_key(&mut self, database: u32, cx: &mut Context<Self>) {
        let Some((connection_id, endpoint)) = self.redis_value_target() else {
            return;
        };
        self.ui.host_redis_create_key_dialog = Some(HostRedisCreateKeyDialog {
            connection_id,
            endpoint,
            database,
            name: zeroize::Zeroizing::new(String::new()),
            kind: RedisCreateKind::String,
            ttl_never: true,
            ttl: zeroize::Zeroizing::new(String::new()),
            value: zeroize::Zeroizing::new(String::new()),
            field: zeroize::Zeroizing::new(String::new()),
            entries: Vec::new(),
            error: None,
            loading: false,
        });
        self.ui.focus_input(HostToolsTextInput::RedisCreateKeyName);
        cx.notify();
    }

    pub fn dismiss_redis_create_key(&mut self, cx: &mut Context<Self>) {
        if self.ui.host_redis_create_key_dialog.take().is_some() {
            self.ui.clear_input_focus();
            cx.notify();
        }
    }

    pub fn set_redis_create_kind(&mut self, kind: RedisCreateKind, cx: &mut Context<Self>) {
        let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() else {
            return;
        };
        if dialog.kind == kind {
            return;
        }
        dialog.kind = kind;
        // Entries and pending inputs have a different shape per type.
        dialog.entries.clear();
        dialog.field = zeroize::Zeroizing::new(String::new());
        dialog.value = zeroize::Zeroizing::new(String::new());
        dialog.error = None;
        cx.notify();
    }

    pub fn set_redis_create_ttl_never(&mut self, never: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut()
            && dialog.ttl_never != never
        {
            dialog.ttl_never = never;
            dialog.error = None;
            cx.notify();
        }
    }

    pub fn add_redis_create_entry(&mut self, cx: &mut Context<Self>) {
        let messages = self.messages.clone();
        let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() else {
            return;
        };
        let entry = match dialog.kind {
            RedisCreateKind::Hash => {
                let field = dialog.field.trim().to_string();
                if field.is_empty() || field.contains(['\t', '\n']) {
                    dialog.error = messages
                        .as_ref()
                        .map(|messages| messages.redis_create_key_invalid_field.clone());
                    cx.notify();
                    return;
                }
                let value = dialog.value.to_string();
                dialog.field = zeroize::Zeroizing::new(String::new());
                dialog.value = zeroize::Zeroizing::new(String::new());
                format!("{field}\t{value}")
            }
            RedisCreateKind::SortedSet => {
                let member = dialog.field.trim().to_string();
                if member.is_empty() || member.contains(['\t', '\n']) {
                    dialog.error = messages
                        .as_ref()
                        .map(|messages| messages.redis_create_key_invalid_field.clone());
                    cx.notify();
                    return;
                }
                let score = dialog.value.trim();
                let score = if score.is_empty() {
                    0.0
                } else {
                    match score.parse::<f64>() {
                        Ok(score) if score.is_finite() => score,
                        _ => {
                            dialog.error = messages
                                .as_ref()
                                .map(|messages| messages.redis_create_key_invalid_score.clone());
                            cx.notify();
                            return;
                        }
                    }
                };
                dialog.field = zeroize::Zeroizing::new(String::new());
                dialog.value = zeroize::Zeroizing::new(String::new());
                format!("{member}\t{score}")
            }
            RedisCreateKind::List | RedisCreateKind::Set => {
                let value = dialog.value.to_string();
                if value.is_empty() {
                    return;
                }
                dialog.value = zeroize::Zeroizing::new(String::new());
                value
            }
            RedisCreateKind::String | RedisCreateKind::Json => return,
        };
        dialog.entries.push(zeroize::Zeroizing::new(entry));
        dialog.error = None;
        cx.notify();
    }

    pub fn remove_redis_create_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut()
            && index < dialog.entries.len()
        {
            dialog.entries.remove(index);
            cx.notify();
        }
    }

    pub fn submit_redis_create_key(&mut self, cx: &mut Context<Self>) {
        let messages = self.messages.clone();
        if !self
            .ui
            .host_redis_create_key_dialog
            .as_ref()
            .is_some_and(|d| self.redis_target_selected(&d.connection_id, &d.endpoint))
        {
            return;
        }
        let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() else {
            return;
        };
        if dialog.loading {
            return;
        }
        let error = |messages: &Option<HostToolsMessages>,
                     pick: fn(&HostToolsMessages) -> &String| {
            messages.as_ref().map(pick).cloned()
        };
        let name = dialog.name.trim().to_string();
        if name.is_empty() || name.contains(['\n', '\r']) {
            dialog.error = error(&messages, |messages| {
                &messages.redis_create_key_invalid_name
            });
            cx.notify();
            return;
        }
        let ttl_seconds = if dialog.ttl_never {
            None
        } else {
            match dialog.ttl.trim().parse::<u64>() {
                Ok(seconds) if seconds > 0 => Some(seconds),
                _ => {
                    dialog.error =
                        error(&messages, |messages| &messages.redis_create_key_invalid_ttl);
                    cx.notify();
                    return;
                }
            }
        };
        let body = match dialog.kind {
            RedisCreateKind::String => dialog.value.as_str().to_string(),
            RedisCreateKind::Json => {
                // A JSON key is validated before it is written.
                match serde_json::from_str::<serde_json::Value>(dialog.value.trim()) {
                    Ok(_) => dialog.value.trim().to_string(),
                    Err(parse_error) => {
                        let template = self
                            .messages
                            .as_ref()
                            .map(|messages| messages.redis_value_set_invalid_json.clone())
                            .unwrap_or_default();
                        if let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() {
                            dialog.error =
                                Some(template.replace("{{error}}", &parse_error.to_string()));
                            cx.notify();
                        }
                        return;
                    }
                }
            }
            RedisCreateKind::Hash
            | RedisCreateKind::List
            | RedisCreateKind::Set
            | RedisCreateKind::SortedSet => {
                if dialog.entries.is_empty() {
                    dialog.error = error(&messages, |messages| {
                        &messages.redis_create_key_empty_entries
                    });
                    cx.notify();
                    return;
                }
                String::new()
            }
        };
        let Some(value) = dialog.kind.payload(&body, &dialog.entries, None) else {
            return;
        };
        let request = HostRedisCreateKeyRequest {
            connection_id: dialog.connection_id.clone(),
            endpoint: dialog.endpoint.clone(),
            database: dialog.database,
            key: name,
            password: self.ui.host_redis_password.clone(),
            value,
            failure_fallback: messages
                .as_ref()
                .map(|messages| messages.redis_key_action_failed.clone())
                .unwrap_or_default(),
        };
        let kind = dialog.kind;
        dialog.loading = true;
        dialog.error = None;
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        let os_type = self.connection_os_type(&request.connection_id);
        let command = os_type.as_deref().and_then(|os_type| {
            redis_create_key_command(&request.endpoint, os_type, kind, ttl_seconds)
        });
        let Some(command) = command else {
            if let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() {
                dialog.loading = false;
                dialog.error = Some(request.failure_fallback.clone());
            }
            cx.notify();
            return;
        };
        self.host_redis.create_key_running = Some(request.clone());
        let spawned = self.spawn_redis_create_key_capture(
            command,
            request.clone(),
            HOST_REDIS_ACTION_TIMEOUT,
            HOST_REDIS_VALUE_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_redis.create_key_running = None;
            if let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() {
                dialog.loading = false;
                dialog.error = Some(request.failure_fallback.clone());
            }
        }
        cx.notify();
    }

    pub(crate) fn finish_host_redis_create_key(
        &mut self,
        delivery: HostRedisCreateKeyDelivery,
        cx: &mut Context<Self>,
    ) {
        let is_current = self
            .host_redis
            .create_key_running
            .as_ref()
            .is_some_and(|running| {
                running.connection_id == delivery.request.connection_id
                    && running.endpoint == delivery.request.endpoint
                    && running.database == delivery.request.database
                    && running.key == delivery.request.key
            });
        if !is_current {
            return;
        }
        self.host_redis.create_key_running = None;
        let exists_message = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_create_key_exists.clone());
        let (succeeded, message) = match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let text = String::from_utf8_lossy(&output.stdout);
                match parse_redis_key_mutation(&text) {
                    ResourceRedisStatus::Available => (true, String::new()),
                    ResourceRedisStatus::Error { message } => (
                        false,
                        if message == REDIS_KEY_EXISTS_SENTINEL {
                            exists_message.clone().unwrap_or_else(|| message.clone())
                        } else {
                            message
                        },
                    ),
                    _ => (false, delivery.request.failure_fallback.clone()),
                }
            }
            _ => (false, delivery.request.failure_fallback.clone()),
        };
        if succeeded {
            self.ui.host_redis_create_key_dialog = None;
            self.ui.clear_input_focus();
            self.request_redis_keys_for_selected(cx);
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisKeyCreated {
                    key: delivery.request.key.clone(),
                },
            ));
        } else if let Some(dialog) = self.ui.host_redis_create_key_dialog.as_mut() {
            dialog.loading = false;
            dialog.error = Some(message.clone());
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisKeyActionFailed { message },
            ));
        }
        cx.notify();
    }

    /// Opens the editor for the value currently shown, pre-filled with its
    /// rows so an edit starts from the real content.
    pub fn open_redis_set_value(&mut self, database: u32, key: String, cx: &mut Context<Self>) {
        // The editor edits what the viewer shows, so it inherits that dialog's
        // host/endpoint instead of the panel's current selection.
        let Some((connection_id, endpoint, value)) = self
            .host_redis
            .value_dialog
            .as_ref()
            .filter(|dialog| dialog.database == database && dialog.key == key)
            .and_then(|dialog| {
                dialog
                    .value
                    .clone()
                    .map(|value| (dialog.connection_id.clone(), dialog.endpoint.clone(), value))
            })
        else {
            return;
        };
        let Some(kind) = RedisValueWriteKind::from_value_kind(value.kind) else {
            return;
        };
        let rows = redis_value_rows(&value);
        // The editor replaces the whole key: a read that hit its row cap cannot
        // be written back without deleting the entries it never saw.
        let capped = matches!(
            value.kind,
            ResourceRedisValueKind::Hash
                | ResourceRedisValueKind::List
                | ResourceRedisValueKind::Set
                | ResourceRedisValueKind::SortedSet
        ) && value
            .length
            .is_some_and(|length| length > rows.len() as u64);
        if capped || !redis_value_copy_is_complete(&value) {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisValueEditCapped,
            ));
            return;
        }
        // Whatever the reader could not represent exactly would be dropped or
        // merged by the rewrite, so that value stays read-only.
        if redis_value_edit_unsupported(&value) {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisValueEditUnsupported,
            ));
            return;
        }
        let (prefill, entries) = match kind {
            RedisValueWriteKind::String => (value.body.clone(), Vec::new()),
            RedisValueWriteKind::Json => (
                redis_value_json(&value).unwrap_or_else(|| value.body.clone()),
                Vec::new(),
            ),
            _ => (
                String::new(),
                rows.into_iter()
                    .map(|(first, second)| {
                        zeroize::Zeroizing::new(match second {
                            Some(second) => format!("{first}\t{second}"),
                            None => first,
                        })
                    })
                    .collect(),
            ),
        };
        self.ui.host_redis_set_value_dialog = Some(HostRedisSetValueDialog {
            connection_id,
            endpoint,
            database,
            key,
            kind,
            base: Some(value),
            value: zeroize::Zeroizing::new(prefill),
            field: zeroize::Zeroizing::new(String::new()),
            entries,
            error: None,
            loading: false,
        });
        self.ui.focus_input(HostToolsTextInput::RedisSetValue);
        cx.notify();
    }

    pub fn redis_set_value_open(&self) -> bool {
        self.ui.host_redis_set_value_dialog.is_some()
    }

    pub fn dismiss_redis_set_value(&mut self, cx: &mut Context<Self>) {
        if self.ui.host_redis_set_value_dialog.take().is_some() {
            self.ui.clear_input_focus();
            cx.notify();
        }
    }

    /// Appends the pending row; hash pairs need a field, the others one value.
    /// Pretty-prints the JSON draft in place; the save still minifies it, this
    /// only makes the text editable by hand.
    pub fn format_redis_json_value(&mut self, cx: &mut Context<Self>) {
        let invalid_json = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_value_set_invalid_json.clone())
            .unwrap_or_default();
        let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() else {
            return;
        };
        let json_view = self
            .host_redis
            .value_dialog
            .as_ref()
            .is_some_and(|viewer| viewer.json_view);
        if dialog.kind != RedisValueWriteKind::Json
            && !(dialog.kind == RedisValueWriteKind::String && json_view)
        {
            return;
        }
        match serde_json::from_str::<serde_json::Value>(dialog.value.trim()) {
            Ok(parsed) => {
                if let Ok(pretty) = serde_json::to_string_pretty(&parsed) {
                    dialog.value = zeroize::Zeroizing::new(pretty);
                }
                dialog.error = None;
            }
            Err(error) => {
                dialog.error = Some(invalid_json.replace("{{error}}", &error.to_string()));
            }
        }
        cx.notify();
    }

    pub fn add_redis_set_value_entry(&mut self, cx: &mut Context<Self>) {
        let messages = self.messages.clone();
        let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() else {
            return;
        };
        let entry = match dialog.kind {
            RedisValueWriteKind::String | RedisValueWriteKind::Json => return,
            RedisValueWriteKind::Hash => {
                let field = dialog.field.trim().to_string();
                if field.is_empty() || field.contains(['\t', '\n']) {
                    dialog.error = messages
                        .as_ref()
                        .map(|messages| messages.redis_create_key_invalid_field.clone());
                    cx.notify();
                    return;
                }
                let value = dialog.value.trim_matches(['\n', '\r']).to_string();
                dialog.field = zeroize::Zeroizing::new(String::new());
                dialog.value = zeroize::Zeroizing::new(String::new());
                format!("{field}\t{value}")
            }
            RedisValueWriteKind::List
            | RedisValueWriteKind::Set
            | RedisValueWriteKind::SortedSet => {
                let value = dialog.value.trim().to_string();
                if value.is_empty() {
                    return;
                }
                dialog.value = zeroize::Zeroizing::new(String::new());
                if dialog.kind == RedisValueWriteKind::SortedSet {
                    // `member score`; a missing score defaults to 0.
                    match value.split_once([' ', '\t']) {
                        Some((member, score)) if score.trim().parse::<f64>().is_ok() => {
                            format!("{}\t{}", member.trim(), score.trim())
                        }
                        Some((member, _)) => format!("{}\t0", member.trim()),
                        None => format!("{value}\t0"),
                    }
                } else {
                    value
                }
            }
        };
        dialog.entries.push(zeroize::Zeroizing::new(entry));
        dialog.error = None;
        cx.notify();
    }

    pub fn remove_redis_set_value_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut()
            && index < dialog.entries.len()
        {
            dialog.entries.remove(index);
            cx.notify();
        }
    }

    pub fn submit_redis_set_value(&mut self, cx: &mut Context<Self>) {
        let json_error_key = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_value_set_invalid_json.clone())
            .unwrap_or_default();
        let empty_entries_key = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_create_key_empty_entries.clone())
            .unwrap_or_default();
        let unsupported_message = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_value_edit_unsupported.clone())
            .unwrap_or_default();
        let action_failed_message = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_key_action_failed.clone())
            .unwrap_or_default();
        if !self
            .ui
            .host_redis_set_value_dialog
            .as_ref()
            .is_some_and(|d| self.redis_target_selected(&d.connection_id, &d.endpoint))
        {
            return;
        }
        let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() else {
            return;
        };
        if dialog.loading {
            return;
        }
        let body = match validate_redis_set_value(dialog) {
            Ok(body) => body,
            Err(error) => {
                dialog.loading = false;
                dialog.error = Some(redis_set_value_error_text(
                    error,
                    &json_error_key,
                    &empty_entries_key,
                    &unsupported_message,
                ));
                cx.notify();
                return;
            }
        };
        let Some(base) = dialog.base.as_ref() else {
            return;
        };
        let Some(value) = dialog.kind.payload(&body, &dialog.entries, Some(base)) else {
            return;
        };
        let request = HostRedisSetValueRequest {
            connection_id: dialog.connection_id.clone(),
            endpoint: dialog.endpoint.clone(),
            database: dialog.database,
            key: dialog.key.clone(),
            password: self.ui.host_redis_password.clone(),
            value,
            failure_fallback: action_failed_message,
        };
        let write_kind = dialog.kind;
        dialog.loading = true;
        dialog.error = None;
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            if let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() {
                dialog.loading = false;
                dialog.error = Some(request.failure_fallback.clone());
            }
            cx.notify();
            return;
        };
        let os_type = self.connection_os_type(&request.connection_id);
        let command = os_type.as_deref().and_then(|os_type| {
            redis_replace_value_command(&request.endpoint, os_type, write_kind)
        });
        let Some(command) = command else {
            if let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() {
                dialog.loading = false;
                dialog.error = Some(request.failure_fallback.clone());
            }
            cx.notify();
            return;
        };
        self.host_redis.set_value_running = Some(request.clone());
        let spawned = self.spawn_redis_set_value_capture(
            command,
            request.clone(),
            HOST_REDIS_ACTION_TIMEOUT,
            HOST_REDIS_VALUE_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_redis.set_value_running = None;
            if let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() {
                dialog.loading = false;
                dialog.error = Some(request.failure_fallback.clone());
            }
        }
        cx.notify();
    }

    pub(crate) fn finish_host_redis_set_value(
        &mut self,
        delivery: HostRedisSetValueDelivery,
        cx: &mut Context<Self>,
    ) {
        let is_current = self
            .host_redis
            .set_value_running
            .as_ref()
            .is_some_and(|running| {
                running.connection_id == delivery.request.connection_id
                    && running.endpoint == delivery.request.endpoint
                    && running.database == delivery.request.database
                    && running.key == delivery.request.key
            });
        if !is_current {
            return;
        }
        self.host_redis.set_value_running = None;
        let (succeeded, message) = match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let text = String::from_utf8_lossy(&output.stdout);
                match parse_redis_key_mutation(&text) {
                    ResourceRedisStatus::Available => (true, String::new()),
                    ResourceRedisStatus::Error { message }
                        if message == "__OXIDE_VALUE_CHANGED__" =>
                    {
                        (
                            false,
                            self.messages
                                .as_ref()
                                .map(|m| m.redis_value_edit_conflict.clone())
                                .unwrap_or_else(|| delivery.request.failure_fallback.clone()),
                        )
                    }
                    ResourceRedisStatus::Error { message }
                        if message == REDIS_KEY_EXISTS_SENTINEL =>
                    {
                        (
                            false,
                            self.messages
                                .as_ref()
                                .map(|m| m.redis_create_key_exists.clone())
                                .unwrap_or_else(|| delivery.request.failure_fallback.clone()),
                        )
                    }
                    ResourceRedisStatus::Error { .. } => {
                        (false, delivery.request.failure_fallback.clone())
                    }
                    _ => (false, delivery.request.failure_fallback.clone()),
                }
            }
            _ => (false, delivery.request.failure_fallback.clone()),
        };
        if succeeded {
            self.ui.host_redis_set_value_dialog = None;
            self.ui.clear_input_focus();
            if let Some(dialog) = self.host_redis.value_dialog.as_mut()
                && dialog.connection_id == delivery.request.connection_id
                && dialog.endpoint == delivery.request.endpoint
                && dialog.database == delivery.request.database
                && dialog.key == delivery.request.key
            {
                dialog.loading = true;
                dialog.error = None;
                self.refresh_redis_value(cx);
            }
            cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::RedisValueSet {
                key: delivery.request.key.clone(),
            }));
        } else if let Some(dialog) = self.ui.host_redis_set_value_dialog.as_mut() {
            dialog.loading = false;
            dialog.error = Some(message.clone());
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisKeyActionFailed { message },
            ));
        }
        cx.notify();
    }

    pub fn copy_redis_key(&mut self, key: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(key.clone()));
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::RedisKeyCopied { key },
        ));
    }

    pub fn copy_redis_value(&mut self, database: u32, key: String, cx: &mut Context<Self>) {
        let Some((connection_id, endpoint)) = self.redis_value_target() else {
            return;
        };
        self.request_redis_value(
            connection_id,
            endpoint,
            database,
            key,
            HostRedisValuePurpose::Copy,
            cx,
        );
        cx.notify();
    }

    /// Starts the inline rename next to the key in the value viewer; the
    /// rename never opens a dialog of its own.
    pub fn open_redis_rename(&mut self, _database: u32, _key: String, cx: &mut Context<Self>) {
        let Some(dialog) = self.host_redis.value_dialog.as_ref() else {
            return;
        };
        // Editing starts from the current name: a rename is usually a small
        // change, not a retype.
        self.ui.host_redis_rename_value = Some(zeroize::Zeroizing::new(dialog.key.clone()));
        self.ui.host_redis_rename_error = None;
        self.ui.focus_input(HostToolsTextInput::RedisRename);
        cx.notify();
    }

    pub fn dismiss_redis_rename(&mut self, cx: &mut Context<Self>) {
        if self.ui.host_redis_rename_value.take().is_none() {
            return;
        }
        self.ui.host_redis_rename_error = None;
        self.ui.clear_input_focus();
        cx.notify();
    }

    pub fn submit_redis_rename(&mut self, cx: &mut Context<Self>) {
        let invalid_message = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_rename_invalid.clone())
            .unwrap_or_default();
        if self.host_redis.mutation_running.is_some() {
            return;
        }
        let Some(dialog) = self.host_redis.value_dialog.as_ref() else {
            return;
        };
        let Some(rename_value) = self.ui.host_redis_rename_value.as_deref() else {
            return;
        };
        let new_key = rename_value.trim().to_string();
        if new_key.is_empty() || new_key == dialog.key {
            self.ui.host_redis_rename_error = Some(invalid_message);
            cx.notify();
            return;
        }
        let connection_id = dialog.connection_id.clone();
        let endpoint = dialog.endpoint.clone();
        let database = dialog.database;
        let key = dialog.key.clone();
        self.ui.host_redis_rename_error = None;
        self.request_redis_mutation(
            connection_id,
            endpoint,
            database,
            HostRedisMutationKind::Rename,
            key,
            Vec::new(),
            Some(new_key),
            None,
            cx,
        );
    }

    /// Opens the TTL chip for editing; empty input means "no expiry".
    pub fn open_redis_ttl_edit(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.host_redis.value_dialog.as_ref() else {
            return;
        };
        let current = match dialog.value.as_ref().map(|value| value.ttl) {
            Some(oxideterm_connection_monitor::ResourceRedisTtl::Seconds(seconds)) => {
                seconds.to_string()
            }
            _ => String::new(),
        };
        self.ui.host_redis_ttl_value = Some(zeroize::Zeroizing::new(current));
        self.ui.host_redis_ttl_error = None;
        self.ui.focus_input(HostToolsTextInput::RedisTtl);
        cx.notify();
    }

    pub fn dismiss_redis_ttl_edit(&mut self, cx: &mut Context<Self>) {
        if self.ui.host_redis_ttl_value.take().is_none() {
            return;
        }
        self.ui.host_redis_ttl_error = None;
        self.ui.clear_input_focus();
        cx.notify();
    }

    pub fn submit_redis_ttl(&mut self, cx: &mut Context<Self>) {
        let invalid_message = self
            .messages
            .as_ref()
            .map(|messages| messages.redis_create_key_invalid_ttl.clone())
            .unwrap_or_default();
        if self.host_redis.mutation_running.is_some() {
            return;
        }
        let Some(dialog) = self.host_redis.value_dialog.as_ref() else {
            return;
        };
        let Some(ttl_value) = self.ui.host_redis_ttl_value.as_deref() else {
            return;
        };
        let seconds = match ttl_value.trim().parse::<u64>() {
            Ok(seconds) if seconds > 0 => seconds,
            _ => {
                self.ui.host_redis_ttl_error = Some(invalid_message);
                cx.notify();
                return;
            }
        };
        let (connection_id, endpoint, database, key) = (
            dialog.connection_id.clone(),
            dialog.endpoint.clone(),
            dialog.database,
            dialog.key.clone(),
        );
        self.ui.host_redis_ttl_error = None;
        self.request_redis_mutation(
            connection_id,
            endpoint,
            database,
            HostRedisMutationKind::Expire,
            key,
            Vec::new(),
            None,
            Some(seconds),
            cx,
        );
    }

    /// `PERSIST` keeps the key and drops the expiry.
    pub fn submit_redis_ttl_never(&mut self, cx: &mut Context<Self>) {
        if self.host_redis.mutation_running.is_some() {
            return;
        }
        let Some(dialog) = self.host_redis.value_dialog.as_ref() else {
            return;
        };
        let (connection_id, endpoint, database, key) = (
            dialog.connection_id.clone(),
            dialog.endpoint.clone(),
            dialog.database,
            dialog.key.clone(),
        );
        self.ui.host_redis_ttl_error = None;
        self.request_redis_mutation(
            connection_id,
            endpoint,
            database,
            HostRedisMutationKind::Persist,
            key,
            Vec::new(),
            None,
            None,
            cx,
        );
    }

    pub fn request_redis_delete(&mut self, database: u32, key: String, cx: &mut Context<Self>) {
        let Some((connection_id, endpoint)) = self.redis_value_target() else {
            return;
        };
        self.host_redis.delete_confirm = Some(HostRedisDeleteConfirm {
            connection_id,
            endpoint,
            database,
            keys: vec![key.clone()],
            key,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
        });
        cx.notify();
    }

    /// Opens the confirmation for a whole prefix, listing every key the
    /// subtree holds. Redis has no folder object, so "delete a node" means
    /// deleting each key whose name lives under that colon path.
    pub(crate) fn request_redis_delete_prefix(
        &mut self,
        database: u32,
        prefix: String,
        cx: &mut Context<Self>,
    ) {
        if prefix.trim().is_empty() {
            return;
        }
        let Some((connection_id, endpoint)) = self.redis_value_target() else {
            return;
        };
        if !self.ui.host_redis_search_query.trim().is_empty() {
            // A filtered scan only holds matches, so the subtree's other keys
            // would survive a deletion that claims to remove it.
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisPrefixDeleteIncomplete,
            ));
            return;
        }
        let Some(keys) = self.host_redis.keys.as_ref() else {
            return;
        };
        let Some(database_keys) = keys
            .databases
            .iter()
            .find(|candidate| candidate.index == database)
            .map(|candidate| {
                // The scan is capped, so `key_count` above the loaded rows means
                // keys under the prefix were never seen.
                let complete = candidate.key_count as usize == candidate.keys.len();
                (complete, redis_prefix_keys(&candidate.keys, &prefix))
            })
        else {
            return;
        };
        let (complete, database_keys) = database_keys;
        if database_keys.is_empty() {
            return;
        }
        if !complete {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::RedisPrefixDeleteIncomplete,
            ));
            return;
        }
        self.host_redis.delete_confirm = Some(HostRedisDeleteConfirm {
            connection_id,
            endpoint,
            database,
            keys: database_keys,
            key: prefix,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
        });
        cx.notify();
    }

    pub fn dismiss_redis_delete(&mut self, cx: &mut Context<Self>) {
        if self.host_redis.delete_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub fn confirm_redis_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.host_redis.delete_confirm.take() else {
            return;
        };
        self.ui.clear_input_focus();
        // A subtree deletion always targets the confirmed key list: when only
        // one key matched, `confirm.key` is the prefix shown in the dialog, not
        // that key's name, so a single-key mutation would delete the wrong
        // key.
        let kind = if confirm.keys.len() == 1 && confirm.keys[0] == confirm.key {
            HostRedisMutationKind::Delete
        } else {
            HostRedisMutationKind::DeleteMany
        };
        self.request_redis_mutation(
            confirm.connection_id,
            confirm.endpoint,
            confirm.database,
            kind,
            confirm.key,
            confirm.keys,
            None,
            None,
            cx,
        );
        cx.notify();
    }

    fn request_redis_value(
        &mut self,
        connection_id: String,
        endpoint: ResourceRedisEndpoint,
        database: u32,
        key: String,
        purpose: HostRedisValuePurpose,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(messages) = self.messages.clone() else {
            return false;
        };
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return false;
        };
        if !self.redis_target_selected(&connection_id, &endpoint) {
            return false;
        }
        let request = HostRedisValueRequest {
            connection_id,
            endpoint,
            database,
            key,
            password: self.ui.host_redis_password.clone(),
            purpose,
            failure_fallback: messages.redis_probe_failed,
        };
        if self.host_redis.value_in_flight {
            self.host_redis.value_pending = Some(HostRedisValuePending { request, runtime });
            cx.notify();
            return true;
        }
        self.start_redis_value(request, runtime, cx);
        true
    }

    fn start_redis_value(
        &mut self,
        request: HostRedisValueRequest,
        runtime: HostToolsRuntime,
        cx: &mut Context<Self>,
    ) {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            self.host_redis.value_in_flight = false;
            self.fail_redis_value_dialog(&request, cx);
            cx.notify();
            return;
        };
        let Some(command) = redis_value_command(&request.endpoint, &os_type) else {
            self.host_redis.value_in_flight = false;
            self.fail_redis_value_dialog(&request, cx);
            cx.notify();
            return;
        };
        self.host_redis.value_running = Some(request.clone());
        self.host_redis.value_in_flight = true;
        let spawned = self.spawn_redis_value_capture(
            command,
            request.clone(),
            HOST_REDIS_VALUE_TIMEOUT,
            HOST_REDIS_VALUE_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_redis.value_running = None;
            self.host_redis.value_in_flight = false;
            self.fail_redis_value_dialog(&request, cx);
        }
        cx.notify();
    }

    /// Marks the read that failed. The failure is only shown on the surface
    /// that asked for this exact target: a stale failure must not repaint
    /// another host's dialog or editor.
    fn fail_redis_value_dialog(&mut self, request: &HostRedisValueRequest, cx: &mut Context<Self>) {
        let message = request.failure_fallback.clone();
        match request.purpose {
            HostRedisValuePurpose::Open => {
                if let Some(dialog) = self.host_redis.value_dialog.as_mut()
                    && dialog.matches_request(request)
                {
                    dialog.loading = false;
                    dialog.error = Some(message);
                }
            }
            HostRedisValuePurpose::Copy => {
                // Nothing was copied, so the user must not be told it was.
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::RedisValueCopyFailed {
                        key: request.key.clone(),
                    },
                ));
            }
        }
    }

    pub(crate) fn finish_host_redis_value(
        &mut self,
        delivery: HostRedisValueDelivery,
        cx: &mut Context<Self>,
    ) {
        let is_current = self
            .host_redis
            .value_running
            .as_ref()
            .is_some_and(|running| running.same_target(&delivery.request));
        if !is_current {
            return;
        }
        self.host_redis.value_in_flight = false;
        self.host_redis.value_running = None;
        // A cut short capture carries only a prefix of the value, so the flag
        // travels with the result into the copy decision.
        let truncated = match &delivery.result {
            Ok(output) => output.truncated,
            Err(()) => false,
        };
        let value = match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let text = String::from_utf8_lossy(&output.stdout);
                Some(parse_redis_value(&text))
            }
            _ => None,
        };
        match delivery.request.purpose {
            HostRedisValuePurpose::Open => {
                self.install_redis_value(&delivery.request, if truncated { None } else { value });
            }
            HostRedisValuePurpose::Copy => {
                self.finish_redis_value_copy(&delivery.request, value.as_ref(), truncated, cx);
            }
        }
        let pending = self.host_redis.value_pending.take();
        if let Some(pending) = pending {
            self.start_redis_value(pending.request, pending.runtime, cx);
        }
        cx.notify();
    }

    /// Installs a browsed value into the viewer, but only when the delivery
    /// still addresses the dialog's own host, endpoint, database and key. A
    /// key name is not unique across databases or endpoints, so matching the
    /// name alone could show one key's value and save it onto another.
    fn install_redis_value(
        &mut self,
        request: &HostRedisValueRequest,
        value: Option<ResourceRedisValue>,
    ) {
        let messages = self.messages.clone();
        let Some(dialog) = self.host_redis.value_dialog.as_mut() else {
            return;
        };
        if !dialog.matches_request(request) {
            return;
        }
        dialog.loading = false;
        dialog.clear_value();
        dialog.error = None;
        match value {
            Some(value) => match &value.status {
                ResourceRedisStatus::Available => {
                    // Types without a JSON shape fall back to the text view;
                    // a RedisJSON value is always shown as JSON.
                    if dialog.json_view && redis_value_json(&value).is_none() {
                        dialog.json_view = false;
                    }
                    if value.kind == oxideterm_connection_monitor::ResourceRedisValueKind::Json {
                        dialog.json_view = true;
                    }
                    dialog.set_value(Some(value));
                }
                ResourceRedisStatus::AuthRequired => {
                    dialog.error = messages
                        .as_ref()
                        .map(|messages| messages.redis_auth_required.clone());
                }
                ResourceRedisStatus::NoServer => {
                    dialog.error = messages
                        .as_ref()
                        .map(|messages| messages.redis_key_missing.clone());
                }
                ResourceRedisStatus::Error { message } => {
                    dialog.error = Some(message.clone());
                }
                _ => {
                    dialog.error = Some(request.failure_fallback.clone());
                }
            },
            None => {
                dialog.error = Some(request.failure_fallback.clone());
            }
        }
        self.sync_redis_scalar_editor();
    }

    /// Keeps the value box editable without an extra click: a string or JSON
    /// value always has an inline editor whose text is the value itself. The
    /// collection editor stays behind its explicit action.
    fn sync_redis_scalar_editor(&mut self) {
        let Some(dialog) = self.host_redis.value_dialog.as_ref() else {
            return;
        };
        let Some(value) = dialog.value.as_ref() else {
            return;
        };
        if !matches!(value.status, ResourceRedisStatus::Available) {
            return;
        }
        let Some(kind) = RedisValueWriteKind::from_value_kind(value.kind) else {
            return;
        };
        if !matches!(
            kind,
            RedisValueWriteKind::String | RedisValueWriteKind::Json
        ) {
            return;
        }
        if !redis_value_copy_is_complete(value) || redis_value_edit_unsupported(value) {
            return;
        }
        // A draft for this exact target is the user's work in progress.
        if self
            .ui
            .host_redis_set_value_dialog
            .as_ref()
            .is_some_and(|editor| {
                editor.connection_id == dialog.connection_id
                    && editor.endpoint == dialog.endpoint
                    && editor.database == dialog.database
                    && editor.key == dialog.key
            })
        {
            return;
        }
        let prefill = match kind {
            RedisValueWriteKind::Json => {
                redis_value_json(value).unwrap_or_else(|| value.body.clone())
            }
            RedisValueWriteKind::String if dialog.json_view => {
                redis_value_json(value).unwrap_or_else(|| value.body.clone())
            }
            _ => value.body.clone(),
        };
        self.ui.host_redis_set_value_dialog = Some(HostRedisSetValueDialog {
            connection_id: dialog.connection_id.clone(),
            endpoint: dialog.endpoint.clone(),
            database: dialog.database,
            key: dialog.key.clone(),
            kind,
            base: Some(value.clone()),
            value: zeroize::Zeroizing::new(prefill),
            field: zeroize::Zeroizing::new(String::new()),
            entries: Vec::new(),
            error: None,
            loading: false,
        });
    }

    /// Copies the value only when the read produced the whole thing. A capped
    /// collection, a cut short capture, or an error reply is not the key's
    /// value; copying it would hand the user a silent prefix.
    fn finish_redis_value_copy(
        &mut self,
        request: &HostRedisValueRequest,
        value: Option<&ResourceRedisValue>,
        truncated: bool,
        cx: &mut Context<Self>,
    ) {
        let complete = !truncated
            && value.is_some_and(|value| {
                matches!(value.status, ResourceRedisStatus::Available)
                    && redis_value_copy_is_complete(value)
            });
        match value.filter(|_| complete) {
            Some(value) => {
                cx.write_to_clipboard(ClipboardItem::new_string(redis_value_copy_text(value)));
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::RedisValueCopied {
                        key: request.key.clone(),
                    },
                ));
            }
            None => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::RedisValueCopyFailed {
                        key: request.key.clone(),
                    },
                ));
            }
        }
        cx.notify();
    }

    /// Compares a save's pre-write re-read with the value the editor opened
    /// with. A different server copy aborts the write: overwriting it would
    /// silently drop someone else's update.
    fn request_redis_mutation(
        &mut self,
        connection_id: String,
        endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
        database: u32,
        kind: HostRedisMutationKind,
        key: String,
        keys: Vec<String>,
        new_key: Option<String>,
        ttl_seconds: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some(messages) = self.messages.clone() else {
            return;
        };
        if !self.redis_target_selected(&connection_id, &endpoint) {
            return;
        }
        let request = HostRedisMutationRequest {
            connection_id,
            endpoint,
            database,
            kind,
            key,
            keys,
            new_key,
            ttl_seconds,
            password: self.ui.host_redis_password.clone(),
            failure_fallback: messages.redis_key_action_failed,
        };
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        let os_type = self.connection_os_type(&request.connection_id);
        let command = os_type.as_deref().and_then(|os_type| {
            redis_key_mutation_command(
                &request.endpoint,
                os_type,
                match request.kind {
                    HostRedisMutationKind::Delete => RedisKeyMutation::Delete,
                    HostRedisMutationKind::DeleteMany => {
                        return redis_keys_delete_command(&request.endpoint, os_type);
                    }
                    HostRedisMutationKind::Rename => RedisKeyMutation::Rename,
                    HostRedisMutationKind::Expire => RedisKeyMutation::Expire,
                    HostRedisMutationKind::Persist => RedisKeyMutation::Persist,
                },
            )
        });
        let Some(command) = command else {
            let message = request.failure_fallback.clone();
            self.finish_redis_mutation_ui(&request, false, message, cx);
            return;
        };
        self.host_redis.mutation_running = Some(request.clone());
        self.host_redis.value_in_flight = false;
        let spawned = self.spawn_redis_mutation_capture(
            command,
            request.clone(),
            HOST_REDIS_ACTION_TIMEOUT,
            HOST_REDIS_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_redis.mutation_running = None;
            let message = request.failure_fallback.clone();
            self.finish_redis_mutation_ui(&request, false, message, cx);
        }
        cx.notify();
    }

    pub(crate) fn finish_host_redis_mutation(
        &mut self,
        delivery: HostRedisMutationDelivery,
        cx: &mut Context<Self>,
    ) {
        let is_current = self
            .host_redis
            .mutation_running
            .as_ref()
            .is_some_and(|running| {
                running.connection_id == delivery.request.connection_id
                    && running.database == delivery.request.database
                    && running.kind == delivery.request.kind
                    && running.key == delivery.request.key
                    && running.keys == delivery.request.keys
                    && running.new_key == delivery.request.new_key
            });
        if !is_current {
            return;
        }
        self.host_redis.mutation_running = None;
        let (succeeded, message) = match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let text = String::from_utf8_lossy(&output.stdout);
                match parse_redis_key_mutation(&text) {
                    ResourceRedisStatus::Available => (true, String::new()),
                    ResourceRedisStatus::Error { message }
                        if message == REDIS_KEY_EXISTS_SENTINEL =>
                    {
                        (
                            false,
                            self.messages
                                .as_ref()
                                .map(|m| m.redis_create_key_exists.clone())
                                .unwrap_or_else(|| delivery.request.failure_fallback.clone()),
                        )
                    }
                    ResourceRedisStatus::Error { message } => (false, message),
                    _ => (false, delivery.request.failure_fallback.clone()),
                }
            }
            _ => (false, delivery.request.failure_fallback.clone()),
        };
        self.finish_redis_mutation_ui(&delivery.request, succeeded, message, cx);
    }

    fn finish_redis_mutation_ui(
        &mut self,
        request: &HostRedisMutationRequest,
        succeeded: bool,
        message: String,
        cx: &mut Context<Self>,
    ) {
        if succeeded {
            self.ui.clear_input_focus();
            match request.kind {
                // The browsed keys changed shape: re-read the endpoint and
                // reopen the viewer under the new name.
                HostRedisMutationKind::Rename => {
                    self.ui.host_redis_rename_value = None;
                    self.ui.host_redis_rename_error = None;
                    if let Some(new_key) = request.new_key.clone() {
                        self.host_redis.value_dialog = None;
                        self.open_redis_value(request.database, new_key, cx);
                    }
                    self.request_redis_keys_for_selected(cx);
                }
                // An expiry change leaves the key list intact; re-read the
                // value so the chip shows the server's answer.
                HostRedisMutationKind::Expire | HostRedisMutationKind::Persist => {
                    self.ui.host_redis_ttl_value = None;
                    self.ui.host_redis_ttl_error = None;
                    if let Some(dialog) = self.host_redis.value_dialog.as_mut()
                        && dialog.matches_mutation(request)
                    {
                        dialog.loading = true;
                        dialog.error = None;
                    }
                    self.refresh_redis_value(cx);
                }
                HostRedisMutationKind::Delete | HostRedisMutationKind::DeleteMany => {
                    self.host_redis.value_dialog = None;
                    self.request_redis_keys_for_selected(cx);
                }
            }
        } else if self
            .host_redis
            .value_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.matches_mutation(request))
        {
            if self.ui.host_redis_ttl_value.is_some() {
                self.ui.host_redis_ttl_error = Some(message.clone());
            } else if self.ui.host_redis_rename_value.is_some() {
                self.ui.host_redis_rename_error = Some(message.clone());
            }
        }
        cx.emit(HostToolsEvent::ShowNotice(
            match (request.kind, succeeded) {
                (HostRedisMutationKind::Delete | HostRedisMutationKind::DeleteMany, true) => {
                    HostToolsNotice::RedisKeyDeleted {
                        key: request.key.clone(),
                    }
                }
                (HostRedisMutationKind::Rename, true) => HostToolsNotice::RedisKeyRenamed {
                    key: request.new_key.clone().unwrap_or_default(),
                },
                (HostRedisMutationKind::Expire | HostRedisMutationKind::Persist, true) => {
                    HostToolsNotice::RedisTtlUpdated {
                        key: request.key.clone(),
                    }
                }
                _ => HostToolsNotice::RedisKeyActionFailed { message },
            },
        ));
        cx.notify();
    }
}

fn push_redis_tree_rows(
    nodes: &[ResourceRedisKeyNode],
    prefix: &str,
    database_index: u32,
    depth: usize,
    expanded_segments: &std::collections::BTreeSet<String>,
    rows: &mut Vec<HostRedisTreeRow>,
) {
    for (index, node) in nodes.iter().enumerate() {
        let path = format!("{prefix}{index}");
        let expandable = !node.children.is_empty();
        let expanded = expandable && expanded_segments.contains(&path);
        rows.push(HostRedisTreeRow {
            path: path.clone(),
            label: node.label.clone(),
            depth,
            key_count: node.key_count,
            expandable,
            expanded,
            is_database: false,
            is_key: node.is_key,
            key: node.is_key.then(|| node.path.clone()),
            prefix: Some(node.path.clone()),
            database: database_index,
            is_current_key: false,
        });
        if expanded {
            // A prefix that is itself a key (`config` and `config:version`)
            // renders as its own leaf inside the prefix, otherwise the key's
            // value would be unreachable from the tree.
            if node.is_key {
                rows.push(HostRedisTreeRow {
                    path: format!("{path}:self"),
                    label: node.path.clone(),
                    depth: depth + 1,
                    key_count: 1,
                    expandable: false,
                    expanded: false,
                    is_database: false,
                    is_key: true,
                    key: Some(node.path.clone()),
                    prefix: Some(node.path.clone()),
                    database: database_index,
                    is_current_key: true,
                });
            }
            push_redis_tree_rows(
                &node.children,
                &format!("{path}:"),
                database_index,
                depth + 1,
                expanded_segments,
                rows,
            );
        }
    }
}

/// A rejected replacement, before it is turned into a localized message.
enum RedisSetValueError {
    InvalidJson(String),
    EmptyEntries,
    Unsupported,
}

fn redis_set_value_error_text(
    error: RedisSetValueError,
    json_error_key: &str,
    empty_entries_key: &str,
    unsupported_message: &str,
) -> String {
    match error {
        RedisSetValueError::InvalidJson(reason) => json_error_key.replace("{{error}}", &reason),
        RedisSetValueError::EmptyEntries => empty_entries_key.to_string(),
        RedisSetValueError::Unsupported => unsupported_message.to_string(),
    }
}

/// Validates an edit and returns the body the write command receives.
///
/// The write path is line framed (`field<TAB>value` for hashes and sorted sets,
/// one member per line otherwise), so a row that cannot survive that framing is
/// refused here instead of being written back changed.
fn validate_redis_set_value(
    dialog: &HostRedisSetValueDialog,
) -> Result<String, RedisSetValueError> {
    match dialog.kind {
        RedisValueWriteKind::String => Ok(dialog.value.to_string()),
        RedisValueWriteKind::Json => {
            // A JSON document must parse before it is written.
            match serde_json::from_str::<serde_json::Value>(dialog.value.trim()) {
                // Stored JSON is compact: the viewer pretty-prints it again,
                // so formatting is not part of the value.
                Ok(parsed) => serde_json::to_string(&parsed)
                    .map_err(|error| RedisSetValueError::InvalidJson(error.to_string())),
                Err(error) => Err(RedisSetValueError::InvalidJson(error.to_string())),
            }
        }
        RedisValueWriteKind::Hash
        | RedisValueWriteKind::List
        | RedisValueWriteKind::Set
        | RedisValueWriteKind::SortedSet => {
            if redis_set_value_entries_unsupported(
                dialog.kind,
                dialog.entries.iter().map(|entry| entry.as_str()),
            ) {
                return Err(RedisSetValueError::Unsupported);
            }
            if dialog.entries.is_empty() {
                return Err(RedisSetValueError::EmptyEntries);
            }
            Ok(String::new())
        }
    }
}

/// Rows the current framing cannot represent: the write script skips empty
/// members and splits hash/sorted-set rows at the first tab, so either one
/// would silently drop or reshape an entry on the way back.
pub fn redis_set_value_entries_unsupported<'a>(
    kind: RedisValueWriteKind,
    entries: impl Iterator<Item = &'a str>,
) -> bool {
    match kind {
        RedisValueWriteKind::String | RedisValueWriteKind::Json => false,
        RedisValueWriteKind::Hash | RedisValueWriteKind::SortedSet => {
            entries.into_iter().any(|entry| {
                if entry.contains('\n') {
                    return true;
                }
                match entry.split_once('\t') {
                    Some((field, value)) => {
                        // An empty field is skipped by the script, and a second
                        // tab would be read as part of the field.
                        field.is_empty() || field.contains('\t') || value.contains('\n')
                    }
                    // A pair without its separator cannot be rebuilt.
                    None => true,
                }
            })
        }
        RedisValueWriteKind::List | RedisValueWriteKind::Set => entries
            .into_iter()
            .any(|entry| entry.is_empty() || entry.contains('\n')),
    }
}

/// Whether the fetched value cannot be written back byte-for-byte with the
/// current line framing. The editor replaces the whole key, so any element it
/// cannot represent would be dropped or merged on save; refusing the edit is
/// the only safe option until the write frame is lossless too.
fn redis_value_edit_unsupported(value: &ResourceRedisValue) -> bool {
    // The reader keeps the exact element bytes; the display body is only a
    // UTF-8 projection, and a genuine U+FFFD stored value projects to the same
    // character as a replacement. Without exact bytes, a replacement character
    // means invalid bytes were already lost and a write-back would change them.
    let lossy_body = value.body.contains('\u{FFFD}');
    match value.kind {
        ResourceRedisValueKind::String | ResourceRedisValueKind::Json => {
            match value.elements.first() {
                Some(element) => std::str::from_utf8(element).is_err(),
                None => lossy_body,
            }
        }
        ResourceRedisValueKind::Hash | ResourceRedisValueKind::SortedSet => {
            if value.elements.is_empty() {
                let lines = value.body.lines().collect::<Vec<_>>();
                // An unpaired row cannot be split back into field/value, and a
                // tab or carriage return inside a line would shift the pair.
                return lossy_body
                    || !lines.len().is_multiple_of(2)
                    || lines
                        .iter()
                        .any(|line| line.contains('\t') || line.contains('\r'));
            }
            if !value.elements.len().is_multiple_of(2) {
                return true;
            }
            value.elements.iter().enumerate().any(|(index, element)| {
                let field = index.is_multiple_of(2);
                std::str::from_utf8(element).is_err()
                    || element.contains(&b'\n')
                    || element.contains(&b'\r')
                    || (field && element.contains(&b'\t'))
            })
        }
        ResourceRedisValueKind::List | ResourceRedisValueKind::Set => {
            if value.elements.is_empty() {
                // An empty line is an empty member, which the write path skips
                // and every later member would shift up.
                return lossy_body
                    || value
                        .body
                        .lines()
                        .any(|line| line.is_empty() || line.contains('\r'));
            }
            value.elements.iter().any(|element| {
                element.is_empty()
                    || std::str::from_utf8(element).is_err()
                    || element.contains(&b'\n')
                    || element.contains(&b'\r')
            })
        }
        ResourceRedisValueKind::Stream | ResourceRedisValueKind::Other => true,
    }
}

/// Whether a read captured the whole value. Collection reads stop at a row cap
/// and the capture has a byte budget, so a short count means the clipboard
/// would receive a prefix.
fn redis_value_copy_is_complete(value: &ResourceRedisValue) -> bool {
    let captured = match value.kind {
        ResourceRedisValueKind::String | ResourceRedisValueKind::Json => {
            // JSON reads report no length; a string is bounded by STRLEN, so a
            // shorter reply means the capture stopped early.
            return value
                .length
                .map_or(value.kind == ResourceRedisValueKind::Json, |length| {
                    let bytes = value
                        .elements
                        .first()
                        .map_or(value.body.len(), |element| element.len());
                    bytes as u64 == length
                });
        }
        ResourceRedisValueKind::Hash | ResourceRedisValueKind::SortedSet => {
            let elements = if value.elements.is_empty() {
                redis_value_rows(value).len() * 2
            } else {
                value.elements.len()
            };
            // Field/value (or member/score) replies are paired; an odd reply is
            // already incomplete.
            if !elements.is_multiple_of(2) {
                return false;
            }
            (elements / 2) as u64
        }
        ResourceRedisValueKind::List | ResourceRedisValueKind::Set => {
            if value.elements.is_empty() {
                redis_value_rows(value).len() as u64
            } else {
                value.elements.len() as u64
            }
        }
        ResourceRedisValueKind::Stream | ResourceRedisValueKind::Other => return false,
    };
    value.length.is_some_and(|length| captured == length)
}

fn redis_value_copy_text(value: &ResourceRedisValue) -> String {
    match value.kind {
        // The stored document is what the user wants; pretty printing would
        // hand them a rewritten copy.
        ResourceRedisValueKind::Json | ResourceRedisValueKind::String => value.body.clone(),
        _ => redis_value_text(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_connection_monitor::{ResourceRedisTtl, ResourceRedisValue};

    fn database(index: u32, keys: &[&str]) -> ResourceRedisDatabase {
        ResourceRedisDatabase {
            index,
            key_count: keys.len() as u64,
            keys: keys.iter().map(|key| key.to_string()).collect(),
        }
    }

    fn rows(
        databases: &[ResourceRedisDatabase],
        show_empty_databases: bool,
        collapsed: &[&str],
        expanded: &[&str],
    ) -> Vec<String> {
        host_redis_tree_rows(
            databases,
            show_empty_databases,
            &collapsed.iter().map(|path| path.to_string()).collect(),
            &expanded.iter().map(|path| path.to_string()).collect(),
            "",
        )
        .into_iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth), row.label))
        .collect()
    }

    #[test]
    fn value_dialog_size_stays_inside_the_window() {
        // A size remembered from a larger window is pulled back to the current
        // viewport instead of leaving its header and footer off screen.
        assert_eq!(
            clamped_redis_value_dialog_size((1200.0, 800.0), (900.0, 700.0)),
            (852.0, 548.0)
        );
        // A tiny window falls back to the dialog minimums, never to a band the
        // body could not render.
        assert_eq!(
            clamped_redis_value_dialog_size((900.0, 600.0), (100.0, 100.0)),
            (
                HOST_REDIS_VALUE_DIALOG_MIN_WIDTH,
                HOST_REDIS_VALUE_DIALOG_MIN_HEIGHT
            )
        );
        // A size already inside the window is left untouched.
        assert_eq!(
            clamped_redis_value_dialog_size((720.0, 420.0), (1440.0, 900.0)),
            (720.0, 420.0)
        );
    }

    #[gpui::test]
    fn value_dialog_view_prepares_selectable_rows_and_caps_them(cx: &mut gpui::TestAppContext) {
        let (profiler_tx, profiler_rx) = tokio::sync::mpsc::channel(1);
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_tx,
                profiler_rx,
                oxideterm_ssh::SshConnectionRegistry::default(),
                cx,
            )
        });
        entity.update(cx, |entity, _cx| {
            let endpoint = ResourceRedisEndpoint {
                kind: ResourceRedisEndpointKind::Host,
                target: "127.0.0.1:6379".to_string(),
            };
            let value = |kind, body: String, length| ResourceRedisValue {
                status: ResourceRedisStatus::Available,
                kind,
                raw_type: None,
                ttl: ResourceRedisTtl::Seconds(30),
                memory_usage: Some(2048),
                length,
                body,
                elements: Vec::new(),
            };
            entity.host_redis.value_dialog = Some(HostRedisValueDialog {
                connection_id: "local".to_string(),
                endpoint: endpoint.clone(),
                database: 3,
                key: "user:1".to_string(),
                value: Some(value(
                    ResourceRedisValueKind::Hash,
                    "name\nalice\ncache\n42\n".to_string(),
                    Some(4),
                )),
                loading: false,
                error: None,
                json_view: false,
            });
            let view = entity
                .redis_value_dialog_view((1440.0, 900.0))
                .expect("the dialog is open");
            assert!(matches!(
                view.body,
                HostRedisValueBody::Rows { two_column: true }
            ));
            assert_eq!(view.rows.len(), 2);
            assert_eq!(view.rows[0].left, "name");
            assert_eq!(view.rows[0].right.as_deref(), Some("alice"));
            assert_eq!(view.rows[1].left, "cache");
            assert_eq!(view.rows[1].right.as_deref(), Some("42"));
            assert_eq!(view.database, 3);
            assert_eq!(view.kind_label, Some("HASH"));

            // Strings keep one cell per line and stop at the viewer's cap.
            let body = (0..HOST_REDIS_VALUE_MAX_LINES + 25)
                .map(|index| format!("line-{index}"))
                .collect::<Vec<_>>()
                .join("\n");
            entity.host_redis.value_dialog.as_mut().unwrap().value = Some(value(
                ResourceRedisValueKind::String,
                body,
                Some(HOST_REDIS_VALUE_MAX_LINES as u64 + 25),
            ));
            let view = entity
                .redis_value_dialog_view((1440.0, 900.0))
                .expect("the dialog is open");
            assert!(matches!(
                view.body,
                HostRedisValueBody::Rows { two_column: false }
            ));
            assert_eq!(view.rows.len(), HOST_REDIS_VALUE_MAX_LINES);
            assert_eq!(view.omitted_rows, 25);
            assert_eq!(view.rows[0].left, "line-0");
            assert_eq!(view.rows[0].right, None);
        });
    }

    #[test]
    fn databases_start_expanded_and_segments_start_collapsed() {
        let databases = [
            database(0, &["user:1", "user:2", "session:9"]),
            database(3, &["cache:a"]),
        ];
        // Only first-level prefixes show until the user expands a segment.
        assert_eq!(
            rows(&databases, false, &[], &[]),
            vec!["db0", "  session", "  user", "db3", "  cache"]
        );
        // Expanding `user` (index 1 inside db0) reveals its children.
        assert_eq!(
            rows(&databases, false, &[], &["db:0:1"]),
            vec![
                "db0",
                "  session",
                "  user",
                "    1",
                "    2",
                "db3",
                "  cache"
            ]
        );
        // A collapsed database hides its whole subtree.
        assert_eq!(
            rows(&databases, false, &["db:0"], &["db:0:1"]),
            vec!["db0", "db3", "  cache"]
        );
    }

    #[test]
    fn empty_databases_are_hidden_unless_the_filter_shows_all() {
        let databases = [database(0, &["user:1"]), database(1, &[])];
        assert_eq!(rows(&databases, false, &[], &[]), vec!["db0", "  user"]);
        assert_eq!(
            rows(&databases, true, &[], &[]),
            vec!["db0", "  user", "db1"]
        );
    }

    #[test]
    fn a_prefix_that_is_also_a_key_lists_its_own_leaf() {
        // Reported case: `jsm-config` and `jsm-config:version` both exist.
        let databases = [database(
            0,
            &[
                "helphub:config:jsm-config",
                "helphub:config:jsm-config:version",
            ],
        )];
        let expanded = host_redis_tree_rows(
            &databases,
            false,
            &Default::default(),
            &[
                "db:0:0".to_string(),
                "db:0:0:0".to_string(),
                "db:0:0:0:0".to_string(),
            ]
            .into_iter()
            .collect(),
            "",
        );
        let labels = expanded
            .iter()
            .map(|row| (row.depth, row.label.as_str(), row.is_key))
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                (0, "db0", false),
                (1, "helphub", false),
                (2, "config", false),
                (3, "jsm-config", true),
                // The key itself is its own leaf next to the deeper key.
                (4, "helphub:config:jsm-config", true),
                (4, "version", true),
            ]
        );
        // The prefix reports both keys, and expanding it shows both.
        let jsm = &expanded[3];
        assert_eq!(jsm.key_count, 2);
        assert_eq!(expanded[4..].len(), 2);
        // The prefix's own value renders as a `{current}` marker row, not as a
        // repeated full path; the real key name stays on the row for actions.
        assert!(expanded[4].is_current_key);
        assert_eq!(
            expanded[4].key.as_deref(),
            Some("helphub:config:jsm-config")
        );
        assert!(!expanded[5].is_current_key);
    }

    #[test]
    fn key_rows_carry_the_database_they_came_from() {
        // A key name is only unique inside one database; a row that lost its
        // index would read db0 and report "key no longer exists".
        let databases = [database(0, &["a"]), database(9, &["b:1"])];
        let rows = host_redis_tree_rows(
            &databases,
            false,
            &Default::default(),
            &["db:9:0".to_string()].into_iter().collect(),
            "",
        );
        let db0_key = rows
            .iter()
            .find(|row| row.key.as_deref() == Some("a"))
            .expect("db0 key row");
        assert_eq!(db0_key.database, 0);
        let db9_key = rows
            .iter()
            .find(|row| row.key.as_deref() == Some("b:1"))
            .expect("db9 key row");
        assert_eq!(db9_key.database, 9);
    }

    #[test]
    fn key_search_filters_loaded_keys_and_hides_empty_databases() {
        let databases = [
            database(0, &["user:1", "config:app"]),
            database(1, &["user:2"]),
        ];
        let rows = host_redis_tree_rows(
            &databases,
            false,
            &Default::default(),
            &Default::default(),
            "CONFIG",
        );
        // The query is case-insensitive; db1 has no match and disappears even
        // though it holds keys.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "db0");
        assert_eq!(rows[0].key_count, 1);
        assert_eq!(rows[1].label, "config");
        assert!(!rows[1].is_key);

        // Expanding the segment reveals the matching key leaf.
        let rows = host_redis_tree_rows(
            &databases,
            false,
            &Default::default(),
            &["db:0:0".to_string()].into_iter().collect(),
            "config",
        );
        assert!(
            rows.iter()
                .any(|row| row.key.as_deref() == Some("config:app"))
        );
    }

    #[test]
    fn tree_rows_carry_subtree_counts_and_key_markers() {
        let databases = [database(0, &["user:1", "user:profile:2", "user"])];
        let rows = host_redis_tree_rows(
            &databases,
            false,
            &Default::default(),
            &["db:0:0".to_string()].into_iter().collect(),
            "",
        );
        assert_eq!(rows[0].key_count, 3);
        assert!(rows[0].is_database);
        // `user` is both a key and a parent, so it keeps its own leaf inside.
        assert_eq!(rows[1].label, "user");
        assert!(rows[1].is_key);
        assert_eq!(rows[1].key_count, 3);
        assert_eq!(
            rows.iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            vec!["db0", "user", "user", "1", "profile"]
        );
        assert!(rows[2].is_key && !rows[2].expandable);
        assert!(rows[2].is_current_key);
        assert!(!rows[1].is_current_key);
    }
}

/// Keys that belong to a colon-separated prefix: the prefix itself when it is
/// also a key, plus every key below it.
fn redis_prefix_keys(keys: &[String], prefix: &str) -> Vec<String> {
    let nested = format!("{prefix}:");
    keys.iter()
        .filter(|key| key.as_str() == prefix || key.starts_with(&nested))
        .cloned()
        .collect()
}

#[cfg(test)]
mod subtree_tests {
    use super::*;

    #[test]
    fn prefix_selection_includes_the_prefix_key_and_its_descendants() {
        let keys: Vec<String> = ["app", "app:user:1", "app:user:2", "appdata", "other"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(
            redis_prefix_keys(&keys, "app"),
            vec![
                "app".to_string(),
                "app:user:1".to_string(),
                "app:user:2".to_string()
            ]
        );
        assert!(redis_prefix_keys(&keys, "missing").is_empty());
        assert_eq!(
            redis_prefix_keys(&keys, "appdata"),
            vec!["appdata".to_string()]
        );
    }
}

#[cfg(test)]
mod redis_edit_safety_tests {
    use super::*;

    fn value(
        kind: ResourceRedisValueKind,
        elements: Vec<Vec<u8>>,
        length: Option<u64>,
    ) -> ResourceRedisValue {
        let body = elements
            .iter()
            .map(|element| String::from_utf8_lossy(element).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        ResourceRedisValue {
            status: ResourceRedisStatus::Available,
            kind,
            length,
            body,
            elements,
            ..Default::default()
        }
    }

    #[test]
    fn edits_are_refused_when_the_frame_cannot_round_trip() {
        // The write script skips empty members, so the rewrite would drop it.
        assert!(redis_value_edit_unsupported(&value(
            ResourceRedisValueKind::List,
            vec![b"a".to_vec(), Vec::new()],
            Some(2),
        )));
        // A clean list stays editable.
        assert!(!redis_value_edit_unsupported(&value(
            ResourceRedisValueKind::List,
            vec![b"a".to_vec(), b"b".to_vec()],
            Some(2),
        )));
        // A hash row needs its field/value pair.
        assert!(redis_value_edit_unsupported(&value(
            ResourceRedisValueKind::Hash,
            vec![b"field".to_vec()],
            Some(1),
        )));
        // A field carrying the separator would be split on write.
        assert!(redis_value_edit_unsupported(&value(
            ResourceRedisValueKind::Hash,
            vec![b"f\tx".to_vec(), b"v".to_vec()],
            Some(1),
        )));
        // Invalid UTF-8 cannot survive the text editor.
        assert!(redis_value_edit_unsupported(&value(
            ResourceRedisValueKind::Set,
            vec![vec![0xff, 0xfe]],
            Some(1),
        )));
    }

    #[test]
    fn copy_requires_the_whole_value() {
        assert!(!redis_value_copy_is_complete(&value(
            ResourceRedisValueKind::List,
            vec![b"a".to_vec()],
            Some(2),
        )));
        assert!(redis_value_copy_is_complete(&value(
            ResourceRedisValueKind::List,
            vec![b"a".to_vec(), b"b".to_vec()],
            Some(2),
        )));
        assert!(redis_value_copy_is_complete(&value(
            ResourceRedisValueKind::String,
            vec![b"abc".to_vec()],
            Some(3),
        )));
        assert!(!redis_value_copy_is_complete(&value(
            ResourceRedisValueKind::String,
            vec![b"abc".to_vec()],
            Some(9),
        )));
        // Streams have no text rendering to copy.
        assert!(!redis_value_copy_is_complete(&value(
            ResourceRedisValueKind::Stream,
            Vec::new(),
            Some(1),
        )));
    }

    #[test]
    fn json_writes_are_compacted_before_they_are_sent() {
        let endpoint = ResourceRedisEndpoint {
            kind: ResourceRedisEndpointKind::Host,
            target: "127.0.0.1:6379".to_string(),
        };
        let dialog = |text: &str| HostRedisSetValueDialog {
            connection_id: "local".to_string(),
            endpoint: endpoint.clone(),
            database: 0,
            key: "doc".to_string(),
            kind: RedisValueWriteKind::Json,
            base: None,
            value: zeroize::Zeroizing::new(text.to_string()),
            field: zeroize::Zeroizing::new(String::new()),
            entries: Vec::new(),
            error: None,
            loading: false,
        };
        // Formatting is a view concern: what lands in Redis stays compact.
        let Ok(compact) = validate_redis_set_value(&dialog("{\n  \"a\" : [1, 2]\n}")) else {
            panic!("valid JSON must be accepted");
        };
        assert_eq!(compact, "{\"a\":[1,2]}");
        assert!(validate_redis_set_value(&dialog("{ not json }")).is_err());
    }

    #[test]
    fn edited_rows_that_cannot_be_framed_are_rejected() {
        let hash_ok = ["f\tv"].into_iter();
        assert!(!redis_set_value_entries_unsupported(
            RedisValueWriteKind::Hash,
            hash_ok
        ));
        let hash_unpaired = ["f"].into_iter();
        assert!(redis_set_value_entries_unsupported(
            RedisValueWriteKind::Hash,
            hash_unpaired
        ));
        let list_empty = [""].into_iter();
        assert!(redis_set_value_entries_unsupported(
            RedisValueWriteKind::List,
            list_empty
        ));
    }
}
