// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * AI Tool Executor
 *
 * Dispatches tool calls to the appropriate backend APIs and returns results.
 * Uses the remote agent (JSON-RPC over SSH) when available, with fallback to
 * SFTP/exec for basic operations.
 *
 * Tools are categorized into three routing modes:
 * - CONTEXT_FREE: No nodeId needed (list_sessions, list_connections, get_connection_health)
 * - SESSION_ID: Uses session_id parameter (get_terminal_buffer, search_terminal layered history search)
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
import { nodeSftpListDir, nodeSftpPreview, nodeSftpStat, nodeSftpWrite } from '../../api';
import { api } from '../../api';
import { ragSearch } from '../../api';
import type { AiToolResult, AgentFileEntry, TabType } from '../../../types';
import { CONTEXT_FREE_TOOLS, SESSION_ID_TOOLS } from './toolDefinitions';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import { useAppStore } from '../../../store/appStore';
import { useLocalTerminalStore } from '../../../store/localTerminalStore';
import { useIdeStore } from '../../../store/ideStore';
import { useSettingsStore } from '../../../store/settingsStore';
import { requiresEmbeddingApiKey, resolveEmbeddingProvider } from '../embeddingConfig';
import { resolveChatEmbeddingApiKey } from '../providerKeyScope';
import { usePluginStore } from '../../../store/pluginStore';
import { useEventLogStore } from '../../../store/eventLogStore';
import { useTransferStore } from '../../../store/transferStore';
import { useRecordingStore } from '../../../store/recordingStore';
import { useBroadcastStore } from '../../../store/broadcastStore';
import { waitForTerminalReady } from '../../terminalRegistry';
import { compressOutput } from './outputCompressor';
import { sanitizeConnectionInfo } from '../contextSanitizer';
import { MAX_OUTPUT_BYTES } from '../agentConfig';
import {
  buildFileDiffSummary,
  createTerminalOutputSubscription,
  buildCapabilityStatuses,
  buildToolTargets,
  byteLengthOfText,
  createExecutionSummary,
  createToolResultEnvelope,
  formatScreenSnapshot,
  hashTextContent,
  parseFileWriteRequest,
  readBufferLineCount,
  readBufferTail,
  readRenderedBufferLines,
  readRenderedBufferTail,
  readRenderedBufferText,
  readTerminalScreen,
  renderedDeltaFromLineCount,
  renderedDeltaFromTextSnapshot,
  searchRenderedBuffer,
  terminalRunRemote,
  terminalSend,
  toLegacyToolResult,
  waitForTerminalOutput as waitForTerminalOutputV2,
  type FileReadData,
  type FileWriteData,
  type FileWriteRequest,
  type TerminalOutputSubscription,
  type TerminalWaitResult,
  type ToolTarget,
} from './protocol';

const MAX_COMMAND_TIMEOUT_SECS = 60;
const MAX_LIST_DEPTH = 8;
const MAX_GREP_RESULTS = 200;
const MAX_PATTERN_LENGTH = 200;
const AUTO_AWAIT_TIMEOUT_SECS = 30;
const AUTO_AWAIT_STABLE_SECS = 3;
const UNCONDITIONAL_OVERWRITE_WARNING = 'unconditional overwrite: provide expectedHash from a prior read_file/sftp_read_file/ide_get_file_content result to enable optimistic locking';

/**
 * Shell prompt patterns for detecting command completion.
 * Matches common bash/zsh/fish/sh prompts at end of line.
 * Only fires when the prompt-like text is at the very end of output (trailing whitespace allowed).
 */
const COMPLETION_PROMPT_RE = /(?:^|\n)[\w@.\-~:\/\[\]\(\) ]*[\$#>%]\s*$/;
/** Interactive prompts that wait for hidden input and often do not emit a newline. */
const INTERACTIVE_INPUT_PROMPT_RE = /(?:^|\n).*(?:\[sudo\]\s*)?(?:password|passphrase|密码|口令)(?:\s+for\s+[^\n:]+)?\s*:\s*$/i;
/** Short grace period after prompt detection to catch trailing output */
const PROMPT_GRACE_MS = 200;
/** Maximum stability window when output keeps growing */
const MAX_ADAPTIVE_STABLE_SECS = 5;
/** Number of buffer tail lines to include when output is empty */
const EMPTY_OUTPUT_TAIL_LINES = 20;
/** Default tail window used when only a lightweight snapshot is needed */
const TERMINAL_BUFFER_TAIL_FALLBACK_LINES = 500;
const TERMINAL_READY_TIMEOUT_MS = 3000;

/** Context needed to execute tools — activeNodeId may be null when no terminal is focused */
export type ToolExecutionContext = {
  /** Currently active node ID — null when no terminal is focused */
  activeNodeId: string | null;
  /** Whether the active node has remote agent available */
  activeAgentAvailable: boolean;
  /** Currently active terminal session ID for implicit session routing */
  activeSessionId?: string | null;
  /** Terminal type of the active session, if any */
  activeTerminalType?: 'terminal' | 'local_terminal' | null;
  /** If true, tabs created by tools won't steal focus (used by Agent mode) */
  skipFocus?: boolean;
  /** If true, execution/write/interaction tools must pass target_id, node_id, or session_id explicitly. */
  requireExplicitTarget?: boolean;
};

export type ToolExecutionOptions = {
  dangerousCommandApproved?: boolean;
  abortSignal?: AbortSignal;
};

/** Resolved target for a tool that requires a specific node */
type ResolvedNode = {
  nodeId: string;
  agentAvailable: boolean;
};

type ResolvedTarget = {
  target: ToolTarget;
  nodeId?: string;
  sessionId?: string;
};

function makeAbortError(): DOMException {
  return new DOMException('Generation was stopped.', 'AbortError');
}

async function waitForInteractiveTerminalReady(
  sessionId: string,
  abortSignal?: AbortSignal,
): Promise<string | null> {
  const ready = await waitForTerminalReady(sessionId, {
    timeoutMs: TERMINAL_READY_TIMEOUT_MS,
    abortSignal,
  });
  if (ready.ready) {
    return null;
  }

  const state = ready.state;
  if (!state) {
    return `Terminal session is not ready for interactive input yet: ${sessionId}. The terminal view may still be opening. Retry terminal_exec after the terminal is visible.`;
  }

  const missing: string[] = [];
  if (!state.writerReady) missing.push('writer');
  if (!state.frontendOutputListenerReady) missing.push('output listener');
  if (!state.renderBufferReady) missing.push('render buffer');
  if (!state.backendBufferReady) missing.push('backend buffer');

  return `Terminal session is not ready for interactive input yet: ${sessionId}. Waiting for ${missing.join(', ') || 'terminal readiness'} timed out. Retry shortly or inspect with get_terminal_buffer.`;
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) {
    throw makeAbortError();
  }
}

function envelopeResult<TData>(
  toolCallId: string,
  input: Parameters<typeof createToolResultEnvelope<TData>>[0],
): AiToolResult {
  return toLegacyToolResult(createToolResultEnvelope(input), toolCallId);
}

function plainStringArg(args: Record<string, unknown>, key: string): string {
  return typeof args[key] === 'string' ? args[key].trim() : '';
}

function savedConnectionIdFromTargetId(targetId: string): string | null {
  return targetId.startsWith('saved-connection:')
    ? targetId.slice('saved-connection:'.length).trim() || null
    : null;
}

function createSavedConnectionTarget(connectionId: string): ToolTarget {
  return {
    id: `saved-connection:${connectionId}`,
    kind: 'saved-connection',
    label: `Saved connection ${connectionId}`,
    capabilities: ['navigation.open', 'state.list'],
    metadata: {
      savedConnection: true,
      connectionId,
    },
  };
}

function isBroadConnectionDiscoveryQuery(query: string, intent: string): boolean {
  const text = query.trim();
  if (intent !== 'connection') return false;
  if (!text) return true;
  return [
    /\b(?:list|show|what|which|available|saved)\b.*\b(?:hosts?|servers?|connections?|sessions?)\b/i,
    /\b(?:hosts?|servers?|connections?|sessions?)\b.*\b(?:available|saved|configured)\b/i,
    /(?:有哪些|有什么|列出|查看|显示|可用|保存的|已保存).*(?:远程主机|主机|服务器|连接|SSH|ssh|会话)/i,
    /(?:远程主机|主机|服务器|保存连接|已保存连接|连接配置|SSH|ssh|会话).*(?:有哪些|有什么|列表|列出|查看|显示|可用)/i,
  ].some((pattern) => pattern.test(text));
}

function resolveTargetById(targetId: string): ResolvedTarget | null {
  if (!targetId) return null;
  const savedConnectionId = savedConnectionIdFromTargetId(targetId);
  if (savedConnectionId) {
    return {
      target: createSavedConnectionTarget(savedConnectionId),
    };
  }
  const target = collectToolTargets().find((candidate) => candidate.id === targetId);
  if (!target) return null;
  const terminalSessionIds = Array.isArray(target.metadata?.terminalSessionIds)
    ? target.metadata.terminalSessionIds.filter((id): id is string => typeof id === 'string')
    : [];
  return {
    target,
    nodeId: target.nodeId,
    sessionId: target.sessionId ?? terminalSessionIds[0],
  };
}

function targetNotFoundResult(toolCallId: string, toolName: string, targetId: string, startTime: number): AiToolResult {
  return envelopeResult(toolCallId, {
    ok: false,
    toolName,
    summary: 'Target not found.',
    output: `Target not found: ${targetId}. Resolve the target again before continuing.`,
    error: {
      code: 'target_not_found',
      message: `Target not found: ${targetId}`,
      recoverable: true,
    },
    recoverable: true,
    durationMs: Date.now() - startTime,
    nextActions: [
      { tool: 'resolve_target', reason: 'Rediscover the target and retry with the returned target_id.', priority: 'recommended' },
    ],
  });
}

function missingExplicitTargetResult(toolCallId: string, toolName: string, startTime: number): AiToolResult {
  return envelopeResult(toolCallId, {
    ok: false,
    toolName,
    summary: 'This tool requires an explicit target.',
    output: `${toolName} requires target_id, node_id, or session_id in OxideSens target-first mode. Call resolve_target first, then retry with the returned target_id.`,
    error: {
      code: 'target_required',
      message: 'Missing explicit target_id, node_id, or session_id.',
      recoverable: true,
    },
    recoverable: true,
    durationMs: Date.now() - startTime,
    nextActions: [
      { tool: 'resolve_target', reason: 'Resolve the intended target before performing this action.', priority: 'recommended' },
    ],
  });
}

function recoverableFileError(
  toolCallId: string,
  toolName: string,
  startTime: number,
  code: string,
  message: string,
  output = '',
): AiToolResult {
  return envelopeResult(toolCallId, {
    ok: false,
    toolName,
    capability: 'filesystem.write',
    summary: message,
    output,
    error: {
      code,
      message,
      recoverable: true,
    },
    durationMs: Date.now() - startTime,
  });
}

function isNotFoundError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /not\s+found|no\s+such\s+file|does\s+not\s+exist|不存在|找不到/i.test(message);
}

function validateExistingFile(
  toolCallId: string,
  toolName: string,
  request: FileWriteRequest,
  existing: { hash?: string; mtime?: number | null },
  startTime: number,
): AiToolResult | undefined {
  if (request.expectedHash && existing.hash && existing.hash !== request.expectedHash) {
    return recoverableFileError(
      toolCallId,
      toolName,
      startTime,
      'expected_hash_mismatch',
      `File changed before writing: expected hash ${request.expectedHash}, current hash ${existing.hash}.`,
    );
  }

  if (request.expectedMtime !== undefined && existing.mtime != null && existing.mtime !== request.expectedMtime) {
    return recoverableFileError(
      toolCallId,
      toolName,
      startTime,
      'expected_mtime_mismatch',
      `File changed before writing: expected mtime ${request.expectedMtime}, current mtime ${existing.mtime}.`,
    );
  }

  return undefined;
}

async function waitWithAbort(ms: number, signal?: AbortSignal): Promise<void> {
  if (!signal) {
    await new Promise((resolve) => setTimeout(resolve, ms));
    return;
  }

  if (signal.aborted) throw makeAbortError();

  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', onAbort);
      resolve();
    }, ms);

    const onAbort = () => {
      clearTimeout(timer);
      signal.removeEventListener('abort', onAbort);
      reject(makeAbortError());
    };

    signal.addEventListener('abort', onAbort, { once: true });
  });
}

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

  if (!context.requireExplicitTarget && context.activeNodeId) {
    return { nodeId: context.activeNodeId, agentAvailable: context.activeAgentAvailable };
  }

  return null;
}

function resolveActiveSessionId(
  args: Record<string, unknown>,
  context: ToolExecutionContext,
): string {
  const targetId = plainStringArg(args, 'target_id');
  if (targetId) {
    const resolvedTarget = resolveTargetById(targetId);
    return resolvedTarget?.sessionId ?? '';
  }

  const explicitSessionId = typeof args.session_id === 'string' ? args.session_id.trim() : '';
  if (explicitSessionId.length > 0) {
    return explicitSessionId;
  }

  if (context.requireExplicitTarget) {
    return '';
  }

  if (context.activeTerminalType !== 'local_terminal') {
    return '';
  }

  return typeof context.activeSessionId === 'string' ? context.activeSessionId.trim() : '';
}

/**
 * Execute a tool call and return the result.
 * Dispatches to the appropriate backend based on tool name and routing category.
 */
