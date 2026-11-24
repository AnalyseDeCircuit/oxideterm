#[derive(Default)]
struct DesktopSession {
    access_request: Option<(String, String)>,
    backend_binding_abort: Option<tokio::task::AbortHandle>,
    agent_install_progress: Option<oxideterm_ide_fs::AgentInstallProgress>,
    agent_install_task: Option<Task<()>>,
    agent_status_menu: Option<AgentStatusMenu>,
    agent_status_trigger_bounds: Option<Bounds<Pixels>>,
    agent_remove_confirm_open: bool,
    agent_action: Option<AgentActionKind>,
    agent_refresh_origin: Option<AgentStatusRefreshOrigin>,
    agent_poll_generation: u64,
    agent_poll_task: Option<Task<()>>,
    agent_sampling_refresh_task: Option<Task<()>>,
    agent_sampling_backend_abort: Option<tokio::task::AbortHandle>,
    agent_watch_generation: u64,
    watched_root_path: Option<String>,
    agent_watch_task: Option<Task<()>>,
    agent_watch_retry_task: Option<Task<()>>,
    agent_watch_stop_task: Option<Task<()>>,
    agent_watch_backend_abort: Option<tokio::task::AbortHandle>,
    agent_watch_stop_in_flight: bool,
    agent_watch_restart_requested: bool,
    agent_watch_stop_generation: u64,
    agent_watch_stop_release_queue: Option<Arc<IdeWatchStopReleaseQueue>>,
}

const IDE_AGENT_MENU_WIDTH: f32 = 180.0;
const IDE_AGENT_MENU_MANUAL_WIDTH: f32 = 300.0;
const IDE_AGENT_MENU_PADDING_Y: f32 = 4.0;
const IDE_AGENT_MENU_DESCRIPTION_PADDING_X: f32 = 8.0;
const IDE_AGENT_MENU_DESCRIPTION_PADDING_Y: f32 = 6.0;

const IDE_AGENT_POLL_READY_SECS: u64 = 5;
const IDE_AGENT_POLL_DEPLOYING_SECS: u64 = 2;
const IDE_AGENT_POLL_MANUAL_SECS: u64 = 10;
const IDE_AGENT_WATCH_RETRY_SECS: u64 = 3;

const TAILWIND_EMERALD_400: u32 = 0x34d399;

#[derive(Clone, Copy, Debug, PartialEq)]
struct AgentStatusMenu {
    trigger_bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentActionKind {
    Deploy,
    Remove,
    Refresh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentStatusRefreshOrigin {
    VisibilitySampling,
    UserAction,
}

#[derive(Default)]
struct IdeWatchStopReleaseState {
    stop_complete: bool,
    node_ids: Vec<String>,
}

#[derive(Default)]
struct IdeWatchStopReleaseQueue {
    state: Mutex<IdeWatchStopReleaseState>,
}

impl IdeWatchStopReleaseQueue {
    fn request_release(&self, fs: &NodeAgentIdeFileSystem, node_id: String) {
        let release_now = {
            let mut state = self
                .state
                .lock()
                .expect("IDE watch stop release queue poisoned");
            if state.stop_complete {
                Some(node_id)
            } else {
                if !state.node_ids.contains(&node_id) {
                    state.node_ids.push(node_id);
                }
                None
            }
        };
        if let Some(node_id) = release_now {
            fs.release_ide_session_for_node(&node_id);
        }
    }

    fn finish_stop(&self, fs: &NodeAgentIdeFileSystem) {
        let node_ids = {
            let mut state = self
                .state
                .lock()
                .expect("IDE watch stop release queue poisoned");
            state.stop_complete = true;
            std::mem::take(&mut state.node_ids)
        };
        for node_id in node_ids {
            fs.release_ide_session_for_node(&node_id);
        }
    }
}
