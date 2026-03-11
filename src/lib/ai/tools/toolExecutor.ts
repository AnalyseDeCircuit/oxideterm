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
import { api } from '../../api';
import type { AiToolResult, AgentFileEntry } from '../../../types';
import { isCommandDenied, CONTEXT_FREE_TOOLS, SESSION_ID_TOOLS } from './toolDefinitions';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import { useAppStore } from '../../../store/appStore';
import { useLocalTerminalStore } from '../../../store/localTerminalStore';
import { findPaneBySessionId, getTerminalBuffer } from '../../terminalRegistry';

/** Max output size returned from a tool execution (bytes) */
const MAX_OUTPUT_BYTES = 8192;
const MAX_COMMAND_TIMEOUT_SECS = 60;
const MAX_LIST_DEPTH = 8;
const MAX_GREP_RESULTS = 200;
const MAX_PATTERN_LENGTH = 200;

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

  try {
    // Context-free tools — no node required
    if (CONTEXT_FREE_TOOLS.has(toolName)) {
      switch (toolName) {
        case 'list_sessions':
          return await execListSessions(args, startTime, toolCallId);
        case 'list_connections':
          return await execListConnections(startTime, toolCallId);
        case 'get_connection_health':
          return await execGetConnectionHealth(args, startTime, toolCallId);
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
        default:
          return { toolCallId, toolName, success: false, output: '', error: `Unknown session tool: ${toolName}`, durationMs: Date.now() - startTime };
      }
    }

    // Node-ID tools — resolve target node
    const resolved = await resolveNodeForTool(args.node_id as string | undefined, context);
    if (!resolved) {
      return {
        toolCallId,
        toolName,
        success: false,
        output: '',
        error: 'No target node available. Use list_sessions to find a target, then pass node_id.',
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
      default:
        return { toolCallId, toolName, success: false, output: '', error: `Unknown tool: ${toolName}`, durationMs: Date.now() - startTime };
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

  // Safety: deny-list check
  if (isCommandDenied(command)) {
    return { toolCallId, toolName: 'terminal_exec', success: false, output: '', error: 'Command rejected: matches security deny-list', durationMs: Date.now() - startTime };
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
