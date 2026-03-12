/**
 * AI Tool Executor
 *
 * Dispatches tool calls to the appropriate backend APIs and returns results.
 * Uses the remote agent (JSON-RPC over SSH) when available, with fallback to
 * SFTP/exec for basic operations.
 *
 * Tools are categorized into three routing modes:
 * - CONTEXT_FREE: No nodeId needed (list_sessions, list_connections, get_connection_health)
 * - SESSION_ID: Uses session_id parameter (get_terminal_buffer, search_terminal)
 * - NODE_ID: Resolves target node via explicit node_id param or active terminal fallback
 */

import { nodeIdeExecCommand, nodeGetState, nodeAgentStatus } from '../../api';
import {
  nodeAgentReadFile,
  nodeAgentWriteFile,
  nodeAgentListTree,
  nodeAgentGrep,
  nodeAgentGitStatus,
} from '../../api';
import { nodeSftpListDir, nodeSftpPreview, nodeSftpStat } from '../../api';
import { api } from '../../api';
import type { AiToolResult, AgentFileEntry } from '../../../types';
import { CONTEXT_FREE_TOOLS, SESSION_ID_TOOLS, isCommandDenied } from './toolDefinitions';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import { useAppStore } from '../../../store/appStore';
import { useLocalTerminalStore } from '../../../store/localTerminalStore';
import { useIdeStore } from '../../../store/ideStore';
import { useSettingsStore } from '../../../store/settingsStore';
import { usePluginStore } from '../../../store/pluginStore';
import { findPaneBySessionId, getTerminalBuffer, writeToTerminal, subscribeTerminalOutput, readScreen } from '../../terminalRegistry';

/** Max output size returned from a tool execution (bytes) */
const MAX_OUTPUT_BYTES = 8192;
const MAX_COMMAND_TIMEOUT_SECS = 60;
const MAX_LIST_DEPTH = 8;
const MAX_GREP_RESULTS = 200;
const MAX_PATTERN_LENGTH = 200;
const AUTO_AWAIT_TIMEOUT_SECS = 15;
const AUTO_AWAIT_STABLE_SECS = 2;

/** Context needed to execute tools — activeNodeId may be null when no terminal is focused */
export type ToolExecutionContext = {
  /** Currently active node ID — null when no terminal is focused */
  activeNodeId: string | null;
  /** Whether the active node has remote agent available */
  activeAgentAvailable: boolean;
};

/** Resolved target for a tool that requires a specific node */
type ResolvedNode = {
  nodeId: string;
  agentAvailable: boolean;
};

/**
 * Resolve the target node for a tool call.
 * Priority: explicit node_id parameter > active terminal's node.
 */
async function resolveNodeForTool(
  explicitNodeId: string | undefined,
  context: ToolExecutionContext,
): Promise<ResolvedNode | null> {
  if (explicitNodeId) {
    const nodes = useSessionTreeStore.getState().nodes;
    const node = nodes.find(n => n.id === explicitNodeId);
    if (!node) return null;
    try {
      const snapshot = await nodeGetState(explicitNodeId);
      if (snapshot.state.readiness !== 'ready') return null;
    } catch {
      return null;
    }
    let agentAvailable = false;
    try {
      const agentStatus = await nodeAgentStatus(explicitNodeId);
      agentAvailable = agentStatus.type === 'ready';
    } catch { /* agent unavailable */ }
    return { nodeId: explicitNodeId, agentAvailable };
  }

  if (context.activeNodeId) {
    return { nodeId: context.activeNodeId, agentAvailable: context.activeAgentAvailable };
  }

  return null;
}

/**
 * Execute a tool call and return the result.
 * Dispatches to the appropriate backend based on tool name and routing category.
 */
export async function executeTool(
  toolName: string,
  args: Record<string, unknown>,
  context: ToolExecutionContext,
): Promise<AiToolResult> {
  const startTime = Date.now();
  const toolCallId = `exec-${Date.now()}`;
  const explicitNodeId = typeof args.node_id === 'string' ? args.node_id.trim() : '';

  try {
    // Context-free tools — no node required
    if (CONTEXT_FREE_TOOLS.has(toolName)) {
      switch (toolName) {
        case 'list_tabs':
          return execListTabs(startTime, toolCallId);
        case 'list_sessions':
          return await execListSessions(args, startTime, toolCallId);
        case 'list_connections':
          return await execListConnections(startTime, toolCallId);
        case 'get_connection_health':
          return await execGetConnectionHealth(args, startTime, toolCallId);
        case 'ide_get_open_files':
          return execIdeGetOpenFiles(startTime, toolCallId);
        case 'ide_get_file_content':
          return execIdeGetFileContent(args, startTime, toolCallId);
        case 'ide_get_project_info':
          return execIdeGetProjectInfo(startTime, toolCallId);
        case 'ide_apply_edit':
          return await execIdeApplyEdit(args, startTime, toolCallId);
        // Local terminal tools
        case 'local_list_shells':
          return await execLocalListShells(startTime, toolCallId);
        case 'local_get_terminal_info':
          return await execLocalGetTerminalInfo(startTime, toolCallId);
        case 'local_exec':
          return await execLocalExec(args, startTime, toolCallId);
        case 'local_get_drives':
          return await execLocalGetDrives(startTime, toolCallId);
        // Settings tools
        case 'get_settings':
          return execGetSettings(args, startTime, toolCallId);
        case 'update_setting':
          return execUpdateSetting(args, startTime, toolCallId);
        // Connection pool tools
        case 'get_pool_stats':
          return await execGetPoolStats(startTime, toolCallId);
        case 'set_pool_config':
          return await execSetPoolConfig(args, startTime, toolCallId);
        // Connection monitor tools
        case 'get_all_health':
          return await execGetAllHealth(startTime, toolCallId);
        case 'get_resource_metrics':
          return await execGetResourceMetrics(args, startTime, toolCallId);
        // Session manager tools
        case 'list_saved_connections':
          return await execListSavedConnections(startTime, toolCallId);
        case 'search_saved_connections':
          return await execSearchSavedConnections(args, startTime, toolCallId);
        case 'get_session_tree':
          return await execGetSessionTree(startTime, toolCallId);
        // Plugin manager tools
        case 'list_plugins':
          return execListPlugins(startTime, toolCallId);
        default:
          return { toolCallId, toolName, success: false, output: '', error: `Unknown context-free tool: ${toolName}`, durationMs: Date.now() - startTime };
      }
    }

    // Session-ID tools — route by session_id parameter
    if (SESSION_ID_TOOLS.has(toolName)) {
      switch (toolName) {
        case 'get_terminal_buffer':
          return await execGetTerminalBuffer(args, startTime, toolCallId);
        case 'search_terminal':
          return await execSearchTerminal(args, startTime, toolCallId);
        case 'await_terminal_output':
          return await execAwaitTerminalOutput(args, startTime, toolCallId);
        case 'send_control_sequence':
          return await execSendControlSequence(args, startTime, toolCallId);
        case 'batch_exec':
          return await execBatchExec(args, startTime, toolCallId);
        case 'read_screen':
          return execReadScreen(args, startTime, toolCallId);
        case 'send_keys':
          return await execSendKeys(args, startTime, toolCallId);
        case 'send_mouse':
          return await execSendMouse(args, startTime, toolCallId);
        default:
          return { toolCallId, toolName, success: false, output: '', error: `Unknown session tool: ${toolName}`, durationMs: Date.now() - startTime };
      }
    }

    if (toolName === 'terminal_exec' && explicitNodeId.length === 0) {
      const sessionId = typeof args.session_id === 'string' ? args.session_id.trim() : '';
      if (sessionId) {
        return await execTerminalCommandToSession(args, sessionId, startTime, toolCallId);
      }
    }

    // Node-ID tools — resolve target node
    const resolved = await resolveNodeForTool(explicitNodeId || undefined, context);
    if (!resolved) {
      return {
        toolCallId,
        toolName,
        success: false,
        output: '',
        error: 'No target node or terminal session available. Use list_sessions to find a target, then pass node_id or session_id.',
        durationMs: Date.now() - startTime,
      };
    }

    switch (toolName) {
      case 'terminal_exec':
        return await execTerminalCommand(args, resolved, startTime, toolCallId);
      case 'read_file':
        return await execReadFile(args, resolved, startTime, toolCallId);
      case 'write_file':
        return await execWriteFile(args, resolved, startTime, toolCallId);
      case 'list_directory':
        return await execListDirectory(args, resolved, startTime, toolCallId);
      case 'grep_search':
        return await execGrepSearch(args, resolved, startTime, toolCallId);
      case 'git_status':
        return await execGitStatus(args, resolved, startTime, toolCallId);
      case 'list_port_forwards':
        return await execListPortForwards(args, resolved, startTime, toolCallId);
      case 'get_detected_ports':
        return await execGetDetectedPorts(args, resolved, startTime, toolCallId);
      case 'create_port_forward':
        return await execCreatePortForward(args, resolved, startTime, toolCallId);
      case 'stop_port_forward':
        return await execStopPortForward(args, resolved, startTime, toolCallId);
      // SFTP tools
      case 'sftp_list_dir':
        return await execSftpListDir(args, resolved, startTime, toolCallId);
      case 'sftp_read_file':
        return await execSftpReadFile(args, resolved, startTime, toolCallId);
      case 'sftp_stat':
        return await execSftpStat(args, resolved, startTime, toolCallId);
      case 'sftp_get_cwd':
        return await execSftpGetCwd(resolved, startTime, toolCallId);
      default: {
        // Check if this is an MCP tool (prefixed with mcp::)
        if (toolName.startsWith('mcp::')) {
          return await executeMcpTool(toolName, args, startTime, toolCallId);
        }
        return { toolCallId, toolName, success: false, output: '', error: `Unknown tool: ${toolName}`, durationMs: Date.now() - startTime };
      }
    }
  } catch (e) {
    return {
      toolCallId,
      toolName,
      success: false,
      output: '',
      error: e instanceof Error ? e.message : String(e),
      durationMs: Date.now() - startTime,
    };
  }
}

