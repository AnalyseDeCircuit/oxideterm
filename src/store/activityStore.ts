// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { create } from 'zustand';

export type ActivityView = 'notifications' | 'event_log';

type ActivityStore = {
  activeView: ActivityView;
  setActiveView: (view: ActivityView) => void;
};

export const useActivityStore = create<ActivityStore>((set) => ({
  activeView: 'notifications',
  setActiveView: (view) => set({ activeView: view }),
}));