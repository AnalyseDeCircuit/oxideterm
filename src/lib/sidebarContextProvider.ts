/**
 * Sidebar Context Provider
 * 
 * Aggregates environment context for AI sidebar chat, providing:
 * 1. Environment Snapshot - OS, connection details, session info
 * 2. Dynamic Buffer Sync - Last N lines from active terminal
 * 3. Selection Priority - Highlighted text as "focus area"
 * 
 * This enables GitHub Copilot-style deep context awareness.
 */

import { platform } from './platform';
import { 
  getActivePaneId, 
  getActivePaneMetadata, 
  getActiveTerminalBuffer,
  getActiveTerminalSelection,
} from './terminalRegistry';
import { useAppStore } from '../store/appStore';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import type { RemoteEnvInfo } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface EnvironmentSnapshot {
  /** Operating system of the LOCAL machine running OxideTerm */
  localOS: 'macOS' | 'Windows' | 'Linux';
  
  /** Terminal type: SSH or Local */
  terminalType: 'terminal' | 'local_terminal' | null;
  
  /** Session ID of the active terminal */
  sessionId: string | null;
  
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

export interface SidebarContext {
  /** Environment snapshot */
  env: EnvironmentSnapshot;
  
  /** Terminal buffer and selection */
  terminal: TerminalContext;
  
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
 * Gather complete sidebar context for AI
 * 
 * @param config - Optional configuration overrides
 * @returns Complete context snapshot
 */
export function gatherSidebarContext(config = DEFAULT_CONTEXT_CONFIG): SidebarContext {
  const paneId = getActivePaneId();
  const metadata = getActivePaneMetadata();
  
  // ─── Environment Snapshot ───────────────────────────────────────────────
  
  const env: EnvironmentSnapshot = {
    localOS: getLocalOS(),
    terminalType: metadata?.terminalType ?? null,
    sessionId: metadata?.sessionId ?? null,
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
          formatted: `${conn.username}@${conn.host}`,
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
        formatted: `${session.username}@${session.host}`,
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
          formatted: `${conn.username}@${conn.host}`,
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
    // Get buffer
    const rawBuffer = getActiveTerminalBuffer();
    if (rawBuffer) {
      // Limit buffer size
      let truncated = rawBuffer;
      if (truncated.length > config.maxBufferChars) {
        truncated = truncated.slice(-config.maxBufferChars);
      }
      const extracted = extractLastLines(truncated, config.maxBufferLines);
      buffer = extracted.text;
      lineCount = extracted.lineCount;
    }
    
    // Get selection (priority focus)
    const rawSelection = getActiveTerminalSelection();
    if (rawSelection?.trim()) {
      selection = rawSelection.length > config.maxSelectionChars
        ? rawSelection.slice(0, config.maxSelectionChars) + '...'
        : rawSelection;
    }
  }
  
  const terminal: TerminalContext = {
    buffer,
    lineCount,
    selection,
    hasSelection: !!selection,
  };
  
  // ─── Format System Prompt Segment ───────────────────────────────────────
  
  const systemPromptSegment = formatSystemPromptSegment(env, terminal);
  const contextBlock = formatContextBlock(env, terminal);
  
  return {
    env,
    terminal,
    systemPromptSegment,
    contextBlock,
    gatheredAt: Date.now(),
  };
}

/**
 * Format environment info as a system prompt segment
 */
function formatSystemPromptSegment(env: EnvironmentSnapshot, terminal: TerminalContext): string {
  const parts: string[] = [];
  
  // Environment header
  parts.push('## Environment');
  parts.push(`- Local OS: ${env.localOS}`);
  
  if (env.terminalType === 'terminal' && env.connection) {
    parts.push(`- Terminal: SSH to ${env.connection.formatted}`);
    
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
  } else {
    parts.push('- Terminal: No active terminal');
  }
  
  // Selection notice
  if (terminal.hasSelection) {
    parts.push('');
    parts.push('## User Selection (Priority Focus)');
    parts.push('The user has selected specific text in the terminal. This selection should be treated as the PRIMARY subject of their query unless they explicitly ask about something else.');
  }
  
  return parts.join('\n');
}

/**
 * Format context as a code block for API messages
 */
function formatContextBlock(_env: EnvironmentSnapshot, terminal: TerminalContext): string {
  const parts: string[] = [];
  
  // Selection first (priority)
  if (terminal.selection) {
    parts.push('=== SELECTED TEXT (Focus Area) ===');
    parts.push(terminal.selection);
    parts.push('');
  }
  
  // Buffer context
  if (terminal.buffer) {
    parts.push(`=== Terminal Output (last ${terminal.lineCount} lines) ===`);
    parts.push(terminal.buffer);
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
