// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { memo, useState, useCallback } from 'react';
import { ChevronDown, ChevronRight, Terminal, FileText, FolderOpen, Search, GitBranch, Pen, Loader2, CheckCircle2, XCircle, AlertTriangle, Package, Network, Radio, CirclePlus, CircleStop, Activity, HardDrive, FolderSearch, FileCode, Code2, Info, ListTree, Settings, Puzzle, ShieldAlert, Check, X, Eye, Zap, Layers, Monitor, Keyboard, MousePointer2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import { useAiChatStore } from '../../store/aiChatStore';
import { fromLegacyToolResult, hasDeniedCommands, inferToolRisk, sanitizeToolArguments } from '../../lib/ai/tools';
import type { AiToolCall, AiToolResult } from '../../types';
import type { ToolRisk } from '../../lib/ai/tools';
import type { AiToolRound, AiTurnPart, AiTurnToolCall } from '../../lib/ai/turnModel/types';

interface ToolCallBlockProps {
  toolCalls?: AiToolCall[];
  toolRounds?: AiToolRound[];
  turnParts?: AiTurnPart[];
  /** Total number of tool rounds (optional, for condensation indicator) */
  totalRounds?: number;
}

function mapTurnToolCallStatus(toolCall: AiTurnToolCall): AiToolCall['status'] {
  if (toolCall.approvalState === 'rejected') return 'rejected';
  if (toolCall.executionState === 'completed') return 'completed';
  if (toolCall.executionState === 'error') return 'error';
  if (toolCall.executionState === 'running') return 'running';
  if (toolCall.approvalState === 'approved') return 'approved';
  if (toolCall.approvalState === 'pending') return 'pending_user_approval';
  return 'pending';
}

function collectToolResults(turnParts?: AiTurnPart[]): Map<string, AiToolResult> {
  const results = new Map<string, AiToolResult>();

  for (const part of turnParts ?? []) {
    if (part.type !== 'tool_result') continue;

    results.set(part.toolCallId, {
      toolCallId: part.toolCallId,
      toolName: part.toolName,
      success: part.success,
      output: part.output,
      error: part.error,
      durationMs: part.durationMs,
      truncated: part.truncated,
      envelope: part.envelope,
    });
  }

  return results;
}

function collectPartLevelToolCalls(turnParts: AiTurnPart[] | undefined, existingIds: Set<string>, resultMap: Map<string, AiToolResult>): AiToolCall[] {
  const normalized: AiToolCall[] = [];

  for (const part of turnParts ?? []) {
    if (part.type !== 'tool_call' || existingIds.has(part.id)) {
      continue;
    }

    const result = resultMap.get(part.id);
    normalized.push({
      id: part.id,
      name: part.name,
      arguments: part.argumentsText,
      status: result ? (result.success ? 'completed' : 'error') : (part.status === 'partial' ? 'pending' : 'running'),
      result,
    });
    existingIds.add(part.id);
  }

  return normalized;
}

function normalizeToolCalls(toolCalls?: AiToolCall[], toolRounds?: AiToolRound[], turnParts?: AiTurnPart[]): AiToolCall[] {
  if (toolCalls && toolCalls.length > 0) {
    return toolCalls;
  }

  const resultMap = collectToolResults(turnParts);

  if (!toolRounds || toolRounds.length === 0) {
    return collectPartLevelToolCalls(turnParts, new Set<string>(), resultMap);
  }

  const existingIds = new Set<string>();
  const roundDerived = toolRounds.flatMap((round) => round.toolCalls.map((toolCall) => {
    existingIds.add(toolCall.id);
    return {
    id: toolCall.id,
    name: toolCall.name,
    arguments: toolCall.argumentsText,
    status: mapTurnToolCallStatus(toolCall),
    result: resultMap.get(toolCall.id),
    };
  }));

  return [...roundDerived, ...collectPartLevelToolCalls(turnParts, existingIds, resultMap)];
}

const TOOL_ICONS: Record<string, React.ElementType> = {
  terminal_exec: Terminal,
  read_file: FileText,
  write_file: Pen,
  list_directory: FolderOpen,
  grep_search: Search,
  git_status: GitBranch,
  list_tabs: ListTree,
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
  // Local terminal tools
  local_list_shells: Terminal,
  local_get_terminal_info: ListTree,
  local_exec: Terminal,
  local_get_drives: HardDrive,
  // Settings tools
  get_settings: Settings,
  update_setting: Settings,
  open_settings_section: Settings,
  // Connection pool tools
  get_pool_stats: Activity,
  set_pool_config: Settings,
  // Connection monitor tools
  get_all_health: Activity,
  get_resource_metrics: Activity,
  // Session manager tools
  list_saved_connections: Network,
  search_saved_connections: Search,
  connect_saved_session: Network,
  connect_saved_connection_by_query: Network,
  get_session_tree: ListTree,
  // Plugin manager tools
  list_plugins: Puzzle,
  // Meta tools
  send_control_sequence: Zap,
  batch_exec: Layers,
  // TUI interaction (experimental)
  read_screen: Monitor,
  send_keys: Keyboard,
  send_mouse: MousePointer2,
};

const LONG_OUTPUT_PREVIEW_CHARS = 1200;

function humanizeToolToken(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function formatToolDisplayName(toolName: string, t: ReturnType<typeof useTranslation>['t']): string {
  const translated = t(`ai.tool_use.tool_names.${toolName}`, { defaultValue: '' });
  if (translated) return translated;

  const mcpMatch = /^mcp::([^:]+)::(.+)$/.exec(toolName);
  if (mcpMatch) {
    return `MCP: ${mcpMatch[1]} / ${humanizeToolToken(mcpMatch[2])}`;
  }

  const pluginMatch = /^plugin::([^:]+)::(.+)$/.exec(toolName);
  if (pluginMatch) {
    return `Plugin: ${pluginMatch[1]} / ${humanizeToolToken(pluginMatch[2])}`;
  }

  return humanizeToolToken(toolName);
}

function formatRiskLabel(risk: ToolRisk, t: ReturnType<typeof useTranslation>['t']): string {
  return t(`ai.tool_use.risk_labels.${risk}`, { defaultValue: humanizeToolToken(risk) });
}

const RISK_CLASS: Record<ToolRisk, string> = {
  read: 'border-sky-500/25 text-sky-300 bg-sky-500/10',
  'write-file': 'border-amber-500/30 text-amber-300 bg-amber-500/10',
  'execute-command': 'border-blue-500/30 text-blue-300 bg-blue-500/10',
  'interactive-input': 'border-violet-500/30 text-violet-300 bg-violet-500/10',
  destructive: 'border-red-500/40 text-red-300 bg-red-500/10',
  'network-expose': 'border-orange-500/40 text-orange-300 bg-orange-500/10',
  'settings-change': 'border-yellow-500/35 text-yellow-300 bg-yellow-500/10',
  'credential-sensitive': 'border-red-500/40 text-red-300 bg-red-500/10',
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
    const parsed = sanitizeToolArguments(JSON.parse(argsJson));
    // Show compact representation for common patterns
    if (parsed.command) return parsed.command;
    if (parsed.path) return parsed.path;
    if (parsed.pattern && parsed.path) return `${parsed.pattern} in ${parsed.path}`;
    return JSON.stringify(parsed, null, 2);
  } catch {
    return argsJson;
  }
}

function formatArgsForDetails(argsJson: string): string {
  try {
    return JSON.stringify(sanitizeToolArguments(JSON.parse(argsJson)), null, 2);
  } catch {
    return argsJson;
  }
}

function getLatestRoundMarker(toolRounds?: AiToolRound[]): string | undefined {
  if (!toolRounds || toolRounds.length === 0) {
    return undefined;
  }

  for (let index = toolRounds.length - 1; index >= 0; index -= 1) {
    const marker = toolRounds[index]?.statefulMarker;
    if (marker !== undefined) {
      return marker;
    }
  }

  return undefined;
}

function parseArgs(argsJson: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(argsJson);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : {};
  } catch {
    return {};
  }
}

