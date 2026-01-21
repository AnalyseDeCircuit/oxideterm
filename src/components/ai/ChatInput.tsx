import { useState, useRef, useCallback, useEffect } from 'react';
import { Send, StopCircle, Terminal } from 'lucide-react';

interface ChatInputProps {
  onSend: (content: string, context?: string) => void;
  onStop: () => void;
  isLoading: boolean;
  disabled?: boolean;
}

export function ChatInput({ onSend, onStop, isLoading, disabled }: ChatInputProps) {
  const [input, setInput] = useState('');
  const [includeContext, setIncludeContext] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // TODO: Get active session for context capture
  const hasActiveSession = false;

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
    if (includeContext && hasActiveSession) {
      // TODO: Get last N lines from terminal buffer
      // For now, just indicate context is requested
      context = '(Terminal context requested but not yet implemented)';
    }

    onSend(trimmed, context);
    setInput('');
    setIncludeContext(false);
  }, [input, isLoading, disabled, includeContext, hasActiveSession, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
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
      {hasActiveSession && (
        <div className="flex items-center gap-2 mb-2">
          <button
            type="button"
            onClick={() => setIncludeContext(!includeContext)}
            className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs transition-colors ${
              includeContext
                ? 'bg-orange-600/20 text-orange-400 border border-orange-600/30'
                : 'bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-700'
            }`}
            title="Include terminal context"
          >
            <Terminal className="w-3 h-3" />
            <span>Include context</span>
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
