import { memo, useMemo, useEffect, useRef, useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { RotateCcw, Pencil, Trash2, Check, X, ChevronDown, ChevronRight, ChevronLeft, Archive } from 'lucide-react';
import { emit } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { AiChatMessage } from '../../types';
import { renderMarkdown, markdownStyles, renderMathInElement } from '../../lib/markdownRenderer';
import { useMermaid } from '../../hooks/useMermaid';
import { ThinkingBlock } from './ThinkingBlock';
import { ToolCallBlock } from './ToolCallBlock';

interface ChatMessageProps {
  message: AiChatMessage;
  /** Whether this is the last assistant message (for regenerate button) */
  isLastAssistant?: boolean;
  /** Callback to regenerate the response */
  onRegenerate?: () => void;
  /** Whether regeneration is in progress */
  isRegenerating?: boolean;
  /** Callback to edit and resend a user message */
  onEdit?: (messageId: string, newContent: string) => void;
  /** Callback to delete a message */
  onDelete?: (messageId: string) => void;
  /** Callback to switch branch at a branch-point message */
  onSwitchBranch?: (messageId: string, branchIndex: number) => void;
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
    prev.message.toolCalls === next.message.toolCalls &&
    prev.message.branches?.activeIndex === next.message.branches?.activeIndex &&
    prev.message.branches?.total === next.message.branches?.total &&
    prev.isLastAssistant === next.isLastAssistant &&
    prev.isRegenerating === next.isRegenerating
  );
}

