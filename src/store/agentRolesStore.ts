// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent Roles Store — Custom role CRUD + persistence.
 *
 * Wraps builtinRoles registry and adds user-created custom roles,
 * pipeline presets, and persists to localStorage.
 */

import { create } from 'zustand';
import {
  BUILTIN_PLANNER,
  BUILTIN_EXECUTOR,
  BUILTIN_REVIEWER,
  DEFAULT_PIPELINE,
  registerCustomRole,
  unregisterCustomRole,
} from '../lib/ai/roles';
import type { AgentRoleDefinition, AgentPipelinePreset } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Persistence
// ═══════════════════════════════════════════════════════════════════════════

const STORAGE_KEY = 'oxideterm:agent-roles';

type PersistedState = {
  customRoles: AgentRoleDefinition[];
  customPipelines: AgentPipelinePreset[];
  activePipelineId: string;
};

function loadPersistedState(): PersistedState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as PersistedState;
      return {
        customRoles: Array.isArray(parsed.customRoles) ? parsed.customRoles : [],
        customPipelines: Array.isArray(parsed.customPipelines) ? parsed.customPipelines : [],
        activePipelineId: typeof parsed.activePipelineId === 'string' ? parsed.activePipelineId : DEFAULT_PIPELINE.id,
      };
    }
  } catch { /* ignore corrupt data */ }
  return { customRoles: [], customPipelines: [], activePipelineId: DEFAULT_PIPELINE.id };
}

function persistState(state: PersistedState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch { /* ignore quota errors */ }
}

// ═══════════════════════════════════════════════════════════════════════════
// Store
// ═══════════════════════════════════════════════════════════════════════════

const BUILTIN_ROLES: AgentRoleDefinition[] = [BUILTIN_PLANNER, BUILTIN_EXECUTOR, BUILTIN_REVIEWER];
const BUILTIN_PIPELINES: AgentPipelinePreset[] = [DEFAULT_PIPELINE];

type AgentRolesStore = {
  // ─── State ──────────────────────────────────────────────────────────────
  customRoles: AgentRoleDefinition[];
  customPipelines: AgentPipelinePreset[];
  activePipelineId: string;

  // ─── Computed ─────────────────────────────────────────────────────────
  /** All roles (builtin + custom) */
  allRoles: () => AgentRoleDefinition[];
  /** All pipelines (builtin + custom) */
  allPipelines: () => AgentPipelinePreset[];
  /** Active pipeline preset */
  activePipeline: () => AgentPipelinePreset;
  /** Find a role by ID */
  getRole: (id: string) => AgentRoleDefinition | undefined;

  // ─── Role CRUD ──────────────────────────────────────────────────────────
  addRole: (role: AgentRoleDefinition) => void;
  updateRole: (id: string, updates: Partial<Omit<AgentRoleDefinition, 'id' | 'builtin'>>) => void;
  removeRole: (id: string) => void;
  duplicateRole: (id: string) => AgentRoleDefinition | null;

  // ─── Pipeline CRUD ──────────────────────────────────────────────────────
  addPipeline: (pipeline: AgentPipelinePreset) => void;
  updatePipeline: (id: string, updates: Partial<Omit<AgentPipelinePreset, 'id' | 'builtin'>>) => void;
  removePipeline: (id: string) => void;
  setActivePipeline: (id: string) => void;

  // ─── Import/Export ──────────────────────────────────────────────────────
  exportRoles: () => string;
  importRoles: (json: string) => { imported: number; errors: string[] };
  exportPipelines: () => string;
  importPipelines: (json: string) => { imported: number; errors: string[] };
};

