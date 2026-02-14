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
    <div className="bg-theme-bg border-t border-theme-border/40 px-3 py-2.5">
      {/* Context Toggles — Flat Rectangular Chips */}
      {(hasActiveTerminal || hasSplitPanes) && (
        <div className="flex flex-wrap items-center gap-1.5 mb-2">
          {hasActiveTerminal && (
            <button
              type="button"
              onClick={() => setIncludeContext(!includeContext)}
              disabled={fetchingContext}
              className={`flex items-center gap-1 px-2 py-0.5 text-[10px] font-bold tracking-tight uppercase border shrink-0 ${includeContext
                ? 'bg-theme-accent/10 border-theme-accent/30 text-theme-accent'
                : 'bg-transparent text-theme-text-muted border-theme-border/30 hover:border-theme-border/50'
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
              className={`flex items-center gap-1 px-2 py-0.5 text-[10px] font-bold tracking-tight uppercase border shrink-0 ${includeAllPanes
                ? 'bg-blue-500/10 border-blue-500/30 text-blue-500'
                : 'bg-transparent text-theme-text-muted border-theme-border/30 hover:border-theme-border/50'
                } ${fetchingContext ? 'opacity-50 cursor-wait' : ''}`}
            >
              <Layers className="w-3 h-3" />
              <span>{t('ai.input.panes')}</span>
            </button>
          )}
        </div>
      )}

      {/* Input area — Flat, no rounded corners, integrated */}
      <div className="flex flex-col bg-theme-bg-panel/15 border border-theme-border/40 focus-within:border-theme-accent/40">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(e) => handleInputChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={disabled ? t('ai.input.placeholder_disabled') : t('ai.input.placeholder')}
          disabled={disabled || isLoading}
          rows={1}
          className="w-full resize-none bg-transparent border-none px-3 py-2 text-[13px] text-theme-text placeholder-theme-text-muted/30 focus:outline-none focus:ring-0 disabled:opacity-50 leading-relaxed min-h-[36px]"
        />

        <div className="flex items-center justify-between px-2 py-1 border-t border-theme-border/10">
          <div className="flex items-center gap-2 text-[9px] font-bold tracking-tight text-theme-text-muted/30 uppercase min-w-0 overflow-hidden">
            {isLoading ? (
              <div className="flex items-center gap-1 text-theme-accent">
                <Sparkles className="w-3 h-3 shrink-0" />
                <span className="truncate">{t('ai.input.thinking')}</span>
              </div>
            ) : (
              <ContextIndicator pendingInput={input} />
            )}
          </div>

          <div className="flex items-center gap-1.5">
            {!isLoading && (
              <span className="text-[9px] text-theme-text-muted/20 font-mono hidden sm:inline">
                SHIFT+ENTER
              </span>
            )}
            {isLoading ? (
              <button
                type="button"
                onClick={onStop}
                className="flex items-center gap-1 px-2 py-0.5 bg-red-500/10 hover:bg-red-500/15 text-red-500 text-[10px] font-bold"
                title={t('ai.input.stop_generation')}
              >
                <StopCircle className="w-3 h-3" />
                {t('ai.input.stop')}
              </button>
            ) : (
              <button
                type="button"
                onClick={handleSubmit}
                disabled={!input.trim() || disabled}
                className="px-2.5 py-0.5 bg-theme-accent text-theme-bg hover:opacity-90 disabled:opacity-20 disabled:grayscale font-bold text-[10px]"
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
