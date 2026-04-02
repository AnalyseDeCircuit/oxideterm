// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * AgentPanel — Independent tab for autonomous AI Agent
 *
 * Provides: task input, plan view, step log, approval bar, control bar.
 * Uses agentStore for state, agentOrchestrator for execution.
 */

import { useState, useRef, useEffect, useLayoutEffect, useMemo, useCallback, memo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Bot,
  Play,
  Pause,
  Square,
  Check,
  X,
  ChevronDown,
  ChevronRight,
  Loader2,
  CheckCircle2,
  XCircle,
  Terminal,
  FileText,
  Clock,
  Zap,
  Shield,
  ShieldAlert,
  ShieldCheck,
  ListChecks,
  History,
  RotateCcw,
  Trash2,
  Eye,
  ArrowLeft,
  FastForward,
  ScanSearch,
} from 'lucide-react';
import { useAgentStore } from '../../store/agentStore';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { runAgent } from '../../lib/ai/agentOrchestrator';
import { cn } from '../../lib/utils';
import { Button } from '../ui/button';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '../ui/select';
import { CustomRolesSection, PipelineSelector } from './AgentRoleEditor';
import type { AgentTask, AgentStep, AutonomyLevel, AgentRolesConfig, AgentPlan } from '../../types';

// ═══════════════════════════════════════════════════════════════════════════
// Autonomy Level Selector
// ═══════════════════════════════════════════════════════════════════════════

const AUTONOMY_CONFIG: Record<
  AutonomyLevel,
  { icon: React.ElementType; colorClass: string }
> = {
  supervised: { icon: Shield, colorClass: 'text-blue-400' },
  balanced: { icon: ShieldAlert, colorClass: 'text-amber-400' },
  autonomous: { icon: ShieldCheck, colorClass: 'text-green-400' },
};

const AutonomySelector = memo(() => {
  const { t } = useTranslation();
  const autonomyLevel = useAgentStore((s) => s.autonomyLevel);
  const setAutonomyLevel = useAgentStore((s) => s.setAutonomyLevel);
  const isRunning = useAgentStore((s) => s.isRunning);

  const levels: AutonomyLevel[] = ['supervised', 'balanced', 'autonomous'];

  return (
    <div className="flex items-center gap-1 rounded-lg bg-theme-bg-hover p-0.5">
      {levels.map((level) => {
        const { icon: Icon, colorClass } = AUTONOMY_CONFIG[level];
        const active = autonomyLevel === level;
        return (
          <button
            key={level}
            disabled={isRunning}
            onClick={() => setAutonomyLevel(level)}
            className={cn(
              'flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium transition-all',
              active
                ? 'bg-theme-bg-active shadow-sm text-theme-text'
                : 'text-theme-text-muted hover:text-theme-text',
              isRunning && 'opacity-50 cursor-not-allowed',
            )}
            title={t(`agent.autonomy.${level}_desc`)}
          >
            <Icon className={cn('w-3.5 h-3.5', active && colorClass)} />
            <span>{t(`agent.autonomy.${level}`)}</span>
          </button>
        );
      })}
    </div>
  );
});
AutonomySelector.displayName = 'AutonomySelector';

// ═══════════════════════════════════════════════════════════════════════════
// Agent Roles Configuration (Planner / Reviewer)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_ROLES: AgentRolesConfig = {
  planner: { enabled: false, providerId: null, model: null },
  reviewer: { enabled: false, providerId: null, model: null, interval: 5 },
};

