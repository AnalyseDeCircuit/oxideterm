import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../lib/api';
import { useSettingsStore } from './settingsStore';
import { gatherSidebarContext, type SidebarContext } from '../lib/sidebarContextProvider';
import { getProvider } from '../lib/ai/providerRegistry';
import { estimateTokens, trimHistoryToTokenBudget, getModelContextWindow } from '../lib/ai/tokenUtils';
import type { ChatMessage as ProviderChatMessage } from '../lib/ai/providers';
import type { AiChatMessage, AiConversation } from '../types';
import i18n from '../i18n';

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_MESSAGES_PER_CONVERSATION = 200;

// ═══════════════════════════════════════════════════════════════════════════
// Backend Types (matching Rust structs)
// ═══════════════════════════════════════════════════════════════════════════

interface ContextSnapshotDto {
  sessionId: string | null;
  connectionName: string | null;
  remoteOs: string | null;
  cwd: string | null;
  selection: string | null;
  bufferTail: string | null;
}

interface ConversationMetaDto {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

// Backend returns flat structure, not nested meta
interface FullConversationDto {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  sessionId: string | null;
  messages: Array<{
    id: string;
    role: 'user' | 'assistant';
    content: string;
    timestamp: number;
    context: string | null; // Backend returns just the buffer_tail as 'context'
  }>;
}

// Wrapper for list conversations response
interface ConversationListResponseDto {
  conversations: ConversationMetaDto[];
}

// ═══════════════════════════════════════════════════════════════════════════
// Store Interface
// ═══════════════════════════════════════════════════════════════════════════

interface AiChatStore {
  // State
  conversations: AiConversation[];
  activeConversationId: string | null;
  isLoading: boolean;
  isInitialized: boolean;
  error: string | null;
  abortController: AbortController | null;

  // Initialization
  init: () => Promise<void>;

  // Actions
  createConversation: (title?: string) => Promise<string>;
  deleteConversation: (id: string) => Promise<void>;
  setActiveConversation: (id: string | null) => void;
  renameConversation: (id: string, title: string) => Promise<void>;
  clearAllConversations: () => Promise<void>;

  // Message actions
  sendMessage: (content: string, context?: string, options?: { skipUserMessage?: boolean }) => Promise<void>;
  stopGeneration: () => void;
  regenerateLastResponse: () => Promise<void>;
  summarizeConversation: () => Promise<void>;

  // Internal (persist to backend)
  _addMessage: (conversationId: string, message: AiChatMessage, sidebarContext?: SidebarContext | null) => Promise<void>;
  _updateMessage: (conversationId: string, messageId: string, content: string) => Promise<void>;
  _setStreaming: (conversationId: string, messageId: string, streaming: boolean) => void;
  _loadConversation: (id: string) => Promise<void>;

