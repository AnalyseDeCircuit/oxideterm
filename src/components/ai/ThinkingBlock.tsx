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
          "py-1 px-2 hover:bg-theme-bg-subtle"
        )}
      >
        <Brain className="w-3 h-3" />
        <span>{isStreaming ? t('ai.thinking.thinking') : t('ai.thinking.thought')}</span>
        <ChevronRight className="w-3 h-3 ml-1" />
      </button>
    );
  }

  return (
    <div className="mb-3 border border-theme-border/20 bg-theme-bg-subtle/50 overflow-hidden">
      {/* Header - always visible */}
      <button
        onClick={toggleExpanded}
        className={cn(
          "w-full flex items-center gap-2 px-3 py-1.5 text-left",
          "text-[11px] text-theme-text-muted/70 hover:text-theme-text-muted",
          "hover:bg-theme-bg-subtle/80"
        )}
      >
        {isExpanded ? (
          <ChevronDown className="w-3.5 h-3.5 flex-shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 flex-shrink-0" />
        )}
        <Brain className={cn(
          "w-3.5 h-3.5 flex-shrink-0",
          isStreaming && "text-theme-accent"
        )} />
        <span className="font-medium">
          {isStreaming ? t('ai.thinking.thinking') : t('ai.thinking.thought')}
        </span>
        {isStreaming && (
          <span className="ml-auto text-[10px] text-theme-accent/60 font-mono">...</span>
        )}
      </button>

      {/* Content - collapsible */}
      {isExpanded && (
        <div className={cn(
          "px-3 pb-3 max-h-[300px] overflow-y-auto",
          "text-[12px] text-theme-text-muted/80 leading-relaxed",
          "whitespace-pre-wrap font-mono"
        )}>
          {content || (isStreaming ? t('ai.thinking.loading') : '')}
        </div>
      )}
    </div>
  );
});
