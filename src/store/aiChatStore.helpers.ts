import { parseSuggestions } from '../lib/ai/suggestionParser';
import type { ChatMessage as ProviderChatMessage } from '../lib/ai/providers';
import type { AiConversation, AiChatMessage, AiToolCall, ToolResultEnvelope } from '../types';
import type {
  AiAssistantTurn,
  AiConversationSessionMetadata,
  AiPendingSummary,
  AiSummaryReference,
  AiTranscriptReference,
  AiTurnPart,
  AiTurnSummaryMetadata,
  AiToolRound,
} from '../lib/ai/turnModel/types';
import { projectLegacyMessageToTurn, projectTurnToLegacyMessageFields } from '../lib/ai/turnModel/turnProjection';
import { normalizePendingSummaries } from '../lib/ai/turnModel/summaryMetadata';

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
    model?: string | null;
    toolCalls?: AiToolCall[];
    context: string | null;
    turn?: AiAssistantTurn | null;
    transcriptRef?: AiTranscriptReference | null;
    summaryRef?: AiSummaryReference | null;
  }>;
}

export interface TranscriptEntryDto {
  id: string;
  conversationId: string;
  turnId?: string | null;
  parentId?: string | null;
  timestamp: number;
  kind: string;
  payload: Record<string, unknown>;
}

export interface TranscriptResponseDto {
  entries: TranscriptEntryDto[];
}

interface ParsedResponse {
  content: string;
  thinkingContent?: string;
}

const ANCHOR_META_HEADER = '$$ANCHOR_B64$$';
const CONDENSE_KEEP_RECENT = 5;
const CONDENSE_SUMMARY_MAX = 300;

