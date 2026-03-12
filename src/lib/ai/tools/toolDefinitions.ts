/**
 * AI Tool Definitions
 *
 * Defines the built-in tools available to AI models for agentic interactions.
 * Each tool has a JSON Schema definition that gets sent to the provider API.
 */

import type { AiToolDefinition } from '../providers';
import type { TabType } from '../../../types';

// ═══════════════════════════════════════════════════════════════════════════
// Tool Definitions
// ═══════════════════════════════════════════════════════════════════════════

export const BUILTIN_TOOLS: AiToolDefinition[] = [
  {
    name: 'terminal_exec',
    description:
      'Execute a shell command on a remote node or send a command into an existing terminal session. Use node_id for direct remote execution with captured stdout/stderr. Use session_id to send a command to an open terminal session — output is captured automatically (no need for await_terminal_output).',
    parameters: {
      type: 'object',
      properties: {
        command: {
          type: 'string',
          description: 'The shell command to execute.',
        },
        cwd: {
          type: 'string',
          description: 'Working directory for the command. Optional.',
        },
        timeout_secs: {
          type: 'number',
          minimum: 1,
          maximum: 60,
          description: 'Timeout in seconds. Default: 30. Max: 60.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID for direct remote execution. If omitted, uses the active remote terminal when available. Use list_sessions to discover nodes.',
        },
        session_id: {
          type: 'string',
          description: 'Target open terminal session ID. Use this to send a command into an existing SSH or local terminal session discovered via list_sessions.',
        },
        await_output: {
          type: 'boolean',
          description: 'When using session_id, automatically wait for command output. Default: true. Set false for interactive/long-running commands.',
        },
      },
      required: ['command'],
    },
  },
  {
    name: 'read_file',
    description:
      'Read the contents of a file on the remote server. Returns the file content as text. Best for source code, config files, and other text files.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the file to read.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active terminal.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'write_file',
    description:
      'Write content to a file on the remote server. Creates the file if it does not exist, overwrites if it does.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the file to write.',
        },
        content: {
          type: 'string',
          description: 'Content to write to the file.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active terminal.',
        },
      },
      required: ['path', 'content'],
    },
  },
  {
    name: 'list_directory',
    description:
      'List files and directories at the given path on the remote server. Returns a recursive directory tree.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the directory to list.',
        },
        max_depth: {
          type: 'number',
          minimum: 1,
          maximum: 8,
          description: 'Maximum recursion depth. Default: 3. Max: 8.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active terminal.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'grep_search',
    description:
      'Search for a text pattern across files in a directory on the remote server. Returns matching lines with file paths and line numbers.',
    parameters: {
      type: 'object',
      properties: {
        pattern: {
          type: 'string',
          description: 'Search pattern (regex supported).',
        },
        path: {
          type: 'string',
          description: 'Directory path to search in.',
        },
        case_sensitive: {
          type: 'boolean',
          description: 'Whether the search is case-sensitive. Default: false.',
        },
        max_results: {
          type: 'number',
          minimum: 1,
          maximum: 200,
          description: 'Maximum number of matches to return. Default: 50. Max: 200.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active terminal.',
        },
      },
      required: ['pattern', 'path'],
    },
  },
  {
    name: 'git_status',
    description:
      'Get the git status of a repository on the remote server. Returns the current branch and list of modified/untracked files.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Path to the git repository root.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active terminal.',
        },
      },
      required: ['path'],
    },
  },

  // ── Tab & Session Discovery Tools ──
  {
    name: 'list_tabs',
    description:
      'List all open tabs in the application. Returns tab IDs, types (terminal, sftp, ide, local_terminal, settings, etc.), titles, and which tab is currently active. Use this to understand the user\'s workspace layout.',
    parameters: {
      type: 'object',
      properties: {},
    },
  },
  {
    name: 'list_sessions',
    description:
      'List all open terminal sessions (SSH and local). Returns node IDs, hostnames, connection status, and terminal counts. Use this to discover available targets before using other tools.',
    parameters: {
      type: 'object',
      properties: {
        session_type: {
          type: 'string',
          enum: ['ssh', 'local', 'all'],
          description: 'Filter by session type. Default: "all".',
        },
      },
    },
  },
  {
    name: 'get_terminal_buffer',
    description:
      'Read the terminal buffer (scrollback history) of a specific session. Returns recent output lines. Use list_sessions first to find session IDs.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'The terminal session ID to read buffer from. Get this from list_sessions.',
        },
        max_lines: {
          type: 'number',
          minimum: 1,
          maximum: 500,
          description: 'Maximum number of lines to return. Default: 100. Max: 500.',
        },
      },
      required: ['session_id'],
    },
  },
  {
    name: 'search_terminal',
    description:
      'Search for a text pattern in a terminal session\'s buffer. Returns matching lines with line numbers.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'The terminal session ID to search in.',
        },
        query: {
          type: 'string',
          description: 'Search text or regex pattern.',
        },
        case_sensitive: {
          type: 'boolean',
          description: 'Case-sensitive search. Default: false.',
        },
        regex: {
          type: 'boolean',
          description: 'Treat query as regex. Default: false.',
        },
        max_results: {
          type: 'number',
          minimum: 1,
          maximum: 100,
          description: 'Maximum number of matches. Default: 50. Max: 100.',
        },
      },
      required: ['session_id', 'query'],
    },
  },
  {
    name: 'await_terminal_output',
    description:
      'Wait for new output in a terminal session after sending a command. Polls the terminal buffer until new content appears and stabilizes, an optional regex pattern matches, or timeout is reached. Returns only the NEW output since invocation. Use this after terminal_exec with session_id to observe command results.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'The terminal session ID to watch. Get this from list_sessions.',
        },
        timeout_secs: {
          type: 'number',
          minimum: 1,
          maximum: 120,
          description: 'How long to wait for output. Default: 15. Max: 120.',
        },
        pattern: {
          type: 'string',
          description: 'Optional regex pattern — return immediately when matched in new output. Useful for waiting for a specific prompt, error, or completion marker.',
        },
        stable_secs: {
          type: 'number',
          minimum: 0.5,
          maximum: 10,
          description: 'Seconds of no new output before considering output stable and returning. Default: 2.',
        },
      },
      required: ['session_id'],
    },
  },

  // ── Infrastructure Tools ──
  {
    name: 'list_connections',
    description:
      'List all SSH connections in the connection pool with their status, remote OS, and usage counts.',
    parameters: {
      type: 'object',
      properties: {},
    },
  },
  {
    name: 'list_port_forwards',
    description:
      'List all port forwarding rules for a specific node.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Node ID to list forwards for. Use list_sessions to find nodes.',
        },
      },
      required: ['node_id'],
    },
  },
  {
    name: 'get_detected_ports',
    description:
      'List ports detected as listening on the remote server. Useful for discovering services that could be forwarded.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Node ID to check.',
        },
      },
      required: ['node_id'],
    },
  },
  {
    name: 'get_connection_health',
    description:
      'Get health and latency metrics for SSH connections. If no node_id is specified, returns health for all connections.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Node ID to check. If omitted, returns health for all connections.',
        },
      },
    },
  },

  // ── Port Forwarding Management Tools ──
  {
    name: 'create_port_forward',
    description:
      'Create a port forwarding rule on a remote node. Use get_detected_ports to find available services.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Node ID to create forward on.',
        },
        forward_type: {
          type: 'string',
          enum: ['local', 'remote', 'dynamic'],
          description: 'Forwarding type: local (remote→local), remote (local→remote), or dynamic (SOCKS).',
        },
        bind_port: {
          type: 'number',
          minimum: 1,
          maximum: 65535,
          description: 'Local bind port.',
        },
        target_host: {
          type: 'string',
          description: 'Remote target hostname. Default: "localhost".',
        },
        target_port: {
          type: 'number',
          minimum: 1,
          maximum: 65535,
          description: 'Remote target port.',
        },
        bind_addr: {
          type: 'string',
          description: 'Bind address. Default: "127.0.0.1".',
        },
      },
      required: ['node_id', 'forward_type', 'bind_port', 'target_port'],
    },
  },
  {
    name: 'stop_port_forward',
    description:
      'Stop an active port forwarding rule. Use list_port_forwards to find forward IDs.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Node ID the forward belongs to.',
        },
        forward_id: {
          type: 'string',
          description: 'Forward rule ID to stop.',
        },
      },
      required: ['node_id', 'forward_id'],
    },
  },
  // ── Meta Tools ──────────────────────────────────────────────────────────
  {
    name: 'send_control_sequence',
    description:
      'Send a control sequence (signal) to an open terminal session. Use ctrl-c to cancel a running command, ctrl-d to send EOF, ctrl-z to suspend, ctrl-l to clear screen.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'Target terminal session ID.',
        },
        sequence: {
          type: 'string',
          enum: ['ctrl-c', 'ctrl-d', 'ctrl-z', 'ctrl-l', 'ctrl-\\'],
          description: 'The control sequence to send.',
        },
      },
      required: ['session_id', 'sequence'],
    },
  },
  {
    name: 'batch_exec',
    description:
      'Execute multiple commands sequentially in a terminal session, returning individual output for each command. More efficient than calling terminal_exec multiple times. Commands run in order; failures do not stop subsequent commands.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'Target terminal session ID.',
        },
        commands: {
          type: 'array',
          items: { type: 'string' },
          description: 'Array of shell commands to execute sequentially. Max 10.',
        },
      },
      required: ['session_id', 'commands'],
    },
  },
  // ── TUI Interaction (Experimental) ──────────────────────────────────────
  {
    name: 'read_screen',
    description:
      '[Experimental] Read the current terminal viewport as a screen snapshot. Returns line-by-line text with cursor position, terminal dimensions, and whether a TUI app is active (alternate buffer). Use this before send_keys/send_mouse to understand the current screen state.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'Target terminal session ID.',
        },
      },
      required: ['session_id'],
    },
  },
  {
    name: 'send_keys',
    description:
      '[Experimental] Send a sequence of keystrokes to a terminal session. Supports plain text and special keys (Enter, Escape, Tab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, F1-F12, Space, Insert). Use read_screen first to see current state, then send_keys to interact with TUI apps like vim, nano, htop, less.',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'Target terminal session ID.',
        },
        keys: {
          type: 'array',
          items: { type: 'string' },
          description: 'Array of keys to send. Each element is either plain text (sent as-is) or a special key name (case-insensitive): Enter, Escape, Tab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Space, F1-F12. Max 50 elements.',
        },
        delay_ms: {
          type: 'number',
          description: 'Delay between each key in milliseconds. Default 50. Range: 10-1000.',
        },
      },
      required: ['session_id', 'keys'],
    },
  },
  {
    name: 'send_mouse',
    description:
      '[Experimental] Send a mouse event to a terminal session using SGR protocol. Only effective when the TUI app has mouse tracking enabled (e.g. htop, mc, tmux). Use read_screen first to verify alternate buffer is active. Coordinates are 1-based (row 1 = top, col 1 = left).',
    parameters: {
      type: 'object',
      properties: {
        session_id: {
          type: 'string',
          description: 'Target terminal session ID.',
        },
        action: {
          type: 'string',
          enum: ['click', 'scroll'],
          description: 'Mouse action type.',
        },
        x: {
          type: 'integer',
          description: 'Column coordinate (1-based).',
        },
        y: {
          type: 'integer',
          description: 'Row coordinate (1-based).',
        },
        button: {
          type: 'string',
          enum: ['left', 'right', 'middle'],
          description: 'Mouse button for click action. Default: left.',
        },
        direction: {
          type: 'string',
          enum: ['up', 'down'],
          description: 'Scroll direction for scroll action.',
        },
        count: {
          type: 'integer',
          description: 'Number of scroll steps. Default: 1. Max: 20.',
        },
      },
      required: ['session_id', 'action', 'x', 'y'],
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// SFTP Tools — Available only when SFTP tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const SFTP_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'sftp_list_dir',
    description:
      'List files and directories at the given path on the remote server via SFTP. Returns file names, types, sizes, permissions and modification times.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the directory to list.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active SFTP tab\'s node.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'sftp_read_file',
    description:
      'Read the contents of a remote file via SFTP. Returns text content with detected encoding and language. Best for text files, config files, and source code.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the file to read.',
        },
        max_size: {
          type: 'number',
          description: 'Maximum file size in bytes to read. Default: 1MB.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active SFTP tab\'s node.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'sftp_stat',
    description:
      'Get detailed information about a remote file or directory via SFTP. Returns name, type, size, permissions, and modification time.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the file or directory.',
        },
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active SFTP tab\'s node.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'sftp_get_cwd',
    description:
      'Get the current working directory of the SFTP file browser for the active node.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Target node ID. If omitted, uses the active SFTP tab\'s node.',
        },
      },
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// IDE Tools — Available only when IDE tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const IDE_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'ide_get_open_files',
    description:
      'List all files currently open in the IDE editor. Returns tab IDs, file paths, language, dirty status, and pinned status.',
    parameters: {
      type: 'object',
      properties: {},
    },
  },
  {
    name: 'ide_get_file_content',
    description:
      'Get the current content of a file open in the IDE editor. Returns the content, language, dirty status, and cursor position.',
    parameters: {
      type: 'object',
      properties: {
        tab_id: {
          type: 'string',
          description: 'The IDE tab ID (from ide_get_open_files) to read content from.',
        },
      },
      required: ['tab_id'],
    },
  },
  {
    name: 'ide_get_project_info',
    description:
      'Get information about the currently open IDE project. Returns root path, project name, git repo status, and git branch.',
    parameters: {
      type: 'object',
      properties: {},
    },
  },
  {
    name: 'ide_apply_edit',
    description:
      'Apply a text edit to a file currently open in the IDE editor. Updates the content and optionally saves it.',
    parameters: {
      type: 'object',
      properties: {
        tab_id: {
          type: 'string',
          description: 'The IDE tab ID to edit.',
        },
        content: {
          type: 'string',
          description: 'The new full content for the file.',
        },
        save: {
          type: 'boolean',
          description: 'Whether to save the file after editing. Default: false.',
        },
      },
      required: ['tab_id', 'content'],
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Local Terminal Tools — Available only when local_terminal tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const LOCAL_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'local_list_shells',
    description: 'List all available shells on the local machine. Returns shell name, path, and whether it is the default shell.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'local_get_terminal_info',
    description: 'List all local terminal sessions (active and background). Returns session IDs, shell info, dimensions, and running state.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'local_exec',
    description: 'Execute a command on the local machine and return stdout/stderr/exit_code. Use for quick one-shot commands, not interactive programs.',
    parameters: {
      type: 'object',
      properties: {
        command: { type: 'string', description: 'The shell command to execute.' },
        cwd: { type: 'string', description: 'Working directory. Optional.' },
        timeout_secs: { type: 'number', minimum: 1, maximum: 60, description: 'Timeout in seconds. Default: 30. Max: 60.' },
      },
      required: ['command'],
    },
  },
  {
    name: 'local_get_drives',
    description: 'List local drives/volumes with path, name, type, total/available space, and read-only status.',
    parameters: { type: 'object', properties: {} },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Settings Tools — Available only when settings tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const SETTINGS_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'get_settings',
    description: 'Read application settings. Optionally specify a section name to get only that section (e.g. "terminal", "ai", "appearance").',
    parameters: {
      type: 'object',
      properties: {
        section: { type: 'string', description: 'Settings section name. If omitted, returns all settings.' },
      },
    },
  },
  {
    name: 'update_setting',
    description: 'Update a single setting value. Specify the section and the key-value pair to change. Cannot modify ai.toolUse settings.',
    parameters: {
      type: 'object',
      properties: {
        section: { type: 'string', description: 'Settings section (e.g. "terminal", "appearance", "ai", "sftp").' },
        key: { type: 'string', description: 'The setting key within the section.' },
        value: { description: 'The new value for the setting.' },
      },
      required: ['section', 'key', 'value'],
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Connection Pool Tools — Available only when connection_pool tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const POOL_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'get_pool_stats',
    description: 'Get SSH connection pool statistics: active, idle, reconnecting, and total connection counts, plus pool configuration.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'set_pool_config',
    description: 'Update SSH connection pool configuration (idle timeout, max connections, keep-alive interval).',
    parameters: {
      type: 'object',
      properties: {
        idle_timeout_secs: { type: 'number', minimum: 0, description: 'Idle timeout in seconds before recycling a connection. 0 = no timeout.' },
        max_connections: { type: 'number', minimum: 1, maximum: 100, description: 'Maximum number of pooled connections.' },
        keepalive_interval_secs: { type: 'number', minimum: 0, description: 'Keep-alive interval in seconds. 0 = disabled.' },
      },
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Connection Monitor Tools — Available only when connection_monitor tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const MONITOR_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'get_all_health',
    description: 'Get health status summary for all SSH connections. Returns latency, uptime, packet loss, and health grade per connection.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'get_resource_metrics',
    description: 'Get real-time resource metrics (CPU, memory, network, disk) for a specific connection.',
    parameters: {
      type: 'object',
      properties: {
        connection_id: { type: 'string', description: 'Connection ID to get metrics for. Use list_connections or get_all_health to find IDs.' },
      },
      required: ['connection_id'],
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Session Manager Tools — Available only when session_manager tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const SESSION_MGR_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'list_saved_connections',
    description: 'List all saved connection configurations. Returns host, port, username, name, and creation date. Passwords and key paths are excluded.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'search_saved_connections',
    description: 'Search saved connections by keyword (matches host, username, name).',
    parameters: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Search keyword.' },
      },
      required: ['query'],
    },
  },
  {
    name: 'get_session_tree',
    description: 'Get the current session tree structure showing all open sessions, their hierarchy, connection state, and tab types.',
    parameters: { type: 'object', properties: {} },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Manager Tools — Available only when plugin_manager tab is active
// ═══════════════════════════════════════════════════════════════════════════

export const PLUGIN_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'list_plugins',
    description: 'List all installed plugins with their ID, name, version, enabled/disabled state, and error status.',
    parameters: { type: 'object', properties: {} },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Safety Classification
// ═══════════════════════════════════════════════════════════════════════════

/** Tools that only read data — safe for auto-approve */
export const READ_ONLY_TOOLS = new Set([
  'read_file',
  'list_directory',
  'grep_search',
  'git_status',
  'list_tabs',
  'list_sessions',
  'get_terminal_buffer',
  'search_terminal',
  'list_connections',
  'list_port_forwards',
  'get_detected_ports',
  'get_connection_health',
  // SFTP (all read-only)
  'sftp_list_dir',
  'sftp_read_file',
  'sftp_stat',
  'sftp_get_cwd',
  // IDE (read-only subset)
  'ide_get_open_files',
  'ide_get_file_content',
  'ide_get_project_info',
  // Local terminal (read-only subset)
  'local_list_shells',
  'local_get_terminal_info',
  'local_get_drives',
  // Settings (read-only)
  'get_settings',
  // Connection pool (read-only)
  'get_pool_stats',
  // Connection monitor (read-only)
  'get_all_health',
  'get_resource_metrics',
  // Session manager (all read-only)
  'list_saved_connections',
  'search_saved_connections',
  'get_session_tree',
  // Plugin manager (read-only)
  'list_plugins',
]);

/**
 * Experimental tools — not included in "approve all" or "revoke all" bulk actions.
 * Users must explicitly enable these in settings. Marked with [Experimental] badge.
 */
export const EXPERIMENTAL_TOOLS = new Set([
  'read_screen',
  'send_keys',
  'send_mouse',
]);

/** Tools that modify state — require explicit user approval */
export const WRITE_TOOLS = new Set([
  'terminal_exec',
  'write_file',
  'create_port_forward',
  'stop_port_forward',
  'ide_apply_edit',
  // Local terminal (write)
  'local_exec',
  // Settings (write)
  'update_setting',
  // Connection pool (write)
  'set_pool_config',
  // Meta tools (write)
  'send_control_sequence',
  'batch_exec',
  // TUI interaction (experimental, write)
  'send_keys',
  'send_mouse',
]);

/** Tools that do NOT require any node context — work globally or read from local stores */
export const CONTEXT_FREE_TOOLS = new Set([
  'list_tabs',
  'list_sessions',
  'list_connections',
  'get_connection_health',
  // IDE tools read from local Zustand store, no node resolution needed
  'ide_get_open_files',
  'ide_get_file_content',
  'ide_get_project_info',
  'ide_apply_edit',
  // Local terminal tools
  'local_list_shells',
  'local_get_terminal_info',
  'local_exec',
  'local_get_drives',
  // Settings tools
  'get_settings',
  'update_setting',
  // Connection pool tools
  'get_pool_stats',
  'set_pool_config',
  // Connection monitor tools
  'get_all_health',
  'get_resource_metrics',
  // Session manager tools
  'list_saved_connections',
  'search_saved_connections',
  'get_session_tree',
  // Plugin manager tools
  'list_plugins',
]);

/** Tools that use session_id parameter instead of node_id */
export const SESSION_ID_TOOLS = new Set([
  'get_terminal_buffer',
  'search_terminal',
  'await_terminal_output',
  'send_control_sequence',
  'batch_exec',
  // TUI interaction (experimental)
  'read_screen',
  'send_keys',
  'send_mouse',
]);

/** Tools that only make sense for SSH connections (remote nodes) */
export const SSH_ONLY_TOOLS = new Set([
  'list_port_forwards',
  'get_detected_ports',
  'create_port_forward',
  'stop_port_forward',
  'list_connections',
  'get_connection_health',
]);

/** Tools only shown when SFTP tab is active */
export const SFTP_ONLY_TOOLS = new Set([
  'sftp_list_dir',
  'sftp_read_file',
  'sftp_stat',
  'sftp_get_cwd',
]);

/** Tools only shown when IDE tab is active */
export const IDE_ONLY_TOOLS = new Set([
  'ide_get_open_files',
  'ide_get_file_content',
  'ide_get_project_info',
  'ide_apply_edit',
]);

/** Tools only shown when local_terminal tab is active */
export const LOCAL_ONLY_TOOLS = new Set([
  'local_list_shells',
  'local_get_terminal_info',
  'local_exec',
  'local_get_drives',
]);

/** Tools only shown when settings tab is active */
export const SETTINGS_ONLY_TOOLS = new Set([
  'get_settings',
  'update_setting',
]);

/** Tools only shown when connection_pool tab is active */
export const POOL_ONLY_TOOLS = new Set([
  'get_pool_stats',
  'set_pool_config',
]);

/** Tools only shown when connection_monitor tab is active */
export const MONITOR_ONLY_TOOLS = new Set([
  'get_all_health',
  'get_resource_metrics',
]);

/** Tools only shown when session_manager tab is active */
export const SESSION_MGR_ONLY_TOOLS = new Set([
  'list_saved_connections',
  'search_saved_connections',
  'get_session_tree',
]);

/** Tools only shown when plugin_manager tab is active */
export const PLUGIN_MGR_ONLY_TOOLS = new Set([
  'list_plugins',
]);

/**
 * Grouped tool list for settings UI.
 * Groups: terminal → session → infrastructure → sftp → ide
 * Each group is split into read-only and write sub-groups.
 */
export const TOOL_GROUPS: { groupKey: string; readOnly: string[]; write: string[] }[] = [
  {
    groupKey: 'terminal',
    readOnly: ['read_file', 'list_directory', 'grep_search', 'git_status'],
    write: ['terminal_exec', 'write_file'],
  },
  {
    groupKey: 'session',
    readOnly: ['list_tabs', 'list_sessions', 'get_terminal_buffer', 'search_terminal', 'await_terminal_output'],
    write: ['send_control_sequence', 'batch_exec'],
  },
  {
    groupKey: 'tui_interaction',
    readOnly: ['read_screen'],
    write: ['send_keys', 'send_mouse'],
  },
  {
    groupKey: 'infrastructure',
    readOnly: ['list_connections', 'get_connection_health', 'list_port_forwards', 'get_detected_ports'],
    write: ['create_port_forward', 'stop_port_forward'],
  },
  {
    groupKey: 'sftp',
    readOnly: ['sftp_list_dir', 'sftp_read_file', 'sftp_stat', 'sftp_get_cwd'],
    write: [],
  },
  {
    groupKey: 'ide',
    readOnly: ['ide_get_open_files', 'ide_get_file_content', 'ide_get_project_info'],
    write: ['ide_apply_edit'],
  },
  {
    groupKey: 'local_terminal',
    readOnly: ['local_list_shells', 'local_get_terminal_info', 'local_get_drives'],
    write: ['local_exec'],
  },
  {
    groupKey: 'settings',
    readOnly: ['get_settings'],
    write: ['update_setting'],
  },
  {
    groupKey: 'connection_pool',
    readOnly: ['get_pool_stats'],
    write: ['set_pool_config'],
  },
  {
    groupKey: 'connection_monitor',
    readOnly: ['get_all_health', 'get_resource_metrics'],
    write: [],
  },
  {
    groupKey: 'session_manager',
    readOnly: ['list_saved_connections', 'search_saved_connections', 'get_session_tree'],
    write: [],
  },
  {
    groupKey: 'plugin_manager',
    readOnly: ['list_plugins'],
    write: [],
  },
];

/**
 * Get relevant tool definitions based on active tab type and session context.
 * Completely hides tools irrelevant to the active tab, saving tokens and focus.
 */
export function getToolsForContext(
  activeTabType: TabType | null,
  hasAnySSHSession: boolean,
  disabledTools?: Set<string>,
): AiToolDefinition[] {
  // Combine all tools into a single pool
  const allTools = [
    ...BUILTIN_TOOLS,
    ...SFTP_TOOL_DEFS,
    ...IDE_TOOL_DEFS,
    ...LOCAL_TOOL_DEFS,
    ...SETTINGS_TOOL_DEFS,
    ...POOL_TOOL_DEFS,
    ...MONITOR_TOOL_DEFS,
    ...SESSION_MGR_TOOL_DEFS,
    ...PLUGIN_TOOL_DEFS,
  ];
  
  return allTools.filter(t => {
    // User-disabled tools: never sent to LLM
    if (disabledTools?.has(t.name)) return false;

    // SSH-only tools: hide when only local terminals and no SSH sessions
    if (SSH_ONLY_TOOLS.has(t.name)) {
      if (!hasAnySSHSession) return false;
    }
    
    // Tab-specific tools: only show on their respective tabs
    if (SFTP_ONLY_TOOLS.has(t.name)) return activeTabType === 'sftp';
    if (IDE_ONLY_TOOLS.has(t.name)) return activeTabType === 'ide';
    if (LOCAL_ONLY_TOOLS.has(t.name)) return activeTabType === 'local_terminal';
    if (SETTINGS_ONLY_TOOLS.has(t.name)) return activeTabType === 'settings';
    if (POOL_ONLY_TOOLS.has(t.name)) return activeTabType === 'connection_pool';
    if (MONITOR_ONLY_TOOLS.has(t.name)) return activeTabType === 'connection_monitor';
    if (SESSION_MGR_ONLY_TOOLS.has(t.name)) return activeTabType === 'session_manager';
    if (PLUGIN_MGR_ONLY_TOOLS.has(t.name)) return activeTabType === 'plugin_manager';
    
    return true;
  });
}

/**
 * Command deny-list for terminal_exec safety.
 * These patterns are checked against the command string before execution.
 * If any pattern matches, the command is rejected without prompting the user.
 *
 * NOTE: Deny-lists are fundamentally incomplete. This is a defense-in-depth
 * measure, not a security boundary. The real boundary is user approval.
 */
export const COMMAND_DENY_LIST: RegExp[] = [
  // ── Destructive filesystem ──
  /\brm\s+.*\s+\/(\s|$|\*)/,            // rm ... / or rm ... /*
  /\brm\s+(-[a-zA-Z]*)*\s*--no-preserve-root/, // rm --no-preserve-root
  /\bmkfs\b/,                           // mkfs (format disk)
  /\bdd\s+if=/,                         // dd if= (raw disk write)
  /\bfdisk\b/,                          // fdisk (partition table)
  /\bchmod\s+777\s+\//,                 // chmod 777 /
  /\bchown\s+-R\s+.*\s+\//,            // chown -R ... /

  // ── Privilege escalation ──
  /\bsudo\b/,                           // sudo
  /\bdoas\b/,                           // doas (OpenBSD)
  /\bpkexec\b/,                         // pkexec (Polkit)
  /\brunuser\b/,                        // runuser (systemd)
  /\brun0\b/,                           // run0 (systemd)
  /\bsu\s+-?c\b/,                       // su -c "command"
  /\bsu\s+-\s*$/,                       // su - (login as root)
  /\bsudo\s+-i\b/,                      // sudo -i (interactive root)

  // ── System control ──
  /\bshutdown\b/,                       // shutdown
  /\breboot\b/,                         // reboot
  /\bhalt\b/,                           // halt
  /\bpoweroff\b/,                       // poweroff
  /\bsystemctl\s+(disable|mask)\b/,     // systemctl disable/mask

  // ── Resource exhaustion ──
  /:\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;?\s*:/, // fork bomb

  // ── Network ──
  /\biptables\s+-F\b/,                  // iptables -F (flush all rules)

  // ── Remote code execution via pipe ──
  /\b(?:curl|wget)\b[^\n]*\|\s*(?:sh|bash|zsh)\b/, // curl/wget | sh
  /\b(?:curl|wget)\b[^\n]*-[oO]\s*[^\s]+.*;\s*(?:sh|bash|zsh)\b/, // curl -o file; sh file

  // ── Encoding / obfuscation bypass ──
  /\bbase64\b[^\n]*\|\s*(?:sh|bash|zsh)\b/, // base64 decode | sh
  /\bprintf\b[^\n]*\|\s*(?:sh|bash|zsh)\b/, // printf | sh
  /\becho\b[^\n]*\|\s*(?:sh|bash|zsh)\b/,   // echo ... | sh

  // ── Command substitution piped to shell ──
  /\$\([^)]*\)\s*\|\s*(?:sh|bash|zsh)\b/,   // $(cmd) | sh
  /`[^`]*`\s*\|\s*(?:sh|bash|zsh)\b/,       // `cmd` | sh

  // ── SSH key / credential exfiltration via redirect ──
  />>?\s*~?\/?\.ssh\/authorized_keys/,       // >> ~/.ssh/authorized_keys
  />>?\s*~?\/?\.ssh\/config/,                // >> ~/.ssh/config

  // ── Cron persistence ──
  /\bcrontab\b/,                             // crontab manipulation
  /\/etc\/cron/,                             // cron directories

  // ── History / audit evasion ──
  /\bunset\s+HISTFILE\b/,                    // unset HISTFILE
  /\bhistory\s+-c\b/,                        // history -c
  /\bHISTSIZE=0\b/,                          // HISTSIZE=0 (with or without export)

  // ── Reverse shells / port listening ──
  /\bnc\s+.*-[elp]/,                         // netcat listener
  /\bsocat\b.*TCP-LISTEN/i,                  // socat listener
  /\/dev\/tcp\//,                             // bash /dev/tcp

  // ── Dangerous builtins ──
  /\beval\b/,                           // eval (arbitrary code execution)
  /(?:^|[;&|]\s*)exec\s/,               // exec at command position (replaces shell process)
  /\bsource\s/,                         // source (execute file in current shell)
];

/**
 * Check if a command is in the deny-list.
 */
export function isCommandDenied(command: string): boolean {
  return COMMAND_DENY_LIST.some((pattern) => pattern.test(command));
}
