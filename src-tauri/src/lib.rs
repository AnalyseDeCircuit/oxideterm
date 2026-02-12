//! OxideTerm - A modern SSH terminal client
//!
//! Built with Rust, Tauri, and xterm.js for high-performance terminal emulation.

// Use mimalloc as the global allocator for better performance
// with high-frequency small allocations (WebSocket frames, scroll buffer, etc.)
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod bridge;
pub mod commands;
pub mod config;
pub mod forwarding;
pub mod graphics;
#[cfg(feature = "local-terminal")]
pub mod local;
pub mod launcher;
pub mod oxide_file;
pub mod terminal_bg;
pub mod router;
pub mod session;
pub mod sftp;
pub mod ssh;
pub mod state;

// Windows: 高精度系统定时器
#[cfg(target_os = "windows")]
mod windows_timer {
    use std::sync::atomic::{AtomicBool, Ordering};

    static TIMER_ENABLED: AtomicBool = AtomicBool::new(false);

    #[link(name = "winmm")]
    extern "C" {
        // Windows Multimedia API
        fn timeBeginPeriod(uPeriod: u32) -> u32;
        fn timeEndPeriod(uPeriod: u32) -> u32;
    }

    pub fn enable_high_precision_timer() -> Result<(), String> {
        if TIMER_ENABLED.load(Ordering::Relaxed) {
            tracing::debug!("High-precision timer already enabled");
            return Ok(());
        }

        unsafe {
            // 设置系统定时器精度为 1ms
            // 返回 0 表示成功 (TIMERR_NOERROR)
            let result = timeBeginPeriod(1);
            if result == 0 {
                TIMER_ENABLED.store(true, Ordering::Relaxed);
                tracing::info!("✅ Windows high-precision timer enabled (1ms precision)");
                Ok(())
            } else {
                let error_code = result as u32;
                let msg = format!("timeBeginPeriod failed with error code: {}", error_code);
                tracing::error!("❌ {}", msg);
                Err(msg)
            }
        }
    }

    pub fn disable_high_precision_timer() {
        if TIMER_ENABLED.load(Ordering::Relaxed) {
            unsafe {
                timeEndPeriod(1);
                TIMER_ENABLED.store(false, Ordering::Relaxed);
                tracing::info!("Windows high-precision timer disabled");
            }
        }
    }
}

#[cfg(target_os = "windows")]
use windows_timer::{disable_high_precision_timer, enable_high_precision_timer};

use bridge::BridgeManager;
use commands::config::ConfigState;
use commands::plugin_server::PluginFileServer;
use commands::session_tree::SessionTreeState;
use commands::HealthRegistry;
use session::{AutoReconnectService, SessionRegistry};
use sftp::session::SftpRegistry;
use sftp::{ProgressStore, RedbProgressStore, TransferManager};
use ssh::SshConnectionRegistry;
use state::ai_chat::AiChatStore;
use state::StateStore;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Write startup log to file (useful for debugging Windows startup issues)
fn write_startup_log(message: &str) {
    if let Ok(log_dir) = config::storage::log_dir() {
        // Ensure log directory exists
        let _ = std::fs::create_dir_all(&log_dir);

        let log_file = log_dir.join("startup.log");
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_file) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, message);
        }
    }
}

/// Initialize logging
fn init_logging() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Show error dialog on Windows when startup fails
#[cfg(windows)]
fn show_startup_error(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    let title: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let message: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();

    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(
                hwnd: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                type_: u32,
            ) -> i32;
        }
        MessageBoxW(null_mut(), message.as_ptr(), title.as_ptr(), 0x10); // MB_ICONERROR
    }
}

