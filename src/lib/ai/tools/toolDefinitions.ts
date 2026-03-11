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
      'Execute a shell command on the connected remote server (or local terminal) and return stdout/stderr. Use this for running shell commands, inspecting system state, building projects, etc.',
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
          description: 'Target node ID. If omitted, uses the active terminal. Use list_sessions to discover nodes.',
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

  // ── Session Discovery Tools ──
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
// Safety Classification
// ═══════════════════════════════════════════════════════════════════════════

/** Tools that only read data — safe for auto-approve */
export const READ_ONLY_TOOLS = new Set([
  'read_file',
  'list_directory',
  'grep_search',
  'git_status',
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
]);

/** Tools that modify state — require explicit user approval */
export const WRITE_TOOLS = new Set([
  'terminal_exec',
  'write_file',
  'create_port_forward',
  'stop_port_forward',
  'ide_apply_edit',
]);

/** Tools that do NOT require any node context — work globally or read from local stores */
export const CONTEXT_FREE_TOOLS = new Set([
  'list_sessions',
  'list_connections',
  'get_connection_health',
  // IDE tools read from local Zustand store, no node resolution needed
  'ide_get_open_files',
  'ide_get_file_content',
  'ide_get_project_info',
  'ide_apply_edit',
]);

/** Tools that use session_id parameter instead of node_id */
export const SESSION_ID_TOOLS = new Set([
  'get_terminal_buffer',
  'search_terminal',
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

/**
 * Get relevant tool definitions based on active tab type and session context.
 * Completely hides tools irrelevant to the active tab, saving tokens and focus.
 */
export function getToolsForContext(
  activeTabType: TabType | null,
  hasAnySSHSession: boolean,
): AiToolDefinition[] {
  // Combine all tools into a single pool
  const allTools = [...BUILTIN_TOOLS, ...SFTP_TOOL_DEFS, ...IDE_TOOL_DEFS];
  
  return allTools.filter(t => {
    // SSH-only tools: hide when only local terminals and no SSH sessions
    if (SSH_ONLY_TOOLS.has(t.name)) {
      if (!hasAnySSHSession) return false;
    }
    
    // SFTP-only tools: only show on SFTP tab
    if (SFTP_ONLY_TOOLS.has(t.name)) {
      return activeTabType === 'sftp';
    }
    
    // IDE-only tools: only show on IDE tab
    if (IDE_ONLY_TOOLS.has(t.name)) {
      return activeTabType === 'ide';
    }
    
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
