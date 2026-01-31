import { useState, useRef, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Send, StopCircle, Terminal, Layers } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { api } from '../../lib/api';
import { useSettingsStore } from '../../store/settingsStore';
import {
  getActiveTerminalBuffer,
  getActivePaneId,
  getActivePaneMetadata,
  getCombinedPaneContext
} from '../../lib/terminalRegistry';

interface ChatInputProps {
  onSend: (content: string, context?: string) => void;
  onStop: () => void;
  isLoading: boolean;
  disabled?: boolean;
}

export function ChatInput({ onSend, onStop, isLoading, disabled }: ChatInputProps) {
  const { t } = useTranslation();
  const [input, setInput] = useState('');
  const [includeContext, setIncludeContext] = useState(false);
  const [includeAllPanes, setIncludeAllPanes] = useState(false);
  const [fetchingContext, setFetchingContext] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Get active terminal session
  const tabs = useAppStore((state) => state.tabs);
  const activeTabId = useAppStore((state) => state.activeTabId);
  const contextMaxChars = useSettingsStore((state) => state.settings.ai.contextVisibleLines);

  // Find active terminal tab
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const hasActiveTerminal = activeTab?.type === 'terminal' || activeTab?.type === 'local_terminal';

  // Check if tab has multiple panes (split panes)
  const hasSplitPanes = hasActiveTerminal && activeTab?.rootPane?.type === 'group';

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = Math.min(textarea.scrollHeight, 150) + 'px';
    }
  }, [input]);

  const handleSubmit = useCallback(async () => {
    const trimmed = input.trim();
    if (!trimmed || isLoading || disabled) return;

    // Get terminal context if requested
    // Now uses unified Registry for both SSH and Local terminals
    let context: string | undefined;
    if (includeContext && hasActiveTerminal && activeTab) {
      setFetchingContext(true);
      try {
        // Cross-Pane Vision: Gather context from ALL panes if enabled
        if (includeAllPanes && hasSplitPanes) {
          const maxCharsPerPane = contextMaxChars ? Math.floor(contextMaxChars / 4) : 2000;
          context = getCombinedPaneContext(activeTab.id, maxCharsPerPane);
          if (!context) {
            console.warn('[AI] getCombinedPaneContext returned empty, falling back to active pane');
          }
        }

        // Fallback to active pane only
        if (!context) {
          const activePaneId = getActivePaneId();
          if (activePaneId) {
            // Get buffer from registry (validates tab ID for security)
            const buffer = getActiveTerminalBuffer(activeTab.id);
            if (buffer) {
              // Trim to contextMaxChars if needed
              context = contextMaxChars && buffer.length > contextMaxChars
                ? buffer.slice(-contextMaxChars)
                : buffer;
            } else {
              // Fallback: For SSH terminals, try backend API if Registry returns null
              const metadata = getActivePaneMetadata();
              if (metadata?.terminalType === 'terminal' && metadata.sessionId) {
                const lines = await api.getScrollBuffer(metadata.sessionId, 0, contextMaxChars || 50);
                if (lines.length > 0) {
                  context = lines.map((l) => l.text).join('\n');
                }
              }
            }
          }
        }
      } catch (e) {
        console.error('[AI] Failed to get terminal context:', e);
      } finally {
        setFetchingContext(false);
      }
    }

    onSend(trimmed, context);
    setInput('');
    setIncludeContext(false);
    setIncludeAllPanes(false);
  }, [input, isLoading, disabled, includeContext, includeAllPanes, hasSplitPanes, hasActiveTerminal, activeTab, contextMaxChars, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Ignore Enter during IME composition (e.g., Chinese input)
      if (e.nativeEvent.isComposing || e.keyCode === 229) return;

      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit]
  );

  return (
    <div className="bg-theme-bg-panel/30 border-t border-theme-border p-4">
      {/* Context toggle */}
      {hasActiveTerminal && (
        <div className="flex items-center gap-2 mb-3 flex-wrap">
          <button
            type="button"
            onClick={() => setIncludeContext(!includeContext)}
            disabled={fetchingContext}
            className={`flex items-center gap-1.5 px-2 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-all ${includeContext
              ? 'bg-theme-accent text-theme-bg shadow-sm'
              : 'bg-theme-bg text-theme-text-muted hover:text-theme-text border border-theme-border/50'
              } ${fetchingContext ? 'opacity-50 cursor-wait' : ''}`}
            title={t('ai.input.include_context')}
          >
            <Terminal className="w-3 h-3" />
            <span>{fetchingContext ? t('ai.input.fetching_context') : t('ai.input.include_context')}</span>
          </button>

          {/* Cross-Pane Vision: Include all split panes */}
          {hasSplitPanes && includeContext && (
            <button
              type="button"
              onClick={() => setIncludeAllPanes(!includeAllPanes)}
              disabled={fetchingContext}
              className={`flex items-center gap-1.5 px-2 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-all ${includeAllPanes
                ? 'bg-blue-500 text-white shadow-sm'
                : 'bg-theme-bg text-theme-text-muted hover:text-theme-text border border-theme-border/50'
                } ${fetchingContext ? 'opacity-50 cursor-wait' : ''}`}
              title={t('ai.input.include_all_panes')}
            >
              <Layers className="w-3 h-3" />
              <span>{t('ai.input.include_all_panes')}</span>
            </button>
          )}
        </div>
      )}

      {/* Input area */}
      <div className="flex items-end gap-2 bg-theme-bg border border-theme-border rounded-xl focus-within:border-theme-accent/50 focus-within:ring-2 focus-within:ring-theme-accent/10 transition-all p-2 pr-2">
        <div className="flex-1 relative">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={disabled ? t('ai.input.placeholder_disabled') : t('ai.input.placeholder')}
            disabled={disabled || isLoading}
            rows={1}
            className="w-full resize-none bg-transparent border-none rounded-none px-2 py-1.5 text-sm text-theme-text placeholder-theme-text-muted/40 focus:outline-none focus:ring-0 disabled:opacity-50 disabled:cursor-not-allowed"
          />
        </div>

        {isLoading ? (
          <button
            type="button"
            onClick={onStop}
            className="flex-shrink-0 mb-0.5 p-2 rounded-lg bg-red-500 hover:bg-red-600 text-white shadow-sm transition-all active:scale-95"
            title={t('ai.input.stop_generation')}
          >
            <StopCircle className="w-4 h-4" />
          </button>
        ) : (
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!input.trim() || disabled}
            className="flex-shrink-0 mb-0.5 p-2 rounded-lg bg-theme-accent hover:opacity-90 text-theme-bg shadow-sm transition-all disabled:opacity-30 disabled:grayscale disabled:scale-100 active:scale-95"
            title={t('ai.input.send')}
          >
            <Send className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* Hint */}
      <div className="mt-3 flex items-center justify-between text-[10px] text-theme-text-muted opacity-50 px-1">
        <div className="flex items-center gap-3">
          <span>
            <kbd className="font-sans px-1 py-0.5 bg-theme-bg border border-theme-border rounded text-[9px]">Enter</kbd>
            {' '}{t('ai.input.send_hint', 'to send')}
          </span>
          <span>
            <kbd className="font-sans px-1 py-0.5 bg-theme-bg border border-theme-border rounded text-[9px]">Shift+Enter</kbd>
            {' '}{t('ai.input.newline_hint', 'new line')}
          </span>
        </div>
      </div>
    </div>
  );
}
