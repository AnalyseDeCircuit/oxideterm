import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../lib/api';
import { useSettingsStore } from './settingsStore';
import { gatherSidebarContext, type SidebarContext } from '../lib/sidebarContextProvider';
import type { AiChatMessage, AiConversation } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_MESSAGES_PER_CONVERSATION = 100;

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

interface PersistedMessageDto {
  id: string;
  conversationId: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
  contextSnapshot: ContextSnapshotDto | null;
}

interface FullConversationDto {
  meta: ConversationMetaDto;
  messages: PersistedMessageDto[];
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
  sendMessage: (content: string, context?: string) => Promise<void>;
  stopGeneration: () => void;
  regenerateLastResponse: () => Promise<void>;

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
    id: dto.meta.id,
    title: dto.meta.title,
    createdAt: dto.meta.createdAt,
    updatedAt: dto.meta.updatedAt,
    messages: dto.messages.map((m) => ({
      id: m.id,
      role: m.role,
      content: m.content,
      timestamp: m.timestamp,
      context: m.contextSnapshot?.bufferTail || undefined,
    })),
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
// OpenAI-compatible Streaming API
// ═══════════════════════════════════════════════════════════════════════════

interface ChatCompletionMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

async function* streamChatCompletion(
  baseUrl: string,
  model: string,
  apiKey: string,
  messages: ChatCompletionMessage[],
  signal: AbortSignal
): AsyncGenerator<string, void, unknown> {
  const cleanBaseUrl = baseUrl.replace(/\/+$/, '');
  const url = `${cleanBaseUrl}/chat/completions`;

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model,
      messages,
      stream: true,
    }),
    signal,
  });

  if (!response.ok) {
    const errorText = await response.text();
    let errorMessage = `API error: ${response.status}`;
    try {
      const errorJson = JSON.parse(errorText);
      errorMessage = errorJson.error?.message || errorJson.message || errorMessage;
    } catch {
      if (errorText) errorMessage = errorText.slice(0, 200);
    }
    throw new Error(errorMessage);
  }

  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body');

  const decoder = new TextDecoder();

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      const chunk = decoder.decode(value, { stream: true });
      const lines = chunk.split('\n');

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          const data = line.slice(6);
          if (data === '[DONE]') return;

          try {
            const json = JSON.parse(data);
            const content = json.choices?.[0]?.delta?.content || '';
            if (content) yield content;
          } catch {
            // Ignore parse errors for partial chunks
          }
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

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
      const metas = await invoke<ConversationMetaDto[]>('ai_chat_list_conversations');
      const conversations = metas.map(metaToConversation);

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
      const fullConv = await invoke<FullConversationDto>('ai_chat_get_conversation', { id });
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
      await invoke('ai_chat_delete_conversation', { id });
    } catch (e) {
      console.warn(`[AiChatStore] Failed to delete conversation ${id}:`, e);
    }
  },

  // Set active conversation (and load messages if needed)
  setActiveConversation: (id) => {
    set({ activeConversationId: id, error: null });

    if (id) {
      const conv = get().conversations.find((c) => c.id === id);
      if (conv && conv.messages.length === 0) {
        // Load messages on demand
        get()._loadConversation(id);
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
        id,
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
  sendMessage: async (content, context) => {
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

    // Get API key
    let apiKey: string | null;
    try {
      apiKey = await api.getAiApiKey();
      if (!apiKey) {
        set({ error: 'API key not found. Please configure it in Settings > AI.' });
        return;
      }
    } catch (e) {
      set({ error: 'Failed to get API key.' });
      return;
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

    // Add user message
    const userMessage: AiChatMessage = {
      id: generateId(),
      role: 'user',
      content,
      timestamp: Date.now(),
      context: effectiveContext || undefined,
    };
    await _addMessage(convId, userMessage, sidebarContext);

    // Update title if this is first message
    if (conversation.messages.length === 0) {
      const title = generateTitle(content);
      set((state) => ({
        conversations: state.conversations.map((c) =>
          c.id === convId ? { ...c, title } : c
        ),
      }));
      try {
        await invoke('ai_chat_update_conversation', { id: convId, title });
      } catch (e) {
        console.warn('[AiChatStore] Failed to update conversation title:', e);
      }
    }

    // Create assistant message placeholder
    const assistantMessage: AiChatMessage = {
      id: generateId(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      isStreaming: true,
    };
    await _addMessage(convId, assistantMessage, null);

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

    // Add conversation history (limited)
    const historyMessages = get().conversations.find((c) => c.id === convId)?.messages || [];
    const recentHistory = historyMessages.slice(-10);
    for (const msg of recentHistory) {
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

      const updateContent = (content: string, force = false) => {
        const now = Date.now();
        if (!force && now - lastUpdateTime < UPDATE_INTERVAL) return;
        lastUpdateTime = now;
        
        set((state) => ({
          conversations: state.conversations.map((c) => {
            if (c.id !== convId) return c;
            return {
              ...c,
              messages: c.messages.map((m) =>
                m.id === assistantMessage.id ? { ...m, content } : m
              ),
              updatedAt: now,
            };
          }),
        }));
      };

      for await (const chunk of streamChatCompletion(
        aiSettings.baseUrl,
        aiSettings.model,
        apiKey,
        apiMessages,
        abortController.signal
      )) {
        fullContent += chunk;
        // Throttled update for smoother streaming
        updateContent(fullContent);
      }

      // Final update to ensure complete content is shown
      updateContent(fullContent, true);

      _setStreaming(convId, assistantMessage.id, false);

      // Persist final content to backend
      try {
        await invoke('ai_chat_update_message', {
          messageId: assistantMessage.id,
          content: fullContent,
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
          set((state) => ({
            conversations: state.conversations.map((c) =>
              c.id === convId
                ? { ...c, messages: c.messages.filter((m) => m.id !== assistantMessage.id) }
                : c
            ),
          }));
        } else {
          _setStreaming(convId, assistantMessage.id, false);
        }
      } else {
        const errorMessage = e instanceof Error ? e.message : String(e);
        set({ error: errorMessage });
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

    // Remove messages after last user message (local)
    set((state) => ({
      conversations: state.conversations.map((c) =>
        c.id === activeConversationId
          ? {
              ...c,
              messages: c.messages.slice(0, lastUserMessageIndex),
              updatedAt: Date.now(),
            }
          : c
      ),
    }));

    // Delete from backend
    try {
      await invoke('ai_chat_delete_messages_after', {
        conversationId: activeConversationId,
        afterMessageId: lastUserMessage.id,
      });
    } catch (e) {
      console.warn('[AiChatStore] Failed to delete messages from backend:', e);
    }

    // Resend
    await sendMessage(lastUserMessage.content, lastUserMessage.context);
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
