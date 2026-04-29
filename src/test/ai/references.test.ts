// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, expect, it } from 'vitest';
import {
  REFERENCES,
  filterReferences,
  resolveReferenceType,
} from '@/lib/ai/references';

const ACTIVE_REFERENCES = ['buffer', 'selection', 'error', 'pane', 'cwd'];
const REMOVED_REFERENCES = ['file', 'env', 'history'];

describe('REFERENCES registry', () => {
  it('only exposes real snapshot references', () => {
    expect(REFERENCES.map(r => r.type)).toEqual(ACTIVE_REFERENCES);
  });

  it('resolves every active reference', () => {
    for (const reference of REFERENCES) {
      expect(resolveReferenceType(reference.type)).toBe(reference);
    }
  });

  it('does not resolve removed pseudo references', () => {
    for (const type of REMOVED_REFERENCES) {
      expect(resolveReferenceType(type)).toBeUndefined();
    }
  });

  it('filters by prefix case-insensitively', () => {
    expect(filterReferences('pa').map(r => r.type)).toEqual(['pane']);
    expect(filterReferences('PA').map(r => r.type)).toEqual(['pane']);
  });
});
