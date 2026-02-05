import { memo, useState, useCallback } from 'react';
import { ChevronDown, ChevronRight, Brain } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../../store/settingsStore';
import { cn } from '../../lib/utils';

interface ThinkingBlockProps {
  /** The thinking content to display */
  content: string;
  /** Whether thinking is currently streaming */
  isStreaming?: boolean;
  /** Whether expanded by default (overrides settings) */
  defaultExpanded?: boolean;
}

/**
 * ThinkingBlock - Displays AI model's reasoning/thinking process
 * 
 * Features:
 * - Collapsible panel with expand/collapse toggle
 * - Shimmer animation during streaming
 * - Supports 'detailed' and 'compact' display styles
 * - Scrollable content for long thinking outputs
 */
export const ThinkingBlock = memo(function ThinkingBlock({
  content,
  isStreaming = false,
  defaultExpanded,
}: ThinkingBlockProps) {
  const { t } = useTranslation();
  const { settings } = useSettingsStore();
  const { thinkingStyle, thinkingDefaultExpanded } = settings.ai;

  // Determine initial expanded state
  const initialExpanded = defaultExpanded ?? thinkingDefaultExpanded;
  const [isExpanded, setIsExpanded] = useState(initialExpanded);

  const toggleExpanded = useCallback(() => {
    setIsExpanded(prev => !prev);
  }, []);

  // Compact mode: show minimal indicator
  if (thinkingStyle === 'compact' && !isExpanded) {
    return (
      <button
        onClick={toggleExpanded}
        className={cn(
          "flex items-center gap-1.5 text-[11px] text-theme-text-muted/60 hover:text-theme-text-muted",
          "transition-colors py-1 px-2 rounded-md hover:bg-theme-bg-subtle",
          isStreaming && "animate-pulse"
        )}
      >
        <Brain className="w-3 h-3" />
        <span>{isStreaming ? t('ai.thinking.thinking') : t('ai.thinking.thought')}</span>
        <ChevronRight className="w-3 h-3 ml-1" />
      </button>
    );
  }

  return (
    <div className="mb-3 rounded-lg border border-theme-border/30 bg-theme-bg-subtle/50 overflow-hidden">
      {/* Header - always visible */}
      <button
        onClick={toggleExpanded}
        className={cn(
          "w-full flex items-center gap-2 px-3 py-2 text-left",
          "text-[11px] text-theme-text-muted/70 hover:text-theme-text-muted",
          "transition-colors hover:bg-theme-bg-subtle/80"
        )}
      >
        {isExpanded ? (
          <ChevronDown className="w-3.5 h-3.5 flex-shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 flex-shrink-0" />
        )}
        <Brain className={cn(
          "w-3.5 h-3.5 flex-shrink-0",
          isStreaming && "animate-pulse text-theme-accent"
        )} />
        <span className="font-medium">
          {isStreaming ? t('ai.thinking.thinking') : t('ai.thinking.thought')}
        </span>
        {isStreaming && (
          <span className="ml-auto flex items-center gap-1">
            <span className="inline-block w-1 h-1 rounded-full bg-theme-accent animate-bounce" style={{ animationDelay: '0ms' }} />
            <span className="inline-block w-1 h-1 rounded-full bg-theme-accent animate-bounce" style={{ animationDelay: '150ms' }} />
            <span className="inline-block w-1 h-1 rounded-full bg-theme-accent animate-bounce" style={{ animationDelay: '300ms' }} />
          </span>
        )}
      </button>

      {/* Content - collapsible */}
      {isExpanded && (
        <div className={cn(
          "px-3 pb-3 max-h-[300px] overflow-y-auto",
          "text-[12px] text-theme-text-muted/80 leading-relaxed",
          "whitespace-pre-wrap font-mono",
          // Shimmer effect during streaming
          isStreaming && "relative overflow-hidden"
        )}>
          {content || (isStreaming ? t('ai.thinking.loading') : '')}
          
          {/* Shimmer overlay during streaming */}
          {isStreaming && content && (
            <div 
              className="absolute inset-0 pointer-events-none"
              style={{
                background: 'linear-gradient(90deg, transparent 0%, rgba(var(--theme-accent-rgb), 0.05) 50%, transparent 100%)',
                animation: 'shimmer 2s infinite',
              }}
            />
          )}
        </div>
      )}

      {/* Shimmer animation keyframes - injected once */}
      <style>{`
        @keyframes shimmer {
          0% { transform: translateX(-100%); }
          100% { transform: translateX(100%); }
        }
      `}</style>
    </div>
  );
});
