import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Info } from 'lucide-react';
import { useAiChatStore } from '../../store/aiChatStore';
import { useSettingsStore } from '../../store/settingsStore';

// ═══════════════════════════════════════════════════════════════════════════
// Token Estimation
// ═══════════════════════════════════════════════════════════════════════════

// Hardcoded system prompt (matches aiChatStore.ts)
const DEFAULT_SYSTEM_PROMPT = `You are a helpful terminal assistant. You help users with shell commands, scripts, and terminal operations. Be concise and direct. When providing commands, format them clearly. You can use markdown for formatting.`;

// ═══════════════════════════════════════════════════════════════════════════
// Token Estimation
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Rough token estimation (1 token ≈ 4 chars for English, ~2 for CJK)
 * This is a heuristic - actual tokenization varies by model
 */
function estimateTokens(text: string): number {
  if (!text) return 0;
  
  // Count CJK characters (Chinese, Japanese, Korean)
  const cjkRegex = /[\u4e00-\u9fff\u3040-\u309f\u30a0-\u30ff\uac00-\ud7af]/g;
  const cjkMatches = text.match(cjkRegex);
  const cjkCount = cjkMatches?.length || 0;
  
  // Non-CJK characters
  const nonCjkLength = text.length - cjkCount;
  
  // CJK: ~1.5 tokens per char, Latin: ~0.25 tokens per char (1 token ≈ 4 chars)
  return Math.ceil(cjkCount * 1.5 + nonCjkLength * 0.25);
}

interface TokenBreakdown {
  system: number;
  history: number;
  context: number;
  total: number;
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
  const { activeConversationId, conversations } = useAiChatStore();
  
  // Get active conversation
  const conversation = conversations.find((c) => c.id === activeConversationId);
  
  // Calculate token breakdown
  const breakdown = useMemo<TokenBreakdown>(() => {
    // System prompt tokens (using default, actual may vary with context)
    const systemTokens = estimateTokens(DEFAULT_SYSTEM_PROMPT);
    
    // History tokens (last N messages sent to API)
    let historyTokens = 0;
    if (conversation) {
      const recentMessages = conversation.messages.slice(-10); // Match API logic
      for (const msg of recentMessages) {
        if (msg.role === 'user' || msg.role === 'assistant') {
          historyTokens += estimateTokens(msg.content);
        }
      }
    }
    
    // Pending input + context
    const contextTokens = estimateTokens(pendingInput);
    
    return {
      system: systemTokens,
      history: historyTokens,
      context: contextTokens,
      total: systemTokens + historyTokens + contextTokens,
    };
  }, [conversation?.messages, pendingInput]);
  
  // Context window limits by model family (rough estimates)
  const maxTokens = useMemo(() => {
    const model = aiSettings.model.toLowerCase();
    if (model.includes('gpt-4-turbo') || model.includes('gpt-4o')) return 128000;
    if (model.includes('gpt-4-32k')) return 32000;
    if (model.includes('gpt-4')) return 8192;
    if (model.includes('gpt-3.5-turbo-16k')) return 16000;
    if (model.includes('gpt-3.5')) return 4096;
    if (model.includes('claude-3')) return 200000;
    if (model.includes('claude-2')) return 100000;
    if (model.includes('claude')) return 100000;
    if (model.includes('gemini')) return 128000;
    if (model.includes('llama-3')) return 8192;
    if (model.includes('mistral')) return 32000;
    if (model.includes('qwen')) return 32000;
    if (model.includes('deepseek')) return 128000;
    // Default for unknown models
    return 8192;
  }, [aiSettings.model]);
  
  const percentage = Math.min((breakdown.total / maxTokens) * 100, 100);
  const isWarning = percentage > 70;
  const isDanger = percentage > 90;
  
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
  
  // Build tooltip text
  const tooltipText = [
    `${t('ai.context.system')}: ${formatTokens(breakdown.system)}`,
    `${t('ai.context.history')}: ${formatTokens(breakdown.history)}`,
    `${t('ai.context.pending')}: ${formatTokens(breakdown.context)}`,
    `${t('ai.context.total')}: ${formatTokens(breakdown.total)} / ${formatTokens(maxTokens)}`,
    isDanger ? `⚠️ ${t('ai.context.warning_limit')}` : '',
  ].filter(Boolean).join('\n');
  
  return (
    <div 
      className="flex items-center gap-1.5 sm:gap-2 cursor-help group shrink-0"
      title={tooltipText}
    >
      <Info className={`w-3 h-3 shrink-0 ${textColor} opacity-50 group-hover:opacity-100 transition-opacity`} />
      
      {/* Mini progress bar */}
      <div className="w-10 sm:w-16 h-1 bg-theme-border/20 rounded-full overflow-hidden">
        <div 
          className={`h-full ${barColor} transition-all duration-300`}
          style={{ width: `${percentage}%` }}
        />
      </div>
      
      {/* Token count - always visible but compact */}
      <span className={`text-[9px] font-mono ${textColor} opacity-60 whitespace-nowrap`}>
        {formatTokens(breakdown.total)}
      </span>
    </div>
  );
}
