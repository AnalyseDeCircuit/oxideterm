import type { AiChatMessage, AiConversation, AiToolCall } from '../types';
import { hydrateStructuredConversation } from './aiChatStore.helpers';

export type PendingApprovalEntry = {
  runId: string;
  conversationId: string;
  assistantMessageId: string;
  resolve: (approved: boolean) => void;
};

export type AiSafetyMode = 'default' | 'bypass';

// Module-scoped runtime state that should not survive test resets.
export const compactingConversations = new Set<string>();
export const pendingApprovalResolvers = new Map<string, PendingApprovalEntry>();

function updateStructuredToolCallStatus(
  message: AiChatMessage,
  toolCallId: string,
  status: AiToolCall['status'],
): AiChatMessage {
  if (
    message.role !== 'assistant'
    || !message.turn?.toolRounds.some((round) => round.toolCalls.some((toolCall) => toolCall.id === toolCallId))
  ) {
    return message;
  }

  return {
    ...message,
    turn: {
      ...message.turn,
      toolRounds: message.turn.toolRounds.map((round) => ({
        ...round,
        toolCalls: round.toolCalls.map((toolCall) => {
          if (toolCall.id !== toolCallId) return toolCall;

          if (status === 'approved') {
            return {
              ...toolCall,
              approvalState: 'approved',
              executionState: undefined,
            };
          }

          if (status === 'rejected') {
            return {
              ...toolCall,
              approvalState: 'rejected',
              executionState: undefined,
            };
          }

          return toolCall;
        }),
      })),
    },
  };
}

export function updateToolCallStatusInConversations(
  conversations: AiConversation[],
  conversationId: string,
  assistantMessageId: string,
  toolCallId: string,
  status: AiToolCall['status'],
): AiConversation[] {
  return conversations.map((conversation) => {
    if (conversation.id !== conversationId) return conversation;

    let conversationChanged = false;
    const messages = conversation.messages.map((message) => {
      if (message.id !== assistantMessageId || message.role !== 'assistant') {
        return message;
      }

      const hasLegacyToolCall = message.toolCalls?.some((toolCall) => toolCall.id === toolCallId) ?? false;
      const hasStructuredToolCall = message.turn?.toolRounds.some((round) => round.toolCalls.some((toolCall) => toolCall.id === toolCallId)) ?? false;
      if (!hasLegacyToolCall && !hasStructuredToolCall) {
        return message;
      }

      conversationChanged = true;
      const nextMessage = hasLegacyToolCall
        ? {
            ...message,
            toolCalls: message.toolCalls?.map((toolCall) => (
              toolCall.id === toolCallId ? { ...toolCall, status } : toolCall
            )),
          }
        : message;

      return hasStructuredToolCall
        ? updateStructuredToolCallStatus(nextMessage, toolCallId, status)
        : nextMessage;
    });

    return conversationChanged
      ? hydrateStructuredConversation({ ...conversation, messages })
      : conversation;
  });
}

export function resetAiChatStoreRuntimeState() {
  compactingConversations.clear();
  for (const [, entry] of pendingApprovalResolvers) {
    entry.resolve(false);
  }
  pendingApprovalResolvers.clear();
}
