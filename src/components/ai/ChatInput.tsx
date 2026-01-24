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
    <div className="border-t border-zinc-700/50 bg-zinc-800/50 p-3">
      {/* Context toggle */}
      {hasActiveTerminal && (
        <div className="flex items-center gap-2 mb-2 flex-wrap">
          <button
            type="button"
            onClick={() => setIncludeContext(!includeContext)}
            disabled={fetchingContext}
            className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs transition-colors ${
              includeContext
                ? 'bg-orange-600/20 text-orange-400 border border-orange-600/30'
                : 'bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-700'
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
              className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs transition-colors ${
                includeAllPanes
                  ? 'bg-purple-600/20 text-purple-400 border border-purple-600/30'
                  : 'bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-700'
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
      <div className="flex items-end gap-2">
        <div className="flex-1 relative">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={disabled ? t('ai.input.placeholder_disabled') : t('ai.input.placeholder')}
            disabled={disabled || isLoading}
            rows={1}
            className="w-full resize-none bg-zinc-900/50 border border-zinc-700/50 rounded-lg px-3 py-2 text-sm text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-orange-600/50 focus:ring-1 focus:ring-orange-600/25 disabled:opacity-50 disabled:cursor-not-allowed"
          />
        </div>

        {isLoading ? (
          <button
            type="button"
            onClick={onStop}
            className="flex-shrink-0 p-2 rounded-lg bg-red-600 hover:bg-red-500 text-white transition-colors"
            title={t('ai.input.stop_generation')}
          >
            <StopCircle className="w-5 h-5" />
          </button>
        ) : (
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!input.trim() || disabled}
            className="flex-shrink-0 p-2 rounded-lg bg-orange-600 hover:bg-orange-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-orange-600"
            title={t('ai.input.send')}
          >
            <Send className="w-5 h-5" />
          </button>
        )}
      </div>

      {/* Hint */}
      <div className="mt-2 text-[10px] text-zinc-500">
        Press <kbd className="px-1 py-0.5 bg-zinc-700/50 rounded text-zinc-400">Enter</kbd> to send,{' '}
        <kbd className="px-1 py-0.5 bg-zinc-700/50 rounded text-zinc-400">Shift+Enter</kbd> for new line
      </div>
    </div>
  );
}
