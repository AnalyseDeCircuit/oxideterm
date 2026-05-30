# Native Plugin API Ledger

This ledger tracks native implementation against
`/Users/dominical/Documents/oxideterm-main/plugin-api.d.ts`.

Status values:

- `done`: implemented and covered by tests.
- `partial`: native has a compatible data model or UI placeholder, but not the
  full API behavior.
- `planned`: not implemented yet, but mapped to a native owner.
- `native-incompatible`: Tauri browser/React/JS behavior that must not execute
  in native.

Capability values are the native permission gates that must exist before the API
can be executable. `read` means the API exposes host state but does not mutate
it. `manifest` means the feature is only driven by `plugin.json`.

## Source Sections

| Source section | Lines | Native owner | Status |
| --- | ---: | --- | --- |
| Manifest contribution types | `plugin-api.d.ts:27-121` | `crates/oxideterm-gpui-app/src/workspace/plugin_host.rs` | partial |
| `PluginManifest` | `plugin-api.d.ts:123-148` | `plugin_host.rs` | partial |
| `PluginModule` lifecycle | `plugin-api.d.ts:155-158` | `plugin_runtime.rs`, `plugin_lifecycle.rs` | partial |
| `Disposable` | `plugin-api.d.ts:164-166` | `plugin_host.rs` runtime contribution cleanup | partial |
| Snapshot types | `plugin-api.d.ts:168-294` | per-namespace snapshot adapters | planned |
| Terminal hook primitives | `plugin-api.d.ts:299-305` | future terminal hook pipeline | planned |
| UI primitive types | `plugin-api.d.ts:311-348` | future native contribution registry | planned |
| SFTP / forwarding types | `plugin-api.d.ts:356-462` | future SFTP/forward adapters | planned |
| Sync and `.oxide` types | `plugin-api.d.ts:465-591` | session manager, Cloud Sync, plugin settings store | partial |
| `PluginContext` | `plugin-api.d.ts:597-854` | future runtime host context | planned |
| `window.__OXIDE__` shared modules | `plugin-api.d.ts:857-875` | unsupported in native | native-incompatible |

## Manifest Ledger

| API | Source | Native owner | Capability | Status | Test requirement | Notes |
| --- | ---: | --- | --- | --- | --- | --- |
| `PluginTabDef` | `27-31` | `plugin_host.rs`, `plugin_ui.rs` | manifest | implemented | `plugin_host::runtime_tab_and_sidebar_views_require_manifest_declarations_and_valid_schema` | Native renders declared tabs through host-owned declarative schemas, not React. |
| `PluginSidebarDef` | `33-38` | `plugin_host.rs`, `plugin_ui.rs` | manifest | implemented | `plugin_host::runtime_tab_and_sidebar_views_require_manifest_declarations_and_valid_schema` | Native panel bodies are declarative schemas only. |
| `PluginSettingDef` | `58-64` | `plugin_host.rs`, future plugin settings store | settings.read/write | partial | setting type/default validation | Phase 2 must render and persist values. |
| `PluginTerminalHooksDef` | `67-71` | future terminal hook pipeline | terminal.send/observe | partial | declaration gate | Hooks cannot run until Phase 5. |
| `ConnectionHookType` | `73` | future event bridge | state.list | planned | subscription declaration gate | Must map to node/connection lifecycle events. |
| `PluginTerminalTransportType` | `75` | future terminal transport registry | network.forward | partial | telnet declaration gate | Only `telnet` is public API. |
| `PluginToolCapability` | `77-91` | future capability checker | manifest | partial | capability parse and denial tests | Must drive API approvals and denials. |
| `PluginToolTargetKind` | `93-101` | future AI tool registry | manifest | partial | target kind parse | Used by OxideSens tool display and routing. |
| `PluginToolRisk` | `103-111` | future AI approval UI | manifest | partial | risk parse | Must not infer destructive permissions silently. |
| `PluginAiToolDef` | `113-121` | future AI tool registry | plugin.invoke | partial | metadata round trip | Phase 2 metadata only; execution waits for runtime. |
| `PluginManifest` | `123-148` | `plugin_host.rs` | manifest | partial | Tauri manifest fixture parse | Native adds optional `runtime` while preserving Tauri fields. |