type TranscriptAssistantState = {
  messageId: string;
  timestamp: number;
  startEntryId?: string;
  endEntryId?: string;
  status: AiAssistantTurn['status'];
  plainTextSummary?: string;
  parts: AiTurnPart[];
  hasCompleteParts?: boolean;
  roundsById: Map<string, AiToolRound>;
  roundOrder: string[];
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function getStringField(payload: Record<string, unknown>, key: string): string | undefined {
  const value = payload[key];
  return typeof value === 'string' ? value : undefined;
}

function getNumberField(payload: Record<string, unknown>, key: string): number | undefined {
  const value = payload[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function getBooleanField(payload: Record<string, unknown>, key: string): boolean | undefined {
  const value = payload[key];
  return typeof value === 'boolean' ? value : undefined;
}

function getEnvelopeField(payload: Record<string, unknown>): ToolResultEnvelope | undefined {
  const envelope = payload.envelope;
  return envelope && typeof envelope === 'object' ? envelope as ToolResultEnvelope : undefined;
}

function isToolTurnPart(part: AiTurnPart): boolean {
  return part.type === 'tool_call' || part.type === 'tool_result';
}

function appendTranscriptPart(assistantState: TranscriptAssistantState, part: AiTurnPart): void {
  if (assistantState.hasCompleteParts) {
    return;
  }
  assistantState.parts.push(part);
}

function appendTranscriptParts(assistantState: TranscriptAssistantState, parts: AiTurnPart[]): void {
  if (assistantState.hasCompleteParts || parts.length === 0) {
    return;
  }
  assistantState.parts.push(...parts);
}

function isSummaryReferenceKind(value: unknown): value is NonNullable<AiSummaryReference['kind']> {
  return value === 'round' || value === 'conversation' || value === 'compaction';
}

function getSummaryReferenceKind(
  payload: Record<string, unknown>,
  fallback?: AiSummaryReference,
): AiSummaryReference['kind'] {
  const explicitKind = payload.summaryKind;
  if (isSummaryReferenceKind(explicitKind)) {
    return explicitKind;
  }

  if (getStringField(payload, 'roundId')) {
    return 'round';
  }

  if (getNumberField(payload, 'compactedMessageCount') !== undefined || getStringField(payload, 'compactedUntilMessageId')) {
    return 'compaction';
  }

  if (getNumberField(payload, 'replacedMessageCount') !== undefined) {
    return 'conversation';
  }

  return fallback?.kind;
}

function extractSummaryMetadata(payload: Record<string, unknown>): AiTurnSummaryMetadata | undefined {
  const metadata: AiTurnSummaryMetadata = {};
  const source = getStringField(payload, 'source');
  const model = getStringField(payload, 'model');
  const summarizationMode = getStringField(payload, 'summarizationMode');
  const durationMs = getNumberField(payload, 'durationMs');
  const contextLengthBefore = getNumberField(payload, 'contextLengthBefore');
  const numRounds = getNumberField(payload, 'numRounds');
  const numRoundsSinceLastSummarization = getNumberField(payload, 'numRoundsSinceLastSummarization');

  if (source === 'foreground' || source === 'background') {
    metadata.source = source;
  }
  if (model) {
    metadata.model = model;
  }
  if (summarizationMode === 'inline' || summarizationMode === 'background' || summarizationMode === 'manual') {
    metadata.summarizationMode = summarizationMode;
  }
  if (durationMs !== undefined) {
    metadata.durationMs = durationMs;
  }
  if (contextLengthBefore !== undefined) {
    metadata.contextLengthBefore = contextLengthBefore;
  }
  if (numRounds !== undefined) {
    metadata.numRounds = numRounds;
  }
  if (numRoundsSinceLastSummarization !== undefined) {
    metadata.numRoundsSinceLastSummarization = numRoundsSinceLastSummarization;
  }

  return Object.keys(metadata).length > 0 ? metadata : undefined;
}

function isRoundSummaryReference(summaryRef: AiSummaryReference | undefined): summaryRef is AiSummaryReference & { roundId: string } {
  return Boolean(summaryRef?.roundId) && (summaryRef?.kind === 'round' || summaryRef?.kind === undefined);
}

function mergePendingSummaries(...summaryLists: ReadonlyArray<readonly AiPendingSummary[]>): AiPendingSummary[] {
  const merged = new Map<string, AiPendingSummary>();
  for (const summaryList of summaryLists) {
    for (const summary of summaryList) {
      merged.set(summary.roundId, summary);
    }
  }
  return [...merged.values()];
}

function getSummaryReference(
  conversationId: string,
  entryId: string,
  payload: Record<string, unknown>,
  fallback?: AiSummaryReference,
): AiSummaryReference | undefined {
  const roundId = getStringField(payload, 'roundId');
  const kind = getSummaryReferenceKind(payload, fallback);
  if (!kind && !roundId) {
    return fallback;
  }

  const sourceStartEntryId = getStringField(payload, 'sourceStartEntryId');
  const sourceEndEntryId = getStringField(payload, 'sourceEndEntryId');
  const fallbackTranscriptRef = fallback?.transcriptRef;

  return {
    kind,
    roundId,
    transcriptRef: {
      conversationId,
      startEntryId: sourceStartEntryId ?? fallbackTranscriptRef?.startEntryId,
      endEntryId: sourceEndEntryId ?? fallbackTranscriptRef?.endEntryId ?? entryId,
    },
  };
}

function ensureTranscriptAssistantState(
  states: Map<string, TranscriptAssistantState>,
  messageId: string,
  timestamp: number,
): TranscriptAssistantState {
  const existing = states.get(messageId);
  if (existing) {
    if (timestamp < existing.timestamp) {
      existing.timestamp = timestamp;
    }
    return existing;
  }

  const created: TranscriptAssistantState = {
    messageId,
    timestamp,
    status: 'streaming',
    parts: [],
    roundsById: new Map<string, AiToolRound>(),
    roundOrder: [],
  };
  states.set(messageId, created);
  return created;
}

function ensureTranscriptRound(
  assistantState: TranscriptAssistantState,
  roundId: string,
  roundNumber?: number,
  timestamp?: number,
): AiToolRound {
  const existing = assistantState.roundsById.get(roundId);
  if (existing) {
    if (roundNumber !== undefined) {
      existing.round = roundNumber;
    }
    if (timestamp !== undefined) {
      existing.timestamp = timestamp;
    }
    return existing;
  }

  const round: AiToolRound = {
    id: roundId,
    round: roundNumber ?? assistantState.roundOrder.length + 1,
    timestamp,
    retryCount: undefined,
    toolCalls: [],
  };
  assistantState.roundsById.set(roundId, round);
  assistantState.roundOrder.push(roundId);
  return round;
}

function buildAssistantMessageFromTranscript(
  conversationId: string,
  assistantState: TranscriptAssistantState,
  fallback?: AiChatMessage,
): AiChatMessage {
  const rounds = assistantState.roundOrder
    .map((roundId) => assistantState.roundsById.get(roundId))
    .filter((round): round is AiToolRound => Boolean(round))
    .sort((left, right) => left.round - right.round);

  const turn: AiAssistantTurn = {
    id: fallback?.turn?.id ?? assistantState.messageId,
    status: assistantState.endEntryId
      ? assistantState.status
      : fallback?.turn?.status ?? 'error',
    parts: (() => {
      const fallbackParts = fallback?.turn?.parts;
      if (
        !assistantState.hasCompleteParts
        && fallbackParts?.some(isToolTurnPart)
        && assistantState.parts.some(isToolTurnPart)
      ) {
        return fallbackParts.slice();
      }
      return assistantState.parts.slice();
    })(),
    toolRounds: rounds,
    plainTextSummary: assistantState.plainTextSummary ?? '',
  };
  return projectAssistantMessage({
    id: assistantState.messageId,
    role: 'assistant',
    content: turn.plainTextSummary,
    timestamp: fallback?.timestamp ?? assistantState.timestamp,
    context: fallback?.context,
    turn,
    transcriptRef: {
      conversationId,
      startEntryId: assistantState.startEntryId,
      endEntryId: assistantState.endEntryId,
    },
    summaryRef: fallback?.summaryRef,
  });
}

export function projectAssistantMessage(message: AiChatMessage): AiChatMessage {
  if (message.role !== 'assistant') {
    return message;
  }

  const turn = message.turn ?? projectLegacyMessageToTurn(message);
  const projected = projectTurnToLegacyMessageFields(turn);

  let content = projected.content;
  let thinkingContent = projected.thinkingContent;

  if (!thinkingContent && content.includes('<thinking>')) {
    const parsed = parseThinkingContent(content);
    content = parsed.content;
    thinkingContent = parsed.thinkingContent;
  }

  const shouldParseSuggestions = !message.isStreaming && !message.isThinkingStreaming;
  const suggestions = message.suggestions
    ? { cleanContent: content, suggestions: message.suggestions }
    : shouldParseSuggestions
      ? parseSuggestions(content)
      : { cleanContent: content, suggestions: [] };
  const nextMessage: AiChatMessage = {
    ...message,
    content: suggestions.cleanContent,
    turn,
  };

  if (thinkingContent) {
    nextMessage.thinkingContent = thinkingContent;
  } else {
    delete nextMessage.thinkingContent;
  }

  if (projected.toolCalls && projected.toolCalls.length > 0) {
    nextMessage.toolCalls = projected.toolCalls;
  } else {
    delete nextMessage.toolCalls;
  }

  if (suggestions.suggestions.length > 0) {
    nextMessage.suggestions = suggestions.suggestions;
  } else {
    delete nextMessage.suggestions;
  }

  return nextMessage;
}

function buildSummaryMessageFromTranscript(
  conversationId: string,
  entry: TranscriptEntryDto,
  payload: Record<string, unknown>,
  fallback?: AiChatMessage,
): AiChatMessage | null {
  const messageId = getStringField(payload, 'messageId');
  const summaryText = getStringField(payload, 'summaryText');
  if (!messageId || !summaryText) {
    return null;
  }

  const transcriptRef = {
    conversationId,
    endEntryId: entry.id,
  };
  const summaryRef = getSummaryReference(conversationId, entry.id, payload, fallback?.summaryRef);
  const compactedMessageCount = getNumberField(payload, 'compactedMessageCount');
  const compactedUntilMessageId = getStringField(payload, 'compactedUntilMessageId');
  const isCompactionSummary = compactedMessageCount !== undefined || compactedUntilMessageId !== undefined;

  if (isCompactionSummary) {
    return {
      id: messageId,
      role: 'system',
      content: summaryText,
      timestamp: fallback?.timestamp ?? entry.timestamp,
      context: fallback?.context,
      transcriptRef,
      summaryRef,
      metadata: {
        type: 'compaction-anchor',
        originalCount: compactedMessageCount ?? fallback?.metadata?.originalCount ?? 0,
        compactedAt: fallback?.metadata?.compactedAt ?? entry.timestamp,
        originalMessages: fallback?.metadata?.originalMessages,
      },
    };
  }

  const suggestions = parseSuggestions(summaryText);
  const nextMessage: AiChatMessage = {
    id: messageId,
    role: 'assistant',
    content: suggestions.cleanContent,
    timestamp: fallback?.timestamp ?? entry.timestamp,
    context: fallback?.context,
    transcriptRef,
    summaryRef,
  };

  if (suggestions.suggestions.length > 0) {
    nextMessage.suggestions = suggestions.suggestions;
  }

  return nextMessage;
}

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
    profileId: dto.sessionMetadata?.profileId,
    sessionMetadata: dto.sessionMetadata ?? {
      conversationId: dto.id,
      origin: dto.origin || 'sidebar',
    },
    messages: dto.messages.map((m) => {
      if (m.role === 'assistant') {
        return projectAssistantMessage({
          id: m.id,
          role: m.role as 'assistant',
          content: m.content,
          timestamp: m.timestamp,
          model: m.model ?? undefined,
          context: m.context || undefined,
          turn: m.turn ?? undefined,
          transcriptRef: m.transcriptRef ?? undefined,
          summaryRef: m.summaryRef ?? undefined,
          ...(m.toolCalls ? { toolCalls: m.toolCalls } : {}),
        });
      }

      if (m.role === 'system') {
        const anchor = decodeAnchorContent(m.content);
        if (anchor) {
          return {
            id: m.id,
            role: m.role as 'system',
            content: anchor.content,
            timestamp: m.timestamp,
            model: m.model ?? undefined,
            context: m.context || undefined,
            metadata: anchor.metadata,
            transcriptRef: m.transcriptRef ?? undefined,
            summaryRef: m.summaryRef ?? undefined,
          };
        }
      }

      return {
        id: m.id,
        role: m.role as AiChatMessage['role'],
        content: m.content,
        toolCalls: m.toolCalls,
        timestamp: m.timestamp,
        model: m.model ?? undefined,
        context: m.context || undefined,
        turn: m.turn ?? undefined,
        transcriptRef: m.transcriptRef ?? undefined,
        summaryRef: m.summaryRef ?? undefined,
      };
    }),
  });
}