const RoleModelSelect = memo(({
  label,
  enabled,
  providerId,
  model,
  onToggle,
  onChange,
  disabled,
  children,
}: {
  label: string;
  enabled: boolean;
  providerId: string | null;
  model: string | null;
  onToggle: (enabled: boolean) => void;
  onChange: (providerId: string, model: string) => void;
  disabled?: boolean;
  children?: React.ReactNode;
}) => {
  const providers = useSettingsStore((s) => s.settings.ai.providers);
  const enabledProviders = useMemo(
    () => providers.filter((p) => p.enabled),
    [providers],
  );

  const activeProvider = useMemo(
    () => enabledProviders.find((p) => p.id === providerId) || enabledProviders[0],
    [enabledProviders, providerId],
  );

  const models = useMemo(
    () => activeProvider?.models ?? [],
    [activeProvider],
  );

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2">
        <button
          onClick={() => onToggle(!enabled)}
          disabled={disabled}
          className={cn(
            'w-7 h-4 rounded-full relative transition-colors flex-shrink-0',
            enabled ? 'bg-theme-accent' : 'bg-theme-border',
            disabled && 'opacity-50 cursor-not-allowed',
          )}
          aria-label={label}
        >
          <span className={cn(
            'absolute top-0.5 w-3 h-3 rounded-full bg-white transition-transform',
            enabled ? 'translate-x-3.5' : 'translate-x-0.5',
          )} />
        </button>
        <span className="text-xs font-medium text-theme-text">{label}</span>
      </div>
      {enabled && (
        <div className="flex items-center gap-1.5 pl-9">
          <Select
            value={providerId || ''}
            onValueChange={(val) => {
              const p = enabledProviders.find((p) => p.id === val);
              onChange(val, p?.defaultModel || p?.models?.[0] || '');
            }}
            disabled={disabled}
          >
            <SelectTrigger className="h-6 text-[11px] min-w-0 flex-1">
              <SelectValue placeholder="Provider" />
            </SelectTrigger>
            <SelectContent>
              {enabledProviders.map((p) => (
                <SelectItem key={p.id} value={p.id}>{p.name || p.type}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select
            value={model || ''}
            onValueChange={(val) => onChange(providerId || activeProvider?.id || '', val)}
            disabled={disabled || models.length === 0}
          >
            <SelectTrigger className="h-6 text-[11px] min-w-0 flex-1 max-w-[140px]">
              <SelectValue placeholder="Model" />
            </SelectTrigger>
            <SelectContent>
              {models.map((m) => (
                <SelectItem key={m} value={m}>{m}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          {children}
        </div>
      )}
    </div>
  );
});
RoleModelSelect.displayName = 'RoleModelSelect';

const AgentRolesPanel = memo(() => {
  const { t } = useTranslation();
  const isRunning = useAgentStore((s) => s.isRunning);
  const agentRoles = useSettingsStore((s) => s.settings.ai.agentRoles) ?? DEFAULT_ROLES;
  const updateAi = useSettingsStore((s) => s.updateAi);
  const [expanded, setExpanded] = useState(false);

  const updateRole = useCallback(
    (patch: Partial<AgentRolesConfig>) => {
      updateAi('agentRoles', { ...DEFAULT_ROLES, ...agentRoles, ...patch });
    },
    [agentRoles, updateAi],
  );

  const hasActiveRole = agentRoles.planner.enabled || agentRoles.reviewer.enabled;

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1.5 text-[11px] text-theme-text-muted hover:text-theme-text transition-colors w-full"
      >
        {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        <span className="font-medium">{t('agent.roles.title')}</span>
        {hasActiveRole && <span className="w-1.5 h-1.5 rounded-full bg-theme-accent" />}
      </button>
      {expanded && (
        <div className="mt-2 space-y-3 pl-1">
          <RoleModelSelect
            label={t('agent.roles.planner')}
            enabled={agentRoles.planner.enabled}
            providerId={agentRoles.planner.providerId}
            model={agentRoles.planner.model}
            onToggle={(enabled) => updateRole({ planner: { ...agentRoles.planner, enabled } })}
            onChange={(pid, m) => updateRole({ planner: { ...agentRoles.planner, providerId: pid, model: m } })}
            disabled={isRunning}
          />
          <RoleModelSelect
            label={t('agent.roles.reviewer')}
            enabled={agentRoles.reviewer.enabled}
            providerId={agentRoles.reviewer.providerId}
            model={agentRoles.reviewer.model}
            onToggle={(enabled) => updateRole({ reviewer: { ...agentRoles.reviewer, enabled } })}
            onChange={(pid, m) => updateRole({ reviewer: { ...agentRoles.reviewer, providerId: pid, model: m } })}
            disabled={isRunning}
          >
            <div className="flex items-center gap-1 ml-1">
              <span className="text-[10px] text-theme-text-muted">{t('agent.roles.interval')}</span>
              <input
                type="number"
                min={1}
                max={20}
                value={agentRoles.reviewer.interval || 5}
                onChange={(e) => {
                  const val = Math.max(1, Math.min(20, parseInt(e.target.value) || 5));
                  updateRole({ reviewer: { ...agentRoles.reviewer, interval: val } });
                }}
                disabled={isRunning}
                className="w-10 h-6 text-[11px] text-center rounded border border-theme-border bg-theme-bg text-theme-text"
              />
            </div>
          </RoleModelSelect>
        </div>
      )}
    </div>
  );
});
AgentRolesPanel.displayName = 'AgentRolesPanel';

// ═══════════════════════════════════════════════════════════════════════════
// Task Input
// ═══════════════════════════════════════════════════════════════════════════

const TaskInput = memo(({ onStart }: { onStart: (goal: string) => void }) => {
  const { t } = useTranslation();
  const [goal, setGoal] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isRunning = useAgentStore((s) => s.isRunning);

  const handleSubmit = useCallback(() => {
    const trimmed = goal.trim();
    if (!trimmed || isRunning) return;
    onStart(trimmed);
    setGoal('');
  }, [goal, isRunning, onStart]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  // Auto-resize textarea
  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [goal]);

  return (
    <div className="flex flex-col gap-2">
      <textarea
        ref={textareaRef}
        value={goal}
        onChange={(e) => setGoal(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={t('agent.input.placeholder')}
        disabled={isRunning}
        rows={3}
        className={cn(
          'w-full resize-none rounded-lg border border-theme-border bg-theme-bg-hover px-3 py-2',
          'text-sm text-theme-text placeholder:text-theme-text-muted',
          'focus:outline-none focus:ring-1 focus:ring-theme-accent',
          'disabled:opacity-50 disabled:cursor-not-allowed',
        )}
      />
      <div className="flex items-center justify-between">
        <span className="text-xs text-theme-text-muted">
          {t('agent.input.shortcut')}
        </span>
        <Button
          size="sm"
          onClick={handleSubmit}
          disabled={!goal.trim() || isRunning}
          className="gap-1.5"
        >
          <Play className="w-3.5 h-3.5" />
          {t('agent.input.start')}
        </Button>
      </div>
    </div>
  );
});
TaskInput.displayName = 'TaskInput';

// ═══════════════════════════════════════════════════════════════════════════
// Plan View
// ═══════════════════════════════════════════════════════════════════════════

const PlanView = memo(({ task, allowSkip }: { task: AgentTask; allowSkip?: boolean }) => {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);
  const skipPlanStep = useAgentStore((s) => s.skipPlanStep);

  if (!task.plan) return null;

  const { steps, currentStepIndex } = task.plan;
  const isPaused = task.status === 'paused';

  return (
    <div className="border border-theme-border rounded-lg overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full px-3 py-2 text-sm font-medium text-theme-text bg-theme-bg-hover hover:bg-theme-bg-active transition-colors"
      >
        {expanded ? (
          <ChevronDown className="w-3.5 h-3.5" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5" />
        )}
        <ListChecks className="w-3.5 h-3.5 text-theme-accent" />
        {t('agent.plan.title')}
        <span className="ml-auto text-xs text-theme-text-muted">
          {currentStepIndex}/{steps.length}
        </span>
      </button>
      {expanded && (
        <div className="px-3 py-2 space-y-1">
          {steps.map((step, i) => {
            const isSkipped = step.status === 'skipped';
            const done = step.status === 'completed' || i < currentStepIndex;
            const active = i === currentStepIndex && !isSkipped;
            const canSkip = allowSkip && isPaused && !done && !isSkipped && i >= currentStepIndex;
            return (
              <div
                key={i}
                className={cn(
                  'flex items-start gap-2 text-xs py-1',
                  isSkipped && 'text-theme-text-muted line-through',
                  !isSkipped && done && 'text-theme-text-muted line-through',
                  !isSkipped && active && 'text-theme-text font-medium',
                  !isSkipped && !done && !active && 'text-theme-text-muted',
                )}
              >
                <span className="mt-0.5">
                  {isSkipped ? (
                    <X className="w-3.5 h-3.5 text-theme-text-muted" />
                  ) : done ? (
                    <CheckCircle2 className="w-3.5 h-3.5 text-green-400" />
                  ) : active ? (
                    <Loader2 className="w-3.5 h-3.5 text-theme-accent animate-spin" />
                  ) : (
                    <span className="inline-block w-3.5 h-3.5 rounded-full border border-current" />
                  )}
                </span>
                <span className="flex-1">{step.description}</span>
                {canSkip && (
                  <button
                    onClick={() => skipPlanStep(i)}
                    className="text-[10px] text-theme-text-muted hover:text-theme-text flex-shrink-0"
                    title={t('agent.plan.skip_step')}
                  >
                    {t('agent.plan.skip_step')}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
});
PlanView.displayName = 'PlanView';

// ═══════════════════════════════════════════════════════════════════════════
// Step Log
// ═══════════════════════════════════════════════════════════════════════════

const STEP_ICONS: Record<AgentStep['type'], React.ElementType> = {
  plan: ListChecks,
  tool_call: Terminal,
  observation: FileText,
  decision: Zap,
  error: XCircle,
  user_input: Shield,
  verify: CheckCircle2,
  review: ScanSearch,
};

const StepEntry = memo(({ step }: { step: AgentStep }) => {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const Icon = STEP_ICONS[step.type] ?? FileText;
  const isLong = step.content.length > 200;

  return (
    <div
      className={cn(
        'border-l-2 pl-3 py-1.5',
        step.status === 'completed' && 'border-green-500/40',
        step.status === 'running' && 'border-theme-accent',
        step.status === 'error' && 'border-red-500/40',
        step.status === 'skipped' && 'border-theme-border',
        step.status === 'pending' && 'border-theme-border/40',
      )}
    >
      <div className="flex items-center gap-2">
        {step.status === 'running' ? (
          <Loader2 className="w-3.5 h-3.5 text-theme-accent animate-spin flex-shrink-0" />
        ) : (
          <Icon className={cn('w-3.5 h-3.5 flex-shrink-0', step.status === 'error' ? 'text-red-400' : 'text-theme-text-muted')} />
        )}
        <span className="text-xs font-medium text-theme-text-muted">
          {t(`agent.step.${step.type}`)}
        </span>
        {step.toolCall && (
          <code className="text-xs px-1.5 py-0.5 rounded bg-theme-bg-hover text-theme-accent font-mono">
            {step.toolCall.name}
          </code>
        )}
        {step.durationMs != null && (
          <span className="text-[10px] text-theme-text-muted flex items-center gap-0.5 ml-auto">
            <Clock className="w-3 h-3" />
            {step.durationMs < 1000
              ? `${step.durationMs}ms`
              : `${(step.durationMs / 1000).toFixed(1)}s`}
          </span>
        )}
      </div>
      {step.content && (
        <div className="mt-1">
          <pre
            className={cn(
              'text-xs whitespace-pre-wrap break-words text-theme-text-muted font-mono',
              !expanded && isLong && 'line-clamp-4',
            )}
          >
            {step.content}
          </pre>
          {isLong && (
            <button
              onClick={() => setExpanded(!expanded)}
              className="text-[10px] text-theme-accent hover:underline mt-0.5"
            >
              {expanded ? t('agent.step.collapse') : t('agent.step.expand')}
            </button>
          )}
        </div>
      )}
    </div>
  );
});
StepEntry.displayName = 'StepEntry';

const StepLog = memo(({ steps }: { steps: AgentStep[] }) => {
  const { t } = useTranslation();
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom on new steps
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [steps.length]);

  if (steps.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-theme-text-muted">
        <Bot className="w-10 h-10 opacity-20 mb-3" />
        <p className="text-sm">{t('agent.log.empty')}</p>
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      {steps.map((step) => (
        <StepEntry key={step.id} step={step} />
      ))}
      <div ref={bottomRef} />
    </div>
  );
});
StepLog.displayName = 'StepLog';

// ═══════════════════════════════════════════════════════════════════════════
// Approval Bar
// ═══════════════════════════════════════════════════════════════════════════

const ApprovalItem = memo(({ approval, onResolve, onSkip }: {
  approval: { id: string; toolName: string; arguments: string; reasoning?: string };
  onResolve: (id: string, approved: boolean) => void;
  onSkip: (id: string) => void;
}) => {
  const { t } = useTranslation();
  const [argsExpanded, setArgsExpanded] = useState(false);
  const isLong = approval.arguments.length > 80;

  return (
    <div className="rounded-md bg-theme-bg-hover px-3 py-2 space-y-1.5">
      <div className="flex items-center gap-2">
        <Terminal className="w-3.5 h-3.5 text-theme-text-muted flex-shrink-0" />
        <code className="text-xs font-mono text-theme-text flex-1 truncate">
          {approval.toolName}
        </code>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => onSkip(approval.id)}
          className="h-6 px-1.5 text-theme-text-muted hover:text-theme-text"
          title={t('agent.approval.skip')}
        >
          <span className="text-[10px]">{t('agent.approval.skip')}</span>
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => onResolve(approval.id, false)}
          className="h-6 w-6 p-0 text-red-400 hover:text-red-300"
          aria-label={t('agent.approval.reject_all')}
        >
          <X className="w-3.5 h-3.5" />
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => onResolve(approval.id, true)}
          className="h-6 w-6 p-0 text-green-400 hover:text-green-300"
          aria-label={t('agent.approval.approve_all')}
        >
          <Check className="w-3.5 h-3.5" />
        </Button>
      </div>
      {/* Expandable arguments */}
      <div className="pl-5">
        <button
          onClick={() => setArgsExpanded(!argsExpanded)}
          className="text-[10px] text-theme-text-muted hover:text-theme-text flex items-center gap-1"
        >
          {argsExpanded ? <ChevronDown className="w-2.5 h-2.5" /> : <ChevronRight className="w-2.5 h-2.5" />}
          {t('agent.approval.viewArgs')}
        </button>
        {argsExpanded && (
          <pre className="text-[10px] font-mono text-theme-text-muted mt-1 p-1.5 rounded bg-theme-bg whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
            {approval.arguments}
          </pre>
        )}
        {!argsExpanded && isLong && (
          <code className="text-[10px] font-mono text-theme-text-muted block truncate">
            {approval.arguments.slice(0, 80)}...
          </code>
        )}
        {!argsExpanded && !isLong && (
          <code className="text-[10px] font-mono text-theme-text-muted block truncate">
            {approval.arguments}
          </code>
        )}
      </div>
      {approval.reasoning && (
        <p className="text-[11px] text-theme-text-muted pl-5 line-clamp-2">
          {approval.reasoning}
        </p>
      )}
    </div>
  );
});
ApprovalItem.displayName = 'ApprovalItem';

const ApprovalBar = memo(() => {
  const { t } = useTranslation();
  const pendingApprovals = useAgentStore((s) => s.pendingApprovals);
  const resolveApproval = useAgentStore((s) => s.resolveApproval);
  const skipApproval = useAgentStore((s) => s.skipApproval);
  const resolveAllApprovals = useAgentStore((s) => s.resolveAllApprovals);

  if (pendingApprovals.length === 0) return null;

  return (
    <div className="border border-amber-500/30 rounded-lg bg-amber-500/5 p-3 space-y-2">
      <div className="flex items-center gap-2 text-sm font-medium text-amber-400">
        <ShieldAlert className="w-4 h-4" />
        {t('agent.approval.title', { count: pendingApprovals.length })}
      </div>
      {pendingApprovals.map((approval) => (
        <ApprovalItem
          key={approval.id}
          approval={approval}
          onResolve={resolveApproval}
          onSkip={skipApproval}
        />
      ))}
      {pendingApprovals.length > 1 && (
        <div className="flex gap-2 justify-end">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => resolveAllApprovals(false)}
            className="text-xs text-red-400"
          >
            {t('agent.approval.reject_all')}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => resolveAllApprovals(true)}
            className="text-xs text-green-400"
          >
            {t('agent.approval.approve_all')}
          </Button>
        </div>
      )}
    </div>
  );
});
ApprovalBar.displayName = 'ApprovalBar';

// ═══════════════════════════════════════════════════════════════════════════
// Control Bar
// ═══════════════════════════════════════════════════════════════════════════

const ControlBar = memo(() => {
  const { t } = useTranslation();
  const activeTask = useAgentStore((s) => s.activeTask);
  const pauseTask = useAgentStore((s) => s.pauseTask);
  const resumeTask = useAgentStore((s) => s.resumeTask);
  const cancelTask = useAgentStore((s) => s.cancelTask);
  const autonomyLevel = useAgentStore((s) => s.autonomyLevel);
  const setAutonomyLevel = useAgentStore((s) => s.setAutonomyLevel);

  if (!activeTask) return null;

  const isPaused = activeTask.status === 'paused';
  const isActive = activeTask.status === 'executing' || activeTask.status === 'planning' || activeTask.status === 'awaiting_approval';
  const AutonomyIcon = AUTONOMY_CONFIG[autonomyLevel].icon;

  return (
    <div className="space-y-2 pt-2 border-t border-theme-border">
      <div className="flex items-center gap-3">
        {/* Progress */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between text-xs text-theme-text-muted mb-1">
            <span>{t(`agent.status.${activeTask.status}`)}</span>
            <span>
              {t('agent.control.round', {
                current: activeTask.currentRound,
                max: activeTask.maxRounds,
              })}
            </span>
          </div>
          <div className="h-1 rounded-full bg-theme-bg-hover overflow-hidden">
            <div
              className={cn(
                'h-full rounded-full transition-all',
                activeTask.status === 'failed' ? 'bg-red-500' : 'bg-theme-accent',
              )}
              style={{
                width: `${Math.min(100, (activeTask.currentRound / activeTask.maxRounds) * 100)}%`,
              }}
            />
          </div>
        </div>

        {/* Mid-task autonomy switch */}
        <button
          onClick={() => {
            const levels: AutonomyLevel[] = ['supervised', 'balanced', 'autonomous'];
            const idx = levels.indexOf(autonomyLevel);
            setAutonomyLevel(levels[(idx + 1) % levels.length]);
          }}
          className={cn('h-7 w-7 p-0 flex items-center justify-center rounded', AUTONOMY_CONFIG[autonomyLevel].colorClass)}
          title={t('agent.control.switchAutonomy', { level: t(`agent.autonomy.${autonomyLevel}`) })}
        >
          <AutonomyIcon className="w-3.5 h-3.5" />
        </button>

        {/* Actions */}
        {isActive && (
          <>
            <Button
              size="sm"
              variant="ghost"
              onClick={pauseTask}
              className="h-7 w-7 p-0"
              title={t('agent.control.pause')}
            >
              <Pause className="w-4 h-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={cancelTask}
              className="h-7 w-7 p-0 text-red-400"
              title={t('agent.control.cancel')}
            >
              <Square className="w-4 h-4" />
            </Button>
          </>
        )}
        {isPaused && (
          <>
            <Button
              size="sm"
              variant="ghost"
              onClick={resumeTask}
              className="h-7 w-7 p-0 text-green-400"
              title={t('agent.control.resume')}
            >
              <Play className="w-4 h-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={cancelTask}
              className="h-7 w-7 p-0 text-red-400"
              title={t('agent.control.cancel')}
            >
              <Square className="w-4 h-4" />
            </Button>
          </>
        )}
      </div>
    </div>
  );
});
ControlBar.displayName = 'ControlBar';

// ═══════════════════════════════════════════════════════════════════════════
// Task Summary (shown when completed/failed)
// ═══════════════════════════════════════════════════════════════════════════

const TaskSummary = memo(({ task }: { task: AgentTask }) => {
  const { t } = useTranslation();

  if (task.status !== 'completed' && task.status !== 'failed') return null;

  return (
    <div
      className={cn(
        'rounded-lg border p-3',
        task.status === 'completed'
          ? 'border-green-500/30 bg-green-500/5'
          : 'border-red-500/30 bg-red-500/5',
      )}
    >
      <div className="flex items-center gap-2 mb-1">
        {task.status === 'completed' ? (
          <CheckCircle2 className="w-4 h-4 text-green-400" />
        ) : (
          <XCircle className="w-4 h-4 text-red-400" />
        )}
        <span className="text-sm font-medium text-theme-text">
          {task.status === 'completed' ? t('agent.summary.completed') : t('agent.summary.failed')}
        </span>
      </div>
      {task.summary && (
        <p className="text-xs text-theme-text-muted whitespace-pre-wrap">{task.summary}</p>
      )}
      {task.error && (
        <p className="text-xs text-red-400 mt-1">{task.error}</p>
      )}
    </div>
  );
});
TaskSummary.displayName = 'TaskSummary';

// ═══════════════════════════════════════════════════════════════════════════
// Task History
// ═══════════════════════════════════════════════════════════════════════════

const TaskHistory = memo(({ onRerun, onResume, onRerunWithPlan }: { onRerun: (goal: string) => void; onResume: (taskId: string, fromRound?: number) => void; onRerunWithPlan: (goal: string, plan: AgentPlan) => void }) => {
  const { t } = useTranslation();
  const taskHistory = useAgentStore((s) => s.taskHistory);
  const viewingTask = useAgentStore((s) => s.viewingTask);
  const isRunning = useAgentStore((s) => s.isRunning);
  const setViewingTask = useAgentStore((s) => s.setViewingTask);
  const removeFromHistory = useAgentStore((s) => s.removeFromHistory);
  const clearHistory = useAgentStore((s) => s.clearHistory);
  const [expanded, setExpanded] = useState(false);

  if (taskHistory.length === 0 && !viewingTask) return null;

  // Replay view — show the selected historical task's steps
  if (viewingTask) {
    return (
      <div className="border-t border-theme-border pt-3">
        <div className="flex items-center gap-2 mb-2">
          <button
            onClick={() => setViewingTask(null)}
            className="flex items-center gap-1 text-xs text-theme-accent hover:text-theme-accent-hover transition-colors"
          >
            <ArrowLeft className="w-3 h-3" />
            {t('agent.history.back')}
          </button>
          <span className="text-xs text-theme-text-muted flex-1 truncate">{viewingTask.goal}</span>
        </div>
        <div className="mb-2 flex items-center gap-2 text-xs text-theme-text-muted">
          {viewingTask.status === 'completed' ? (
            <CheckCircle2 className="w-3 h-3 text-green-400" />
          ) : (
            <XCircle className="w-3 h-3 text-red-400" />
          )}
          <span>{t(`agent.status.${viewingTask.status}`)}</span>
          <span className="text-[10px]">{t('agent.history.rounds', { count: viewingTask.currentRound })}</span>
          {viewingTask.completedAt && (
            <span className="text-[10px]">{new Date(viewingTask.completedAt).toLocaleString()}</span>
          )}
        </div>
        {viewingTask.summary && (
          <div className="text-xs text-theme-text bg-theme-bg-hover rounded-md px-2.5 py-2 mb-2 whitespace-pre-wrap">
            {viewingTask.summary}
          </div>
        )}
        {viewingTask.plan && <PlanView task={viewingTask} />}
        <StepLog steps={viewingTask.steps} />
        {!isRunning && (
          <div className="mt-2 flex items-center gap-3">
            <button
              onClick={() => {
                if (useAgentStore.getState().isRunning) return;
                setViewingTask(null);
                onRerun(viewingTask.goal);
              }}
              className="flex items-center gap-1.5 text-xs text-theme-accent hover:text-theme-accent-hover transition-colors"
              aria-label={t('agent.history.rerun')}
            >
              <RotateCcw className="w-3 h-3" />
              {t('agent.history.rerun')}
            </button>
            {(viewingTask.status === 'failed' || viewingTask.status === 'cancelled') && viewingTask.steps.length > 0 && (
              <button
                onClick={() => {
                  if (useAgentStore.getState().isRunning) return;
                  setViewingTask(null);
                  onResume(viewingTask.id);
                }}
                className="flex items-center gap-1.5 text-xs text-theme-accent hover:text-theme-accent-hover transition-colors"
                aria-label={t('agent.history.resume')}
              >
                <FastForward className="w-3 h-3" />
                {t('agent.history.resume')}
              </button>
            )}
            {viewingTask.plan && (
              <button
                onClick={() => {
                  if (useAgentStore.getState().isRunning) return;
                  setViewingTask(null);
                  onRerunWithPlan(viewingTask.goal, viewingTask.plan!);
                }}
                className="flex items-center gap-1.5 text-xs text-theme-accent hover:text-theme-accent-hover transition-colors"
                aria-label={t('agent.history.rerunWithPlan')}
              >
                <ListChecks className="w-3 h-3" />
                {t('agent.history.rerunWithPlan')}
              </button>
            )}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="border-t border-theme-border pt-3">
      <div className="flex items-center w-full">
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-2 text-xs font-medium text-theme-text-muted hover:text-theme-text transition-colors flex-1"
        >
          <History className="w-3.5 h-3.5" />
          {t('agent.history.title', { count: taskHistory.length })}
          {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        </button>
        {expanded && taskHistory.length > 0 && !isRunning && (
          <button
            onClick={clearHistory}
            className="text-[10px] text-theme-text-muted hover:text-red-400 transition-colors ml-2"
            title={t('agent.history.clear')}
          >
            <Trash2 className="w-3 h-3" />
          </button>
        )}
      </div>
      {expanded && (
        <div className="mt-2 space-y-1">
          {taskHistory.map((task) => (
            <div
              key={task.id}
              className="group flex items-center gap-2 text-xs text-theme-text-muted bg-theme-bg-hover rounded-md px-2.5 py-1.5 hover:bg-theme-bg-active transition-colors"
            >
              {task.status === 'completed' ? (
                <CheckCircle2 className="w-3 h-3 text-green-400 flex-shrink-0" />
              ) : (
                <XCircle className="w-3 h-3 text-red-400 flex-shrink-0" />
              )}
              <span className="flex-1 truncate">{task.goal}</span>
              <span className="text-[10px] flex-shrink-0 group-hover:hidden">
                {task.completedAt ? new Date(task.completedAt).toLocaleTimeString() : ''}
              </span>
              <div className="hidden group-hover:flex items-center gap-1 flex-shrink-0">
                <button
                  onClick={() => setViewingTask(task)}
                  className="p-0.5 hover:text-theme-accent transition-colors"
                  title={t('agent.history.view')}
                >
                  <Eye className="w-3 h-3" />
                </button>
                {!isRunning && (
                  <button
                    onClick={() => onRerun(task.goal)}
                    className="p-0.5 hover:text-theme-accent transition-colors"
                    title={t('agent.history.rerun')}
                  >
                    <RotateCcw className="w-3 h-3" />
                  </button>
                )}
                <button
                  onClick={() => removeFromHistory(task.id)}
                  className={cn(
                    'p-0.5 transition-colors',
                    isRunning ? 'opacity-50 cursor-not-allowed' : 'hover:text-red-400',
                  )}
                  disabled={isRunning}
                  title={t('agent.history.delete')}
                >
                  <Trash2 className="w-3 h-3" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
});
TaskHistory.displayName = 'TaskHistory';

// ═══════════════════════════════════════════════════════════════════════════
// Provider/Model Selector (simplified for agent use)
// ═══════════════════════════════════════════════════════════════════════════

const ProviderModelSelect = memo(({
  providerId,
  model,
  onChange,
  disabled,
}: {
  providerId: string;
  model: string;
  onChange: (providerId: string, model: string) => void;
  disabled?: boolean;
}) => {
  const providers = useSettingsStore((s) => s.settings.ai.providers);
  const enabledProviders = useMemo(
    () => providers.filter((p) => p.enabled),
    [providers],
  );

  const activeProvider = useMemo(
    () => enabledProviders.find((p) => p.id === providerId) || enabledProviders[0],
    [enabledProviders, providerId],
  );

  const models = useMemo(
    () => activeProvider?.models ?? [],
    [activeProvider],
  );

  // Auto-select first provider/model if not set
  useEffect(() => {
    if (!activeProvider) return;
    if (providerId !== activeProvider.id || (!model && models.length > 0)) {
      onChange(activeProvider.id, model || activeProvider.defaultModel || models[0] || '');
    }
  }, [activeProvider, providerId, model, models, onChange]);

  return (
    <div className="flex items-center gap-2 text-xs">
      <Select
        value={providerId}
        onValueChange={(val) => {
          const p = enabledProviders.find((p) => p.id === val);
          onChange(
            val,
            p?.defaultModel || p?.models?.[0] || '',
          );
        }}
        disabled={disabled}
      >
        <SelectTrigger className="h-7 text-xs min-w-0">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {enabledProviders.map((p) => (
            <SelectItem key={p.id} value={p.id}>
              {p.name || p.type}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select
        value={model}
        onValueChange={(val) => onChange(providerId, val)}
        disabled={disabled || models.length === 0}
      >
        <SelectTrigger className="h-7 text-xs min-w-0 max-w-[200px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {models.map((m) => (
            <SelectItem key={m} value={m}>
              {m}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
});
ProviderModelSelect.displayName = 'ProviderModelSelect';

// ═══════════════════════════════════════════════════════════════════════════
// Main AgentPanel
// ═══════════════════════════════════════════════════════════════════════════

export const AgentPanel = () => {
  const { t } = useTranslation();
  const activeTask = useAgentStore((s) => s.activeTask);
  const startTask = useAgentStore((s) => s.startTask);
  const isRunning = useAgentStore((s) => s.isRunning);

  // Local provider / model selection
  const aiSettings = useSettingsStore((s) => s.settings.ai);
  const [providerId, setProviderId] = useState(aiSettings.activeProviderId || '');
  const [model, setModel] = useState(aiSettings.activeModel || '');

  const handleProviderModelChange = useCallback(
    (pid: string, m: string) => {
      setProviderId(pid);
      setModel(m);
    },
    [],
  );

  const handleStart = useCallback(
    (goal: string) => {
      if (!providerId || !model) return;
      const contextTabType = useAppStore.getState().lastNonAgentTabType ?? 'terminal';
      const task = startTask(goal, providerId, model, contextTabType);
      // Get the abort controller from the store
      const controller = useAgentStore.getState().abortController;
      if (controller) {
        runAgent(task, controller.signal).catch((err) => {
          if (err instanceof DOMException && err.name === 'AbortError') return;
          useAgentStore.getState().setTaskError(
            err instanceof Error ? err.message : String(err),
          );
        });
      }
    },
    [providerId, model, startTask],
  );

  const handleResume = useCallback(
    (taskId: string, fromRound?: number) => {
      if (useAgentStore.getState().isRunning) return;
      const resumeHistoryTask = useAgentStore.getState().resumeHistoryTask;
      const newTask = resumeHistoryTask(taskId, fromRound);
      if (!newTask) return;
      const controller = useAgentStore.getState().abortController;
      if (controller) {
        runAgent(newTask, controller.signal).catch((err) => {
          if (err instanceof DOMException && err.name === 'AbortError') return;
          useAgentStore.getState().setTaskError(
            err instanceof Error ? err.message : String(err),
          );
        });
      }
    },
    [],
  );

  const handleRerunWithPlan = useCallback(
    (goal: string, plan: AgentPlan) => {
      if (!providerId || !model) return;
      const contextTabType = useAppStore.getState().lastNonAgentTabType ?? 'terminal';
      const task = startTask(goal, providerId, model, contextTabType, plan);
      const controller = useAgentStore.getState().abortController;
      if (controller) {
        runAgent(task, controller.signal).catch((err) => {
          if (err instanceof DOMException && err.name === 'AbortError') return;
          useAgentStore.getState().setTaskError(
            err instanceof Error ? err.message : String(err),
          );
        });
      }
    },
    [providerId, model, startTask],
  );

  return (
    <div className="flex flex-col h-full bg-theme-bg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-theme-border">
        <div className="flex items-center gap-2">
          <Bot className="w-5 h-5 text-theme-accent" />
          <h2 className="text-sm font-semibold text-theme-text">
            {t('agent.title')}
          </h2>
        </div>
        <ProviderModelSelect
          providerId={providerId}
          model={model}
          onChange={handleProviderModelChange}
          disabled={isRunning}
        />
      </div>

      {/* Autonomy Selector + Roles */}
      <div className="px-4 py-2 border-b border-theme-border space-y-2">
        <AutonomySelector />
        <AgentRolesPanel />
        <PipelineSelector />
        <CustomRolesSection />
      </div>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
        {activeTask && <PlanView task={activeTask} allowSkip />}
        <StepLog steps={activeTask?.steps ?? []} />
        {activeTask && <TaskSummary task={activeTask} />}
        <TaskHistory onRerun={handleStart} onResume={handleResume} onRerunWithPlan={handleRerunWithPlan} />
      </div>

      {/* Footer */}
      <div className="px-4 py-3 border-t border-theme-border space-y-3">
        <ApprovalBar />
        {activeTask && (activeTask.status === 'executing' || activeTask.status === 'planning' || activeTask.status === 'paused' || activeTask.status === 'awaiting_approval') && (
          <ControlBar />
        )}
        {!isRunning && <TaskInput onStart={handleStart} />}
      </div>
    </div>
  );
};