function formatJson(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

function toolResultEnvelope(call: AiToolCall) {
  return call.result ? fromLegacyToolResult(call.result) : undefined;
}

function toolRisk(call: AiToolCall): ToolRisk {
  const envelope = toolResultEnvelope(call);
  return inferToolRisk(call.name, parseArgs(call.arguments), envelope?.meta.capability);
}

function Badge({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <span className={cn(
      'inline-flex items-center rounded border px-1 py-0.5 text-[9px] leading-none font-medium',
      className,
    )}>
      {children}
    </span>
  );
}

const ToolCallItem = memo(function ToolCallItem({ call }: { call: AiToolCall }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [showRawOutput, setShowRawOutput] = useState(false);
  const resolveToolApproval = useAiChatStore((s) => s.resolveToolApproval);
  const Icon = TOOL_ICONS[call.name] || Terminal;

  const toggleExpand = useCallback(() => setExpanded((v) => !v), []);

  const envelope = toolResultEnvelope(call);
  const risk = toolRisk(call);
  const summary = envelope?.summary || formatArgs(call.arguments);
  const capability = envelope?.meta.capability;
  const targetId = envelope?.meta.targetId;
  const bypassApproval = envelope?.meta.approvalMode === 'bypass';
  const policyDecision = envelope?.meta.policyDecision;
  const warnings = envelope?.warnings ?? [];
  const structuredData = envelope?.data;
  const hasOutput = call.result && (call.result.output || call.result.error);
  const isPendingApproval = call.status === 'pending_user_approval';
  const previewOutput = call.result?.output ?? '';
  const fullOutput = envelope?.rawOutput;
  const outputPreview = envelope?.outputPreview;
  const canShowFullOutput = typeof fullOutput === 'string' && fullOutput.length > 0;
  const outputWasCompressed = Boolean(call.result?.truncated || outputPreview?.strategy !== undefined && outputPreview.strategy !== 'full');
  const isLongOutput = previewOutput.length > LONG_OUTPUT_PREVIEW_CHARS;
  const shouldShowOutputToggle = canShowFullOutput || isLongOutput || outputWasCompressed;
  const displayedOutput = (() => {
    if (showRawOutput && canShowFullOutput) return fullOutput;
    if (isLongOutput && !showRawOutput) return `${previewOutput.slice(0, LONG_OUTPUT_PREVIEW_CHARS)}\n…`;
    return previewOutput;
  })();
  const outputStatsLabel = outputPreview
    ? t('ai.tool_use.output_stats', {
        defaultValue: '{{chars}} chars, {{lines}} lines{{omitted}}',
        chars: outputPreview.charCount,
        lines: outputPreview.lineCount,
        omitted: outputPreview.omittedChars ? `, ${outputPreview.omittedChars} omitted` : '',
      })
    : undefined;
  const outputToggleLabel = showRawOutput
    ? t('ai.tool_use.hide_raw_output')
    : canShowFullOutput
      ? t('ai.tool_use.show_raw_output')
      : t('ai.tool_use.show_more_preview', { defaultValue: 'Show more preview' });
  const sessionId = (() => {
    const args = parseArgs(call.arguments);
    return typeof args.session_id === 'string' ? args.session_id : undefined;
  })();

  // Check if this is a deny-listed command for showing a stronger warning
  const isDenyListCommand = isPendingApproval && (() => {
    try {
      const parsed = JSON.parse(call.arguments);
      return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
        ? hasDeniedCommands(call.name, parsed as Record<string, unknown>)
        : false;
    } catch {
      return false;
    }
  })();

  return (
    <div className={cn(
      "border rounded-md overflow-hidden",
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
          {formatToolDisplayName(call.name, t)}
        </span>
        <Badge className={cn('shrink-0', RISK_CLASS[risk])}>
          {formatRiskLabel(risk, t)}
        </Badge>
        {bypassApproval && (
          <Badge className="shrink-0 gap-1 border-amber-400/70 bg-amber-400/20 text-amber-200 shadow-[0_0_12px_rgba(251,191,36,0.25)]">
            <ShieldAlert className="h-2.5 w-2.5" />
            {t('ai.tool_use.bypass_badge', { defaultValue: 'Bypass' })}
          </Badge>
        )}
        {capability && (
          <Badge className="shrink-0 border-theme-border/30 text-theme-text-muted/60 bg-theme-bg/30">
            {capability}
          </Badge>
        )}
        <span className="text-theme-text-muted/50 truncate flex-1 ml-1 text-[10px]">
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
            className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-medium bg-green-500/20 text-green-400 hover:bg-green-500/30 transition-colors"
          >
            <Check className="w-3 h-3" />
            {t('ai.tool_use.approve')}
          </button>
          <button
            onClick={() => resolveToolApproval(call.id, false)}
            className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-medium bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors"
          >
            <X className="w-3 h-3" />
            {t('ai.tool_use.reject')}
          </button>
        </div>
      )}

      {/* Expanded details */}
      {expanded && (
        <div className="border-t border-theme-border/15 px-2 py-1.5 space-y-1.5">
          {(targetId || envelope?.summary) && (
            <div className="rounded-md border border-theme-border/15 bg-theme-bg/35 px-1.5 py-1 space-y-1">
              {envelope?.summary && (
                <div>
                  <span className="text-[9px] text-theme-text-muted/40 uppercase tracking-wider mr-1">{t('ai.tool_use.summary')}</span>
                  <span className="text-[10px] text-theme-text-muted/75">{envelope.summary}</span>
                </div>
              )}
              {targetId && (
                <div>
                  <span className="text-[9px] text-theme-text-muted/40 uppercase tracking-wider mr-1">{t('ai.tool_use.target')}</span>
                  <span className="text-[10px] text-theme-text-muted/65 font-mono">{targetId}</span>
                </div>
              )}
              {policyDecision && (
                <div>
                  <span className="text-[9px] text-theme-text-muted/40 uppercase tracking-wider mr-1">{t('ai.tool_use.policy', { defaultValue: 'Policy' })}</span>
                  <span className="text-[10px] text-theme-text-muted/65">
                    {policyDecision.decision} · {policyDecision.reasonCode} · {policyDecision.matchedPolicyKey}
                  </span>
                </div>
              )}
              {envelope?.meta.commandRecordId && (
                <div>
                  <span className="text-[9px] text-theme-text-muted/40 uppercase tracking-wider mr-1">{t('ai.tool_use.command_record', { defaultValue: 'Command record' })}</span>
                  <span className="text-[10px] text-theme-text-muted/65 font-mono">{envelope.meta.commandRecordId}</span>
                </div>
              )}
            </div>
          )}

          {warnings.length > 0 && (
            <div>
              <div className="text-[9px] text-amber-400/70 font-medium uppercase tracking-wider mb-0.5">
                {t('ai.tool_use.warnings')}
              </div>
              <div className="space-y-1">
                {warnings.map((warning, index) => (
                  <div key={`${warning}-${index}`} className="flex items-start gap-1 rounded-md bg-amber-500/10 border border-amber-500/20 px-1.5 py-1 text-[10px] text-amber-200/80">
                    <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                    <span>{warning}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {structuredData !== undefined && (
            <div>
              <div className="text-[9px] text-theme-text-muted/40 font-medium uppercase tracking-wider mb-0.5">
                {t('ai.tool_use.structured_data')}
              </div>
              <pre className="text-[10px] text-theme-text-muted/60 font-[family-name:var(--terminal-font-family)] bg-theme-bg/50 rounded-md px-1.5 py-1 overflow-x-auto max-h-[160px] overflow-y-auto whitespace-pre-wrap break-all">
                {formatJson(structuredData)}
              </pre>
            </div>
          )}

          {/* Arguments */}
          <div>
            <div className="text-[9px] text-theme-text-muted/40 font-medium uppercase tracking-wider mb-0.5">
              {t('ai.tool_use.arguments')}
            </div>
            <pre className="text-[10px] text-theme-text-muted/60 font-[family-name:var(--terminal-font-family)] bg-theme-bg/50 rounded-md px-1.5 py-1 overflow-x-auto max-h-[120px] overflow-y-auto whitespace-pre-wrap break-all">
              {formatArgsForDetails(call.arguments)}
            </pre>
          </div>

          {/* Output */}
          {hasOutput && (
            <div>
              <div className="text-[9px] text-theme-text-muted/40 font-medium uppercase tracking-wider mb-0.5">
                {t('ai.tool_use.raw_output')}
              </div>
              {call.result!.error && (
                <div className="text-[10px] text-red-400/80 font-[family-name:var(--terminal-font-family)] bg-red-500/5 rounded-md px-1.5 py-1 mb-1">
                  {call.result!.error}
                </div>
              )}
              {call.result!.output && (
                <pre className="text-[10px] text-theme-text-muted/60 font-[family-name:var(--terminal-font-family)] bg-theme-bg/50 rounded-md px-1.5 py-1 overflow-x-auto max-h-[200px] overflow-y-auto whitespace-pre-wrap break-all">
                  {displayedOutput}
                </pre>
              )}
              {shouldShowOutputToggle && (
                <button
                  type="button"
                  onClick={() => setShowRawOutput((value) => !value)}
                  className="text-[9px] text-theme-accent hover:text-theme-accent/80 mt-0.5"
                >
                  {outputToggleLabel}
                </button>
              )}
              {call.result!.truncated && (
                <div className="text-[9px] text-yellow-500/60 mt-0.5">
                  {canShowFullOutput
                    ? t('ai.tool_use.output_truncated_with_full', { defaultValue: 'Output was compacted for the model. Full output is stored for this chat.' })
                    : t('ai.tool_use.output_truncated_no_full', { defaultValue: 'Output was compacted; full output was too large to store.' })}
                  {outputStatsLabel ? ` (${outputStatsLabel})` : ''}
                </div>
              )}
            </div>
          )}

          {(sessionId || envelope?.error?.recoverable) && (
            <div>
              <div className="text-[9px] text-theme-text-muted/40 font-medium uppercase tracking-wider mb-0.5">
                {t('ai.tool_use.recovery_actions')}
              </div>
              <div className="flex flex-wrap gap-1">
                {sessionId && (
                  <button
                    type="button"
                    onClick={() => void navigator.clipboard?.writeText(sessionId)}
                    className="rounded border border-theme-border/30 bg-theme-bg/40 px-1.5 py-0.5 text-[10px] text-theme-text-muted/70 hover:text-theme-text"
                  >
                    {t('ai.tool_use.copy_session_id')}
                  </button>
                )}
                {envelope?.error?.recoverable && (
                  <span className="rounded border border-amber-500/25 bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-200/75">
                    {t('ai.tool_use.recoverable_error')}
                  </span>
                )}
              </div>
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
export const ToolCallBlock = memo(function ToolCallBlock({ toolCalls, toolRounds, turnParts }: ToolCallBlockProps) {
  const { t } = useTranslation();
  const [showEarly, setShowEarly] = useState(false);
  const normalizedToolCalls = normalizeToolCalls(toolCalls, toolRounds, turnParts);
  const latestRoundMarker = getLatestRoundMarker(toolRounds);

  if (normalizedToolCalls.length === 0) return null;

  // When 5+ tool calls, collapse the first N-3 behind a toggle
  const shouldCondense = normalizedToolCalls.length >= 5;
  const splitAt = shouldCondense ? Math.max(0, normalizedToolCalls.length - 3) : 0;
  const earlyCalls = shouldCondense ? normalizedToolCalls.slice(0, splitAt) : [];
  const recentCalls = shouldCondense ? normalizedToolCalls.slice(splitAt) : normalizedToolCalls;

  return (
    <div className="my-2 space-y-1">
      <div className="text-[10px] text-theme-text-muted/40 font-medium uppercase tracking-wider px-0.5">
        {t('ai.tool_use.heading')} ({normalizedToolCalls.length})
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
              'flex items-center gap-1.5 px-2 py-1 rounded-md w-full text-left',
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

      {latestRoundMarker === 'awaiting-summary' && (
        <div className="flex items-center gap-2 rounded-md border border-theme-border/20 bg-theme-bg-hover/20 px-2.5 py-2 text-[11px] text-theme-text-muted/60">
          <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-theme-accent/70" />
          <span>{t('ai.tool_use.awaiting_summary')}</span>
        </div>
      )}
    </div>
  );
});