export function rebuildConversationFromTranscript(
  conversation: AiConversation,
  transcriptEntries: TranscriptEntryDto[],
): AiConversation {
  if (transcriptEntries.length === 0) {
    return hydrateStructuredConversation(conversation);
  }

  const sortedEntries = transcriptEntries
    .slice()
    .sort((left, right) => left.timestamp - right.timestamp || left.id.localeCompare(right.id));
  const existingMessages = new Map(conversation.messages.map((message) => [message.id, message]));
  const transcriptUsers = new Map<string, AiChatMessage>();
  const transcriptAssistants = new Map<string, TranscriptAssistantState>();
  const transcriptSummaries = new Map<string, AiChatMessage>();
  const transcriptRoundSummaries = new Map<string, AiPendingSummary>();
  const orderedMessageIds: string[] = [];
  const orderedMessageIdSet = new Set<string>();
  const messageTimestamps = new Map<string, number>();
  let latestSummaryMessageId: string | undefined;
  let latestSummaryTimestamp = -Infinity;

  const rememberMessageOrder = (messageId: string, timestamp: number) => {
    if (!orderedMessageIdSet.has(messageId)) {
      orderedMessageIdSet.add(messageId);
      orderedMessageIds.push(messageId);
    }
    const existingTimestamp = messageTimestamps.get(messageId);
    if (existingTimestamp === undefined || timestamp < existingTimestamp) {
      messageTimestamps.set(messageId, timestamp);
    }
  };

  for (const entry of sortedEntries) {
    const payload = isRecord(entry.payload) ? entry.payload : {};
    switch (entry.kind) {
      case 'user_message': {
        const messageId = getStringField(payload, 'messageId');
        const content = getStringField(payload, 'content');
        if (!messageId || content === undefined) {
          break;
        }
        transcriptUsers.set(messageId, {
          id: messageId,
          role: 'user',
          content,
          timestamp: existingMessages.get(messageId)?.timestamp ?? entry.timestamp,
          context: existingMessages.get(messageId)?.context,
          transcriptRef: {
            conversationId: entry.conversationId,
            startEntryId: entry.id,
            endEntryId: entry.id,
          },
        });
        rememberMessageOrder(messageId, entry.timestamp);
        break;
      }
      case 'assistant_turn_start': {
        const messageId = getStringField(payload, 'messageId') ?? entry.turnId ?? undefined;
        if (!messageId) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        assistantState.startEntryId = entry.id;
        rememberMessageOrder(messageId, entry.timestamp);
        break;
      }
      case 'assistant_part': {
        const messageId = entry.turnId ?? getStringField(payload, 'messageId');
        if (!messageId) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        const parts = Array.isArray(payload.parts) ? payload.parts as AiTurnPart[] : [];
        if (getBooleanField(payload, 'completeTurnParts')) {
          assistantState.parts = parts.slice();
          assistantState.hasCompleteParts = true;
        } else {
          appendTranscriptParts(assistantState, parts);
        }
        break;
      }
      case 'guardrail': {
        const messageId = entry.turnId ?? getStringField(payload, 'messageId');
        const code = getStringField(payload, 'code');
        const message = getStringField(payload, 'message');
        if (!messageId || !code || !message) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        appendTranscriptPart(assistantState, {
          type: 'guardrail',
          code: code as Extract<AiTurnPart, { type: 'guardrail' }>['code'],
          message,
          rawText: getStringField(payload, 'rawText'),
        });
        break;
      }
      case 'assistant_round': {
        const messageId = entry.turnId ?? getStringField(payload, 'messageId');
        const roundId = getStringField(payload, 'roundId');
        if (!messageId || !roundId) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        const round = ensureTranscriptRound(assistantState, roundId, getNumberField(payload, 'round'), entry.timestamp);
        const retryAttempt = getNumberField(payload, 'retryAttempt');
        if (retryAttempt !== undefined) {
          round.retryCount = retryAttempt;
        }
        break;
      }
      case 'tool_call': {
        const messageId = entry.turnId ?? getStringField(payload, 'messageId');
        const toolCallId = getStringField(payload, 'id');
        const toolName = getStringField(payload, 'name');
        const argumentsText = getStringField(payload, 'argumentsText');
        if (!messageId || !toolCallId || !toolName || argumentsText === undefined) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        const roundId = getStringField(payload, 'roundId') ?? `${messageId}-round-transcript`;
        const round = ensureTranscriptRound(assistantState, roundId, undefined, entry.timestamp);
        const existingToolCall = round.toolCalls.find((toolCall) => toolCall.id === toolCallId);
        if (!existingToolCall) {
          round.toolCalls.push({
            id: toolCallId,
            name: toolName,
            argumentsText,
            approvalState: getBooleanField(payload, 'syntheticDenied') ? 'rejected' : undefined,
            executionState: 'pending',
          });
        }
        appendTranscriptPart(assistantState, {
          type: 'tool_call',
          id: toolCallId,
          name: toolName,
          argumentsText,
          status: 'complete',
        });
        break;
      }
      case 'tool_result': {
        const messageId = entry.turnId ?? getStringField(payload, 'messageId');
        const toolCallId = getStringField(payload, 'toolCallId');
        const toolName = getStringField(payload, 'toolName');
        const output = getStringField(payload, 'output') ?? '';
        const success = getBooleanField(payload, 'success');
        if (!messageId || !toolCallId || !toolName || success === undefined) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        const roundId = getStringField(payload, 'roundId') ?? `${messageId}-round-transcript`;
        const round = ensureTranscriptRound(assistantState, roundId, undefined, entry.timestamp);
        const toolCall = round.toolCalls.find((candidate) => candidate.id === toolCallId);
        if (toolCall) {
          toolCall.executionState = success ? 'completed' : 'error';
          if (!success && getBooleanField(payload, 'syntheticDenied')) {
            toolCall.approvalState = 'rejected';
          }
        }
        appendTranscriptPart(assistantState, {
          type: 'tool_result',
          toolCallId,
          toolName,
          success,
          output,
          error: getStringField(payload, 'error'),
          durationMs: getNumberField(payload, 'durationMs'),
          truncated: getBooleanField(payload, 'truncated'),
          envelope: getEnvelopeField(payload),
        });
        break;
      }
      case 'assistant_turn_end': {
        const messageId = getStringField(payload, 'messageId') ?? entry.turnId ?? undefined;
        if (!messageId) {
          break;
        }
        const assistantState = ensureTranscriptAssistantState(transcriptAssistants, messageId, entry.timestamp);
        assistantState.endEntryId = entry.id;
        const status = getStringField(payload, 'status');
        if (status === 'complete' || status === 'error') {
          assistantState.status = status;
        }
        assistantState.plainTextSummary = getStringField(payload, 'plainTextSummary') ?? assistantState.plainTextSummary;
        break;
      }
      case 'summary_created': {
        const roundId = getStringField(payload, 'roundId');
        const summaryText = getStringField(payload, 'summaryText');
        const summaryKind = getSummaryReferenceKind(payload);
        if (roundId && summaryText && summaryKind === 'round') {
          transcriptRoundSummaries.set(roundId, {
            roundId,
            text: summaryText,
            metadata: extractSummaryMetadata(payload),
          });
        }

        const summaryMessage = buildSummaryMessageFromTranscript(
          entry.conversationId,
          entry,
          payload,
          (() => {
            const messageId = getStringField(payload, 'messageId');
            return messageId ? existingMessages.get(messageId) : undefined;
          })(),
        );
        if (!summaryMessage) {
          break;
        }
        transcriptSummaries.set(summaryMessage.id, summaryMessage);
        rememberMessageOrder(summaryMessage.id, summaryMessage.timestamp);
        if (entry.timestamp >= latestSummaryTimestamp) {
          latestSummaryTimestamp = entry.timestamp;
          latestSummaryMessageId = summaryMessage.id;
        }
        break;
      }
      default:
        break;
    }
  }

  const transcriptAssistantMessages = new Map<string, AiChatMessage>();
  for (const [messageId, assistantState] of transcriptAssistants.entries()) {
    transcriptAssistantMessages.set(
      messageId,
      buildAssistantMessageFromTranscript(conversation.id, assistantState, existingMessages.get(messageId)),
    );
  }

  const resolveTranscriptMessage = (messageId: string): AiChatMessage | undefined => {
    return transcriptSummaries.get(messageId)
      ?? transcriptAssistantMessages.get(messageId)
      ?? transcriptUsers.get(messageId)
      ?? existingMessages.get(messageId);
  };

  const mergedMessages = conversation.messages.map((message) => {
    if (message.role === 'assistant' && transcriptAssistantMessages.has(message.id)) {
      return transcriptAssistantMessages.get(message.id) ?? message;
    }
    if (transcriptSummaries.has(message.id)) {
      return transcriptSummaries.get(message.id) ?? message;
    }
    if (message.role === 'user' && transcriptUsers.has(message.id)) {
      return transcriptUsers.get(message.id) ?? message;
    }
    return message;
  });

  const activeSummaryTimestamp = latestSummaryMessageId
    ? latestSummaryTimestamp
    : -Infinity;
  const transcriptProjectionIds = latestSummaryMessageId
    ? orderedMessageIds.filter((messageId) => (
        messageId === latestSummaryMessageId
        || (messageTimestamps.get(messageId) ?? -Infinity) > activeSummaryTimestamp
      ))
    : orderedMessageIds.slice();
  const transcriptProjectionMessages = transcriptProjectionIds
    .map((messageId) => resolveTranscriptMessage(messageId))
    .filter((message): message is AiChatMessage => Boolean(message));

  const rebuiltMessages = (() => {
    if (transcriptProjectionMessages.length > 0) {
      const projectionMessageIdSet = new Set(transcriptProjectionMessages.map((message) => message.id));
      const existingOnlyMessages = mergedMessages.filter((message) => (
        !projectionMessageIdSet.has(message.id)
        && (activeSummaryTimestamp < 0 || message.timestamp > activeSummaryTimestamp)
      ));

      return [...transcriptProjectionMessages, ...existingOnlyMessages];
    }

    return mergedMessages;
  })();

  const mergedTurns = mergeTurnsWithTranscriptSummaries(
    conversation.turns ?? [],
    transcriptRoundSummaries,
  );

  return hydrateStructuredConversation({
    ...conversation,
    messages: rebuiltMessages,
    turns: mergedTurns,
  });
}