#[cfg(not(windows))]
fn show_startup_error(_title: &str, _message: &str) {
    // No-op on non-Windows platforms
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    write_startup_log("OxideTerm starting...");

    init_logging();

    tracing::info!("Starting OxideTerm...");
    write_startup_log("Logging initialized");

    // Initialize state store
    let state_db_path = match config::storage::config_dir() {
        Ok(dir) => dir.join("state.redb"),
        Err(e) => {
            let msg = format!("Failed to get config directory: {}", e);
            tracing::error!("{}", msg);
            write_startup_log(&msg);
            show_startup_error("OxideTerm Startup Error", &msg);
            return;
        }
    };

    write_startup_log(&format!("State DB path: {:?}", state_db_path));

    let state_store = match StateStore::new(state_db_path.clone()) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            let msg = format!(
                "Failed to initialize state store at {:?}: {}",
                state_db_path, e
            );
            tracing::error!("{}", msg);
            write_startup_log(&msg);
            show_startup_error("OxideTerm Startup Error", &msg);
            return;
        }
    };

    write_startup_log("State store initialized");

    // Initialize AI chat store for conversation persistence
    let ai_chat_db_path = match config::storage::config_dir() {
        Ok(dir) => dir.join("chat_history.redb"),
        Err(e) => {
            let msg = format!("Failed to get config directory for AI chat: {}", e);
            tracing::warn!("{}", msg);
            write_startup_log(&msg);
            // Continue without AI chat persistence - not critical
            std::path::PathBuf::from("")
        }
    };

    let ai_chat_store = if !ai_chat_db_path.as_os_str().is_empty() {
        match AiChatStore::new(ai_chat_db_path.clone()) {
            Ok(store) => {
                tracing::info!("AI chat store initialized at {:?}", ai_chat_db_path);
                write_startup_log(&format!(
                    "AI chat store initialized at {:?}",
                    ai_chat_db_path
                ));
                Some(Arc::new(store))
            }
            Err(e) => {
                let msg = format!(
                    "Failed to initialize AI chat store at {:?}: {}. AI chat persistence disabled.",
                    ai_chat_db_path, e
                );
                tracing::warn!("{}", msg);
                write_startup_log(&msg);
                None
            }
        }
    } else {
        None
    };

    // Create shared session registry with state store
    let registry = Arc::new(SessionRegistry::new(state_store.clone()));

    // Create forwarding registry with state store (Arc for sharing with reconnect service)
    let forwarding_registry = Arc::new(commands::ForwardingRegistry::new_with_state(
        state_store.clone(),
    ));

    // Create health registry
    let health_registry = HealthRegistry::new();

    // Create SFTP registry
    let sftp_registry = Arc::new(SftpRegistry::new());

    // Create SSH connection registry (connection pool)
    let ssh_connection_registry = Arc::new(SshConnectionRegistry::new());

    // Create progress store for transfer resume
    let progress_store = match RedbProgressStore::default_path() {
        Ok(path) => {
            match RedbProgressStore::new(&path) {
                Ok(store) => Arc::new(store),
                Err(e) => {
                    tracing::warn!(
                        "Failed to create progress store at {:?}: {}. Resume disabled.",
                        path,
                        e
                    );
                    // Create a no-op store that doesn't persist
                    Arc::new(crate::sftp::progress::DummyProgressStore) as Arc<dyn ProgressStore>
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to get progress store path: {}. Resume disabled.", e);
            // Create a no-op store
            Arc::new(crate::sftp::progress::DummyProgressStore) as Arc<dyn ProgressStore>
        }
    };

    // Create transfer manager for concurrent transfer control
    let transfer_manager = Arc::new(TransferManager::new());

    // Create session tree state for dynamic jump host support
    let session_tree_state = Arc::new(SessionTreeState::new());

    // Oxide-Next Phase 2: 创建 NodeEventEmitter（共享实例）
    let node_event_emitter = Arc::new(router::NodeEventEmitter::new());

    // 注入 emitter 到 SshConnectionRegistry
    ssh_connection_registry.set_node_event_emitter(node_event_emitter.clone());

    // Create NodeRouter — Oxide-Next Phase 2
    // 节点路由器：nodeId → 后端资源解析 + 共享 emitter
    let node_router = Arc::new(router::NodeRouter::new(
        session_tree_state.clone(),
        ssh_connection_registry.clone(),
        registry.clone(),
        node_event_emitter.clone(),
    ));

    // Create local terminal state (only when feature enabled)
    #[cfg(feature = "local-terminal")]
    let local_terminal_state = Arc::new(commands::local::LocalTerminalState::new());

    // Create WSL graphics state (only on Windows with feature enabled)
    #[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
    let wsl_graphics_state = Arc::new(graphics::WslGraphicsState::new());

    write_startup_log("All registries initialized, building Tauri app...");

    // Windows: 设置 panic hook，确保异常退出时也能清理高精度定时器
    #[cfg(target_os = "windows")]
    {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // 在 panic 时强制清理高精度定时器，避免影响系统
            disable_high_precision_timer();
            // 继续正常的 panic 处理（打印堆栈等）
            default_hook(panic_info);
        }));
    }

    // Windows: 启用高精度定时器（必须在所有其他初始化之前）
    #[cfg(target_os = "windows")]
    if let Err(e) = enable_high_precision_timer() {
        tracing::warn!(
            "⚠️  Failed to enable high-precision timer: {}. Tokio scheduling may be less precise.",
            e
        );
        write_startup_log(&format!(
            "WARNING: Failed to enable high-precision timer: {}",
            e
        ));
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(BridgeManager::new())
        .manage(registry.clone())
        .manage(forwarding_registry.clone())
        .manage(health_registry)
        .manage(commands::ProfilerRegistry::new())
        .manage(sftp_registry)
        .manage(transfer_manager)
        .manage(progress_store)
        .manage(ssh_connection_registry.clone())
        .manage(session_tree_state)
        .manage(node_router)
        .manage(node_event_emitter.clone())
        .manage(Arc::new(PluginFileServer::new()));

    // Conditionally add AI chat store (may be None if initialization failed)
    let builder = if let Some(ai_store) = ai_chat_store {
        builder.manage(ai_store)
    } else {
        builder
    };

    // Conditionally add local terminal state
    #[cfg(feature = "local-terminal")]
    let builder = builder.manage(local_terminal_state);

    // Conditionally add WSL graphics state
    #[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
    let builder = builder.manage(wsl_graphics_state);

    let builder = builder.setup(move |app| {
        // Initialize config state synchronously (blocking)
        tracing::info!("Initializing config state...");
        write_startup_log("Initializing config state...");

        match tauri::async_runtime::block_on(ConfigState::new()) {
            Ok(config_state) => {
                app.manage(Arc::new(config_state));
                tracing::info!("Config state initialized successfully");
                write_startup_log("Config state initialized successfully");
            }
            Err(e) => {
                let msg = format!("Failed to initialize config state: {}", e);
                tracing::error!("{}", msg);
                write_startup_log(&msg);
                return Err(e.into());
            }
        }

        // Set AppHandle for SSH connection registry (for event broadcasting)
        {
            let registry = ssh_connection_registry.clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                registry.set_app_handle(handle).await;
                tracing::info!("SSH connection registry app handle set");
            });
        }

        // Oxide-Next Phase 2: Set AppHandle for NodeEventEmitter
        node_event_emitter.set_app_handle(app.handle().clone());

        // Initialize auto reconnect service
        let reconnect_service = Arc::new(AutoReconnectService::new(
            registry.clone(),
            forwarding_registry.clone(),
            app.handle().clone(),
        ));
        app.manage(reconnect_service);
        tracing::info!("Auto reconnect service initialized");

        write_startup_log("Tauri setup complete");
        Ok(())
    });

    // Register all commands - use cfg to conditionally include local terminal commands
    #[cfg(feature = "local-terminal")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        // Local terminal commands (feature-gated)
        commands::local_list_shells,
        commands::local_get_default_shell,
        commands::local_create_terminal,
        commands::local_close_terminal,
        commands::local_resize_terminal,
        commands::local_list_terminals,
        commands::local_write_terminal,
        commands::local_get_terminal_info,
        commands::local_cleanup_dead_sessions,
        commands::local_get_drives,
        commands::local_get_file_metadata,
        commands::local_read_file_range,
        commands::local_calculate_checksum,
        commands::local_dir_stats,
        commands::allow_asset_file,
        commands::revoke_asset_file,
        commands::get_audio_metadata,
        // Session commands (v2 with registry)
        commands::disconnect_v2,
        commands::list_sessions_v2,
        commands::get_session_stats,
        commands::get_session,
        commands::resize_session_v2,
        commands::reorder_sessions,
        commands::check_ssh_keys,
        commands::is_ssh_agent_available,
        commands::restore_sessions,
        commands::list_persisted_sessions,
        commands::delete_persisted_session,
        // SSH connection pool commands (new architecture)
        commands::establish_connection,
        commands::list_connections,
        commands::disconnect_connection,
        commands::ssh_disconnect,
        commands::ssh_list_connections,
        commands::ssh_set_keep_alive,
        commands::ssh_get_pool_config,
        commands::ssh_set_pool_config,
        commands::ssh_get_pool_stats,
        commands::create_terminal,
        commands::close_terminal,
        commands::recreate_terminal_pty,
        // SSH host key preflight (TOFU)
        commands::ssh_preflight,
        commands::ssh_accept_host_key,
        commands::ssh_clear_host_key_cache,
        // Remote environment detection
        commands::get_remote_env,
        // Scroll buffer commands
        commands::get_scroll_buffer,
        commands::get_buffer_stats,
        commands::clear_buffer,
        commands::get_all_buffer_lines,
        // Search commands
        commands::search_terminal,
        commands::scroll_to_line,
        // Session tree commands (dynamic jump host)
        commands::get_session_tree,
        commands::get_session_tree_summary,
        commands::add_root_node,
        commands::tree_drill_down,
        commands::expand_manual_preset,
        // Auto-route commands (auto-generated from saved connections)
        commands::get_topology_nodes,
        commands::get_topology_edges,
        commands::get_topology_edges_overlay,
        commands::add_topology_edge,
        commands::remove_topology_edge,
        commands::exclude_topology_edge,
        commands::expand_auto_route,
        // Session tree node management
        commands::update_tree_node_state,
        commands::set_tree_node_connection,
        commands::set_tree_node_terminal,
        commands::set_tree_node_sftp,
        commands::remove_tree_node,
        commands::get_tree_node,
        commands::get_tree_node_path,
        commands::clear_session_tree,
        commands::connect_tree_node,
        commands::disconnect_tree_node,
        commands::connect_manual_preset,
        commands::destroy_node_sessions,
        // Config commands
        commands::config::get_connections,
        commands::config::get_recent_connections,
        commands::config::get_connections_by_group,
        commands::config::search_connections,
        commands::config::get_groups,
        commands::config::save_connection,
        commands::config::delete_connection,
        commands::config::mark_connection_used,
        commands::config::get_connection_password,
        commands::config::get_saved_connection_for_connect,
        commands::config::list_ssh_config_hosts,
        commands::config::import_ssh_host,
        commands::config::get_ssh_config_path,
        commands::config::create_group,
        commands::config::delete_group,
        // AI API key commands
        commands::config::set_ai_api_key,
        commands::config::get_ai_api_key,
        commands::config::has_ai_api_key,
        commands::config::delete_ai_api_key,
        // AI Provider API key commands
        commands::config::set_ai_provider_api_key,
        commands::config::get_ai_provider_api_key,
        commands::config::has_ai_provider_api_key,
        commands::config::delete_ai_provider_api_key,
        commands::config::list_ai_provider_keys,
        // Oxide file export/import commands
        commands::oxide_export::export_to_oxide,
        commands::oxide_export::preflight_export,
        commands::oxide_import::validate_oxide_file,
        commands::oxide_import::preview_oxide_import,
        commands::oxide_import::import_from_oxide,
        // Port forwarding commands
        commands::create_port_forward,
        commands::stop_port_forward,
        commands::list_port_forwards,
        commands::pause_port_forwards,
        commands::restore_port_forwards,
        commands::forward_jupyter,
        commands::forward_tensorboard,
        commands::forward_vscode,
        commands::stop_all_forwards,
        commands::delete_port_forward,
        commands::restart_port_forward,
        commands::update_port_forward,
        commands::get_port_forward_stats,
        commands::list_saved_forwards,
        commands::set_forward_auto_start,
        commands::delete_saved_forward,
        // Health check commands
        commands::get_connection_health,
        commands::get_quick_health,
        commands::get_all_health_status,
        commands::get_health_for_display,
        commands::simulate_health_response,
        // Resource profiler commands
        commands::start_resource_profiler,
        commands::stop_resource_profiler,
        commands::get_resource_metrics,
        commands::get_resource_history,
        // IDE mode commands
        commands::ide_open_project,
        commands::ide_check_file,
        commands::ide_batch_stat,
        commands::ide_exec_command,
        // SFTP transfer control commands (node-independent)
        commands::sftp_cancel_transfer,
        commands::sftp_pause_transfer,
        commands::sftp_resume_transfer,
        commands::sftp_transfer_stats,
        commands::sftp_update_settings,
        // Network and reconnect commands
        commands::network_status_changed,
        commands::cancel_reconnect,
        commands::is_reconnecting,
        // Keyboard-Interactive (2FA) commands - completely isolated from connect_v2
        commands::ssh_connect_kbi,
        commands::ssh_kbi_respond,
        commands::ssh_kbi_cancel,
        // Archive commands (compression/extraction)
        commands::compress_files,
        commands::extract_archive,
        commands::list_archive_contents,
        // AI chat persistence commands
        commands::ai_chat_list_conversations,
        commands::ai_chat_get_conversation,
        commands::ai_chat_create_conversation,
        commands::ai_chat_update_conversation,
        commands::ai_chat_delete_conversation,
        commands::ai_chat_save_message,
        commands::ai_chat_update_message,
        commands::ai_chat_delete_messages_after,
        commands::ai_chat_clear_all,
        commands::ai_chat_get_stats,
        // Plugin system commands
        commands::list_plugins,
        commands::read_plugin_file,
        commands::save_plugin_config,
        commands::load_plugin_config,
        // Plugin file server commands
        commands::start_plugin_server,
        commands::get_plugin_server_port,
        commands::stop_plugin_server,
        // Plugin registry commands (remote install)
        commands::fetch_plugin_registry,
        commands::install_plugin,
        commands::uninstall_plugin,
        commands::check_plugin_updates,
        // Oxide-Next: node-first commands (Phase 0)
        commands::node_get_state,
        commands::node_sftp_init,
        commands::node_sftp_list_dir,
        commands::node_sftp_stat,
        commands::node_sftp_preview,
        commands::node_sftp_write,
        commands::node_sftp_download,
        commands::node_sftp_upload,
        commands::node_sftp_delete,
        commands::node_sftp_mkdir,
        commands::node_sftp_rename,
        commands::node_terminal_url,
        // Oxide-Next: Phase 4 补全命令
        commands::node_sftp_delete_recursive,
        commands::node_sftp_download_dir,
        commands::node_sftp_upload_dir,
        commands::node_sftp_preview_hex,
        commands::node_sftp_list_incomplete_transfers,
        commands::node_sftp_resume_transfer,
        commands::node_ide_open_project,
        commands::node_ide_exec_command,
        commands::node_ide_check_file,
        commands::node_ide_batch_stat,
        // Oxide-Next: node-first forwarding commands
        commands::node_list_forwards,
        commands::node_create_forward,
        commands::node_stop_forward,
        commands::node_delete_forward,
        commands::node_restart_forward,
        commands::node_update_forward,
        commands::node_get_forward_stats,
        commands::node_stop_all_forwards,
        commands::node_forward_jupyter,
        commands::node_forward_tensorboard,
        commands::node_forward_vscode,
        commands::node_list_saved_forwards,
        // WSL Graphics commands (stub on non-Windows platforms)
        graphics::commands::wsl_graphics_list_distros,
        graphics::commands::wsl_graphics_start,
        graphics::commands::wsl_graphics_stop,
        graphics::commands::wsl_graphics_reconnect,
        graphics::commands::wsl_graphics_list_sessions,
        graphics::commands::wsl_graphics_detect_wslg,
        graphics::commands::wsl_graphics_start_app,
        // Platform launcher commands
        launcher::launcher_list_apps,
        launcher::launcher_launch_app,
        launcher::launcher_wsl_launch,
        launcher::launcher_clear_cache,
        // Terminal background image commands
        terminal_bg::upload_terminal_background,
        terminal_bg::list_terminal_backgrounds,
        terminal_bg::delete_terminal_background,
        terminal_bg::clear_terminal_background,
        terminal_bg::init_terminal_background,
    ]);
    #[cfg(not(feature = "local-terminal"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        // Session commands (v2 with registry)
        commands::disconnect_v2,
        commands::list_sessions_v2,
        commands::get_session_stats,
        commands::get_session,
        commands::resize_session_v2,
        commands::reorder_sessions,
        commands::check_ssh_keys,
        commands::is_ssh_agent_available,
        commands::restore_sessions,
        commands::list_persisted_sessions,
        commands::delete_persisted_session,
        // SSH connection pool commands (new architecture)
        commands::establish_connection,
        commands::list_connections,
        commands::disconnect_connection,
        commands::ssh_disconnect,
        commands::ssh_list_connections,
        commands::ssh_set_keep_alive,
        commands::ssh_get_pool_config,
        commands::ssh_set_pool_config,
        commands::ssh_get_pool_stats,
        commands::create_terminal,
        commands::close_terminal,
        commands::recreate_terminal_pty,
        // SSH host key preflight (TOFU)
        commands::ssh_preflight,
        commands::ssh_accept_host_key,
        commands::ssh_clear_host_key_cache,
        // Remote environment detection
        commands::get_remote_env,
        // Scroll buffer commands
        commands::get_scroll_buffer,
        commands::get_buffer_stats,
        commands::clear_buffer,
        commands::get_all_buffer_lines,
        // Search commands
        commands::search_terminal,
        commands::scroll_to_line,
        // Session tree commands (dynamic jump host)
        commands::get_session_tree,
        commands::get_session_tree_summary,
        commands::add_root_node,
        commands::tree_drill_down,
        commands::expand_manual_preset,
        // Auto-route commands (auto-generated from saved connections)
        commands::get_topology_nodes,
        commands::get_topology_edges,
        commands::get_topology_edges_overlay,
        commands::add_topology_edge,
        commands::remove_topology_edge,
        commands::exclude_topology_edge,
        commands::expand_auto_route,
        // Session tree node management
        commands::update_tree_node_state,
        commands::set_tree_node_connection,
        commands::set_tree_node_terminal,
        commands::set_tree_node_sftp,
        commands::remove_tree_node,
        commands::get_tree_node,
        commands::get_tree_node_path,
        commands::clear_session_tree,
        commands::connect_tree_node,
        commands::disconnect_tree_node,
        commands::connect_manual_preset,
        commands::destroy_node_sessions,
        // Config commands
        commands::config::get_connections,
        commands::config::get_recent_connections,
        commands::config::get_connections_by_group,
        commands::config::search_connections,
        commands::config::get_groups,
        commands::config::save_connection,
        commands::config::delete_connection,
        commands::config::mark_connection_used,
        commands::config::get_connection_password,
        commands::config::get_saved_connection_for_connect,
        commands::config::list_ssh_config_hosts,
        commands::config::import_ssh_host,
        commands::config::get_ssh_config_path,
        commands::config::create_group,
        commands::config::delete_group,
        // AI API key commands
        commands::config::set_ai_api_key,
        commands::config::get_ai_api_key,
        commands::config::has_ai_api_key,
        commands::config::delete_ai_api_key,
        // AI Provider API key commands
        commands::config::set_ai_provider_api_key,
        commands::config::get_ai_provider_api_key,
        commands::config::has_ai_provider_api_key,
        commands::config::delete_ai_provider_api_key,
        commands::config::list_ai_provider_keys,
        // Oxide file export/import commands
        commands::oxide_export::export_to_oxide,
        commands::oxide_export::preflight_export,
        commands::oxide_import::validate_oxide_file,
        commands::oxide_import::preview_oxide_import,
        commands::oxide_import::import_from_oxide,
        // Port forwarding commands
        commands::create_port_forward,
        commands::stop_port_forward,
        commands::list_port_forwards,
        commands::pause_port_forwards,
        commands::restore_port_forwards,
        commands::forward_jupyter,
        commands::forward_tensorboard,
        commands::forward_vscode,
        commands::stop_all_forwards,
        commands::delete_port_forward,
        commands::restart_port_forward,
        commands::update_port_forward,
        commands::get_port_forward_stats,
        commands::list_saved_forwards,
        commands::set_forward_auto_start,
        commands::delete_saved_forward,
        // Health check commands
        commands::get_connection_health,
        commands::get_quick_health,
        commands::get_all_health_status,
        commands::get_health_for_display,
        commands::simulate_health_response,
        // Resource profiler commands
        commands::start_resource_profiler,
        commands::stop_resource_profiler,
        commands::get_resource_metrics,
        commands::get_resource_history,
        // IDE mode commands
        commands::ide_open_project,
        commands::ide_check_file,
        commands::ide_batch_stat,
        // SFTP transfer control commands (node-independent)
        commands::sftp_cancel_transfer,
        commands::sftp_pause_transfer,
        commands::sftp_resume_transfer,
        commands::sftp_transfer_stats,
        commands::sftp_update_settings,
        // Network and reconnect commands
        commands::network_status_changed,
        commands::cancel_reconnect,
        commands::is_reconnecting,
        // Keyboard-Interactive (2FA) commands - completely isolated from connect_v2
        commands::ssh_connect_kbi,
        commands::ssh_kbi_respond,
        commands::ssh_kbi_cancel,
        // Archive commands (compression/extraction)
        commands::compress_files,
        commands::extract_archive,
        commands::list_archive_contents,
        // AI chat persistence commands
        commands::ai_chat_list_conversations,
        commands::ai_chat_get_conversation,
        commands::ai_chat_create_conversation,
        commands::ai_chat_update_conversation,
        commands::ai_chat_delete_conversation,
        commands::ai_chat_save_message,
        commands::ai_chat_update_message,
        commands::ai_chat_delete_messages_after,
        commands::ai_chat_clear_all,
        commands::ai_chat_get_stats,
        // Plugin system commands
        commands::list_plugins,
        commands::read_plugin_file,
        commands::save_plugin_config,
        commands::load_plugin_config,
        // Plugin file server commands
        commands::start_plugin_server,
        commands::get_plugin_server_port,
        commands::stop_plugin_server,
        // Plugin registry commands (remote install)
        commands::fetch_plugin_registry,
        commands::install_plugin,
        commands::uninstall_plugin,
        commands::check_plugin_updates,
        // Oxide-Next: node-first commands (Phase 0)
        commands::node_get_state,
        commands::node_sftp_init,
        commands::node_sftp_list_dir,
        commands::node_sftp_stat,
        commands::node_sftp_preview,
        commands::node_sftp_write,
        commands::node_sftp_download,
        commands::node_sftp_upload,
        commands::node_sftp_delete,
        commands::node_sftp_mkdir,
        commands::node_sftp_rename,
        commands::node_terminal_url,
        // Oxide-Next: Phase 4 补全命令
        commands::node_sftp_delete_recursive,
        commands::node_sftp_download_dir,
        commands::node_sftp_upload_dir,
        commands::node_sftp_preview_hex,
        commands::node_sftp_list_incomplete_transfers,
        commands::node_sftp_resume_transfer,
        commands::node_ide_open_project,
        commands::node_ide_exec_command,
        commands::node_ide_check_file,
        commands::node_ide_batch_stat,
        // Oxide-Next: node-first forwarding commands
        commands::node_list_forwards,
        commands::node_create_forward,
        commands::node_stop_forward,
        commands::node_delete_forward,
        commands::node_restart_forward,
        commands::node_update_forward,
        commands::node_get_forward_stats,
        commands::node_stop_all_forwards,
        commands::node_forward_jupyter,
        commands::node_forward_tensorboard,
        commands::node_forward_vscode,
        commands::node_list_saved_forwards,
        // WSL Graphics commands (stub on non-Windows platforms)
        graphics::commands::wsl_graphics_list_distros,
        graphics::commands::wsl_graphics_start,
        graphics::commands::wsl_graphics_stop,
        graphics::commands::wsl_graphics_reconnect,
        graphics::commands::wsl_graphics_list_sessions,
        graphics::commands::wsl_graphics_detect_wslg,
        graphics::commands::wsl_graphics_start_app,
        // Platform launcher commands
        launcher::launcher_list_apps,
        launcher::launcher_launch_app,
        launcher::launcher_wsl_launch,
        launcher::launcher_clear_cache,
        // Terminal background image commands
        terminal_bg::upload_terminal_background,
        terminal_bg::list_terminal_backgrounds,
        terminal_bg::delete_terminal_background,
        terminal_bg::clear_terminal_background,
        terminal_bg::init_terminal_background,
    ]);

    builder
        .build(tauri::generate_context!())
        .map_err(|e| {
            let msg = format!("Failed to build Tauri application: {}", e);
            tracing::error!("{}", msg);
            write_startup_log(&msg);
            show_startup_error("OxideTerm Startup Error", &msg);
            e
        })
        .ok()
        .map(|app| {
            app.run(|app_handle, event| {
                // Handle app lifecycle events
                if let tauri::RunEvent::Exit = event {
                    tracing::info!("App exit requested, cleaning up resources...");

                    // Windows: 清理高精度定时器（恢复系统默认值）
                    #[cfg(target_os = "windows")]
                    disable_high_precision_timer();

                    // Clean up ProfilerRegistry (resource profilers) — BEFORE BridgeManager
                    if let Some(profiler_registry) =
                        app_handle.try_state::<commands::ProfilerRegistry>()
                    {
                        tracing::info!("Stopping all resource profilers...");
                        profiler_registry.stop_all();
                    }

                    // Clean up BridgeManager (WebSocket servers)
                    if let Some(bridge_manager) = app_handle.try_state::<BridgeManager>() {
                        tracing::info!("Closing all WebSocket bridges...");
                        tauri::async_runtime::block_on(bridge_manager.close_all());
                    }

                    // Clean up ForwardingRegistry (port forwards)
                    if let Some(fwd_registry) =
                        app_handle.try_state::<Arc<commands::ForwardingRegistry>>()
                    {
                        tracing::info!("Stopping all port forwards...");
                        // Stop all forwards for all sessions
                        tauri::async_runtime::block_on(async {
                            fwd_registry.stop_all_forwards().await;
                        });
                    }

                    // Clean up SessionRegistry (SSH sessions)
                    if let Some(registry) = app_handle.try_state::<Arc<SessionRegistry>>() {
                        tracing::info!("Disconnecting all SSH sessions...");
                        tauri::async_runtime::block_on(async {
                            registry.disconnect_all().await;
                        });
                    }

                    // Clean up SFTP sessions
                    if let Some(sftp_registry) = app_handle.try_state::<Arc<SftpRegistry>>() {
                        tracing::info!("Closing all SFTP sessions...");
                        tauri::async_runtime::block_on(async {
                            sftp_registry.close_all().await;
                        });
                    }

                    // Clean up SSH connection registry (connection pool)
                    if let Some(conn_registry) =
                        app_handle.try_state::<Arc<SshConnectionRegistry>>()
                    {
                        tracing::info!("Disconnecting all pooled SSH connections...");
                        tauri::async_runtime::block_on(async {
                            conn_registry.disconnect_all().await;
                        });
                    }

                    // Clean up local terminal sessions (only when feature enabled)
                    #[cfg(feature = "local-terminal")]
                    if let Some(local_state) =
                        app_handle.try_state::<Arc<commands::local::LocalTerminalState>>()
                    {
                        tracing::info!("Closing all local terminal sessions...");
                        tauri::async_runtime::block_on(async {
                            local_state.registry.close_all().await;
                        });
                    }

                    // Clean up WSL graphics sessions (only on Windows with feature)
                    #[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
                    if let Some(gfx_state) =
                        app_handle.try_state::<Arc<graphics::WslGraphicsState>>()
                    {
                        tracing::info!("Shutting down all WSL graphics sessions...");
                        tauri::async_runtime::block_on(async {
                            gfx_state.shutdown().await;
                        });
                    }

                    tracing::info!("All resources cleaned up, exiting...");
                }
            })
        });
}
