// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { Channel, invoke } from '@tauri-apps/api/core';
import type { AiStreamEvent, ChatMessage } from '../providers';
import type { AcpAgentConfig } from './acpTypes';

type AcpBackendStreamEvent = AiStreamEvent;

type AcpSessionStartedEvent = {
  type: 'session_started';
  sessionId: string;
  sessionMetadata?: Record<string, unknown> | null;
};

export type AcpPermissionRequestedEvent = {
  type: 'permission_requested';
  requestId: string;
  toolCallId: string;
  name: string;
  arguments: string;
  summary: string;
  risk: string;
  options: Array<{
    optionId: string;
    name: string;
    kind: 'allow_once' | 'allow_always' | 'reject_once' | 'reject_always' | 'unknown';
  }>;
};

type AcpStreamCompletionConfig = {
  agent: AcpAgentConfig;
  conversationId: string;
  generationId: string;
  existingSessionId?: string | null;
  onSessionStarted?: (event: AcpSessionStartedEvent) => void | Promise<void>;
  onPermissionRequested?: (event: AcpPermissionRequestedEvent) => string | null | Promise<string | null>;
};

const ACP_DEFAULT_SESSION_CWD = '.';

function acpPromptFromMessages(messages: ChatMessage[]): string {
  return [...messages].reverse().find((message) => message.role === 'user')?.content.trim() ?? '';
}

function acpRequestId(): string {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `acp-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function acpLaunchConfig(agent: AcpAgentConfig) {
  return {
    id: agent.id,
    displayName: agent.displayName || agent.id,
    command: agent.command,
    args: agent.args ?? [],
    env: agent.env ?? {},
    cwd: agent.cwd ?? null,
  };
}

export async function* streamAcpCompletion(
  config: AcpStreamCompletionConfig,
  messages: ChatMessage[],
  signal?: AbortSignal,
): AsyncGenerator<AiStreamEvent> {
  const prompt = acpPromptFromMessages(messages);
  if (!prompt) {
    yield { type: 'error', message: 'Cannot start ACP agent without a user prompt.' };
    return;
  }

  const queue: AiStreamEvent[] = [];
  let wake: (() => void) | null = null;
  let closed = false;
  let thrownError: Error | null = null;
  const requestId = acpRequestId();
  const notify = () => {
    wake?.();
    wake = null;
  };
  const push = (event: AiStreamEvent) => {
    queue.push(event);
    notify();
  };

  const channel = new Channel<AcpBackendStreamEvent | AcpSessionStartedEvent | AcpPermissionRequestedEvent>();
  channel.onmessage = (event: AcpBackendStreamEvent | AcpSessionStartedEvent | AcpPermissionRequestedEvent) => {
    if (event.type === 'session_started') {
      void config.onSessionStarted?.(event);
      return;
    }
    if (event.type === 'permission_requested') {
      void Promise.resolve(config.onPermissionRequested?.(event) ?? null)
        .then((optionId) => invoke('acp_stream_permission_respond', {
          requestId: event.requestId,
          optionId: optionId ?? null,
        }))
        .catch(() => invoke('acp_stream_permission_respond', {
          requestId: event.requestId,
          optionId: null,
        }).catch(() => {
          // If the stream is already closing, the Rust side will cancel any pending responder.
        }));
      return;
    }
    push(event);
  };

  const onAbort = () => {
    invoke('acp_stream_prompt_cancel', { requestId }).catch(() => {
      // Cancellation is best effort; local stream closure still prevents stale UI updates.
    });
    closed = true;
    notify();
  };
  signal?.addEventListener('abort', onAbort, { once: true });

  invoke('acp_stream_prompt', {
    request: {
      requestId,
      launchConfig: acpLaunchConfig(config.agent),
      capabilityPolicy: config.agent.capabilityPolicy,
      sessionCwd: config.agent.cwd || ACP_DEFAULT_SESSION_CWD,
      existingSessionId: config.existingSessionId ?? null,
      prompt,
      conversationId: config.conversationId,
      generationId: config.generationId,
    },
    onEvent: channel,
  })
    .catch((error) => {
      thrownError = error instanceof Error ? error : new Error(String(error));
      push({ type: 'error', message: thrownError.message });
    })
    .finally(() => {
      closed = true;
      notify();
    });

  try {
    while (!closed || queue.length > 0) {
      if (queue.length === 0) {
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
        continue;
      }
      const event = queue.shift();
      if (event) {
        yield event;
      }
    }
    if (thrownError) {
      throw thrownError;
    }
  } finally {
    signal?.removeEventListener('abort', onAbort);
  }
}
