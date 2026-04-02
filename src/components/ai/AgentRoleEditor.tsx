// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * AgentRoleEditor — Create/edit custom agent roles + pipeline preset selector.
 *
 * - RoleEditorDialog: modal for editing role definition (name, prompt template, tools, etc.)
 * - CustomRolesSection: expandable panel listing custom roles with add/edit/delete/duplicate
 * - PipelineSelector: dropdown to pick active pipeline preset
 */

import { useState, useCallback, memo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Plus,
  Pencil,
  Trash2,
  Copy,
  Download,
  Upload,
  ChevronDown,
  ChevronRight,
  Layers,
  Sparkles,
  X,
  Check,
} from 'lucide-react';
import { useAgentRolesStore } from '../../store/agentRolesStore';
import { useAgentStore } from '../../store/agentStore';
import { cn } from '../../lib/utils';
import { Button } from '../ui/button';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '../ui/select';
import type { AgentRoleDefinition, AgentRoleType } from '../../types';

// ═══════════════════════════════════════════════════════════════════════════
// Role Editor Dialog (inline modal)
// ═══════════════════════════════════════════════════════════════════════════

type RoleEditorProps = {
  role?: AgentRoleDefinition;
  onSave: (role: AgentRoleDefinition) => void;
  onClose: () => void;
};

const ROLE_TYPES: AgentRoleType[] = ['planner', 'executor', 'reviewer'];
const OUTPUT_SCHEMAS = ['text', 'json', 'structured'] as const;
const TEMPLATE_VARS = ['{{autonomyLevel}}', '{{maxRounds}}', '{{currentRound}}', '{{sessions}}', '{{context}}', '{{goal}}', '{{steps}}', '{{plan}}'];

