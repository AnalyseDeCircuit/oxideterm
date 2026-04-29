// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, expect, it, beforeEach } from 'vitest';
import {
  addAiCommandRecord,
  clearAiCommandLedger,
  clearCliAgentSessions,
  commandRecordFromToolResult,
  detectCliAgentKind,
  listAiCommandRecords,
  observeCliAgentTerminalInput,
  resolveAiPolicyDecision,
} from '@/lib/ai/orchestrator';
import { normalizeExecutionProfiles } from '@/lib/ai/profiles';
import type { AiSettings } from '@/store/settingsStore';

const baseAiSettings = {
  toolUse: {
    enabled: true,
    autoApproveTools: {
      run_command: true,
      send_terminal_input: false,
      'write_resource:settings': false,
    },
    disabledTools: [],
    maxRounds: 10,
  },
} as unknown as AiSettings;

describe('OxideSens vNext runtime layers', () => {
  beforeEach(() => {
    clearAiCommandLedger();
    clearCliAgentSessions();
  });

  it('requires destructive approval in default mode but allows it in bypass mode', () => {
    const defaultDecision = resolveAiPolicyDecision({
      toolName: 'run_command',
      args: { command: 'sudo reboot' },
      aiSettings: baseAiSettings,
      safetyMode: 'default',
      profileId: 'profile-a',
    });
    expect(defaultDecision.decision).toBe('require_approval');
    expect(defaultDecision.risk).toBe('destructive');
    expect(defaultDecision.profileId).toBe('profile-a');

    const bypassDecision = resolveAiPolicyDecision({
      toolName: 'run_command',
      args: { command: 'sudo reboot' },
      aiSettings: baseAiSettings,
      safetyMode: 'bypass',
    });
    expect(bypassDecision.decision).toBe('allow');
    expect(bypassDecision.reasonCode).toBe('bypass_destructive_allowed');
  });

  it('records command ledger entries from tool results without persisting a separate log', () => {
    const record = commandRecordFromToolResult({
      toolName: 'run_command',
      args: { command: 'pwd', cwd: '/tmp' },
      ok: true,
      risk: 'execute',
      outputPreview: { strategy: 'full', charCount: 5, lineCount: 1 },
      rawOutputStored: true,
      exitCode: 0,
    });

    expect(record?.command).toBe('pwd');
    expect(record?.cwd).toBe('/tmp');
    expect(record?.rawOutputRef).toBe('tool-result.rawOutput');
    expect(listAiCommandRecords()).toHaveLength(1);
  });

  it('keeps only bounded runtime command records', () => {
    for (let i = 0; i < 220; i += 1) {
      addAiCommandRecord({
        command: `echo ${i}`,
        source: 'ai.run_command',
        status: 'completed',
        risk: 'execute',
        finishedAt: Date.now(),
      });
    }
    expect(listAiCommandRecords().length).toBeLessThanOrEqual(200);
  });

  it('detects external CLI agent commands from full commands and terminal input', () => {
    expect(detectCliAgentKind('codex')).toBe('codex');
    expect(detectCliAgentKind('npx @anthropic-ai/claude')).toBe('claude');
    expect(detectCliAgentKind('ls -la')).toBeNull();

    const session = observeCliAgentTerminalInput({ sessionId: 'local-1', data: 'gemini\r' });
    expect(session?.kind).toBe('gemini');
    expect(session?.sessionId).toBe('local-1');
  });

  it('normalizes execution profiles from existing AI settings', () => {
    const profiles = normalizeExecutionProfiles({
      providerId: 'provider-1',
      model: 'model-1',
      reasoningEffort: 'medium',
      toolUse: baseAiSettings.toolUse,
    });

    expect(profiles.profiles).toHaveLength(1);
    expect(profiles.defaultProfileId).toBe(profiles.profiles[0].id);
    expect(profiles.profiles[0].providerId).toBe('provider-1');
    expect(profiles.profiles[0].model).toBe('model-1');
  });
});
