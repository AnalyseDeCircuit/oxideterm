// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Global Keyboard-Interactive Dialog for Multi-Step Auth Chaining
 *
 * Always mounted in App.tsx. Handles all SSH keyboard-interactive prompts,
 * including standalone direct auth and multi-step auth chaining.
 *
 * Session creation is handled by the surrounding connect flow. This dialog
 * only collects user responses and forwards them to the backend.
 */

import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter
} from '../ui/dialog';
import { Loader2, Shield, Clock } from 'lucide-react';
import type { KbiPromptEvent, KbiResultEvent, KbiRespondRequest, KbiCancelRequest } from '../../types';

export const GlobalKbiDialog = () => {
  const { t } = useTranslation();
  const [currentPrompt, setCurrentPrompt] = useState<KbiPromptEvent | null>(null);
  const [responses, setResponses] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [timeLeft, setTimeLeft] = useState(60);

  const listenersRef = useRef<UnlistenFn[]>([]);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const currentAuthFlowIdRef = useRef<string | null>(null);

  const clearTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  };

  const activatePrompt = (prompt: KbiPromptEvent) => {
    currentAuthFlowIdRef.current = prompt.authFlowId;
    setCurrentPrompt(prompt);
    setResponses(new Array(prompt.prompts.length).fill(''));
    setLoading(false);
    setError(null);
    setTimeLeft(60);

    clearTimer();
    timerRef.current = setInterval(() => {
      setTimeLeft((prev) => {
        if (prev <= 1) {
          clearTimer();
          return 0;
        }

        return prev - 1;
      });
    }, 1000);
  };

  useEffect(() => {
    let mounted = true;

    // Listen for all KBI prompt events
    listen<KbiPromptEvent>('ssh_kbi_prompt', (event) => {
      if (!mounted) return;

      if (!currentAuthFlowIdRef.current) {
        activatePrompt(event.payload);
        return;
      }

      if (currentAuthFlowIdRef.current === event.payload.authFlowId) {
        activatePrompt(event.payload);
        return;
      }

      invoke('ssh_kbi_cancel', {
        request: { authFlowId: event.payload.authFlowId } as KbiCancelRequest,
      }).catch(() => {
        // Ignore errors - the flow may have already completed or timed out
      });
    }).then((fn) => {
      if (mounted) {
        listenersRef.current.push(fn);
      } else {
        fn();
      }
    });

    // Listen for result events to close dialog
    listen<KbiResultEvent>('ssh_kbi_result', (event) => {
      if (!mounted) return;
      // Only handle results for our active flow
      if (currentAuthFlowIdRef.current !== event.payload.authFlowId) return;

      clearTimer();

      if (event.payload.success) {
        currentAuthFlowIdRef.current = null;
        setCurrentPrompt(null);
      } else {
        setError(event.payload.error || 'Authentication failed');
        setLoading(false);
      }
    }).then((fn) => {
      if (mounted) {
        listenersRef.current.push(fn);
      } else {
        fn();
      }
    });

    return () => {
      mounted = false;
      listenersRef.current.forEach((unlisten) => unlisten());
      listenersRef.current = [];

      clearTimer();

      if (currentAuthFlowIdRef.current) {
        const authFlowId = currentAuthFlowIdRef.current;
        currentAuthFlowIdRef.current = null;
        invoke('ssh_kbi_cancel', {
          request: { authFlowId } as KbiCancelRequest,
        }).catch(() => {});
      }
    };
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!currentPrompt || loading) return;

    setLoading(true);
    setError(null);

    try {
      const request: KbiRespondRequest = {
        authFlowId: currentPrompt.authFlowId,
        responses,
      };
      await invoke('ssh_kbi_respond', { request });
    } catch (err) {
      setError(String(err));
      setLoading(false);
    }
  };

  const handleCancel = async () => {
    if (!currentPrompt) return;

    const authFlowId = currentPrompt.authFlowId;

    try {
      const request: KbiCancelRequest = {
        authFlowId,
      };
      await invoke('ssh_kbi_cancel', { request });
    } catch {
      // Ignore cancel errors
    }

    clearTimer();
    currentAuthFlowIdRef.current = null;
    setCurrentPrompt(null);
  };

  const updateResponse = (index: number, value: string) => {
    setResponses((prev) => {
      const next = [...prev];
      next[index] = value;
      return next;
    });
  };

  const allResponsesFilled = responses.every((r) => r.length > 0);

  return (
    <Dialog
      open={!!currentPrompt}
      onOpenChange={(open) => !open && handleCancel()}
    >
      <DialogContent className="sm:max-w-[450px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Shield className="h-5 w-5 text-blue-500" />
            {t('modals.kbi.title')}
          </DialogTitle>
          <DialogDescription>
            {currentPrompt?.name && (
              <span className="font-medium text-theme-text">{currentPrompt.name}</span>
            )}
            {currentPrompt?.instructions && (
              <span className="block mt-1">{currentPrompt.instructions}</span>
            )}
            {!currentPrompt?.name && !currentPrompt?.instructions && (
              <span>{t('modals.kbi.default_instruction')}</span>
            )}
          </DialogDescription>
        </DialogHeader>

        {/* Timeout warning */}
        <div
          className={`flex items-center gap-2 text-xs px-3 py-2 rounded ${
            timeLeft <= 15
              ? 'bg-red-950/50 text-red-400 border border-red-900/50'
              : 'bg-theme-bg-hover/50 text-theme-text-muted'
          }`}
        >
          <Clock className="h-3.5 w-3.5" />
          <span>
            {timeLeft > 0
              ? t('modals.kbi.time_remaining', { seconds: timeLeft })
              : t('modals.kbi.timeout')}
          </span>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {currentPrompt?.prompts.map((prompt, index) => (
            <div key={index} className="space-y-2">
              <Label htmlFor={`global-kbi-input-${index}`}>{prompt.prompt}</Label>
              <Input
                id={`global-kbi-input-${index}`}
                type={prompt.echo ? 'text' : 'password'}
                value={responses[index] || ''}
                onChange={(e) => updateResponse(index, e.target.value)}
                placeholder={prompt.echo ? t('modals.kbi.enter_response') : t('modals.kbi.enter_code')}
                autoFocus={index === 0}
                disabled={loading || timeLeft === 0}
              />
            </div>
          ))}

          {error && (
            <div className="text-sm text-red-400 bg-red-950/30 border border-red-900/50 rounded-sm p-2">
              {error}
            </div>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={handleCancel}
              disabled={loading}
            >
              {t('modals.kbi.cancel')}
            </Button>
            <Button
              type="submit"
              disabled={loading || !allResponsesFilled || timeLeft === 0}
            >
              {loading ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  {t('modals.kbi.verifying')}
                </>
              ) : (
                t('modals.kbi.continue')
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};
