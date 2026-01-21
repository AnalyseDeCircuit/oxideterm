import { useState, useRef, useCallback, useEffect } from 'react';
import { Send, StopCircle, Terminal } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { api } from '../../lib/api';
import { useSettingsStore } from '../../store/settingsStore';
import { getTerminalBuffer } from '../../lib/terminalRegistry';

interface ChatInputProps {
  onSend: (content: string, context?: string) => void;
  onStop: () => void;
  isLoading: boolean;
  disabled?: boolean;
}

export function ChatInput({ onSend, onStop, isLoading, disabled }: ChatInputProps) {
  const [input, setInput] = useState('');
  const [includeContext, setIncludeContext] = useState(false);
  const [fetchingContext, setFetchingContext] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Get active terminal session
  const tabs = useAppStore((state) => state.tabs);
  const activeTabId = useAppStore((state) => state.activeTabId);
  const contextMaxChars = useSettingsStore((state) => state.settings.ai.contextVisibleLines);
  
  // Find active terminal tab
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const hasActiveTerminal = activeTab?.type === 'terminal' || activeTab?.type === 'local_terminal';
  const terminalSessionId = hasActiveTerminal ? activeTab?.sessionId : null;

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
    let context: string | undefined;
    if (includeContext && terminalSessionId && activeTab) {
      setFetchingContext(true);
      try {
        if (activeTab.type === 'terminal') {
          // For SSH terminals, use scroll buffer API
          const lines = await api.getScrollBuffer(terminalSessionId, 0, contextMaxChars || 50);
          if (lines.length > 0) {
            context = lines.map((l) => l.text).join('\n');
          }
        } else if (activeTab.type === 'local_terminal') {
          // For local terminals, use the terminal registry with tab ID validation
          const buffer = getTerminalBuffer(terminalSessionId, activeTab.id);
          if (buffer) {
            context = buffer;
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
  }, [input, isLoading, disabled, includeContext, terminalSessionId, activeTab, contextMaxChars, onSend]);

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
        <div className="flex items-center gap-2 mb-2">
          <button
            type="button"
            onClick={() => setIncludeContext(!includeContext)}
            disabled={fetchingContext}
            className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs transition-colors ${
              includeContext
                ? 'bg-orange-600/20 text-orange-400 border border-orange-600/30'
                : 'bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-700'
            } ${fetchingContext ? 'opacity-50 cursor-wait' : ''}`}
            title="Include terminal context"
          >
            <Terminal className="w-3 h-3" />
            <span>{fetchingContext ? 'Fetching...' : 'Include context'}</span>
          </button>
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
            placeholder={disabled ? 'Enable AI in Settings first...' : 'Ask anything about terminal...'}
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
            title="Stop generation"
          >
            <StopCircle className="w-5 h-5" />
          </button>
        ) : (
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!input.trim() || disabled}
            className="flex-shrink-0 p-2 rounded-lg bg-orange-600 hover:bg-orange-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-orange-600"
            title="Send message"
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