## Lifecycle Ledger

| API | Source | Native owner | Capability | Status | Test requirement | Notes |
| --- | ---: | --- | --- | --- | --- | --- |
| `activate(ctx)` | `155-157` | `plugin_runtime.rs`, `plugin_lifecycle.rs` | plugin.invoke | partial | activate timeout, crash cleanup | Process runtime activates over JSON Lines; WASM runs bounded WASIp1 `_start`, exposes memory ABI command/event dispatch, and drains outbound protocol frames; preview2 component ABI remains future work. |
| `deactivate()` | `157` | `plugin_runtime.rs` supervisor | plugin.invoke | partial | unload disposes registrations | Process runtime kill/deactivate exists; full manager unload wiring still pending. |
| `Disposable.dispose()` | `164-166` | `plugin_host.rs` runtime contribution registry | plugin.invoke | partial | idempotent disposal | Host owns registration ids and cleanup by plugin id. |

## Package Registry Ledger

| API | Source | Native owner | Capability | Status | Test requirement | Notes |
| --- | ---: | --- | --- | --- | --- | --- |
| registry index | `src-tauri/src/commands/plugin_registry.rs` | `plugin_host.rs` | install/update | implemented | `registry_index_parses_capabilities_summary`; `plugin_package_install_supports_flat_nested_conflict_and_updates` | Native has the Tauri-shaped index/entry model, version comparison, and capabilities summary display. |
| install package/from URL | `install_plugin`, `install_plugin_from_url` | `plugin_host.rs`, `plugin_manager.rs` | install/update | implemented | `plugin_package_install_supports_flat_nested_conflict_and_updates`; `plugin_package_rejects_zip_slip_and_checksum_mismatch_without_replacing_existing`; `plugin_manager_conflict_error_preserves_plugin_id` | Enforces package size, extracted size, entry count, checksum, HTTP(S)-only URL, staging dir, flat or single nested root, id validation, backup rollback, final rename, and Plugin Manager overwrite confirmation. |
| uninstall package | `uninstall_plugin` | `plugin_host.rs`, `plugin_manager.rs` | install/update | implemented | `uninstall_plugin_removes_directory_contributions_and_optional_state` | Removes package directory, disposes native contribution state, and can preserve or remove plugin settings/storage. Plugin Manager exposes preserve-settings uninstall. |

## PluginContext Namespace Ledger

