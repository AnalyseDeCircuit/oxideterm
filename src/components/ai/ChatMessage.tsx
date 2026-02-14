import { memo, useMemo, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { RotateCcw } from 'lucide-react';
import { emit } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { AiChatMessage } from '../../types';
import { renderMarkdown, markdownStyles, renderMathInElement } from '../../lib/markdownRenderer';
import { useMermaid } from '../../hooks/useMermaid';
import { ThinkingBlock } from './ThinkingBlock';

interface ChatMessageProps {
  message: AiChatMessage;
  /** Whether this is the last assistant message (for regenerate button) */
  isLastAssistant?: boolean;
  /** Callback to regenerate the response */
  onRegenerate?: () => void;
  /** Whether regeneration is in progress */
  isRegenerating?: boolean;
}

// Inject markdown styles once
let stylesInjected = false;
function injectStyles(): void {
  if (stylesInjected) return;
  const style = document.createElement('style');
  style.id = 'ai-markdown-styles';
  style.textContent = markdownStyles;
  document.head.appendChild(style);
  stylesInjected = true;
}

// Simple HTML escape for user messages
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// Custom comparison for memo - only re-render when content actually changes
function arePropsEqual(prev: ChatMessageProps, next: ChatMessageProps): boolean {
  return (
    prev.message.id === next.message.id &&
    prev.message.content === next.message.content &&
    prev.message.isStreaming === next.message.isStreaming &&
    prev.message.thinkingContent === next.message.thinkingContent &&
    prev.message.isThinkingStreaming === next.message.isThinkingStreaming &&
    prev.isLastAssistant === next.isLastAssistant &&
    prev.isRegenerating === next.isRegenerating
  );
}

export const ChatMessage = memo(function ChatMessage({
  message,
  isLastAssistant = false,
  onRegenerate,
  isRegenerating = false,
}: ChatMessageProps) {
  const { t } = useTranslation();
  const isUser = message.role === 'user';
  const contentRef = useRef<HTMLDivElement>(null);

  // Inject styles on mount
  useEffect(() => {
    injectStyles();
  }, []);

  // Render markdown content
  const renderedHtml = useMemo(() => {
    if (isUser) {
      // For user messages, simple text with line breaks
      return message.content
        .split('\n')
        .map(line => `<p class="md-paragraph">${escapeHtml(line)}</p>`)
        .join('');
    }
    return renderMarkdown(message.content);
  }, [message.content, isUser]);

  // Handle Mermaid diagram rendering
  useMermaid(contentRef, message.content);

  // Handle KaTeX math formula rendering
  useEffect(() => {
    if (contentRef.current && !isUser) {
      // Render math formulas after content is in DOM
      renderMathInElement(contentRef.current);
    }
  }, [renderedHtml, isUser]);

  // Handle code block interactions
  const handleClick = useCallback(async (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    const button = target.closest('button[data-action]') as HTMLButtonElement | null;
    const link = target.closest('a') as HTMLAnchorElement | null;

    // Handle code block buttons
    if (button) {
      const action = button.dataset.action;
      const targetId = button.dataset.target;

      if (targetId) {
        const codeBlock = contentRef.current?.querySelector(`[data-code-id="${targetId}"]`);
        const code = codeBlock?.getAttribute('data-code')
          ?.replace(/&amp;/g, '&')
          ?.replace(/&quot;/g, '"')
          ?.replace(/&lt;/g, '<')
          ?.replace(/&gt;/g, '>');

        if (code) {
          if (action === 'copy') {
            await navigator.clipboard.writeText(code);
            button.classList.add('copied');
            const span = button.querySelector('span');
            if (span) {
              const originalText = span.textContent;
              span.textContent = '✓';
              setTimeout(() => {
                button.classList.remove('copied');
                if (span) span.textContent = originalText;
              }, 2000);
            }
          } else if (action === 'run') {
            await emit('ai-insert-command', { command: code });
          }
        }
      }
      e.preventDefault();
      return;
    }

    // Handle links
    if (link) {
      e.preventDefault();

      // File path link
      const filePath = link.dataset.filePath;
      if (filePath) {
        // Emit event to navigate to file in terminal
        await emit('ai-open-file', { path: filePath });
        return;
      }

      // External link - open in system browser
      const href = link.getAttribute('href');
      if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
        await openUrl(href);
        return;
      }
    }
  }, []);

  return (
    <div className="py-3 px-3">
      {/* Header — user on right, AI on left */}
      <div className={`flex items-center gap-1.5 mb-0.5 ${isUser ? 'flex-row-reverse' : ''}`}>
        <span className="text-[11px] font-semibold text-theme-text-muted/50">
          {isUser ? t('ai.message.you') : 'Copilot'}
        </span>
        {message.context && !isUser && (
          <span className="text-[10px] text-theme-text-muted/40 font-medium">
            (used context)
          </span>
        )}
        <span className={`text-[10px] text-theme-text-muted/25 font-mono shrink-0 ${isUser ? 'mr-auto' : 'ml-auto'}`}>
          {new Date(message.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
        </span>
      </div>

      {/* Content — user messages get prominent bubble with accent color */}
      <div className={`mt-1 ${isUser ? 'bg-theme-accent/10 border border-theme-accent/30 px-3 py-2 rounded-md' : ''}`}>
        {/* Thinking Block */}
        {!isUser && message.thinkingContent && (
          <ThinkingBlock
            content={message.thinkingContent}
            isStreaming={message.isThinkingStreaming}
          />
        )}

        <div
          ref={contentRef}
          className="md-content selection:bg-theme-accent/20"
          onClick={handleClick}
          dangerouslySetInnerHTML={{ __html: renderedHtml }}
        />
        {message.isStreaming && (
          <span className="inline-block w-1.5 h-4 ml-0.5 bg-theme-accent/60 animate-pulse align-middle" />
        )}

        {/* Regenerate Button */}
        {!isUser && isLastAssistant && !message.isStreaming && onRegenerate && (
          <div className="mt-1.5">
            <button
              onClick={onRegenerate}
              disabled={isRegenerating}
              className="flex items-center gap-1 text-[11px] text-theme-text-muted/40 
                hover:text-theme-text-muted py-0.5 px-1.5
                hover:bg-theme-border/10 disabled:opacity-50 disabled:cursor-not-allowed"
              title={t('ai.message.regenerate')}
            >
              <RotateCcw className={`w-3 h-3 ${isRegenerating ? 'animate-spin' : ''}`} />
              <span>{isRegenerating ? t('ai.message.regenerating') : t('ai.message.regenerate')}</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}, arePropsEqual);
