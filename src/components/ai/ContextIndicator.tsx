import { useMemo, useState, useRef, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Info, Shrink } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useAiChatStore } from '../../store/aiChatStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useAppStore } from '../../store/appStore';
import { estimateTokens, estimateToolDefinitionsTokens, getModelContextWindow, responseReserve } from '../../lib/ai/tokenUtils';
import { DEFAULT_SYSTEM_PROMPT, CONTEXT_WARNING_THRESHOLD, CONTEXT_DANGER_THRESHOLD } from '../../lib/ai/constants';
import { getToolsForContext } from '../../lib/ai/tools';
import { useSessionTreeStore } from '../../store/sessionTreeStore';

interface DetailedTokenBreakdown {
  systemInstructions: number;
  toolDefinitions: number;
  reservedOutput: number;
  messages: number;
  toolResults: number;
  total: number;
  maxTokens: number;
}

// ═══════════════════════════════════════════════════════════════════════════
// Context Window Indicator Component
// ═══════════════════════════════════════════════════════════════════════════

interface ContextIndicatorProps {
  pendingInput?: string;
}

export function ContextIndicator({ pendingInput = '' }: ContextIndicatorProps) {
  const { t } = useTranslation();
  const aiSettings = useSettingsStore((s) => s.settings.ai);
  const { activeConversationId, conversations, compactConversation } = useAiChatStore();
  const nodes = useSessionTreeStore((s) => s.nodes);
  const activeTabId = useAppStore((s) => s.activeTabId);
  const tabs = useAppStore((s) => s.tabs);
  const [showPopover, setShowPopover] = useState(false);
  const popoverRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLDivElement>(null);
  
  // Get active conversation
  const conversation = conversations.find((c) => c.id === activeConversationId);

  // Resolve active model name
  const activeModel = aiSettings.activeModel
    || aiSettings.providers?.find(p => p.id === aiSettings.activeProviderId)?.defaultModel
    || aiSettings.model
    || '';

  // Context window size
  const maxTokens = useMemo(() => {
    return getModelContextWindow(activeModel, aiSettings.modelContextWindows, aiSettings.activeProviderId ?? undefined);
  }, [activeModel, aiSettings.modelContextWindows, aiSettings.activeProviderId]);

  // Calculate detailed token breakdown
  const breakdown = useMemo<DetailedTokenBreakdown>(() => {
    // System prompt tokens
    const effectivePrompt = aiSettings.customSystemPrompt?.trim() || DEFAULT_SYSTEM_PROMPT;
    const systemInstructions = estimateTokens(effectivePrompt);
    
    // Tool definitions tokens
    const toolUseEnabled = aiSettings.toolUse?.enabled === true;
    let toolDefinitions = 0;
    if (toolUseEnabled) {
      const hasAnySSH = nodes.some(n =>
        n.runtime?.status === 'connected' || n.runtime?.status === 'active' || n.runtime?.connectionId
      );
      const activeTab = tabs.find(t => t.id === activeTabId);
      const tools = getToolsForContext(activeTab?.type ?? null, hasAnySSH);
      toolDefinitions = estimateToolDefinitionsTokens(tools);
    }

    // Reserved output tokens
    const reservedOutput = responseReserve(maxTokens);

    // Messages vs tool results
    let messages = 0;
    let toolResults = 0;
    if (conversation) {
      for (const msg of conversation.messages) {
        if (msg.role === 'user' || msg.role === 'assistant') {
          messages += estimateTokens(msg.content);
          // Count tokens from tool call arguments and results
          if (msg.toolCalls) {
            for (const tc of msg.toolCalls) {
              toolResults += estimateTokens(tc.arguments);
              if (tc.result) {
                toolResults += estimateTokens(tc.result.output);
              }
            }
          }
        }
      }
    }
    
    // Pending input
    messages += estimateTokens(pendingInput);
    
    const total = systemInstructions + toolDefinitions + reservedOutput + messages + toolResults;
    
    return { systemInstructions, toolDefinitions, reservedOutput, messages, toolResults, total, maxTokens };
  }, [conversation?.messages, pendingInput, aiSettings.customSystemPrompt, aiSettings.toolUse?.enabled, maxTokens, nodes, activeTabId, tabs]);
  
  const percentage = Math.min((breakdown.total / maxTokens) * 100, 100);
  const isWarning = percentage > CONTEXT_WARNING_THRESHOLD * 100;
  const isDanger = percentage > CONTEXT_DANGER_THRESHOLD * 100;
  
  // Color based on usage
  const barColor = isDanger 
    ? 'bg-red-500' 
    : isWarning 
      ? 'bg-amber-500' 
      : 'bg-theme-accent';
  
  const textColor = isDanger
    ? 'text-red-500'
    : isWarning
      ? 'text-amber-500'
      : 'text-theme-text-muted';
  
  // Format number with K suffix
  const formatTokens = (n: number) => {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return n.toString();
  };

  // Calculate percentage of context window for each category
  const pct = (n: number) => {
    if (maxTokens === 0) return '0%';
    const p = (n / maxTokens) * 100;
    return p < 0.1 ? '<0.1%' : `${p.toFixed(1)}%`;
  };

  // Close popover on outside click
  useEffect(() => {
    if (!showPopover) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (
        popoverRef.current && !popoverRef.current.contains(e.target as Node) &&
        triggerRef.current && !triggerRef.current.contains(e.target as Node)
      ) {
        setShowPopover(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [showPopover]);

  // Calculate popover position relative to viewport
  const popoverPos = useMemo(() => {
    if (!showPopover || !triggerRef.current) return { top: 0, left: 0 };
    const rect = triggerRef.current.getBoundingClientRect();
    return {
      top: rect.top - 8, // 8px gap above trigger
      left: rect.left,
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showPopover]);

  const handleCompact = useCallback(async () => {
    setShowPopover(false);
    await compactConversation();
  }, [compactConversation]);
  
  return (
    <div className="relative" ref={triggerRef}>
      <div 
        className="flex items-center gap-1.5 sm:gap-2 cursor-pointer group shrink-0"
        onClick={() => setShowPopover(!showPopover)}
      >
        <Info className={cn('w-3 h-3 shrink-0 opacity-50 group-hover:opacity-100', textColor)} />
        
        {/* Mini progress bar */}
        <div className="w-10 sm:w-16 h-1 bg-theme-border/20 rounded-full overflow-hidden">
          <div 
            className={`h-full ${barColor}`}
            style={{ width: `${percentage}%` }}
          />
        </div>
        
        {/* Token count - always visible but compact */}
        <span className={cn('text-[9px] font-mono opacity-60 whitespace-nowrap', textColor)}>
          {formatTokens(breakdown.total)}
        </span>
      </div>

      {/* Detail popover — rendered via portal to escape overflow:hidden ancestors */}
      {showPopover && createPortal(
        <div
          ref={popoverRef}
          className="fixed w-60 bg-theme-bg-panel border border-theme-border/30 rounded-lg shadow-xl z-[9999] overflow-hidden"
          style={{
            top: popoverPos.top,
            left: popoverPos.left,
            transform: 'translateY(-100%)',
          }}
        >
          {/* Header: Context Window */}
          <div className="px-3 pt-3 pb-2">
            <div className="text-[11px] font-semibold text-theme-text mb-0.5">
              {t('ai.context.breakdown')}
            </div>
            <div className="flex items-baseline justify-between mb-1.5">
              <span className="text-[12px] font-mono text-theme-text">
                {formatTokens(breakdown.total)} / {formatTokens(maxTokens)} tokens
              </span>
              <span className={cn('text-[11px] font-mono font-semibold', textColor)}>
                {Math.round(percentage)}%
              </span>
            </div>
            {/* Full-width progress bar */}
            <div className="w-full h-1 bg-theme-border/20 rounded-full overflow-hidden">
              <div 
                className={`h-full ${barColor} transition-all duration-300`}
                style={{ width: `${percentage}%` }}
              />
            </div>
          </div>

          <div className="border-t border-theme-border/10" />

          {/* System section */}
          <div className="px-3 py-2">
            <div className="text-[10px] font-semibold text-theme-text-muted uppercase tracking-wider mb-1.5">
              {t('ai.context.system')}
            </div>
            <BreakdownRow label={t('ai.context.system_instructions')} value={pct(breakdown.systemInstructions)} />
            <BreakdownRow label={t('ai.context.tool_definitions')} value={pct(breakdown.toolDefinitions)} />
            <BreakdownRow label={t('ai.context.reserved_output')} value={pct(breakdown.reservedOutput)} />
          </div>

          <div className="border-t border-theme-border/10" />

          {/* User Context section */}
          <div className="px-3 py-2">
            <div className="text-[10px] font-semibold text-theme-text-muted uppercase tracking-wider mb-1.5">
              {t('ai.context.user_context')}
            </div>
            <BreakdownRow label={t('ai.context.messages_label')} value={pct(breakdown.messages)} />
            <BreakdownRow label={t('ai.context.tool_results')} value={pct(breakdown.toolResults)} />
          </div>

          {/* Compact button */}
          {conversation && conversation.messages.length >= 4 && (
            <>
              <div className="border-t border-theme-border/10" />
              <div className="px-3 py-2">
                <button
                  onClick={handleCompact}
                  className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 text-[11px] font-medium text-theme-text bg-theme-border/10 hover:bg-theme-border/20 rounded-md transition-colors"
                >
                  <Shrink className="w-3 h-3" />
                  {t('ai.context.compress_dialog')}
                </button>
              </div>
            </>
          )}
        </div>,
        document.body
      )}
    </div>
  );
}

/** A single row in the breakdown panel */
function BreakdownRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-0.5">
      <span className="text-[11px] text-theme-text-muted">{label}</span>
      <span className="text-[11px] font-mono text-theme-text">{value}</span>
    </div>
  );
}