| Namespace | Source | Native owner | Capability | Status | Test requirement | Notes |
| --- | ---: | --- | --- | --- | --- | --- |
| `connections.getAll` | `601-602` | `plugin_lifecycle.rs`, SSH registry snapshot adapter | state.list | implemented | `connections_returnable_host_apis_match_tauri_snapshot_shape` | Returns Tauri-shaped read-only snapshot values; no live handles or pool keys. |
| `connections.get` | `603` | `plugin_lifecycle.rs`, SSH registry snapshot adapter | state.list | implemented | `connections_returnable_host_apis_return_null_for_missing_ids` | Missing ids return `null`; secrets are not included. |
| `connections.getState` | `604` | `plugin_lifecycle.rs`, SSH registry snapshot adapter | state.list | implemented | `connections_returnable_host_apis_match_tauri_snapshot_shape` | Tauri state strings and `{ error }` object remain API-compatible. |
| `connections.getByNode` | `605` | `plugin_lifecycle.rs`, NodeRuntimeStore adapter | state.list | implemented | `connections_returnable_host_apis_match_tauri_snapshot_shape` | Resolves stable node id through runtime connection id, then returns the same connection snapshot. |
| `events.onConnect` | `609-610` | `plugin_host.rs` event subscription registry | state.list | partial | lifecycle event subscription | Placeholder subscription keys are accepted; Phase 5 will attach frozen connection snapshots. |
| `events.onDisconnect` | `611` | `plugin_host.rs` event subscription registry | state.list | partial | unsubscribe disposal | Placeholder subscription keys are accepted; Phase 5 will attach disconnect semantics. |
| `events.onLinkDown` | `612` | `plugin_host.rs` event subscription registry | state.list | partial | link-down event timing | Placeholder subscription keys are accepted; Phase 5 must guard stale worker results. |
| `events.onReconnect` | `613` | `plugin_host.rs` event subscription registry | state.list | partial | reconnect phase event test | Placeholder subscription keys are accepted; Phase 5 will map reconnect success. |
| `events.on` | `614` | `plugin_host.rs`, `plugin_lifecycle.rs` event bus | plugin.invoke | partial | namespace isolation | Runtime subscription registration normalizes custom names to `plugin.{owner}:{event}`. |
| `events.emit` | `615` | `plugin_lifecycle.rs` event bus | plugin.invoke | partial | cross-plugin event policy | Emits only validated plugin-scoped custom events; arbitrary global names are rejected. |
| `ui.registerTabView` | `619-620` | `plugin_lifecycle.rs`, `plugin_host.rs`, `plugin_ui.rs` | navigation.open | implemented | `plugin_host::runtime_tab_and_sidebar_views_require_manifest_declarations_and_valid_schema`; `plugin_lifecycle` | Native accepts a declarative schema payload instead of `React.ComponentType`, validates the declared tab id, and renders through GPUI. |
| `ui.registerSidebarPanel` | `620` | `plugin_lifecycle.rs`, `plugin_host.rs`, `plugin_ui.rs` | navigation.open | implemented | `plugin_host::runtime_tab_and_sidebar_views_require_manifest_declarations_and_valid_schema`; `plugin_lifecycle` | Native accepts a declarative schema payload, validates the declared panel id, and renders it in the Extensions sidebar. |
| `ui.registerCommand` | `621` | command palette contribution registry | plugin.invoke | partial | command appears and dispatches RPC | Runtime command metadata appears in command palette and dispatches to process RPC. |
| `ui.registerContextMenu` | `622` | context menu contribution registry | plugin.invoke | partial | disabled/loading guard | Registration is parsed and render-time predicates are rejected; page menu wiring still pending. |
| `ui.registerStatusBarItem` | `623` | status bar contribution registry | plugin.invoke | partial | update/dispose handle | Registration and dispose are host-owned; status bar surface wiring still pending. |
| `ui.registerKeybinding` | `624` | keybinding registry | plugin.invoke | partial | normalized combo and global route tests | Native preserves Tauri Cmd/Ctrl normalization and only fires after built-ins miss. |
| `ui.openTab` | `625` | `plugin_ui.rs`, `plugin_lifecycle.rs` | navigation.open | implemented | `plugin_lifecycle`; `cargo check -p oxideterm-gpui-app` | Requires a declared tab id and opens or focuses the native plugin tab. |
| `ui.showToast` | `626` | workspace toast host | plugin.invoke | partial | toast variant mapping | Process host call maps to workspace toast. |
| `ui.showConfirm` | `627` | native confirm dialog | plugin.invoke | partial | returnable response + focus/backdrop policy test | Process runtime opens a shared native confirm dialog and resolves `Promise<boolean>`; manual backdrop/focus validation still pending. |
| `ui.showNotification` | `628` | notification center | plugin.invoke | partial | notification scope test | Process host call currently maps to workspace toast; notification center parity pending. |
| `ui.showProgress` | `629` | progress host | plugin.invoke | implemented | `show_progress_returnable_host_api_creates_host_owned_reporter`; `progress_effect_updates_host_owned_toast_payload`; lifecycle compile test | Returnable host call creates a host-owned progress reporter id and outbound `ReportProgress` / sync progress updates refresh the same keyed toast; `done` dismisses the reporter. |
| `ui.getLayout` | `630` | workspace layout adapter | read | partial | layout snapshot test | Returns the Tauri-shaped sidebar/active-tab/tab-count snapshot from the native workspace. |
| `ui.onLayoutChange` | `631` | workspace layout event bridge | read | partial | subscription disposal + event delivery test | Runtime subscriptions map to `ui.layoutChanged`; native emits only when the serialized snapshot shape changes. |
| `terminal.registerShortcut` | `638` | `TerminalShortcut` runtime contribution, keybinding dispatcher | terminal.send | implemented | `runtime_registrations_feed_host_owned_contribution_store_and_cleanup`; `terminal_shortcut_registration_requires_manifest_declaration` | Requires `contributes.terminalHooks.shortcuts`; native uses the declared key and dispatches the declared command through the runtime command RPC after built-ins miss. |
| `terminal.getActiveTarget` | `639` | `plugin_lifecycle.rs`, active pane/node resolver | terminal.observe | implemented | `terminal_readonly_returnable_host_apis_use_node_snapshots` | Includes local vs SSH target shape and projects Rust error state to Tauri `"error"`. |
| `terminal.writeToActive` | `640` | `plugin_lifecycle.rs`, GPUI terminal request bridge | terminal.send | implemented | `terminal_write_host_calls_parse_text_and_node_id`; lifecycle compile test | Returnable host call waits for the GPUI-thread writer and returns `false` for non-active targets. |
| `terminal.writeToNode` | `641` | `plugin_lifecycle.rs`, NodeRuntimeStore + terminal pane writer | terminal.send | implemented | `terminal_write_host_calls_parse_text_and_node_id`; lifecycle compile test | Stable node id lookup rejects non-active/disconnected nodes before writing to the first terminal pane. |
| `terminal.getNodeBuffer` | `642` | `plugin_lifecycle.rs`, GPUI terminal pane snapshot | terminal.observe | implemented | `terminal_readonly_returnable_host_apis_use_node_snapshots` | Reads the node's first terminal pane snapshot without exposing terminal handles. |
| `terminal.getNodeSelection` | `643` | `plugin_lifecycle.rs`, GPUI terminal pane selection snapshot | terminal.observe | implemented | `terminal_readonly_returnable_host_apis_use_node_snapshots` | Returns selected text or `null`; does not mutate selection ownership. |
| `terminal.search` | `644` | `plugin_lifecycle.rs`, terminal snapshot search adapter | terminal.observe | implemented | `terminal_search_scroll_and_size_are_bounded`; `terminal_search_supports_regex_whole_word_and_invalid_regex` | Matches Tauri search option semantics over the live native snapshot, including regex, whole-word literal matching, invalid regex errors, and snake_case match payloads. |
| `terminal.getScrollBuffer` | `645` | `plugin_lifecycle.rs`, GPUI terminal pane snapshot | terminal.observe | implemented | `terminal_search_scroll_and_size_are_bounded` | Returns bounded `{ text, lineNumber }` rows from the node's terminal snapshot. |
| `terminal.getBufferSize` | `646` | `plugin_lifecycle.rs`, GPUI terminal pane snapshot | terminal.observe | implemented | `terminal_search_scroll_and_size_are_bounded` | Returns current/total/max line counts from the native pane snapshot. |
| `terminal.clearBuffer` | `647` | `TerminalSession::clear_buffer`, GPUI terminal request bridge | terminal.send | implemented | `terminal_write_host_calls_parse_text_and_node_id`; lifecycle compile test | Clears native emulator viewport/scrollback on the GPUI pane thread and marks open command marks/facts stale with `TerminalReset`, matching Tauri `clear_buffer` host-side semantics. |
| `terminal.registerInputInterceptor` | `647` | `TerminalInputInterceptor` runtime contribution registry, `TerminalPane` input hook bridge | terminal.send | implemented | `runtime_registrations_feed_host_owned_contribution_store_and_cleanup`; `terminal_hook_registration_requires_manifest_declaration`; `terminal_input_interceptors_run_in_order_and_fail_open`; `terminal_input_interceptor_null_suppresses_input`; lifecycle compile test | Manifest declaration gate, ordered host-owned registration rows, pane synchronization, pre-write input transformation, `null` suppression, and 5ms fail-open runtime dispatch are wired. Host APIs are disabled while a hook runs so UI-thread request queues cannot defeat the input timeout. |
| `terminal.registerOutputProcessor` | `647` | `TerminalOutputProcessor` runtime contribution registry, `TerminalOutputProcessor` backend parser-prelude hook | terminal.observe | implemented | `runtime_registrations_feed_host_owned_contribution_store_and_cleanup`; `terminal_hook_registration_requires_manifest_declaration`; `terminal_output_processors_preserve_bytes_on_failure`; `terminal_output_processor_transforms_and_suppresses_parser_input`; lifecycle compile test | Manifest declaration gate, ordered host-owned registration rows, pane synchronization, local PTY reader-thread processor updates, and SSH/Telnet parser-before byte processing are implemented; timeout/error preserves current bytes. |
| `terminal.openTelnet` | `648-660` | `TelnetSession` backend, `WorkspaceApp::create_telnet_terminal_tab`, terminal host-call bridge | network.forward | implemented | `telnet_codec_filters_negotiation_and_answers_supported_options`; `telnet_codec_escapes_client_iac_bytes`; `terminal_write_host_calls_parse_text_and_node_id`; lifecycle compile test | Requires `contributes.terminalTransports` to include `telnet`, trims and validates host, defaults port to 23, opens a real TCP Telnet session with negotiation/NAWS/terminal-type handling, and returns Tauri-shaped `{ sessionId, info }`. |
| `settings.get` | `664-665` | `plugin_host.rs`, `plugin_lifecycle.rs` returnable host call | settings.read | partial | declared/default lookup | Runtime reads use the same declared default path as manifest-rendered settings. |
| `settings.set` | `666` | `plugin_host.rs`, `plugin_lifecycle.rs` one-way host call | settings.write | partial | type/value validation | Runtime writes use the registry typed writer; change events/export bridge remain. |
| `settings.onChange` | `667` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | settings.read | partial | subscription disposal and scoped mutation event | Plugin-scoped setting changes emit `settings.changed` PluginEvent frames. |
| `settings.exportSyncableSettings` | `668-672` | `plugin_lifecycle.rs` syncable settings adapter | settings.read | partial | revision round trip | Exports Tauri-shaped appearance/terminal/reconnect payload with warnings and FNV-1a revision. |
| `settings.applySyncableSettings` | `673` | `plugin_lifecycle.rs`, settings runtime side-effect bridge | settings.write | partial | warning normalization | Normalizes Tauri-shaped payload, reports warnings, and applies host-normalized settings through native settings side effects. |
| `i18n.t` | `678-679` | `plugin_lifecycle.rs` returnable host call, future plugin locale loader | read | partial | locale fallback test | Native mirrors Tauri's `plugin.{pluginId}.{key}` lookup and raw-key fallback; locale bundle loading still remains. |
| `i18n.getLanguage` | `680` | `plugin_lifecycle.rs` host snapshot | read | partial | language snapshot test | Returns the native settings language string. |
| `i18n.onLanguageChange` | `681` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | read | partial | subscription disposal | Settings-driven language changes emit `i18n.languageChanged` PluginEvent frames. |
| `storage.get` | `685-686` | `plugin_host.rs` plugin KV store, `plugin_runtime.rs`, `plugin_lifecycle.rs` returnable host call | read | partial | SDK contract and end-to-end demo plugin | Registry read, process response transport, permission gate, and Workspace resolver are wired. |
| `storage.set` | `687` | `plugin_host.rs`, `plugin_lifecycle.rs` | settings.write | partial | size limit test | Plugin-scoped JSON write persists with per-plugin size limit; process host call is one-way. |
| `storage.remove` | `688` | `plugin_host.rs`, `plugin_lifecycle.rs` | settings.write | partial | delete test | Plugin-scoped delete persists and is exposed as a one-way process host call. |
| `sync.*` | `692-738` | `plugin_lifecycle.rs`, `ConnectionStore`, `ForwardingRegistry`, Cloud Sync, `.oxide` services | settings.read/write | implemented | `sync_host_call_returns_saved_connection_snapshots_and_metadata`; `sync_apply_saved_connections_args_parse_snapshot_and_strategy`; `sync_oxide_host_calls_export_validate_and_preview_without_workspace_mutation`; `sync_import_oxide_args_and_core_import_match_tauri_defaults`; `sync_plugin_settings_export_filters_selected_plugins_and_revisions`; `.oxide` round trip | `listSavedConnections`, `refreshSavedConnections`, `exportSavedConnectionsSnapshot`, and `getLocalSyncMetadata` return Workspace-owned saved connection/local metadata snapshots. `applySavedConnectionsSnapshot` commits through the Workspace mutation bridge with native conflict strategy parsing. `.oxide` `preflightExport`, `exportOxide`, `validateOxide`, `previewImport`, and `importOxide` use the shared native codec; plugin-driven `.oxide` export excludes managed SSH keys unless `includeManagedKeys` is explicitly set. Mutating imports commit through the Workspace bridge. Selected plugin settings import/export follows existing `.oxide` storage-key filtering rules, and `.oxide` progress updates use host-owned progress reporters. |
| `secrets.*` | `742-748` | `plugin_lifecycle.rs`, `AiProviderKeyStore` OS keychain adapter | credential-sensitive | implemented | `plugin_secret_account_ids_are_plugin_scoped_and_validated`; lifecycle compile test | Implements `get`, `getMany`, `set`, `has`, and `delete` with Tauri-compatible `plugin-secret:{pluginLen}:{pluginId}:{keyLen}:{key}` account ids; empty `set` deletes, secret write temporaries use `Zeroizing<String>`, and secret values are not logged. |
| `api.invoke` | `751-753` | `plugin_lifecycle.rs`, manifest `apiCommands` contribution registry, NodeRouter, ForwardingRegistry, SFTP transfer manager, plugin HTTP proxy | plugin.invoke plus target API capabilities | implemented | `api_invoke_rejects_undeclared_commands_and_runs_supported_whitelisted_commands`; `api_invoke_native_adapters_cover_system_transfer_and_capability_paths`; lifecycle compile test | Enforces the same manifest `contributes.apiCommands` whitelist as Tauri, then dispatches only commands with explicit native adapters. Covered commands include `ssh_get_pool_stats`, `list_connections`, app/system info, documented `node_sftp_*` file/directory/tar operations, transfer queue controls/stats, documented port-forward controls, and binary-safe `plugin_http_request`. Target adapters still enforce native filesystem/forward capability checks, and declared-but-unsupported commands fail closed. |
| `assets.loadCSS` | `756-757` | unsupported native CSS path | none | native-incompatible | CSS injection denied | No global CSS injection in GPUI. |
| `assets.getAssetUrl` | `758` | future safe asset handle | filesystem.read | planned | path validation | Return host asset handle, not browser blob URL. |
| `assets.revokeAssetUrl` | `759` | future safe asset handle | filesystem.read | planned | idempotent revoke | Host-owned asset ids. |
| `sftp.*` | `763-771` | `plugin_lifecycle.rs`, `NodeRouter` SFTP owner | filesystem.read/write | implemented | `sftp_host_call_args_reject_missing_or_invalid_paths`; `sftp_host_calls_require_matching_filesystem_capability`; lifecycle compile test | Implements `listDir`, `stat`, `readFile`, `writeFile`, `mkdir`, `delete`, and `rename` through true NodeRouter-owned SFTP sessions. Read operations retry once after recoverable channel errors; mutating operations require `filesystem.write`, use the shared SFTP owner directly, and invalid node/path arguments are rejected before acquisition. |
| `forward.*` | `774-783` | `plugin_lifecycle.rs`, `plugin_host.rs`, `ForwardingRegistry` adapter | network.forward | partial | `forward_host_calls_require_network_forward_capability`; `forward_create_request_accepts_tauri_camel_case_shape`; `forward_rule_snapshot_matches_plugin_forward_rule_shape`; `plugin_host`; lifecycle compile test | Direct `list`, `listSavedForwards`, `exportSavedForwardsSnapshot`, `applySavedForwardsSnapshot`, `create`, `stop`, `stopAll`, and `getStats` host calls are wired for existing forwarding managers with Tauri-shaped rule/stat snapshots and `network.forward` capability checks. `onSavedForwardsChange` maps to `forward.savedForwardsChanged` and emits saved-forward snapshots when they change. Manager creation for sessions without an existing forwarding owner remains pending to avoid leaking SSH consumers outside the Workspace owner table. |
| `sessions.getTree` | `787-788` | `plugin_lifecycle.rs`, `NodeRuntimeStore` snapshot adapter | state.list | implemented | `sessions_returnable_host_apis_match_tauri_snapshot_shape` | Returns Tauri-shaped frozen node tree projection with native title/terminal maps. |
| `sessions.getActiveNodes` | `789` | `plugin_lifecycle.rs`, node tree snapshot adapter | state.list | implemented | `sessions_returnable_host_apis_match_tauri_snapshot_shape` | Mirrors Tauri filter for `active` and `connected` runtime statuses. |
| `sessions.getNodeState` | `790` | `plugin_lifecycle.rs`, node tree snapshot adapter | state.list | implemented | `sessions_returnable_host_apis_return_null_for_missing_node` | Missing nodes return `null`; native readiness is projected to Tauri status strings. |
| `sessions.onTreeChange` | `791` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | state.list | implemented | `plugin_host`, `plugin_lifecycle` subscription/event tests | Emits `sessions.treeChanged` PluginEvent frames when the serialized tree changes. |
| `sessions.onNodeStateChange` | `792` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | state.list | implemented | `plugin_host`, `plugin_lifecycle` subscription/event tests | Emits `sessions.nodeStateChanged` with `{ nodeId, state }`; removed nodes map to `idle`. |
| `transfers.*` | `796-801` | `plugin_lifecycle.rs`, `plugin_host.rs`, `SftpTransferManager` | state.list | implemented | `transfers_host_calls_return_tauri_snapshot_shape_and_filter_by_node`; `transfer_state_helpers_detect_complete_and_error_transitions`; `plugin_host`; lifecycle compile test | Implements `getAll` and `getByNode` from backend-owned SFTP background transfer snapshots using the Tauri `TransferSnapshot` projection. `onProgress`, `onComplete`, and `onError` map to runtime event subscriptions; progress is throttled to 500ms, and complete/error events fire on state transitions. |
| `profiler.*` | `805-810` | `plugin_lifecycle.rs`, `plugin_host.rs`, `ProfilerRegistry`, `NodeRuntimeStore` | state.list | implemented | `profiler_host_calls_map_node_ids_to_tauri_metrics_shape`; `profiler_history_limits_and_subscription_filters_are_node_scoped`; `plugin_host`; lifecycle compile test | Implements `getMetrics`, `getHistory`, and `isRunning` by mapping plugin node ids to profiler connection ids and returning the Tauri `ProfilerMetricsSnapshot` projection. `onMetrics` maps to `profiler.metrics`; host delivery is node-filtered and throttled to 1s. |
| `eventLog.getEntries` | `813-814` | `plugin_lifecycle.rs`, native notification center event log | state.list | implemented | `event_log_get_entries_filters_tauri_snapshot_shape` | Returns Tauri-shaped `{ id, timestamp, severity, category, ... }` snapshots with severity/category filters. |
| `eventLog.onEntry` | `815` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | state.list | implemented | `plugin_host`, `plugin_lifecycle` event bridge tests | Emits `eventLog.entry` PluginEvent frames for newly appended log entries only. |
| `ide.*` | `819-827` | `oxideterm-gpui-ide` plugin snapshot, `plugin_lifecycle.rs`, `plugin_host.rs` | state.list | implemented | `ide_host_calls_return_project_open_files_and_active_file`; `ide_file_maps_detect_open_close_and_active_changes`; `plugin_host`; lifecycle compile test | Implements `isOpen`, `getProject`, `getOpenFiles`, and `getActiveFile` from the active native IDE surface without exposing file contents, tree internals, agent process state, or reconnect metadata. `onFileOpen`, `onFileClose`, and `onActiveFileChange` map to runtime events by diffing host-owned IDE snapshots. |
| `ai.*` | `830-836` | `plugin_lifecycle.rs`, `plugin_host.rs`, `AiChatState`, provider settings | state.list | implemented | `ai_host_calls_return_sanitized_messages_and_provider_info`; `ai_new_message_events_omit_message_content`; `plugin_host`; lifecycle compile test | Implements `getConversations`, `getMessages`, `getActiveProvider`, and `getAvailableModels` from host-owned AI chat/settings snapshots. Message content is passed through the existing `sanitize_for_ai` policy, tool-role messages are not exposed to the plugin API, and `onMessage` emits metadata-only `{ conversationId, messageId, role }` runtime events. |
| `app.getTheme` | `840` | `plugin_lifecycle.rs` host snapshot | read | partial | theme snapshot test | Returns Tauri-shaped `{ name, isDark }`; theme-change subscription remains. |
| `app.getSettings` | `841` | `plugin_lifecycle.rs` settings snapshot | read | partial | category snapshot test | Returns a read-only JSON section by category; settings-change subscription remains. |
| `app.getVersion` | `842` | `plugin_lifecycle.rs` host snapshot | read | partial | version snapshot test | Returns native Cargo package version instead of `window.__OXIDE__`. |
| `app.getPlatform` | `843` | `plugin_lifecycle.rs` host snapshot | read | partial | platform mapping test | Uses compile-time native target mapping. |
| `app.getLocale` | `844` | `plugin_lifecycle.rs` host snapshot | read | partial | locale snapshot test | Returns the native settings language string. |
| `app.onThemeChange` | `845` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | read | partial | subscription disposal | Settings-driven theme changes emit `app.themeChanged` PluginEvent frames. |
| `app.onSettingsChange` | `846` | `plugin_host.rs`, `plugin_lifecycle.rs` event subscription bridge | read | partial | category subscription test | Settings mutations through native settings edit path emit `app.settingsChanged` snapshots. |
| `app.getPoolStats` | `847` | `plugin_lifecycle.rs`, SSH registry monitor stats | read | partial | compact pool stats test | Returns Tauri-shaped `{ activeConnections, totalSessions }`. |
| `app.refreshAfterExternalSync` | `848-851` | `plugin_lifecycle.rs`, settings/connections reload bridge | settings.read/write | partial | refresh side-effect test | Reloads native settings and saved connections from disk, reapplies runtime settings side effects, and queues Cloud Sync dirty refresh; forwarding manager reload remains future work. |