export const useAgentRolesStore = create<AgentRolesStore>((set, get) => {
  const saved = loadPersistedState();

  // Register saved custom roles into the runtime registry
  for (const role of saved.customRoles) {
    try { registerCustomRole(role); } catch { /* skip duplicates */ }
  }

  const save = () => {
    const s = get();
    persistState({
      customRoles: s.customRoles,
      customPipelines: s.customPipelines,
      activePipelineId: s.activePipelineId,
    });
  };

  return {
    customRoles: saved.customRoles,
    customPipelines: saved.customPipelines,
    activePipelineId: saved.activePipelineId,

    // ─── Computed ───────────────────────────────────────────────────────
    allRoles: () => [...BUILTIN_ROLES, ...get().customRoles],
    allPipelines: () => [...BUILTIN_PIPELINES, ...get().customPipelines],
    activePipeline: () => {
      const id = get().activePipelineId;
      const all = get().allPipelines();
      return all.find(p => p.id === id) ?? DEFAULT_PIPELINE;
    },
    getRole: (id) => {
      const builtin = BUILTIN_ROLES.find(r => r.id === id);
      if (builtin) return builtin;
      return get().customRoles.find(r => r.id === id);
    },

    // ─── Role CRUD ──────────────────────────────────────────────────────
    addRole: (role) => {
      const r = { ...role, builtin: false };
      registerCustomRole(r);
      set(s => ({ customRoles: [...s.customRoles, r] }));
      save();
    },

    updateRole: (id, updates) => {
      set(s => ({
        customRoles: s.customRoles.map(r =>
          r.id === id ? { ...r, ...updates, id, builtin: false } : r
        ),
      }));
      // Re-register to update runtime registry
      const updated = get().customRoles.find(r => r.id === id);
      if (updated) {
        try { unregisterCustomRole(id); } catch { /* ok */ }
        registerCustomRole(updated);
      }
      save();
    },

    removeRole: (id) => {
      if (BUILTIN_ROLES.some(r => r.id === id)) return;
      try { unregisterCustomRole(id); } catch { /* ok */ }
      set(s => ({ customRoles: s.customRoles.filter(r => r.id !== id) }));
      save();
    },

    duplicateRole: (id) => {
      const source = get().getRole(id);
      if (!source) return null;
      const newRole: AgentRoleDefinition = {
        ...source,
        id: `custom:${crypto.randomUUID().slice(0, 8)}`,
        name: `${source.name} (copy)`,
        builtin: false,
      };
      get().addRole(newRole);
      return newRole;
    },

    // ─── Pipeline CRUD ──────────────────────────────────────────────────
    addPipeline: (pipeline) => {
      const p = { ...pipeline, builtin: false };
      set(s => ({ customPipelines: [...s.customPipelines, p] }));
      save();
    },

    updatePipeline: (id, updates) => {
      set(s => ({
        customPipelines: s.customPipelines.map(p =>
          p.id === id ? { ...p, ...updates, id, builtin: false } : p
        ),
      }));
      save();
    },

    removePipeline: (id) => {
      if (BUILTIN_PIPELINES.some(p => p.id === id)) return;
      set(s => {
        const next = s.customPipelines.filter(p => p.id !== id);
        const activePipelineId = s.activePipelineId === id ? DEFAULT_PIPELINE.id : s.activePipelineId;
        return { customPipelines: next, activePipelineId };
      });
      save();
    },

    setActivePipeline: (id) => {
      set({ activePipelineId: id });
      save();
    },

    // ─── Import/Export ──────────────────────────────────────────────────
    exportRoles: () => {
      return JSON.stringify(get().customRoles, null, 2);
    },

    importRoles: (json: string) => {
      const errors: string[] = [];
      let imported = 0;
      const MAX_ROLES = 50;
      const MAX_TEMPLATE_LEN = 100_000;
      try {
        const parsed = JSON.parse(json);
        const roles: AgentRoleDefinition[] = Array.isArray(parsed) ? parsed : [parsed];
        if (roles.length > MAX_ROLES) {
          return { imported: 0, errors: [`Too many roles: ${roles.length} (max ${MAX_ROLES})`] };
        }
        for (const role of roles) {
          const VALID_ROLE_TYPES = new Set(['planner', 'executor', 'reviewer']);
          if (!role.id || !VALID_ROLE_TYPES.has(role.roleType) || !role.systemPromptTemplate) {
            errors.push(`Invalid role: missing required fields (${role.id ?? 'unknown'})`);
            continue;
          }
          if (role.systemPromptTemplate.length > MAX_TEMPLATE_LEN) {
            errors.push(`Role "${role.id}" template too large (max ${MAX_TEMPLATE_LEN} chars)`);
            continue;
          }
          if (BUILTIN_ROLES.some(r => r.id === role.id)) {
            errors.push(`Cannot overwrite builtin role: ${role.id}`);
            continue;
          }
          const existing = get().customRoles.find(r => r.id === role.id);
          if (existing) {
            get().updateRole(role.id, role);
          } else {
            get().addRole({ ...role, builtin: false });
          }
          imported++;
        }
      } catch (e) {
        errors.push(`JSON parse error: ${e instanceof Error ? e.message : String(e)}`);
      }
      return { imported, errors };
    },

    exportPipelines: () => {
      return JSON.stringify(get().customPipelines, null, 2);
    },

    importPipelines: (json: string) => {
      const errors: string[] = [];
      let imported = 0;
      const MAX_PIPELINES = 20;
      const MAX_STAGES = 10;
      try {
        const parsed = JSON.parse(json);
        const pipelines: AgentPipelinePreset[] = Array.isArray(parsed) ? parsed : [parsed];
        if (pipelines.length > MAX_PIPELINES) {
          return { imported: 0, errors: [`Too many pipelines: ${pipelines.length} (max ${MAX_PIPELINES})`] };
        }
        for (const pipeline of pipelines) {
          if (!pipeline.id || !Array.isArray(pipeline.stages)) {
            errors.push(`Invalid pipeline: missing required fields (${pipeline.id ?? 'unknown'})`);
            continue;
          }
          if (pipeline.stages.length > MAX_STAGES) {
            errors.push(`Pipeline "${pipeline.id}" has too many stages (max ${MAX_STAGES})`);
            continue;
          }
          if (BUILTIN_PIPELINES.some(p => p.id === pipeline.id)) {
            errors.push(`Cannot overwrite builtin pipeline: ${pipeline.id}`);
            continue;
          }
          const existing = get().customPipelines.find(p => p.id === pipeline.id);
          if (existing) {
            get().updatePipeline(pipeline.id, pipeline);
          } else {
            get().addPipeline({ ...pipeline, builtin: false });
          }
          imported++;
        }
      } catch (e) {
        errors.push(`JSON parse error: ${e instanceof Error ? e.message : String(e)}`);
      }
      return { imported, errors };
    },
  };
});