function mergeTurnsWithTranscriptSummaries(
  turns: NonNullable<AiConversation['turns']>,
  transcriptRoundSummaries: ReadonlyMap<string, AiPendingSummary>,
): NonNullable<AiConversation['turns']> {
  if (transcriptRoundSummaries.size === 0) {
    return turns;
  }

  return turns.map((turn) => {
    const matchingSummaries = turn.rounds
      .map((round) => transcriptRoundSummaries.get(round.id))
      .filter((summary): summary is AiPendingSummary => Boolean(summary));

    if (matchingSummaries.length === 0) {
      return turn;
    }

    return {
      ...turn,
      pendingSummaries: mergePendingSummaries(turn.pendingSummaries ?? [], matchingSummaries),
    };
  });
}

export function hydrateStructuredConversation(conversation: AiConversation): AiConversation {
  const existingTurns = conversation.turns ?? [];
  const usedTurnIds = new Set<string>();
  const turns: NonNullable<AiConversation['turns']> = [];
  let lastUserMessage: AiChatMessage | undefined;
  const messageBackedRoundSummaries = new Map<string, AiPendingSummary>();

  for (const message of conversation.messages) {
    if (!isRoundSummaryReference(message.summaryRef)) {
      continue;
    }

    messageBackedRoundSummaries.set(message.summaryRef.roundId, {
      roundId: message.summaryRef.roundId,
      text: message.content,
    });
  }

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
            statefulMarker: Object.prototype.hasOwnProperty.call(round, 'statefulMarker')
              ? round.statefulMarker
              : existingRound.statefulMarker,
            summary: round.summary ?? existingRound.summary,
            summaryMetadata: round.summaryMetadata ?? existingRound.summaryMetadata,
            toolCalls: round.toolCalls.map((toolCall, toolIndex) => ({
              ...(existingRound.toolCalls.find((candidate) => candidate.id === toolCall.id) ?? existingRound.toolCalls[toolIndex] ?? {}),
              ...toolCall,
            })),
          };
        })
      : (matchingExistingTurn?.rounds ?? []);
    const messageBackedPendingSummaries = mergedRounds
      .filter((round) => !round.summary)
      .map((round) => messageBackedRoundSummaries.get(round.id))
      .filter((summary): summary is AiPendingSummary => Boolean(summary));
    const normalizedSummaryState = normalizePendingSummaries(
      mergedRounds,
      mergePendingSummaries(
        matchingExistingTurn?.pendingSummaries ?? [],
        messageBackedPendingSummaries,
      ),
    );
    const normalizedTurn: AiAssistantTurn = {
      ...turn,
      toolRounds: normalizedSummaryState.rounds,
    };

    turns.push({
      id: matchingExistingTurn?.id ?? `${lastUserMessage?.id ?? message.id}-${message.id}`,
      requestMessageId: matchingExistingTurn?.requestMessageId ?? lastUserMessage?.id ?? message.id,
      requestText: matchingExistingTurn?.requestText ?? lastUserMessage?.content ?? '',
      startedAt: matchingExistingTurn?.startedAt ?? lastUserMessage?.timestamp ?? message.timestamp,
      status: normalizedTurn.status,
      rounds: normalizedSummaryState.rounds,
      pendingSummaries: normalizedSummaryState.unresolved,
    });

    return projectAssistantMessage({
      ...message,
      turn: normalizedTurn,
      transcriptRef,
    });
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
