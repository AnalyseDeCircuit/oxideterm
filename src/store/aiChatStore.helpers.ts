import { parseSuggestions } from '../lib/ai/suggestionParser';
import type { ChatMessage as ProviderChatMessage } from '../lib/ai/providers';
import type { AiConversation, AiChatMessage, AiToolCall } from '../types';
import type {
  AiAssistantTurn,
  AiConversationSessionMetadata,
  AiTranscriptReference,
} from '../lib/ai/turnModel/types';
import { projectLegacyMessageToTurn } from '../lib/ai/turnModel/turnProjection';

export interface FullConversationDto {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  sessionId: string | null;
  origin?: string;
  sessionMetadata?: AiConversationSessionMetadata | null;
  messages: Array<{
    id: string;
    role: 'user' | 'assistant' | 'system' | 'tool';
    content: string;
    timestamp: number;
    toolCalls?: AiToolCall[];
    context: string | null;
    turn?: AiAssistantTurn | null;
    transcriptRef?: AiTranscriptReference | null;
  }>;
}

interface ParsedResponse {
  content: string;
  thinkingContent?: string;
}

const ANCHOR_META_HEADER = '$$ANCHOR_B64$$';
const CONDENSE_KEEP_RECENT = 5;
const CONDENSE_SUMMARY_MAX = 300;

export function generateTitle(firstMessage: string): string {
  const cleaned = firstMessage.replace(/\n/g, ' ').trim();
  return cleaned.length > 30 ? cleaned.slice(0, 30) + '...' : cleaned;
}

export function dtoToConversation(dto: FullConversationDto): AiConversation {
  return hydrateStructuredConversation({
    id: dto.id,
    title: dto.title,
    createdAt: dto.createdAt,
    updatedAt: dto.updatedAt,
    sessionId: dto.sessionId ?? undefined,
    origin: dto.origin || 'sidebar',
    sessionMetadata: dto.sessionMetadata ?? {
      conversationId: dto.id,
      origin: dto.origin || 'sidebar',
    },
    messages: dto.messages.map((m) => {
      if (m.role === 'assistant') {
        let content = m.content;
        let thinkingContent: string | undefined;
        if (content.includes('<thinking>')) {
          const parsed = parseThinkingContent(content);
          content = parsed.content;
          thinkingContent = parsed.thinkingContent;
        }
        const sugResult = parseSuggestions(content);
        return {
          id: m.id,
          role: m.role as 'assistant',
          content: sugResult.cleanContent,
          thinkingContent,
          toolCalls: m.toolCalls,
          timestamp: m.timestamp,
          context: m.context || undefined,
          turn: m.turn ?? undefined,
          transcriptRef: m.transcriptRef ?? undefined,
          ...(sugResult.suggestions.length > 0 ? { suggestions: sugResult.suggestions } : {}),
        };
      }

      if (m.role === 'system') {
        const anchor = decodeAnchorContent(m.content);
        if (anchor) {
          return {
            id: m.id,
            role: m.role as 'system',
            content: anchor.content,
            timestamp: m.timestamp,
            context: m.context || undefined,
            metadata: anchor.metadata,
          };
        }
      }

      return {
        id: m.id,
        role: m.role as AiChatMessage['role'],
        content: m.content,
        toolCalls: m.toolCalls,
        timestamp: m.timestamp,
        context: m.context || undefined,
        turn: m.turn ?? undefined,
        transcriptRef: m.transcriptRef ?? undefined,
      };
    }),
  });
}