const RoleEditorDialog = memo(({ role, onSave, onClose }: RoleEditorProps) => {
  const { t } = useTranslation();
  const isNew = !role;

  const [name, setName] = useState(role?.name ?? '');
  const [description, setDescription] = useState(role?.description ?? '');
  const [roleType, setRoleType] = useState<AgentRoleType>(role?.roleType ?? 'executor');
  const [promptTemplate, setPromptTemplate] = useState(role?.systemPromptTemplate ?? '');
  const [toolAllowlist, setToolAllowlist] = useState(
    role?.toolAllowlist === '*' ? '*' : (role?.toolAllowlist ?? []).join(', ')
  );
  const [toolMode, setToolMode] = useState<'all' | 'none' | 'custom'>(
    role?.toolAllowlist === '*' ? 'all' : (role?.toolAllowlist?.length === 0 ? 'none' : 'custom')
  );
  const [maxRounds, setMaxRounds] = useState<string>(role?.maxRounds?.toString() ?? '');
  const [outputSchema, setOutputSchema] = useState<typeof OUTPUT_SCHEMAS[number]>(role?.outputSchema ?? 'text');

  const handleSave = useCallback(() => {
    if (!name.trim() || !promptTemplate.trim()) return;

    const resolvedAllowlist: '*' | string[] =
      toolMode === 'all' ? '*' :
      toolMode === 'none' ? [] :
      toolAllowlist.split(',').map(s => s.trim()).filter(Boolean);

    const newRole: AgentRoleDefinition = {
      id: role?.id ?? `custom:${crypto.randomUUID().slice(0, 8)}`,
      name: name.trim(),
      description: description.trim(),
      roleType,
      systemPromptTemplate: promptTemplate,
      toolAllowlist: resolvedAllowlist,
      maxRounds: maxRounds ? parseInt(maxRounds, 10) || null : null,
      outputSchema,
      builtin: false,
    };
    onSave(newRole);
  }, [name, description, roleType, promptTemplate, toolAllowlist, toolMode, maxRounds, outputSchema, role, onSave]);

  return (
    <div className="border border-theme-border rounded-lg bg-theme-bg-hover p-3 space-y-3">
      <div className="flex items-center justify-between">
        <h4 className="text-xs font-semibold text-theme-text">
          {isNew ? t('agent.customRoles.newRole') : t('agent.customRoles.editRole')}
        </h4>
        <button onClick={onClose} className="text-theme-text-muted hover:text-theme-text">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Name + Role Type */}
      <div className="grid grid-cols-2 gap-2">
        <div>
          <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.name')}</label>
          <input
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder={t('agent.customRoles.nameHint')}
            className="w-full h-7 text-xs px-2 rounded border border-theme-border bg-theme-bg text-theme-text"
          />
        </div>
        <div>
          <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.roleType')}</label>
          <Select value={roleType} onValueChange={v => setRoleType(v as AgentRoleType)}>
            <SelectTrigger className="h-7 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ROLE_TYPES.map(rt => (
                <SelectItem key={rt} value={rt}>{rt}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Description */}
      <div>
        <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.description')}</label>
        <input
          value={description}
          onChange={e => setDescription(e.target.value)}
          placeholder={t('agent.customRoles.descriptionHint')}
          className="w-full h-7 text-xs px-2 rounded border border-theme-border bg-theme-bg text-theme-text"
        />
      </div>

      {/* Prompt Template */}
      <div>
        <div className="flex items-center justify-between mb-0.5">
          <label className="text-[10px] text-theme-text-muted">{t('agent.customRoles.promptTemplate')}</label>
          <div className="flex gap-1 flex-wrap">
            {TEMPLATE_VARS.map(v => (
              <button
                key={v}
                onClick={() => setPromptTemplate(p => p + v)}
                className="text-[9px] px-1 py-0.5 rounded bg-theme-bg border border-theme-border text-theme-text-muted hover:text-theme-accent"
              >
                {v}
              </button>
            ))}
          </div>
        </div>
        <textarea
          value={promptTemplate}
          onChange={e => setPromptTemplate(e.target.value)}
          rows={6}
          className={cn(
            'w-full resize-none rounded border border-theme-border bg-theme-bg px-2 py-1.5',
            'text-xs font-mono text-theme-text placeholder:text-theme-text-muted',
            'focus:outline-none focus:ring-1 focus:ring-theme-accent',
          )}
          placeholder={t('agent.customRoles.promptHint')}
        />
      </div>

      {/* Tool Access + Output + Max Rounds */}
      <div className="grid grid-cols-3 gap-2">
        <div>
          <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.toolAccess')}</label>
          <Select value={toolMode} onValueChange={v => setToolMode(v as 'all' | 'none' | 'custom')}>
            <SelectTrigger className="h-7 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('agent.customRoles.toolAll')}</SelectItem>
              <SelectItem value="none">{t('agent.customRoles.toolNone')}</SelectItem>
              <SelectItem value="custom">{t('agent.customRoles.toolCustom')}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div>
          <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.outputFormat')}</label>
          <Select value={outputSchema} onValueChange={v => setOutputSchema(v as typeof OUTPUT_SCHEMAS[number])}>
            <SelectTrigger className="h-7 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {OUTPUT_SCHEMAS.map(s => (
                <SelectItem key={s} value={s}>{s}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div>
          <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.maxRounds')}</label>
          <input
            type="number"
            min={1}
            max={200}
            value={maxRounds}
            onChange={e => setMaxRounds(e.target.value)}
            placeholder="—"
            className="w-full h-7 text-xs px-2 text-center rounded border border-theme-border bg-theme-bg text-theme-text"
          />
        </div>
      </div>

      {/* Custom tool list */}
      {toolMode === 'custom' && (
        <div>
          <label className="text-[10px] text-theme-text-muted block mb-0.5">{t('agent.customRoles.toolList')}</label>
          <input
            value={typeof toolAllowlist === 'string' ? toolAllowlist : ''}
            onChange={e => setToolAllowlist(e.target.value)}
            placeholder={t('agent.customRoles.toolListHint')}
            className="w-full h-7 text-xs px-2 rounded border border-theme-border bg-theme-bg text-theme-text"
          />
        </div>
      )}

      {/* Actions */}
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={onClose} className="text-xs h-7">
          {t('common.cancel')}
        </Button>
        <Button
          size="sm"
          onClick={handleSave}
          disabled={!name.trim() || !promptTemplate.trim()}
          className="text-xs h-7 gap-1"
        >
          <Check className="w-3 h-3" />
          {t('common.save')}
        </Button>
      </div>
    </div>
  );
});
RoleEditorDialog.displayName = 'RoleEditorDialog';

// ═══════════════════════════════════════════════════════════════════════════
// Custom Roles Section
// ═══════════════════════════════════════════════════════════════════════════