export async function executeTool(
  toolName: string,
  args: Record<string, unknown>,
  context: ToolExecutionContext,
  options: ToolExecutionOptions = {},
): Promise<AiToolResult> {
  const startTime = Date.now();
  const toolCallId = `exec-${Date.now()}`;
  const explicitTargetId = plainStringArg(args, 'target_id');
  const explicitTarget = explicitTargetId ? resolveTargetById(explicitTargetId) : null;
  if (explicitTargetId && !explicitTarget) {
    return targetNotFoundResult(toolCallId, toolName, explicitTargetId, startTime);
  }
  const explicitNodeId = explicitTarget?.nodeId ?? plainStringArg(args, 'node_id');

  try {
    throwIfAborted(options.abortSignal);

    // Dynamic MCP tools are global external tools, not SSH-node tools.
    if (toolName.startsWith('mcp::')) {
      return await executeMcpTool(toolName, args, startTime, toolCallId);
    }

    // Context-free tools — no node required
    if (CONTEXT_FREE_TOOLS.has(toolName)) {
      return await executeContextFreeTool(toolName, args, context, options, startTime, toolCallId);
    }

    // Session-ID tools — route by session_id parameter
    if (SESSION_ID_TOOLS.has(toolName)) {
      const sessionId = resolveActiveSessionId(args, context);
      if (!sessionId && context.requireExplicitTarget) {
        return missingExplicitTargetResult(toolCallId, toolName, startTime);
      }
      const routedArgs = sessionId ? { ...args, session_id: sessionId } : args;

      switch (toolName) {
        case 'get_terminal_buffer':
          return await execGetTerminalBuffer(routedArgs, startTime, toolCallId);
        case 'search_terminal':
          return await execSearchTerminal(routedArgs, startTime, toolCallId);
        case 'await_terminal_output':
          return await execAwaitTerminalOutput(routedArgs, startTime, toolCallId, options.abortSignal);
        case 'send_control_sequence':
          return await execSendControlSequence(routedArgs, startTime, toolCallId, options.abortSignal);
        case 'batch_exec':
          return await execBatchExec(routedArgs, startTime, toolCallId, options.abortSignal);
        case 'read_screen':
          return execReadScreen(routedArgs, startTime, toolCallId);
        case 'send_keys':
          return await execSendKeys(routedArgs, startTime, toolCallId, options.abortSignal);
        case 'send_mouse':
          return await execSendMouse(routedArgs, startTime, toolCallId, options.abortSignal);
        default:
          return { toolCallId, toolName, success: false, output: '', error: `Unknown session tool: ${toolName}`, durationMs: Date.now() - startTime };
      }
    }

    // terminal_exec with terminal-session target/session_id: route to interactive terminal path.
    // Priority: target_id > node_id (direct exec) > session_id (terminal send) > legacy active terminal fallback.
    if (toolName === 'terminal_exec' && explicitTarget?.target.kind === 'terminal-session') {
      const sessionId = explicitTarget.sessionId;
      if (sessionId) {
        return await execTerminalCommandToSession({ ...args, session_id: sessionId }, sessionId, startTime, toolCallId, options.abortSignal);
      }
      return missingExplicitTargetResult(toolCallId, toolName, startTime);
    }

    if (toolName === 'terminal_exec' && explicitTarget?.target.kind === 'saved-connection') {
      const connectionId = typeof explicitTarget.target.metadata?.connectionId === 'string'
        ? explicitTarget.target.metadata.connectionId
        : savedConnectionIdFromTargetId(explicitTarget.target.id);
      return envelopeResult(toolCallId, {
        ok: false,
        toolName,
        summary: 'Connect the saved SSH target before running commands.',
        output: `target_id=${explicitTarget.target.id} is a saved connection, not a live SSH node. Use connect_saved_session first; then run terminal_exec against the returned ssh-node target_id.`,
        error: {
          code: 'saved_connection_not_connected',
          message: 'Saved connection target must be connected before command execution.',
          recoverable: true,
        },
        recoverable: true,
        durationMs: Date.now() - startTime,
        targets: [{ id: explicitTarget.target.id, kind: 'saved-connection', label: explicitTarget.target.label, metadata: explicitTarget.target.metadata }],
        nextActions: connectionId
          ? [
              { tool: 'connect_saved_session', args: { connection_id: connectionId }, reason: 'Open the saved SSH connection and get a live ssh-node target.', priority: 'recommended' },
            ]
          : [
              { tool: 'resolve_target', reason: 'Resolve the saved connection again before connecting.', priority: 'recommended' },
            ],
      });
    }

    if (toolName === 'terminal_exec' && explicitNodeId.length === 0) {
      const sessionId = resolveActiveSessionId(args, context);
      if (sessionId) {
        return await execTerminalCommandToSession({ ...args, session_id: sessionId }, sessionId, startTime, toolCallId, options.abortSignal);
      }
      if (explicitTarget?.target.kind === 'local-shell') {
        return envelopeResult(toolCallId, {
          ok: false,
          toolName,
          summary: 'Use local_exec for the local shell target.',
          output: 'target_id=local-shell:default is a local shell target. Use local_exec for local one-shot commands, or open_local_terminal then target a terminal-session for visible shell interaction.',
          error: {
            code: 'wrong_tool_for_target',
            message: 'Use local_exec for local-shell targets.',
            recoverable: true,
          },
          recoverable: true,
          durationMs: Date.now() - startTime,
          targets: [{ id: 'local-shell:default', kind: 'local-shell', label: 'Local shell' }],
          nextActions: [
            { tool: 'local_exec', args: { command: args.command }, reason: 'Run the command on the local shell target.', priority: 'recommended' },
          ],
        });
      }
      if (context.requireExplicitTarget) {
        return missingExplicitTargetResult(toolCallId, toolName, startTime);
      }
    }

    // Node-ID tools — resolve target node
    const resolved = await resolveNodeForTool(explicitNodeId || undefined, context);
    if (!resolved) {
      if (context.requireExplicitTarget) {
        return missingExplicitTargetResult(toolCallId, toolName, startTime);
      }
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
      case 'sftp_write_file':
        return await execSftpWriteFile(args, resolved, startTime, toolCallId);
      case 'list_mcp_resources':
        return await execListMcpResources(startTime, toolCallId);
      case 'read_mcp_resource':
        return await execReadMcpResource(args, startTime, toolCallId);
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

async function executeContextFreeTool(
  toolName: string,
  args: Record<string, unknown>,
  context: ToolExecutionContext,
  options: ToolExecutionOptions,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  switch (toolName) {
    case 'resolve_target':
      return await execResolveTarget(args, startTime, toolCallId);
    case 'list_tabs':
      return execListTabs(startTime, toolCallId);
    case 'list_sessions':
      return await execListSessions(args, startTime, toolCallId);
    case 'list_targets':
      return execListTargets(args, startTime, toolCallId);
    case 'list_capabilities':
      return execListCapabilities(args, startTime, toolCallId);
    case 'list_connections':
      return await execListConnections(startTime, toolCallId);
    case 'get_connection_health':
      return await execGetConnectionHealth(args, startTime, toolCallId);
    case 'ide_get_open_files':
      return execIdeGetOpenFiles(startTime, toolCallId);
    case 'ide_get_file_content':
      return await execIdeGetFileContent(args, startTime, toolCallId);
    case 'ide_get_project_info':
      return execIdeGetProjectInfo(startTime, toolCallId);
    case 'ide_replace_string':
      return await execIdeReplaceString(args, startTime, toolCallId);
    case 'ide_insert_text':
      return await execIdeInsertText(args, startTime, toolCallId);
    case 'ide_open_file':
      return await execIdeOpenFile(args, startTime, toolCallId);
    case 'ide_create_file':
      return await execIdeCreateFile(args, startTime, toolCallId);
    case 'local_list_shells':
      return await execLocalListShells(startTime, toolCallId);
    case 'local_get_terminal_info':
      return await execLocalGetTerminalInfo(startTime, toolCallId);
    case 'local_exec':
      return await execLocalExec(args, startTime, toolCallId, options.dangerousCommandApproved === true);
    case 'local_get_drives':
      return await execLocalGetDrives(startTime, toolCallId);
    case 'open_local_terminal':
      return await execOpenLocalTerminal(args, startTime, toolCallId, context.skipFocus);
    case 'open_tab':
      return execOpenTab(args, startTime, toolCallId, context.skipFocus);
    case 'open_session_tab':
      return execOpenSessionTab(args, startTime, toolCallId, context.skipFocus);
    case 'get_settings':
      return execGetSettings(args, startTime, toolCallId);
    case 'update_setting':
      if (context.requireExplicitTarget && !plainStringArg(args, 'target_id')) {
        return missingExplicitTargetResult(toolCallId, toolName, startTime);
      }
      return execUpdateSetting(args, startTime, toolCallId);
    case 'open_settings_section':
      return execOpenSettingsSection(args, startTime, toolCallId, context.skipFocus);
    case 'get_pool_stats':
      return await execGetPoolStats(startTime, toolCallId);
    case 'set_pool_config':
      return await execSetPoolConfig(args, startTime, toolCallId);
    case 'get_all_health':
      return await execGetAllHealth(startTime, toolCallId);
    case 'get_resource_metrics':
      return await execGetResourceMetrics(args, startTime, toolCallId);
    case 'list_saved_connections':
      return await execListSavedConnections(startTime, toolCallId);
    case 'search_saved_connections':
      return await execSearchSavedConnections(args, startTime, toolCallId);
    case 'get_session_tree':
      return await execGetSessionTree(startTime, toolCallId);
    case 'connect_saved_session':
      return await execConnectSavedSession(args, startTime, toolCallId, context.skipFocus);
    case 'connect_saved_connection_by_query':
      return await execConnectSavedConnectionByQuery(args, startTime, toolCallId, context.skipFocus);
    case 'list_plugins':
      return execListPlugins(startTime, toolCallId);
    case 'get_event_log':
      return execGetEventLog(args, startTime, toolCallId);
    case 'get_transfer_status':
      return execGetTransferStatus(args, startTime, toolCallId);
    case 'get_recording_status':
      return execGetRecordingStatus(startTime, toolCallId);
    case 'get_broadcast_status':
      return execGetBroadcastStatus(startTime, toolCallId);
    case 'get_plugin_details':
      return execGetPluginDetails(args, startTime, toolCallId);
    case 'get_ssh_environment':
      return await execGetSshEnvironment(startTime, toolCallId);
    case 'get_topology':
      return await execGetTopology(startTime, toolCallId);
    case 'search_docs':
      return await execSearchDocs(args, startTime, toolCallId);
    default:
      return { toolCallId, toolName, success: false, output: '', error: `Unknown context-free tool: ${toolName}`, durationMs: Date.now() - startTime };
  }
}

function truncateOutput(output: string): { text: string; truncated: boolean } {
  const compressed = compressOutput(output);
  if (compressed.length <= MAX_OUTPUT_BYTES) return { text: compressed, truncated: false };
  return { text: compressed.slice(0, MAX_OUTPUT_BYTES) + '\n... (output truncated)', truncated: true };
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum);
}

function hasPotentiallyCatastrophicRegex(pattern: string): boolean {
  return /(\([^)]*[+*][^)]*\))[+*]|([+*])\1/.test(pattern);
}

type ParsedSshCommand = {
  host: string;
  username?: string;
  port?: number;
};

type SafeSavedConnection = {
  id: string;
  host: string;
  port: number;
  username: string;
  name?: string;
  group?: string;
};

function shellTokens(command: string): string[] {
  const tokens: string[] = [];
  let current = '';
  let quote: '"' | "'" | null = null;
  let escaped = false;

  for (const char of command.trim()) {
    if (escaped) {
      current += char;
      escaped = false;
      continue;
    }
    if (char === '\\' && quote !== "'") {
      escaped = true;
      continue;
    }
    if ((char === '"' || char === "'") && (!quote || quote === char)) {
      quote = quote ? null : char;
      continue;
    }
    if (!quote && /\s/.test(char)) {
      if (current) tokens.push(current);
      current = '';
      continue;
    }
    current += char;
  }
  if (current) tokens.push(current);
  return tokens;
}

function parseSshCommand(command: string): ParsedSshCommand | null {
  const tokens = shellTokens(command);
  if (tokens[0] === 'sudo') tokens.shift();
  const binary = tokens.shift();
  if (!binary || !/(?:^|\/)ssh$/.test(binary)) return null;

  let username: string | undefined;
  let port: number | undefined;
  const optionsWithValue = new Set(['-B', '-b', '-c', '-D', '-E', '-e', '-F', '-I', '-i', '-J', '-L', '-l', '-m', '-O', '-o', '-p', '-Q', '-R', '-S', '-W', '-w']);

  while (tokens.length > 0) {
    const token = tokens.shift()!;
    if (token === '--') break;
    if (!token.startsWith('-')) {
      const target = token;
      const at = target.lastIndexOf('@');
      const host = at >= 0 ? target.slice(at + 1) : target;
      if (at >= 0) username = target.slice(0, at) || username;
      const normalizedHost = host.replace(/^\[/, '').replace(/\]$/, '');
      return normalizedHost ? { host: normalizedHost, username, port } : null;
    }

    if (token.startsWith('-p') && token.length > 2) {
      const parsedPort = Number(token.slice(2));
      if (Number.isFinite(parsedPort)) port = parsedPort;
      continue;
    }
    if (token.startsWith('-l') && token.length > 2) {
      username = token.slice(2);
      continue;
    }
    if (optionsWithValue.has(token)) {
      const value = tokens.shift();
      if (token === '-p' && value) {
        const parsedPort = Number(value);
        if (Number.isFinite(parsedPort)) port = parsedPort;
      } else if (token === '-l' && value) {
        username = value;
      }
    }
  }

  const target = tokens.shift();
  if (!target) return null;
  const at = target.lastIndexOf('@');
  const host = at >= 0 ? target.slice(at + 1) : target;
  if (at >= 0) username = target.slice(0, at) || username;
  const normalizedHost = host.replace(/^\[/, '').replace(/\]$/, '');
  return normalizedHost ? { host: normalizedHost, username, port } : null;
}

function savedConnectionMatches(parsed: ParsedSshCommand, connection: SafeSavedConnection): boolean {
  if (connection.host !== parsed.host) return false;
  if (parsed.username && connection.username !== parsed.username) return false;
  if (parsed.port && connection.port !== parsed.port) return false;
  return true;
}

async function detectSavedConnectionSshMisuse(
  command: string,
  startTime: number,
  toolCallId: string,
  toolName: string,
): Promise<AiToolResult | null> {
  const parsed = parseSshCommand(command);
  if (!parsed) return null;

  try {
    const connections = (await api.searchConnections(parsed.host)).map((connection) => ({
      id: connection.id,
      host: connection.host,
      port: connection.port,
      username: connection.username,
      name: connection.name,
      group: connection.group ?? undefined,
    }));
    const matches = connections.filter((connection) => savedConnectionMatches(parsed, connection));
    if (matches.length === 0) return null;

    return envelopeResult(toolCallId, {
      ok: false,
      toolName,
      summary: 'A matching saved SSH connection exists. Use OxideTerm connection tools instead of manual ssh.',
      output: `Manual ssh command matches ${matches.length} saved connection${matches.length === 1 ? '' : 's'} for ${parsed.username ? `${parsed.username}@` : ''}${parsed.host}. Use connect_saved_connection_by_query or connect_saved_session so OxideTerm can handle credentials, proxy chains, host key verification, and terminal registration.`,
      data: { parsedCommand: parsed, matches },
      error: {
        code: 'manual_ssh_matches_saved_connection',
        message: 'A matching saved SSH connection exists. Use the saved connection workflow instead of manual ssh.',
        recoverable: true,
      },
      recoverable: true,
      durationMs: Date.now() - startTime,
      targets: matches.map((connection) => ({
        id: `saved-connection:${connection.id}`,
        kind: 'saved-connection',
        label: `${connection.name || connection.host} (${connection.username}@${connection.host}:${connection.port})`,
        metadata: connection,
      })),
      nextActions: matches.length === 1
        ? [
            { tool: 'connect_saved_session', args: { connection_id: matches[0].id }, reason: 'Use the matching saved connection instead of manual ssh.', priority: 'recommended' },
          ]
        : [
            { tool: 'connect_saved_connection_by_query', args: { query: parsed.host }, reason: 'Disambiguate and connect using saved connection metadata.', priority: 'recommended' },
          ],
      ...(matches.length > 1
        ? {
            disambiguation: {
              prompt: 'Multiple saved connections match this ssh target. Choose one saved connection.',
              options: matches.map((connection) => ({
                id: connection.id,
                label: `${connection.name || connection.host} — ${connection.username}@${connection.host}:${connection.port}`,
                args: { query: parsed.host, connection_id: connection.id },
              })),
            },
          }
        : {}),
    });
  } catch {
    return null;
  }
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

  const result = await terminalRunRemote({ nodeId: resolved.nodeId, command, cwd, timeoutSecs });
  const combined = result.stderr
    ? `${result.stdout}\n--- stderr ---\n${result.stderr}`
    : result.stdout;

  // Apply semantic sampling on verbose output to focus on errors/commands
  const lines = combined.split('\n');
  const processed = lines.length > 100 ? semanticSample(lines, 200).join('\n') : combined;

  const { text, truncated } = truncateOutput(processed);

  const success = result.exitCode === 0 || result.exitCode === null;
  return envelopeResult(toolCallId, {
    ok: success,
    toolName: 'terminal_exec',
    summary: success ? 'Remote command completed.' : `Remote command exited with ${result.exitCode}.`,
    output: text,
    data: { nodeId: resolved.nodeId, exitCode: result.exitCode },
    execution: createExecutionSummary({
      kind: 'command',
      command,
      cwd,
      target: { id: `ssh-node:${resolved.nodeId}`, kind: 'ssh-node', label: resolved.nodeId },
      exitCode: result.exitCode ?? null,
      timedOut: false,
      truncated,
      stderr: result.stderr,
      errorMessage: success ? undefined : `Exit code: ${result.exitCode}`,
    }),
    capability: 'command.run',
    targetId: `ssh-node:${resolved.nodeId}`,
    truncated,
    durationMs: Date.now() - startTime,
    targets: [{ id: `ssh-node:${resolved.nodeId}`, kind: 'ssh-node', label: resolved.nodeId, metadata: { nodeId: resolved.nodeId } }],
    ...(success ? {} : {
      error: {
        code: 'remote_command_failed',
        message: `Exit code: ${result.exitCode}`,
        recoverable: true,
      },
      recoverable: true,
    }),
  });
}