function truncateOutput(output: string): { text: string; truncated: boolean } {
  if (output.length <= MAX_OUTPUT_BYTES) return { text: output, truncated: false };
  return { text: output.slice(0, MAX_OUTPUT_BYTES) + '\n... (output truncated)', truncated: true };
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum);
}

function hasPotentiallyCatastrophicRegex(pattern: string): boolean {
  return /(\([^)]*[+*][^)]*\))[+*]|([+*])\1/.test(pattern);
}

// ═══════════════════════════════════════════════════════════════════════════
// Individual Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

async function execTerminalCommand(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const command = typeof args.command === 'string' ? args.command.trim() : '';
  if (!command) {
    return { toolCallId, toolName: 'terminal_exec', success: false, output: '', error: 'Missing required argument: command', durationMs: Date.now() - startTime };
  }

  const cwd = args.cwd as string | undefined;
  const timeoutSecs = clamp(Number(args.timeout_secs) || 30, 1, MAX_COMMAND_TIMEOUT_SECS);

  const result = await nodeIdeExecCommand(resolved.nodeId, command, cwd, timeoutSecs);
  const combined = result.stderr
    ? `${result.stdout}\n--- stderr ---\n${result.stderr}`
    : result.stdout;

  const { text, truncated } = truncateOutput(combined);

  return {
    toolCallId,
    toolName: 'terminal_exec',
    success: result.exitCode === 0 || result.exitCode === null,
    output: text,
    error: result.exitCode !== 0 && result.exitCode !== null ? `Exit code: ${result.exitCode}` : undefined,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execTerminalCommandToSession(
  args: Record<string, unknown>,
  sessionId: string,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const command = typeof args.command === 'string' ? args.command.trim() : '';
  if (!command) {
    return { toolCallId, toolName: 'terminal_exec', success: false, output: '', error: 'Missing required argument: command', durationMs: Date.now() - startTime };
  }

  if (isCommandDenied(command)) {
    return { toolCallId, toolName: 'terminal_exec', success: false, output: '', error: 'Command rejected: matches deny-list pattern.', durationMs: Date.now() - startTime };
  }

  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return {
      toolCallId,
      toolName: 'terminal_exec',
      success: false,
      output: '',
      error: `Open terminal session not found: ${sessionId}`,
      durationMs: Date.now() - startTime,
    };
  }

  const sent = writeToTerminal(paneId, `${command}\r`);
  if (!sent) {
    return {
      toolCallId,
      toolName: 'terminal_exec',
      success: false,
      output: '',
      error: `Terminal session is not writable: ${sessionId}`,
      durationMs: Date.now() - startTime,
    };
  }

  // Auto-await output (default: true)
  const awaitOutput = args.await_output !== false;
  if (!awaitOutput) {
    return {
      toolCallId,
      toolName: 'terminal_exec',
      success: true,
      output: `Command sent to terminal session ${sessionId}: ${command}`,
      durationMs: Date.now() - startTime,
    };
  }

  const waitResult = await waitForTerminalOutput(
    sessionId,
    AUTO_AWAIT_TIMEOUT_SECS,
    AUTO_AWAIT_STABLE_SECS,
    null,
    startTime,
  );
  return {
    toolCallId,
    toolName: 'terminal_exec',
    success: waitResult.success,
    output: waitResult.output,
    error: waitResult.error,
    truncated: waitResult.truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execReadFile(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = args.path as string;
  if (!path) {
    return { toolCallId, toolName: 'read_file', success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  }

  if (resolved.agentAvailable) {
    const result = await nodeAgentReadFile(resolved.nodeId, path);
    const { text, truncated } = truncateOutput(result.content);
    return { toolCallId, toolName: 'read_file', success: true, output: text, truncated, durationMs: Date.now() - startTime };
  }

  // Fallback: exec cat via SSH
  const result = await nodeIdeExecCommand(resolved.nodeId, `cat ${shellEscape(path)}`, undefined, 10);
  const { text, truncated } = truncateOutput(result.stdout);
  return {
    toolCallId,
    toolName: 'read_file',
    success: result.exitCode === 0,
    output: text,
    error: result.exitCode !== 0 ? result.stderr : undefined,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execWriteFile(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = args.path as string;
  const content = args.content as string;
  if (!path || content === undefined) {
    return { toolCallId, toolName: 'write_file', success: false, output: '', error: 'Missing required arguments: path, content', durationMs: Date.now() - startTime };
  }

  if (resolved.agentAvailable) {
    const result = await nodeAgentWriteFile(resolved.nodeId, path, content);
    return { toolCallId, toolName: 'write_file', success: true, output: `Written ${result.size} bytes to ${path} (hash: ${result.hash})`, durationMs: Date.now() - startTime };
  }

  return {
    toolCallId,
    toolName: 'write_file',
    success: false,
    output: '',
    error: 'write_file requires remote agent support and is unavailable on exec fallback',
    durationMs: Date.now() - startTime,
  };
}

async function execListDirectory(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = args.path as string;
  if (!path) {
    return { toolCallId, toolName: 'list_directory', success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  }

  const maxDepth = clamp(Number(args.max_depth) || 3, 1, MAX_LIST_DEPTH);

  if (resolved.agentAvailable) {
    const result = await nodeAgentListTree(resolved.nodeId, path, maxDepth, 500);
    const output = formatTreeEntries(result.entries, '') +
      (result.truncated ? '\n... (listing truncated)' : '');
    const { text, truncated } = truncateOutput(output);
    return { toolCallId, toolName: 'list_directory', success: true, output: text, truncated, durationMs: Date.now() - startTime };
  }

  // Fallback: ls via SSH
  const result = await nodeIdeExecCommand(resolved.nodeId, `ls -la ${shellEscape(path)}`, undefined, 10);
  const { text, truncated } = truncateOutput(result.stdout);
  return {
    toolCallId,
    toolName: 'list_directory',
    success: result.exitCode === 0,
    output: text,
    error: result.exitCode !== 0 ? result.stderr : undefined,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execGrepSearch(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const pattern = args.pattern as string;
  const path = args.path as string;
  if (!pattern || !path) {
    return { toolCallId, toolName: 'grep_search', success: false, output: '', error: 'Missing required arguments: pattern, path', durationMs: Date.now() - startTime };
  }

  const caseSensitive = (args.case_sensitive as boolean) ?? false;
  const maxResults = clamp(Number(args.max_results) || 50, 1, MAX_GREP_RESULTS);

  if (pattern.length > MAX_PATTERN_LENGTH) {
    return { toolCallId, toolName: 'grep_search', success: false, output: '', error: `Pattern too long (max ${MAX_PATTERN_LENGTH} characters)`, durationMs: Date.now() - startTime };
  }

  if (hasPotentiallyCatastrophicRegex(pattern)) {
    return { toolCallId, toolName: 'grep_search', success: false, output: '', error: 'Pattern rejected: potentially catastrophic regular expression', durationMs: Date.now() - startTime };
  }

  if (resolved.agentAvailable) {
    const matches = await nodeAgentGrep(resolved.nodeId, pattern, path, caseSensitive, maxResults);
    const output = matches.map((m) => `${m.path}:${m.line}: ${m.text}`).join('\n');
    const { text, truncated } = truncateOutput(output || 'No matches found.');
    return { toolCallId, toolName: 'grep_search', success: true, output: text, truncated, durationMs: Date.now() - startTime };
  }

  // Fallback: grep via SSH
  const flags = caseSensitive ? '-rn' : '-rni';
  const result = await nodeIdeExecCommand(
    resolved.nodeId,
    `grep ${flags} --max-count=${maxResults} ${shellEscape(pattern)} ${shellEscape(path)}`,
    undefined,
    15,
  );
  const { text, truncated } = truncateOutput(result.stdout || 'No matches found.');
  return {
    toolCallId,
    toolName: 'grep_search',
    success: true, // grep returns exit 1 when no match — not an error
    output: text,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execGitStatus(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = args.path as string;
  if (!path) {
    return { toolCallId, toolName: 'git_status', success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  }

  if (resolved.agentAvailable) {
    const result = await nodeAgentGitStatus(resolved.nodeId, path);
    const files = result.files.map((f) => `${f.status} ${f.path}`).join('\n');
    const output = `Branch: ${result.branch}\n${files || '(clean working tree)'}`;
    const { text, truncated } = truncateOutput(output);
    return { toolCallId, toolName: 'git_status', success: true, output: text, truncated, durationMs: Date.now() - startTime };
  }

  // Fallback: git status via SSH
  const result = await nodeIdeExecCommand(resolved.nodeId, 'git status --short --branch', path, 10);
  const { text, truncated } = truncateOutput(result.stdout);
  return {
    toolCallId,
    toolName: 'git_status',
    success: result.exitCode === 0,
    output: text,
    error: result.exitCode !== 0 ? result.stderr : undefined,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Context-Free Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

function execListTabs(
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const { tabs, activeTabId } = useAppStore.getState();
  if (tabs.length === 0) {
    return { toolCallId, toolName: 'list_tabs', success: true, output: 'No tabs open.', durationMs: Date.now() - startTime };
  }

  const lines = tabs.map((tab, i) => {
    const active = tab.id === activeTabId ? ' ★' : '';
    const session = tab.sessionId ? ` session=${tab.sessionId}` : '';
    const node = tab.nodeId ? ` node=${tab.nodeId}` : '';
    return `${i + 1}. [${tab.type}] id=${tab.id} "${tab.title}"${session}${node}${active}`;
  });

  lines.push(`\nActive tab: ${activeTabId ?? '(none)'}`);
  lines.push(`Total: ${tabs.length} tab(s)`);
  return { toolCallId, toolName: 'list_tabs', success: true, output: lines.join('\n'), durationMs: Date.now() - startTime };
}

async function execListSessions(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const filter = (args.session_type as string) || 'all';
  const lines: string[] = [];

  if (filter === 'all' || filter === 'ssh') {
    const nodes = useSessionTreeStore.getState().nodes;
    const connections = useAppStore.getState().connections;

    lines.push('## SSH Sessions');
    const sshNodes = nodes.filter(n => n.runtime?.connectionId || n.runtime?.status === 'connected' || n.runtime?.status === 'active');
    if (sshNodes.length === 0) {
      lines.push('(none)');
    } else {
      for (const node of sshNodes) {
        const conn = node.runtime.connectionId ? connections.get(node.runtime.connectionId) : undefined;
        const status = node.runtime.status || 'unknown';
        const terminals = node.runtime.terminalIds?.length ?? 0;
        const host = conn ? `${conn.username}@${conn.host}:${conn.port}` : `${node.username}@${node.host}:${node.port}`;
        const env = conn?.remoteEnv
          ? ` (${conn.remoteEnv.osType}${conn.remoteEnv.osVersion ? ' ' + conn.remoteEnv.osVersion : ''})`
          : '';
        const terminalIds = node.runtime.terminalIds?.length
          ? ` [terminals: ${node.runtime.terminalIds.join(', ')}]`
          : '';
        lines.push(`- [${status}] node_id=${node.id} → ${host}${env} — ${terminals} terminal(s)${terminalIds}`);
      }
    }
    lines.push('');
  }

  if (filter === 'all' || filter === 'local') {
    const localTerminals = useLocalTerminalStore.getState().terminals;

    lines.push('## Local Terminals');
    if (localTerminals.size === 0) {
      lines.push('(none)');
    } else {
      for (const [sessionId, info] of localTerminals) {
        const shellName = info.shell?.label || info.shell?.path || 'shell';
        const state = info.running ? 'running' : 'stopped';
        lines.push(`- session_id=${sessionId} → ${shellName} (${state})`);
      }
    }
  }

  const output = lines.join('\n');
  return { toolCallId, toolName: 'list_sessions', success: true, output, durationMs: Date.now() - startTime };
}

async function execListConnections(
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const connections = await api.sshListConnections();
  if (connections.length === 0) {
    return { toolCallId, toolName: 'list_connections', success: true, output: 'No SSH connections.', durationMs: Date.now() - startTime };
  }

  const lines = connections.map(conn => {
    const env = conn.remoteEnv
      ? ` (${conn.remoteEnv.osType}${conn.remoteEnv.osVersion ? ' ' + conn.remoteEnv.osVersion : ''})`
      : '';
    return `- [${conn.state}] id=${conn.id} → ${conn.username}@${conn.host}:${conn.port}${env} — ${conn.terminalIds.length} terminal(s), ${conn.forwardIds.length} forward(s), keepAlive=${conn.keepAlive}`;
  });

  return { toolCallId, toolName: 'list_connections', success: true, output: lines.join('\n'), durationMs: Date.now() - startTime };
}

async function execGetConnectionHealth(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const nodeId = args.node_id as string | undefined;

  if (nodeId) {
    // Specific node health
    const nodes = useSessionTreeStore.getState().nodes;
    const node = nodes.find(n => n.id === nodeId);
    if (!node) {
      return { toolCallId, toolName: 'get_connection_health', success: false, output: '', error: `Node not found: ${nodeId}`, durationMs: Date.now() - startTime };
    }
    const terminalId = node.runtime.terminalIds?.[0];
    if (!terminalId) {
      return { toolCallId, toolName: 'get_connection_health', success: false, output: '', error: 'No terminal sessions on this node.', durationMs: Date.now() - startTime };
    }
    const health = await api.getQuickHealth(terminalId);
    return {
      toolCallId, toolName: 'get_connection_health', success: true,
      output: `Status: ${health.status}, Latency: ${health.latency_ms !== null ? health.latency_ms + 'ms' : 'N/A'}, Message: ${health.message}`,
      durationMs: Date.now() - startTime,
    };
  }

  // All connections health
  const allHealth = await api.getAllHealthStatus();
  const entries = Object.entries(allHealth);
  if (entries.length === 0) {
    return { toolCallId, toolName: 'get_connection_health', success: true, output: 'No active connections.', durationMs: Date.now() - startTime };
  }
  const lines = entries.map(([sessionId, h]) =>
    `- session=${sessionId}: ${h.status}, latency=${h.latency_ms !== null ? h.latency_ms + 'ms' : 'N/A'}`
  );
  return { toolCallId, toolName: 'get_connection_health', success: true, output: lines.join('\n'), durationMs: Date.now() - startTime };
}

// ═══════════════════════════════════════════════════════════════════════════
// Session-ID Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Semantic buffer sampling: keeps the last TAIL_SIZE lines in full,
 * then filters the older lines to retain only "interesting" ones
 * (commands, errors, warnings, status changes). This preserves
 * context depth while cutting token consumption by 60%+.
 */
const SEMANTIC_TAIL_SIZE = 50;
const SEMANTIC_KEYWORDS = /\b(error|fail|fatal|panic|exception|denied|warning|warn|exit|killed|timeout|refused|not found|no such|segfault|oom|abort|SIGTERM|SIGKILL|SIGSEGV)\b/i;
const PROMPT_PATTERN = /^[\s]*[\$#>%»›]\s|^\[.*@.*\][\$#]\s|^[a-zA-Z0-9._-]+@[a-zA-Z0-9._-]+[:\s]/;
const SEPARATOR_PATTERN = /^[-=]{4,}$|^#{1,3}\s/;

function semanticSample(lines: string[], maxLines: number): string[] {
  if (lines.length <= SEMANTIC_TAIL_SIZE) return lines;

  // Split: older head vs recent tail
  const tailStart = Math.max(0, lines.length - SEMANTIC_TAIL_SIZE);
  const tail = lines.slice(tailStart);
  const head = lines.slice(0, tailStart);

  // Filter head: keep only interesting lines
  const sampledHead: string[] = [];
  for (let i = 0; i < head.length; i++) {
    const line = head[i];
    if (
      SEMANTIC_KEYWORDS.test(line) ||
      PROMPT_PATTERN.test(line) ||
      SEPARATOR_PATTERN.test(line)
    ) {
      sampledHead.push(line);
    }
  }

  // Build output — always include omitted marker when lines were filtered
  const result: string[] = [];
  const omittedCount = head.length - sampledHead.length;
  if (sampledHead.length > 0) {
    result.push(...sampledHead);
  }
  if (omittedCount > 0) {
    result.push(`--- (${omittedCount} lines omitted, ${tail.length} recent lines follow) ---`);
  }
  result.push(...tail);

  // Apply maxLines limit — ensure we keep the separator + tail over head
  if (result.length > maxLines) {
    // Reserve at least 20% for semantic head, rest for tail
    const headBudget = Math.min(sampledHead.length, Math.floor(maxLines * 0.2));
    const tailBudget = Math.max(0, maxLines - headBudget - 1); // -1 for separator
    const kept: string[] = [];
    if (headBudget > 0) {
      kept.push(...sampledHead.slice(-headBudget));
    }
    kept.push(`--- (${omittedCount} lines omitted, showing last ${Math.min(tailBudget, tail.length)} lines) ---`);
    kept.push(...tail.slice(-tailBudget));
    return kept;
  }

  return result;
}

async function execGetTerminalBuffer(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const sessionId = args.session_id as string;
  if (!sessionId) {
    return { toolCallId, toolName: 'get_terminal_buffer', success: false, output: '', error: 'Missing required argument: session_id. Use list_sessions to find session IDs.', durationMs: Date.now() - startTime };
  }
  const maxLines = clamp(Number(args.max_lines) || 100, 1, 500);

  // Path 1: Backend buffer (SSH terminals)
  try {
    const response = await api.getAllBufferLines(sessionId);
    const allLines = response.lines.map(l => l.text);
    const sampled = semanticSample(allLines, maxLines);
    const { text, truncated } = truncateOutput(sampled.join('\n'));
    return { toolCallId, toolName: 'get_terminal_buffer', success: true, output: text || '(empty buffer)', truncated, durationMs: Date.now() - startTime };
  } catch {
    // May be a local terminal or invalid session — try frontend registry
  }

  // Path 2: Frontend terminalRegistry fallback (local terminals + rendered terminals)
  const paneId = findPaneBySessionId(sessionId);
  if (paneId) {
    const buffer = getTerminalBuffer(paneId);
    if (buffer) {
      const allLines = buffer.split('\n');
      const sampled = semanticSample(allLines, maxLines);
      const { text, truncated } = truncateOutput(sampled.join('\n'));
      return { toolCallId, toolName: 'get_terminal_buffer', success: true, output: text || '(empty buffer)', truncated, durationMs: Date.now() - startTime };
    }
  }

  return { toolCallId, toolName: 'get_terminal_buffer', success: false, output: '', error: 'Session not found or buffer unavailable. Use list_sessions to see available sessions.', durationMs: Date.now() - startTime };
}

async function execSearchTerminal(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const sessionId = args.session_id as string;
  const query = args.query as string;
  if (!sessionId || !query) {
    return { toolCallId, toolName: 'search_terminal', success: false, output: '', error: 'Missing required arguments: session_id, query', durationMs: Date.now() - startTime };
  }

  const maxResults = clamp(Number(args.max_results) || 50, 1, 100);
  const result = await api.searchTerminal(sessionId, {
    query,
    case_sensitive: (args.case_sensitive as boolean) ?? false,
    regex: (args.regex as boolean) ?? false,
    whole_word: false,
    max_matches: maxResults,
  });

  if (result.error) {
    return { toolCallId, toolName: 'search_terminal', success: false, output: '', error: result.error, durationMs: Date.now() - startTime };
  }

  if (result.matches.length === 0) {
    return { toolCallId, toolName: 'search_terminal', success: true, output: 'No matches found.', durationMs: Date.now() - startTime };
  }

  const lines = result.matches.map(m => `L${m.line_number}:${m.column_start}: ${m.line_content}`);
  const footer = `\n${result.total_matches} match(es) in ${result.duration_ms}ms` + (result.truncated ? ' (results truncated)' : '');
  const { text, truncated } = truncateOutput(lines.join('\n') + footer);
  return { toolCallId, toolName: 'search_terminal', success: true, output: text, truncated, durationMs: Date.now() - startTime };
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared Terminal Output Waiting Logic
// ═══════════════════════════════════════════════════════════════════════════

type WaitResult = {
  success: boolean;
  output: string;
  error?: string;
  truncated?: boolean;
};

/**
 * Core logic: wait for new terminal output after a command is sent.
 * Subscribes to terminal output notifications and waits for stability, pattern match, or timeout.
 * Shared by `terminal_exec` (auto-await) and `await_terminal_output`.
 */
async function waitForTerminalOutput(
  sessionId: string,
  timeoutSecs: number,
  stableSecs: number,
  patternRe: RegExp | null,
  startTime: number,
): Promise<WaitResult> {
  // Snapshot current buffer line count
  const initialLines = await readBufferLines(sessionId);
  if (initialLines === null) {
    return { success: false, output: '', error: 'Session not found or buffer unavailable.' };
  }
  const initialLineCount = initialLines.length;

  const timeoutMs = timeoutSecs * 1000;
  const stableMs = stableSecs * 1000;

  // Event-driven wait: subscribe to terminal output notifications
  const result = await new Promise<'pattern' | 'stable' | 'timeout' | 'lost'>((resolve) => {
    let stableTimer: ReturnType<typeof setTimeout> | null = null;
    let settled = false;

    const done = (reason: 'pattern' | 'stable' | 'timeout' | 'lost') => {
      if (settled) return;
      settled = true;
      clearTimeout(timeoutTimer);
      if (stableTimer) clearTimeout(stableTimer);
      unsubscribe();
      resolve(reason);
    };

    // Timeout guard
    const timeoutTimer = setTimeout(() => done('timeout'), Math.max(0, timeoutMs - (Date.now() - startTime)));

    // On each terminal output event, check conditions
    const onOutput = async () => {
      if (settled) return;

      let currentLines: string[] | null;
      try {
        currentLines = await readBufferLines(sessionId);
      } catch {
        if (!settled) done('lost');
        return;
      }
      // Re-check after async gap — timeout or another callback may have settled
      if (settled) return;

      if (currentLines === null) {
        done('lost');
        return;
      }

      // Check pattern match on new lines
      if (patternRe && currentLines.length > initialLineCount) {
        const newLines = currentLines.slice(initialLineCount);
        if (newLines.some(line => patternRe!.test(line))) {
          done('pattern');
          return;
        }
      }

      // Reset stability timer on each new output
      if (currentLines.length > initialLineCount) {
        if (stableTimer) clearTimeout(stableTimer);
        stableTimer = setTimeout(() => done('stable'), stableMs);
      }
    };

    const unsubscribe = subscribeTerminalOutput(sessionId, onOutput);
  });

  // Read final buffer and extract delta
  const finalLines = await readBufferLines(sessionId);
  if (finalLines === null || result === 'lost') {
    return { success: false, output: '', error: 'Session became unavailable during wait.' };
  }

  // Handle buffer shrink (e.g. terminal clear/reset)
  if (finalLines.length < initialLineCount) {
    const { text, truncated } = truncateOutput(finalLines.join('\n'));
    return { success: true, output: `[Buffer was cleared during wait]\n${text}`, truncated };
  }

  const newLines = finalLines.slice(initialLineCount);
  if (newLines.length === 0) {
    const msg = result === 'timeout'
      ? `No new output after ${timeoutSecs}s. The command may be waiting for input or still running.`
      : 'No new output detected.';
    return { success: true, output: msg };
  }

  const { text, truncated } = truncateOutput(newLines.join('\n'));
  return { success: true, output: text, truncated };
}

/**
 * Wait for new output in a terminal session using event-driven notifications.
 * Returns the delta (new lines) once output stabilizes, a pattern matches, or timeout is reached.
 * Works for both SSH (remote) and local terminals via the unified notifyTerminalOutput hook.
 */
async function execAwaitTerminalOutput(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'await_terminal_output';
  const sessionId = args.session_id as string;
  if (!sessionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: session_id. Use list_sessions to find session IDs.', durationMs: Date.now() - startTime };
  }

  const timeoutSecs = clamp(Number(args.timeout_secs) || 15, 1, 120);
  const stableSecs = clamp(Number(args.stable_secs) || 2, 0.5, 10);
  const patternStr = typeof args.pattern === 'string' ? args.pattern.trim() : '';

  let patternRe: RegExp | null = null;
  if (patternStr) {
    if (patternStr.length > MAX_PATTERN_LENGTH) {
      return { toolCallId, toolName, success: false, output: '', error: `Pattern too long (max ${MAX_PATTERN_LENGTH} characters)`, durationMs: Date.now() - startTime };
    }
    if (hasPotentiallyCatastrophicRegex(patternStr)) {
      return { toolCallId, toolName, success: false, output: '', error: 'Pattern rejected: potentially catastrophic regular expression', durationMs: Date.now() - startTime };
    }
    try {
      patternRe = new RegExp(patternStr, 'i');
    } catch {
      return { toolCallId, toolName, success: false, output: '', error: `Invalid regex pattern: ${patternStr}`, durationMs: Date.now() - startTime };
    }
  }

  const waitResult = await waitForTerminalOutput(sessionId, timeoutSecs, stableSecs, patternRe, startTime);
  return {
    toolCallId,
    toolName,
    success: waitResult.success,
    output: waitResult.output,
    error: waitResult.error,
    truncated: waitResult.truncated,
    durationMs: Date.now() - startTime,
  };
}

/** Read all buffer lines for a session (backend or frontend fallback). */
async function readBufferLines(sessionId: string): Promise<string[] | null> {
  // Path 1: Backend buffer (SSH terminals)
  try {
    const response = await api.getAllBufferLines(sessionId);
    return response.lines.map(l => l.text);
  } catch {
    // fallback
  }
  // Path 2: Frontend registry (local terminals)
  const paneId = findPaneBySessionId(sessionId);
  if (paneId) {
    const buffer = getTerminalBuffer(paneId);
    if (buffer) return buffer.split('\n');
  }
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Meta Tool Executors (error recovery, batch operations)
// ═══════════════════════════════════════════════════════════════════════════

/** Map control sequence names to actual bytes */
const CONTROL_SEQUENCES: Record<string, string> = {
  'ctrl-c': '\x03',
  'ctrl-d': '\x04',
  'ctrl-z': '\x1a',
  'ctrl-l': '\x0c',
  'ctrl-\\': '\x1c',
};

const CONTROL_LABELS: Record<string, string> = {
  'ctrl-c': 'SIGINT (cancel)',
  'ctrl-d': 'EOF',
  'ctrl-z': 'SIGTSTP (suspend)',
  'ctrl-l': 'Clear screen',
  'ctrl-\\': 'SIGQUIT',
};

async function execSendControlSequence(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'send_control_sequence';
  const sessionId = args.session_id as string;
  const rawSequence = typeof args.sequence === 'string' ? args.sequence.toLowerCase() : '';

  if (!sessionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: session_id.', durationMs: Date.now() - startTime };
  }
  if (!rawSequence || !CONTROL_SEQUENCES[rawSequence]) {
    return { toolCallId, toolName, success: false, output: '', error: `Invalid sequence. Must be one of: ${Object.keys(CONTROL_SEQUENCES).join(', ')}`, durationMs: Date.now() - startTime };
  }

  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return { toolCallId, toolName, success: false, output: '', error: `Open terminal session not found: ${sessionId}`, durationMs: Date.now() - startTime };
  }

  const sent = writeToTerminal(paneId, CONTROL_SEQUENCES[rawSequence]);
  if (!sent) {
    return { toolCallId, toolName, success: false, output: '', error: `Terminal session is not writable: ${sessionId}`, durationMs: Date.now() - startTime };
  }

  // Wait briefly for terminal response
  const waitResult = await waitForTerminalOutput(sessionId, 3, 1, null, startTime);

  const label = CONTROL_LABELS[rawSequence] || rawSequence;
  const output = waitResult.output
    ? `Sent ${label} to session ${sessionId}.\n\nTerminal response:\n${waitResult.output}`
    : `Sent ${label} to session ${sessionId}. No immediate terminal response.`;

  return {
    toolCallId,
    toolName,
    success: true,
    output,
    truncated: waitResult.truncated,
    durationMs: Date.now() - startTime,
  };
}

const MAX_BATCH_COMMANDS = 10;

async function execBatchExec(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'batch_exec';
  const sessionId = args.session_id as string;
  const commands = args.commands as string[] | undefined;

  if (!sessionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: session_id.', durationMs: Date.now() - startTime };
  }
  if (!Array.isArray(commands) || commands.length === 0) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: commands (non-empty array).', durationMs: Date.now() - startTime };
  }
  if (commands.length > MAX_BATCH_COMMANDS) {
    return { toolCallId, toolName, success: false, output: '', error: `Too many commands (max ${MAX_BATCH_COMMANDS}).`, durationMs: Date.now() - startTime };
  }

  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return { toolCallId, toolName, success: false, output: '', error: `Open terminal session not found: ${sessionId}`, durationMs: Date.now() - startTime };
  }

  const results: string[] = [];

  for (let i = 0; i < commands.length; i++) {
    const cmd = typeof commands[i] === 'string' ? commands[i].trim() : '';
    if (!cmd) {
      results.push(`[${i + 1}] (empty command — skipped)`);
      continue;
    }

    if (isCommandDenied(cmd)) {
      results.push(`[${i + 1}] $ ${cmd}\n⛔ Command rejected: matches deny-list pattern.`);
      continue;
    }

    const sent = writeToTerminal(paneId, `${cmd}\r`);
    if (!sent) {
      results.push(`[${i + 1}] $ ${cmd}\n❌ Terminal is not writable.`);
      break;
    }

    const waitResult = await waitForTerminalOutput(
      sessionId,
      AUTO_AWAIT_TIMEOUT_SECS,
      AUTO_AWAIT_STABLE_SECS,
      null,
      Date.now(),
    );

    if (!waitResult.success) {
      results.push(`[${i + 1}] $ ${cmd}\n❌ ${waitResult.error || 'Failed to read output.'}`);
    } else {
      results.push(`[${i + 1}] $ ${cmd}\n${waitResult.output}`);
    }
  }

  const combinedOutput = results.join('\n\n');
  const { text, truncated } = truncateOutput(combinedOutput);

  return {
    toolCallId,
    toolName,
    success: true,
    output: text,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// TUI Interaction Executors (Experimental)
// ═══════════════════════════════════════════════════════════════════════════

/** Special key name → terminal escape sequence mapping */
const KEY_SEQUENCES: Record<string, string> = {
  'enter': '\r',
  'escape': '\x1b',
  'tab': '\t',
  'backspace': '\x7f',
  'delete': '\x1b[3~',
  'up': '\x1b[A',
  'down': '\x1b[B',
  'right': '\x1b[C',
  'left': '\x1b[D',
  'home': '\x1b[H',
  'end': '\x1b[F',
  'pageup': '\x1b[5~',
  'pagedown': '\x1b[6~',
  'insert': '\x1b[2~',
  'space': ' ',
  'f1': '\x1bOP',
  'f2': '\x1bOQ',
  'f3': '\x1bOR',
  'f4': '\x1bOS',
  'f5': '\x1b[15~',
  'f6': '\x1b[17~',
  'f7': '\x1b[18~',
  'f8': '\x1b[19~',
  'f9': '\x1b[20~',
  'f10': '\x1b[21~',
  'f11': '\x1b[23~',
  'f12': '\x1b[24~',
};

/** SGR mouse button codes */
const MOUSE_BUTTONS: Record<string, number> = {
  'left': 0,
  'middle': 1,
  'right': 2,
};
const MOUSE_SCROLL_UP = 64;
const MOUSE_SCROLL_DOWN = 65;

/** Max keys in single send_keys call to prevent accidental spam */
const MAX_KEYS = 50;
/** Max scroll events per send_mouse call (prevents infinite scrolling) */
const MAX_SCROLL_COUNT = 20;
/** Wait up to 1s for terminal output to stabilize after keystrokes */
const SEND_KEYS_STABLE_SECS = 1;
/** Total timeout for waiting on keystroke response (3s for slow remote) */
const SEND_KEYS_TIMEOUT_SECS = 3;

function execReadScreen(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const toolName = 'read_screen';
  const sessionId = args.session_id as string;

  if (!sessionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: session_id.', durationMs: Date.now() - startTime };
  }

  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return { toolCallId, toolName, success: false, output: '', error: `Open terminal session not found: ${sessionId}`, durationMs: Date.now() - startTime };
  }

  const snapshot = readScreen(paneId);
  if (!snapshot) {
    return { toolCallId, toolName, success: false, output: '', error: 'Screen reader not available for this terminal.', durationMs: Date.now() - startTime };
  }

  // Format output with metadata header + numbered lines
  const bufferMode = snapshot.isAlternateBuffer ? 'alternate buffer (TUI mode)' : 'normal buffer';
  const header = `[Screen ${snapshot.cols}×${snapshot.rows} | Cursor: (${snapshot.cursorX},${snapshot.cursorY}) | ${bufferMode}]`;
  const separator = '─'.repeat(Math.min(snapshot.cols, 80));
  const lineWidth = String(snapshot.rows).length;
  const numberedLines = snapshot.lines.map((line: string, i: number) => {
    const num = String(i + 1).padStart(lineWidth);
    return `${num}│${line}`;
  });

  const output = `${header}\n${separator}\n${numberedLines.join('\n')}`;
  const { text, truncated } = truncateOutput(output);

  return {
    toolCallId,
    toolName,
    success: true,
    output: text,
    truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execSendKeys(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'send_keys';
  const sessionId = args.session_id as string;
  const keys = args.keys as string[] | undefined;
  if (args.delay_ms !== undefined && typeof args.delay_ms !== 'number') {
    return { toolCallId, toolName, success: false, output: '', error: 'delay_ms must be a number (10-1000 milliseconds).', durationMs: Date.now() - startTime };
  }
  const delayMs = Math.max(10, Math.min(1000, typeof args.delay_ms === 'number' ? args.delay_ms : 50));

  if (!sessionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: session_id.', durationMs: Date.now() - startTime };
  }
  if (!Array.isArray(keys) || keys.length === 0) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: keys (non-empty array).', durationMs: Date.now() - startTime };
  }
  if (keys.length > MAX_KEYS) {
    return { toolCallId, toolName, success: false, output: '', error: `Too many keys (max ${MAX_KEYS}).`, durationMs: Date.now() - startTime };
  }

  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return { toolCallId, toolName, success: false, output: '', error: `Open terminal session not found: ${sessionId}`, durationMs: Date.now() - startTime };
  }

  // Validate all keys are non-empty strings before sending
  for (let i = 0; i < keys.length; i++) {
    if (typeof keys[i] !== 'string' || keys[i] === '') {
      return { toolCallId, toolName, success: false, output: '', error: `keys[${i}] must be a non-empty string.`, durationMs: Date.now() - startTime };
    }
  }

  const sentSummary: string[] = [];

  /** Printable-only regex: ASCII 0x20-0x7E plus extended Unicode (no control chars) */
  const PRINTABLE_RE = /^[\x20-\x7E\u0080-\uFFFF]+$/;

  for (let i = 0; i < keys.length; i++) {
    const raw = keys[i];
    const lower = raw.toLowerCase();
    const sequence = KEY_SEQUENCES[lower];

    if (sequence !== undefined) {
      // Special key — send its escape sequence
      const sent = writeToTerminal(paneId, sequence);
      if (!sent) {
        return { toolCallId, toolName, success: false, output: sentSummary.join(', '), error: `Terminal not writable at key ${i + 1}.`, durationMs: Date.now() - startTime };
      }
      sentSummary.push(`[${raw}]`);
    } else {
      // Plain text — must be printable characters only (no raw escape sequences)
      if (!PRINTABLE_RE.test(raw)) {
        return { toolCallId, toolName, success: false, output: sentSummary.join(', '), error: `keys[${i}] contains control characters. Use named keys (e.g. "Escape", "Enter") instead.`, durationMs: Date.now() - startTime };
      }
      const sent = writeToTerminal(paneId, raw);
      if (!sent) {
        return { toolCallId, toolName, success: false, output: sentSummary.join(', '), error: `Terminal not writable at key ${i + 1}.`, durationMs: Date.now() - startTime };
      }
      sentSummary.push(raw.length <= 10 ? `"${raw}"` : `"${raw.slice(0, 10)}…"`);
    }

    // Delay between keys (skip after last key)
    if (i < keys.length - 1) {
      await new Promise(resolve => setTimeout(resolve, delayMs));
    }
  }

  // Wait briefly for terminal to process keystrokes
  const waitResult = await waitForTerminalOutput(sessionId, SEND_KEYS_TIMEOUT_SECS, SEND_KEYS_STABLE_SECS, null, startTime);

  const summary = `Sent ${keys.length} key(s): ${sentSummary.join(', ')}`;
  const output = waitResult.output
    ? `${summary}\n\nTerminal response:\n${waitResult.output}`
    : `${summary}\n\nNo immediate terminal response.`;

  const { text, truncated } = truncateOutput(output);

  return {
    toolCallId,
    toolName,
    success: true,
    output: text,
    truncated: truncated || waitResult.truncated,
    durationMs: Date.now() - startTime,
  };
}

async function execSendMouse(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'send_mouse';
  const sessionId = args.session_id as string;
  const action = typeof args.action === 'string' ? args.action.toLowerCase() : '';
  const x = typeof args.x === 'number' ? Math.floor(args.x) : 0;
  const y = typeof args.y === 'number' ? Math.floor(args.y) : 0;
  const button = typeof args.button === 'string' ? args.button.toLowerCase() : 'left';
  const direction = typeof args.direction === 'string' ? args.direction.toLowerCase() : 'down';
  const count = Math.max(1, Math.min(MAX_SCROLL_COUNT, typeof args.count === 'number' ? Math.floor(args.count) : 1));

  if (!sessionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: session_id.', durationMs: Date.now() - startTime };
  }
  if (action !== 'click' && action !== 'scroll') {
    return { toolCallId, toolName, success: false, output: '', error: 'Invalid action. Must be "click" or "scroll".', durationMs: Date.now() - startTime };
  }
  if (x < 1 || y < 1) {
    return { toolCallId, toolName, success: false, output: '', error: 'Coordinates must be >= 1 (1-based).', durationMs: Date.now() - startTime };
  }

  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return { toolCallId, toolName, success: false, output: '', error: `Open terminal session not found: ${sessionId}`, durationMs: Date.now() - startTime };
  }

  // Validate coordinates are within terminal bounds
  const snapshot = readScreen(paneId);
  if (snapshot && (x > snapshot.cols || y > snapshot.rows)) {
    return { toolCallId, toolName, success: false, output: '', error: `Coordinates out of bounds. Terminal is ${snapshot.cols}×${snapshot.rows}, got (${x},${y}).`, durationMs: Date.now() - startTime };
  }

  let summary: string;

  if (action === 'click') {
    const btnCode = MOUSE_BUTTONS[button];
    if (btnCode === undefined) {
      return { toolCallId, toolName, success: false, output: '', error: `Invalid button: "${button}". Must be "left", "right", or "middle".`, durationMs: Date.now() - startTime };
    }

    // SGR mouse protocol: press = \x1b[<btn;x;yM, release = \x1b[<btn;x;ym
    const press = `\x1b[<${btnCode};${x};${y}M`;
    const release = `\x1b[<${btnCode};${x};${y}m`;

    const sent = writeToTerminal(paneId, press + release);
    if (!sent) {
      return { toolCallId, toolName, success: false, output: '', error: `Terminal not writable: ${sessionId}`, durationMs: Date.now() - startTime };
    }
    summary = `Clicked ${button} button at (${x},${y})`;
  } else {
    // scroll — button param is not used
    if (direction !== 'up' && direction !== 'down') {
      return { toolCallId, toolName, success: false, output: '', error: 'Invalid direction. Must be "up" or "down".', durationMs: Date.now() - startTime };
    }
    const scrollCode = direction === 'up' ? MOUSE_SCROLL_UP : MOUSE_SCROLL_DOWN;
    let scrollData = '';
    for (let i = 0; i < count; i++) {
      // SGR scroll: press event only (no release for scroll)
      scrollData += `\x1b[<${scrollCode};${x};${y}M`;
    }

    const sent = writeToTerminal(paneId, scrollData);
    if (!sent) {
      return { toolCallId, toolName, success: false, output: '', error: `Terminal not writable: ${sessionId}`, durationMs: Date.now() - startTime };
    }
    summary = `Scrolled ${direction} ${count} step(s) at (${x},${y})`;
  }

  // Brief wait for TUI to react
  const waitResult = await waitForTerminalOutput(sessionId, 2, 0.5, null, startTime);

  const output = waitResult.output
    ? `${summary}\n\nTerminal response:\n${waitResult.output}`
    : `${summary}\n\nNo immediate terminal response.`;

  const { text, truncated } = truncateOutput(output);

  return {
    toolCallId,
    toolName,
    success: true,
    output: text,
    truncated: truncated || waitResult.truncated,
    durationMs: Date.now() - startTime,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Node-ID Tool Executors (new tools that require resolved node)
// ═══════════════════════════════════════════════════════════════════════════

async function execListPortForwards(
  _args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const nodes = useSessionTreeStore.getState().nodes;
  const node = nodes.find(n => n.id === resolved.nodeId);
  if (!node) {
    return { toolCallId, toolName: 'list_port_forwards', success: false, output: '', error: 'Node no longer available', durationMs: Date.now() - startTime };
  }
  const terminalId = node.runtime.terminalIds?.[0];
  if (!terminalId) {
    return { toolCallId, toolName: 'list_port_forwards', success: false, output: '', error: 'No terminal sessions on this node', durationMs: Date.now() - startTime };
  }

  try {
    const forwards = await api.listPortForwards(terminalId);
    if (forwards.length === 0) {
      return { toolCallId, toolName: 'list_port_forwards', success: true, output: 'No port forwards configured.', durationMs: Date.now() - startTime };
    }

    const lines = forwards.map(f =>
      `- [${f.status}] id=${f.id} ${f.forward_type}: ${f.bind_address}:${f.bind_port} → ${f.target_host}:${f.target_port}${f.description ? ' (' + f.description + ')' : ''}`
    );
    return { toolCallId, toolName: 'list_port_forwards', success: true, output: lines.join('\n'), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'list_port_forwards', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to list port forwards', durationMs: Date.now() - startTime };
  }
}

async function execGetDetectedPorts(
  _args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const nodes = useSessionTreeStore.getState().nodes;
  const node = nodes.find(n => n.id === resolved.nodeId);
  if (!node) {
    return { toolCallId, toolName: 'get_detected_ports', success: false, output: '', error: 'Node no longer available', durationMs: Date.now() - startTime };
  }
  const connectionId = node.runtime.connectionId;
  if (!connectionId) {
    return { toolCallId, toolName: 'get_detected_ports', success: false, output: '', error: 'No active connection for this node', durationMs: Date.now() - startTime };
  }

  try {
    const ports = await api.getDetectedPorts(connectionId);
    if (ports.length === 0) {
      return { toolCallId, toolName: 'get_detected_ports', success: true, output: 'No listening ports detected.', durationMs: Date.now() - startTime };
    }

    const lines = ports.map(p =>
      `- port=${p.port} bind=${p.bind_addr}${p.process_name ? ' process=' + p.process_name : ''}${p.pid ? ' pid=' + p.pid : ''}`
    );
    return { toolCallId, toolName: 'get_detected_ports', success: true, output: lines.join('\n'), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'get_detected_ports', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to detect ports', durationMs: Date.now() - startTime };
  }
}

async function execCreatePortForward(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const forwardType = typeof args.forward_type === 'string' ? args.forward_type : '';
  const bindPort = Number(args.bind_port);
  const targetPort = Number(args.target_port);
  const targetHost = typeof args.target_host === 'string' ? args.target_host : 'localhost';
  const bindAddr = typeof args.bind_addr === 'string' ? args.bind_addr : '127.0.0.1';

  if (!forwardType || Number.isNaN(bindPort) || Number.isNaN(targetPort)) {
    return { toolCallId, toolName: 'create_port_forward', success: false, output: '', error: 'Missing required arguments: forward_type, bind_port, target_port', durationMs: Date.now() - startTime };
  }
  if (bindPort < 1 || bindPort > 65535 || targetPort < 1 || targetPort > 65535) {
    return { toolCallId, toolName: 'create_port_forward', success: false, output: '', error: 'Port must be between 1 and 65535', durationMs: Date.now() - startTime };
  }

  const nodes = useSessionTreeStore.getState().nodes;
  const node = nodes.find(n => n.id === resolved.nodeId);
  if (!node) {
    return { toolCallId, toolName: 'create_port_forward', success: false, output: '', error: 'Node no longer available', durationMs: Date.now() - startTime };
  }
  const terminalId = node.runtime.terminalIds?.[0];
  if (!terminalId) {
    return { toolCallId, toolName: 'create_port_forward', success: false, output: '', error: 'No terminal sessions on this node', durationMs: Date.now() - startTime };
  }

  try {
    const response = await api.createPortForward({
      session_id: terminalId,
      forward_type: forwardType as 'local' | 'remote' | 'dynamic',
      bind_address: bindAddr,
      bind_port: bindPort,
      target_host: targetHost,
      target_port: targetPort,
    });

    if (!response.success) {
      return { toolCallId, toolName: 'create_port_forward', success: false, output: '', error: response.error || 'Failed to create port forward', durationMs: Date.now() - startTime };
    }

    return {
      toolCallId, toolName: 'create_port_forward', success: true,
      output: `Port forward created: ${forwardType} ${bindAddr}:${bindPort} → ${targetHost}:${targetPort} (id=${response.forward?.id || 'unknown'})`,
      durationMs: Date.now() - startTime,
    };
  } catch (e) {
    return { toolCallId, toolName: 'create_port_forward', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to create port forward', durationMs: Date.now() - startTime };
  }
}

async function execStopPortForward(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const forwardId = typeof args.forward_id === 'string' ? args.forward_id : '';
  if (!forwardId) {
    return { toolCallId, toolName: 'stop_port_forward', success: false, output: '', error: 'Missing required argument: forward_id. Use list_port_forwards to find IDs.', durationMs: Date.now() - startTime };
  }

  const nodes = useSessionTreeStore.getState().nodes;
  const node = nodes.find(n => n.id === resolved.nodeId);
  if (!node) {
    return { toolCallId, toolName: 'stop_port_forward', success: false, output: '', error: 'Node no longer available', durationMs: Date.now() - startTime };
  }
  const terminalId = node.runtime.terminalIds?.[0];
  if (!terminalId) {
    return { toolCallId, toolName: 'stop_port_forward', success: false, output: '', error: 'No terminal sessions on this node', durationMs: Date.now() - startTime };
  }

  try {
    await api.stopPortForward(terminalId, forwardId);
    return { toolCallId, toolName: 'stop_port_forward', success: true, output: `Port forward ${forwardId} stopped.`, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'stop_port_forward', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to stop port forward', durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// SFTP Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

async function execSftpListDir(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = typeof args.path === 'string' ? args.path : '';
  if (!path) {
    return { toolCallId, toolName: 'sftp_list_dir', success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  }

  try {
    const entries = await nodeSftpListDir(resolved.nodeId, path);
    const lines = entries.map(e => {
      const type = e.file_type === 'Directory' ? 'd' : e.file_type === 'Symlink' ? 'l' : '-';
      const size = e.size != null ? ` ${e.size}B` : '';
      const perm = e.permissions ?? '';
      return `${type} ${perm} ${size} ${e.name}`;
    });
    const { text } = truncateOutput(lines.join('\n'));
    return { toolCallId, toolName: 'sftp_list_dir', success: true, output: text, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'sftp_list_dir', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to list directory', durationMs: Date.now() - startTime };
  }
}

async function execSftpReadFile(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = typeof args.path === 'string' ? args.path : '';
  if (!path) {
    return { toolCallId, toolName: 'sftp_read_file', success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  }

  const maxSize = typeof args.max_size === 'number' ? args.max_size : undefined;

  try {
    const preview = await nodeSftpPreview(resolved.nodeId, path, maxSize);
    if ('Text' in preview) {
      const { data, language, encoding } = preview.Text;
      const { text } = truncateOutput(data);
      return { toolCallId, toolName: 'sftp_read_file', success: true, output: `Language: ${language ?? 'unknown'}\nEncoding: ${encoding ?? 'utf-8'}\n\n${text}`, durationMs: Date.now() - startTime };
    } else if ('TooLarge' in preview) {
      return { toolCallId, toolName: 'sftp_read_file', success: false, output: '', error: `File too large to preview (${preview.TooLarge.size} bytes, max ${preview.TooLarge.max_size})`, durationMs: Date.now() - startTime };
    } else {
      const contentType = Object.keys(preview)[0] ?? 'unknown';
      return { toolCallId, toolName: 'sftp_read_file', success: false, output: '', error: `Cannot read file as text: content type is ${contentType}`, durationMs: Date.now() - startTime };
    }
  } catch (e) {
    return { toolCallId, toolName: 'sftp_read_file', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to read file', durationMs: Date.now() - startTime };
  }
}

async function execSftpStat(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const path = typeof args.path === 'string' ? args.path : '';
  if (!path) {
    return { toolCallId, toolName: 'sftp_stat', success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  }

  try {
    const info = await nodeSftpStat(resolved.nodeId, path);
    const output = JSON.stringify({
      name: info.name,
      path: info.path,
      type: info.file_type,
      size: info.size,
      modified: info.modified,
      permissions: info.permissions,
    }, null, 2);
    return { toolCallId, toolName: 'sftp_stat', success: true, output, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'sftp_stat', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to stat file', durationMs: Date.now() - startTime };
  }
}

async function execSftpGetCwd(
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  try {
    const snapshot = await nodeGetState(resolved.nodeId);
    const cwd = snapshot.state.sftpCwd ?? '/';
    return { toolCallId, toolName: 'sftp_get_cwd', success: true, output: cwd, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'sftp_get_cwd', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to get SFTP cwd', durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// IDE Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

function execIdeGetOpenFiles(
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const { tabs, activeTabId } = useIdeStore.getState();
  const output = JSON.stringify(tabs.map(t => ({
    tab_id: t.id,
    path: t.path,
    name: t.name,
    language: t.language,
    is_dirty: t.isDirty,
    is_pinned: t.isPinned,
    is_active: t.id === activeTabId,
  })), null, 2);
  return { toolCallId, toolName: 'ide_get_open_files', success: true, output, durationMs: Date.now() - startTime };
}

function execIdeGetFileContent(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const tabId = typeof args.tab_id === 'string' ? args.tab_id : '';
  if (!tabId) {
    return { toolCallId, toolName: 'ide_get_file_content', success: false, output: '', error: 'Missing required argument: tab_id. Use ide_get_open_files to find tab IDs.', durationMs: Date.now() - startTime };
  }

  const { tabs } = useIdeStore.getState();
  const tab = tabs.find(t => t.id === tabId);
  if (!tab) {
    return { toolCallId, toolName: 'ide_get_file_content', success: false, output: '', error: `Tab not found: ${tabId}. Use ide_get_open_files to list available tabs.`, durationMs: Date.now() - startTime };
  }

  if (tab.content === null) {
    return { toolCallId, toolName: 'ide_get_file_content', success: false, output: '', error: `File content not yet loaded for tab: ${tabId}`, durationMs: Date.now() - startTime };
  }

  const { text } = truncateOutput(tab.content);
  const output = JSON.stringify({
    path: tab.path,
    language: tab.language,
    is_dirty: tab.isDirty,
    cursor: tab.cursor ?? null,
    content: text,
  }, null, 2);
  return { toolCallId, toolName: 'ide_get_file_content', success: true, output, durationMs: Date.now() - startTime };
}

function execIdeGetProjectInfo(
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const { project, nodeId } = useIdeStore.getState();
  if (!project) {
    return { toolCallId, toolName: 'ide_get_project_info', success: false, output: '', error: 'No project is currently open in IDE mode.', durationMs: Date.now() - startTime };
  }
  const output = JSON.stringify({
    root_path: project.rootPath,
    name: project.name,
    is_git_repo: project.isGitRepo,
    git_branch: project.gitBranch ?? null,
    node_id: nodeId,
  }, null, 2);
  return { toolCallId, toolName: 'ide_get_project_info', success: true, output, durationMs: Date.now() - startTime };
}

async function execIdeApplyEdit(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const tabId = typeof args.tab_id === 'string' ? args.tab_id : '';
  const content = typeof args.content === 'string' ? args.content : undefined;
  const shouldSave = args.save === true;

  if (!tabId) {
    return { toolCallId, toolName: 'ide_apply_edit', success: false, output: '', error: 'Missing required argument: tab_id', durationMs: Date.now() - startTime };
  }
  if (content === undefined) {
    return { toolCallId, toolName: 'ide_apply_edit', success: false, output: '', error: 'Missing required argument: content', durationMs: Date.now() - startTime };
  }

  const ideStore = useIdeStore.getState();
  const tab = ideStore.tabs.find(t => t.id === tabId);
  if (!tab) {
    return { toolCallId, toolName: 'ide_apply_edit', success: false, output: '', error: `Tab not found: ${tabId}`, durationMs: Date.now() - startTime };
  }

  try {
    ideStore.updateTabContent(tabId, content);
    if (shouldSave) {
      await ideStore.saveFile(tabId);
    }
    return { toolCallId, toolName: 'ide_apply_edit', success: true, output: `File ${tab.name} updated${shouldSave ? ' and saved' : ' (unsaved)'}. ${content.split('\n').length} lines.`, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'ide_apply_edit', success: false, output: '', error: e instanceof Error ? e.message : 'Failed to apply edit', durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Local Terminal Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

async function execLocalListShells(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const shells = await api.localListShells();
    const output = shells.map((s: { id: string; label: string; path: string; isDefault?: boolean }) =>
      `${s.label} (${s.path})${s.isDefault ? ' [default]' : ''}`
    ).join('\n');
    return { toolCallId, toolName: 'local_list_shells', success: true, output: output || 'No shells found', durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'local_list_shells', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execLocalGetTerminalInfo(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const [terminals, backgrounds] = await Promise.all([
      api.localListTerminals(),
      api.localListBackground(),
    ]);
    const lines: string[] = [];
    if (terminals.length > 0) {
      lines.push('Active terminals:');
      terminals.forEach((t) => {
        lines.push(`  ${t.id} — ${t.shell?.path || 'unknown'} (${t.cols}×${t.rows})`);
      });
    }
    if (backgrounds.length > 0) {
      lines.push('Background sessions:');
      backgrounds.forEach((b) => {
        lines.push(`  ${b.id} — ${b.shell?.path || 'unknown'}`);
      });
    }
    return { toolCallId, toolName: 'local_get_terminal_info', success: true, output: lines.length > 0 ? lines.join('\n') : 'No local terminals', durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'local_get_terminal_info', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execLocalExec(args: Record<string, unknown>, startTime: number, toolCallId: string): Promise<AiToolResult> {
  const command = args.command as string | undefined;
  if (!command) {
    return { toolCallId, toolName: 'local_exec', success: false, output: '', error: 'Missing required argument: command', durationMs: Date.now() - startTime };
  }

  try {
    const result = await api.localExecCommand(
      command,
      args.cwd as string | undefined,
      args.timeout_secs as number | undefined,
    );

    if (result.timedOut) {
      return { toolCallId, toolName: 'local_exec', success: false, output: result.stderr, error: 'Command timed out', durationMs: Date.now() - startTime };
    }

    const parts: string[] = [];
    if (result.stdout) parts.push(result.stdout);
    if (result.stderr) parts.push(`[stderr]\n${result.stderr}`);
    parts.push(`[exit_code: ${result.exitCode ?? 'unknown'}]`);

    return { toolCallId, toolName: 'local_exec', success: (result.exitCode === 0), output: parts.join('\n'), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'local_exec', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execLocalGetDrives(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const drives = await api.localGetDrives();
    const output = drives.map((d) => {
      const total = d.totalSpace ? `${(d.totalSpace / (1024 ** 3)).toFixed(1)}GB` : '?';
      const avail = d.availableSpace ? `${(d.availableSpace / (1024 ** 3)).toFixed(1)}GB free` : '';
      return `${d.path} — ${d.name} (${d.driveType}) ${total} ${avail}${d.isReadOnly ? ' [read-only]' : ''}`.trim();
    }).join('\n');
    return { toolCallId, toolName: 'local_get_drives', success: true, output: output || 'No drives found', durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'local_get_drives', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Settings Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

function execGetSettings(args: Record<string, unknown>, startTime: number, toolCallId: string): AiToolResult {
  const section = args.section as string | undefined;
  const settings = useSettingsStore.getState().settings;

  // Sanitize: strip sensitive fields from AI provider config before returning
  const sanitize = (obj: unknown): unknown => {
    if (typeof obj !== 'object' || obj === null) return obj;
    const raw = obj as Record<string, unknown>;
    // Filter AI providers to only expose safe fields
    if ('providers' in raw && Array.isArray(raw.providers)) {
      return {
        ...raw,
        providers: (raw.providers as Array<Record<string, unknown>>).map(p => ({
          id: p.id,
          name: p.name,
          type: p.type,
          enabled: p.enabled,
          // baseUrl, apiKey, and other sensitive fields intentionally excluded
        })),
      };
    }
    return raw;
  };

  if (section) {
    const sectionData = (settings as unknown as Record<string, unknown>)[section];
    if (sectionData === undefined) {
      return { toolCallId, toolName: 'get_settings', success: false, output: '', error: `Unknown settings section: ${section}`, durationMs: Date.now() - startTime };
    }
    const safe = section === 'ai' ? sanitize(sectionData) : sectionData;
    return { toolCallId, toolName: 'get_settings', success: true, output: JSON.stringify(safe, null, 2), durationMs: Date.now() - startTime };
  }

  // Sanitize the full settings object: filter the ai section
  const safeSettings = { ...settings as unknown as Record<string, unknown> };
  if (safeSettings.ai) {
    safeSettings.ai = sanitize(safeSettings.ai);
  }
  return { toolCallId, toolName: 'get_settings', success: true, output: JSON.stringify(safeSettings, null, 2), durationMs: Date.now() - startTime };
}

function execUpdateSetting(args: Record<string, unknown>, startTime: number, toolCallId: string): AiToolResult {
  const section = args.section as string | undefined;
  const key = args.key as string | undefined;
  const value = args.value;

  if (!section || !key || value === undefined) {
    return { toolCallId, toolName: 'update_setting', success: false, output: '', error: 'Missing required arguments: section, key, value', durationMs: Date.now() - startTime };
  }

  // Security: only allow modifying safe setting sections
  const ALLOWED_SECTIONS = new Set(['terminal', 'appearance', 'connectionDefaults', 'sftp', 'ide', 'reconnect', 'general']);
  if (!ALLOWED_SECTIONS.has(section)) {
    return { toolCallId, toolName: 'update_setting', success: false, output: '', error: `Cannot modify '${section}' settings — only ${[...ALLOWED_SECTIONS].join(', ')} are allowed`, durationMs: Date.now() - startTime };
  }

  try {
    const store = useSettingsStore.getState();
    const updateMethod = `update${section.charAt(0).toUpperCase()}${section.slice(1)}` as keyof typeof store;
    if (typeof store[updateMethod] !== 'function') {
      return { toolCallId, toolName: 'update_setting', success: false, output: '', error: `No update method for section: ${section}`, durationMs: Date.now() - startTime };
    }
    (store[updateMethod] as (patch: Record<string, unknown>) => void)({ [key]: value });
    return { toolCallId, toolName: 'update_setting', success: true, output: `Updated ${section}.${key}`, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'update_setting', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Connection Pool Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

async function execGetPoolStats(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const stats = await api.sshGetPoolStats();
    return { toolCallId, toolName: 'get_pool_stats', success: true, output: JSON.stringify(stats, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'get_pool_stats', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execSetPoolConfig(args: Record<string, unknown>, startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    // Build a full config object, using defaults for missing fields
    const idleTimeout = typeof args.idle_timeout_secs === 'number' ? args.idle_timeout_secs : 300;
    const maxConns = typeof args.max_connections === 'number' ? Math.max(1, Math.min(100, args.max_connections as number)) : 10;

    const config: import('../../../types').ConnectionPoolConfig = {
      idleTimeoutSecs: idleTimeout,
      maxConnections: maxConns,
      protectOnExit: true,
    };

    await api.sshSetPoolConfig(config);
    const changed = Object.entries(args).filter(([k]) => ['idle_timeout_secs', 'max_connections', 'keepalive_interval_secs'].includes(k)).map(([k]) => k);
    return { toolCallId, toolName: 'set_pool_config', success: true, output: `Pool config updated: ${changed.join(', ') || 'no changes'}`, durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'set_pool_config', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Connection Monitor Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

async function execGetAllHealth(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const health = await api.getAllHealthStatus();
    return { toolCallId, toolName: 'get_all_health', success: true, output: JSON.stringify(health, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'get_all_health', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execGetResourceMetrics(args: Record<string, unknown>, startTime: number, toolCallId: string): Promise<AiToolResult> {
  const connectionId = args.connection_id as string | undefined;
  if (!connectionId) {
    return { toolCallId, toolName: 'get_resource_metrics', success: false, output: '', error: 'Missing required argument: connection_id', durationMs: Date.now() - startTime };
  }

  try {
    const metrics = await api.getResourceMetrics(connectionId);
    return { toolCallId, toolName: 'get_resource_metrics', success: true, output: JSON.stringify(metrics, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'get_resource_metrics', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Session Manager Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

async function execListSavedConnections(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const connections = await api.getConnections();
    // Filter out sensitive fields (passwords, key paths)
    const safe = connections.map((c) => ({
      id: c.id,
      host: c.host,
      port: c.port,
      username: c.username,
      name: c.name,
      created_at: c.created_at,
      group: c.group,
    }));
    return { toolCallId, toolName: 'list_saved_connections', success: true, output: JSON.stringify(safe, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'list_saved_connections', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execSearchSavedConnections(args: Record<string, unknown>, startTime: number, toolCallId: string): Promise<AiToolResult> {
  const query = args.query as string | undefined;
  if (!query) {
    return { toolCallId, toolName: 'search_saved_connections', success: false, output: '', error: 'Missing required argument: query', durationMs: Date.now() - startTime };
  }

  try {
    const connections = await api.searchConnections(query);
    const safe = connections.map((c) => ({
      id: c.id,
      host: c.host,
      port: c.port,
      username: c.username,
      name: c.name,
      group: c.group,
    }));
    return { toolCallId, toolName: 'search_saved_connections', success: true, output: JSON.stringify(safe, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'search_saved_connections', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execGetSessionTree(startTime: number, toolCallId: string): Promise<AiToolResult> {
  try {
    const tree = await api.getSessionTree();
    return { toolCallId, toolName: 'get_session_tree', success: true, output: JSON.stringify(tree, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName: 'get_session_tree', success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Manager Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

function execListPlugins(startTime: number, toolCallId: string): AiToolResult {
  const plugins = usePluginStore.getState().plugins;
  const summary: { id: string; name: string; version: string; state: string; hasError: boolean }[] = [];
  plugins.forEach((p, id) => {
    summary.push({
      id,
      name: p.manifest?.name ?? id,
      version: p.manifest?.version ?? 'unknown',
      state: p.state,
      hasError: !!p.error,
    });
  });
  return { toolCallId, toolName: 'list_plugins', success: true, output: JSON.stringify(summary, null, 2), durationMs: Date.now() - startTime };
}

// ═══════════════════════════════════════════════════════════════════════════
// Utility
// ═══════════════════════════════════════════════════════════════════════════

function shellEscape(s: string): string {
  return "'" + s.replace(/'/g, "'\\''") + "'";
}

function formatTreeEntries(entries: AgentFileEntry[], indent: string): string {
  return entries
    .map((e) => {
      const prefix = e.file_type === 'directory' ? `${indent}${e.name}/` : `${indent}${e.name}`;
      const children = e.children && Array.isArray(e.children) && e.children.length > 0
        ? '\n' + formatTreeEntries(e.children as typeof entries, indent + '  ')
        : '';
      return prefix + children;
    })
    .join('\n');
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP Tool Execution
// ═══════════════════════════════════════════════════════════════════════════

async function executeMcpTool(
  toolName: string,
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const { useMcpRegistry } = await import('../mcp');
  const registry = useMcpRegistry.getState();
  const match = registry.findServerForTool(toolName);

  if (!match) {
    return { toolCallId, toolName, success: false, output: '', error: `No MCP server found for tool: ${toolName}`, durationMs: Date.now() - startTime };
  }

  const { server, originalName } = match;

  const result = await registry.callTool(server.config.id, originalName, args);

  // Extract text content from MCP result
  const textParts = result.content
    .filter(c => c.type === 'text' && c.text)
    .map(c => c.text!);
  const output = textParts.join('\n').slice(0, MAX_OUTPUT_BYTES);

  return {
    toolCallId,
    toolName,
    success: !result.isError,
    output,
    error: result.isError ? output : undefined,
    durationMs: Date.now() - startTime,
    truncated: textParts.join('\n').length > MAX_OUTPUT_BYTES,
  };
}
