// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/appStore';
import { useBroadcastStore } from '@/store/broadcastStore';
import { useRecordingStore } from '@/store/recordingStore';
import { useSettingsStore } from '@/store/settingsStore';
import { api } from '@/lib/api';
import {
  getCwd,
  getCwdHost,
  getTerminalBuffer,
  broadcastToTargets,
  subscribeTerminalOutput,
} from '@/lib/terminalRegistry';
import {
  getTerminalAutosuggestCandidates,
} from '@/lib/terminal/autosuggest';
import type { TerminalAutosuggestCandidate } from '@/lib/terminal/autosuggest';

export type TerminalCommandBarTerminalType = 'terminal' | 'local_terminal';

type UseTerminalCommandBarStateOptions = {
  paneId: string;
  sessionId: string;
  tabId: string;
  terminalType: TerminalCommandBarTerminalType;
  isActive: boolean;
  sendInput: (input: string) => void;
};

export function useTerminalCommandBarState(options: UseTerminalCommandBarStateOptions) {
  const { paneId, sessionId, tabId, terminalType, isActive, sendInput } = options;
  const { t } = useTranslation();
  const [value, setValue] = useState('');
  const [focused, setFocused] = useState(false);
  const [gitBranch, setGitBranch] = useState<string | null>(null);
  const [terminalActivityTick, setTerminalActivityTick] = useState(0);
  const commandBarSettings = useSettingsStore((s) => s.settings.terminal.commandBar);
  const broadcastEnabled = useBroadcastStore((s) => s.enabled);
  const broadcastTargets = useBroadcastStore((s) => s.targets);
  const isRecording = useRecordingStore((s) => s.isRecording(sessionId));
  const session = useAppStore((s) => s.sessions.get(sessionId));
  const tab = useAppStore((s) => s.tabs.find((candidate) => candidate.id === tabId));

  const cwd = getCwd(paneId);
  const cwdHost = getCwdHost(paneId);
  const visibleBuffer = terminalType === 'local_terminal'
    ? getTerminalBuffer(paneId, tabId)
    : null;
  const targetLabel = useMemo(() => {
    if (terminalType === 'local_terminal') {
      const sshIdentity = inferSshIdentityFromLocalBuffer(visibleBuffer ?? '');
      if (sshIdentity) return sshIdentity;
      if (cwdHost && looksLikeRemoteCwd(cwd)) return cwdHost;
      return t('terminal.command_bar.local_shell');
    }
    if (session?.username && session?.host) {
      return `${session.username}@${session.host}`;
    }
    return tab?.title ?? t('terminal.command_bar.remote_shell');
  }, [cwd, cwdHost, session?.host, session?.username, t, tab?.title, terminalActivityTick, terminalType, visibleBuffer]);

  useEffect(() => {
    let timer: number | null = null;
    const unsubscribe = subscribeTerminalOutput(sessionId, () => {
      if (timer) return;
      timer = window.setTimeout(() => {
        timer = null;
        setTerminalActivityTick((tick) => tick + 1);
      }, 250);
    });
    return () => {
      if (timer) window.clearTimeout(timer);
      unsubscribe();
    };
  }, [sessionId]);

  const suggestions = useMemo<TerminalAutosuggestCandidate[]>(() => {
    if (!commandBarSettings.enabled) return [];
    return getTerminalAutosuggestCandidates(value, 6);
  }, [commandBarSettings.enabled, value]);

  useEffect(() => {
    if (!commandBarSettings.gitStatus || terminalType !== 'local_terminal' || !cwd) {
      setGitBranch(null);
      return;
    }

    let cancelled = false;
    const timeout = window.setTimeout(() => {
      void api.localExecCommand('git rev-parse --abbrev-ref HEAD 2>/dev/null', cwd, 1, false)
        .then((result) => {
          if (cancelled || result.exitCode !== 0) return;
          const branch = result.stdout.trim().split(/\r?\n/)[0];
          setGitBranch(branch && branch !== 'HEAD' ? branch : null);
        })
        .catch(() => {
          if (!cancelled) setGitBranch(null);
        });
    }, 250);

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, [commandBarSettings.gitStatus, cwd, terminalType]);

  const submitCommand = useCallback(() => {
    const command = value.trim();
    if (!command || !isActive) return false;
    const payload = `${command}\r`;
    sendInput(payload);
    const broadcast = useBroadcastStore.getState();
    if (broadcast.enabled) {
      broadcastToTargets(paneId, payload, broadcast.targets);
    }
    setValue('');
    return true;
  }, [isActive, paneId, sendInput, value]);

  const acceptSuggestion = useCallback((candidate?: TerminalAutosuggestCandidate) => {
    const next = candidate?.command ?? suggestions[0]?.command;
    if (!next) return false;
    setValue(next);
    return true;
  }, [suggestions]);

  return {
    value,
    setValue,
    focused,
    setFocused,
    suggestions,
    acceptSuggestion,
    submitCommand,
    cwd,
    targetLabel,
    chips: {
      broadcastEnabled,
      broadcastTargetCount: broadcastTargets.size,
      isRecording,
      gitBranch,
    },
  };
}

function looksLikeRemoteCwd(cwd: string | null): boolean {
  if (!cwd) return false;
  return cwd.startsWith('/home/') || cwd.startsWith('/root/') || cwd.startsWith('/srv/') || cwd.startsWith('/var/www/');
}

function inferSshIdentityFromLocalBuffer(buffer: string): string | null {
  if (!buffer) return null;
  const tail = buffer.slice(-8000);
  const matches = Array.from(tail.matchAll(/(?:^|\s)([A-Za-z0-9._-]{1,64}@[A-Za-z0-9][A-Za-z0-9._-]{1,127})(?=[:\s~#$>])/gm));
  if (matches.length === 0) return null;
  return matches[matches.length - 1]?.[1] ?? null;
}