async function execTerminalCommandToSession(
  args: Record<string, unknown>,
  sessionId: string,
  startTime: number,
  toolCallId: string,
  abortSignal?: AbortSignal,
): Promise<AiToolResult> {
  const command = typeof args.command === 'string' ? args.command.trim() : '';
  if (!command) {
    return { toolCallId, toolName: 'terminal_exec', success: false, output: '', error: 'Missing required argument: command', durationMs: Date.now() - startTime };
  }

  const savedConnectionGuardrail = await detectSavedConnectionSshMisuse(command, startTime, toolCallId, 'terminal_exec');
  if (savedConnectionGuardrail) {
    return savedConnectionGuardrail;
  }

  const notReadyError = await waitForInteractiveTerminalReady(sessionId, abortSignal);
  if (notReadyError) {
    return { toolCallId, toolName: 'terminal_exec', success: false, output: '', error: notReadyError, durationMs: Date.now() - startTime };
  }

  // Pre-command snapshot: take BEFORE writing command to avoid race condition
  // where backend buffer updates before our snapshot read completes.
  const preSnapshotLineCount = await readBufferLineCount(sessionId);
  if (preSnapshotLineCount !== null) {
    console.debug(`[AI:ToolExec] pre-command snapshot: ${preSnapshotLineCount} lines`);
  }
  const preRenderedLineCount = readRenderedBufferLines(sessionId)?.length ?? null;
  const preRenderedText = readRenderedBufferText(sessionId);

  const outputSubscription = createTerminalOutputSubscription(sessionId);
  try {
    const sendResult = terminalSend({ sessionId, input: command, inputKind: 'command', appendEnter: true });
    if (!sendResult.ok) {
      return {
        toolCallId,
        toolName: 'terminal_exec',
        success: false,
        output: '',
        error: sendResult.error,
        durationMs: Date.now() - startTime,
      };
    }

    // Auto-await output (default: true)
    const awaitOutput = args.await_output !== false;
    if (!awaitOutput) {
      return envelopeResult(toolCallId, {
        ok: true,
        toolName: 'terminal_exec',
        summary: 'Command sent to terminal session.',
        output: `Command sent to terminal session ${sessionId}: ${command}`,
        data: { sessionId, command },
        execution: createExecutionSummary({
          kind: 'terminal',
          command,
          target: { id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}` },
          exitCode: null,
          timedOut: false,
          truncated: false,
        }),
        capability: 'terminal.send',
        targetId: `terminal-session:${sessionId}`,
        durationMs: Date.now() - startTime,
        targets: [{ id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}`, metadata: { sessionId } }],
      });
    }

    const waitResult = await waitForTerminalOutput(
      sessionId,
      AUTO_AWAIT_TIMEOUT_SECS,
      AUTO_AWAIT_STABLE_SECS,
      null,
      startTime,
      preSnapshotLineCount,
      abortSignal,
      outputSubscription,
      preRenderedText,
    );

    if (waitResult.error === 'Generation was stopped.') {
      return {
        toolCallId,
        toolName: 'terminal_exec',
        success: false,
        output: `Command was sent to terminal session ${sessionId} before generation stopped: ${command}`,
        error: waitResult.error,
        durationMs: Date.now() - startTime,
      };
    }

    const renderedWaitResult = renderedDeltaFromLineCount(sessionId, preRenderedLineCount, waitResult, {
      completionPromptRe: COMPLETION_PROMPT_RE,
      truncateOutput,
    }) ?? renderedDeltaFromTextSnapshot(sessionId, preRenderedText, waitResult, truncateOutput);

    return envelopeResult(toolCallId, {
      ok: renderedWaitResult?.success ?? waitResult.success,
      toolName: 'terminal_exec',
      summary: (renderedWaitResult?.success ?? waitResult.success) ? 'Terminal command output captured.' : 'Terminal command did not produce completed output.',
      output: renderedWaitResult?.output ?? waitResult.output,
      data: { sessionId, command },
      execution: createExecutionSummary({
        kind: 'terminal',
        command,
        target: { id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}` },
        exitCode: null,
        timedOut: waitResult.reason === 'timeout',
        truncated: renderedWaitResult?.truncated ?? waitResult.truncated ?? false,
        errorMessage: renderedWaitResult?.error ?? waitResult.error,
      }),
      capability: 'terminal.send',
      targetId: `terminal-session:${sessionId}`,
      ...(renderedWaitResult?.error ?? waitResult.error ? {
        error: {
          code: 'terminal_command_wait_failed',
          message: renderedWaitResult?.error ?? waitResult.error ?? 'Terminal command wait failed.',
          recoverable: true,
        },
        recoverable: true,
      } : {}),
      truncated: renderedWaitResult?.truncated ?? waitResult.truncated,
      durationMs: Date.now() - startTime,
      targets: [{ id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}`, metadata: { sessionId } }],
    });
  } finally {
    outputSubscription.unsubscribe();
  }
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
    const data: FileReadData = {
      path,
      content: text,
      encoding: result.encoding ?? 'utf-8',
      size: result.size,
      mtime: result.mtime,
      contentHash: result.hash,
      ...(truncated ? { truncated: true } : {}),
    };
    return envelopeResult<FileReadData>(toolCallId, {
      ok: true,
      toolName: 'read_file',
      capability: 'filesystem.read',
      targetId: `ssh-node:${resolved.nodeId}`,
      summary: `Read ${path} (${result.size} bytes, hash: ${result.hash})`,
      output: text,
      data,
      truncated,
      durationMs: Date.now() - startTime,
    });
  }

  // Fallback: exec cat via SSH
  const result = await nodeIdeExecCommand(resolved.nodeId, `cat ${shellEscape(path)}`, undefined, 10);
  const { text, truncated } = truncateOutput(result.stdout);
  const contentHash = await hashTextContent(result.stdout);
  return {
    toolCallId,
    toolName: 'read_file',
    success: result.exitCode === 0,
    output: text,
    error: result.exitCode !== 0 ? result.stderr : undefined,
    truncated,
    durationMs: Date.now() - startTime,
    ...(result.exitCode === 0
      ? {
          envelope: createToolResultEnvelope<FileReadData>({
            ok: true,
            toolName: 'read_file',
            capability: 'filesystem.read',
            targetId: `ssh-node:${resolved.nodeId}`,
            summary: `Read ${path} (${byteLengthOfText(result.stdout)} bytes, hash: ${contentHash})`,
            output: text,
            data: {
              path,
              content: text,
              encoding: 'utf-8',
              size: byteLengthOfText(result.stdout),
              contentHash,
              ...(truncated ? { truncated: true } : {}),
            },
            truncated,
            durationMs: Date.now() - startTime,
          }),
        }
      : {}),
  };
}

