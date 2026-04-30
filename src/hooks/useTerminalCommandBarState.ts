// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  beginTerminalCommandMark,
  broadcastToTargets,
  subscribeTerminalOutput,
} from '@/lib/terminalRegistry';
import {
  getCommandBarCompletions,
  type CommandBarCompletion,
} from '@/lib/terminal/completion';

export type TerminalCommandBarTerminalType = 'terminal' | 'local_terminal';

type UseTerminalCommandBarStateOptions = {
  paneId: string;
  sessionId: string;
  tabId: string;
  terminalType: TerminalCommandBarTerminalType;
  nodeId?: string | null;
  isActive: boolean;
  sendInput: (input: string) => void;
};

export function useTerminalCommandBarState(options: UseTerminalCommandBarStateOptions) {
  const { paneId, sessionId, tabId, terminalType, nodeId, isActive, sendInput } = options;
  const { t } = useTranslation();
  const [value, setValue] = useState('');
  const [cursorIndex, setCursorIndex] = useState(0);
  const [suggestions, setSuggestions] = useState<CommandBarCompletion[]>([]);
  const [focused, setFocused] = useState(false);
  const [inputComposing, setInputComposing] = useState(false);
  const [gitBranch, setGitBranch] = useState<string | null>(null);
  const [terminalActivityTick, setTerminalActivityTick] = useState(0);
  const completionRequestRef = useRef(0);
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

  useEffect(() => {
    if (!commandBarSettings.enabled || !commandBarSettings.smartCompletion || !focused || inputComposing) {
      setSuggestions([]);
      return;
    }

    const controller = new AbortController();
    const requestId = ++completionRequestRef.current;
    void getCommandBarCompletions(
      value,
      cursorIndex,
      { paneId, sessionId, tabId, terminalType, nodeId, cwd, cwdHost, targetLabel },
      controller.signal,
    ).then((nextSuggestions) => {
      if (!controller.signal.aborted && requestId === completionRequestRef.current) {
        setSuggestions(nextSuggestions);
      }
    });

    return () => controller.abort();
  }, [
    commandBarSettings.enabled,
    commandBarSettings.smartCompletion,
    cursorIndex,
    cwd,
    cwdHost,
    focused,
    inputComposing,
    nodeId,
    paneId,
    sessionId,
    tabId,
    terminalType,
    targetLabel,
    value,
  ]);

  const revealHistorySuggestions = useCallback(async () => {
    if (!commandBarSettings.enabled || !commandBarSettings.smartCompletion || !focused || inputComposing) return 0;
    const controller = new AbortController();
    const requestId = ++completionRequestRef.current;
    const nextSuggestions = await getCommandBarCompletions(
      value,
      cursorIndex,
      { paneId, sessionId, tabId, terminalType, nodeId, cwd, cwdHost, targetLabel },
      controller.signal,
      {
        // Empty focus must stay quiet, but ArrowUp/ArrowDown is an explicit
        // history-recall gesture. Keep this opt-in so an empty command bar does
        // not regress into auto-opening the full smart completion catalog.
        allowEmptyHistory: true,
        historyOnly: true,
      },
    );
    if (controller.signal.aborted || requestId !== completionRequestRef.current) return 0;
    setSuggestions(nextSuggestions);
    return nextSuggestions.length;
  }, [
    commandBarSettings.enabled,
    commandBarSettings.smartCompletion,
    cursorIndex,
    cwd,
    cwdHost,
    focused,
    inputComposing,
    nodeId,
    paneId,
    sessionId,
    tabId,
    terminalType,
    targetLabel,
    value,
  ]);

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

  const submitCommand = useCallback((commandOverride?: string) => {
    const command = (commandOverride ?? value).trim();
    if (!command || !isActive) return false;
    const payload = `${command.replace(/\r?\n/g, '\r')}\r`;
    beginTerminalCommandMark(paneId, {
      command,
      source: 'command_bar',
      sessionId,
      cwd,
    });
    sendInput(payload);
    const broadcast = useBroadcastStore.getState();
    if (broadcast.enabled) {
      broadcastToTargets(paneId, payload, broadcast.targets, {
        commandMark: {
          command,
          source: 'broadcast',
          cwd,
        },
      });
    }
    setValue('');
    setCursorIndex(0);
    setSuggestions([]);
    return true;
  }, [cwd, isActive, paneId, sendInput, sessionId, value]);

  const acceptSuggestion = useCallback((candidate?: CommandBarCompletion) => {
    const completion = candidate ?? suggestions[0];
    if (!completion) return false;
    const start = Math.max(0, Math.min(value.length, completion.replacement.start));
    const end = Math.max(start, Math.min(value.length, completion.replacement.end));
    const next = [
      value.slice(0, start),
      completion.insertText,
      value.slice(end),
    ].join('');
    setValue(next);
    setCursorIndex(start + completion.insertText.length);
    setSuggestions([]);
    return true;
  }, [suggestions, value]);

  const ghostText = useMemo(() => {
    const completion = suggestions.find((candidate) => candidate.inlineSafe);
    if (!completion) return '';
    const current = value.slice(completion.replacement.start, completion.replacement.end);
    if (!completion.insertText.startsWith(current)) return '';
    return completion.insertText.slice(current.length);
  }, [suggestions, value]);

  return {
    value,
    setValue,
    cursorIndex,
    setCursorIndex,
    focused,
    setFocused,
    inputComposing,
    setInputComposing,
    suggestions,
    ghostText,
    revealHistorySuggestions,
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
