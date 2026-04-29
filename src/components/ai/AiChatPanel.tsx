// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useRef, useCallback, useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, MessageSquare, MoreVertical, Settings, Terminal, HelpCircle, FileCode, Zap, AlertTriangle, Shrink, Scissors, Loader2 } from 'lucide-react';
import { useAiChatStore } from '../../store/aiChatStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useAppStore } from '../../store/appStore';
import { useConfirm } from '../../hooks/useConfirm';
import { ChatMessage } from './ChatMessage';
import { ChatInput } from './ChatInput';
import { ModelSelector } from './ModelSelector';
import { ToolIndicator } from './ToolIndicator';
import { AiSafetyModeIndicator } from './AiSafetyModeIndicator';
import { ProfileIndicator } from './ProfileIndicator';
import { estimateTokens, getModelContextWindow } from '../../lib/ai/tokenUtils';
import { CONTEXT_WARNING_THRESHOLD, CONTEXT_DANGER_THRESHOLD } from '../../lib/ai/constants';
import type { SidebarContext } from '../../lib/sidebarContextProvider';
import type { AiConversation } from '../../types';

export function AiChatPanel() {
  const { t } = useTranslation();
  const {
    conversations,
    activeConversationId,
    isLoading,
    isInitialized,
    isInitializing,
    initializationError,
    error,
    init,
    retryInit,
    createConversation,
    deleteConversation,
    setActiveConversation,
    sendMessage,
    stopGeneration,
    clearAllConversations,
    getActiveConversation,
    regenerateLastResponse,
    summarizeConversation,
    compactConversation,
    editAndResend,
    switchBranch,
    deleteMessage,
    renameConversation,
    trimInfo,
    compactionInfo,
  } = useAiChatStore();

  const aiEnabled = useSettingsStore((state) => state.settings.ai.enabled);
  const createTab = useAppStore((state) => state.createTab);
  const { confirm, ConfirmDialog } = useConfirm();

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [showConversations, setShowConversations] = useState(false);
  const [showMenu, setShowMenu] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const [isRegenerating, setIsRegenerating] = useState(false);
  const [showTrimNotice, setShowTrimNotice] = useState(false);
  const [showCompactionNotice, setShowCompactionNotice] = useState(false);

  // Auto-show trim notification when trimInfo changes, auto-dismiss after 5s
  useEffect(() => {
    if (trimInfo && trimInfo.count > 0) {
      setShowTrimNotice(true);
      const timer = setTimeout(() => setShowTrimNotice(false), 5000);
      return () => clearTimeout(timer);
    }
  }, [trimInfo?.timestamp]);

  const activeConversation = getActiveConversation();
  const hasInitializationError = Boolean(isInitialized && initializationError && conversations.length === 0 && !activeConversation);
  const isInitializationPending = Boolean((!isInitialized || isInitializing) && conversations.length === 0 && !activeConversation);
  const isInitializationBlocked = hasInitializationError || isInitializationPending;
  const initializationErrorMessage = initializationError ? t(initializationError.messageKey) : null;
  const activeSilentCompaction = useMemo(() => {
    if (
      !compactionInfo
      || compactionInfo.mode !== 'silent'
      || compactionInfo.conversationId !== activeConversationId
    ) {
      return null;
    }
    return compactionInfo;
  }, [activeConversationId, compactionInfo]);

  useEffect(() => {
    if (!activeSilentCompaction) {
      setShowCompactionNotice(false);
      return;
    }

    setShowCompactionNotice(true);

    if (activeSilentCompaction.phase === 'running') {
      return;
    }

    const noticeTimestamp = activeSilentCompaction.timestamp;
    const timer = setTimeout(() => {
      setShowCompactionNotice(false);
      const currentInfo = useAiChatStore.getState().compactionInfo;
      if (
        currentInfo
        && currentInfo.mode === 'silent'
        && currentInfo.phase === 'done'
        && currentInfo.timestamp === noticeTimestamp
      ) {
        useAiChatStore.setState({ compactionInfo: null });
      }
    }, 5000);
    return () => clearTimeout(timer);
  }, [activeSilentCompaction]);

  // ─── Context Usage Computation ──────────────────────────────────────────
  const aiSettings = useSettingsStore((s) => s.settings.ai);
  const activeModel = aiSettings.activeModel
    || aiSettings.providers?.find(p => p.id === aiSettings.activeProviderId)?.defaultModel
    || aiSettings.model
    || '';

  const contextUsage = useMemo(() => {
    if (!activeConversation) return { percentage: 0, isWarning: false, isDanger: false };
    let totalTokens = 0;
    for (const msg of activeConversation.messages) {
      if (msg.role === 'user' || msg.role === 'assistant') {
        totalTokens += estimateTokens(msg.content);
      }
    }
    const maxTokens = getModelContextWindow(
      activeModel,
      aiSettings.modelContextWindows,
      aiSettings.activeProviderId ?? undefined,
      aiSettings.userContextWindows,
    );
    const percentage = Math.min((totalTokens / maxTokens) * 100, 100);
    return {
      percentage,
      isWarning: percentage > CONTEXT_WARNING_THRESHOLD * 100,
      isDanger: percentage > CONTEXT_DANGER_THRESHOLD * 100,
      totalTokens,
      maxTokens,
    };
  }, [activeConversation?.messages, activeModel, aiSettings.modelContextWindows, aiSettings.activeProviderId, aiSettings.userContextWindows]);

  // ─── Model Switch Detection ─────────────────────────────────────────────
  const [modelSwitchWarning, setModelSwitchWarning] = useState<{
    percentage: number;
  } | null>(null);
  const prevModelRef = useRef(activeModel);

  useEffect(() => {
    const prevModel = prevModelRef.current;
    prevModelRef.current = activeModel;

    // Skip on initial render or same model
    if (!prevModel || prevModel === activeModel || !activeConversation) return;

    // Check if the current conversation exceeds the warning threshold in the new model
    if (contextUsage.totalTokens && contextUsage.maxTokens) {
      const pct = (contextUsage.totalTokens / contextUsage.maxTokens) * 100;
      if (pct > CONTEXT_WARNING_THRESHOLD * 100) {
        setModelSwitchWarning({ percentage: Math.round(pct) });
      }
    }
  }, [activeModel]); // intentionally narrow deps — only fire on model change

  // Find the last assistant message index for regenerate button
  const lastAssistantIndex = activeConversation?.messages
    .map((msg, i) => msg.role === 'assistant' ? i : -1)
    .filter(i => i !== -1)
    .pop() ?? -1;

  // Initialize store on mount
  useEffect(() => {
    if (!isInitialized && !error) {
      init();
    }
  }, [error, init, isInitialized]);

  useEffect(() => {
    let cancelled = false;

    const bootstrapMcp = async () => {
      try {
        const { useMcpRegistry } = await import('../../lib/ai/mcp');
        if (!cancelled) {
          await useMcpRegistry.getState().connectAll();
        }
      } catch (e) {
        console.warn('[AiChatPanel] Failed to initialize MCP registry on demand:', e);
      }
    };

    void bootstrapMcp();

    return () => {
      cancelled = true;
    };
  }, []);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [activeConversation?.messages]);

  const handleNewChat = useCallback(() => {
    if (isInitializationBlocked) return;
    createConversation();
    setShowConversations(false);
  }, [createConversation, isInitializationBlocked]);

  const handleRetryInit = useCallback(() => {
    retryInit();
  }, [retryInit]);

  const handleSend = useCallback(
    (content: string, context?: string, sidebarContext?: SidebarContext | null) => {
      sendMessage(content, context, { sidebarContext });
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

  const handleClearAll = useCallback(async () => {
    if (await confirm({ title: t('ai.chat.clear_all_confirm') })) {
      clearAllConversations();
    }
    setShowMenu(false);
  }, [clearAllConversations, t, confirm]);

  const openSettingsTab = useCallback((tab?: string, section?: string) => {
    createTab('settings');
    setShowMenu(false);
    if (tab) {
      window.setTimeout(() => {
        window.dispatchEvent(new CustomEvent('oxideterm:open-settings-tab', { detail: { tab, section } }));
      }, 0);
    }
  }, [createTab]);

  const handleOpenSettings = useCallback(() => {
    openSettingsTab();
  }, [openSettingsTab]);

  const handleOpenToolSettings = useCallback(() => {
    openSettingsTab('ai', 'tool-use');
  }, [openSettingsTab]);

  // Handle regenerate last response
  const handleRegenerate = useCallback(async () => {
    // Read isLoading directly from store to avoid stale closure
    // (ChatMessage memo doesn't compare onRegenerate, so the closure
    //  captured at isStreaming=false time may still have isLoading=true)
    const currentLoading = useAiChatStore.getState().isLoading;
    if (isRegenerating || currentLoading) return;
    setIsRegenerating(true);
    try {
      await regenerateLastResponse();
    } finally {
      setIsRegenerating(false);
    }
  }, [regenerateLastResponse, isRegenerating]);

  // Handle edit and resend
  const handleEdit = useCallback(async (messageId: string, newContent: string) => {
    if (isLoading) return;
    await editAndResend(messageId, newContent);
  }, [editAndResend, isLoading]);

  // Handle delete message
  const handleDeleteMessage = useCallback(async (messageId: string) => {
    if (isLoading) return;
    if (await confirm({ title: t('ai.message.delete_confirm') })) {
      await deleteMessage(messageId);
    }
  }, [deleteMessage, isLoading, confirm, t]);

  // Handle branch switch — reset scroll position after switching
  const handleSwitchBranch = useCallback(async (messageId: string, branchIndex: number) => {
    if (isLoading) return;
    await switchBranch(messageId, branchIndex);
    // Reset scroll to bottom after branch content changes
    setTimeout(() => messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' }), 50);
  }, [switchBranch, isLoading]);

  const handleSummarize = useCallback(async () => {
    if (isLoading) return;
    if (await confirm({ title: t('ai.context.summarize_confirm') })) {
      await summarizeConversation();
    }
  }, [summarizeConversation, isLoading, confirm, t]);

  const handleCompact = useCallback(async () => {
    if (isLoading) return;
    await compactConversation();
  }, [compactConversation, isLoading]);

  // Not enabled state
  if (!aiEnabled) {
    return (
      <div className="h-full flex flex-col items-center justify-center p-6 text-center bg-theme-bg">
        <div className="w-12 h-12 bg-theme-accent/5 flex items-center justify-center mb-4">
          <MessageSquare className="w-6 h-6 text-theme-text-muted opacity-40" />
        </div>
        <h3 className="text-[13px] font-bold text-theme-text mb-1">{t('ai.chat.title')}</h3>
        <p className="text-[12px] text-theme-text-muted mb-4 max-w-[220px] leading-relaxed">
          {t('ai.chat.disabled_message')}
        </p>
        <button
          onClick={() => createTab('settings')}
          className="flex items-center gap-2 px-4 py-1.5 rounded-md bg-theme-accent hover:opacity-90 text-theme-bg text-[12px] font-bold"
        >
          <Settings className="w-3.5 h-3.5" />
          {t('ai.chat.open_settings')}
        </button>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-theme-bg relative">
      {/* Header — Flat Utility Bar */}
      <div className="flex-shrink-0 flex items-center justify-between px-3 py-1.5 border-b border-theme-border/30 bg-theme-bg gap-2 min-h-[36px]">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-[10px] font-bold tracking-[0.12em] text-theme-text-muted uppercase shrink-0">{t('ai.chat.header')}</span>
          {activeConversation?.title && (
            <>
              <span className="text-theme-border/40 shrink-0">·</span>
              <button
                onClick={() => setShowConversations(!showConversations)}
                className="text-[11px] text-theme-text-muted/60 hover:text-theme-text truncate min-w-0"
                title={activeConversation.title}
              >
                {activeConversation.title}
              </button>
            </>
          )}
        </div>

        <div className="flex items-center gap-0.5 shrink-0">
          <button
            onClick={handleNewChat}
            disabled={isInitializationBlocked}
            className="p-1 rounded-md hover:bg-theme-border/10 text-theme-text-muted hover:text-theme-text disabled:opacity-40 disabled:cursor-not-allowed"
            title={t('ai.chat.new_chat_tooltip')}
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
          <div className="relative">
            <button
              onClick={() => setShowMenu(!showMenu)}
              className="p-1 rounded-md hover:bg-theme-border/10 text-theme-text-muted hover:text-theme-text"
              title={t('ai.chat.more_options')}
            >
              <MoreVertical className="w-3.5 h-3.5" />
            </button>
            {showMenu && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setShowMenu(false)} />
                <div className="absolute right-0 top-full mt-0.5 w-40 py-0.5 bg-theme-bg-elevated border border-theme-border rounded-md shadow-lg z-20">
                  <button
                    onClick={handleOpenSettings}
                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-[12px] text-theme-text-muted hover:text-theme-text hover:bg-theme-border/10"
                  >
                    <Settings className="w-3.5 h-3.5" />
                    {t('ai.chat.settings')}
                  </button>
                  <button
                    onClick={handleClearAll}
                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-[12px] text-red-500 hover:bg-red-500/10"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
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
          <div className="absolute left-2 right-2 top-[36px] max-h-64 overflow-y-auto bg-theme-bg-elevated border border-theme-border rounded-md shadow-lg z-20">
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
                  onRename={(id, title) => renameConversation(id, title)}
                />
              ))
            )}
          </div>
        </>
      )}

      {showCompactionNotice && activeSilentCompaction && (
        <div
          className={`flex-shrink-0 flex items-center gap-2 px-3 py-2 border-b ${
            activeSilentCompaction.phase === 'running'
              ? 'bg-theme-accent/10 border-theme-accent/20'
              : 'bg-theme-border/10 border-theme-border/20'
          }`}
        >
          <Shrink
            className={`w-3.5 h-3.5 shrink-0 text-theme-text-muted ${
              activeSilentCompaction.phase === 'running' ? 'animate-pulse' : ''
            }`}
          />
          <span className="text-[11px] text-theme-text-muted flex-1">
            {activeSilentCompaction.phase === 'running'
              ? t('ai.context.compaction_running')
              : t('ai.context.compaction_done', { count: activeSilentCompaction.compactedCount ?? 0 })}
          </span>
        </div>
      )}

      {/* Messages Area */}
      <div className="flex-1 overflow-y-auto selection:bg-theme-accent/20">
        {isInitializationPending ? (
          <div className="h-full flex flex-col items-center justify-center p-6 text-center bg-theme-bg gap-3">
            <div className="w-12 h-12 bg-theme-accent/10 rounded-md flex items-center justify-center">
              <Loader2 className="w-5 h-5 text-theme-accent animate-spin" />
            </div>
            <div className="space-y-1 max-w-[260px]">
              <h3 className="text-[13px] font-bold text-theme-text">
                {t('layout.loading')}
              </h3>
            </div>
          </div>
        ) : hasInitializationError ? (
          <div className="h-full flex flex-col items-center justify-center p-6 text-center bg-theme-bg gap-3">
            <div className="w-12 h-12 bg-red-500/10 rounded-md flex items-center justify-center">
              <AlertTriangle className="w-5 h-5 text-red-400" />
            </div>
            <div className="space-y-1 max-w-[260px]">
              <h3 className="text-[13px] font-bold text-theme-text">
                {t('ai.chat.load_failed_title')}
              </h3>
              <p className="text-[12px] leading-relaxed text-theme-text-muted">
                {initializationErrorMessage}
              </p>
            </div>
            {initializationError?.canRetry && (
              <button
                onClick={handleRetryInit}
                className="px-4 py-1.5 rounded-md bg-theme-accent hover:opacity-90 text-theme-bg text-[12px] font-bold"
              >
                {t('common.retry')}
              </button>
            )}
          </div>
        ) : !activeConversation || activeConversation.messages.length === 0 ? (
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
            {/* Trim notification — shown when messages were silently dropped from API context */}
            {showTrimNotice && trimInfo && (
              <div className="flex items-center gap-2 px-3 py-1.5 bg-amber-500/10 border-b border-amber-500/20 animate-in fade-in duration-200">
                <Scissors className="w-3 h-3 text-amber-500 shrink-0" />
                <span className="text-[10px] text-amber-400 flex-1">
                  {t('ai.context.messages_trimmed', { count: trimInfo.count })}
                </span>
              </div>
            )}
            {activeConversation.messages.map((msg, index) => (
              <ChatMessage
                key={msg.id}
                message={msg}
                isLastAssistant={index === lastAssistantIndex}
                onRegenerate={handleRegenerate}
                isRegenerating={isRegenerating}
                onEdit={handleEdit}
                onDelete={handleDeleteMessage}
                onSwitchBranch={handleSwitchBranch}
                onSuggestionClick={(text) => handleSend(text)}
              />
            ))}
            <div ref={messagesEndRef} className="h-4" />
          </div>
        )}
      </div>

      {/* Error display */}
      {error && !hasInitializationError && (
        <div className="flex-shrink-0 px-3 py-2 bg-red-500/10 border-t border-theme-border">
          <p className="text-xs text-red-400">{error}</p>
        </div>
      )}

      {/* Model switch warning banner — shown when switching to a smaller model overflows context */}
      {modelSwitchWarning && (
        <div className="flex-shrink-0 px-3 py-2 bg-amber-500/10 border-t border-amber-500/20 flex items-center gap-2">
          <AlertTriangle className="w-3.5 h-3.5 text-amber-500 shrink-0" />
          <span className="text-[11px] text-amber-400 flex-1">
            {t('ai.context.model_switched_warning', { percentage: modelSwitchWarning.percentage })}
          </span>
          <div className="flex items-center gap-1.5 shrink-0">
            <button
              onClick={() => { handleCompact(); setModelSwitchWarning(null); }}
              disabled={isLoading}
              className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium text-amber-400 hover:text-amber-300 hover:bg-amber-500/10 rounded-md disabled:opacity-50"
            >
              <Shrink className="w-3 h-3" />
              {t('ai.context.compact_button')}
            </button>
            <button
              onClick={() => { handleNewChat(); setModelSwitchWarning(null); }}
              className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium text-amber-400 hover:text-amber-300 hover:bg-amber-500/10 rounded-md"
            >
              <Plus className="w-3 h-3" />
              {t('ai.chat.new_chat_tooltip')}
            </button>
            <button
              onClick={() => setModelSwitchWarning(null)}
              className="px-1.5 py-0.5 text-[10px] text-amber-400/60 hover:text-amber-300 rounded-md"
            >
              ✕
            </button>
          </div>
        </div>
      )}

      {/* Context limit warning banner */}
      {contextUsage.isDanger && activeConversation && activeConversation.messages.length >= 4 && (
        <div className="flex-shrink-0 px-3 py-2 bg-amber-500/10 border-t border-amber-500/20 flex items-center gap-2">
          <AlertTriangle className="w-3.5 h-3.5 text-amber-500 shrink-0" />
          <span className="text-[11px] text-amber-400 flex-1">
            {t('ai.context.approaching_limit')}
          </span>
          <div className="flex items-center gap-1.5 shrink-0">
            <button
              onClick={handleCompact}
              disabled={isLoading}
              className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium text-amber-400 hover:text-amber-300 hover:bg-amber-500/10 rounded-md disabled:opacity-50"
            >
              <Shrink className="w-3 h-3" />
              {t('ai.context.compact_button')}
            </button>
            <button
              onClick={handleSummarize}
              disabled={isLoading}
              className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium text-amber-400 hover:text-amber-300 hover:bg-amber-500/10 rounded-md disabled:opacity-50"
            >
              <Shrink className="w-3 h-3" />
              {t('ai.context.summarize')}
            </button>
            <button
              onClick={handleNewChat}
              className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium text-amber-400 hover:text-amber-300 hover:bg-amber-500/10 rounded-md"
            >
              <Plus className="w-3 h-3" />
              {t('ai.chat.new_chat_tooltip')}
            </button>
          </div>
        </div>
      )}

      {/* Model Selector + Tool Indicator - bottom position like VS Code */}
      <div className="flex-shrink-0 px-3 py-1.5 border-t border-theme-border/20 bg-theme-bg flex items-center gap-1.5 min-w-0">
        <ModelSelector onOpenSettings={handleOpenSettings} />
        <ProfileIndicator />
        <AiSafetyModeIndicator onOpenToolSettings={handleOpenToolSettings} />
        <ToolIndicator onOpenSettings={handleOpenToolSettings} />
      </div>

      {/* Input */}
      <ChatInput
        onSend={handleSend}
        onStop={stopGeneration}
        isLoading={isLoading}
        disabled={!aiEnabled || isInitializationBlocked}
        externalValue={inputValue}
        onExternalValueChange={setInputValue}
      />
      {ConfirmDialog}
    </div>
  );
}

