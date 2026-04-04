// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import { detectIntent } from '@/lib/ai/intentDetector';
import { parseUserInput } from '@/lib/ai/inputParser';
import type { ParsedInput } from '@/lib/ai/inputParser';

// Helper: parse and detect in one step
function detect(raw: string) {
  return detectIntent(parseUserInput(raw));
}

// Helper: make a minimal ParsedInput
function makeParsed(overrides: Partial<ParsedInput> = {}): ParsedInput {
  return {
    slashCommand: null,
    participants: [],
    references: [],
    cleanText: '',
    rawText: '',
    ...overrides,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Slash Command → Intent Mapping
// ═══════════════════════════════════════════════════════════════════════════

describe('detectIntent — slash commands', () => {
  it('/explain → explain with 0.95 confidence', () => {
    const intent = detect('/explain what is nginx');
    expect(intent.type).toBe('explain');
    expect(intent.confidence).toBe(0.95);
  });

  it('/fix → troubleshoot', () => {
    expect(detect('/fix this error').type).toBe('troubleshoot');
  });

  it('/script → create', () => {
    expect(detect('/script backup tool').type).toBe('create');
  });

  it('/deploy → execute', () => {
    expect(detect('/deploy the app').type).toBe('execute');
  });

  it('/monitor → explore', () => {
    expect(detect('/monitor system health').type).toBe('explore');
  });

  it('/search → explore', () => {
    expect(detect('/search logs for errors').type).toBe('explore');
  });

  it('/optimize → create', () => {
    expect(detect('/optimize this query').type).toBe('create');
  });

  it('/connect → execute', () => {
    expect(detect('/connect to production').type).toBe('execute');
  });

  it('unknown slash command falls through to pattern matching', () => {
    const intent = detect('/unknowncmd why is it broken');
    // No slash mapping → falls to patterns → "broken" matches troubleshoot
    expect(intent.type).toBe('troubleshoot');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Pattern Matching
// ═══════════════════════════════════════════════════════════════════════════

describe('detectIntent — pattern matching', () => {
  // ── Execute ──
  it('detects "run" as execute', () => {
    expect(detect('run docker compose up').type).toBe('execute');
  });

  it('detects "sudo" as execute', () => {
    expect(detect('sudo apt update').type).toBe('execute');
  });

  it('detects "ssh into" as execute', () => {
    expect(detect('ssh into production server').type).toBe('execute');
  });

  // ── Explain ──
  it('detects "what is" as explain', () => {
    expect(detect('what is a reverse proxy').type).toBe('explain');
  });

  it('detects question mark as explain', () => {
    expect(detect('how do I configure nginx?').type).toBe('explain');
  });

  it('detects "tell me about" as explain', () => {
    expect(detect('tell me about load balancing').type).toBe('explain');
  });

  // ── Troubleshoot ──
  it('detects "error" as troubleshoot', () => {
    expect(detect('I see an error in the logs').type).toBe('troubleshoot');
  });

  it('detects "permission denied" as troubleshoot', () => {
    expect(detect('getting permission denied on /var/log').type).toBe('troubleshoot');
  });

  it('detects "not working" as troubleshoot', () => {
    expect(detect('the service is not working').type).toBe('troubleshoot');
  });

  it('troubleshoot has highest confidence (0.9)', () => {
    expect(detect('fix this error').confidence).toBe(0.9);
  });

  // ── Create ──
  it('detects "create" as create', () => {
    expect(detect('create a dockerfile').type).toBe('create');
  });

  it('detects "write me" as create', () => {
    expect(detect('write me a bash script').type).toBe('create');
  });

  // ── Explore ──
  it('detects "find" as explore', () => {
    expect(detect('find all nginx configs').type).toBe('explore');
  });

  it('detects "ls" as explore', () => {
    expect(detect('ls /var/log').type).toBe('explore');
  });

  it('detects "how many" as explore', () => {
    expect(detect('how many containers are running').type).toBe('explore');
  });

  // ── Configure ──
  it('detects "configure" as configure', () => {
    expect(detect('configure the firewall').type).toBe('configure');
  });

  it('detects "enable" as configure', () => {
    expect(detect('enable ssh key forwarding').type).toBe('configure');
  });

  it('detects "settings" as configure', () => {
    expect(detect('change the terminal settings').type).toBe('configure');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

describe('detectIntent — edge cases', () => {
  it('returns general for empty text', () => {
    const intent = detectIntent(makeParsed({ cleanText: '' }));
    expect(intent.type).toBe('general');
    expect(intent.confidence).toBe(0.5);
  });

  it('returns general for whitespace only', () => {
    const intent = detectIntent(makeParsed({ cleanText: '   ' }));
    expect(intent.type).toBe('general');
  });

  it('returns general for unrecognizable text', () => {
    const intent = detect('lorem ipsum dolor sit amet');
    expect(intent.type).toBe('general');
    expect(intent.confidence).toBe(0.5);
  });

  it('picks highest confidence when multiple intents match', () => {
    // "fix" matches troubleshoot (0.9), "script" matches create (0.85)
    // troubleshoot should win
    const intent = detect('fix this broken script');
    expect(intent.type).toBe('troubleshoot');
  });

  it('system hint is non-empty for non-general intents', () => {
    const intent = detect('explain what is TCP');
    expect(intent.systemHint.length).toBeGreaterThan(0);
  });

  it('system hint is empty for general', () => {
    const intent = detect('lorem ipsum');
    expect(intent.systemHint).toBe('');
  });

  it('handles CJK text gracefully (falls to general)', () => {
    const intent = detect('你好世界');
    expect(intent.type).toBe('general');
  });

  it('handles only @participant (cleanText empty after strip)', () => {
    const intent = detect('@terminal');
    expect(intent.type).toBe('general');
  });
});