  // Getters
  getActiveConversation: () => AiConversation | null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function generateTitle(firstMessage: string): string {
  const cleaned = firstMessage.replace(/\n/g, ' ').trim();
  return cleaned.length > 30 ? cleaned.slice(0, 30) + '...' : cleaned;
}

// Convert backend DTO to frontend model
function dtoToConversation(dto: FullConversationDto): AiConversation {
  return {
    id: dto.id,
    title: dto.title,
    createdAt: dto.createdAt,
    updatedAt: dto.updatedAt,
    messages: dto.messages.map((m) => {
      // Re-parse thinking content from persisted full content
      if (m.role === 'assistant' && m.content.includes('<thinking>')) {
        const parsed = parseThinkingContent(m.content);
        return {
          id: m.id,
          role: m.role as 'assistant',
          content: parsed.content,
          thinkingContent: parsed.thinkingContent,
          timestamp: m.timestamp,
          context: m.context || undefined,
        };
      }
      return {
        id: m.id,
        role: m.role,
        content: m.content,
        timestamp: m.timestamp,
        context: m.context || undefined,
      };
    }),
  };
}

function metaToConversation(meta: ConversationMetaDto): AiConversation {
  return {
    id: meta.id,
    title: meta.title,
    createdAt: meta.createdAt,
    updatedAt: meta.updatedAt,
    messages: [], // Will be loaded on demand
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Thinking Content Parser
// ═══════════════════════════════════════════════════════════════════════════

interface ParsedResponse {
  /** Main response content (without thinking tags) */
  content: string;
  /** Extracted thinking content (if present) */
  thinkingContent?: string;
}

/**
 * Parse AI response to extract thinking content
 * Supports: <thinking>...</thinking> tags (common in Claude-style responses)
 */
function parseThinkingContent(rawContent: string): ParsedResponse {
  // Match <thinking>...</thinking> block (case-insensitive, multiline)
  const thinkingRegex = /<thinking>([\s\S]*?)<\/thinking>/gi;
  let thinkingContent = '';
  let content = rawContent;
  
  // Extract all thinking blocks
  let match;
  while ((match = thinkingRegex.exec(rawContent)) !== null) {
    if (thinkingContent) thinkingContent += '\n\n';
    thinkingContent += match[1].trim();
  }
  
  // Remove thinking tags from main content
  if (thinkingContent) {
    content = rawContent.replace(thinkingRegex, '').trim();
  }
  
  return {
    content,
    thinkingContent: thinkingContent || undefined,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider-based Streaming API
// ═══════════════════════════════════════════════════════════════════════════

// Re-export ChatMessage type from providers for internal use
type ChatCompletionMessage = ProviderChatMessage;

// ═══════════════════════════════════════════════════════════════════════════
// Store Implementation (redb Backend)
// ═══════════════════════════════════════════════════════════════════════════

export const useAiChatStore = create<AiChatStore>()((set, get) => ({
  // Initial state
  conversations: [],
  activeConversationId: null,
  isLoading: false,
  isInitialized: false,
  error: null,
  abortController: null,

  // Initialize store from backend
  init: async () => {
    if (get().isInitialized) return;

    try {
      // Load conversation list (metadata only)
      const response = await invoke<ConversationListResponseDto>('ai_chat_list_conversations');
      const conversations = response.conversations.map(metaToConversation);

      set({
        conversations,
        activeConversationId: conversations[0]?.id ?? null,
        isInitialized: true,
      });

      // Load first conversation's messages if exists
      if (conversations[0]) {
        await get()._loadConversation(conversations[0].id);
      }

      console.log(`[AiChatStore] Initialized with ${conversations.length} conversations`);
    } catch (e) {
      console.warn('[AiChatStore] Backend not available, using memory-only mode:', e);
      set({ isInitialized: true });
    }
  },

  // Load full conversation with messages
  _loadConversation: async (id) => {
    try {
      const fullConv = await invoke<FullConversationDto>('ai_chat_get_conversation', { conversationId: id });
      const conversation = dtoToConversation(fullConv);

      set((state) => ({
        conversations: state.conversations.map((c) =>
          c.id === id ? conversation : c
        ),
      }));
    } catch (e) {
      console.warn(`[AiChatStore] Failed to load conversation ${id}:`, e);
    }
  },

  // Create a new conversation
  createConversation: async (title) => {
    const id = generateId();
    const now = Date.now();
    const conversation: AiConversation = {
      id,
      title: title || 'New Chat',
      messages: [],
      createdAt: now,
      updatedAt: now,
    };

    // Update local state immediately
    set((state) => ({
      conversations: [conversation, ...state.conversations],
      activeConversationId: id,
    }));

    // Persist to backend
    try {
      await invoke('ai_chat_create_conversation', {
        request: {
          id,
          title: conversation.title,
          createdAt: now,
        },
      });
    } catch (e) {
      console.warn('[AiChatStore] Failed to persist conversation:', e);
    }

    return id;
  },

  // Delete a conversation
  deleteConversation: async (id) => {
    set((state) => {
      const conversations = state.conversations.filter((c) => c.id !== id);
      const activeConversationId =
        state.activeConversationId === id
          ? conversations[0]?.id ?? null
          : state.activeConversationId;
      return { conversations, activeConversationId };
    });

    try {
      await invoke('ai_chat_delete_conversation', { conversationId: id });
    } catch (e) {
      console.warn(`[AiChatStore] Failed to delete conversation ${id}:`, e);
    }
  },

  // Set active conversation (and load messages if needed)
  setActiveConversation: (id) => {
    const prevId = get().activeConversationId;
    set({ activeConversationId: id, error: null });

    // Unload messages from the previous conversation to free memory
    if (prevId && prevId !== id) {
      set((state) => ({
        conversations: state.conversations.map((c) =>
          c.id === prevId ? { ...c, messages: [] } : c
        ),
      }));
    }

    if (id) {
      const conv = get().conversations.find((c) => c.id === id);
      if (conv && conv.messages.length === 0) {
        // Load messages on demand (await to prevent flash of empty content)
        get()._loadConversation(id).catch((e) =>
          console.warn(`[AiChatStore] Failed to load conversation ${id}:`, e)
        );
      }
    }
  },

  // Rename a conversation
  renameConversation: async (id, title) => {
    set((state) => ({
      conversations: state.conversations.map((c) =>
        c.id === id ? { ...c, title, updatedAt: Date.now() } : c
      ),
    }));

    try {
      await invoke('ai_chat_update_conversation', {
        conversationId: id,
        title,
      });
    } catch (e) {
      console.warn(`[AiChatStore] Failed to rename conversation ${id}:`, e);
    }
  },

  // Clear all conversations
  clearAllConversations: async () => {
    set({
      conversations: [],
      activeConversationId: null,
      error: null,
    });

    try {
      await invoke('ai_chat_clear_all');
    } catch (e) {
      console.warn('[AiChatStore] Failed to clear all conversations:', e);
    }
  },

  // Send a message
  sendMessage: async (content, context, options) => {
    const skipUserMessage = options?.skipUserMessage ?? false;
    const { activeConversationId, createConversation, _addMessage, _setStreaming } = get();

    // Get or create conversation
    let convId = activeConversationId;
    if (!convId) {
      convId = await createConversation(generateTitle(content));
    }

    const conversation = get().conversations.find((c) => c.id === convId);
    if (!conversation) return;

    // Get AI settings
    const aiSettings = useSettingsStore.getState().settings.ai;
    if (!aiSettings.enabled) {
      set({ error: 'AI is not enabled. Please enable it in Settings.' });
      return;
    }

    // ════════════════════════════════════════════════════════════════════
    // Resolve Active Provider and API Key
    // ════════════════════════════════════════════════════════════════════

    const activeProvider = aiSettings.providers?.find(p => p.id === aiSettings.activeProviderId);
    const providerType = activeProvider?.type || 'openai';
    const providerBaseUrl = activeProvider?.baseUrl || aiSettings.baseUrl;
    const providerModel = aiSettings.activeModel || activeProvider?.defaultModel || aiSettings.model;
    const providerId = activeProvider?.id;

    if (!providerModel) {
      set({ error: 'No model selected. Please refresh models or select one in Settings > AI.' });
      return;
    }

    // Get API key - provider-specific only
    let apiKey: string | null = null;
    try {
      if (providerId) {
        apiKey = await api.getAiProviderApiKey(providerId);
      }
      // Ollama doesn't require an API key
      if (!apiKey && providerType !== 'ollama') {
        set({ error: i18n.t('ai.model_selector.api_key_not_found') });
        return;
      }
    } catch (e) {
      if (providerType !== 'ollama') {
        set({ error: i18n.t('ai.model_selector.failed_to_get_api_key') });
        return;
      }
    }

    // ════════════════════════════════════════════════════════════════════
    // Automatic Context Injection (Sidebar Deep Awareness)
    // ════════════════════════════════════════════════════════════════════

    let sidebarContext: SidebarContext | null = null;
    try {
      sidebarContext = gatherSidebarContext({
        maxBufferLines: aiSettings.contextVisibleLines || 50,
        maxBufferChars: aiSettings.contextMaxChars || 8000,
        maxSelectionChars: 2000,
      });
    } catch (e) {
      console.warn('[AiChatStore] Failed to gather sidebar context:', e);
    }

    const effectiveContext = context || sidebarContext?.contextBlock || '';

    // Add user message (skipped during regeneration — user message is already in store)
    const userMessage: AiChatMessage = {
      id: generateId(),
      role: 'user',
      content,
      timestamp: Date.now(),
      context: effectiveContext || undefined,
    };
    if (!skipUserMessage) {
      await _addMessage(convId, userMessage, sidebarContext);
    }

    // Update title if this is first message
    if (!skipUserMessage && conversation.messages.length === 0) {
      const title = generateTitle(content);
      set((state) => ({
        conversations: state.conversations.map((c) =>
          c.id === convId ? { ...c, title } : c
        ),
      }));
      try {
        await invoke('ai_chat_update_conversation', { conversationId: convId, title });
      } catch (e) {
        console.warn('[AiChatStore] Failed to update conversation title:', e);
      }
    }

    // Create assistant message placeholder (local only — persisted after streaming completes)
    const assistantMessage: AiChatMessage = {
      id: generateId(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      isStreaming: true,
    };
    // Add to frontend state only — do NOT persist empty placeholder to backend.
    // Backend persistence happens after streaming completes (success or abort-with-content).
    set((state) => ({
      conversations: state.conversations.map((c) => {
        if (c.id !== convId) return c;
        return { ...c, messages: [...c.messages, assistantMessage], updatedAt: Date.now() };
      }),
    }));

    // Prepare messages for API
    const apiMessages: ChatCompletionMessage[] = [];

    // ════════════════════════════════════════════════════════════════════
    // Enhanced System Prompt with Environment Awareness
    // ════════════════════════════════════════════════════════════════════

    let systemPrompt = `You are a helpful terminal assistant. You help users with shell commands, scripts, and terminal operations. Be concise and direct. When providing commands, format them clearly. You can use markdown for formatting.`;

    if (sidebarContext?.systemPromptSegment) {
      systemPrompt += `\n\n${sidebarContext.systemPromptSegment}`;
    }

    apiMessages.push({
      role: 'system',
      content: systemPrompt,
    });

    if (effectiveContext) {
      apiMessages.push({
        role: 'system',
        content: `Current terminal context:\n\`\`\`\n${effectiveContext}\n\`\`\``,
      });
    }

    // ════════════════════════════════════════════════════════════════════
    // Token-Aware History Trimming
    // ════════════════════════════════════════════════════════════════════

    const systemTokens = estimateTokens(systemPrompt) + estimateTokens(effectiveContext);
    const contextWindow = getModelContextWindow(
      providerModel,
      aiSettings.modelContextWindows,
      providerId,
    );

    const historyMessages = get().conversations.find((c) => c.id === convId)?.messages || [];
    const trimmed = trimHistoryToTokenBudget(historyMessages, contextWindow, systemTokens, 0);
    for (const msg of trimmed) {
      if ((msg.role === 'user' || msg.role === 'assistant') && msg.content.trim() !== '') {
        apiMessages.push({ role: msg.role, content: msg.content });
      }
    }

    // Create abort controller
    const abortController = new AbortController();
    set({ isLoading: true, error: null, abortController });

    try {
      let fullContent = '';
      let lastUpdateTime = 0;
      const UPDATE_INTERVAL = 50; // ms - throttle updates for smoother streaming

      const updateContent = (content: string, force = false, isThinkingStreaming = false) => {
        const now = Date.now();
        if (!force && now - lastUpdateTime < UPDATE_INTERVAL) return;
        lastUpdateTime = now;
        
        set((state) => ({
          conversations: state.conversations.map((c) => {
            if (c.id !== convId) return c;
            return {
              ...c,
              messages: c.messages.map((m) =>
                m.id === assistantMessage.id 
                  ? { ...m, content, isThinkingStreaming } 
                  : m
              ),
              updatedAt: now,
            };
          }),
        }));
      };

      // ════════════════════════════════════════════════════════════════════
      // Stream via Provider Abstraction Layer
      // ════════════════════════════════════════════════════════════════════

      const provider = getProvider(providerType);
      let thinkingContent = '';

      for await (const event of provider.streamCompletion(
        { baseUrl: providerBaseUrl, model: providerModel, apiKey: apiKey || '' },
        apiMessages,
        abortController.signal
      )) {
        switch (event.type) {
          case 'content':
            fullContent += event.content;
            updateContent(fullContent, false, false);
            break;
          case 'thinking':
            thinkingContent += event.content;
            // Show thinking as temporary content with thinking tag
            updateContent(fullContent || '...', false, true);
            break;
          case 'error':
            throw new Error(event.message);
          case 'done':
            break;
        }
      }

      // For providers that handle thinking natively (Anthropic), use extracted thinking
      // For others (OpenAI-compatible), parse <thinking> tags from content
      let mainContent = fullContent;
      let parsedThinking = thinkingContent || undefined;

      if (!thinkingContent && fullContent.includes('<thinking>')) {
        const parsed = parseThinkingContent(fullContent);
        mainContent = parsed.content;
        parsedThinking = parsed.thinkingContent;
      }

      // Final update with parsed content
      set((state) => ({
        conversations: state.conversations.map((c) => {
          if (c.id !== convId) return c;
          return {
            ...c,
            messages: c.messages.map((m) =>
              m.id === assistantMessage.id
                ? {
                    ...m,
                    content: mainContent,
                    thinkingContent: parsedThinking,
                    isThinkingStreaming: false,
                    isStreaming: false,
                  }
                : m
            ),
            updatedAt: Date.now(),
          };
        }),
      }));

      // Persist final content to backend (first persist — placeholder was local-only)
      try {
        await invoke('ai_chat_save_message', {
          request: {
            id: assistantMessage.id,
            conversationId: convId,
            role: 'assistant',
            content: fullContent, // Store full content including thinking tags
            timestamp: assistantMessage.timestamp,
            contextSnapshot: null,
          },
        });
      } catch (e) {
        console.warn('[AiChatStore] Failed to persist final message content:', e);
      }
    } catch (e) {
      if (e instanceof Error && e.name === 'AbortError') {
        const currentMsg = get().conversations
          .find((c) => c.id === convId)
          ?.messages.find((m) => m.id === assistantMessage.id);
        if (!currentMsg?.content) {
          // No content generated — remove placeholder from frontend (never persisted to backend)
          set((state) => ({
            conversations: state.conversations.map((c) =>
              c.id === convId
                ? { ...c, messages: c.messages.filter((m) => m.id !== assistantMessage.id) }
                : c
            ),
          }));
        } else {
          // Partial content — keep it and persist to backend
          _setStreaming(convId, assistantMessage.id, false);
          try {
            await invoke('ai_chat_save_message', {
              request: {
                id: assistantMessage.id,
                conversationId: convId,
                role: 'assistant',
                content: currentMsg.content,
                timestamp: assistantMessage.timestamp,
                contextSnapshot: null,
              },
            });
          } catch (persistErr) {
            console.warn('[AiChatStore] Failed to persist aborted message:', persistErr);
          }
        }
      } else {
        const errorMessage = e instanceof Error ? e.message : String(e);
        set({ error: errorMessage });
        // Remove failed placeholder from frontend (never persisted to backend)
        set((state) => ({
          conversations: state.conversations.map((c) =>
            c.id === convId
              ? { ...c, messages: c.messages.filter((m) => m.id !== assistantMessage.id) }
              : c
          ),
        }));
      }
    } finally {
      set({ isLoading: false, abortController: null });
    }
  },

  // Stop generation
  stopGeneration: () => {
    const { abortController } = get();
    if (abortController) {
      abortController.abort();
      set({ abortController: null, isLoading: false });
    }
  },

  // Regenerate last response
  regenerateLastResponse: async () => {
    const { activeConversationId, conversations, sendMessage } = get();
    if (!activeConversationId) return;

    const conversation = conversations.find((c) => c.id === activeConversationId);
    if (!conversation || conversation.messages.length < 2) return;

    const messages = [...conversation.messages];
    let lastUserMessageIndex = -1;
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role === 'user') {
        lastUserMessageIndex = i;
        break;
      }
    }

    if (lastUserMessageIndex === -1) return;

    const lastUserMessage = messages[lastUserMessageIndex];

    // Keep messages up to AND including the last user message (remove only assistant responses)
    set((state) => ({
      conversations: state.conversations.map((c) =>
        c.id === activeConversationId
          ? {
              ...c,
              messages: c.messages.slice(0, lastUserMessageIndex + 1),
              updatedAt: Date.now(),
            }
          : c
      ),
    }));

    // Delete assistant messages after the user message from backend
    // Backend keeps the user message (at idx) and deletes everything after idx
    try {
      await invoke('ai_chat_delete_messages_after', {
        conversationId: activeConversationId,
        afterMessageId: lastUserMessage.id,
      });
    } catch (e) {
      console.warn('[AiChatStore] Failed to delete messages from backend:', e);
    }

    // Resend — skipUserMessage since user message is already persisted in both frontend and backend
    await sendMessage(lastUserMessage.content, lastUserMessage.context, { skipUserMessage: true });
  },

  // Summarize conversation — compress history into a single summary message
  summarizeConversation: async () => {
    const { activeConversationId, conversations } = get();
    if (!activeConversationId) return;

    const conversation = conversations.find((c) => c.id === activeConversationId);
    if (!conversation || conversation.messages.length < 4) return;

    // Get AI settings for provider
    const aiSettings = useSettingsStore.getState().settings.ai;
    if (!aiSettings.enabled) return;

    const activeProvider = aiSettings.providers?.find(p => p.id === aiSettings.activeProviderId);
    const providerType = activeProvider?.type || 'openai';
    const providerBaseUrl = activeProvider?.baseUrl || aiSettings.baseUrl;
    const providerModel = aiSettings.activeModel || activeProvider?.defaultModel || aiSettings.model;
    const providerId = activeProvider?.id;

    if (!providerModel) return;

    // Get API key
    let apiKey: string | null = null;
    try {
      if (providerId) {
        apiKey = await api.getAiProviderApiKey(providerId);
      }
      if (!apiKey && providerType !== 'ollama') return;
    } catch {
      if (providerType !== 'ollama') return;
    }

    // Build summary request
    const historyText = conversation.messages
      .filter(m => m.role === 'user' || m.role === 'assistant')
      .map(m => `${m.role === 'user' ? 'User' : 'Assistant'}: ${m.content}`)
      .join('\n\n');

    const summaryPrompt: ChatCompletionMessage[] = [
      {
        role: 'system',
        content: 'Summarize the following conversation in a concise paragraph. Capture the key topics, questions asked, solutions provided, and any important context. Write in the same language as the conversation. Keep it under 200 words.',
      },
      {
        role: 'user',
        content: historyText,
      },
    ];

    set({ isLoading: true, error: null });

    try {
      const provider = getProvider(providerType);
      let summaryContent = '';
      const abortController = new AbortController();
      set({ abortController });

      for await (const event of provider.streamCompletion(
        { baseUrl: providerBaseUrl, model: providerModel, apiKey: apiKey || '' },
        summaryPrompt,
        abortController.signal,
      )) {
        if (event.type === 'content') {
          summaryContent += event.content;
        } else if (event.type === 'error') {
          throw new Error(event.message);
        }
      }

      if (!summaryContent.trim()) return;

      // Replace all messages with a single summary message pair
      const originalCount = conversation.messages.length;
      const summaryMessage: AiChatMessage = {
        id: generateId(),
        role: 'assistant',
        content: `📋 **${i18n.t('ai.context.summary_prefix', { count: originalCount })}**\n\n${summaryContent}`,
        timestamp: Date.now(),
      };

      // Atomically replace all messages in a single backend transaction.
      // If the command fails, local state is untouched and the error bubbles
      // to the outer catch which sets the user-visible error state.
      await invoke('ai_chat_replace_conversation_messages', {
        request: {
          conversationId: activeConversationId,
          title: conversation.title,
          message: {
            id: summaryMessage.id,
            conversationId: activeConversationId,
            role: summaryMessage.role,
            content: summaryMessage.content,
            timestamp: summaryMessage.timestamp,
            contextSnapshot: null,
          },
        },
      });

      // Persistence succeeded — now update local state
      set((state) => ({
        conversations: state.conversations.map((c) => {
          if (c.id !== activeConversationId) return c;
          return { ...c, messages: [summaryMessage], updatedAt: Date.now() };
        }),
      }));
    } catch (e) {
      if (!(e instanceof Error && e.name === 'AbortError')) {
        const errorMessage = e instanceof Error ? e.message : String(e);
        set({ error: errorMessage });
      }
    } finally {
      set({ isLoading: false, abortController: null });
    }
  },

  // Internal: Add message to conversation and persist
  _addMessage: async (conversationId, message, sidebarContext) => {
    // Update local state immediately
    set((state) => ({
      conversations: state.conversations.map((c) => {
        if (c.id !== conversationId) return c;
        let messages = [...c.messages, message];
        if (messages.length > MAX_MESSAGES_PER_CONVERSATION) {
          messages = messages.slice(-MAX_MESSAGES_PER_CONVERSATION);
        }
        return { ...c, messages, updatedAt: Date.now() };
      }),
    }));

    // Persist to backend
    try {
      const contextSnapshot: ContextSnapshotDto | null = sidebarContext
        ? {
            sessionId: sidebarContext.env.sessionId,
            connectionName: sidebarContext.env.connection?.formatted || null,
            remoteOs: sidebarContext.env.remoteOSHint,
            cwd: null, // Not captured in current context
            selection: sidebarContext.terminal.selection,
            bufferTail: sidebarContext.terminal.buffer,
          }
        : null;

      await invoke('ai_chat_save_message', {
        request: {
          id: message.id,
          conversationId,
          role: message.role,
          content: message.content,
          timestamp: message.timestamp,
          contextSnapshot,
        },
      });
    } catch (e) {
      console.warn('[AiChatStore] Failed to persist message:', e);
    }
  },

  // Internal: Update message content (for streaming - batch persist)
  _updateMessage: async (conversationId, messageId, content) => {
    // Just update local state - backend persisted after streaming completes
    set((state) => ({
      conversations: state.conversations.map((c) => {
        if (c.id !== conversationId) return c;
        return {
          ...c,
          messages: c.messages.map((m) =>
            m.id === messageId ? { ...m, content } : m
          ),
          updatedAt: Date.now(),
        };
      }),
    }));
  },

  // Internal: Set streaming state (local only)
  _setStreaming: (conversationId, messageId, streaming) => {
    set((state) => ({
      conversations: state.conversations.map((c) => {
        if (c.id !== conversationId) return c;
        return {
          ...c,
          messages: c.messages.map((m) =>
            m.id === messageId ? { ...m, isStreaming: streaming } : m
          ),
        };
      }),
    }));
  },

  // Getter: Get active conversation
  getActiveConversation: () => {
    const { activeConversationId, conversations } = get();
    if (!activeConversationId) return null;
    return conversations.find((c) => c.id === activeConversationId) ?? null;
  },
}));