// Conversation list item
function ConversationItem({
  conversation,
  isActive,
  onSelect,
  onDelete,
  onRename,
}: {
  conversation: AiConversation;
  isActive: boolean;
  onSelect: () => void;
  onDelete: (e: React.MouseEvent) => void;
  onRename: (id: string, title: string) => void;
}) {
  const { t } = useTranslation();
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const renameInputRef = useRef<HTMLInputElement>(null);
  const timeStr = new Date(conversation.updatedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  const handleStartRename = (e: React.MouseEvent) => {
    e.stopPropagation();
    setRenameValue(conversation.title);
    setIsRenaming(true);
    setTimeout(() => renameInputRef.current?.select(), 50);
  };

  const handleFinishRename = () => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== conversation.title) {
      onRename(conversation.id, trimmed);
    }
    setIsRenaming(false);
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => e.key === 'Enter' && onSelect()}
      className={`w-full flex items-center justify-between px-3 py-1.5 text-left group/item border-l-2 cursor-pointer ${isActive
        ? 'bg-theme-accent/5 border-theme-accent'
        : 'hover:bg-theme-bg-panel/40 border-transparent'
        }`}
    >
      <div className="flex-1 min-w-0 pr-2">
        {isRenaming ? (
          <input
            ref={renameInputRef}
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            onClick={(e) => e.stopPropagation()}
            onBlur={handleFinishRename}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') handleFinishRename();
              if (e.key === 'Escape') setIsRenaming(false);
            }}
            className="w-full text-[12px] font-bold tracking-tight bg-theme-bg/50 border border-theme-accent/40 px-1 py-0 text-theme-text focus:outline-none"
          />
        ) : (
          <div className="flex items-center gap-1.5 min-w-0">
            {conversation.origin === 'cli' && (
              <span className="inline-flex items-center justify-center w-4 h-4 rounded-md border border-theme-border/40 text-theme-text-muted/70 shrink-0">
                <Terminal className="w-2.5 h-2.5" />
              </span>
            )}
            <div
              className={`text-[12px] truncate font-bold tracking-tight ${isActive ? 'text-theme-text' : 'text-theme-text-muted group-hover/item:text-theme-text'}`}
              onDoubleClick={handleStartRename}
              title={t('ai.chat.double_click_rename')}
            >
              {conversation.title}
            </div>
          </div>
        )}
        <div className="text-[9px] text-theme-text-muted/40 uppercase tracking-tight mt-0.5 font-mono">
          {t('ai.chat.messages_count', { count: conversation.messages.length || conversation.messageCount || 0 })} · {timeStr}
        </div>
      </div>
      <button
        onClick={onDelete}
        className="flex-shrink-0 p-1 opacity-0 group-hover/item:opacity-40 hover:opacity-100 text-theme-text-muted hover:text-red-500"
        title={t('ai.chat.delete_conversation')}
      >
        <Trash2 className="w-3 h-3" />
      </button>
    </div>
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
      className="w-full flex items-center gap-3 px-3 py-1.5 rounded-md border border-transparent hover:border-theme-border/20 hover:bg-theme-bg-panel/20 hover:shadow-sm text-left group/btn transition-[color,background-color,border-color,box-shadow]"
    >
      <div className="flex-shrink-0 text-theme-text-muted group-hover/btn:text-theme-accent">
        {icon}
      </div>
      <span className="text-[12px] text-theme-text-muted group-hover/btn:text-theme-text font-medium">
        {label}
      </span>
    </button>
  );
}