export function hydrateStructuredConversation(conversation: AiConversation): AiConversation {
  const existingTurns = conversation.turns ?? [];
  const usedTurnIds = new Set<string>();
  const turns: NonNullable<AiConversation['turns']> = [];
  let lastUserMessage: AiChatMessage | undefined;

  const messages = conversation.messages.map((message) => {
    if (message.role === 'user') {
      lastUserMessage = message;
      return message;
    }

    if (message.role !== 'assistant') {
      return message;
    }

    const turn = message.turn ?? projectLegacyMessageToTurn(message);
    const expectedTranscriptRef = {
      conversationId: conversation.id,
      startEntryId: lastUserMessage?.id,
      endEntryId: message.id,
    };
    const transcriptRef = message.transcriptRef
      && message.transcriptRef.conversationId === expectedTranscriptRef.conversationId
      && message.transcriptRef.startEntryId === expectedTranscriptRef.startEntryId
      && message.transcriptRef.endEntryId === expectedTranscriptRef.endEntryId
      ? message.transcriptRef
      : expectedTranscriptRef;
    const matchingExistingTurn = existingTurns.find((existingTurn) => {
      if (usedTurnIds.has(existingTurn.id)) return false;
      return existingTurn.requestMessageId === (lastUserMessage?.id ?? message.id);
    });

    if (matchingExistingTurn) {
      usedTurnIds.add(matchingExistingTurn.id);
    }

    const mergedRounds = turn.toolRounds.length > 0
      ? turn.toolRounds.map((round, index) => {
          const existingRound = matchingExistingTurn?.rounds.find((candidate) => candidate.id === round.id || candidate.round === round.round)
            ?? matchingExistingTurn?.rounds[index];

          if (!existingRound) {
            return round;
          }

          return {
            ...existingRound,
            ...round,
            responseText: round.responseText ?? existingRound.responseText,
            retryCount: round.retryCount ?? existingRound.retryCount,
            timestamp: round.timestamp ?? existingRound.timestamp,
            statefulMarker: round.statefulMarker ?? existingRound.statefulMarker,
            summary: round.summary ?? existingRound.summary,
            summaryMetadata: round.summaryMetadata ?? existingRound.summaryMetadata,
            toolCalls: round.toolCalls.map((toolCall, toolIndex) => ({
              ...(existingRound.toolCalls.find((candidate) => candidate.id === toolCall.id) ?? existingRound.toolCalls[toolIndex] ?? {}),
              ...toolCall,
            })),
          };
        })
      : (matchingExistingTurn?.rounds ?? []);

    turns.push({
      id: matchingExistingTurn?.id ?? `${lastUserMessage?.id ?? message.id}-${message.id}`,
      requestMessageId: matchingExistingTurn?.requestMessageId ?? lastUserMessage?.id ?? message.id,
      requestText: matchingExistingTurn?.requestText ?? lastUserMessage?.content ?? '',
      startedAt: matchingExistingTurn?.startedAt ?? lastUserMessage?.timestamp ?? message.timestamp,
      status: turn.status,
      rounds: mergedRounds,
      pendingSummaries: matchingExistingTurn?.pendingSummaries ?? [],
    });

    return {
      ...message,
      turn,
      transcriptRef,
    };
  });

  const firstUserMessage = messages.find((message) => message.role === 'user')?.content;

  return {
    ...conversation,
    messages,
    turns,
    sessionMetadata: {
      ...conversation.sessionMetadata,
      conversationId: conversation.id,
      origin: conversation.origin ?? conversation.sessionMetadata?.origin ?? 'sidebar',
      ...(conversation.sessionMetadata?.firstUserMessage === undefined && firstUserMessage
        ? { firstUserMessage }
        : {}),
    },
  };
}

export function parseThinkingContent(rawContent: string): ParsedResponse {
  const thinkingRegex = /<thinking>([\s\S]*?)<\/thinking>/gi;
  let thinkingContent = '';
  let content = rawContent;

  let match;
  while ((match = thinkingRegex.exec(rawContent)) !== null) {
    if (thinkingContent) thinkingContent += '\n\n';
    thinkingContent += match[1].trim();
  }

  if (thinkingContent) {
    content = rawContent.replace(thinkingRegex, '').trim();
  }

  return {
    content,
    thinkingContent: thinkingContent || undefined,
  };
}

export function encodeAnchorContent(content: string, metadata: NonNullable<AiChatMessage['metadata']>): string {
  const metaJson = JSON.stringify(metadata);
  const b64 = btoa(unescape(encodeURIComponent(metaJson)));
  return `${ANCHOR_META_HEADER}${b64}\n${content}`;
}

export function decodeAnchorContent(content: string): { content: string; metadata: NonNullable<AiChatMessage['metadata']> } | null {
  if (!content.startsWith(ANCHOR_META_HEADER)) return null;
  const newlineIdx = content.indexOf('\n');
  if (newlineIdx === -1) return null;
  try {
    const b64 = content.slice(ANCHOR_META_HEADER.length, newlineIdx);
    const jsonStr = decodeURIComponent(escape(atob(b64)));
    const metadata = JSON.parse(jsonStr);
    const realContent = content.slice(newlineIdx + 1);
    return { content: realContent, metadata };
  } catch {
    return null;
  }
}

type ChatCompletionMessage = ProviderChatMessage;

export function condenseToolMessages(apiMessages: ChatCompletionMessage[]): void {
  const toolIndices: number[] = [];
  for (let i = 0; i < apiMessages.length; i++) {
    if (apiMessages[i].role === 'tool') {
      toolIndices.push(i);
    }
  }

  if (toolIndices.length <= CONDENSE_KEEP_RECENT) return;

  const toCondense = toolIndices.slice(0, -CONDENSE_KEEP_RECENT);
  for (const idx of toCondense) {
    const msg = apiMessages[idx];
    const content = msg.content;
    const toolName = msg.tool_name || 'tool';

    if (content.startsWith('[condensed]')) continue;

    let isError = false;
    try {
      const parsed = JSON.parse(content);
      isError = typeof parsed === 'object' && parsed !== null && 'error' in parsed && !!parsed.error;
    } catch {
      // Plain-text success output.
    }

    if (isError) continue;

    const lines = content.split('\n').filter((line) => line.trim().length > 0);
    let summary: string;
    if (lines.length <= 4) {
      summary = lines.join('\n');
    } else {
      const head = lines.slice(0, 2).join('\n');
      const tail = lines.slice(-2).join('\n');
      summary = `${head}\n... (${lines.length - 4} lines omitted)\n${tail}`;
    }

    if (summary.length > CONDENSE_SUMMARY_MAX) {
      summary = summary.slice(0, CONDENSE_SUMMARY_MAX) + '…';
    }

    msg.content = `[condensed] ${toolName} → ok:\n${summary}`;
  }
}