## Native-Incompatible Browser Primitives

| Tauri primitive | Source | Native replacement | Status | Notes |
| --- | ---: | --- | --- | --- |
| Dynamic ESM import of `main.js` | `PluginModule`, `pluginLoader.ts` | WASM/process runtime bridge | native-incompatible | Native must not evaluate plugin JS. |
| `React.ComponentType` tab/sidebar views | `plugin-api.d.ts:619-620` | declarative native UI schema | native-incompatible | Host renders GPUI controls. |
| `window.__OXIDE__.React` | `plugin-api.d.ts:862` | none | native-incompatible | No shared browser module object. |
| `window.__OXIDE__.ReactDOM` | `plugin-api.d.ts:863` | none | native-incompatible | No DOM root. |
| `window.__OXIDE__.zustand` | `plugin-api.d.ts:864` | plugin storage/settings RPC | native-incompatible | Runtime cannot mutate host UI state directly. |
| `window.__OXIDE__.lucideIcons` | `plugin-api.d.ts:865` | manifest icon names mapped by host | native-incompatible | Host resolves icons. |
| `window.__OXIDE__.ui` | `plugin-api.d.ts:867` | declarative native schema controls | native-incompatible | No component injection. |
| `window.__OXIDE__.clsx/cn` | `plugin-api.d.ts:868-869` | none | native-incompatible | Styling is host-owned. |
| `window.__OXIDE__.useTranslation` | `plugin-api.d.ts:870` | `ctx.i18n.t` RPC | native-incompatible | No React hooks. |
| CSS asset injection | `assets.loadCSS` | unsupported or scoped theme schema | native-incompatible | GPUI styling cannot accept arbitrary CSS. |

## Verification Gate For Phase 0

Phase 0 is complete when:

- every `PluginContext` namespace above has an owner, capability, state, and
  test requirement;
- every browser-only primitive is explicitly marked native-incompatible;
- `docs/native-plugin-system-plan.md` points future implementation at this
  ledger instead of relying only on memory.
