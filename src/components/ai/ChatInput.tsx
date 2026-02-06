import { useState, useRef, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { StopCircle, Terminal, Layers, Sparkles } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { api } from '../../lib/api';
import { useSettingsStore } from '../../store/settingsStore';
import { ContextIndicator } from './ContextIndicator';
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
  externalValue?: string;
  onExternalValueChange?: (value: string) => void;
}

export function ChatInput({ onSend, onStop, isLoading, disabled, externalValue, onExternalValueChange }: ChatInputProps) {
  const { t } = useTranslation();
  const [input, setInput] = useState('');
  const [includeContext, setIncludeContext] = useState(false);
  const [includeAllPanes, setIncludeAllPanes] = useState(false);
  const [fetchingContext, setFetchingContext] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Sync with external value (from quick prompts)
  useEffect(() => {
    if (externalValue !== undefined && externalValue !== input) {
      setInput(externalValue);
      // Focus the textarea when value is set externally
      textareaRef.current?.focus();
    }
  }, [externalValue]);

  // Notify parent of changes
  const handleInputChange = (value: string) => {
    setInput(value);
    onExternalValueChange?.(value);
  };

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
    onExternalValueChange?.('');
    setIncludeContext(false);
    setIncludeAllPanes(false);
  }, [input, isLoading, disabled, includeContext, includeAllPanes, hasSplitPanes, hasActiveTerminal, activeTab, contextMaxChars, onSend, onExternalValueChange]);

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
    <div className="bg-theme-bg border-t border-theme-border/50 p-4">
      {/* Context Toggles - Copilot Style Minimalist Chips */}
      {(hasActiveTerminal || hasSplitPanes) && (
        <div className="flex flex-wrap items-center gap-2 mb-3">
          {hasActiveTerminal && (
            <button
              type="button"
              onClick={() => setIncludeContext(!includeContext)}
              disabled={fetchingContext}
              className={`flex items-center gap-1.5 px-2 py-0.5 rounded-sm text-[10px] font-bold tracking-tight uppercase transition-all border shrink-0 ${includeContext
                ? 'bg-theme-accent/10 border-theme-accent/40 text-theme-accent'
                : 'bg-theme-bg-panel/20 text-theme-text-muted border-theme-border/30 hover:border-theme-border/60'
                } ${fetchingContext ? 'opacity-50 cursor-wait' : ''}`}
            >
              <Terminal className="w-3 h-3" />
              <span>{fetchingContext ? t('ai.input.context_loading') : t('ai.input.context')}</span>
            </button>
          )}

          {hasSplitPanes && includeContext && (
            <button
              type="button"
              onClick={() => setIncludeAllPanes(!includeAllPanes)}
              disabled={fetchingContext}
              className={`flex items-center gap-1.5 px-2 py-0.5 rounded-sm text-[10px] font-bold tracking-tight uppercase transition-all border shrink-0 ${includeAllPanes
                ? 'bg-blue-500/10 border-blue-500/40 text-blue-500'
                : 'bg-theme-bg-panel/20 text-theme-text-muted border-theme-border/30 hover:border-theme-border/60'
                } ${fetchingContext ? 'opacity-50 cursor-wait' : ''}`}
            >
              <Layers className="w-3 h-3" />
              <span>{t('ai.input.panes')}</span>
            </button>
          )}
        </div>
      )}

      {/* Input area - Strictly Flat and Integrated */}
      <div className="flex flex-col bg-theme-bg-panel/20 border border-theme-border/60 rounded-sm focus-within:border-theme-accent/50 transition-all">
        <div className="flex-1 min-w-0">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => handleInputChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={disabled ? t('ai.input.placeholder_disabled') : t('ai.input.placeholder')}
            disabled={disabled || isLoading}
            rows={1}
            className="w-full resize-none bg-transparent border-none rounded-none px-3 py-2 text-[13px] text-theme-text placeholder-theme-text-muted/40 focus:outline-none focus:ring-0 disabled:opacity-50 leading-relaxed min-h-[40px]"
          />
        </div>

        <div className="flex items-center justify-between px-2 py-1.5 bg-theme-bg-panel/10 border-t border-theme-border/10">
          <div className="flex items-center gap-2 sm:gap-3 text-[9px] font-bold tracking-tight text-theme-text-muted opacity-40 uppercase min-w-0 overflow-hidden">
            {isLoading ? (
              <div className="flex items-center gap-1.5 text-theme-accent animate-pulse">
                <Sparkles className="w-3 h-3 shrink-0" />
                <span className="truncate">{t('ai.input.thinking')}</span>
              </div>
            ) : (
              <ContextIndicator pendingInput={input} />
            )}
          </div>

          <div className="flex items-center gap-2">
            {isLoading ? (
              <button
                type="button"
                onClick={onStop}
                className="p-1 px-2 rounded-sm bg-red-500/10 hover:bg-red-500/20 text-red-500 transition-all flex items-center gap-1"
                title={t('ai.input.stop_generation')}
              >
                <StopCircle className="w-3 h-3" />
                <span className="text-[10px] font-bold">{t('ai.input.stop')}</span>
              </button>
            ) : (
              <button
                type="button"
                onClick={handleSubmit}
                disabled={!input.trim() || disabled}
                className="p-1 px-3 rounded-sm bg-theme-accent text-theme-bg hover:opacity-90 transition-all disabled:opacity-20 disabled:grayscale font-bold text-[10px]"
                title={t('ai.input.send')}
              >
                {t('ai.input.send_btn')}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
