import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { api } from '../lib/api';
import { useSettingsStore } from './settingsStore';
import type { AiChatMessage, AiConversation } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_CONVERSATIONS = 50;
const MAX_MESSAGES_PER_CONVERSATION = 100;
const STORAGE_KEY = 'oxide-ai-chat';

// ═══════════════════════════════════════════════════════════════════════════
// Store Interface
// ═══════════════════════════════════════════════════════════════════════════

interface AiChatStore {
  // State
  conversations: AiConversation[];
  activeConversationId: string | null;
  isLoading: boolean;
  error: string | null;
  abortController: AbortController | null;

  // Actions
  createConversation: (title?: string) => string;
  deleteConversation: (id: string) => void;
  setActiveConversation: (id: string | null) => void;
  renameConversation: (id: string, title: string) => void;
  clearAllConversations: () => void;

  // Message actions
  sendMessage: (content: string, context?: string) => Promise<void>;
  stopGeneration: () => void;
  regenerateLastResponse: () => Promise<void>;

  // Internal
  _addMessage: (conversationId: string, message: AiChatMessage) => void;
  _updateMessage: (conversationId: string, messageId: string, content: string) => void;
  _setStreaming: (conversationId: string, messageId: string, streaming: boolean) => void;

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
  // Use first 30 chars of message as title
  const cleaned = firstMessage.replace(/\n/g, ' ').trim();
  return cleaned.length > 30 ? cleaned.slice(0, 30) + '...' : cleaned;
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
// Store Implementation
// ═══════════════════════════════════════════════════════════════════════════

export const useAiChatStore = create<AiChatStore>()(
  persist(
    (set, get) => ({
      // Initial state
      conversations: [],
      activeConversationId: null,
      isLoading: false,
      error: null,
      abortController: null,

      // Create a new conversation
      createConversation: (title) => {
        const id = generateId();
        const now = Date.now();
        const conversation: AiConversation = {
          id,
          title: title || 'New Chat',
          messages: [],
          createdAt: now,
          updatedAt: now,
        };

        set((state) => {
          // Limit total conversations
          let conversations = [conversation, ...state.conversations];
          if (conversations.length > MAX_CONVERSATIONS) {
            conversations = conversations.slice(0, MAX_CONVERSATIONS);
          }
          return {
            conversations,
            activeConversationId: id,
          };
        });

        return id;
      },

      // Delete a conversation
      deleteConversation: (id) => {
        set((state) => {
          const conversations = state.conversations.filter((c) => c.id !== id);
          const activeConversationId =
            state.activeConversationId === id
              ? conversations[0]?.id ?? null
              : state.activeConversationId;
          return { conversations, activeConversationId };
        });
      },

      // Set active conversation
      setActiveConversation: (id) => {
        set({ activeConversationId: id, error: null });
      },

      // Rename a conversation
      renameConversation: (id, title) => {
        set((state) => ({
          conversations: state.conversations.map((c) =>
            c.id === id ? { ...c, title, updatedAt: Date.now() } : c
          ),
        }));
      },

      // Clear all conversations
      clearAllConversations: () => {
        set({
          conversations: [],
          activeConversationId: null,
          error: null,
        });
      },

      // Send a message
      sendMessage: async (content, context) => {
        const { activeConversationId, createConversation, _addMessage, _updateMessage, _setStreaming } = get();

        // Get or create conversation
        let convId = activeConversationId;
        if (!convId) {
          convId = createConversation(generateTitle(content));
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

        // Add user message
        const userMessage: AiChatMessage = {
          id: generateId(),
          role: 'user',
          content,
          timestamp: Date.now(),
          context,
        };
        _addMessage(convId, userMessage);

        // Update title if this is first message
        if (conversation.messages.length === 0) {
          set((state) => ({
            conversations: state.conversations.map((c) =>
              c.id === convId ? { ...c, title: generateTitle(content) } : c
            ),
          }));
        }

        // Create assistant message placeholder
        const assistantMessage: AiChatMessage = {
          id: generateId(),
          role: 'assistant',
          content: '',
          timestamp: Date.now(),
          isStreaming: true,
        };
        _addMessage(convId, assistantMessage);

        // Prepare messages for API
        const apiMessages: ChatCompletionMessage[] = [];

        // System prompt
        apiMessages.push({
          role: 'system',
          content: `You are a helpful terminal assistant. You help users with shell commands, scripts, and terminal operations. Be concise and direct. When providing commands, format them clearly. You can use markdown for formatting.`,
        });

        // Add context if provided
        if (context) {
          apiMessages.push({
            role: 'system',
            content: `Current terminal context:\n\`\`\`\n${context}\n\`\`\``,
          });
        }

        // Add conversation history (limited)
        const historyMessages = get().conversations.find((c) => c.id === convId)?.messages || [];
        const recentHistory = historyMessages.slice(-10); // Last 10 messages
        for (const msg of recentHistory) {
          // Skip empty messages and the current streaming placeholder
          if ((msg.role === 'user' || msg.role === 'assistant') && msg.content.trim() !== '') {
            apiMessages.push({ role: msg.role, content: msg.content });
          }
        }

        // Create abort controller
        const abortController = new AbortController();
        set({ isLoading: true, error: null, abortController });

        try {
          let fullContent = '';

          for await (const chunk of streamChatCompletion(
            aiSettings.baseUrl,
            aiSettings.model,
            apiKey,
            apiMessages,
            abortController.signal
          )) {
            fullContent += chunk;
            _updateMessage(convId, assistantMessage.id, fullContent);
          }

          _setStreaming(convId, assistantMessage.id, false);
        } catch (e) {
          if (e instanceof Error && e.name === 'AbortError') {
            // User cancelled - keep the partial message if any content
            const currentMsg = get().conversations
              .find((c) => c.id === convId)
              ?.messages.find((m) => m.id === assistantMessage.id);
            if (!currentMsg?.content) {
              // Remove empty message
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
            // Remove the empty assistant message on error
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

        // Find last user message
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

        // Remove messages after last user message
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

        // Resend
        await sendMessage(lastUserMessage.content, lastUserMessage.context);
      },

      // Internal: Add message to conversation
      _addMessage: (conversationId, message) => {
        set((state) => ({
          conversations: state.conversations.map((c) => {
            if (c.id !== conversationId) return c;
            let messages = [...c.messages, message];
            // Limit messages
            if (messages.length > MAX_MESSAGES_PER_CONVERSATION) {
              messages = messages.slice(-MAX_MESSAGES_PER_CONVERSATION);
            }
            return { ...c, messages, updatedAt: Date.now() };
          }),
        }));
      },

      // Internal: Update message content
      _updateMessage: (conversationId, messageId, content) => {
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

      // Internal: Set streaming state
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
    }),
    {
      name: STORAGE_KEY,
      partialize: (state) => ({
        conversations: state.conversations,
        activeConversationId: state.activeConversationId,
      }),
    }
  )
);
