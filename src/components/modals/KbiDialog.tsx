/**
 * Keyboard-Interactive (2FA) Authentication Dialog
 *
 * This is a simple MVP modal for handling SSH keyboard-interactive authentication.
 * It displays server prompts and collects user responses with strict timeout awareness.
 *
 * Design principles:
 * - Completely isolated from normal password/key auth flows
 * - Supports both echo=true (visible) and echo=false (password) inputs
 * - 60s timeout enforced by backend - dialog should communicate urgency
 * - No auto-retry on failure - user must manually reconnect
 */

import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
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

interface KbiDialogProps {
  /** Called when authentication succeeds */
  onSuccess: (sessionId: string, wsPort: number, wsToken: string) => void;
  /** Called when authentication fails or is cancelled */
  onFailure: (error: string) => void;
}

export const KbiDialog = ({ onSuccess, onFailure }: KbiDialogProps) => {
  const { t } = useTranslation();
  // Current prompt state
  const [currentPrompt, setCurrentPrompt] = useState<KbiPromptEvent | null>(null);
  const [responses, setResponses] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [timeLeft, setTimeLeft] = useState(60);

  // Track listeners for cleanup
  const listenersRef = useRef<UnlistenFn[]>([]);
  const timerRef = useRef<NodeJS.Timeout | null>(null);
  // Track current auth flow ID for cleanup cancellation
  const currentAuthFlowIdRef = useRef<string | null>(null);

  // Set up event listeners
  useEffect(() => {
    let mounted = true;

    // Listen for prompt events from backend
    listen<KbiPromptEvent>('ssh_kbi_prompt', (event) => {
      if (!mounted) return;
      // Track the auth flow ID for cleanup
      currentAuthFlowIdRef.current = event.payload.authFlowId;
      setCurrentPrompt(event.payload);
      setResponses(new Array(event.payload.prompts.length).fill(''));
      setLoading(false);
      setError(null);
      setTimeLeft(60);

      // Start countdown timer
      if (timerRef.current) {
        clearInterval(timerRef.current);
      }
      timerRef.current = setInterval(() => {
        setTimeLeft((prev) => {
          if (prev <= 1) {
            if (timerRef.current) clearInterval(timerRef.current);
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
    }).then((fn) => {
      if (mounted) {
        listenersRef.current.push(fn);
      } else {
        fn(); // Component unmounted, clean up immediately
      }
    });

    // Listen for result events from backend
    listen<KbiResultEvent>('ssh_kbi_result', (event) => {
      if (!mounted) return;
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }

      const result = event.payload;
      // Clear the auth flow ID since authentication completed
      currentAuthFlowIdRef.current = null;
      if (result.success && result.sessionId && result.wsPort && result.wsToken) {
        setCurrentPrompt(null);
        onSuccess(result.sessionId, result.wsPort, result.wsToken);
      } else {
        setError(result.error || 'Authentication failed');
        setLoading(false);
        // Don't close dialog - let user see the error
        // They can click Cancel to dismiss
      }
    }).then((fn) => {
      if (mounted) {
        listenersRef.current.push(fn);
      } else {
        fn(); // Component unmounted, clean up immediately
      }
    });

    return () => {
      mounted = false;
      // Cleanup listeners
      listenersRef.current.forEach((unlisten) => unlisten());
      listenersRef.current = [];

      // Cleanup timer
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }

      // CRITICAL: Cancel pending auth flow if component unmounts during authentication
      // This prevents the backend from waiting 60s for a response that will never come
      if (currentAuthFlowIdRef.current) {
        const authFlowId = currentAuthFlowIdRef.current;
        currentAuthFlowIdRef.current = null;
        invoke('ssh_kbi_cancel', { 
          request: { authFlowId } as KbiCancelRequest 
        }).catch(() => {
          // Ignore errors - the flow may have already completed or timed out
        });
      }
    };
  }, [onSuccess]);

  // Handle response submission
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!currentPrompt || loading) return;

    setLoading(true);
    setError(null);

    try {
      const request: KbiRespondRequest = {
        authFlowId: currentPrompt.authFlowId,
        responses: responses,
      };
      await invoke('ssh_kbi_respond', { request });
      // Result will come via ssh_kbi_result event
    } catch (err) {
      setError(String(err));
      setLoading(false);
    }
  };

  // Handle cancel
  const handleCancel = async () => {
    if (!currentPrompt) return;

    try {
      const request: KbiCancelRequest = {
        authFlowId: currentPrompt.authFlowId,
      };
      await invoke('ssh_kbi_cancel', { request });
    } catch {
      // Ignore cancel errors
    }

    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }

    setCurrentPrompt(null);
    onFailure('Authentication cancelled by user');
  };

  // Update response at index
  const updateResponse = (index: number, value: string) => {
    setResponses((prev) => {
      const next = [...prev];
      next[index] = value;
      return next;
    });
  };

  // Check if all required responses are filled
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
              <span className="font-medium text-zinc-300">{currentPrompt.name}</span>
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
              : 'bg-zinc-800/50 text-zinc-400'
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
          {/* Prompt inputs */}
          {currentPrompt?.prompts.map((prompt, index) => (
            <div key={index} className="space-y-2">
              <Label htmlFor={`kbi-input-${index}`}>{prompt.prompt}</Label>
              <Input
                id={`kbi-input-${index}`}
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
