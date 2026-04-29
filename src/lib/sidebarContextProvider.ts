// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Sidebar Context Provider
 * 
 * Aggregates environment context for AI sidebar chat, providing:
 * 1. Environment Snapshot - OS, connection details, session info
 * 2. Dynamic Buffer Sync - Last N lines from active terminal
 * 3. Selection Priority - Highlighted text as "focus area"
 * 
 * This enables deep context awareness for OxideSens.
 */

import { platform } from './platform';
import { 
  getActivePaneId, 
  getPaneMetadata,
  getTerminalBuffer,
  getTerminalSelection,
  getCwd,
  getActiveTerminalSelection,
  getPanesForTab,
  gatherAllPaneContexts,
} from './terminalRegistry';
import { useAppStore } from '../store/appStore';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import { useLocalTerminalStore } from '../store/localTerminalStore';
import { useIdeStore } from '../store/ideStore';
import { useSettingsStore } from '../store/settingsStore';
import { getSftpContext } from './sftpContextRegistry';
import { sanitizeForAi, sanitizeConnectionInfo } from './ai/contextSanitizer';
import { useEventLogStore } from '../store/eventLogStore';
import { useTransferStore } from '../store/transferStore';
import { useRecordingStore } from '../store/recordingStore';
import { useBroadcastStore } from '../store/broadcastStore';
import type { RemoteEnvInfo, TabType } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface EnvironmentSnapshot {
  /** Operating system of the LOCAL machine running OxideTerm */
  localOS: 'macOS' | 'Windows' | 'Linux';
  
  /** Terminal type: SSH or Local */
  terminalType: 'terminal' | 'local_terminal' | null;
  
  /** Currently active tab type (for tab-aware tool filtering) */
  activeTabType: TabType | null;
  
  /** Node ID associated with the active tab (for SFTP/IDE context) */
  activeNodeId: string | null;
  
  /** Session ID of the active terminal */
  sessionId: string | null;
  
  /** Current working directory from OSC 7 shell integration */
  cwd: string | null;
  
  /** Connection details for SSH terminals */
  connection: {
    id: string;
    host: string;
    port: number;
    username: string;
    /** Formatted as user@host */
    formatted: string;
  } | null;
  
  /** 
   * Remote environment info (detected after SSH connection)
   * - undefined: Detection not yet triggered or in progress  
   * - null: Detection failed (show "Unknown" in prompt)
   * - RemoteEnvInfo: Detection succeeded
   */
  remoteEnv: RemoteEnvInfo | null | undefined;
  
  /** Remote OS hint (fallback: from connection name or host patterns) */
  remoteOSHint: string | null;
}

export interface TerminalContext {
  /** Last N lines from the terminal buffer */
  buffer: string | null;
  
  /** Number of lines captured */
  lineCount: number;
  
  /** Currently selected text (priority focus) */
  selection: string | null;
  
  /** Whether selection exists */
  hasSelection: boolean;
}

export interface IdeContext {
  /** Project root path */
  projectRoot: string;
  /** Project name */
  projectName: string;
  /** Whether the project is a git repo */
  isGitRepo: boolean;
  /** Git branch if available */
  gitBranch?: string;
  /** Active file path (if any tab is open) */
  activeFile?: string;
  /** Active file language */
  activeLanguage?: string;
  /** Cursor position in active file */
  cursor?: { line: number; col: number };
  /** Whether the active file has unsaved changes */
  isDirty?: boolean;
  /** Number of open tabs */
  openTabCount: number;
  /** Open tab file paths */
  openTabPaths: string[];
  /** Code snippet around cursor (if available) */
  codeSnippet?: string;
}

export interface SftpContext {
  /** Current remote working directory */
  remotePath: string;
  /** Remote home directory */
  remoteHome: string;
  /** Selected file/folder names */
  selectedFiles: string[];
  /** Node ID for context linking */
  nodeId: string;
}

export interface SidebarContext {
  /** Environment snapshot */
  env: EnvironmentSnapshot;
  
  /** Terminal buffer and selection */
  terminal: TerminalContext;
  
  /** IDE context (when IDE mode is active) */
  ide: IdeContext | null;
  
