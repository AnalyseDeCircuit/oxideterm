// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect, beforeEach, vi } from 'vitest';

// ═══════════════════════════════════════════════════════════════════════════
// Platform detection logic replicated from src/lib/platform.ts
// We test the detection algorithm in isolation because the module
// executes detectPlatform() at import time using global navigator.
// ═══════════════════════════════════════════════════════════════════════════

function detectPlatform(
  userAgentData?: { platform?: string },
  legacyPlatform?: string,
): 'windows' | 'macos' | 'linux' | 'unknown' {
  // 1. navigator.userAgentData (Chromium-based, including Tauri WebView)
  if (userAgentData?.platform) {
    const p = userAgentData.platform.toLowerCase();
    if (p === 'windows') return 'windows';
    if (p === 'macos') return 'macos';
    if (p === 'linux') return 'linux';
  }
  // 2. Fallback: navigator.platform (deprecated but still available)
  const legacy = (legacyPlatform ?? '').toLowerCase();
  if (legacy.includes('win')) return 'windows';
  if (legacy.includes('mac')) return 'macos';
  if (legacy.includes('linux')) return 'linux';
  return 'unknown';
}

// ═══════════════════════════════════════════════════════════════════════════
// userAgentData path (modern)
// ═══════════════════════════════════════════════════════════════════════════

describe('detectPlatform — userAgentData', () => {
  it('detects Windows', () => {
    expect(detectPlatform({ platform: 'Windows' })).toBe('windows');
  });

  it('detects macOS', () => {
    expect(detectPlatform({ platform: 'macOS' })).toBe('macos');
  });

  it('detects Linux', () => {
    expect(detectPlatform({ platform: 'Linux' })).toBe('linux');
  });

  it('is case-insensitive', () => {
    expect(detectPlatform({ platform: 'WINDOWS' })).toBe('windows');
    expect(detectPlatform({ platform: 'MACOS' })).toBe('macos');
    expect(detectPlatform({ platform: 'LINUX' })).toBe('linux');
  });

  it('falls through for unknown platform string', () => {
    expect(detectPlatform({ platform: 'ChromeOS' })).toBe('unknown');
  });

  it('falls through when platform is empty string', () => {
    expect(detectPlatform({ platform: '' })).toBe('unknown');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// navigator.platform fallback path (legacy)
// ═══════════════════════════════════════════════════════════════════════════

describe('detectPlatform — legacy platform fallback', () => {
  it('detects Win32', () => {
    expect(detectPlatform(undefined, 'Win32')).toBe('windows');
  });

  it('detects Win64', () => {
    expect(detectPlatform(undefined, 'Win64')).toBe('windows');
  });

  it('detects MacIntel', () => {
    expect(detectPlatform(undefined, 'MacIntel')).toBe('macos');
  });

  it('detects MacPPC', () => {
    expect(detectPlatform(undefined, 'MacPPC')).toBe('macos');
  });

  it('detects Linux x86_64', () => {
    expect(detectPlatform(undefined, 'Linux x86_64')).toBe('linux');
  });

  it('returns unknown for empty string', () => {
    expect(detectPlatform(undefined, '')).toBe('unknown');
  });

  it('returns unknown for undefined', () => {
    expect(detectPlatform(undefined, undefined)).toBe('unknown');
  });

  it('returns unknown for FreeBSD', () => {
    expect(detectPlatform(undefined, 'FreeBSD')).toBe('unknown');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Priority: userAgentData > legacy platform
// ═══════════════════════════════════════════════════════════════════════════

describe('detectPlatform — priority', () => {
  it('userAgentData takes priority over legacy', () => {
    // userAgentData says macOS, legacy says Win32
    expect(detectPlatform({ platform: 'macOS' }, 'Win32')).toBe('macos');
  });

  it('falls to legacy when userAgentData has no platform', () => {
    expect(detectPlatform({}, 'Win32')).toBe('windows');
  });

  it('falls to legacy when userAgentData is undefined', () => {
    expect(detectPlatform(undefined, 'MacIntel')).toBe('macos');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// platform object contract
// ═══════════════════════════════════════════════════════════════════════════

describe('platform object', () => {
  it('has expected shape', async () => {
    // Import actual module — in jsdom env, navigator values are mocked
    const { platform } = await import('@/lib/platform');
    expect(platform).toHaveProperty('isWindows');
    expect(platform).toHaveProperty('isMac');
    expect(platform).toHaveProperty('isLinux');
    expect(platform).toHaveProperty('detected');
    expect(typeof platform.isWindows).toBe('boolean');
    expect(typeof platform.isMac).toBe('boolean');
    expect(typeof platform.isLinux).toBe('boolean');
    expect(['windows', 'macos', 'linux', 'unknown']).toContain(platform.detected);
  });

  it('exactly one platform flag is true (or all false for unknown)', async () => {
    const { platform } = await import('@/lib/platform');
    const flags = [platform.isWindows, platform.isMac, platform.isLinux];
    const trueCount = flags.filter(Boolean).length;
    expect(trueCount).toBeLessThanOrEqual(1);
  });
});