async function execWriteFile(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const request = parseFileWriteRequest(args);
  const path = request.path;
  const content = typeof args.content === 'string' ? request.content : undefined;
  if (!path || content === undefined) {
    return { toolCallId, toolName: 'write_file', success: false, output: '', error: 'Missing required arguments: path, content', durationMs: Date.now() - startTime };
  }

  if (resolved.agentAvailable) {
    let existing: { content?: string; hash?: string; mtime?: number | null; size?: number | null } | undefined;
    if (request.createOnly || request.dryRun || request.append || request.expectedHash || request.expectedMtime !== undefined) {
      try {
        const current = await nodeAgentReadFile(resolved.nodeId, path);
        existing = {
          content: current.content,
          hash: current.hash,
          mtime: current.mtime,
          size: current.size,
        };
        if (request.createOnly) {
          return recoverableFileError(toolCallId, 'write_file', startTime, 'file_exists', `Refusing to create ${path}: file already exists.`);
        }
        const validationError = validateExistingFile(toolCallId, 'write_file', request, existing, startTime);
        if (validationError) return validationError;
      } catch (e) {
        if (isNotFoundError(e)) {
          if (request.expectedHash || request.expectedMtime !== undefined) {
            return recoverableFileError(toolCallId, 'write_file', startTime, 'expected_file_missing', `Cannot verify write precondition for ${path}: file does not exist.`);
          }
        } else if (request.expectedHash || request.expectedMtime !== undefined || request.dryRun) {
          throw e;
        }
      }
    }

    const nextContent = request.append && existing?.content !== undefined ? existing.content + content : content;
    const afterHash = await hashTextContent(nextContent);
    const diffSummary = buildFileDiffSummary({
      beforeContent: existing?.content,
      beforeSize: existing?.size,
      beforeHash: existing?.hash,
      afterContent: nextContent,
      afterHash,
    });

    if (request.dryRun) {
      return envelopeResult<FileWriteData>(toolCallId, {
        ok: true,
        toolName: 'write_file',
        capability: 'filesystem.write',
        targetId: `ssh-node:${resolved.nodeId}`,
        summary: `Dry run: would write ${diffSummary.afterSize} bytes to ${path}`,
        output: JSON.stringify({ path, dryRun: true, diffSummary }, null, 2),
        data: {
          path,
          size: diffSummary.afterSize,
          contentHash: afterHash,
          dryRun: true,
          diffSummary,
        },
        durationMs: Date.now() - startTime,
      });
    }

    const result = await nodeAgentWriteFile(resolved.nodeId, path, nextContent, request.expectedHash);
    const warnings = request.expectedHash || request.createOnly ? [] : [UNCONDITIONAL_OVERWRITE_WARNING];
    return envelopeResult<FileWriteData>(toolCallId, {
      ok: true,
      toolName: 'write_file',
      capability: 'filesystem.write',
      targetId: `ssh-node:${resolved.nodeId}`,
      summary: `Written ${result.size} bytes to ${path} (hash: ${result.hash})`,
      output: `Written ${result.size} bytes to ${path} (hash: ${result.hash})`,
      data: {
        path,
        size: result.size,
        mtime: result.mtime,
        contentHash: result.hash,
        atomic: result.atomic,
        diffSummary: {
          ...diffSummary,
          afterHash: result.hash,
        },
      },
      warnings,
      durationMs: Date.now() - startTime,
    });
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

/** Group grep matches by file path to reduce path repetition in output */
function formatGrepResults(matches: Array<{ path: string; line: number; text: string }>): string {
  if (matches.length === 0) return 'No matches found.';
  const grouped = new Map<string, Array<{ line: number; text: string }>>();
  for (const m of matches) {
    let arr = grouped.get(m.path);
    if (!arr) { arr = []; grouped.set(m.path, arr); }
    arr.push({ line: m.line, text: m.text });
  }
  const parts: string[] = [];
  for (const [path, items] of grouped) {
    parts.push(`${path}:`);
    for (const item of items) {
      parts.push(`  L${item.line}: ${item.text}`);
    }
  }
  return parts.join('\n');
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
    const output = formatGrepResults(matches);
    const { text, truncated } = truncateOutput(output);
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
  if (result.exitCode !== 0 && result.exitCode !== 1) {
    return {
      toolCallId,
      toolName: 'grep_search',
      success: false,
      output: '',
      error: result.stderr || `grep failed with exit code ${result.exitCode}`,
      durationMs: Date.now() - startTime,
    };
  }

  const { text, truncated } = truncateOutput(result.exitCode === 1 ? 'No matches found.' : (result.stdout || 'No matches found.'));
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
        const host = conn ? sanitizeConnectionInfo(conn.username, conn.host, conn.port) : sanitizeConnectionInfo(node.username || '?', node.host || '?', node.port || 22);
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

function collectToolTargets(): ToolTarget[] {
  const appState = useAppStore.getState();
  return buildToolTargets({
    tabs: appState.tabs,
    activeTabId: appState.activeTabId,
    sshNodes: useSessionTreeStore.getState().nodes,
    localTerminals: useLocalTerminalStore.getState().terminals,
  });
}

function formatTarget(target: ToolTarget, index: number): string {
  const active = target.active ? ' ★' : '';
  const ids = [
    target.nodeId ? `node_id=${target.nodeId}` : undefined,
    target.sessionId ? `session_id=${target.sessionId}` : undefined,
    target.tabId ? `tab_id=${target.tabId}` : undefined,
  ].filter(Boolean).join(' ');
  const idSuffix = ids ? ` ${ids}` : '';
  return `${index + 1}. [${target.kind}] id=${target.id}${idSuffix} "${target.label}" capabilities=${target.capabilities.join(',')}${active}`;
}

function targetMatchesText(target: ToolTarget, query: string): boolean {
  if (!query) return true;
  const haystack = [
    target.id,
    target.label,
    target.nodeId,
    target.sessionId,
    target.tabId,
    JSON.stringify(target.metadata ?? {}),
  ].filter(Boolean).join(' ').toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .some((token) => haystack.includes(token));
}

function capabilityScoreForIntent(target: ToolTarget, intent: string): number {
  const caps = target.capabilities;
  switch (intent) {
    case 'command':
      return caps.includes('command.run') ? 8 : caps.includes('terminal.send') ? 5 : 0;
    case 'terminal_interaction':
      return caps.includes('terminal.send') || caps.includes('terminal.observe')
        ? 8
        : target.kind === 'saved-connection'
          ? 4
          : 0;
    case 'connection':
      return target.kind === 'saved-connection' ? 10 : target.kind === 'ssh-node' ? 8 : caps.includes('navigation.open') ? 3 : 0;
    case 'settings':
      return caps.includes('settings.write') || caps.includes('settings.read') ? 10 : 0;
    case 'remote_file':
    case 'sftp':
      return caps.includes('filesystem.read') || caps.includes('filesystem.write') ? 8 : 0;
    case 'ide':
      return target.kind === 'ide-workspace' ? 8 : caps.includes('filesystem.search') ? 4 : 0;
    case 'local_shell':
      return target.kind === 'local-shell' || target.metadata?.terminalType === 'local_terminal' ? 8 : 0;
    case 'monitoring':
    case 'status':
      return caps.includes('state.list') ? 5 : 0;
    case 'navigation':
      return caps.includes('navigation.open') ? 5 : 0;
    default:
      return 0;
  }
}

function serializeResolvedTarget(target: ToolTarget): Record<string, unknown> {
  return {
    target_id: target.id,
    kind: target.kind,
    label: target.label,
    ...(target.nodeId ? { node_id: target.nodeId } : {}),
    ...(target.sessionId ? { session_id: target.sessionId } : {}),
    ...(target.tabId ? { tab_id: target.tabId } : {}),
    capabilities: target.capabilities,
    metadata: target.metadata ?? {},
  };
}

async function collectSavedConnectionTargets(query: string): Promise<ToolTarget[]> {
  if (!query) return [];
  try {
    const connections = await api.searchConnections(query);
    return connections.map((connection) => ({
      id: `saved-connection:${connection.id}`,
      kind: 'saved-connection' as const,
      label: `${connection.name || connection.host} (${connection.username}@${connection.host}:${connection.port})`,
      capabilities: ['navigation.open', 'state.list'] as ToolTarget['capabilities'],
      metadata: {
        savedConnection: true,
        connectionId: connection.id,
        host: connection.host,
        port: connection.port,
        username: connection.username,
        name: connection.name,
        group: connection.group ?? null,
      },
    }));
  } catch {
    return [];
  }
}

async function execResolveTarget(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'resolve_target';
  const query = plainStringArg(args, 'query');
  const intent = plainStringArg(args, 'intent');
  const kind = plainStringArg(args, 'kind') || 'all';
  if (isBroadConnectionDiscoveryQuery(query, intent)) {
    return envelopeResult(toolCallId, {
      ok: false,
      toolName,
      summary: 'This is a connection listing request, not a single-target resolution.',
      output: 'Use list_saved_connections to list available saved SSH hosts. Use search_saved_connections only when the user provides a specific host/name keyword.',
      error: {
        code: 'target_query_too_broad',
        message: 'resolve_target needs one intended target; broad connection discovery should use list_saved_connections.',
        recoverable: true,
      },
      recoverable: true,
      durationMs: Date.now() - startTime,
      nextActions: [
        { tool: 'list_saved_connections', reason: 'List available saved SSH hosts for the user.', priority: 'recommended' },
      ],
    });
  }
  const runtimeTargets = collectToolTargets();
  const savedConnectionTargets = intent === 'connection' || query
    ? await collectSavedConnectionTargets(query)
    : [];
  const allTargets = [...savedConnectionTargets, ...runtimeTargets]
    .filter((target) => kind === 'all' || target.kind === kind);

  const scored = allTargets
    .map((target) => {
      const textMatch = targetMatchesText(target, query);
      const score = (textMatch ? 12 : query ? 0 : 2)
        + capabilityScoreForIntent(target, intent)
        + (target.active ? 1 : 0)
        + (target.metadata?.savedConnection ? 2 : 0);
      return { target, score };
    })
    .filter((entry) => entry.score > 0)
    .sort((a, b) => b.score - a.score || a.target.label.localeCompare(b.target.label));

  if (scored.length === 0) {
    return envelopeResult(toolCallId, {
      ok: false,
      toolName,
      summary: 'No matching target found.',
      output: `No target matched${query ? ` "${query}"` : ''}${intent ? ` for intent "${intent}"` : ''}.`,
      error: {
        code: 'target_not_found',
        message: 'No matching target found.',
        recoverable: true,
      },
      recoverable: true,
      durationMs: Date.now() - startTime,
      nextActions: [
        { tool: 'list_targets', args: kind === 'all' ? {} : { kind }, reason: 'Inspect available targets.', priority: 'recommended' },
      ],
    });
  }

  const top = scored[0];
  const second = scored[1];
  const ambiguous = Boolean(second && top.score - second.score < 3);
  const candidates = scored.slice(0, 6).map((entry) => serializeResolvedTarget(entry.target));

  if (ambiguous) {
    return envelopeResult(toolCallId, {
      ok: false,
      toolName,
      summary: 'Multiple targets match. Choose one target before acting.',
      output: JSON.stringify({ candidates }, null, 2),
      error: {
        code: 'target_ambiguous',
        message: 'Multiple targets match the request.',
        recoverable: true,
      },
      recoverable: true,
      durationMs: Date.now() - startTime,
      targets: scored.slice(0, 6).map((entry) => ({
        id: entry.target.id,
        kind: entry.target.kind,
        label: entry.target.label,
        metadata: entry.target.metadata,
      })),
      disambiguation: {
        prompt: 'Multiple targets match. Use one option before running a write/execute tool.',
        options: scored.slice(0, 6).map((entry) => ({
          id: entry.target.id,
          label: entry.target.label,
          args: { target_id: entry.target.id },
        })),
      },
    });
  }

  const resolved = serializeResolvedTarget(top.target);
  const savedConnectionId = typeof top.target.metadata?.connectionId === 'string'
    ? top.target.metadata.connectionId
    : undefined;
  return envelopeResult(toolCallId, {
    ok: true,
    toolName,
    summary: `Resolved target: ${top.target.label}`,
    output: JSON.stringify(resolved, null, 2),
    data: resolved,
    targetId: top.target.id,
    durationMs: Date.now() - startTime,
    targets: [{
      id: top.target.id,
      kind: top.target.kind,
      label: top.target.label,
      metadata: top.target.metadata,
    }],
    nextActions: top.target.metadata?.savedConnection && savedConnectionId
      ? [
          { tool: 'connect_saved_session', args: { connection_id: savedConnectionId }, reason: 'Connect using the saved connection target.', priority: 'recommended' },
        ]
      : [],
  });
}

function execListTargets(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const kind = typeof args.kind === 'string' ? args.kind : 'all';
  const targets = collectToolTargets().filter((target) => kind === 'all' || target.kind === kind);
  if (targets.length === 0) {
    return { toolCallId, toolName: 'list_targets', success: true, output: `No targets found${kind === 'all' ? '' : ` for kind "${kind}"`}.`, durationMs: Date.now() - startTime };
  }

  const lines = [
    `Targets (${targets.length})`,
    ...targets.map(formatTarget),
    '',
    'JSON:',
    JSON.stringify({ targets }, null, 2),
  ];

  const { text, truncated } = truncateOutput(lines.join('\n'));
  return { toolCallId, toolName: 'list_targets', success: true, output: text, truncated, durationMs: Date.now() - startTime };
}

function execListCapabilities(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const targetId = typeof args.target_id === 'string' ? args.target_id.trim() : '';
  const resolvedTarget = targetId ? resolveTargetById(targetId)?.target ?? null : null;
  const targets = collectToolTargets();
  const scopedTargets = targetId ? (resolvedTarget ? [resolvedTarget] : []) : targets;
  if (targetId && scopedTargets.length === 0) {
    return { toolCallId, toolName: 'list_capabilities', success: false, output: '', error: `Target not found: ${targetId}. Use list_targets first.`, durationMs: Date.now() - startTime };
  }

  const capabilities = buildCapabilityStatuses(scopedTargets);
  if (capabilities.length === 0) {
    return { toolCallId, toolName: 'list_capabilities', success: true, output: 'No capabilities available.', durationMs: Date.now() - startTime };
  }

  const lines = [
    `Capabilities (${capabilities.length})`,
    ...capabilities.map((capability) => (
      `- ${capability.capability} on ${capability.targetId} "${capability.targetLabel}"${capability.notes ? ` (${capability.notes})` : ''}`
    )),
    '',
    'JSON:',
    JSON.stringify({ capabilities }, null, 2),
  ];

  const { text, truncated } = truncateOutput(lines.join('\n'));
  return { toolCallId, toolName: 'list_capabilities', success: true, output: text, truncated, durationMs: Date.now() - startTime };
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
    return `- [${conn.state}] id=${conn.id} → ${sanitizeConnectionInfo(conn.username, conn.host, conn.port)}${env} — ${conn.terminalIds.length} terminal(s), ${conn.forwardIds.length} forward(s), keepAlive=${conn.keepAlive}`;
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
  const maxLines = clamp(Number(args.max_lines) || 200, 1, 500);

  // Prefer the rendered xterm buffer when the pane is open: it has already
  // passed through the terminal encoding decoder, while backend history is
  // still byte-oriented for legacy encodings.
  const renderedSnapshot = readRenderedBufferTail(sessionId, maxLines);
  if (renderedSnapshot) {
    const { text, truncated } = truncateOutput(renderedSnapshot.lines.join('\n'));
    return { toolCallId, toolName: 'get_terminal_buffer', success: true, output: text || '(empty buffer)', truncated, durationMs: Date.now() - startTime };
  }

  const snapshot = await readBufferTail(sessionId, maxLines);
  if (snapshot) {
    const { text, truncated } = truncateOutput(snapshot.lines.join('\n'));
    return { toolCallId, toolName: 'get_terminal_buffer', success: true, output: text || '(empty buffer)', truncated, durationMs: Date.now() - startTime };
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
  const renderedMatches = searchRenderedBuffer(sessionId, query, {
    caseSensitive: (args.case_sensitive as boolean) ?? false,
    regex: (args.regex as boolean) ?? false,
    maxResults,
  });
  if (renderedMatches?.error) {
    return { toolCallId, toolName: 'search_terminal', success: false, output: '', error: renderedMatches.error, durationMs: Date.now() - startTime };
  }
  if (renderedMatches && renderedMatches.lines.length > 0) {
    const { text, truncated } = truncateOutput(renderedMatches.lines.join('\n') + `\n${renderedMatches.lines.length} rendered match(es)`);
    return { toolCallId, toolName: 'search_terminal', success: true, output: text, truncated, durationMs: Date.now() - startTime };
  }

  const result = await api.searchTerminalLayered(sessionId, {
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

  const lines = result.matches.map(m => {
    const sourceLabel = m.source === 'cold' ? '[archived]' : '[recent]';
    return `${sourceLabel} L${m.line_number}:${m.column_start}: ${m.line_content}`;
  });
  const footerParts = [`${result.total_matches} match(es) in ${result.duration_ms}ms`];
  if (result.truncated) footerParts.push('results truncated');
  if (result.partial_failure) footerParts.push('partial archived search failure');
  if (result.archive_status.degraded) footerParts.push('archive degraded');
  const footer = `\n${footerParts.join(' | ')}`;
  const { text, truncated } = truncateOutput(lines.join('\n') + footer);
  return { toolCallId, toolName: 'search_terminal', success: true, output: text, truncated, durationMs: Date.now() - startTime };
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared Terminal Output Waiting Logic
// ═══════════════════════════════════════════════════════════════════════════

type WaitResult = TerminalWaitResult;

/**
 * Core logic: wait for new terminal output after a command is sent.
 * Uses a synchronous notification counter combined with periodic polling
 * to avoid async-in-callback race conditions with the microtask-coalesced
 * notification system in terminalRegistry.
 *
 * Design: The `onOutput` listener is kept purely synchronous (increments a
 * counter) so that `notifyTerminalOutput()` fire-and-forget + microtask
 * coalescing can never swallow it. A `setInterval` poller checks the
 * counter and performs the actual IPC buffer read in a safe async context.
 *
 * Shared by `terminal_exec` (auto-await) and `await_terminal_output`.
 */
async function waitForTerminalOutput(
  sessionId: string,
  timeoutSecs: number,
  stableSecs: number,
  patternRe: RegExp | null,
  startTime: number,
  preSnapshotLineCount?: number | null,
  abortSignal?: AbortSignal,
  existingSubscription?: TerminalOutputSubscription,
  initialRenderedText?: string | null,
): Promise<WaitResult> {
  return waitForTerminalOutputV2({
    sessionId,
    timeoutSecs,
    stableSecs,
    patternRe,
    startTime,
    preSnapshotLineCount,
    abortSignal,
    existingSubscription,
    initialRenderedText,
    completionPromptRe: COMPLETION_PROMPT_RE,
    interactiveInputPromptRe: INTERACTIVE_INPUT_PROMPT_RE,
    truncateOutput,
    emptyOutputTailLines: EMPTY_OUTPUT_TAIL_LINES,
    fallbackTailLines: TERMINAL_BUFFER_TAIL_FALLBACK_LINES,
    promptGraceMs: PROMPT_GRACE_MS,
    maxAdaptiveStableSecs: MAX_ADAPTIVE_STABLE_SECS,
  });
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
  abortSignal?: AbortSignal,
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

  const waitResult = await waitForTerminalOutput(sessionId, timeoutSecs, stableSecs, patternRe, startTime, undefined, abortSignal);
  return envelopeResult(toolCallId, {
    ok: waitResult.success,
    toolName,
    summary: waitResult.success ? 'Terminal output captured.' : 'Terminal output wait failed.',
    output: waitResult.output,
    data: { sessionId, reason: waitResult.reason },
    execution: createExecutionSummary({
      kind: 'terminal',
      target: { id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}` },
      exitCode: null,
      timedOut: waitResult.reason === 'timeout',
      truncated: waitResult.truncated ?? false,
      errorMessage: waitResult.error,
    }),
    capability: 'terminal.wait',
    targetId: `terminal-session:${sessionId}`,
    truncated: waitResult.truncated,
    durationMs: Date.now() - startTime,
    targets: [{ id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}`, metadata: { sessionId } }],
    ...(waitResult.success ? {} : {
      error: {
        code: waitResult.reason === 'timeout' ? 'terminal_wait_timeout' : 'terminal_wait_failed',
        message: waitResult.error ?? 'Terminal output wait failed.',
        recoverable: true,
      },
      recoverable: true,
    }),
  });
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
  abortSignal?: AbortSignal,
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

  const notReadyError = await waitForInteractiveTerminalReady(sessionId, abortSignal);
  if (notReadyError) {
    return { toolCallId, toolName, success: false, output: '', error: notReadyError, durationMs: Date.now() - startTime };
  }

  const preSnapshotLineCount = await readBufferLineCount(sessionId);
  const outputSubscription = createTerminalOutputSubscription(sessionId);
  let waitResult: WaitResult;
  try {
    const sendResult = terminalSend({
      sessionId,
      input: CONTROL_SEQUENCES[rawSequence],
      inputKind: 'control',
    });
    if (!sendResult.ok) {
      return { toolCallId, toolName, success: false, output: '', error: sendResult.error, durationMs: Date.now() - startTime };
    }

    // Wait briefly for terminal response
    waitResult = await waitForTerminalOutput(sessionId, 3, 1, null, startTime, preSnapshotLineCount, abortSignal, outputSubscription);
  } finally {
    outputSubscription.unsubscribe();
  }

  const label = CONTROL_LABELS[rawSequence] || rawSequence;
  if (waitResult.error === 'Generation was stopped.') {
    return {
      toolCallId,
      toolName,
      success: false,
      output: `Sent ${label} to session ${sessionId} before generation stopped.`,
      error: waitResult.error,
      durationMs: Date.now() - startTime,
    };
  }

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
  abortSignal?: AbortSignal,
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

  const results: string[] = [];
  const executionItems: NonNullable<ReturnType<typeof createExecutionSummary>['items']> = [];

  try {
    for (let i = 0; i < commands.length; i++) {
      throwIfAborted(abortSignal);

      const cmd = typeof commands[i] === 'string' ? commands[i].trim() : '';
      if (!cmd) {
        results.push(`[${i + 1}] (empty command — skipped)`);
        executionItems.push({ command: '', exitCode: null, timedOut: false, truncated: false });
        continue;
      }

      const notReadyError = await waitForInteractiveTerminalReady(sessionId, abortSignal);
      if (notReadyError) {
        results.push(`[${i + 1}] $ ${cmd}\n❌ ${notReadyError}`);
        executionItems.push({
          command: cmd,
          exitCode: null,
          timedOut: false,
          truncated: false,
          stderrSummary: createExecutionSummary({ errorMessage: notReadyError }).stderrSummary,
        });
        break;
      }

      // Pre-command snapshot: capture buffer line count BEFORE sending the command
      const preSnapshotLineCount = await readBufferLineCount(sessionId);

      const outputSubscription = createTerminalOutputSubscription(sessionId);
      try {
        const sendResult = terminalSend({ sessionId, input: cmd, inputKind: 'command', appendEnter: true });
        if (!sendResult.ok) {
          results.push(`[${i + 1}] $ ${cmd}\n❌ ${sendResult.error || 'Terminal is not writable.'}`);
          executionItems.push({
            command: cmd,
            exitCode: null,
            timedOut: false,
            truncated: false,
            stderrSummary: createExecutionSummary({ errorMessage: sendResult.error || 'Terminal is not writable.' }).stderrSummary,
          });
          break;
        }

        const waitResult = await waitForTerminalOutput(
          sessionId,
          AUTO_AWAIT_TIMEOUT_SECS,
          AUTO_AWAIT_STABLE_SECS,
          null,
          startTime,
          preSnapshotLineCount,
          abortSignal,
          outputSubscription,
        );

        if (!waitResult.success) {
          results.push(`[${i + 1}] $ ${cmd}\n❌ ${waitResult.error || 'Failed to read output.'}`);
          executionItems.push({
            command: cmd,
            exitCode: null,
            timedOut: waitResult.reason === 'timeout',
            truncated: waitResult.truncated ?? false,
            stderrSummary: createExecutionSummary({ errorMessage: waitResult.error || 'Failed to read output.' }).stderrSummary,
          });
          if (waitResult.error === 'Generation was stopped.') {
            break;
          }
        } else {
          results.push(`[${i + 1}] $ ${cmd}\n${waitResult.output}`);
          executionItems.push({
            command: cmd,
            exitCode: null,
            timedOut: waitResult.reason === 'timeout',
            truncated: waitResult.truncated ?? false,
          });
        }
      } finally {
        outputSubscription.unsubscribe();
      }
    }
  } catch (e) {
    if (!(e instanceof Error && e.name === 'AbortError')) {
      throw e;
    }
  }

  const combinedOutput = results.join('\n\n');
  const { text, truncated } = truncateOutput(combinedOutput);

  return envelopeResult(toolCallId, {
    ok: !abortSignal?.aborted,
    toolName,
    summary: abortSignal?.aborted ? 'Batch execution stopped.' : 'Batch execution completed.',
    output: text,
    data: { sessionId, commandCount: commands.length },
    execution: createExecutionSummary({
      kind: 'batch',
      target: { id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}` },
      timedOut: executionItems.some((item) => item.timedOut === true),
      truncated,
      items: executionItems,
      errorMessage: abortSignal?.aborted ? 'Generation was stopped.' : undefined,
    }),
    capability: 'terminal.send',
    targetId: `terminal-session:${sessionId}`,
    truncated,
    durationMs: Date.now() - startTime,
    targets: [{ id: `terminal-session:${sessionId}`, kind: 'terminal-session', label: `Terminal ${sessionId}`, metadata: { sessionId } }],
    ...(abortSignal?.aborted ? {
      error: { code: 'batch_exec_aborted', message: 'Generation was stopped.', recoverable: true },
      recoverable: true,
    } : {}),
  });
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

const KEY_ALIASES: Record<string, string> = {
  'esc': 'escape',
  'return': 'enter',
  'spacebar': 'space',
  'pgup': 'pageup',
  'page-up': 'pageup',
  'pgdn': 'pagedown',
  'page-down': 'pagedown',
  'del': 'delete',
  'ins': 'insert',
  'arrowup': 'up',
  'arrowdown': 'down',
  'arrowleft': 'left',
  'arrowright': 'right',
  'cmd': 'meta',
  'command': 'meta',
  'super': 'meta',
  'option': 'alt',
  'opt': 'alt',
  'control': 'ctrl',
  'ctl': 'ctrl',
};

const MODIFIER_NAMES = new Set(['ctrl', 'alt', 'shift', 'meta']);

const MODIFIER_CURSOR_FINALS: Record<string, string> = {
  'up': 'A',
  'down': 'B',
  'right': 'C',
  'left': 'D',
  'home': 'H',
  'end': 'F',
  'f1': 'P',
  'f2': 'Q',
  'f3': 'R',
  'f4': 'S',
};

const MODIFIER_TILDE_CODES: Record<string, number> = {
  'insert': 2,
  'delete': 3,
  'pageup': 5,
  'pagedown': 6,
  'f5': 15,
  'f6': 17,
  'f7': 18,
  'f8': 19,
  'f9': 20,
  'f10': 21,
  'f11': 23,
  'f12': 24,
};

const PRINTABLE_KEY_RE = /^[\x20-\x7E\u0080-\uFFFF]+$/;

type TerminalKeyModifiers = {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
};

type EncodedTerminalKey = {
  sequence: string;
  summary: string;
};

function normalizeKeyToken(token: string): string {
  const normalized = token.trim().toLowerCase().replace(/\s+/g, '');
  return KEY_ALIASES[normalized] ?? normalized;
}

function controlCharForKey(key: string): string | null {
  if (key.length === 1) {
    const lower = key.toLowerCase();
    if (lower >= 'a' && lower <= 'z') {
      return String.fromCharCode(lower.charCodeAt(0) - 96);
    }
    switch (lower) {
      case '@':
      case '2':
        return '\x00';
      case '[':
      case '3':
        return '\x1b';
      case '\\':
      case '4':
        return '\x1c';
      case ']':
      case '5':
        return '\x1d';
      case '^':
      case '6':
        return '\x1e';
      case '_':
      case '7':
      case '/':
        return '\x1f';
      case '?':
      case '8':
        return '\x7f';
      default:
        return null;
    }
  }

  if (key === 'space') {
    return '\x00';
  }

  return null;
}

function getModifierParam(modifiers: TerminalKeyModifiers): number {
  const metaAsAlt = modifiers.alt || modifiers.meta;
  return 1 + (modifiers.shift ? 1 : 0) + (metaAsAlt ? 2 : 0) + (modifiers.ctrl ? 4 : 0);
}

function encodeModifiedSpecialKey(keyName: string, modifiers: TerminalKeyModifiers): string | null {
  const modifierParam = getModifierParam(modifiers);
  if (modifierParam <= 1) {
    return KEY_SEQUENCES[keyName] ?? null;
  }

  if (keyName === 'tab') {
    return modifierParam === 2 ? '\x1b[Z' : null;
  }

  if (MODIFIER_CURSOR_FINALS[keyName]) {
    return `\x1b[1;${modifierParam}${MODIFIER_CURSOR_FINALS[keyName]}`;
  }

  if (MODIFIER_TILDE_CODES[keyName] !== undefined) {
    return `\x1b[${MODIFIER_TILDE_CODES[keyName]};${modifierParam}~`;
  }

  if (keyName === 'enter' || keyName === 'escape' || keyName === 'backspace' || keyName === 'space') {
    let sequence = keyName === 'space' ? ' ' : KEY_SEQUENCES[keyName];
    if (modifiers.ctrl) {
      sequence = controlCharForKey(keyName) ?? sequence;
    }
    if (modifiers.alt || modifiers.meta) {
      sequence = `\x1b${sequence}`;
    }
    return sequence;
  }

  return null;
}

function encodeModifiedPrintableKey(rawKey: string, normalizedKey: string, modifiers: TerminalKeyModifiers): string | null {
  let key = normalizedKey === 'space'
    ? ' '
    : (/^[a-z]$/i.test(normalizedKey) ? normalizedKey.toLowerCase() : rawKey);

  if (key.length !== 1 && normalizedKey !== 'space') {
    return null;
  }

  if (modifiers.shift && /^[a-z]$/i.test(key)) {
    key = key.toUpperCase();
  }

  let sequence = key;
  if (modifiers.ctrl) {
    sequence = controlCharForKey(normalizedKey === 'space' ? 'space' : key) ?? '';
    if (!sequence) {
      return null;
    }
  }

  if (modifiers.alt || modifiers.meta) {
    sequence = `\x1b${sequence}`;
  }

  return sequence;
}

function encodeTerminalKey(raw: string): EncodedTerminalKey | { error: string } {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { error: 'Key entries must be non-empty strings.' };
  }

  const parts = trimmed === '+' ? [trimmed] : trimmed.split(/\s*\+\s*/);
  if (parts.length === 1) {
    const normalized = normalizeKeyToken(trimmed);
    const sequence = KEY_SEQUENCES[normalized];
    if (sequence !== undefined) {
      return { sequence, summary: `[${trimmed}]` };
    }
    if (!PRINTABLE_KEY_RE.test(trimmed)) {
      return { error: 'contains control characters. Use named keys (e.g. "Escape", "Enter") instead.' };
    }
    return {
      sequence: trimmed,
      summary: trimmed.length <= 10 ? `"${trimmed}"` : `"${trimmed.slice(0, 10)}…"`,
    };
  }

  const modifiers: TerminalKeyModifiers = { ctrl: false, alt: false, shift: false, meta: false };
  let baseRaw: string | null = null;
  let baseNormalized: string | null = null;

  for (const part of parts) {
    const normalized = normalizeKeyToken(part);
    if (!normalized) {
      return { error: 'contains an empty chord segment.' };
    }
    if (MODIFIER_NAMES.has(normalized)) {
      modifiers[normalized as keyof TerminalKeyModifiers] = true;
      continue;
    }
    if (baseRaw !== null) {
      return { error: 'must contain exactly one non-modifier key.' };
    }
    baseRaw = part.trim();
    baseNormalized = normalized;
  }

  if (!baseRaw || !baseNormalized) {
    return { error: 'must include a non-modifier key (for example "Ctrl+C").' };
  }

  const specialSequence = encodeModifiedSpecialKey(baseNormalized, modifiers);
  if (specialSequence !== null) {
    return { sequence: specialSequence, summary: `[${trimmed}]` };
  }

  const printableSequence = encodeModifiedPrintableKey(baseRaw, baseNormalized, modifiers);
  if (printableSequence !== null) {
    return { sequence: printableSequence, summary: `[${trimmed}]` };
  }

  return {
    error: `combo "${trimmed}" is not supported. Use terminal-style shortcuts like Ctrl+C, Alt+X, Shift+Tab, or Ctrl+Shift+Arrow.`,
  };
}

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

  const snapshot = readTerminalScreen(sessionId);
  if (!snapshot) {
    return { toolCallId, toolName, success: false, output: '', error: 'Screen reader not available for this terminal.', durationMs: Date.now() - startTime };
  }

  const output = formatScreenSnapshot(snapshot);
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
  abortSignal?: AbortSignal,
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

  // Validate all keys are non-empty strings before sending
  for (let i = 0; i < keys.length; i++) {
    if (typeof keys[i] !== 'string' || keys[i] === '') {
      return { toolCallId, toolName, success: false, output: '', error: `keys[${i}] must be a non-empty string.`, durationMs: Date.now() - startTime };
    }
  }

  const notReadyError = await waitForInteractiveTerminalReady(sessionId, abortSignal);
  if (notReadyError) {
    return { toolCallId, toolName, success: false, output: '', error: notReadyError, durationMs: Date.now() - startTime };
  }

  const sentSummary: string[] = [];
  const preSnapshotLineCount = await readBufferLineCount(sessionId);
  const outputSubscription = createTerminalOutputSubscription(sessionId);

  try {
    try {
      for (let i = 0; i < keys.length; i++) {
        throwIfAborted(abortSignal);

        const raw = keys[i];
        const encoded = encodeTerminalKey(raw);
        if ('error' in encoded) {
          const detail = encoded.error.startsWith('combo ') ? encoded.error : `keys[${i}] ${encoded.error}`;
          return { toolCallId, toolName, success: false, output: sentSummary.join(', '), error: detail, durationMs: Date.now() - startTime };
        }

        const sendResult = terminalSend({
          sessionId,
          input: encoded.sequence,
          inputKind: 'keys',
        });
        if (!sendResult.ok) {
          return { toolCallId, toolName, success: false, output: sentSummary.join(', '), error: sendResult.error ?? `Terminal not writable at key ${i + 1}.`, durationMs: Date.now() - startTime };
        }
        sentSummary.push(encoded.summary);

        if (i < keys.length - 1) {
          await waitWithAbort(delayMs, abortSignal);
        }
      }
    } catch (e) {
      if (!(e instanceof Error && e.name === 'AbortError')) {
        throw e;
      }
    }

    // Wait briefly for terminal to process keystrokes
    if (abortSignal?.aborted) {
      return {
        toolCallId,
        toolName,
        success: false,
        output: `Sent ${sentSummary.length} key(s) before generation stopped: ${sentSummary.join(', ')}`,
        error: 'Generation was stopped.',
        durationMs: Date.now() - startTime,
      };
    }

    const waitResult = await waitForTerminalOutput(
      sessionId,
      SEND_KEYS_TIMEOUT_SECS,
      SEND_KEYS_STABLE_SECS,
      null,
      startTime,
      preSnapshotLineCount,
      abortSignal,
      outputSubscription,
    );

    if (waitResult.error === 'Generation was stopped.') {
      return {
        toolCallId,
        toolName,
        success: false,
        output: `Sent ${keys.length} key(s) before generation stopped: ${sentSummary.join(', ')}`,
        error: waitResult.error,
        durationMs: Date.now() - startTime,
      };
    }

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
  } finally {
    outputSubscription.unsubscribe();
  }
}

async function execSendMouse(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
  abortSignal?: AbortSignal,
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

  // Validate coordinates are within terminal bounds
  const snapshot = readTerminalScreen(sessionId);
  if (snapshot && (x > snapshot.cols || y > snapshot.rows)) {
    return { toolCallId, toolName, success: false, output: '', error: `Coordinates out of bounds. Terminal is ${snapshot.cols}×${snapshot.rows}, got (${x},${y}).`, durationMs: Date.now() - startTime };
  }

  const notReadyError = await waitForInteractiveTerminalReady(sessionId, abortSignal);
  if (notReadyError) {
    return { toolCallId, toolName, success: false, output: '', error: notReadyError, durationMs: Date.now() - startTime };
  }

  const preSnapshotLineCount = await readBufferLineCount(sessionId);
  const outputSubscription = createTerminalOutputSubscription(sessionId);
  let summary: string;

  try {
    if (action === 'click') {
      const btnCode = MOUSE_BUTTONS[button];
      if (btnCode === undefined) {
        return { toolCallId, toolName, success: false, output: '', error: `Invalid button: "${button}". Must be "left", "right", or "middle".`, durationMs: Date.now() - startTime };
      }

      // SGR mouse protocol: press = \x1b[<btn;x;yM, release = \x1b[<btn;x;ym
      const press = `\x1b[<${btnCode};${x};${y}M`;
      const release = `\x1b[<${btnCode};${x};${y}m`;

      const sendResult = terminalSend({
        sessionId,
        input: press + release,
        inputKind: 'mouse',
      });
      if (!sendResult.ok) {
        return { toolCallId, toolName, success: false, output: '', error: sendResult.error, durationMs: Date.now() - startTime };
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

      const sendResult = terminalSend({
        sessionId,
        input: scrollData,
        inputKind: 'mouse',
      });
      if (!sendResult.ok) {
        return { toolCallId, toolName, success: false, output: '', error: sendResult.error, durationMs: Date.now() - startTime };
      }
      summary = `Scrolled ${direction} ${count} step(s) at (${x},${y})`;
    }

    // Brief wait for TUI to react
    const waitResult = await waitForTerminalOutput(sessionId, 2, 0.5, null, startTime, preSnapshotLineCount, abortSignal, outputSubscription);

    if (waitResult.error === 'Generation was stopped.') {
      return {
        toolCallId,
        toolName,
        success: false,
        output: `${summary} before generation stopped.`,
        error: waitResult.error,
        durationMs: Date.now() - startTime,
      };
    }

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
  } finally {
    outputSubscription.unsubscribe();
  }
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
      const info = await nodeSftpStat(resolved.nodeId, path).catch(() => undefined);
      const contentHash = await hashTextContent(data, encoding ?? 'utf-8');
      const output = `Language: ${language ?? 'unknown'}\nEncoding: ${encoding ?? 'utf-8'}\nHash: ${contentHash}\n\n${text}`;
      return envelopeResult<FileReadData>(toolCallId, {
        ok: true,
        toolName: 'sftp_read_file',
        capability: 'filesystem.read',
        targetId: `ssh-node:${resolved.nodeId}`,
        summary: `Read ${path} (${info?.size ?? byteLengthOfText(data)} bytes, hash: ${contentHash})`,
        output,
        data: {
          path,
          content: text,
          encoding: encoding ?? 'utf-8',
          size: info?.size ?? byteLengthOfText(data),
          mtime: info?.modified ?? null,
          contentHash,
          ...(text !== data ? { truncated: true } : {}),
        },
        truncated: text !== data,
        durationMs: Date.now() - startTime,
      });
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

async function execSftpWriteFile(
  args: Record<string, unknown>,
  resolved: ResolvedNode,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'sftp_write_file';
  const request = parseFileWriteRequest(args);
  const path = request.path;
  const content = typeof args.content === 'string' ? request.content : undefined;
  const encoding = request.encoding;

  if (!path) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };
  if (content === undefined) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: content', durationMs: Date.now() - startTime };

  try {
    let existing: { content?: string; hash?: string; mtime?: number | null; size?: number | null } | undefined;
    if (request.createOnly || request.dryRun || request.append || request.expectedHash || request.expectedMtime !== undefined) {
      try {
        const info = await nodeSftpStat(resolved.nodeId, path);
        if (request.createOnly) {
          return recoverableFileError(toolCallId, toolName, startTime, 'file_exists', `Refusing to create ${path}: file already exists.`);
        }
        existing = {
          size: info.size,
          mtime: info.modified,
        };

        if (request.expectedHash || request.dryRun || request.append) {
          const preview = await nodeSftpPreview(resolved.nodeId, path);
          if (!('Text' in preview)) {
            return recoverableFileError(toolCallId, toolName, startTime, 'existing_file_not_text', `Cannot safely verify existing file ${path}: preview is not text.`);
          }
          existing.content = preview.Text.data;
          existing.hash = await hashTextContent(preview.Text.data, preview.Text.encoding ?? 'utf-8');
        }

        const validationError = validateExistingFile(toolCallId, toolName, request, existing, startTime);
        if (validationError) return validationError;
      } catch (e) {
        if (isNotFoundError(e)) {
          if (request.expectedHash || request.expectedMtime !== undefined) {
            return recoverableFileError(toolCallId, toolName, startTime, 'expected_file_missing', `Cannot verify write precondition for ${path}: file does not exist.`);
          }
        } else if (request.expectedHash || request.expectedMtime !== undefined || request.dryRun) {
          throw e;
        }
      }
    }

    const nextContent = request.append && existing?.content !== undefined ? existing.content + content : content;
    const afterHash = await hashTextContent(nextContent, encoding ?? 'utf-8');
    const diffSummary = buildFileDiffSummary({
      beforeContent: existing?.content,
      beforeSize: existing?.size,
      beforeHash: existing?.hash,
      afterContent: nextContent,
      afterHash,
    });

    if (request.dryRun) {
      return envelopeResult<FileWriteData>(toolCallId, {
        ok: true,
        toolName,
        capability: 'filesystem.write',
        targetId: `ssh-node:${resolved.nodeId}`,
        summary: `Dry run: would write ${diffSummary.afterSize} bytes to ${path}`,
        output: JSON.stringify({ path, dryRun: true, diffSummary }, null, 2),
        data: {
          path,
          size: diffSummary.afterSize,
          contentHash: afterHash,
          encoding: encoding ?? 'utf-8',
          dryRun: true,
          diffSummary,
        },
        durationMs: Date.now() - startTime,
      });
    }

    const result = await nodeSftpWrite(resolved.nodeId, path, nextContent, encoding);
    const output = JSON.stringify({ path, size: result.size, mtime: result.mtime, encoding_used: result.encodingUsed, atomic_write: result.atomicWrite, contentHash: afterHash }, null, 2);
    const warnings = request.expectedHash || request.createOnly ? [] : [UNCONDITIONAL_OVERWRITE_WARNING];
    return envelopeResult<FileWriteData>(toolCallId, {
      ok: true,
      toolName,
      capability: 'filesystem.write',
      targetId: `ssh-node:${resolved.nodeId}`,
      summary: `Written ${result.size ?? byteLengthOfText(nextContent)} bytes to ${path} (hash: ${afterHash})`,
      output,
      data: {
        path,
        size: result.size,
        mtime: result.mtime,
        contentHash: afterHash,
        encoding: result.encodingUsed,
        atomic: result.atomicWrite,
        diffSummary,
      },
      warnings,
      durationMs: Date.now() - startTime,
    });
  } catch (e) {
    return { toolCallId, toolName, success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
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

async function execIdeGetFileContent(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
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
  const contentHash = await hashTextContent(tab.content);
  const output = JSON.stringify({
    path: tab.path,
    language: tab.language,
    is_dirty: tab.isDirty,
    cursor: tab.cursor ?? null,
    contentHash,
    size: byteLengthOfText(tab.content),
    content: text,
  }, null, 2);
  return envelopeResult<FileReadData>(toolCallId, {
    ok: true,
    toolName: 'ide_get_file_content',
    capability: 'filesystem.read',
    summary: `Read ${tab.path} (${byteLengthOfText(tab.content)} bytes, hash: ${contentHash})`,
    output,
    data: {
      path: tab.path,
      content: text,
      encoding: 'utf-8',
      size: byteLengthOfText(tab.content),
      contentHash,
      ...(text !== tab.content ? { truncated: true } : {}),
    },
    truncated: text !== tab.content,
    durationMs: Date.now() - startTime,
  });
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

async function execIdeReplaceString(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'ide_replace_string';
  const tabId = typeof args.tab_id === 'string' ? args.tab_id : '';
  const oldStr = typeof args.old_string === 'string' ? args.old_string : '';
  const newStr = typeof args.new_string === 'string' ? args.new_string : '';
  const shouldSave = args.save === true;
  const expectedHash = typeof args.expectedHash === 'string'
    ? args.expectedHash
    : typeof args.expected_hash === 'string'
      ? args.expected_hash
      : '';
  const dryRun = args.dryRun === true || args.dry_run === true;

  if (!tabId) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: tab_id', durationMs: Date.now() - startTime };
  if (!oldStr) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: old_string', durationMs: Date.now() - startTime };

  const ideStore = useIdeStore.getState();
  const beforeTab = ideStore.tabs.find(t => t.id === tabId);
  if (!beforeTab || beforeTab.content === null) {
    return { toolCallId, toolName, success: false, output: '', error: `File content not yet loaded for tab: ${tabId}`, durationMs: Date.now() - startTime };
  }

  const beforeHash = await hashTextContent(beforeTab.content);
  if (expectedHash && expectedHash !== beforeHash) {
    return recoverableFileError(toolCallId, toolName, startTime, 'expected_hash_mismatch', `File changed before replacing: expected hash ${expectedHash}, current hash ${beforeHash}.`);
  }

  const matchCount = beforeTab.content.split(oldStr).length - 1;
  if (matchCount !== 1) {
    return recoverableFileError(
      toolCallId,
      toolName,
      startTime,
      matchCount === 0 ? 'replace_string_not_found' : 'replace_string_not_unique',
      matchCount === 0
        ? 'Replacement target was not found.'
        : `Replacement target is not unique (${matchCount} matches). Include more surrounding context in old_string.`,
    );
  }

  const afterContent = beforeTab.content.replace(oldStr, newStr);
  const afterHash = await hashTextContent(afterContent);
  const diffSummary = buildFileDiffSummary({
    beforeContent: beforeTab.content,
    beforeHash,
    afterContent,
    afterHash,
  });

  if (dryRun) {
    return envelopeResult<FileWriteData>(toolCallId, {
      ok: true,
      toolName,
      capability: 'filesystem.write',
      summary: `Dry run: would replace text in ${beforeTab.path}`,
      output: JSON.stringify({ tab_id: tabId, path: beforeTab.path, dryRun: true, diffSummary }, null, 2),
      data: {
        path: beforeTab.path,
        size: diffSummary.afterSize,
        contentHash: afterHash,
        dryRun: true,
        diffSummary,
      },
      durationMs: Date.now() - startTime,
    });
  }

  const result = ideStore.replaceStringInTab(tabId, oldStr, newStr);
  if (!result.success) {
    return { toolCallId, toolName, success: false, output: '', error: result.error ?? 'Replace failed', durationMs: Date.now() - startTime };
  }

  try {
    if (shouldSave) await ideStore.saveFile(tabId);
  } catch (e) {
    return { toolCallId, toolName, success: true, output: `String replaced successfully but save failed: ${e instanceof Error ? e.message : String(e)}`, durationMs: Date.now() - startTime };
  }

  const tab = useIdeStore.getState().tabs.find(t => t.id === tabId);
  return envelopeResult<FileWriteData>(toolCallId, {
    ok: true,
    toolName,
    capability: 'filesystem.write',
    summary: `Replaced in ${tab?.name ?? tabId}${shouldSave ? ' (saved)' : ' (unsaved)'}`,
    output: `Replaced in ${tab?.name ?? tabId}${shouldSave ? ' (saved)' : ' (unsaved)'}`,
    data: {
      path: tab?.path ?? beforeTab.path,
      size: diffSummary.afterSize,
      contentHash: afterHash,
      diffSummary,
    },
    warnings: expectedHash ? [] : [UNCONDITIONAL_OVERWRITE_WARNING],
    durationMs: Date.now() - startTime,
  });
}

async function execIdeInsertText(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'ide_insert_text';
  const tabId = typeof args.tab_id === 'string' ? args.tab_id : '';
  const line = typeof args.line === 'number' ? args.line : 0;
  const text = typeof args.text === 'string' ? args.text : '';
  const shouldSave = args.save === true;

  if (!tabId) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: tab_id', durationMs: Date.now() - startTime };
  if (!line) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: line', durationMs: Date.now() - startTime };
  if (!text) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: text', durationMs: Date.now() - startTime };

  const ideStore = useIdeStore.getState();
  const result = ideStore.insertTextInTab(tabId, line, text);
  if (!result.success) {
    return { toolCallId, toolName, success: false, output: '', error: result.error ?? 'Insert failed', durationMs: Date.now() - startTime };
  }

  try {
    if (shouldSave) await ideStore.saveFile(tabId);
  } catch (e) {
    return { toolCallId, toolName, success: true, output: `Text inserted at line ${result.insertedAtLine} but save failed: ${e instanceof Error ? e.message : String(e)}`, durationMs: Date.now() - startTime };
  }

  const tab = useIdeStore.getState().tabs.find(t => t.id === tabId);
  return { toolCallId, toolName, success: true, output: `Inserted ${text.split('\n').length} line(s) at line ${result.insertedAtLine} in ${tab?.name ?? tabId}${shouldSave ? ' (saved)' : ' (unsaved)'}`, durationMs: Date.now() - startTime };
}

async function execIdeOpenFile(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'ide_open_file';
  const path = typeof args.path === 'string' ? args.path.trim() : '';

  if (!path) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };

  const ideStore = useIdeStore.getState();
  if (!ideStore.nodeId) {
    return { toolCallId, toolName, success: false, output: '', error: 'No IDE project is open. Open an IDE tab first.', durationMs: Date.now() - startTime };
  }

  try {
    await ideStore.openFile(path);
    const tab = useIdeStore.getState().tabs.find(t => t.path === path);
    if (!tab) {
      return { toolCallId, toolName, success: false, output: '', error: 'File opened but tab not found (may be binary or too large)', durationMs: Date.now() - startTime };
    }
    const lineCount = tab.content?.split('\n').length ?? 0;
    return { toolCallId, toolName, success: true, output: JSON.stringify({ tab_id: tab.id, path: tab.path, name: tab.name, language: tab.language, lines: lineCount }, null, 2), durationMs: Date.now() - startTime };
  } catch (e) {
    return { toolCallId, toolName, success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
  }
}

async function execIdeCreateFile(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const toolName = 'ide_create_file';
  const fullPath = typeof args.path === 'string' ? args.path.trim() : '';
  const content = typeof args.content === 'string' ? args.content : '';

  if (!fullPath) return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: path', durationMs: Date.now() - startTime };

  const ideStore = useIdeStore.getState();
  if (!ideStore.nodeId) {
    return { toolCallId, toolName, success: false, output: '', error: 'No IDE project is open. Open an IDE tab first.', durationMs: Date.now() - startTime };
  }

  try {
    // Split path into parent + name
    const lastSlash = fullPath.lastIndexOf('/');
    const parentPath = lastSlash > 0 ? fullPath.substring(0, lastSlash) : '/';
    const name = fullPath.substring(lastSlash + 1);

    if (!name) return { toolCallId, toolName, success: false, output: '', error: 'Invalid path: no filename', durationMs: Date.now() - startTime };

    await ideStore.createFile(parentPath, name);
    let warning: string | null = null;

    // If content was provided, write it into the new tab
    if (content) {
      try {
        await ideStore.openFile(fullPath);
        const tab = useIdeStore.getState().tabs.find(t => t.path === fullPath);
        if (!tab) {
          warning = 'File was created, but the IDE tab could not be opened automatically.';
        } else {
          useIdeStore.setState(state => ({
            tabs: state.tabs.map(t =>
              t.id === tab.id
                ? { ...t, content, isDirty: true, contentVersion: t.contentVersion + 1 }
                : t
            ),
          }));
          await useIdeStore.getState().saveFile(tab.id);
        }
      } catch (e) {
        warning = `File was created, but initial content setup failed: ${e instanceof Error ? e.message : String(e)}`;
      }
    }

    const tab = useIdeStore.getState().tabs.find(t => t.path === fullPath);
    return {
      toolCallId,
      toolName,
      success: true,
      output: JSON.stringify({ tab_id: tab?.id ?? null, path: fullPath, name, ...(warning ? { warning } : {}) }, null, 2),
      durationMs: Date.now() - startTime,
    };
  } catch (e) {
    return { toolCallId, toolName, success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
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

async function execLocalExec(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
  dangerousCommandApproved: boolean,
): Promise<AiToolResult> {
  const command = args.command as string | undefined;
  if (!command) {
    return { toolCallId, toolName: 'local_exec', success: false, output: '', error: 'Missing required argument: command', durationMs: Date.now() - startTime };
  }

  try {
    const savedConnectionGuardrail = await detectSavedConnectionSshMisuse(command, startTime, toolCallId, 'local_exec');
    if (savedConnectionGuardrail) {
      return savedConnectionGuardrail;
    }

    const result = await api.localExecCommand(
      command,
      args.cwd as string | undefined,
      args.timeout_secs as number | undefined,
      dangerousCommandApproved,
    );

    if (result.timedOut) {
      return envelopeResult(toolCallId, {
        ok: false,
        toolName: 'local_exec',
        summary: 'Local command timed out.',
        output: result.stderr || 'Command timed out',
        data: { exitCode: result.exitCode ?? null, timedOut: true },
        execution: createExecutionSummary({
          kind: 'command',
          command,
          cwd: args.cwd as string | undefined,
          target: { id: 'local-shell:default', kind: 'local-shell', label: 'Local shell' },
          exitCode: result.exitCode ?? null,
          timedOut: true,
          truncated: false,
          stderr: result.stderr,
          errorMessage: 'Command timed out',
        }),
        capability: 'command.run',
        targetId: 'local-shell:default',
        durationMs: Date.now() - startTime,
        targets: [{ id: 'local-shell:default', kind: 'local-shell', label: 'Local shell' }],
        error: { code: 'local_command_timeout', message: 'Command timed out', recoverable: true },
        recoverable: true,
      });
    }

    const parts: string[] = [];
    if (result.stdout) parts.push(result.stdout);
    if (result.stderr) parts.push(`[stderr]\n${result.stderr}`);
    parts.push(`[exit_code: ${result.exitCode ?? 'unknown'}]`);

    return envelopeResult(toolCallId, {
      ok: result.exitCode === 0,
      toolName: 'local_exec',
      summary: result.exitCode === 0 ? 'Local command completed.' : `Local command exited with ${result.exitCode ?? 'unknown'}.`,
      output: parts.join('\n'),
      data: { exitCode: result.exitCode ?? null },
      execution: createExecutionSummary({
        kind: 'command',
        command,
        cwd: args.cwd as string | undefined,
        target: { id: 'local-shell:default', kind: 'local-shell', label: 'Local shell' },
        exitCode: result.exitCode ?? null,
        timedOut: false,
        truncated: false,
        stderr: result.stderr,
        errorMessage: result.exitCode === 0 ? undefined : `Exit code: ${result.exitCode ?? 'unknown'}`,
      }),
      capability: 'command.run',
      targetId: 'local-shell:default',
      durationMs: Date.now() - startTime,
      targets: [{ id: 'local-shell:default', kind: 'local-shell', label: 'Local shell' }],
      ...(result.exitCode === 0 ? {} : {
        error: {
          code: 'local_command_failed',
          message: `Exit code: ${result.exitCode ?? 'unknown'}`,
          recoverable: true,
        },
        recoverable: true,
      }),
    });
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

async function execOpenLocalTerminal(args: Record<string, unknown>, startTime: number, toolCallId: string, skipFocus?: boolean): Promise<AiToolResult> {
  try {
    const terminals = useLocalTerminalStore.getState().terminals;
    if (terminals.size >= 10) {
      return { toolCallId, toolName: 'open_local_terminal', success: false, output: '', error: 'Too many local terminals open (max 10). Close some before opening new ones.', durationMs: Date.now() - startTime };
    }
    const cwd = typeof args.cwd === 'string' ? args.cwd : undefined;
    const info = await useLocalTerminalStore.getState().createTerminal(
      cwd ? { cwd } : undefined,
    );
    useAppStore.getState().createTab('local_terminal', info.id, skipFocus ? { skipFocus } : undefined);
    const readyResult = skipFocus
      ? { ready: false as const }
      : await waitForTerminalReady(info.id, { timeoutMs: TERMINAL_READY_TIMEOUT_MS });
    const readinessText = readyResult.ready
      ? 'Terminal is ready for terminal_exec.'
      : 'Terminal is opening; retry terminal_exec after it becomes visible.';
    return {
      toolCallId,
      toolName: 'open_local_terminal',
      success: true,
      output: `Local terminal opened. Session ID: ${info.id}, Shell: ${info.shell?.label ?? 'unknown'}\n${readinessText}`,
      durationMs: Date.now() - startTime,
    };
  } catch (e) {
    return {
      toolCallId,
      toolName: 'open_local_terminal',
      success: false,
      output: '',
      error: e instanceof Error ? e.message : String(e),
      durationMs: Date.now() - startTime,
    };
  }
}

const ALLOWED_SINGLETON_TABS = new Set([
  'settings', 'connection_monitor', 'connection_pool', 'topology',
  'file_manager', 'session_manager', 'plugin_manager', 'launcher',
]);

function execOpenTab(args: Record<string, unknown>, startTime: number, toolCallId: string, skipFocus?: boolean): AiToolResult {
  const tabType = typeof args.tab_type === 'string' ? args.tab_type.trim() : '';
  if (!tabType || !ALLOWED_SINGLETON_TABS.has(tabType)) {
    return { toolCallId, toolName: 'open_tab', success: false, output: '', error: `Invalid tab_type. Allowed: ${[...ALLOWED_SINGLETON_TABS].join(', ')}`, durationMs: Date.now() - startTime };
  }
  useAppStore.getState().createTab(tabType as TabType, undefined, skipFocus ? { skipFocus } : undefined);
  return envelopeResult(toolCallId, {
    ok: true,
    toolName: 'open_tab',
    summary: `Opened ${tabType} tab.`,
    output: `Opened ${tabType} tab.`,
    capability: 'navigation.open',
    durationMs: Date.now() - startTime,
    targets: [{ id: `tab:${tabType}`, kind: 'app-tab', label: tabType }],
    nextActions: tabType === 'settings'
      ? [
          { tool: 'get_settings', reason: 'Read the current settings before making a change.', priority: 'recommended' },
          { tool: 'update_setting', reason: 'Apply the requested setting change after identifying the correct section and key.', priority: 'optional' },
        ]
      : undefined,
  });
}

const ALLOWED_SESSION_TABS = new Set(['sftp', 'ide', 'forwards']);

function execOpenSessionTab(args: Record<string, unknown>, startTime: number, toolCallId: string, skipFocus?: boolean): AiToolResult {
  const tabType = typeof args.tab_type === 'string' ? args.tab_type.trim() : '';
  const nodeId = typeof args.node_id === 'string' ? args.node_id.trim() : '';
  if (!tabType || !ALLOWED_SESSION_TABS.has(tabType)) {
    return { toolCallId, toolName: 'open_session_tab', success: false, output: '', error: `Invalid tab_type. Allowed: ${[...ALLOWED_SESSION_TABS].join(', ')}`, durationMs: Date.now() - startTime };
  }
  if (!nodeId) {
    return { toolCallId, toolName: 'open_session_tab', success: false, output: '', error: 'Missing required argument: node_id. Use list_sessions to discover available nodes.', durationMs: Date.now() - startTime };
  }
  // Resolve the node to get its terminal session ID
  const node = useSessionTreeStore.getState().nodes.find(n => n.id === nodeId);
  if (!node) {
    return { toolCallId, toolName: 'open_session_tab', success: false, output: '', error: `Node not found: ${nodeId}`, durationMs: Date.now() - startTime };
  }
  const status = node.runtime?.status;
  if (status !== 'connected' && status !== 'active') {
    return { toolCallId, toolName: 'open_session_tab', success: false, output: '', error: `Node ${nodeId} is not connected (status: ${status ?? 'unknown'}). Wait for it to connect first.`, durationMs: Date.now() - startTime };
  }
  const terminalId = node.runtime?.terminalIds?.[0];
  if (!terminalId) {
    return { toolCallId, toolName: 'open_session_tab', success: false, output: '', error: `Node ${nodeId} has no active terminal session. Is it connected?`, durationMs: Date.now() - startTime };
  }
  useAppStore.getState().createTab(tabType as TabType, terminalId, { nodeId, ...(skipFocus ? { skipFocus } : {}) });
  return envelopeResult(toolCallId, {
    ok: true,
    toolName: 'open_session_tab',
    summary: `Opened ${tabType} tab for node ${nodeId}.`,
    output: `Opened ${tabType} tab for node ${nodeId}.`,
    capability: 'navigation.open',
    targetId: nodeId,
    durationMs: Date.now() - startTime,
    targets: [{ id: nodeId, kind: 'ssh-node', label: node.host ?? nodeId, metadata: { tabType, terminalId } }],
  });
}

const SETTINGS_SECTION_KEYS: Record<string, string[]> = {
  terminal: ['fontFamily', 'fontSize', 'renderer', 'terminalEncoding', 'scrollback'],
  appearance: ['theme', 'backgroundImage', 'opacity', 'fontScale'],
  connectionDefaults: ['terminalEncoding', 'keepaliveInterval', 'connectTimeout'],
  sftp: ['maxConcurrentTransfers', 'speedLimitKbps', 'directoryParallelism'],
  ide: ['fontSize', 'tabSize', 'wordWrap'],
  reconnect: ['enabled', 'maxAttempts', 'initialDelayMs'],
  general: ['language', 'telemetry', 'startOnBoot'],
  ai: ['providers', 'modelContextWindows', 'userContextWindows', 'reasoningSettings'],
  localTerminal: ['defaultShellId', 'defaultCwd', 'loadShellProfile', 'customEnvVars'],
};

function execOpenSettingsSection(args: Record<string, unknown>, startTime: number, toolCallId: string, skipFocus?: boolean): AiToolResult {
  const section = typeof args.section === 'string' ? args.section.trim() : '';
  if (!section || !SETTINGS_SECTION_KEYS[section]) {
    return {
      toolCallId,
      toolName: 'open_settings_section',
      success: false,
      output: '',
      error: `Invalid settings section. Allowed: ${Object.keys(SETTINGS_SECTION_KEYS).join(', ')}`,
      durationMs: Date.now() - startTime,
    };
  }

  useAppStore.getState().createTab('settings', undefined, skipFocus ? { skipFocus } : undefined);
  if (!skipFocus && typeof window !== 'undefined') {
    window.setTimeout(() => {
      window.dispatchEvent(new CustomEvent('oxideterm:open-settings-tab', { detail: { tab: section } }));
    }, 0);
  }
  const keys = SETTINGS_SECTION_KEYS[section];
  const output = `Opened settings section: ${section}\nCommon keys: ${keys.join(', ')}\nUse get_settings with section="${section}" before update_setting if you need the current value.`;
  return envelopeResult(toolCallId, {
    ok: true,
    toolName: 'open_settings_section',
    summary: `Opened settings section: ${section}`,
    output,
    capability: 'navigation.open',
    durationMs: Date.now() - startTime,
    data: { section, commonKeys: keys },
    targets: [{ id: `settings:${section}`, kind: 'app-tab', label: `Settings: ${section}` }],
    nextActions: [
      { tool: 'get_settings', args: { section }, reason: 'Read the current section before modifying a setting.', priority: 'recommended' },
      { tool: 'update_setting', reason: 'Apply the requested setting change once the section/key/value are clear.', priority: 'optional' },
    ],
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// Settings Tool Executors
// ═══════════════════════════════════════════════════════════════════════════

function execGetSettings(args: Record<string, unknown>, startTime: number, toolCallId: string): AiToolResult {
  const section = args.section as string | undefined;
  const settings = useSettingsStore.getState().settings;

  const REDACTED_VALUE = '[redacted]';

  const redactEnvValues = (value: unknown): Record<string, string> | undefined => {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return undefined;
    const entries = Object.keys(value as Record<string, unknown>)
      .filter((key) => key.trim().length > 0)
      .sort()
      .map((key) => [key, REDACTED_VALUE] as const);
    return Object.fromEntries(entries);
  };

  const redactSensitiveArgs = (value: unknown): string[] | undefined => {
    if (!Array.isArray(value)) return undefined;
    const redacted: string[] = [];
    let redactNext = false;

    for (const entry of value) {
      const arg = typeof entry === 'string' ? entry : String(entry);

      if (redactNext) {
        redacted.push(REDACTED_VALUE);
        redactNext = false;
        continue;
      }

      const trimmed = arg.trim();
      const lower = trimmed.toLowerCase();
      const isSensitiveFlag = /^--?(?:api[-_]?key|auth(?:orization)?|bearer|password|secret|token)\b/.test(lower);

      if (isSensitiveFlag && trimmed.includes('=')) {
        redacted.push(trimmed.replace(/=.*/, `=${REDACTED_VALUE}`));
        continue;
      }

      if (isSensitiveFlag) {
        redacted.push(trimmed);
        redactNext = true;
        continue;
      }

      redacted.push(trimmed);
    }

    return redacted;
  };

  const sanitizeAiSettings = (value: unknown): unknown => {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return value;
    const raw = value as Record<string, unknown>;
    const providers = Array.isArray(raw.providers)
      ? (raw.providers as Array<Record<string, unknown>>).map((provider) => ({
          id: provider.id,
          name: provider.name,
          type: provider.type,
          enabled: provider.enabled,
        }))
      : raw.providers;

    const mcpServers = Array.isArray(raw.mcpServers)
      ? (raw.mcpServers as Array<Record<string, unknown>>).map((server) => {
          const env = redactEnvValues(server.env);
          const headers = redactEnvValues(server.headers);
          const args = redactSensitiveArgs(server.args);
          return {
            id: server.id,
            name: server.name,
            transport: server.transport,
            url: server.url,
            command: server.command,
            authHeaderName: server.authHeaderName,
            authHeaderMode: server.authHeaderMode,
            ...(args !== undefined ? { args } : {}),
            enabled: server.enabled,
            retryOnDisconnect: server.retryOnDisconnect,
            ...(env !== undefined ? { env } : {}),
            ...(headers !== undefined ? { headers } : {}),
            ...(typeof server.authToken === 'string' && server.authToken.length > 0
              ? { hasLegacyAuthToken: true }
              : {}),
          };
        })
      : raw.mcpServers;

    return {
      ...raw,
      providers,
      ...(mcpServers !== undefined ? { mcpServers } : {}),
    };
  };

  const sanitizeLocalTerminalSettings = (value: unknown): unknown => {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return value;
    const raw = value as Record<string, unknown>;
    const customEnvVars = redactEnvValues(raw.customEnvVars);
    return {
      ...raw,
      ...(customEnvVars ? { customEnvVars } : {}),
    };
  };

  const sanitizeSection = (sectionName: string, value: unknown): unknown => {
    if (sectionName === 'ai') return sanitizeAiSettings(value);
    if (sectionName === 'localTerminal') return sanitizeLocalTerminalSettings(value);
    return value;
  };

  if (section) {
    const sectionData = (settings as unknown as Record<string, unknown>)[section];
    if (sectionData === undefined) {
      return { toolCallId, toolName: 'get_settings', success: false, output: '', error: `Unknown settings section: ${section}`, durationMs: Date.now() - startTime };
    }
    const safe = sanitizeSection(section, sectionData);
    return envelopeResult(toolCallId, {
      ok: true,
      toolName: 'get_settings',
      summary: `Read settings section: ${section}`,
      output: JSON.stringify(safe, null, 2),
      data: { section, settings: safe },
      capability: 'settings.read',
      durationMs: Date.now() - startTime,
      nextActions: [
        { tool: 'update_setting', reason: `Modify ${section} after confirming the exact key and requested value.`, priority: 'optional' },
      ],
    });
  }

  // Sanitize the full settings object before exposing it to AI tools.
  const safeSettings = { ...settings as unknown as Record<string, unknown> };
  if (safeSettings.ai) safeSettings.ai = sanitizeAiSettings(safeSettings.ai);
  if (safeSettings.localTerminal) safeSettings.localTerminal = sanitizeLocalTerminalSettings(safeSettings.localTerminal);
  return envelopeResult(toolCallId, {
    ok: true,
    toolName: 'get_settings',
    summary: 'Read application settings',
    output: JSON.stringify(safeSettings, null, 2),
    data: { settings: safeSettings },
    capability: 'settings.read',
    durationMs: Date.now() - startTime,
    nextActions: [
      { tool: 'update_setting', reason: 'Apply the requested setting change after identifying the correct section/key/value.', priority: 'optional' },
    ],
  });
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
    (store[updateMethod] as (key: string, value: unknown) => void)(key, value);
    return envelopeResult(toolCallId, {
      ok: true,
      toolName: 'update_setting',
      summary: `Updated ${section}.${key}`,
      output: `Updated ${section}.${key}`,
      data: { section, key, value },
      capability: 'settings.write',
      durationMs: Date.now() - startTime,
      nextActions: [
        { tool: 'get_settings', args: { section }, reason: 'Verify the setting after the update.', priority: 'optional' },
      ],
    });
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
    if (args.keepalive_interval_secs !== undefined) {
      return {
        toolCallId,
        toolName: 'set_pool_config',
        success: false,
        output: '',
        error: 'keepalive_interval_secs is not supported by the current connection pool backend.',
        durationMs: Date.now() - startTime,
      };
    }

    // Build a full config object, using defaults for missing fields
    const idleTimeout = typeof args.idle_timeout_secs === 'number' ? args.idle_timeout_secs : 300;
    const maxConns = typeof args.max_connections === 'number' ? Math.max(1, Math.min(100, args.max_connections as number)) : 10;

    const config: import('../../../types').ConnectionPoolConfig = {
      idleTimeoutSecs: idleTimeout,
      maxConnections: maxConns,
      protectOnExit: true,
    };

    await api.sshSetPoolConfig(config);
    const changed = Object.entries(args).filter(([k]) => ['idle_timeout_secs', 'max_connections'].includes(k)).map(([k]) => k);
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
    return envelopeResult(toolCallId, {
      ok: true,
      toolName: 'list_saved_connections',
      summary: `Found ${safe.length} saved connection${safe.length === 1 ? '' : 's'}.`,
      output: JSON.stringify(safe, null, 2),
      data: { connections: safe },
      capability: 'state.list',
      durationMs: Date.now() - startTime,
      targets: safe.map((connection) => ({
        id: `saved-connection:${connection.id}`,
        kind: 'saved-connection',
        label: `${connection.name || connection.host} (${connection.username}@${connection.host}:${connection.port})`,
        metadata: connection,
      })),
      nextActions: [
        { tool: 'connect_saved_session', reason: 'Connect using a specific saved connection ID.', priority: 'optional' },
        { tool: 'search_saved_connections', reason: 'Narrow the list if the requested host is ambiguous.', priority: 'optional' },
      ],
    });
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
    const nextActions = safe.length === 1
      ? [{ tool: 'connect_saved_session', args: { connection_id: safe[0].id }, reason: 'Exactly one saved connection matched the query.', priority: 'recommended' as const }]
      : safe.length > 1
        ? [{ tool: 'connect_saved_session', reason: 'Choose one candidate and connect with its connection_id.', priority: 'recommended' as const }]
        : [{ tool: 'list_saved_connections', reason: 'No saved connection matched; inspect all saved connections before trying manual SSH.', priority: 'fallback' as const }];
    return envelopeResult(toolCallId, {
      ok: true,
      toolName: 'search_saved_connections',
      summary: `Found ${safe.length} saved connection match${safe.length === 1 ? '' : 'es'} for "${query}".`,
      output: JSON.stringify(safe, null, 2),
      data: { query, connections: safe },
      capability: 'state.list',
      durationMs: Date.now() - startTime,
      targets: safe.map((connection) => ({
        id: `saved-connection:${connection.id}`,
        kind: 'saved-connection',
        label: `${connection.name || connection.host} (${connection.username}@${connection.host}:${connection.port})`,
        metadata: connection,
      })),
      nextActions,
      ...(safe.length > 1
        ? {
            disambiguation: {
              prompt: 'Multiple saved connections matched. Choose the intended connection before connecting.',
              options: safe.map((connection) => ({
                id: connection.id,
                label: `${connection.name || connection.host} — ${connection.username}@${connection.host}:${connection.port}`,
                args: { connection_id: connection.id },
              })),
            },
          }
        : {}),
    });
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

async function execConnectSavedSession(args: Record<string, unknown>, startTime: number, toolCallId: string, skipFocus?: boolean): Promise<AiToolResult> {
  const toolName = 'connect_saved_session';
  const connectionId = typeof args.connection_id === 'string' ? args.connection_id.trim() : '';
  if (!connectionId) {
    return { toolCallId, toolName, success: false, output: '', error: 'Missing required argument: connection_id. Use list_saved_connections to find available IDs.', durationMs: Date.now() - startTime };
  }

  try {
    const { connectToSaved } = await import('@/lib/connectToSaved');

    // Track what was opened and any errors
    let connectError: string | null = null;
    const createTab = (_type: 'terminal', sessionId: string) => {
      useAppStore.getState().createTab('terminal', sessionId, skipFocus ? { skipFocus } : undefined);
    };

    const connectPromise = connectToSaved(connectionId, {
      createTab,
      toast: () => {}, // No-op: AI context doesn't need toasts
      t: (key: string) => key, // Pass-through: not displayed to user
      onError: (connId, reason) => {
        if (reason === 'missing-password') {
          connectError = `Connection ${connId} requires a password prompt and cannot be completed non-interactively.`;
          return;
        }
        connectError = `Connection failed for ${connId}`;
      },
    });
    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('Connection timed out after 90 seconds')), 90_000)
    );
    const connectResult = await Promise.race([connectPromise, timeout]);

    if (connectError) {
      return { toolCallId, toolName, success: false, output: '', error: connectError, durationMs: Date.now() - startTime };
    }

    if (!connectResult) {
      return { toolCallId, toolName, success: false, output: '', error: 'Connection did not produce a terminal session.', durationMs: Date.now() - startTime };
    }

    const connectedNode = useSessionTreeStore.getState().getNode(connectResult.nodeId);

    const info: Record<string, unknown> = {
      connection_id: connectionId,
      session_id: connectResult.sessionId,
      node_id: connectResult.nodeId,
    };
    if (connectedNode) {
      info.host = connectedNode.host;
      info.port = connectedNode.port;
      info.username = connectedNode.username;
      info.status = connectedNode.runtime?.status;
    }

    return envelopeResult(toolCallId, {
      ok: true,
      toolName,
      summary: 'SSH connection established and terminal opened.',
      output: `SSH connection established and terminal opened.\n${JSON.stringify(info, null, 2)}`,
      data: info,
      capability: 'navigation.open',
      targetId: connectResult.nodeId,
      durationMs: Date.now() - startTime,
      targets: [
        {
          id: connectResult.nodeId,
          kind: 'ssh-node',
          label: typeof info.host === 'string' ? info.host : connectResult.nodeId,
          metadata: { ...info, sessionId: connectResult.sessionId },
        },
        {
          id: connectResult.sessionId,
          kind: 'terminal-session',
          label: `Terminal ${connectResult.sessionId}`,
          metadata: { nodeId: connectResult.nodeId, sessionId: connectResult.sessionId },
        },
      ],
      nextActions: [
        { tool: 'terminal_exec', args: { target_id: `ssh-node:${connectResult.nodeId}` }, reason: 'Run a non-interactive command on the connected SSH node.', priority: 'optional' },
        { tool: 'open_session_tab', args: { node_id: connectResult.nodeId, tab_type: 'sftp' }, reason: 'Open SFTP for remote file operations on this connection.', priority: 'optional' },
      ],
    });
  } catch (e) {
    const errorMsg = e instanceof Error ? e.message : String(e);
    // Provide actionable error messages
    if (errorMsg.includes('not found') || errorMsg.includes('No connection')) {
      return { toolCallId, toolName, success: false, output: '', error: `Saved connection not found: ${connectionId}. Use list_saved_connections to see available connections.`, durationMs: Date.now() - startTime };
    }
    if (errorMsg.includes('authentication') || errorMsg.includes('Auth')) {
      return { toolCallId, toolName, success: false, output: '', error: `Authentication failed for connection ${connectionId}. The user may need to update credentials in the connection settings.`, durationMs: Date.now() - startTime };
    }
    return { toolCallId, toolName, success: false, output: '', error: errorMsg, durationMs: Date.now() - startTime };
  }
}

async function execConnectSavedConnectionByQuery(args: Record<string, unknown>, startTime: number, toolCallId: string, skipFocus?: boolean): Promise<AiToolResult> {
  const toolName = 'connect_saved_connection_by_query';
  const query = typeof args.query === 'string' ? args.query.trim() : '';
  const explicitConnectionId = typeof args.connection_id === 'string' ? args.connection_id.trim() : '';

  if (explicitConnectionId) {
    const result = await execConnectSavedSession({ connection_id: explicitConnectionId }, startTime, toolCallId, skipFocus);
    return { ...result, toolName };
  }

  if (!query) {
    return {
      toolCallId,
      toolName,
      success: false,
      output: '',
      error: 'Missing required argument: query',
      durationMs: Date.now() - startTime,
    };
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

    if (safe.length === 1) {
      const result = await execConnectSavedSession({ connection_id: safe[0].id }, startTime, toolCallId, skipFocus);
      return {
        ...result,
        toolName,
        output: `Matched saved connection "${safe[0].name || safe[0].host}" and connected.\n${result.output}`,
        envelope: result.envelope
          ? {
              ...result.envelope,
              summary: `Matched saved connection "${safe[0].name || safe[0].host}" and connected.`,
              meta: { ...result.envelope.meta, toolName },
            }
          : result.envelope,
      };
    }

    if (safe.length === 0) {
      return envelopeResult(toolCallId, {
        ok: false,
        toolName,
        summary: `No saved connection matched "${query}".`,
        output: `No saved connection matched "${query}". Do not fall back to manual ssh unless the user asks for it; inspect saved connections or ask for clarification.`,
        data: { query, connections: safe },
        error: {
          code: 'saved_connection_not_found',
          message: `No saved connection matched "${query}".`,
          recoverable: true,
        },
        recoverable: true,
        durationMs: Date.now() - startTime,
        nextActions: [
          { tool: 'list_saved_connections', reason: 'Inspect all saved connections before trying a manual SSH command.', priority: 'recommended' },
        ],
      });
    }

    return envelopeResult(toolCallId, {
      ok: true,
      toolName,
      summary: `Multiple saved connections matched "${query}".`,
      output: JSON.stringify(safe, null, 2),
      data: { query, connections: safe },
      capability: 'state.list',
      durationMs: Date.now() - startTime,
      targets: safe.map((connection) => ({
        id: `saved-connection:${connection.id}`,
        kind: 'saved-connection',
        label: `${connection.name || connection.host} (${connection.username}@${connection.host}:${connection.port})`,
        metadata: connection,
      })),
      disambiguation: {
        prompt: 'Multiple saved connections matched. Choose one connection_id before connecting.',
        options: safe.map((connection) => ({
          id: connection.id,
          label: `${connection.name || connection.host} — ${connection.username}@${connection.host}:${connection.port}`,
          args: { query, connection_id: connection.id },
        })),
      },
      nextActions: [
        { tool: 'connect_saved_connection_by_query', reason: 'Retry with the selected connection_id from the disambiguation options.', priority: 'recommended' },
      ],
    });
  } catch (e) {
    return { toolCallId, toolName, success: false, output: '', error: e instanceof Error ? e.message : String(e), durationMs: Date.now() - startTime };
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
// MCP Resource Tools
// ═══════════════════════════════════════════════════════════════════════════

async function execListMcpResources(startTime: number, toolCallId: string): Promise<AiToolResult> {
  const { useMcpRegistry } = await import('../mcp');
  const resources = useMcpRegistry.getState().getAllMcpResources();
  if (resources.length === 0) {
    return { toolCallId, toolName: 'list_mcp_resources', success: true, output: 'No MCP resources available. Either no MCP servers are connected, or none expose resources.', durationMs: Date.now() - startTime };
  }
  const lines = resources.map(r =>
    `[${r.serverName}] ${r.name} (${r.uri})${r.mimeType ? ` [${r.mimeType}]` : ''}${r.description ? ` — ${r.description}` : ''}  server_id=${r.serverId}`
  );
  return { toolCallId, toolName: 'list_mcp_resources', success: true, output: lines.join('\n'), durationMs: Date.now() - startTime };
}

async function execReadMcpResource(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): Promise<AiToolResult> {
  const serverId = String(args.server_id ?? '');
  const uri = String(args.uri ?? '');
  if (!serverId || !uri) {
    return { toolCallId, toolName: 'read_mcp_resource', success: false, output: '', error: 'Both server_id and uri are required.', durationMs: Date.now() - startTime };
  }
  const { useMcpRegistry } = await import('../mcp');
  const content = await useMcpRegistry.getState().readResource(serverId, uri);
  const text = content.text ?? (content.blob ? `[base64 binary, ${content.blob.length} chars, mime=${content.mimeType ?? 'unknown'}]` : '(empty)');
  const output = text.slice(0, MAX_OUTPUT_BYTES);
  return {
    toolCallId,
    toolName: 'read_mcp_resource',
    success: true,
    output,
    durationMs: Date.now() - startTime,
    truncated: text.length > MAX_OUTPUT_BYTES,
  };
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

  const rawText = textParts.join('\n');
  return {
    toolCallId,
    toolName,
    success: !result.isError,
    output: result.isError ? '' : output,
    error: result.isError ? (output || 'MCP tool returned an error with no message.') : undefined,
    durationMs: Date.now() - startTime,
    truncated: !result.isError && rawText.length > MAX_OUTPUT_BYTES,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Status & Observability Tools
// ═══════════════════════════════════════════════════════════════════════════

function execGetEventLog(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const state = useEventLogStore.getState();
  let entries = [...state.entries];

  // Optional severity filter
  const severity = typeof args.severity === 'string' ? args.severity : null;
  if (severity) {
    if (['info', 'warn', 'error'].includes(severity)) {
      entries = entries.filter(e => e.severity === severity);
    } else {
      return { toolCallId, toolName: 'get_event_log', success: false, output: '', error: `Invalid severity: "${severity}". Must be one of: info, warn, error.`, durationMs: Date.now() - startTime };
    }
  }

  // Optional category filter
  const category = typeof args.category === 'string' ? args.category : null;
  if (category) {
    if (['connection', 'reconnect', 'node'].includes(category)) {
      entries = entries.filter(e => e.category === category);
    } else {
      return { toolCallId, toolName: 'get_event_log', success: false, output: '', error: `Invalid category: "${category}". Must be one of: connection, reconnect, node.`, durationMs: Date.now() - startTime };
    }
  }

  // Limit (default 50, max 200)
  const limit = Math.min(Math.max(Number(args.limit) || 50, 1), 200);
  entries = entries.slice(-limit);

  if (entries.length === 0) {
    return { toolCallId, toolName: 'get_event_log', success: true, output: 'No events matching the filter criteria.', durationMs: Date.now() - startTime };
  }

  const formatted = entries.map(e => ({
    id: e.id,
    time: new Date(e.timestamp).toISOString(),
    severity: e.severity,
    category: e.category,
    nodeId: e.nodeId ?? null,
    title: e.title,
    detail: e.detail ?? null,
    source: e.source,
  }));

  const raw = JSON.stringify(formatted, null, 2);
  const { text: output, truncated } = truncateOutput(raw);
  return { toolCallId, toolName: 'get_event_log', success: true, output, durationMs: Date.now() - startTime, truncated };
}

function execGetTransferStatus(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const { transfers } = useTransferStore.getState();
  let items = Array.from(transfers.values());

  // Optional node filter
  const nodeId = typeof args.node_id === 'string' ? args.node_id.trim() : null;
  if (nodeId) {
    items = items.filter(t => t.nodeId === nodeId);
  }

  // Optional state filter
  const stateFilter = typeof args.state === 'string' ? args.state : null;
  if (stateFilter) {
    if (['pending', 'active', 'paused', 'completed', 'cancelled', 'error'].includes(stateFilter)) {
      items = items.filter(t => t.state === stateFilter);
    } else {
      return { toolCallId, toolName: 'get_transfer_status', success: false, output: '', error: `Invalid state: "${stateFilter}". Must be one of: pending, active, paused, completed, cancelled, error.`, durationMs: Date.now() - startTime };
    }
  }

  if (items.length === 0) {
    return { toolCallId, toolName: 'get_transfer_status', success: true, output: 'No transfers matching the filter criteria.', durationMs: Date.now() - startTime };
  }

  const now = Date.now();
  const formatted = items.map(t => {
    const progress = t.size > 0 ? Math.round((t.transferred / t.size) * 100) : 0;
    const elapsedMs = (t.endTime ?? now) - t.startTime;
    const elapsedSecs = Math.round(elapsedMs / 1000);
    return {
      id: t.id,
      name: t.name,
      direction: t.direction,
      size: t.size,
      transferred: t.transferred,
      progress: `${progress}%`,
      state: t.state,
      error: t.error ?? null,
      elapsedSecs,
    };
  });

  const raw = JSON.stringify(formatted, null, 2);
  const { text: output, truncated } = truncateOutput(raw);
  return { toolCallId, toolName: 'get_transfer_status', success: true, output, durationMs: Date.now() - startTime, truncated };
}

function execGetRecordingStatus(startTime: number, toolCallId: string): AiToolResult {
  const { recordings, recordingTicks } = useRecordingStore.getState();

  if (recordings.size === 0) {
    return { toolCallId, toolName: 'get_recording_status', success: true, output: 'No active recordings.', durationMs: Date.now() - startTime };
  }

  const formatted: { sessionId: string; label: string; terminalType: string; state: string; elapsedSecs: number; eventCount: number }[] = [];
  recordings.forEach((entry, sessionId) => {
    const tick = recordingTicks.get(sessionId);
    formatted.push({
      sessionId,
      label: entry.meta.label ?? sessionId,
      terminalType: entry.meta.terminalType ?? 'unknown',
      state: entry.recorder.getState(),
      elapsedSecs: tick ? Math.round(tick.elapsed / 1000) : 0,
      eventCount: tick?.eventCount ?? 0,
    });
  });

  return { toolCallId, toolName: 'get_recording_status', success: true, output: truncateOutput(JSON.stringify(formatted, null, 2)).text, durationMs: Date.now() - startTime };
}

function execGetBroadcastStatus(startTime: number, toolCallId: string): AiToolResult {
  const { enabled, targets } = useBroadcastStore.getState();
  const result = {
    enabled,
    targetCount: targets.size,
    targets: Array.from(targets),
  };
  return { toolCallId, toolName: 'get_broadcast_status', success: true, output: JSON.stringify(result, null, 2), durationMs: Date.now() - startTime };
}

function execGetPluginDetails(
  args: Record<string, unknown>,
  startTime: number,
  toolCallId: string,
): AiToolResult {
  const { plugins, pluginLogs } = usePluginStore.getState();
  const pluginId = typeof args.plugin_id === 'string' ? args.plugin_id.trim() : null;

  if (pluginId) {
    // Single plugin detail mode
    const info = plugins.get(pluginId);
    if (!info) {
      return { toolCallId, toolName: 'get_plugin_details', success: false, output: '', error: `Plugin not found: ${pluginId}`, durationMs: Date.now() - startTime };
    }
    const logs = (pluginLogs.get(pluginId) ?? []).slice(-20).map(l => ({
      time: new Date(l.timestamp).toISOString(),
      level: l.level,
      message: l.message,
    }));
    const detail = {
      id: pluginId,
      name: info.manifest?.name ?? pluginId,
      version: info.manifest?.version ?? 'unknown',
      description: info.manifest?.description ?? null,
      state: info.state,
      error: info.error ?? null,
      recentLogs: logs,
    };
    const raw = JSON.stringify(detail, null, 2);
    const { text: output, truncated } = truncateOutput(raw);
    return { toolCallId, toolName: 'get_plugin_details', success: true, output, durationMs: Date.now() - startTime, truncated };
  }

  // Summary of all plugins
  const summary: { id: string; name: string; version: string; state: string; hasError: boolean; errorCount: number }[] = [];
  plugins.forEach((p, id) => {
    const logs = pluginLogs.get(id) ?? [];
    const errorCount = logs.filter(l => l.level === 'error').length;
    summary.push({
      id,
      name: p.manifest?.name ?? id,
      version: p.manifest?.version ?? 'unknown',
      state: p.state,
      hasError: !!p.error,
      errorCount,
    });
  });

  if (summary.length === 0) {
    return { toolCallId, toolName: 'get_plugin_details', success: true, output: 'No plugins installed.', durationMs: Date.now() - startTime };
  }

  return { toolCallId, toolName: 'get_plugin_details', success: true, output: truncateOutput(JSON.stringify(summary, null, 2)).text, durationMs: Date.now() - startTime };
}

// ═══════════════════════════════════════════════════════════════════════════
// SSH Environment & Topology Tools
// ═══════════════════════════════════════════════════════════════════════════

async function execGetSshEnvironment(startTime: number, toolCallId: string): Promise<AiToolResult> {
  const timeout = new Promise<never>((_, reject) => setTimeout(() => reject(new Error('SSH environment query timed out (10s)')), 10_000));
  const [configHosts, sshKeys, agentAvailable] = await Promise.race([
    Promise.all([
      api.listSshConfigHosts(),
      api.checkSshKeys(),
      api.isAgentAvailable(),
    ]),
    timeout,
  ]);

  // Sanitize: only expose basenames of key paths, not full filesystem paths
  const result = {
    configHosts: configHosts.map(h => ({
      alias: h.alias,
      hostname: h.hostname,
      user: h.user,
      port: h.port,
      identityFile: h.identity_file ? h.identity_file.split('/').pop() ?? h.identity_file : null,
    })),
    sshKeys: sshKeys.map(k => ({
      name: k.name,
      keyType: k.key_type,
    })),
    agentAvailable,
  };

  const raw = JSON.stringify(result, null, 2);
  const { text: output, truncated } = truncateOutput(raw);
  return { toolCallId, toolName: 'get_ssh_environment', success: true, output, durationMs: Date.now() - startTime, truncated };
}

async function execGetTopology(startTime: number, toolCallId: string): Promise<AiToolResult> {
  const timeout = new Promise<never>((_, reject) => setTimeout(() => reject(new Error('Topology query timed out (10s)')), 10_000));
  const [nodes, edges] = await Promise.race([
    Promise.all([
      api.getTopologyNodes(),
      api.getTopologyEdges(),
    ]),
    timeout,
  ]);

  const result = {
    nodes: nodes.map(n => ({
      id: n.id,
      displayName: n.displayName ?? null,
      host: n.host,
      port: n.port,
      username: n.username,
      isLocal: n.isLocal,
      tags: n.tags ?? [],
    })),
    edges: edges.map(e => ({
      from: e.from,
      to: e.to,
      cost: e.cost,
    })),
  };

  if (result.nodes.length === 0) {
    return { toolCallId, toolName: 'get_topology', success: true, output: 'No topology nodes. Save some SSH connections first.', durationMs: Date.now() - startTime };
  }

  const raw = JSON.stringify(result, null, 2);
  const { text: output, truncated } = truncateOutput(raw);
  return { toolCallId, toolName: 'get_topology', success: true, output, durationMs: Date.now() - startTime, truncated };
}

// ═══════════════════════════════════════════════════════════════════════════
// RAG Document Search
// ═══════════════════════════════════════════════════════════════════════════

async function execSearchDocs(args: Record<string, unknown>, startTime: number, toolCallId: string): Promise<AiToolResult> {
  const query = typeof args.query === 'string' ? args.query.trim().slice(0, 500) : '';
  if (!query) {
    return { toolCallId, toolName: 'search_docs', success: false, output: '', error: 'Missing required parameter: query', durationMs: Date.now() - startTime };
  }

  const topK = typeof args.top_k === 'number' ? Math.min(Math.max(1, Math.round(args.top_k)), 10) : 5;

  // Attempt hybrid search with embedding vector (same pattern as auto-inject RAG)
  let queryVector: number[] | undefined;
  try {
    const aiSettings = useSettingsStore.getState().settings.ai;
    const resolvedEmbedding = resolveEmbeddingProvider(aiSettings);
    if (
      resolvedEmbedding.reason === 'ready'
      && resolvedEmbedding.providerConfig
      && resolvedEmbedding.provider?.embedTexts
      && resolvedEmbedding.model
    ) {
      // search_docs may run during an AI turn. Auto-selected embeddings should
      // not unlock a secondary provider behind the user's back, but an explicit
      // embedding provider is a user choice and may load exactly that one key.
      const embeddingRequiresApiKey = requiresEmbeddingApiKey(resolvedEmbedding.providerConfig);
      const activeEmbeddingProvider = resolvedEmbedding.providerConfig.id === aiSettings.activeProviderId;
      const activeEmbeddingApiKey = embeddingRequiresApiKey && activeEmbeddingProvider
        ? await api.getAiProviderApiKey(resolvedEmbedding.providerConfig.id)
        : null;
      const keyDecision = resolveChatEmbeddingApiKey({
        embeddingProviderId: resolvedEmbedding.providerConfig.id,
        activeProviderId: aiSettings.activeProviderId,
        activeProviderApiKey: activeEmbeddingApiKey,
        embeddingRequiresApiKey,
        embeddingMode: resolvedEmbedding.mode,
      });
      let embApiKey = '';
      if (keyDecision.kind === 'use-key') {
        embApiKey = keyDecision.apiKey;
      } else if (keyDecision.kind === 'load-provider-key') {
        embApiKey = (await api.getAiProviderApiKey(keyDecision.providerId)) ?? '';
        if (!embApiKey.trim()) {
          throw new Error('Configured embedding provider key is missing');
        }
      } else if (keyDecision.kind === 'skip') {
        throw new Error('Embedding provider is not the active chat provider');
      }
      const vectors = await Promise.race([
        resolvedEmbedding.provider.embedTexts(
          {
            baseUrl: resolvedEmbedding.providerConfig.baseUrl,
            apiKey: embApiKey,
            model: resolvedEmbedding.model,
          },
          [query],
        ),
        new Promise<never>((_, reject) => setTimeout(() => reject(new Error('embed timeout')), 3000)),
      ]);
      if (vectors.length > 0) queryVector = vectors[0];
    }
  } catch {
    // Embedding failed — fall back to BM25 only
  }

  const results = await ragSearch({ query, collectionIds: [], queryVector, topK });
  if (results.length === 0) {
    return { toolCallId, toolName: 'search_docs', success: true, output: 'No matching documents found. The user may not have imported any operations documentation yet.', durationMs: Date.now() - startTime };
  }

  const formatted = results.map((r: typeof results[number], i: number) => {
    const header = `[${i + 1}] ${r.docTitle}${r.sectionPath ? ` > ${r.sectionPath}` : ''} (score: ${r.score.toFixed(3)})`;
    return `${header}\n${r.content}`;
  }).join('\n\n---\n\n');

  const { text: output, truncated } = truncateOutput(formatted);
  return { toolCallId, toolName: 'search_docs', success: true, output, durationMs: Date.now() - startTime, truncated };
}
