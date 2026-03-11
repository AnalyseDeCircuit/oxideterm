import { memo, useState, useCallback } from 'react';
import { ChevronDown, ChevronRight, Terminal, FileText, FolderOpen, Search, GitBranch, Pen, Loader2, CheckCircle2, XCircle, AlertTriangle, Package, Network, Radio, CirclePlus, CircleStop, Activity, HardDrive, FolderSearch, FileCode, Code2, Info, ListTree, Settings, Puzzle, ShieldAlert, Check, X, Eye } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import { useAiChatStore } from '../../store/aiChatStore';
import { isCommandDenied } from '../../lib/ai/tools';
import type { AiToolCall } from '../../types';

interface ToolCallBlockProps {
  toolCalls: AiToolCall[];
  /** Total number of tool rounds (optional, for condensation indicator) */
  totalRounds?: number;
}

const TOOL_ICONS: Record<string, React.ElementType> = {
  terminal_exec: Terminal,
  read_file: FileText,
  write_file: Pen,
  list_directory: FolderOpen,
  grep_search: Search,
  git_status: GitBranch,
  list_sessions: Network,
  get_terminal_buffer: Terminal,
  search_terminal: Search,
  await_terminal_output: Eye,
  list_connections: Network,
  list_port_forwards: Radio,
  get_detected_ports: Radio,
  get_connection_health: Activity,
  create_port_forward: CirclePlus,
  stop_port_forward: CircleStop,
  // SFTP tools
  sftp_list_dir: FolderSearch,
  sftp_read_file: HardDrive,
  sftp_stat: Info,
  sftp_get_cwd: HardDrive,
  // IDE tools
  ide_get_open_files: FileCode,
  ide_get_file_content: FileCode,
  ide_get_project_info: Code2,
  ide_apply_edit: Pen,
  // Local terminal tools
  local_list_shells: Terminal,
  local_get_terminal_info: ListTree,
  local_exec: Terminal,
  local_get_drives: HardDrive,
  // Settings tools
  get_settings: Settings,
  update_setting: Settings,
  // Connection pool tools
  get_pool_stats: Activity,
  set_pool_config: Settings,
  // Connection monitor tools
  get_all_health: Activity,
  get_resource_metrics: Activity,
  // Session manager tools
  list_saved_connections: Network,
  search_saved_connections: Search,
  get_session_tree: ListTree,
  // Plugin manager tools
  list_plugins: Puzzle,
};

function StatusIcon({ status }: { status: AiToolCall['status'] }) {
  switch (status) {
    case 'pending':
      return <AlertTriangle className="w-3 h-3 text-yellow-500/70" />;
    case 'pending_user_approval':
      return <ShieldAlert className="w-3 h-3 text-amber-400 animate-pulse" />;
    case 'approved':
    case 'running':
      return <Loader2 className="w-3 h-3 text-theme-accent animate-spin" />;
    case 'completed':
      return <CheckCircle2 className="w-3 h-3 text-green-500/70" />;
    case 'error':
      return <XCircle className="w-3 h-3 text-red-500/70" />;
    case 'rejected':
      return <XCircle className="w-3 h-3 text-theme-text-muted/40" />;
  }
}

function formatArgs(argsJson: string): string {
  try {
    const parsed = JSON.parse(argsJson);
    // Show compact representation for common patterns
    if (parsed.command) return parsed.command;
    if (parsed.path) return parsed.path;
    if (parsed.pattern && parsed.path) return `${parsed.pattern} in ${parsed.path}`;
    return JSON.stringify(parsed, null, 2);
  } catch {
    return argsJson;
  }
}

