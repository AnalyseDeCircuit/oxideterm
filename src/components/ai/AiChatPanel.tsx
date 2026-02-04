import { useEffect, useRef, useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, MessageSquare, MoreVertical, Settings, Terminal, HelpCircle, FileCode, Zap } from 'lucide-react';
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
    isInitialized,
    error,
    init,
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
  const [inputValue, setInputValue] = useState('');

  const activeConversation = getActiveConversation();

  // Initialize store on mount
  useEffect(() => {
    if (!isInitialized) {
      init();
    }
  }, [init, isInitialized]);

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
      <div className="h-full flex flex-col items-center justify-center p-6 text-center bg-theme-bg">
        <div className="w-16 h-16 rounded-2xl bg-theme-accent/5 flex items-center justify-center mb-6">
          <MessageSquare className="w-8 h-8 text-theme-text-muted opacity-40" />
        </div>
        <h3 className="text-lg font-bold text-theme-text mb-2">{t('ai.chat.title')}</h3>
        <p className="text-sm text-theme-text-muted mb-6 max-w-[240px] leading-relaxed">
          {t('ai.chat.disabled_message')}
        </p>
        <button
          onClick={() => createTab('settings')}
          className="flex items-center gap-2 px-6 py-2.5 bg-theme-accent hover:opacity-90 rounded-xl text-theme-bg text-sm font-bold shadow-sm transition-all active:scale-95"
        >
          <Settings className="w-4 h-4" />
          {t('ai.chat.open_settings')}
        </button>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-theme-bg">
      {/* Header - Strictly Utilitarian */}
      <div className="flex-shrink-0 flex items-center justify-between px-3 py-1.5 border-b border-theme-border/30 bg-theme-bg">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-bold tracking-[0.15em] text-theme-text-muted uppercase">{t('ai.chat.header')}</span>
          {activeConversation?.title && (
            <div className="flex items-center gap-2 max-w-[120px]">
              <span className="text-theme-border/40 font-thin">|</span>
              <button
                onClick={() => setShowConversations(!showConversations)}
                className="text-[11px] text-theme-text-muted hover:text-theme-text truncate font-medium transition-colors"
              >
                {activeConversation.title}
              </button>
            </div>
          )}
        </div>

        <div className="flex items-center gap-0.5">
          <button
            onClick={handleNewChat}
            className="p-1 px-1.5 rounded-sm hover:bg-theme-accent/10 text-theme-text-muted hover:text-theme-accent transition-colors"
            title={t('ai.chat.new_chat_tooltip')}
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
          <div className="relative">
            <button
              onClick={() => setShowMenu(!showMenu)}
              className="p-1 px-1.5 rounded-sm hover:bg-theme-accent/10 text-theme-text-muted hover:text-theme-text transition-colors"
              title={t('ai.chat.more_options')}
            >
              <MoreVertical className="w-3.5 h-3.5" />
            </button>
            {showMenu && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setShowMenu(false)} />
                <div className="absolute right-0 top-full mt-1 w-40 py-1 bg-theme-bg-panel border border-theme-border shadow-xl z-20">
                  <button
                    onClick={handleOpenSettings}
                    className="w-full flex items-center gap-2 px-3 py-2 text-sm text-theme-text-muted hover:text-theme-text hover:bg-theme-accent/10 transition-colors"
                  >
                    <Settings className="w-4 h-4" />
                    {t('ai.chat.settings')}
                  </button>
                  <button
                    onClick={handleClearAll}
                    className="w-full flex items-center gap-2 px-3 py-2 text-sm text-red-500 hover:bg-red-500/10 transition-colors"
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
          <div className="absolute left-2 right-2 top-12 max-h-64 overflow-y-auto bg-theme-bg-panel border border-theme-border rounded-lg shadow-xl z-20">
            {conversations.length === 0 ? (
              <div className="p-4 text-center text-sm text-theme-text-muted">
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

      {/* Messages Area */}
      <div className="flex-1 overflow-y-auto selection:bg-theme-accent/20">
        {!activeConversation || activeConversation.messages.length === 0 ? (
          <div className="h-full flex flex-col p-6 pt-12">
            <h3 className="text-[13px] font-bold text-theme-text mb-6 tracking-tight">
              {t('ai.chat.get_started')}
            </h3>

            {/* Utilitarian prompt list */}
            <div className="flex flex-col gap-1">
              <QuickPromptButton
                icon={<HelpCircle className="w-3.5 h-3.5" />}
                label={t('ai.quick_prompts.explain_command')}
                prompt={t('ai.quick_prompts.explain_command_prompt')}
                onFillInput={setInputValue}
              />
              <QuickPromptButton
                icon={<Terminal className="w-3.5 h-3.5" />}
                label={t('ai.quick_prompts.find_files')}
                prompt={t('ai.quick_prompts.find_files_prompt')}
                onFillInput={setInputValue}
              />
              <QuickPromptButton
                icon={<FileCode className="w-3.5 h-3.5" />}
                label={t('ai.quick_prompts.write_script')}
                prompt={t('ai.quick_prompts.write_script_prompt')}
                onFillInput={setInputValue}
              />
              <QuickPromptButton
                icon={<Zap className="w-3.5 h-3.5" />}
                label={t('ai.quick_prompts.optimize_command')}
                prompt={t('ai.quick_prompts.optimize_command_prompt')}
                onFillInput={setInputValue}
              />
            </div>
          </div>
        ) : (
          <div className="flex flex-col">
            {activeConversation.messages.map((msg) => (
              <ChatMessage key={msg.id} message={msg} />
            ))}
            <div ref={messagesEndRef} className="h-4" />
          </div>
        )}
      </div>

      {/* Error display */}
      {error && (
        <div className="flex-shrink-0 px-3 py-2 bg-red-500/10 border-t border-theme-border">
          <p className="text-xs text-red-400 font-mono">{error}</p>
        </div>
      )}

      {/* Input */}
      <ChatInput
        onSend={handleSend}
        onStop={stopGeneration}
        isLoading={isLoading}
        disabled={!aiEnabled}
        externalValue={inputValue}
        onExternalValueChange={setInputValue}
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
  const timeStr = new Date(conversation.updatedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  return (
    <button
      onClick={onSelect}
      className={`w-full flex items-center justify-between px-3 py-2 text-left transition-colors group/item border-l-2 ${isActive
        ? 'bg-theme-accent/5 border-theme-accent'
        : 'hover:bg-theme-bg-panel/40 border-transparent'
        }`}
    >
      <div className="flex-1 min-w-0 pr-2">
        <div className={`text-[12px] truncate font-bold tracking-tight ${isActive ? 'text-theme-text' : 'text-theme-text-muted group-hover/item:text-theme-text'}`}>
          {conversation.title}
        </div>
        <div className="text-[9px] text-theme-text-muted/40 uppercase tracking-tight mt-0.5 font-mono">
          {t('ai.chat.messages_count', { count: conversation.messages.length })} · {timeStr}
        </div>
      </div>
      <button
        onClick={onDelete}
        className="flex-shrink-0 p-1 opacity-0 group-hover/item:opacity-40 hover:opacity-100 text-theme-text-muted hover:text-red-500 transition-all"
        title={t('ai.chat.delete_conversation')}
      >
        <Trash2 className="w-3 h-3" />
      </button>
    </button>
  );
}

// Quick prompt button for empty state - fills input instead of sending directly
function QuickPromptButton({
  icon,
  label,
  prompt,
  onFillInput,
}: {
  icon: React.ReactNode;
  label: string;
  prompt: string;
  onFillInput: (value: string) => void;
}) {
  const handleClick = () => {
    // Fill the input with the prompt template, user can edit before sending
    onFillInput(prompt);
  };

  return (
    <button
      onClick={handleClick}
      className="w-full flex items-center gap-3 px-3 py-2 rounded-sm border border-transparent hover:border-theme-border/30 hover:bg-theme-bg-panel/20 text-left transition-colors group/btn active:opacity-70"
    >
      <div className="flex-shrink-0 text-theme-text-muted group-hover/btn:text-theme-accent transition-colors">
        {icon}
      </div>
      <span className="text-[13px] text-theme-text-muted group-hover/btn:text-theme-text transition-colors font-medium">
        {label}
      </span>
    </button>
  );
}