export const CustomRolesSection = memo(() => {
  const { t } = useTranslation();
  const isRunning = useAgentStore(s => s.isRunning);
  const customRoles = useAgentRolesStore(s => s.customRoles);
  const addRole = useAgentRolesStore(s => s.addRole);
  const updateRole = useAgentRolesStore(s => s.updateRole);
  const removeRole = useAgentRolesStore(s => s.removeRole);
  const duplicateRole = useAgentRolesStore(s => s.duplicateRole);
  const exportRoles = useAgentRolesStore(s => s.exportRoles);
  const importRoles = useAgentRolesStore(s => s.importRoles);

  const [expanded, setExpanded] = useState(false);
  const [editingRole, setEditingRole] = useState<AgentRoleDefinition | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  const handleImport = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      const text = await file.text();
      importRoles(text);
    };
    input.click();
  }, [importRoles]);

  const handleExport = useCallback(() => {
    const data = exportRoles();
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'agent-custom-roles.json';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }, [exportRoles]);

  const handleSave = useCallback((role: AgentRoleDefinition) => {
    if (editingRole) {
      updateRole(role.id, role);
    } else {
      addRole(role);
    }
    setEditingRole(null);
    setIsCreating(false);
  }, [editingRole, updateRole, addRole]);

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1.5 text-[11px] text-theme-text-muted hover:text-theme-text transition-colors w-full"
      >
        {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        <Sparkles className="w-3 h-3" />
        <span className="font-medium">{t('agent.customRoles.title')}</span>
        {customRoles.length > 0 && (
          <span className="text-[10px] text-theme-text-muted ml-auto">{customRoles.length}</span>
        )}
      </button>

      {expanded && (
        <div className="mt-2 space-y-2 pl-1">
          {/* Role list */}
          {customRoles.map(role => (
            <div
              key={role.id}
              className="flex items-center gap-2 rounded-md bg-theme-bg-hover px-2 py-1.5 group"
            >
              <div className="flex-1 min-w-0">
                <p className="text-xs font-medium text-theme-text truncate">{role.name}</p>
                <p className="text-[10px] text-theme-text-muted truncate">{role.description || role.roleType}</p>
              </div>
              <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                <button
                  onClick={() => { setEditingRole(role); setIsCreating(false); }}
                  disabled={isRunning}
                  className="p-1 text-theme-text-muted hover:text-theme-text"
                  title={t('agent.customRoles.editRole')}
                >
                  <Pencil className="w-3 h-3" />
                </button>
                <button
                  onClick={() => duplicateRole(role.id)}
                  disabled={isRunning}
                  className="p-1 text-theme-text-muted hover:text-theme-text"
                  title={t('agent.customRoles.duplicate')}
                >
                  <Copy className="w-3 h-3" />
                </button>
                <button
                  onClick={() => removeRole(role.id)}
                  disabled={isRunning}
                  className="p-1 text-theme-text-muted hover:text-red-400"
                  title={t('common.delete')}
                >
                  <Trash2 className="w-3 h-3" />
                </button>
              </div>
            </div>
          ))}

          {/* Editor */}
          {(isCreating || editingRole) && (
            <RoleEditorDialog
              role={editingRole ?? undefined}
              onSave={handleSave}
              onClose={() => { setEditingRole(null); setIsCreating(false); }}
            />
          )}

          {/* Actions */}
          {!isCreating && !editingRole && (
            <div className="flex items-center gap-1.5">
              <Button
                size="sm"
                variant="ghost"
                onClick={() => { setIsCreating(true); setEditingRole(null); }}
                disabled={isRunning}
                className="text-[11px] h-6 gap-1"
              >
                <Plus className="w-3 h-3" />
                {t('agent.customRoles.add')}
              </Button>
              {customRoles.length > 0 && (
                <>
                  <Button size="sm" variant="ghost" onClick={handleExport} className="text-[11px] h-6 gap-1">
                    <Download className="w-3 h-3" />
                  </Button>
                  <Button size="sm" variant="ghost" onClick={handleImport} className="text-[11px] h-6 gap-1" disabled={isRunning}>
                    <Upload className="w-3 h-3" />
                  </Button>
                </>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
});
CustomRolesSection.displayName = 'CustomRolesSection';

// ═══════════════════════════════════════════════════════════════════════════
// Pipeline Selector
// ═══════════════════════════════════════════════════════════════════════════

export const PipelineSelector = memo(() => {
  const { t } = useTranslation();
  const isRunning = useAgentStore(s => s.isRunning);
  const allPipelines = useAgentRolesStore(s => s.allPipelines);
  const activePipelineId = useAgentRolesStore(s => s.activePipelineId);
  const setActivePipeline = useAgentRolesStore(s => s.setActivePipeline);

  const pipelines = allPipelines();
  if (pipelines.length <= 1) return null;

  return (
    <div className="flex items-center gap-2">
      <Layers className="w-3.5 h-3.5 text-theme-text-muted" />
      <Select value={activePipelineId} onValueChange={setActivePipeline} disabled={isRunning}>
        <SelectTrigger className="h-6 text-[11px] min-w-0 flex-1">
          <SelectValue placeholder={t('agent.pipeline.select')} />
        </SelectTrigger>
        <SelectContent>
          {pipelines.map(p => (
            <SelectItem key={p.id} value={p.id}>
              {p.name.startsWith('agent.') ? t(p.name) : p.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
});
PipelineSelector.displayName = 'PipelineSelector';