const ToolCallItem = memo(function ToolCallItem({ call }: { call: AiToolCall }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const resolveToolApproval = useAiChatStore((s) => s.resolveToolApproval);
  const Icon = TOOL_ICONS[call.name] || Terminal;

  const toggleExpand = useCallback(() => setExpanded((v) => !v), []);

  const summary = formatArgs(call.arguments);
  const hasOutput = call.result && (call.result.output || call.result.error);
  const isPendingApproval = call.status === 'pending_user_approval';

  // Check if this is a deny-listed command for showing a stronger warning
  const isDenyListCommand = isPendingApproval &&
    (call.name === 'terminal_exec' || call.name === 'local_exec') &&
    (() => { try { const p = JSON.parse(call.arguments); return typeof p.command === 'string' && isCommandDenied(p.command); } catch { return false; } })();

  return (
    <div className={cn(
      "border rounded overflow-hidden",
      isPendingApproval
        ? isDenyListCommand
          ? "border-red-500/40 bg-red-500/5"
          : "border-amber-500/40 bg-amber-500/5"
        : "border-theme-border/20",
    )}>
      {/* Header */}
      <button
        onClick={toggleExpand}
        className={cn(
          'w-full flex items-center gap-1.5 px-2 py-1.5 text-left',
          'hover:bg-theme-bg-hover/30 transition-colors',
          'text-[11px]',
        )}
      >
        <StatusIcon status={call.status} />
        <Icon className="w-3 h-3 text-theme-text-muted/60 shrink-0" />
        <span className="font-medium text-theme-text-muted/70 shrink-0">
          {t(`ai.tool_use.tool_names.${call.name}`, { defaultValue: call.name })}
        </span>
        <span className="text-theme-text-muted/40 truncate flex-1 ml-1 font-mono text-[10px]">
          {summary.length > 80 ? summary.slice(0, 80) + '…' : summary}
        </span>
        {call.result?.durationMs != null && (
          <span className="text-[9px] text-theme-text-muted/30 font-mono shrink-0">
            {call.result.durationMs < 1000
              ? `${call.result.durationMs}ms`
              : `${(call.result.durationMs / 1000).toFixed(1)}s`}
          </span>
        )}
        {expanded
          ? <ChevronDown className="w-3 h-3 text-theme-text-muted/40 shrink-0" />
          : <ChevronRight className="w-3 h-3 text-theme-text-muted/40 shrink-0" />}
      </button>

      {/* Approval action bar */}
      {isPendingApproval && (
        <div className="flex items-center gap-2 px-2 py-1.5 border-t border-theme-border/15">
          {isDenyListCommand && (
            <span className="flex items-center gap-1 text-[10px] text-red-400 mr-auto">
              <ShieldAlert className="w-3 h-3" />
              {t('ai.tool_use.deny_list_warning')}
            </span>
          )}
          {!isDenyListCommand && (
            <span className="text-[10px] text-amber-400 mr-auto">
              {t('ai.tool_use.approval_required')}
            </span>
          )}
          <button
            onClick={() => resolveToolApproval(call.id, true)}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium bg-green-500/20 text-green-400 hover:bg-green-500/30 transition-colors"
          >
            <Check className="w-3 h-3" />
            {t('ai.tool_use.approve')}
          </button>
          <button
            onClick={() => resolveToolApproval(call.id, false)}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors"
          >
            <X className="w-3 h-3" />
            {t('ai.tool_use.reject')}
          </button>
        </div>
      )}

      {/* Expanded details */}
      {expanded && (
        <div className="border-t border-theme-border/15 px-2 py-1.5 space-y-1.5">
          {/* Arguments */}
          <div>
            <div className="text-[9px] text-theme-text-muted/40 font-medium uppercase tracking-wider mb-0.5">
              {t('ai.tool_use.arguments')}
            </div>
            <pre className="text-[10px] text-theme-text-muted/60 font-mono bg-theme-bg/50 rounded px-1.5 py-1 overflow-x-auto max-h-[120px] overflow-y-auto whitespace-pre-wrap break-all">
              {(() => {
                try { return JSON.stringify(JSON.parse(call.arguments), null, 2); }
                catch { return call.arguments; }
              })()}
            </pre>
          </div>

          {/* Output */}
          {hasOutput && (
            <div>
              <div className="text-[9px] text-theme-text-muted/40 font-medium uppercase tracking-wider mb-0.5">
                {t('ai.tool_use.output')}
              </div>
              {call.result!.error && (
                <div className="text-[10px] text-red-400/80 font-mono bg-red-500/5 rounded px-1.5 py-1 mb-1">
                  {call.result!.error}
                </div>
              )}
              {call.result!.output && (
                <pre className="text-[10px] text-theme-text-muted/60 font-mono bg-theme-bg/50 rounded px-1.5 py-1 overflow-x-auto max-h-[200px] overflow-y-auto whitespace-pre-wrap break-all">
                  {call.result!.output}
                </pre>
              )}
              {call.result!.truncated && (
                <div className="text-[9px] text-yellow-500/60 mt-0.5">
                  {t('ai.tool_use.output_truncated')}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
});

/**
 * ToolCallBlock — Displays tool calls made by the AI assistant.
 * Shows as collapsible blocks with tool name, arguments, status icon, and output.
 * When 5+ tool calls exist, early calls are collapsed behind a compact toggle,
 * reflecting that their results were condensed for the AI context window.
 */
export const ToolCallBlock = memo(function ToolCallBlock({ toolCalls }: ToolCallBlockProps) {
  const { t } = useTranslation();
  const [showEarly, setShowEarly] = useState(false);

  if (!toolCalls || toolCalls.length === 0) return null;

  // When 5+ tool calls, collapse the first N-3 behind a toggle
  const shouldCondense = toolCalls.length >= 5;
  const splitAt = shouldCondense ? Math.max(0, toolCalls.length - 3) : 0;
  const earlyCalls = shouldCondense ? toolCalls.slice(0, splitAt) : [];
  const recentCalls = shouldCondense ? toolCalls.slice(splitAt) : toolCalls;

  return (
    <div className="my-2 space-y-1">
      <div className="text-[10px] text-theme-text-muted/40 font-medium uppercase tracking-wider px-0.5">
        {t('ai.tool_use.heading')} ({toolCalls.length})
      </div>

      {/* Condensed early calls toggle */}
      {shouldCondense && earlyCalls.length > 0 && (
        showEarly ? (
          <>
            <button
              onClick={() => setShowEarly(false)}
              className="flex items-center gap-1 text-[9px] text-theme-text-muted/30 hover:text-theme-text-muted/50 transition-colors px-0.5 mb-1"
            >
              <Package className="w-2.5 h-2.5" />
              <span>{t('ai.tool_use.condensed_label')}</span>
              <ChevronDown className="w-2.5 h-2.5" />
            </button>
            {earlyCalls.map((call) => (
              <ToolCallItem key={call.id} call={call} />
            ))}
            <div className="border-t border-theme-border/10 my-1" />
          </>
        ) : (
          <button
            onClick={() => setShowEarly(true)}
            className={cn(
              'flex items-center gap-1.5 px-2 py-1 rounded w-full text-left',
              'bg-theme-bg-hover/20 hover:bg-theme-bg-hover/40 transition-colors',
              'text-[10px] text-theme-text-muted/40',
            )}
          >
            <Package className="w-3 h-3 shrink-0" />
            <span>
              {t('ai.tool_use.condensed', {
                count: earlyCalls.length,
              })}
            </span>
            <ChevronRight className="w-3 h-3 shrink-0 ml-auto" />
          </button>
        )
      )}

      {/* Recent calls — always visible */}
      {recentCalls.map((call) => (
        <ToolCallItem key={call.id} call={call} />
      ))}
    </div>
  );
});
