// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, GripVertical, Plus, Trash2, Wand2 } from 'lucide-react';
import {
  DndContext,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  arrayMove,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import {
  MAX_HIGHLIGHT_RULES,
  buildRuntimeHighlightRules,
  createDefaultHighlightRule,
  matchCompiledPatternSync,
  reindexHighlightRules,
  type RuntimeHighlightRule,
} from '@/lib/terminal/highlightPattern';
import { cn } from '@/lib/utils';
import type { HighlightRule, HighlightRuleRenderMode } from '@/types';

type TerminalHighlightRulesSectionProps = {
  rules: HighlightRule[];
  updateRules: (rules: HighlightRule[]) => void;
};

type RuleRowProps = {
  rule: HighlightRule;
  runtimeRule?: RuntimeHighlightRule;
  onChange: (patch: Partial<HighlightRule>) => void;
  onDelete: () => void;
};

type HighlightPresetGroup = 'logs' | 'network' | 'system' | 'identity';

type HighlightPresetItem = {
  id: string;
  label: string;
  shortcut: string;
  group: HighlightPresetGroup;
  onSelect: () => void;
};

function renderPreviewLine(line: string, rules: RuntimeHighlightRule[]) {
  const candidates = rules
    .filter((rule) => rule.enabled && rule.compiled)
    .flatMap((rule) => matchCompiledPatternSync(rule.compiled!, line).map((match) => ({
      rule,
      index: match.index,
      length: match.length,
    })))
    .sort((left, right) => {
      if (right.rule.normalizedPriority !== left.rule.normalizedPriority) {
        return right.rule.normalizedPriority - left.rule.normalizedPriority;
      }
      if (left.index !== right.index) {
        return left.index - right.index;
      }
      return right.length - left.length;
    });

  const accepted: typeof candidates = [];
  for (const candidate of candidates) {
    const candidateEnd = candidate.index + candidate.length;
    if (accepted.some((existing) => candidate.index < existing.index + existing.length && candidateEnd > existing.index)) {
      continue;
    }
    accepted.push(candidate);
  }

  accepted.sort((left, right) => left.index - right.index);

  const segments: Array<{ text: string; rule?: RuntimeHighlightRule; key: string }> = [];
  let cursor = 0;
  accepted.forEach((match, index) => {
    if (match.index > cursor) {
      segments.push({ text: line.slice(cursor, match.index), key: `plain-${index}-${cursor}` });
    }
    segments.push({
      text: line.slice(match.index, match.index + match.length),
      rule: match.rule,
      key: `rule-${match.rule.id}-${index}`,
    });
    cursor = match.index + match.length;
  });
  if (cursor < line.length) {
    segments.push({ text: line.slice(cursor), key: `tail-${cursor}` });
  }

  return segments.map((segment) => {
    if (!segment.rule) {
      return <span key={segment.key}>{segment.text}</span>;
    }

    const mode = segment.rule.renderMode ?? 'background';
    const style = mode === 'background'
      ? {
          background: segment.rule.background,
          color: segment.rule.foreground,
        }
      : mode === 'underline'
        ? {
            borderBottom: `2px solid ${segment.rule.background ?? segment.rule.foreground ?? '#f59e0b'}`,
            color: segment.rule.foreground,
          }
        : {
            outline: `1px solid ${segment.rule.background ?? segment.rule.foreground ?? '#f59e0b'}`,
            outlineOffset: '1px',
            color: segment.rule.foreground,
          };

    return (
      <span key={segment.key} className="rounded-[2px] px-0.5" style={style}>
        {segment.text}
      </span>
    );
  });
}

function SortableRuleRow({ rule, runtimeRule, onChange, onDelete }: RuleRowProps) {
  const { t } = useTranslation();
  const [foregroundDraft, setForegroundDraft] = useState(rule.foreground ?? '');
  const [backgroundDraft, setBackgroundDraft] = useState(rule.background ?? '');
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: rule.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  useEffect(() => {
    setForegroundDraft(rule.foreground ?? '');
  }, [rule.foreground]);

  useEffect(() => {
    setBackgroundDraft(rule.background ?? '');
  }, [rule.background]);

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        'rounded-lg border border-theme-border bg-theme-bg-sunken p-4',
        isDragging && 'opacity-70 shadow-xl',
      )}
    >
      <div className="flex items-start gap-3">
        <button
          type="button"
          className="mt-1 cursor-grab rounded-md p-1 text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text active:cursor-grabbing"
          {...attributes}
          {...listeners}
          aria-label={t('settings_view.terminal.highlight_rules.drag_rule')}
        >
          <GripVertical className="h-4 w-4" />
        </button>

        <div className="flex-1 space-y-3">
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)_140px]">
            <div>
              <Label className="text-theme-text">{t('settings_view.terminal.highlight_rules.label')}</Label>
              <Input
                value={rule.label}
                onChange={(event) => onChange({ label: event.target.value })}
                placeholder={t('settings_view.terminal.highlight_rules.label_placeholder')}
                className="mt-1"
              />
            </div>

            <div>
              <Label className="text-theme-text">{t('settings_view.terminal.highlight_rules.pattern')}</Label>
              <Input
                value={rule.pattern}
                onChange={(event) => onChange({ pattern: event.target.value })}
                placeholder={t('settings_view.terminal.highlight_rules.pattern_placeholder')}
                className="mt-1 font-mono text-xs"
              />
            </div>

            <div>
              <Label className="text-theme-text">{t('settings_view.terminal.highlight_rules.render_mode')}</Label>
              <Select value={rule.renderMode ?? 'background'} onValueChange={(value) => onChange({ renderMode: value as HighlightRuleRenderMode })}>
                <SelectTrigger className="mt-1">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="background">{t('settings_view.terminal.highlight_rules.render_mode_background')}</SelectItem>
                  <SelectItem value="underline">{t('settings_view.terminal.highlight_rules.render_mode_underline')}</SelectItem>
                  <SelectItem value="outline">{t('settings_view.terminal.highlight_rules.render_mode_outline')}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="grid gap-3 md:grid-cols-[repeat(2,minmax(0,1fr))_repeat(4,auto)] md:items-end">
            <div>
              <Label className="text-theme-text">{t('settings_view.terminal.highlight_rules.foreground')}</Label>
              <div className="mt-1 flex items-center gap-2">
                <Input
                  type="color"
                  value={rule.foreground ?? '#f8fafc'}
                  onChange={(event) => {
                    setForegroundDraft(event.target.value);
                    onChange({ foreground: event.target.value });
                  }}
                  className="h-10 w-14 p-1"
                />
                <Input
                  value={foregroundDraft}
                  onChange={(event) => setForegroundDraft(event.target.value)}
                  onBlur={() => onChange({ foreground: foregroundDraft })}
                  placeholder="#f8fafc"
                  className="font-mono text-xs"
                />
              </div>
            </div>

            <div>
              <Label className="text-theme-text">{t('settings_view.terminal.highlight_rules.background')}</Label>
              <div className="mt-1 flex items-center gap-2">
                <Input
                  type="color"
                  value={rule.background ?? '#991b1b'}
                  onChange={(event) => {
                    setBackgroundDraft(event.target.value);
                    onChange({ background: event.target.value });
                  }}
                  className="h-10 w-14 p-1"
                />
                <Input
                  value={backgroundDraft}
                  onChange={(event) => setBackgroundDraft(event.target.value)}
                  onBlur={() => onChange({ background: backgroundDraft })}
                  placeholder="#991b1b"
                  className="font-mono text-xs"
                />
              </div>
            </div>

            <label className="flex items-center gap-2 text-xs text-theme-text-muted">
              <Checkbox checked={rule.enabled} onCheckedChange={(checked) => onChange({ enabled: checked as boolean })} />
              {t('settings_view.terminal.highlight_rules.enabled')}
            </label>

            <label className="flex items-center gap-2 text-xs text-theme-text-muted">
              <Checkbox checked={rule.isRegex} onCheckedChange={(checked) => onChange({ isRegex: checked as boolean })} />
              {t('settings_view.terminal.highlight_rules.regex')}
            </label>

            <label className="flex items-center gap-2 text-xs text-theme-text-muted">
              <Checkbox checked={rule.caseSensitive} onCheckedChange={(checked) => onChange({ caseSensitive: checked as boolean })} />
              {t('settings_view.terminal.highlight_rules.case_sensitive')}
            </label>

            <Button type="button" variant="ghost" size="sm" className="justify-self-start text-theme-error hover:bg-theme-error/10 hover:text-theme-error" onClick={onDelete}>
              <Trash2 className="mr-1 h-4 w-4" />
              {t('settings_view.terminal.highlight_rules.delete')}
            </Button>
          </div>

          <div className="flex items-center justify-between gap-3 text-xs">
            <div className="text-theme-text-muted">
              {runtimeRule?.lastValidationError
                ? t(`settings_view.terminal.highlight_rules.validation.${runtimeRule.lastValidationError}`)
                : t(`settings_view.terminal.highlight_rules.mode_hint.${rule.isRegex ? 'regex' : 'literal'}`)}
            </div>
            {runtimeRule?.lastValidationError ? (
              <div className="flex items-center gap-1 text-amber-400">
                <AlertTriangle className="h-3.5 w-3.5" />
                {t('settings_view.terminal.highlight_rules.invalid_rule')}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

export const TerminalHighlightRulesSection = ({ rules, updateRules }: TerminalHighlightRulesSectionProps) => {
  const { t } = useTranslation();
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const runtimeRules = useMemo(() => buildRuntimeHighlightRules(rules), [rules]);
  const runtimeRuleById = useMemo(() => new Map(runtimeRules.map((rule) => [rule.id, rule])), [runtimeRules]);
  const sortableIds = useMemo(() => rules.map((rule) => rule.id), [rules]);
  const previewSampleLines = useMemo(() => [
    t('settings_view.terminal.highlight_rules.preview_line_error'),
    t('settings_view.terminal.highlight_rules.preview_line_warning'),
    t('settings_view.terminal.highlight_rules.preview_line_ok'),
    t('settings_view.terminal.highlight_rules.preview_line_trace'),
    t('settings_view.terminal.highlight_rules.preview_line_audit'),
  ], [t]);

  const setRules = (nextRules: HighlightRule[]) => {
    updateRules(reindexHighlightRules(nextRules));
  };

  const addRule = (rule?: HighlightRule) => {
    if (rules.length >= MAX_HIGHLIGHT_RULES) {
      return;
    }
    setRules([...rules, rule ?? createDefaultHighlightRule()]);
  };

  const addStatusPreset = () => {
    setRules([
      ...rules,
      createDefaultHighlightRule({
        label: t('settings_view.terminal.highlight_rules.preset_label_error'),
        pattern: 'error',
        foreground: '#ffffff',
        background: '#b91c1c',
      }),
      createDefaultHighlightRule({
        label: t('settings_view.terminal.highlight_rules.preset_label_warning'),
        pattern: 'warning',
        foreground: '#111827',
        background: '#f59e0b',
      }),
      createDefaultHighlightRule({
        label: t('settings_view.terminal.highlight_rules.preset_label_ok'),
        pattern: 'OK',
        foreground: '#052e16',
        background: '#4ade80',
      }),
    ].slice(0, MAX_HIGHLIGHT_RULES));
  };

  const addIpPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_ip'),
      pattern: String.raw`\b(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}\b`,
      isRegex: true,
      background: '#1d4ed8',
      foreground: '#eff6ff',
    }));
  };

  const addMacPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_mac'),
      pattern: String.raw`\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b`,
      isRegex: true,
      background: '#0f766e',
      foreground: '#ecfeff',
    }));
  };

  const addTimestampPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_timestamp'),
      pattern: String.raw`\b\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}\b`,
      isRegex: true,
      background: '#334155',
      foreground: '#f8fafc',
    }));
  };

  const addUrlPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_url'),
      pattern: String.raw`https?:\/\/[^
\s)\],;]+[^\s)\],.;:]`,
      isRegex: true,
      background: '#6d28d9',
      foreground: '#f5f3ff',
    }));
  };

  const addPortPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_port'),
      pattern: String.raw`\b(?:(?:localhost|(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}|[A-Za-z][A-Za-z0-9-]*|[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+):(?:6553[0-5]|655[0-2]\d|65[0-4]\d{2}|6[0-4]\d{3}|[1-5]?\d{1,4})|port\s+(?:6553[0-5]|655[0-2]\d|65[0-4]\d{2}|6[0-4]\d{3}|[1-5]?\d{1,4}))\b`,
      isRegex: true,
      background: '#be185d',
      foreground: '#fff1f2',
    }));
  };

  const addPathPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_path'),
      pattern: String.raw`(?:\b[A-Za-z]:\\(?:[^\\/:*?"<>|\r\n]+\\)*[^\\/:*?"<>|\r\n\s]+|\/(?:[\w-]+|\.[\w-]+)(?:\/[\w.-]+)*)`,
      isRegex: true,
      background: '#365314',
      foreground: '#f7fee7',
    }));
  };

  const addUuidPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_uuid'),
      pattern: String.raw`\b[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}\b`,
      isRegex: true,
      background: '#7c2d12',
      foreground: '#fff7ed',
    }));
  };

  const addEmailPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_email'),
      pattern: String.raw`\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b`,
      isRegex: true,
      background: '#0f766e',
      foreground: '#ecfeff',
    }));
  };

  const addDomainPreset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_domain'),
      pattern: String.raw`\b(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,}\b`,
      isRegex: true,
      background: '#1e3a8a',
      foreground: '#dbeafe',
    }));
  };

  const addSha256Preset = () => {
    addRule(createDefaultHighlightRule({
      label: t('settings_view.terminal.highlight_rules.preset_label_sha256'),
      pattern: String.raw`\b[A-Fa-f0-9]{64}\b`,
      isRegex: true,
      background: '#78350f',
      foreground: '#fef3c7',
    }));
  };

  const presetItems: HighlightPresetItem[] = [
    {
      id: 'status',
      label: t('settings_view.terminal.highlight_rules.preset_status'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_warning'),
      group: 'logs',
      onSelect: addStatusPreset,
    },
    {
      id: 'timestamp',
      label: t('settings_view.terminal.highlight_rules.preset_timestamp'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_timestamp'),
      group: 'logs',
      onSelect: addTimestampPreset,
    },
    {
      id: 'ip',
      label: t('settings_view.terminal.highlight_rules.preset_ip'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_ip'),
      group: 'network',
      onSelect: addIpPreset,
    },
    {
      id: 'mac',
      label: t('settings_view.terminal.highlight_rules.preset_mac'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_mac'),
      group: 'network',
      onSelect: addMacPreset,
    },
    {
      id: 'url',
      label: t('settings_view.terminal.highlight_rules.preset_url'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_url'),
      group: 'network',
      onSelect: addUrlPreset,
    },
    {
      id: 'port',
      label: t('settings_view.terminal.highlight_rules.preset_port'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_port'),
      group: 'network',
      onSelect: addPortPreset,
    },
    {
      id: 'email',
      label: t('settings_view.terminal.highlight_rules.preset_email'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_email'),
      group: 'network',
      onSelect: addEmailPreset,
    },
    {
      id: 'domain',
      label: t('settings_view.terminal.highlight_rules.preset_domain'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_domain'),
      group: 'network',
      onSelect: addDomainPreset,
    },
    {
      id: 'path',
      label: t('settings_view.terminal.highlight_rules.preset_path'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_path'),
      group: 'system',
      onSelect: addPathPreset,
    },
    {
      id: 'uuid',
      label: t('settings_view.terminal.highlight_rules.preset_uuid'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_uuid'),
      group: 'identity',
      onSelect: addUuidPreset,
    },
    {
      id: 'sha256',
      label: t('settings_view.terminal.highlight_rules.preset_sha256'),
      shortcut: t('settings_view.terminal.highlight_rules.preset_label_sha256'),
      group: 'identity',
      onSelect: addSha256Preset,
    },
  ];

  const presetGroups: Array<{ id: HighlightPresetGroup; label: string; items: HighlightPresetItem[] }> = [
    {
      id: 'logs',
      label: t('settings_view.terminal.highlight_rules.preset_group_logs'),
      items: presetItems.filter((item) => item.group === 'logs'),
    },
    {
      id: 'network',
      label: t('settings_view.terminal.highlight_rules.preset_group_network'),
      items: presetItems.filter((item) => item.group === 'network'),
    },
    {
      id: 'system',
      label: t('settings_view.terminal.highlight_rules.preset_group_system'),
      items: presetItems.filter((item) => item.group === 'system'),
    },
    {
      id: 'identity',
      label: t('settings_view.terminal.highlight_rules.preset_group_identity'),
      items: presetItems.filter((item) => item.group === 'identity'),
    },
  ];

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) {
      return;
    }
    const oldIndex = rules.findIndex((rule) => rule.id === active.id);
    const newIndex = rules.findIndex((rule) => rule.id === over.id);
    if (oldIndex === -1 || newIndex === -1) {
      return;
    }
    setRules(arrayMove(rules, oldIndex, newIndex));
  };

  return (
    <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h4 className="text-sm font-medium uppercase tracking-wider text-theme-text">{t('settings_view.terminal.highlight_rules.title')}</h4>
          <p className="mt-1 max-w-2xl text-xs text-theme-text-muted">{t('settings_view.terminal.highlight_rules.description')}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" size="sm" variant="outline" disabled={rules.length >= MAX_HIGHLIGHT_RULES}>
                <Wand2 className="mr-1 h-4 w-4" />
                {t('settings_view.terminal.highlight_rules.add_preset')}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-72">
              {presetGroups.map((group, groupIndex) => (
                <div key={group.id}>
                  {groupIndex > 0 ? <DropdownMenuSeparator /> : null}
                  <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
                  {group.items.map((item) => (
                    <DropdownMenuItem key={item.id} onClick={item.onSelect}>
                      <span>{item.label}</span>
                      <span className="ml-auto text-xs text-theme-text-muted">{item.shortcut}</span>
                    </DropdownMenuItem>
                  ))}
                </div>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <Button type="button" size="sm" onClick={() => addRule()} disabled={rules.length >= MAX_HIGHLIGHT_RULES}>
            <Plus className="mr-1 h-4 w-4" />
            {t('settings_view.terminal.highlight_rules.add_rule')}
          </Button>
        </div>
      </div>

      <div className="mt-3 flex items-center justify-between text-xs text-theme-text-muted">
        <span>{t('settings_view.terminal.highlight_rules.limit', { count: MAX_HIGHLIGHT_RULES })}</span>
        <span>{t('settings_view.terminal.highlight_rules.priority_hint')}</span>
      </div>

      <Separator className="my-4 opacity-50" />

      {rules.length === 0 ? (
        <div className="rounded-lg border border-dashed border-theme-border bg-theme-bg-sunken/60 px-4 py-8 text-center text-sm text-theme-text-muted">
          {t('settings_view.terminal.highlight_rules.empty')}
        </div>
      ) : (
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
          <SortableContext items={sortableIds} strategy={verticalListSortingStrategy}>
            <div className="space-y-3">
              {rules.map((rule) => (
                <SortableRuleRow
                  key={rule.id}
                  rule={rule}
                  runtimeRule={runtimeRuleById.get(rule.id)}
                  onChange={(patch) => {
                    setRules(rules.map((currentRule) => currentRule.id === rule.id ? { ...currentRule, ...patch } : currentRule));
                  }}
                  onDelete={() => setRules(rules.filter((currentRule) => currentRule.id !== rule.id))}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}

      <Separator className="my-4 opacity-50" />

      <div className="rounded-lg border border-theme-border bg-[#071018] p-4">
        <div className="mb-2 flex items-center justify-between">
          <Label className="text-theme-text">{t('settings_view.terminal.highlight_rules.preview')}</Label>
          <span className="text-xs text-theme-text-muted">{t('settings_view.terminal.highlight_rules.preview_hint')}</span>
        </div>
        <div className="space-y-1 rounded-md border border-white/5 bg-[#020617] p-3 font-mono text-xs leading-6 text-slate-200">
          {previewSampleLines.map((line, index) => (
            <div key={`${line}-${index}`}>{renderPreviewLine(line, runtimeRules)}</div>
          ))}
        </div>
      </div>
    </div>
  );
};