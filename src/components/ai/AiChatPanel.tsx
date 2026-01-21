import { useEffect, useRef, useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, MessageSquare, MoreVertical, Settings, ChevronDown, Terminal, HelpCircle, FileCode, Zap } from 'lucide-react';
import { useAiChatStore } from '../../store/aiChatStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useAppStore } from '../../store/appStore';
import { ChatMessage } from './ChatMessage';
import { ChatInput } from './ChatInput';
import type { AiConversation } from '../../types';

export function AiChatPanel() {
  const { t } = useTranslation();
  const {
    conversations,
    activeConversationId,
    isLoading,
    error,
    createConversation,
    deleteConversation,
    setActiveConversation,
    sendMessage,
    stopGeneration,
    clearAllConversations,
    getActiveConversation,
  } = useAiChatStore();

  const aiEnabled = useSettingsStore((state) => state.settings.ai.enabled);
  const createTab = useAppStore((state) => state.createTab);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [showConversations, setShowConversations] = useState(false);
  const [showMenu, setShowMenu] = useState(false);

  const activeConversation = getActiveConversation();

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [activeConversation?.messages]);

  const handleNewChat = useCallback(() => {
    createConversation();
    setShowConversations(false);
  }, [createConversation]);

  const handleSend = useCallback(
    (content: string, context?: string) => {
      sendMessage(content, context);
    },
    [sendMessage]
  );

  const handleSelectConversation = useCallback(
    (id: string) => {
      setActiveConversation(id);
      setShowConversations(false);
    },
    [setActiveConversation]
  );

  const handleDelete = useCallback(
    (e: React.MouseEvent, id: string) => {
      e.stopPropagation();
      deleteConversation(id);
    },
    [deleteConversation]
  );

  const handleClearAll = useCallback(() => {
    if (window.confirm(t('ai.chat.clear_all_confirm'))) {
      clearAllConversations();
    }
    setShowMenu(false);
  }, [clearAllConversations, t]);

  const handleOpenSettings = useCallback(() => {
    createTab('settings');
    setShowMenu(false);
  }, [createTab]);

  // Not enabled state
  if (!aiEnabled) {
    return (
      <div className="h-full flex flex-col items-center justify-center p-6 text-center">
        <MessageSquare className="w-12 h-12 text-zinc-600 mb-4" />
        <h3 className="text-lg font-medium text-zinc-300 mb-2">{t('ai.chat.title')}</h3>
        <p className="text-sm text-zinc-500 mb-4">
          {t('ai.chat.disabled_message')}
        </p>
        <button
          onClick={() => createTab('settings')}
          className="flex items-center gap-2 px-4 py-2 bg-orange-600 hover:bg-orange-500 rounded-lg text-white text-sm transition-colors"
        >
          <Settings className="w-4 h-4" />
          {t('ai.chat.open_settings')}
        </button>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-zinc-900/50">
      {/* Header */}
      <div className="flex-shrink-0 flex items-center justify-between px-3 py-2 border-b border-zinc-700/50 bg-zinc-800/50">
        {/* Conversation selector */}
        <button
          onClick={() => setShowConversations(!showConversations)}
          className="flex items-center gap-2 px-2 py-1 rounded hover:bg-zinc-700/50 text-sm text-zinc-200 transition-colors"
        >
          <MessageSquare className="w-4 h-4 text-orange-500" />
          <span className="max-w-[150px] truncate">
            {activeConversation?.title || t('ai.chat.new_chat')}
          </span>
          <ChevronDown className={`w-4 h-4 text-zinc-500 transition-transform ${showConversations ? 'rotate-180' : ''}`} />
        </button>

        <div className="flex items-center gap-1">
          <button
            onClick={handleNewChat}
            className="p-1.5 rounded hover:bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 transition-colors"
            title={t('ai.chat.new_chat_tooltip')}
          >
            <Plus className="w-4 h-4" />
          </button>
          <div className="relative">
            <button
              onClick={() => setShowMenu(!showMenu)}
              className="p-1.5 rounded hover:bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 transition-colors"
              title={t('ai.chat.more_options')}
            >
              <MoreVertical className="w-4 h-4" />
            </button>
            {showMenu && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setShowMenu(false)} />
                <div className="absolute right-0 top-full mt-1 w-40 py-1 bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl z-20">
                  <button
                    onClick={handleOpenSettings}
                    className="w-full flex items-center gap-2 px-3 py-2 text-sm text-zinc-300 hover:bg-zinc-700/50 transition-colors"
                  >
                    <Settings className="w-4 h-4" />
                    {t('ai.chat.settings')}
                  </button>
                  <button
                    onClick={handleClearAll}
                    className="w-full flex items-center gap-2 px-3 py-2 text-sm text-red-400 hover:bg-zinc-700/50 transition-colors"
                  >
                    <Trash2 className="w-4 h-4" />
                    {t('ai.chat.clear_all')}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      </div>

      {/* Conversation list dropdown */}
      {showConversations && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setShowConversations(false)} />
          <div className="absolute left-2 right-2 top-12 max-h-64 overflow-y-auto bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl z-20">
            {conversations.length === 0 ? (
              <div className="p-4 text-center text-sm text-zinc-500">
                {t('ai.chat.no_conversations')}
              </div>
            ) : (
              conversations.map((conv) => (
                <ConversationItem
                  key={conv.id}
                  conversation={conv}
                  isActive={conv.id === activeConversationId}
                  onSelect={() => handleSelectConversation(conv.id)}
                  onDelete={(e) => handleDelete(e, conv.id)}
                />
              ))
            )}
          </div>
        </>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto">
        {!activeConversation || activeConversation.messages.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center p-6 text-center">
            <MessageSquare className="w-10 h-10 text-zinc-600 mb-3" />
            <h3 className="text-sm font-medium text-zinc-300 mb-1">{t('ai.chat.start_conversation')}</h3>
            <p className="text-xs text-zinc-500 max-w-[200px] mb-4">
              {t('ai.chat.start_conversation_hint')}
            </p>
            
            {/* Quick prompt buttons */}
            <div className="w-full max-w-[280px] space-y-2">
              <QuickPromptButton
                icon={<HelpCircle className="w-4 h-4" />}
                label={t('ai.quick_prompts.explain_command')}
                prompt={t('ai.quick_prompts.explain_command_prompt')}
                onSend={handleSend}
              />
              <QuickPromptButton
                icon={<Terminal className="w-4 h-4" />}
                label={t('ai.quick_prompts.find_files')}
                prompt={t('ai.quick_prompts.find_files_prompt')}
                onSend={handleSend}
              />
              <QuickPromptButton
                icon={<FileCode className="w-4 h-4" />}
                label={t('ai.quick_prompts.write_script')}
                prompt={t('ai.quick_prompts.write_script_prompt')}
                onSend={handleSend}
              />
              <QuickPromptButton
                icon={<Zap className="w-4 h-4" />}
                label={t('ai.quick_prompts.optimize_command')}
                prompt={t('ai.quick_prompts.optimize_command_prompt')}
                onSend={handleSend}
              />
            </div>
          </div>
        ) : (
          <>
            {activeConversation.messages.map((msg) => (
              <ChatMessage key={msg.id} message={msg} />
            ))}
            <div ref={messagesEndRef} />
          </>
        )}
      </div>

      {/* Error display */}
      {error && (
        <div className="flex-shrink-0 px-3 py-2 bg-red-900/30 border-t border-red-800/50">
          <p className="text-xs text-red-400">{error}</p>
        </div>
      )}

      {/* Input */}
      <ChatInput
        onSend={handleSend}
        onStop={stopGeneration}
        isLoading={isLoading}
        disabled={!aiEnabled}
      />
    </div>
  );
}