export const ChatMessage = memo(function ChatMessage({
  message,
  isLastAssistant = false,
  onRegenerate,
  isRegenerating = false,
  onEdit,
  onDelete,
  onSwitchBranch,
}: ChatMessageProps) {
  const { t } = useTranslation();
  const isUser = message.role === 'user';
  const contentRef = useRef<HTMLDivElement>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState('');
  const editTextareaRef = useRef<HTMLTextAreaElement>(null);

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

  // ═══════════════════════════════════════════════════════════════════════
  // Compaction Anchor — special rendering for compacted message summaries
  // ═══════════════════════════════════════════════════════════════════════
  const [anchorExpanded, setAnchorExpanded] = useState(false);

  if (message.metadata?.type === 'compaction-anchor') {
    const originalMessages = message.metadata.originalMessages || [];
    const originalCount = message.metadata.originalCount ?? 0;

    return (
      <div className="py-2 px-3">
        <div className="border border-dashed border-theme-border/40 rounded-md overflow-hidden">
          {/* Collapsed header */}
          <button
            onClick={() => setAnchorExpanded(!anchorExpanded)}
            className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-theme-bg-hover/30 transition-colors"
          >
            <Archive className="w-3.5 h-3.5 text-theme-text-muted/50 shrink-0" />
            <span className="text-[11px] text-theme-text-muted/60 font-medium flex-1">
              {t('ai.context.compacted_messages', { count: originalCount })}
            </span>
            {anchorExpanded
              ? <ChevronDown className="w-3 h-3 text-theme-text-muted/40" />
              : <ChevronRight className="w-3 h-3 text-theme-text-muted/40" />
            }
          </button>

          {/* Summary content (always visible below header) */}
          <div className="px-3 pb-2 text-[12px] text-theme-text-muted/70 leading-relaxed">
            {message.content}
          </div>

          {/* Expanded: original messages snapshot (read-only) */}
          {anchorExpanded && originalMessages.length > 0 && (
            <div className="border-t border-dashed border-theme-border/30 mx-2 mb-2">
              <div className="px-2 py-1.5 text-[10px] text-theme-text-muted/40 font-medium">
                {t('ai.context.view_original')} ({originalMessages.length})
              </div>
              <div className="max-h-[300px] overflow-y-auto space-y-1 px-2 pb-2">
                {originalMessages.map((orig) => (
                  <div
                    key={orig.id}
                    className="text-[11px] text-theme-text-muted/50 bg-theme-bg/30 rounded px-2 py-1"
                  >
                    <span className="font-semibold text-theme-text-muted/40 mr-1.5">
                      {orig.role === 'user' ? t('ai.message.you') : 'AI'}:
                    </span>
                    <span className="whitespace-pre-wrap break-words">
                      {orig.content.length > 500 ? orig.content.slice(0, 500) + '…' : orig.content}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="py-3 px-3 group/msg">
      {/* Header — user on right, AI on left */}
      <div className={`flex items-center gap-1.5 mb-0.5 ${isUser ? 'flex-row-reverse' : ''}`}>
        <span className="text-[11px] font-semibold text-theme-text-muted/50">
          {isUser ? t('ai.message.you') : 'Copilot'}
        </span>
        {message.context && !isUser && (
          <span className="text-[10px] text-theme-text-muted/40 font-medium">
            ({t('ai.message.used_context')})
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

        {/* Tool Calls Block */}
        {!isUser && message.toolCalls && message.toolCalls.length > 0 && (
          <ToolCallBlock
            toolCalls={message.toolCalls}
            totalRounds={message.toolCalls.length}
          />
        )}

        {/* Edit mode for user messages */}
        {isUser && isEditing ? (
          <div className="flex flex-col gap-1.5">
            <textarea
              ref={editTextareaRef}
              value={editContent}
              onChange={(e) => setEditContent(e.target.value)}
              className="w-full bg-theme-bg/50 border border-theme-accent/40 text-[13px] text-theme-text px-2 py-1.5 resize-none focus:outline-none focus:border-theme-accent/60 min-h-[60px]"
              rows={3}
              onKeyDown={(e) => {
                if (e.nativeEvent.isComposing || e.keyCode === 229) return;
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  const trimmed = editContent.trim();
                  if (trimmed && onEdit) {
                    onEdit(message.id, trimmed);
                    setIsEditing(false);
                  }
                } else if (e.key === 'Escape') {
                  setIsEditing(false);
                }
              }}
            />
            <div className="flex items-center gap-1 justify-end">
              <button
                onClick={() => setIsEditing(false)}
                className="flex items-center gap-0.5 px-1.5 py-0.5 text-[10px] text-theme-text-muted hover:text-theme-text hover:bg-theme-border/10"
              >
                <X className="w-3 h-3" />
                {t('ai.message.cancel')}
              </button>
              <button
                onClick={() => {
                  const trimmed = editContent.trim();
                  if (trimmed && onEdit) {
                    onEdit(message.id, trimmed);
                    setIsEditing(false);
                  }
                }}
                disabled={!editContent.trim()}
                className="flex items-center gap-0.5 px-1.5 py-0.5 text-[10px] text-theme-accent hover:bg-theme-accent/10 disabled:opacity-30"
              >
                <Check className="w-3 h-3" />
                {t('ai.message.save_and_resend')}
              </button>
            </div>
          </div>
        ) : (
          <>
            <div
              ref={contentRef}
              className="md-content selection:bg-theme-accent/20"
              onClick={handleClick}
              dangerouslySetInnerHTML={{ __html: renderedHtml }}
            />
            {message.isStreaming && (
              <span className="inline-block w-1.5 h-4 ml-0.5 bg-theme-accent/60 animate-pulse align-middle" />
            )}
          </>
        )}

        {/* Branch Navigator — shown on user messages with multiple branches */}
        {isUser && message.branches && message.branches.total > 1 && !isEditing && (
          <div className="mt-1 flex items-center gap-1 z-0">
            <button
              onClick={() => onSwitchBranch?.(message.id, message.branches!.activeIndex - 1)}
              disabled={message.branches.activeIndex <= 0}
              className="p-0.5 text-theme-text-muted/40 hover:text-theme-text-muted disabled:opacity-20 disabled:cursor-not-allowed transition-colors"
              aria-label={t('ai.chat.branch_prev')}
            >
              <ChevronLeft className="w-3 h-3" />
            </button>
            <span className="text-[10px] text-theme-text-muted/50 font-mono tabular-nums select-none min-w-[28px] text-center">
              {message.branches.activeIndex + 1}/{message.branches.total}
            </span>
            <button
              onClick={() => onSwitchBranch?.(message.id, message.branches!.activeIndex + 1)}
              disabled={message.branches.activeIndex >= message.branches.total - 1}
              className="p-0.5 text-theme-text-muted/40 hover:text-theme-text-muted disabled:opacity-20 disabled:cursor-not-allowed transition-colors"
              aria-label={t('ai.chat.branch_next')}
            >
              <ChevronRight className="w-3 h-3" />
            </button>
          </div>
        )}

        {/* Action Buttons */}
        {!message.isStreaming && !isEditing && (
          <div className="mt-1.5 flex items-center gap-0.5">
            {/* Edit button — user messages only */}
            {isUser && onEdit && (
              <button
                onClick={() => {
                  setEditContent(message.content);
                  setIsEditing(true);
                  setTimeout(() => editTextareaRef.current?.focus(), 50);
                }}
                className="flex items-center gap-1 text-[11px] text-theme-text-muted/40 
                  hover:text-theme-text-muted py-0.5 px-1.5
                  hover:bg-theme-border/10 opacity-0 group-hover/msg:opacity-100 transition-opacity"
                title={t('ai.message.edit')}
              >
                <Pencil className="w-3 h-3" />
                <span>{t('ai.message.edit')}</span>
              </button>
            )}

            {/* Regenerate Button — last assistant only */}
            {!isUser && isLastAssistant && onRegenerate && (
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
            )}

            {/* Delete button */}
            {onDelete && (
              <button
                onClick={() => onDelete(message.id)}
                className="flex items-center gap-1 text-[11px] text-theme-text-muted/40 
                  hover:text-red-500 py-0.5 px-1.5
                  hover:bg-red-500/5 opacity-0 group-hover/msg:opacity-100 transition-opacity"
                title={t('ai.message.delete')}
              >
                <Trash2 className="w-3 h-3" />
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}, arePropsEqual);
