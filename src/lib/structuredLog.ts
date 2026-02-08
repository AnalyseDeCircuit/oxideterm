/**
 * structuredLog — Structured logging for Virtual Session Proxy
 *
 * Emits structured diagnostic events for connection lifecycle,
 * session resolution, and SFTP/IDE tab operations. Only active
 * in DEV mode (tree-shaken in production).
 *
 * @module lib/structuredLog
 */

export interface LogEntry {
  component: string;
  event: string;
  nodeId?: string;
  sessionId?: string;
  connectionId?: string;
  phase?: string;
  generation?: number;
  elapsedMs?: number;
  outcome?: 'ok' | 'error' | 'timeout' | 'skipped';
  detail?: string;
  [key: string]: unknown;
}

const STYLE_HEADER = 'color: #818cf8; font-weight: bold';
const STYLE_OK = 'color: #34d399';
const STYLE_ERROR = 'color: #f87171';
const STYLE_WARN = 'color: #fbbf24';

function formatPrefix(entry: LogEntry): string {
  const ids: string[] = [];
  if (entry.nodeId) ids.push(`node=${entry.nodeId.slice(0, 8)}`);
  if (entry.sessionId) ids.push(`sid=${entry.sessionId.slice(0, 8)}`);
  if (entry.connectionId) ids.push(`cid=${entry.connectionId.slice(0, 8)}`);
  if (entry.phase) ids.push(`phase=${entry.phase}`);
  if (entry.generation !== undefined) ids.push(`gen=${entry.generation}`);
  const idStr = ids.length ? ` [${ids.join(' ')}]` : '';
  return `[${entry.component}] ${entry.event}${idStr}`;
}

/**
 * Emit a structured log event. No-op in production.
 */
export function slog(entry: LogEntry): void {
  if (!import.meta.env.DEV) return;

  const prefix = formatPrefix(entry);

  if (entry.outcome === 'error') {
    console.error(`%c${prefix}`, STYLE_ERROR, entry.detail ?? '', entry);
  } else if (entry.outcome === 'timeout') {
    console.warn(`%c${prefix}`, STYLE_WARN, entry.detail ?? '', entry);
  } else {
    console.log(`%c${prefix}`, entry.outcome === 'ok' ? STYLE_OK : STYLE_HEADER, entry);
  }
}

/**
 * Create a scoped logger that pre-fills component name.
 */
export function createScopedLogger(component: string) {
  return (event: string, extra?: Omit<LogEntry, 'component' | 'event'>) =>
    slog({ component, event, ...extra });
}