  /** SFTP context (when SFTP tab is active) */
  sftp: SftpContext | null;
  
  /** Formatted system prompt segment */
  systemPromptSegment: string;
  
  /** Formatted context block for inclusion */
  contextBlock: string;
  
  /** Timestamp when context was gathered */
  gatheredAt: number;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Detect local OS
 */
function getLocalOS(): 'macOS' | 'Windows' | 'Linux' {
  if (platform.isMac) return 'macOS';
  if (platform.isWindows) return 'Windows';
  return 'Linux';
}

import { tabTypeLabel } from './ai/tabTypeLabel';

/**
 * Extract last N lines from buffer
 */
function extractLastLines(buffer: string, maxLines: number): { text: string; lineCount: number } {
  const lines = buffer.split('\n');
  const actualLines = Math.min(lines.length, maxLines);
  const extracted = lines.slice(-maxLines).join('\n');
  return { text: extracted, lineCount: actualLines };
}

/**
 * Try to guess remote OS from connection details
 */
function guessRemoteOS(host: string, username: string): string | null {
  const hostLower = host.toLowerCase();
  const userLower = username.toLowerCase();
  
  // Windows hints
  if (hostLower.includes('windows') || hostLower.includes('win-') || 
      userLower === 'administrator' || hostLower.endsWith('.local')) {
    return 'Windows (guessed)';
  }
  
  // macOS hints
  if (hostLower.includes('mac') || hostLower.includes('darwin')) {
    return 'macOS (guessed)';
  }
  
  // Common Linux server patterns
  if (hostLower.includes('ubuntu') || hostLower.includes('debian') ||
      hostLower.includes('centos') || hostLower.includes('rhel') ||
      hostLower.includes('fedora') || hostLower.includes('arch')) {
    return 'Linux (guessed)';
  }
  
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Main API
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Default configuration for context gathering
 */
export const DEFAULT_CONTEXT_CONFIG = {
  /** Maximum lines to capture from buffer */
  maxBufferLines: 50,
  /** Maximum characters for buffer */
  maxBufferChars: 8000,
  /** Maximum characters for selection */
  maxSelectionChars: 2000,
};

/**
 * Build a compact summary of available sessions for the system prompt.
 * Only shows the active session in detail; other sessions are counted
 * to save tokens (use `list_sessions` tool for full details).
 */
function gatherSessionsSummary(activeSessionId: string | null): string | null {
  const lines: string[] = [];
  let activeFound = false;

  // SSH sessions
  const nodes = useSessionTreeStore.getState().nodes;
  const connections = useAppStore.getState().connections;
  const sshNodes = nodes.filter(n =>
    n.runtime?.connectionId || n.runtime?.status === 'connected' || n.runtime?.status === 'active'
  );

  let sshSessionCount = 0;
  for (const node of sshNodes) {
    const conn = node.runtime.connectionId ? connections.get(node.runtime.connectionId) : undefined;
    const host = conn
      ? sanitizeConnectionInfo(conn.username, conn.host, conn.port)
      : sanitizeConnectionInfo(node.username ?? '?', node.host ?? '?', node.port ?? 22);
    const terminalIds = node.runtime.terminalIds ?? [];
    for (const tid of terminalIds) {
      sshSessionCount++;
      if (tid === activeSessionId) {
        lines.push(`- ★ Active: SSH session_id=${tid} → ${host} (node_id=${node.id})`);
        activeFound = true;
      }
    }
  }

  // Local terminals
  const localTerminals = useLocalTerminalStore.getState().terminals;
  let localCount = 0;
  for (const [sessionId, info] of localTerminals) {
    localCount++;
    if (sessionId === activeSessionId) {
      const shellName = info.shell?.label || info.shell?.path || 'shell';
      lines.push(`- ★ Active: Local session_id=${sessionId} → ${shellName}`);
      activeFound = true;
    }
  }

  // Show count for non-active sessions
  const otherCount = sshSessionCount + localCount - (activeFound ? 1 : 0);
  if (otherCount > 0) {
    lines.push(`- ${otherCount} other session(s) available (use \`list_sessions\` for details)`);
  }

  if (lines.length === 0) return null;

  return `## Available Sessions\n${lines.join('\n')}`;
}

/**
 * Runtime tabs and terminal sessions are intentionally not persisted.
 * Surface this explicitly to the model so old conversation history cannot
 * masquerade as current app state after a reload/restart.
 */
function gatherRuntimeStateHint(): string {
  const appState = useAppStore.getState();
  const sshTerminalCount = useSessionTreeStore.getState().nodes.reduce(
    (count, node) => count + (node.runtime?.terminalIds?.length ?? 0),
    0,
  );
  const localTerminalCount = useLocalTerminalStore.getState().terminals.size;
  const terminalCount = sshTerminalCount + localTerminalCount;

  const lines = [
    '## Runtime State',
    `- Open tabs: ${appState.tabs.length}`,
    `- Runtime terminal sessions: ${terminalCount}`,
    '- Tabs, pane ids, and terminal session ids are memory-only and do not survive an app restart/reload.',
  ];

  if (appState.tabs.length === 0 && terminalCount === 0) {
    lines.push('- No tab or terminal session is currently open. Treat any tab_id/session_id from earlier conversation history as stale; use list_targets/connect_target/open_app_surface before acting.');
  } else {
    lines.push('- If earlier conversation history mentions a tab_id/session_id that is not visible in current targets, treat it as stale and refresh with list_targets.');
  }

  return lines.join('\n');
}

/**
 * Gather complete sidebar context for AI
 * 
 * @param config - Optional configuration overrides
 * @returns Complete context snapshot
 */
export function gatherSidebarContext(config = DEFAULT_CONTEXT_CONFIG): SidebarContext {
  // ─── Derive active pane from appStore (UI truth source) ─────────────
  // appStore.activeTabId + tab.activePaneId is the canonical source.
  // Never fall back to terminalRegistry's global activePaneId — it can
  // point to a pane in a different tab, causing cross-tab context leakage.
  const appState = useAppStore.getState();
  const activeTab = appState.tabs.find(t => t.id === appState.activeTabId);
  
  // Resolve active pane scoped to the current tab:
  // 1. Use tab.activePaneId (UI truth source)
  // 2. Fallback: search registry for any pane in this tab (race condition safety)
  // 3. Never fall back to registry's global activePaneId
  let paneId: string | null = null;
  if (activeTab) {
    paneId = activeTab.activePaneId ?? null;
    if (!paneId) {
      // Tab exists but no activePaneId — search registry for this tab's panes
      const tabPanes = getPanesForTab(activeTab.id);
      paneId = tabPanes.length > 0 ? tabPanes[0] : null;
    }
  }
  const metadata = paneId ? getPaneMetadata(paneId) : null;
  
  // ─── Environment Snapshot ───────────────────────────────────────────────
  
  const env: EnvironmentSnapshot = {
    localOS: getLocalOS(),
    terminalType: metadata?.terminalType ?? null,
    activeTabType: activeTab?.type ?? null,
    activeNodeId: activeTab?.nodeId ?? null,
    sessionId: metadata?.sessionId ?? null,
    cwd: paneId ? getCwd(paneId) : null,
    connection: null,
    remoteEnv: undefined, // Will be set if SSH connection has detected env
    remoteOSHint: null,
  };
  
  // Get connection details for SSH terminals
  if (metadata?.terminalType === 'terminal' && metadata.sessionId) {
    const sessions = useAppStore.getState().sessions;
    const session = sessions.get(metadata.sessionId);
    
    if (session?.connectionId) {
      const connections = useAppStore.getState().connections;
      const conn = connections.get(session.connectionId);
      
      if (conn) {
        env.connection = {
          id: conn.id,
          host: conn.host,
          port: conn.port,
          username: conn.username,
          formatted: sanitizeConnectionInfo(conn.username, conn.host, conn.port),
        };
        // Use detected remoteEnv if available, otherwise fall back to guessing
        if (conn.remoteEnv) {
          env.remoteEnv = conn.remoteEnv;
        } else {
          env.remoteEnv = undefined; // Still detecting
        }
        env.remoteOSHint = guessRemoteOS(conn.host, conn.username);
      }
    } else if (session) {
      // Fallback: use session info directly
      env.connection = {
        id: session.id,
        host: session.host,
        port: session.port,
        username: session.username,
        formatted: sanitizeConnectionInfo(session.username, session.host, session.port),
      };
      env.remoteOSHint = guessRemoteOS(session.host, session.username);
    }
  }
  
  // Try sessionTreeStore for more accurate connection info
  if (metadata?.terminalType === 'terminal' && metadata.sessionId) {
    const nodeByTerminal = useSessionTreeStore.getState().getNodeByTerminalId(metadata.sessionId);
    if (nodeByTerminal?.runtime.connectionId) {
      const conn = useAppStore.getState().connections.get(nodeByTerminal.runtime.connectionId);
      if (conn) {
        env.connection = {
          id: conn.id,
          host: conn.host,
          port: conn.port,
          username: conn.username,
          formatted: sanitizeConnectionInfo(conn.username, conn.host, conn.port),
        };
        // Update remoteEnv from the most specific connection source
        if (conn.remoteEnv) {
          env.remoteEnv = conn.remoteEnv;
        }
      }
    }
  }
  
  // ─── Terminal Context ───────────────────────────────────────────────────
  
  let buffer: string | null = null;
  let lineCount = 0;
  let selection: string | null = null;
  
  if (paneId) {
    // Get buffer — validate tab ID to prevent cross-tab context leakage
    const rawBuffer = getTerminalBuffer(paneId, activeTab?.id);
    if (rawBuffer) {
      // Limit buffer size
      let truncated = rawBuffer;
      if (truncated.length > config.maxBufferChars) {
        truncated = truncated.slice(-config.maxBufferChars);
      }
      const extracted = extractLastLines(truncated, config.maxBufferLines);
      buffer = sanitizeForAi(extracted.text);
      lineCount = extracted.lineCount;
    }
    
    // Get selection (priority focus)
    const rawSelection = getTerminalSelection(paneId);
    if (rawSelection?.trim()) {
      let sel = rawSelection.length > config.maxSelectionChars
        ? rawSelection.slice(0, config.maxSelectionChars) + '...'
        : rawSelection;
      selection = sanitizeForAi(sel);
    }
  }
  
  const terminal: TerminalContext = {
    buffer,
    lineCount,
    selection,
    hasSelection: !!selection,
  };
  
  // ─── IDE Context ────────────────────────────────────────────────────────
  
  const contextSources = useSettingsStore.getState().settings.ai.contextSources;
  
  let ide: IdeContext | null = null;
  if (contextSources?.ide !== false) {
    try {
      const ideState = useIdeStore.getState();
      if (ideState.project && ideState.nodeId) {
        const activeTab = ideState.activeTabId
          ? ideState.tabs.find(t => t.id === ideState.activeTabId)
          : undefined;
        
        ide = {
          projectRoot: ideState.project.rootPath,
          projectName: ideState.project.name,
          isGitRepo: ideState.project.isGitRepo,
          gitBranch: ideState.project.gitBranch,
          activeFile: activeTab?.path,
          activeLanguage: activeTab?.language,
          cursor: activeTab?.cursor,
          isDirty: activeTab?.isDirty,
          openTabCount: ideState.tabs.length,
          openTabPaths: ideState.tabs.map(t => t.path),
        };

        // Extract code snippet around cursor (±10 lines, max 4000 chars)
        if (activeTab?.content && activeTab.cursor) {
          const lines = activeTab.content.split('\n');
          const cursorLine = activeTab.cursor.line - 1; // 0-based
          const start = Math.max(0, cursorLine - 10);
          const end = Math.min(lines.length, cursorLine + 11);
          const snippet = lines.slice(start, end).join('\n');
          ide.codeSnippet = snippet.length > 4000 ? snippet.slice(0, 4000) + '\n... (truncated)' : snippet;
        }
      }
    } catch {
      // IDE store may not be available
    }
  }
  
  // ─── SFTP Context ───────────────────────────────────────────────────────
  
  let sftp: SftpContext | null = null;
  if (contextSources?.sftp !== false) {
    const nodeId = env.activeNodeId;
    if (nodeId) {
      const sftpSnapshot = getSftpContext(nodeId);
      if (sftpSnapshot) {
        sftp = {
          remotePath: sftpSnapshot.remotePath,
          remoteHome: sftpSnapshot.remoteHome,
          selectedFiles: sftpSnapshot.selectedFiles,
          nodeId: sftpSnapshot.nodeId,
        };
      }
    }
  }
  
  // ─── Format System Prompt Segment ───────────────────────────────────────
  
  const systemPromptSegment = formatSystemPromptSegment(env, terminal, ide, sftp);
  const contextBlock = formatContextBlock(env, terminal, ide, paneId ?? null);
  
  return {
    env,
    terminal,
    ide,
    sftp,
    systemPromptSegment,
    contextBlock,
    gatheredAt: Date.now(),
  };
}

/**
 * Gather conditional state hints for sidebar context injection.
 * Only returns content when there are active issues, transfers, recordings or broadcast.
 * Keeps output minimal to avoid wasting tokens when nothing interesting is happening.
 */
function gatherConditionalStateHints(): string | null {
  const hints: string[] = [];

  // Event log: recent errors/warnings in last 5 minutes
  const fiveMinAgo = Date.now() - 5 * 60 * 1000;
  const recentEntries = useEventLogStore.getState().entries.filter(e => e.timestamp >= fiveMinAgo);
  const errorCount = recentEntries.filter(e => e.severity === 'error').length;
  const warnCount = recentEntries.filter(e => e.severity === 'warn').length;
  if (errorCount > 0 || warnCount > 0) {
    const parts: string[] = [];
    if (errorCount > 0) parts.push(`${errorCount} error(s)`);
    if (warnCount > 0) parts.push(`${warnCount} warning(s)`);
    hints.push(`⚠ Recent issues (5min): ${parts.join(', ')}. Use get_event_log for details.`);
  }

  // Active file transfers
  const activeTransfers = useTransferStore.getState().getActiveTransfers();
  const pendingTransfers = Array.from(useTransferStore.getState().transfers.values()).filter(t => t.state === 'pending');
  const transferCount = activeTransfers.length + pendingTransfers.length;
  if (transferCount > 0) {
    hints.push(`📦 Active transfers: ${transferCount}. Use get_transfer_status for details.`);
  }

  // Active recordings
  const { recordings } = useRecordingStore.getState();
  if (recordings.size > 0) {
    hints.push(`🔴 Recording active on ${recordings.size} session(s). Use get_recording_status for details.`);
  }

  // Broadcast mode
  const { enabled, targets } = useBroadcastStore.getState();
  if (enabled) {
    hints.push(`📡 Broadcast mode ON targeting ${targets.size} pane(s).`);
  }

  if (hints.length === 0) return null;

  return '## Active State\n' + hints.join('\n');
}

/**
 * Format environment info as a system prompt segment
 */
function formatSystemPromptSegment(
  env: EnvironmentSnapshot,
  terminal: TerminalContext,
  ide: IdeContext | null,
  sftp: SftpContext | null,
): string {
  const parts: string[] = [];
  
  // Environment header
  parts.push('## Environment');
  parts.push(`- Local OS: ${env.localOS}`);
  
  // Explicit active tab type — so LLM knows UI context
  if (env.activeTabType) {
    parts.push(`- Active tab: ${tabTypeLabel(env.activeTabType)}`);
  }
  
  if (env.cwd) {
    parts.push(`- Current working directory: ${env.cwd}`);
  }
  
  if (env.terminalType === 'terminal' && env.connection) {
    parts.push(`- Terminal: SSH to ${env.connection.formatted}`);
    if (env.sessionId) {
      parts.push(`- Active session_id: ${env.sessionId}`);
    }
    
    // Remote OS: prefer detected env, fall back to guessing
    if (env.remoteEnv) {
      // Full detected environment info
      const { osType, osVersion, arch, kernel, shell } = env.remoteEnv;
      parts.push(`- Remote OS: ${osType}${osVersion ? ` (${osVersion})` : ''}`);
      if (arch) parts.push(`- Architecture: ${arch}`);
      if (kernel) parts.push(`- Kernel: ${kernel}`);
      if (shell) parts.push(`- Shell: ${shell}`);
    } else if (env.remoteEnv === undefined) {
      // Detection in progress
      parts.push(`- Remote OS: [detecting...]${env.remoteOSHint ? ` (hint: ${env.remoteOSHint})` : ''}`);
    } else {
      // Detection failed (env.remoteEnv === null) - use fallback
      parts.push(`- Remote OS: ${env.remoteOSHint ?? 'Unknown'}`);
    }
  } else if (env.terminalType === 'local_terminal') {
    parts.push(`- Terminal: Local (${env.localOS})`);
    if (env.sessionId) {
      parts.push(`- Active session_id: ${env.sessionId}`);
    }
  } else {
    parts.push('- Terminal: No active terminal');
  }
  
  // Available sessions summary — so AI can target sessions without calling list_sessions
  const sessionsSummary = gatherSessionsSummary(env.sessionId);
  if (sessionsSummary) {
    parts.push('');
    parts.push(sessionsSummary);
  }

  parts.push('');
  parts.push(gatherRuntimeStateHint());
  
  // Conditional app state hints — only injected when non-trivial state exists
  const stateHints = gatherConditionalStateHints();
  if (stateHints) {
    parts.push('');
    parts.push(stateHints);
  }
  
  // Selection notice
  if (terminal.hasSelection) {
    parts.push('');
    parts.push('## User Selection (Priority Focus)');
    parts.push('The user has selected specific text in the terminal. This selection should be treated as the PRIMARY subject of their query unless they explicitly ask about something else.');
  }
  
  // IDE context
  if (ide) {
    parts.push('');
    parts.push('## IDE Context');
    parts.push(`- Project: ${ide.projectName} (${ide.projectRoot})`);
    if (ide.isGitRepo) {
      parts.push(`- Git: ${ide.gitBranch ?? 'unknown branch'}`);
    }
    if (ide.activeFile) {
      const dirtyMark = ide.isDirty ? ' [unsaved]' : '';
      parts.push(`- Editing: ${ide.activeFile} (${ide.activeLanguage ?? 'unknown'})${dirtyMark}`);
      if (ide.cursor) {
        parts.push(`- Cursor: line ${ide.cursor.line}, col ${ide.cursor.col}`);
      }
    }
    if (ide.openTabCount > 1) {
      parts.push(`- Open tabs (${ide.openTabCount}): ${ide.openTabPaths.join(', ')}`);
    }
  }
  
  // SFTP file browser context
  if (sftp) {
    parts.push('');
    parts.push('## File Browser Context');
    parts.push(`- CWD: ${sftp.remotePath}`);
    if (sftp.selectedFiles.length > 0) {
      const maxShow = 20;
      const shown = sftp.selectedFiles.slice(0, maxShow);
      const suffix = sftp.selectedFiles.length > maxShow ? ` ... +${sftp.selectedFiles.length - maxShow} more` : '';
      parts.push(`- Selected (${sftp.selectedFiles.length}): [${shown.join(', ')}${suffix}]`);
    }
  }
  
  return parts.join('\n');
}

/**
 * Format context as a code block for API messages.
 * Includes multi-pane context when split panes exist in the active tab.
 * Includes IDE code snippet when IDE mode is active.
 */
function formatContextBlock(_env: EnvironmentSnapshot, terminal: TerminalContext, ide: IdeContext | null, resolvedPaneId: string | null): string {
  const parts: string[] = [];
  
  // Selection first (priority)
  if (terminal.selection) {
    parts.push('=== SELECTED TEXT (Focus Area) ===');
    parts.push(terminal.selection);
    parts.push('');
  }
  
  // Multi-pane context: if active tab has split panes, show all pane buffers
  const appState = useAppStore.getState();
  const activeTab = appState.tabs.find(t => t.id === appState.activeTabId);
  
  const MULTI_PANE_MAX_CHARS = 4000;
  const MULTI_PANE_MAX_LINES = 30;

  if (activeTab) {
    try {
      // Pass resolvedPaneId so isActive marking is consistent with pane resolution
      const paneContexts = gatherAllPaneContexts(activeTab.id, MULTI_PANE_MAX_CHARS, resolvedPaneId);
      if (paneContexts.length > 1) {
        // Multiple panes — show each with label
        for (const ctx of paneContexts) {
          if (!ctx.buffer) continue;
          const label = ctx.isActive ? 'Active Pane' : 'Pane';
          const typeName = ctx.terminalType === 'terminal' ? 'SSH' : 'Local';
          const lines = ctx.buffer.split('\n');
          const lastLines = lines.slice(-MULTI_PANE_MAX_LINES).join('\n');
          parts.push(`=== ${label} (${typeName}, session_id=${ctx.sessionId}) — last ${Math.min(lines.length, MULTI_PANE_MAX_LINES)} lines ===`);
          parts.push(lastLines);
          parts.push('');
        }
      } else {
        // Single pane — use the already-gathered buffer
        if (terminal.buffer) {
          parts.push(`=== Terminal Output (last ${terminal.lineCount} lines) ===`);
          parts.push(terminal.buffer);
        }
      }
    } catch (e) {
      console.warn('[sidebarContextProvider] Failed to gather pane contexts:', e);
      if (terminal.buffer) {
        parts.push(`=== Terminal Output (last ${terminal.lineCount} lines) ===`);
        parts.push(terminal.buffer);
      }
    }
  } else if (terminal.buffer) {
    parts.push(`=== Terminal Output (last ${terminal.lineCount} lines) ===`);
    parts.push(terminal.buffer);
  }
  
  // IDE code snippet (around cursor)
  if (ide?.codeSnippet && ide.activeFile) {
    const startLine = ide.cursor ? Math.max(1, ide.cursor.line - 10) : 1;
    parts.push('');
    parts.push(`=== Code: ${ide.activeFile} (${ide.activeLanguage ?? 'text'}, lines ${startLine}+) ===`);
    parts.push(ide.codeSnippet);
  }
  
  if (parts.length === 0) {
    return '';
  }
  
  return parts.join('\n');
}

/**
 * Quick check if any terminal context is available
 */
export function hasTerminalContext(): boolean {
  return getActivePaneId() !== null;
}

/**
 * Get just the selection (for quick checks)
 */
export function getQuickSelection(): string | null {
  return getActiveTerminalSelection();
}

/**
 * Get environment info only (lightweight)
 */
export function getEnvironmentInfo(): EnvironmentSnapshot {
  const context = gatherSidebarContext({ 
    maxBufferLines: 0, 
    maxBufferChars: 0, 
    maxSelectionChars: 0 
  });
  return context.env;
}

/**
 * Build a compact context reminder for injection after conversation history.
 * Placed right before the LLM's next response to ensure it sees current state,
 * even when conversation history discusses a different tab/terminal.
 */
export function buildContextReminder(ctx: SidebarContext | null): string | null {
  if (!ctx) return null;
  const { env } = ctx;
  const parts: string[] = [];

  if (env.activeTabType) {
    parts.push(`Active tab: ${tabTypeLabel(env.activeTabType)}`);
  }

  if (env.terminalType === 'terminal' && env.connection) {
    parts.push(`Terminal: SSH to ${env.connection.formatted}`);
    if (env.sessionId) parts.push(`session_id: ${env.sessionId}`);
  } else if (env.terminalType === 'local_terminal') {
    parts.push(`Terminal: Local (${env.localOS})`);
    if (env.sessionId) parts.push(`session_id: ${env.sessionId}`);
  } else {
    parts.push('No active terminal');
  }

  if (env.cwd) parts.push(`cwd: ${env.cwd}`);

  const appState = useAppStore.getState();
  const sshTerminalCount = useSessionTreeStore.getState().nodes.reduce(
    (count, node) => count + (node.runtime?.terminalIds?.length ?? 0),
    0,
  );
  const localTerminalCount = useLocalTerminalStore.getState().terminals.size;
  const terminalCount = sshTerminalCount + localTerminalCount;
  parts.push(`open tabs: ${appState.tabs.length}`);
  parts.push(`runtime terminal sessions: ${terminalCount}`);
  if (appState.tabs.length === 0 && terminalCount === 0) {
    parts.push('old tab_id/session_id values from earlier history are stale after app restart/reload');
  }

  return `[Current context: ${parts.join(' | ')}. When the user's question is about the current environment, use this context rather than earlier conversation history.]`;
}
