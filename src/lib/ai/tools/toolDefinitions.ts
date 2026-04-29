// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
      'Execute a shell command on a resolved target. Prefer target_id from resolve_target. Two modes:\n' +
      '• target_id/node_id for non-interactive remote commands: Direct remote execution with reliable stdout/stderr capture.\n' +
      '• target_id/session_id for visible terminal sessions: Send command to an open shell and capture output from the terminal screen.\n' +
      'If target_id, node_id, and session_id conflict, target_id wins. If output is empty or incomplete, use get_terminal_buffer with the same target_id/session_id.',
    parameters: {
      type: 'object',
      properties: {
        command: {
          type: 'string',
          description: 'The shell command to execute.',
        },
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target/list_targets. ssh-node:* runs directly on that SSH node; terminal-session:* sends the command to that visible shell.',
        },
        cwd: {
          type: 'string',
          description: 'Working directory for the command. Only used with node_id.',
        },
        timeout_secs: {
          type: 'number',
          minimum: 1,
          maximum: 60,
          description: 'Timeout in seconds (node_id mode). Default: 30. Max: 60. For session_id mode, auto-await timeout is 30s.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID for direct remote execution. Prefer target_id from resolve_target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy open terminal session ID. Prefer target_id from resolve_target when possible.',
        },
        await_output: {
          type: 'boolean',
          description: 'When using session_id, automatically wait for command output. Default: true. Set false for interactive/long-running commands, then use get_terminal_buffer or await_terminal_output later.',
        },
      },
      required: ['command'],
    },
  },
  {
    name: 'read_file',
    description:
      'Read the contents of a file on the remote server. Returns text plus a content hash, size, and mtime in the structured tool envelope for safe follow-up writes.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the file to read.',
        },
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'write_file',
    description:
      'Write content to a file on the remote server. Creates the file if it does not exist, overwrites if it does. For safe edits, pass expected_hash from a prior read_file result.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
        },
        expected_hash: {
          type: 'string',
          description: 'Optional content hash from a prior read_file result. If the remote file changed, the write fails with a recoverable error.',
        },
        expected_mtime: {
          type: 'number',
          description: 'Optional mtime from a prior read_file result. If the remote file changed, the write fails with a recoverable error.',
        },
        create_only: {
          type: 'boolean',
          description: 'If true, fail when the target path already exists.',
        },
        append: {
          type: 'boolean',
          description: 'If true, append content to the existing text file instead of replacing it.',
        },
        dry_run: {
          type: 'boolean',
          description: 'If true, validate and return a diff summary without writing.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
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
    name: 'resolve_target',
    description:
      'Resolve one user-requested target before executing, editing, navigating, or connecting. Use this when the user named a specific host, connection, session, tab, settings area, or local shell. Do not use it for broad questions like "which remote hosts are available"; use list_saved_connections or search_saved_connections instead. The query is the target selector, not the command text. Saved connections resolve as saved-connection targets and must be opened with connect_saved_session before command.run is available. Returns one exact target or disambiguation options; do not guess when multiple targets match.',
    parameters: {
      type: 'object',
      properties: {
        query: {
          type: 'string',
          description: 'Natural-language target query, such as a connection name, hostname, username, tab title, setting area, or "local shell". Do not put the command to run here.',
        },
        intent: {
          type: 'string',
          enum: ['command', 'terminal_interaction', 'connection', 'settings', 'remote_file', 'local_shell', 'sftp', 'ide', 'monitoring', 'navigation', 'plugin', 'knowledge', 'status'],
          description: 'The intended operation. Helps rank compatible targets and avoid using the active UI as a capability boundary.',
        },
        kind: {
          type: 'string',
          enum: ['all', 'local-shell', 'saved-connection', 'ssh-node', 'terminal-session', 'sftp-session', 'ide-workspace', 'app-tab', 'mcp-server', 'rag-index'],
          description: 'Optional target kind filter. Default: "all".',
        },
      },
    },
  },
  {
    name: 'list_targets',
    description:
      'List all currently actionable targets for AI tools: local shell, SSH nodes, terminal sessions, SFTP/IDE workspaces, and app tabs. Prefer resolve_target for normal task routing; use this for debugging or broad inspection.',
    parameters: {
      type: 'object',
      properties: {
        kind: {
          type: 'string',
          enum: ['all', 'local-shell', 'saved-connection', 'ssh-node', 'terminal-session', 'sftp-session', 'ide-workspace', 'app-tab', 'mcp-server', 'rag-index'],
          description: 'Optional target kind filter. Default: "all".',
        },
      },
    },
  },
  {
    name: 'list_capabilities',
    description:
      'List tool capabilities available on current targets, such as command.run, terminal.observe, filesystem.read/write, network.forward, and navigation.open. saved-connection targets only support navigation.open/state.list until connect_saved_session returns a live ssh-node target. Use this to pick the safest tool path.',
    parameters: {
      type: 'object',
      properties: {
        target_id: {
          type: 'string',
          description: 'Optional target ID from list_targets. If omitted, returns capabilities for all targets.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
        },
        max_lines: {
          type: 'number',
          minimum: 1,
          maximum: 500,
          description: 'Maximum number of lines to return. Default: 200. Max: 500.',
        },
      },
    },
  },
  {
    name: 'search_terminal',
    description:
      'Search for a text pattern in a terminal session\'s layered history. Returns matching lines with line numbers and recent or archived source labels.',
    parameters: {
      type: 'object',
      properties: {
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
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
      required: ['query'],
    },
  },
  {
    name: 'await_terminal_output',
    description:
      'Wait for new output in a terminal session after sending a command. Polls the terminal buffer until new content appears and stabilizes, an optional regex pattern matches, or timeout is reached. Returns only the NEW output since invocation. Use this after terminal_exec with session_id to observe command results.',
    parameters: {
      type: 'object',
      properties: {
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
        },
        sequence: {
          type: 'string',
          enum: ['ctrl-c', 'ctrl-d', 'ctrl-z', 'ctrl-l', 'ctrl-\\'],
          description: 'The control sequence to send.',
        },
      },
      required: ['sequence'],
    },
  },
  {
    name: 'batch_exec',
    description:
      'Execute multiple commands sequentially in a terminal session, returning individual output for each command. More efficient than calling terminal_exec multiple times. Commands run in order; failures do not stop subsequent commands.',
    parameters: {
      type: 'object',
      properties: {
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
        },
        commands: {
          type: 'array',
          items: { type: 'string' },
          description: 'Array of shell commands to execute sequentially. Max 10.',
        },
      },
      required: ['commands'],
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
        },
      },
    },
  },
  {
    name: 'send_keys',
    description:
      '[Experimental] Send a sequence of keystrokes to a terminal session. Supports plain text, special keys (Enter, Escape, Tab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, F1-F12, Space, Insert), and terminal-style key combos such as Ctrl+C, Alt+X, Cmd+K, Shift+Tab, or Ctrl+Shift+Left. Cmd/Command is treated as Meta for terminal input. Use read_screen first to see current state, then send_keys to interact with TUI apps like vim, nano, htop, less.',
    parameters: {
      type: 'object',
      properties: {
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
        },
        keys: {
          type: 'array',
          items: { type: 'string' },
          description: 'Array of keys to send. Each element is either plain text (sent as-is), a special key name (case-insensitive): Enter, Escape, Tab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Space, F1-F12, or a terminal-style combo like Ctrl+C, Alt+X, Cmd+K, Shift+Tab, or Ctrl+Shift+Left. Max 50 elements.',
        },
        delay_ms: {
          type: 'number',
          description: 'Delay between each key in milliseconds. Default 50. Range: 10-1000.',
        },
      },
      required: ['keys'],
    },
  },
  {
    name: 'send_mouse',
    description:
      '[Experimental] Send a mouse event to a terminal session using SGR protocol. Only effective when the TUI app has mouse tracking enabled (e.g. htop, mc, tmux). Use read_screen first to verify alternate buffer is active. Coordinates are 1-based (row 1 = top, col 1 = left).',
    parameters: {
      type: 'object',
      properties: {
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be a terminal-session:* target.',
        },
        session_id: {
          type: 'string',
          description: 'Legacy terminal session ID. Prefer target_id from resolve_target.',
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
      required: ['action', 'x', 'y'],
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* or sftp-session:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'sftp_read_file',
    description:
      'Read the contents of a remote file via SFTP. Returns text content plus detected encoding, size, mtime, and content hash in the structured tool envelope.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* or sftp-session:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* or sftp-session:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
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
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* or sftp-session:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
        },
      },
    },
  },
  {
    name: 'sftp_write_file',
    description:
      'Write content to a file on the remote server via SFTP. Creates the file if it does not exist, overwrites if it does. For safe edits, pass expected_hash from a prior sftp_read_file result.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the file to write.',
        },
        content: {
          type: 'string',
          description: 'The content to write to the file.',
        },
        encoding: {
          type: 'string',
          description: 'Text encoding. Default: utf-8.',
        },
        target_id: {
          type: 'string',
          description: 'Target ID returned by resolve_target. Must be an ssh-node:* or sftp-session:* target.',
        },
        node_id: {
          type: 'string',
          description: 'Legacy target node ID. Prefer target_id from resolve_target.',
        },
        expected_hash: {
          type: 'string',
          description: 'Optional content hash from a prior sftp_read_file result. If the remote file changed, the write fails with a recoverable error.',
        },
        expected_mtime: {
          type: 'number',
          description: 'Optional mtime from a prior sftp_read_file or sftp_stat result. If the remote file changed, the write fails with a recoverable error.',
        },
        create_only: {
          type: 'boolean',
          description: 'If true, fail when the target path already exists.',
        },
        append: {
          type: 'boolean',
          description: 'If true, append content to the existing text file instead of replacing it.',
        },
        dry_run: {
          type: 'boolean',
          description: 'If true, validate and return a diff summary without writing.',
        },
      },
      required: ['path', 'content'],
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
      'Get the current content of a file open in the IDE editor. Returns content, language, dirty status, cursor position, and a content hash for safe edits.',
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
    name: 'ide_replace_string',
    description:
      'Replace the first occurrence of an exact string in a file open in the IDE editor. Include at least 3 lines of surrounding context in old_string to ensure a unique match. Only replaces the first match.',
    parameters: {
      type: 'object',
      properties: {
        tab_id: {
          type: 'string',
          description: 'The IDE tab ID (from ide_get_open_files).',
        },
        old_string: {
          type: 'string',
          description: 'The exact string to find and replace. Must match precisely including whitespace and indentation.',
        },
        new_string: {
          type: 'string',
          description: 'The replacement string.',
        },
        save: {
          type: 'boolean',
          description: 'Whether to save the file after editing. Default: false.',
        },
        expected_hash: {
          type: 'string',
          description: 'Optional content hash from ide_get_file_content. If the editor buffer changed, the replace fails with a recoverable error.',
        },
        dry_run: {
          type: 'boolean',
          description: 'If true, validate uniqueness and return a diff summary without editing.',
        },
      },
      required: ['tab_id', 'old_string', 'new_string'],
    },
  },
  {
    name: 'ide_insert_text',
    description:
      'Insert text at a specific line number in a file open in the IDE editor. The text is inserted before the specified line.',
    parameters: {
      type: 'object',
      properties: {
        tab_id: {
          type: 'string',
          description: 'The IDE tab ID (from ide_get_open_files).',
        },
        line: {
          type: 'number',
          description: 'Line number to insert before (1-based). Use 1 to insert at the beginning of the file.',
        },
        text: {
          type: 'string',
          description: 'The text to insert. Can be multi-line.',
        },
        save: {
          type: 'boolean',
          description: 'Whether to save the file after editing. Default: false.',
        },
      },
      required: ['tab_id', 'line', 'text'],
    },
  },
  {
    name: 'ide_open_file',
    description:
      'Open a remote file in the IDE editor. Returns the tab ID for subsequent editing operations. If the file is already open, activates its tab.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path to the remote file to open.',
        },
      },
      required: ['path'],
    },
  },
  {
    name: 'ide_create_file',
    description:
      'Create a new file on the remote server and open it in the IDE editor. The parent directory must exist.',
    parameters: {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'Absolute path for the new file (e.g. /home/user/project/newfile.ts).',
        },
        content: {
          type: 'string',
          description: 'Initial file content. Default: empty file.',
        },
      },
      required: ['path'],
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
  {
    name: 'open_local_terminal',
    description: 'Open a new local terminal tab in the UI and return its session ID. Use this when the user asks to open/create a local terminal.',
    parameters: {
      type: 'object',
      properties: {
        cwd: { type: 'string', description: 'Initial working directory. Optional.' },
      },
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Navigation Tools — Open / switch to tabs (always available)
// ═══════════════════════════════════════════════════════════════════════════

export const NAVIGATION_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'open_tab',
    description: 'Open or switch to a singleton tab in the UI. Supported tab types: settings, connection_monitor, connection_pool, topology, file_manager, session_manager, plugin_manager, launcher.',
    parameters: {
      type: 'object',
      properties: {
        tab_type: {
          type: 'string',
          enum: ['settings', 'connection_monitor', 'connection_pool', 'topology', 'file_manager', 'session_manager', 'plugin_manager', 'launcher'],
          description: 'The type of tab to open.',
        },
      },
      required: ['tab_type'],
    },
  },
  {
    name: 'open_session_tab',
    description: 'Open an SFTP, IDE, or port-forwarding tab for a specific SSH session. Requires a connected SSH session. Use list_sessions to discover available node_id values.',
    parameters: {
      type: 'object',
      properties: {
        tab_type: {
          type: 'string',
          enum: ['sftp', 'ide', 'forwards'],
          description: 'The type of session tab to open.',
        },
        node_id: {
          type: 'string',
          description: 'The node ID of the SSH session. Use list_sessions to discover available nodes.',
        },
      },
      required: ['tab_type', 'node_id'],
    },
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
        target_id: { type: 'string', description: 'Target ID returned by resolve_target. Use app:settings for settings changes.' },
        section: { type: 'string', description: 'Settings section (e.g. "terminal", "appearance", "ai", "sftp").' },
        key: { type: 'string', description: 'The setting key within the section.' },
        value: { description: 'The new value for the setting.' },
      },
      required: ['section', 'key', 'value'],
    },
  },
  {
    name: 'open_settings_section',
    description: 'Open the Settings tab and return a short guide for a settings section. Use this when the user asks to inspect or change settings from outside the Settings tab.',
    parameters: {
      type: 'object',
      properties: {
        section: {
          type: 'string',
          enum: ['terminal', 'appearance', 'connectionDefaults', 'sftp', 'ide', 'reconnect', 'general', 'ai', 'localTerminal'],
          description: 'Settings section to open or inspect.',
        },
      },
      required: ['section'],
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
    description: 'Update SSH connection pool configuration (idle timeout and max connections).',
    parameters: {
      type: 'object',
      properties: {
        idle_timeout_secs: { type: 'number', minimum: 0, description: 'Idle timeout in seconds before recycling a connection. 0 = no timeout.' },
        max_connections: { type: 'number', minimum: 1, maximum: 100, description: 'Maximum number of pooled connections.' },
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
  {
    name: 'connect_saved_session',
    description: 'Connect to a saved SSH connection and open a terminal tab. Use `list_saved_connections` or `search_saved_connections` first to find the connection ID. Handles proxy chains, authentication (from OS keychain), and host key verification automatically. Returns the session ID and node ID for the new terminal.',
    parameters: {
      type: 'object',
      properties: {
        connection_id: {
          type: 'string',
          description: 'The saved connection ID (from list_saved_connections or search_saved_connections).',
        },
      },
      required: ['connection_id'],
    },
  },
  {
    name: 'connect_saved_connection_by_query',
    description: 'Find a saved SSH connection by natural-language query and connect to it. Prefer this over opening a local terminal and manually running ssh when the user asks to connect to a saved host. If multiple connections match, returns candidates and asks for disambiguation instead of guessing.',
    parameters: {
      type: 'object',
      properties: {
        query: {
          type: 'string',
          description: 'Search query for the saved connection name, host, group, or username.',
        },
        connection_id: {
          type: 'string',
          description: 'Optional explicit saved connection ID from a previous disambiguation result.',
        },
      },
      required: ['query'],
    },
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
// MCP Resource Tools — Available when any MCP server exposes resources
// ═══════════════════════════════════════════════════════════════════════════

export const MCP_RESOURCE_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'list_mcp_resources',
    description: 'List all resources available from connected MCP servers. Returns URI, name, description, and MIME type for each resource.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'read_mcp_resource',
    description: 'Read the content of a specific MCP resource by its URI. Returns text or base64-encoded binary data.',
    parameters: {
      type: 'object',
      properties: {
        server_id: {
          type: 'string',
          description: 'The MCP server ID that owns this resource.',
        },
        uri: {
          type: 'string',
          description: 'The resource URI to read (as returned by list_mcp_resources).',
        },
      },
      required: ['server_id', 'uri'],
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Status & Observability Tools — Always available (CONTEXT_FREE)
// ═══════════════════════════════════════════════════════════════════════════

export const STATUS_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'get_event_log',
    description: 'Read the connection lifecycle event log. Shows connection, reconnect, and node events with severity, category, and timestamps. Useful for diagnosing connection issues.',
    parameters: {
      type: 'object',
      properties: {
        severity: {
          type: 'string',
          enum: ['info', 'warn', 'error'],
          description: 'Filter by severity level. If omitted, returns all severities.',
        },
        category: {
          type: 'string',
          enum: ['connection', 'reconnect', 'node'],
          description: 'Filter by event category. If omitted, returns all categories.',
        },
        limit: {
          type: 'number',
          minimum: 1,
          maximum: 200,
          description: 'Maximum number of entries to return. Default: 50.',
        },
      },
    },
  },
  {
    name: 'get_transfer_status',
    description: 'Get the current SFTP file transfer queue status. Shows pending, active, paused, completed, and failed transfers with progress and speed info.',
    parameters: {
      type: 'object',
      properties: {
        node_id: {
          type: 'string',
          description: 'Filter transfers by node ID. If omitted, returns all transfers.',
        },
        state: {
          type: 'string',
          enum: ['pending', 'active', 'paused', 'completed', 'cancelled', 'error'],
          description: 'Filter by transfer state. If omitted, returns all states.',
        },
      },
    },
  },
  {
    name: 'get_recording_status',
    description: 'Check if any terminal sessions are currently being recorded (asciicast format). Returns recording metadata, elapsed time, and event count for each active recording.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'get_broadcast_status',
    description: 'Check broadcast mode status. When enabled, keyboard input is sent simultaneously to multiple terminal panes. Returns whether broadcast is active and which panes are targeted.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'get_plugin_details',
    description: 'Get detailed information about installed plugins including runtime state, error messages, and recent log entries. Extends list_plugins with diagnostic data.',
    parameters: {
      type: 'object',
      properties: {
        plugin_id: {
          type: 'string',
          description: 'Specific plugin ID to inspect. If omitted, returns summary of all plugins with error counts.',
        },
      },
    },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// SSH Environment & Topology Tools — Always available (CONTEXT_FREE)
// ═══════════════════════════════════════════════════════════════════════════

export const INFRA_EXTRA_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'get_ssh_environment',
    description: 'Get local SSH environment info: configured hosts from ~/.ssh/config, available SSH keys, and whether an SSH agent is running. Useful for connection troubleshooting.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'get_topology',
    description: 'Get the network topology graph showing all saved SSH connections as nodes and their relationships (jump chains, direct connections) as edges. Useful for understanding multi-hop SSH paths.',
    parameters: { type: 'object', properties: {} },
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// RAG Document Search Tools — Always available (CONTEXT_FREE)
// ═══════════════════════════════════════════════════════════════════════════

export const RAG_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'search_docs',
    description: 'Search user-imported operations documentation (runbooks, guides, SOPs) using keyword matching. Returns the most relevant document chunks with section paths and sources. Use this when the user asks about deployment, configuration, troubleshooting, or any domain-specific operations knowledge.',
    parameters: {
      type: 'object',
      properties: {
        query: {
          type: 'string',
          description: 'Search query — natural language question or keywords',
        },
        top_k: {
          type: 'number',
          description: 'Number of results to return (default: 5, max: 10)',
        },
      },
      required: ['query'],
    },
  },
];

export const ALL_BUILTIN_TOOL_DEFS: AiToolDefinition[] = [
  ...BUILTIN_TOOLS,
  ...SFTP_TOOL_DEFS,
  ...IDE_TOOL_DEFS,
  ...LOCAL_TOOL_DEFS,
  ...SETTINGS_TOOL_DEFS,
  ...POOL_TOOL_DEFS,
  ...MONITOR_TOOL_DEFS,
  ...SESSION_MGR_TOOL_DEFS,
  ...PLUGIN_TOOL_DEFS,
  ...MCP_RESOURCE_TOOL_DEFS,
  ...NAVIGATION_TOOL_DEFS,
  ...STATUS_TOOL_DEFS,
  ...INFRA_EXTRA_TOOL_DEFS,
  ...RAG_TOOL_DEFS,
];

// ═══════════════════════════════════════════════════════════════════════════
// Tool System v3 Registry Types
// ═══════════════════════════════════════════════════════════════════════════

export type ToolDomain =
  | 'terminal'
  | 'remote_filesystem'
  | 'sftp'
  | 'ide'
  | 'local_terminal'
  | 'settings'
  | 'connection'
  | 'connection_pool'
  | 'monitoring'
  | 'navigation'
  | 'plugin'
  | 'mcp'
  | 'rag'
  | 'status';

export type ToolIntent =
  | 'command'
  | 'terminal_interaction'
  | 'connection'
  | 'settings'
  | 'remote_file'
  | 'local_shell'
  | 'sftp'
  | 'ide'
  | 'monitoring'
  | 'navigation'
  | 'plugin'
  | 'knowledge'
  | 'status';

export type ToolTargetRequirement =
  | 'none'
  | 'active_or_node'
  | 'session_id'
  | 'ssh_session'
  | 'active_sftp'
  | 'active_ide'
  | 'local_terminal'
  | 'app_tab'
  | 'mcp_server';

export type ToolSideEffect = 'read' | 'write' | 'execute' | 'navigate' | 'network' | 'settings';

export type ToolSpec = {
  name: string;
  definition: AiToolDefinition;
  domain: ToolDomain;
  intentTags: ToolIntent[];
  requiredTarget: ToolTargetRequirement;
  sideEffect: ToolSideEffect;
  groupKey?: string;
  readOnly: boolean;
  write: boolean;
  contextFree: boolean;
  sessionIdTool: boolean;
  experimental: boolean;
};

export type ExternalToolSpecInput = {
  definition: AiToolDefinition;
  domain?: ToolDomain;
  intentTags?: ToolIntent[];
  requiredTarget?: ToolTargetRequirement;
  sideEffect?: ToolSideEffect;
  groupKey?: string;
  readOnly?: boolean;
  write?: boolean;
  contextFree?: boolean;
  sessionIdTool?: boolean;
  experimental?: boolean;
};

// ═══════════════════════════════════════════════════════════════════════════
// Safety Classification
// ═══════════════════════════════════════════════════════════════════════════

/** Tools that only read data — safe for auto-approve */
export const READ_ONLY_TOOLS = new Set([
  'read_file',
  'list_directory',
  'grep_search',
  'git_status',
  'resolve_target',
  'list_tabs',
  'list_sessions',
  'list_targets',
  'list_capabilities',
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
  'open_settings_section',
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
  // MCP resources (read-only)
  'list_mcp_resources',
  'read_mcp_resource',
  // Status & observability (all read-only)
  'get_event_log',
  'get_transfer_status',
  'get_recording_status',
  'get_broadcast_status',
  'get_plugin_details',
  // SSH environment & topology (read-only)
  'get_ssh_environment',
  'get_topology',
  // RAG document search (read-only)
  'search_docs',
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
  // IDE (write)
  'ide_replace_string',
  'ide_insert_text',
  'ide_create_file',
  // Local terminal (write)
  'local_exec',
  // Settings (write)
  'update_setting',
  // Connection pool (write)
  'set_pool_config',
  // Meta tools (write)
  'send_control_sequence',
  'batch_exec',
  // Navigation tools (write — open session-dependent tabs)
  'open_session_tab',
  // Session manager (write — connect)
  'connect_saved_session',
  'connect_saved_connection_by_query',
  // SFTP (write)
  'sftp_write_file',
  // TUI interaction (experimental, write)
  'send_keys',
  'send_mouse',
]);

/** Tools that do NOT require any node context — work globally or read from local stores */
export const CONTEXT_FREE_TOOLS = new Set([
  'resolve_target',
  'list_tabs',
  'list_sessions',
  'list_targets',
  'list_capabilities',
  'list_connections',
  'get_connection_health',
  // IDE tools read from local Zustand store, no node resolution needed
  'ide_get_open_files',
  'ide_get_file_content',
  'ide_get_project_info',
  'ide_replace_string',
  'ide_insert_text',
  // ide_open_file / ide_create_file: self-resolve node via ideStore.nodeId (not pure local,
  // but don't need the executor's resolveNodeForTool — they gate on ideStore.nodeId internally)
  'ide_open_file',
  'ide_create_file',
  // Local terminal tools
  'local_list_shells',
  'local_get_terminal_info',
  'local_exec',
  'local_get_drives',
  'open_local_terminal',
  // Navigation tools
  'open_tab',
  'open_session_tab',
  // Settings tools
  'get_settings',
  'update_setting',
  'open_settings_section',
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
  'connect_saved_session',
  'connect_saved_connection_by_query',
  // Plugin manager tools
  'list_plugins',
  // MCP resource tools
  'list_mcp_resources',
  'read_mcp_resource',
  // Status & observability tools
  'get_event_log',
  'get_transfer_status',
  'get_recording_status',
  'get_broadcast_status',
  'get_plugin_details',
  // SSH environment & topology tools
  'get_ssh_environment',
  'get_topology',
  // RAG document search
  'search_docs',
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

/**
 * Tools that require a remote SSH node context. Hide them when the active tab
 * is a local terminal to reduce local/remote routing mistakes.
 */
export const REMOTE_NODE_ONLY_TOOLS = new Set([
  'read_file',
  'write_file',
  'list_directory',
  'grep_search',
  'git_status',
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
]);

/** Tools only shown when local_terminal tab is active */
export const LOCAL_ONLY_TOOLS = new Set([
  'local_list_shells',
  'local_get_terminal_info',
  'local_exec',
  'local_get_drives',
  // Note: open_local_terminal is intentionally NOT in this set — it must be
  // available from any context so the agent can create a terminal tab.
]);

/** Tools only shown when settings tab is active */
export const SETTINGS_ONLY_TOOLS = new Set([
  'get_settings',
  'update_setting',
  'open_settings_section',
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
  'connect_saved_connection_by_query',
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
    readOnly: ['resolve_target', 'list_tabs', 'list_sessions', 'list_targets', 'list_capabilities', 'get_terminal_buffer', 'search_terminal', 'await_terminal_output'],
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
    write: ['sftp_write_file'],
  },
  {
    groupKey: 'ide',
    readOnly: ['ide_get_open_files', 'ide_get_file_content', 'ide_get_project_info'],
    write: ['ide_replace_string', 'ide_insert_text', 'ide_open_file', 'ide_create_file'],
  },
  {
    groupKey: 'local_terminal',
    readOnly: ['local_list_shells', 'local_get_terminal_info', 'local_get_drives'],
    write: ['local_exec', 'open_local_terminal'],
  },
  {
    groupKey: 'settings',
    readOnly: ['get_settings', 'open_settings_section'],
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
    write: ['connect_saved_session', 'connect_saved_connection_by_query'],
  },
  {
    groupKey: 'plugin_manager',
    readOnly: ['list_plugins'],
    write: [],
  },
  {
    groupKey: 'mcp',
    readOnly: ['list_mcp_resources', 'read_mcp_resource'],
    write: [],
  },
  {
    groupKey: 'navigation',
    readOnly: ['open_tab'],
    write: ['open_session_tab'],
  },
  {
    groupKey: 'status',
    readOnly: ['get_event_log', 'get_transfer_status', 'get_recording_status', 'get_broadcast_status', 'get_plugin_details', 'get_ssh_environment', 'get_topology'],
    write: [],
  },
  {
    groupKey: 'rag',
    readOnly: ['search_docs'],
    write: [],
  },
];

const GROUP_KEY_BY_TOOL = new Map<string, string>(
  TOOL_GROUPS.flatMap((group) =>
    [...group.readOnly, ...group.write].map((toolName) => [toolName, group.groupKey] as const)
  )
);

function inferToolDomain(toolName: string): ToolDomain {
  if (SFTP_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'sftp';
  if (IDE_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'ide';
  if (LOCAL_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'local_terminal';
  if (SETTINGS_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'settings';
  if (POOL_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'connection_pool';
  if (MONITOR_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'monitoring';
  if (SESSION_MGR_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'connection';
  if (PLUGIN_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'plugin';
  if (MCP_RESOURCE_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'mcp';
  if (NAVIGATION_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'navigation';
  if (STATUS_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'status';
  if (INFRA_EXTRA_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'connection';
  if (RAG_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'rag';
  if (['read_file', 'write_file', 'list_directory', 'grep_search', 'git_status'].includes(toolName)) return 'remote_filesystem';
  if (['list_connections', 'list_port_forwards', 'get_detected_ports', 'get_connection_health', 'create_port_forward', 'stop_port_forward'].includes(toolName)) return 'connection';
  return 'terminal';
}

function inferIntentTags(toolName: string, domain: ToolDomain): ToolIntent[] {
  const overrides: Record<string, ToolIntent[]> = {
    terminal_exec: ['command', 'terminal_interaction'],
    batch_exec: ['command', 'terminal_interaction'],
    get_terminal_buffer: ['terminal_interaction', 'status'],
    search_terminal: ['terminal_interaction'],
    await_terminal_output: ['terminal_interaction'],
    send_control_sequence: ['terminal_interaction'],
    read_screen: ['terminal_interaction'],
    send_keys: ['terminal_interaction'],
    send_mouse: ['terminal_interaction'],
    open_local_terminal: ['local_shell', 'navigation'],
    local_exec: ['local_shell', 'command'],
    open_tab: ['navigation'],
    open_session_tab: ['navigation', 'connection'],
    list_saved_connections: ['connection'],
    search_saved_connections: ['connection'],
    connect_saved_session: ['connection'],
    get_session_tree: ['connection', 'status'],
    get_settings: ['settings'],
    update_setting: ['settings'],
    resolve_target: ['connection', 'settings', 'command', 'terminal_interaction'],
    search_docs: ['knowledge'],
  };

  if (overrides[toolName]) return overrides[toolName];

  switch (domain) {
    case 'remote_filesystem':
      return ['remote_file'];
    case 'sftp':
      return ['sftp', 'remote_file'];
    case 'ide':
      return ['ide', 'remote_file'];
    case 'local_terminal':
      return ['local_shell'];
    case 'settings':
      return ['settings'];
    case 'connection':
    case 'connection_pool':
      return ['connection'];
    case 'monitoring':
      return ['monitoring'];
    case 'navigation':
      return ['navigation'];
    case 'plugin':
      return ['plugin'];
    case 'mcp':
    case 'rag':
      return ['knowledge'];
    case 'status':
      return ['status'];
    default:
      return ['command'];
  }
}

function inferRequiredTarget(toolName: string): ToolTargetRequirement {
  if (toolName === 'resolve_target') return 'none';
  if (SESSION_ID_TOOLS.has(toolName)) return 'session_id';
  if (SFTP_ONLY_TOOLS.has(toolName)) return 'active_sftp';
  if (IDE_ONLY_TOOLS.has(toolName)) return 'active_ide';
  if (LOCAL_ONLY_TOOLS.has(toolName)) return 'local_terminal';
  if (SETTINGS_ONLY_TOOLS.has(toolName) || POOL_ONLY_TOOLS.has(toolName) || MONITOR_ONLY_TOOLS.has(toolName) || SESSION_MGR_ONLY_TOOLS.has(toolName) || PLUGIN_MGR_ONLY_TOOLS.has(toolName)) return 'app_tab';
  if (toolName === 'open_session_tab') return 'ssh_session';
  if (MCP_RESOURCE_TOOL_DEFS.some((tool) => tool.name === toolName)) return 'mcp_server';
  if (REMOTE_NODE_ONLY_TOOLS.has(toolName) || SSH_ONLY_TOOLS.has(toolName)) return 'active_or_node';
  return 'none';
}

function inferSideEffect(toolName: string): ToolSideEffect {
  if (toolName === 'open_tab' || toolName === 'open_session_tab' || toolName === 'open_local_terminal') return 'navigate';
  if (toolName === 'update_setting' || toolName === 'set_pool_config') return 'settings';
  if (toolName === 'create_port_forward' || toolName === 'stop_port_forward') return 'network';
  if (['terminal_exec', 'local_exec', 'batch_exec', 'send_control_sequence', 'send_keys', 'send_mouse'].includes(toolName)) return 'execute';
  if (WRITE_TOOLS.has(toolName)) return 'write';
  return 'read';
}

function createToolSpec(definition: AiToolDefinition): ToolSpec {
  const domain = inferToolDomain(definition.name);
  return {
    name: definition.name,
    definition,
    domain,
    intentTags: inferIntentTags(definition.name, domain),
    requiredTarget: inferRequiredTarget(definition.name),
    sideEffect: inferSideEffect(definition.name),
    groupKey: GROUP_KEY_BY_TOOL.get(definition.name),
    readOnly: READ_ONLY_TOOLS.has(definition.name),
    write: WRITE_TOOLS.has(definition.name),
    contextFree: CONTEXT_FREE_TOOLS.has(definition.name),
    sessionIdTool: SESSION_ID_TOOLS.has(definition.name),
    experimental: EXPERIMENTAL_TOOLS.has(definition.name),
  };
}

export const BUILTIN_TOOL_SPECS: ToolSpec[] = ALL_BUILTIN_TOOL_DEFS.map(createToolSpec);

export const TOOL_SPEC_BY_NAME = new Map<string, ToolSpec>(
  BUILTIN_TOOL_SPECS.map((spec) => [spec.name, spec])
);

export function getAllToolSpecs(): ToolSpec[] {
  return BUILTIN_TOOL_SPECS;
}

export function getToolSpec(name: string): ToolSpec | undefined {
  return TOOL_SPEC_BY_NAME.get(name);
}

export function getToolDefinitionByName(name: string): AiToolDefinition | undefined {
  return TOOL_SPEC_BY_NAME.get(name)?.definition;
}

export function createExternalToolSpec(input: ExternalToolSpecInput): ToolSpec {
  const domain = input.domain ?? (input.definition.name.startsWith('mcp::') ? 'mcp' : 'plugin');
  const readOnly = input.readOnly ?? (input.sideEffect === 'read');
  const sideEffect = input.sideEffect ?? (readOnly ? 'read' : 'write');
  return {
    name: input.definition.name,
    definition: input.definition,
    domain,
    intentTags: input.intentTags ?? inferIntentTags(input.definition.name, domain),
    requiredTarget: input.requiredTarget ?? 'none',
    sideEffect,
    groupKey: input.groupKey,
    readOnly,
    write: input.write ?? !readOnly,
    contextFree: input.contextFree ?? true,
    sessionIdTool: input.sessionIdTool ?? false,
    experimental: input.experimental ?? false,
  };
}

/**
 * Get relevant tool definitions based on active tab type and session context.
 * Completely hides tools irrelevant to the active tab, saving tokens and focus.
 *
 * When `participantOverride` is provided (from @participant mentions), those tools
 * bypass the tab-specific filter and are always included.
 */
export function getToolsForContext(
  activeTabType: TabType | null,
  hasAnySSHSession: boolean,
  disabledTools?: Set<string>,
  participantOverride?: Set<string>,
): AiToolDefinition[] {
  return BUILTIN_TOOL_SPECS.filter(spec => {
    const t = spec.definition;
    // User-disabled tools: never sent to LLM
    if (disabledTools?.has(t.name)) return false;

    // Participant override: if a tool was explicitly requested via @participant, always include it
    if (participantOverride?.has(t.name)) return true;

    if (activeTabType === 'local_terminal' && !hasAnySSHSession && REMOTE_NODE_ONLY_TOOLS.has(t.name)) {
      return false;
    }

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
  }).map((spec) => spec.definition);
}

/**
 * Command deny-list for terminal_exec safety.
 * These patterns are checked against the command string before execution.
 * If any pattern matches, the command must never be auto-approved and should
 * require explicit user confirmation before execution.
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

/**
 * Return the specific commands in a tool payload that matched the deny-list.
 * Tools without executable command strings return an empty array.
 */
export function getDeniedCommands(toolName: string, args: Record<string, unknown>): string[] {
  if ((toolName === 'terminal_exec' || toolName === 'local_exec' || toolName === 'run_command') && typeof args.command === 'string') {
    return isCommandDenied(args.command) ? [args.command] : [];
  }

  if (toolName === 'batch_exec' && Array.isArray(args.commands)) {
    return args.commands.filter((cmd): cmd is string => typeof cmd === 'string' && isCommandDenied(cmd));
  }

  return [];
}

export function hasDeniedCommands(toolName: string, args: Record<string, unknown>): boolean {
  return getDeniedCommands(toolName, args).length > 0;
}
