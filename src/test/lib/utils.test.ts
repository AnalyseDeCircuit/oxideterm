// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import { cn, createTypeGuard } from '@/lib/utils';

describe('cn', () => {
  it('merges class names', () => {
    expect(cn('foo', 'bar')).toBe('foo bar');
  });

  it('handles conditional classes', () => {
    expect(cn('base', false && 'hidden', 'visible')).toBe('base visible');
  });

  it('merges tailwind conflicts', () => {
    expect(cn('px-2', 'px-4')).toBe('px-4');
  });

  it('handles empty input', () => {
    expect(cn()).toBe('');
  });
});

describe('createTypeGuard', () => {
  const TYPES = ['local', 'remote', 'dynamic'] as const;
  const isType = createTypeGuard(TYPES);

  it('returns true for valid values', () => {
    expect(isType('local')).toBe(true);
    expect(isType('remote')).toBe(true);
    expect(isType('dynamic')).toBe(true);
  });

  it('returns false for invalid values', () => {
    expect(isType('unknown')).toBe(false);
    expect(isType('')).toBe(false);
    expect(isType(42)).toBe(false);
    expect(isType(null)).toBe(false);
  });
});
