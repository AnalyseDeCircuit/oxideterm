// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export const ACTIVE_PARTICIPANT_NAMES = [
  'terminal',
  'sftp',
  'ide',
  'local',
  'settings',
  'knowledge',
] as const;

export const ACTIVE_REFERENCE_TYPES = [
  'buffer',
  'selection',
  'error',
  'pane',
  'cwd',
] as const;

export type ActiveParticipantName = typeof ACTIVE_PARTICIPANT_NAMES[number];
export type ActiveReferenceType = typeof ACTIVE_REFERENCE_TYPES[number];

export const ACTIVE_PARTICIPANT_NAME_SET = new Set<string>(ACTIVE_PARTICIPANT_NAMES);
export const ACTIVE_REFERENCE_TYPE_SET = new Set<string>(ACTIVE_REFERENCE_TYPES);