// Conversation list item
function ConversationItem({
  conversation,
  isActive,
  onSelect,
  onDelete,
}: {
  conversation: AiConversation;
  isActive: boolean;
  onSelect: () => void;
  onDelete: (e: React.MouseEvent) => void;
}) {
  const { t } = useTranslation();
  const timeStr = new Date(conversation.updatedAt).toLocaleDateString();

  return (
    <button
      onClick={onSelect}
      className={`w-full flex items-center justify-between px-3 py-2 text-left hover:bg-zinc-700/50 transition-colors ${
        isActive ? 'bg-zinc-700/30' : ''
      }`}
    >
      <div className="flex-1 min-w-0">
        <div className="text-sm text-zinc-200 truncate">{conversation.title}</div>
        <div className="text-xs text-zinc-500">
          {t('ai.chat.messages_count', { count: conversation.messages.length })} · {timeStr}
        </div>
      </div>
      <button
        onClick={onDelete}
        className="flex-shrink-0 p-1 rounded hover:bg-red-600/20 text-zinc-500 hover:text-red-400 transition-colors"
        title={t('ai.chat.delete_conversation')}
      >
        <Trash2 className="w-3.5 h-3.5" />
      </button>
    </button>
  );
}

// Quick prompt button for empty state
function QuickPromptButton({
  icon,
  label,
  prompt,
  onSend,
}: {
  icon: React.ReactNode;
  label: string;
  prompt: string;
  onSend: (content: string, context?: string) => void;
}) {
  const handleClick = () => {
    // If prompt ends with space or colon, it's a partial prompt - just send it to start the conversation
    // If it's a complete question, send it directly
    if (prompt.endsWith(' ') || prompt.endsWith(': ')) {
      // For partial prompts, we'd ideally focus the input and fill it
      // But for simplicity, we'll send it as is and let user continue in the chat
      onSend(prompt.trim());
    } else {
      onSend(prompt);
    }
  };

  return (
    <button
      onClick={handleClick}
      className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg bg-zinc-800/50 border border-zinc-700/50 hover:border-orange-600/30 hover:bg-zinc-800 text-left transition-colors group"
    >
      <div className="flex-shrink-0 text-zinc-500 group-hover:text-orange-500 transition-colors">
        {icon}
      </div>
      <span className="text-sm text-zinc-300 group-hover:text-zinc-100 transition-colors">
        {label}
      </span>
    </button>
  );
}
