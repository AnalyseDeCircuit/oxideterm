import { memo, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { User, Bot, Copy, Check, Terminal } from 'lucide-react';
import { emit } from '@tauri-apps/api/event';
import type { AiChatMessage } from '../../types';

interface ChatMessageProps {
  message: AiChatMessage;
}

// Check if language looks like a shell command
function isShellLanguage(language: string): boolean {
  const shellLangs = ['bash', 'sh', 'zsh', 'shell', 'powershell', 'ps1', 'cmd', 'terminal', 'console', ''];
  return shellLangs.includes(language.toLowerCase());
}

// Simple markdown-like rendering for code blocks
function renderContent(content: string): React.ReactNode {
  const parts: React.ReactNode[] = [];
  let key = 0;

  // Split by code blocks
  const codeBlockRegex = /```(\w+)?\n?([\s\S]*?)```/g;
  let lastIndex = 0;
  let match;

  while ((match = codeBlockRegex.exec(content)) !== null) {
    // Text before code block
    if (match.index > lastIndex) {
      const text = content.slice(lastIndex, match.index);
      parts.push(<TextContent key={key++} text={text} />);
    }

    // Code block
    const language = match[1] || '';
    const code = match[2].trim();
    parts.push(
      <CodeBlock key={key++} language={language} code={code} />
    );

    lastIndex = match.index + match[0].length;
  }

  // Remaining text after last code block
  if (lastIndex < content.length) {
    const text = content.slice(lastIndex);
    parts.push(<TextContent key={key++} text={text} />);
  }

  return parts.length > 0 ? parts : <TextContent text={content} />;
}

// Text content with inline code support
function TextContent({ text }: { text: string }) {
  // Handle inline code
  const inlineCodeRegex = /`([^`]+)`/g;
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  let match;
  let key = 0;

  while ((match = inlineCodeRegex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(
        <span key={key++}>
          {text.slice(lastIndex, match.index).split('\n').map((line, i, arr) => (
            <span key={i}>
              {line}
              {i < arr.length - 1 && <br />}
            </span>
          ))}
        </span>
      );
    }
    parts.push(
      <code
        key={key++}
        className="px-1.5 py-0.5 rounded bg-theme-bg-panel border border-theme-border/50 text-theme-accent text-xs font-mono"
      >
        {match[1]}
      </code>
    );
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < text.length) {
    parts.push(
      <span key={key++}>
        {text.slice(lastIndex).split('\n').map((line, i, arr) => (
          <span key={i}>
            {line}
            {i < arr.length - 1 && <br />}
          </span>
        ))}
      </span>
    );
  }

  return <span className="whitespace-pre-wrap">{parts.length > 0 ? parts : text}</span>;
}

// Code block component
function CodeBlock({ language, code }: { language: string; code: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const canInsert = isShellLanguage(language);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleInsert = async () => {
    // Emit event to insert command into active terminal
    await emit('ai-insert-command', { command: code });
  };

  return (
    <div className="my-6 rounded-md overflow-hidden bg-[var(--theme-bg-darker,#09090b)] border border-theme-border/40 shadow-sm">
      <div className="flex items-center justify-between px-3 py-1.5 bg-theme-bg-panel/40 border-b border-theme-border/20">
        <span className="text-[9px] text-theme-text-muted font-mono uppercase tracking-[0.2em] opacity-70">
          {language || 'shell'}
        </span>
        <div className="flex items-center gap-1.5">
          {canInsert && (
            <button
              onClick={handleInsert}
              className="p-1 px-2 rounded-md hover:bg-theme-accent/20 text-theme-accent transition-all flex items-center gap-1"
              title={t('ai.message.insert_to_terminal')}
            >
              <Terminal className="w-3 h-3" />
              <span className="text-[9px] font-bold tracking-tighter">RUN</span>
            </button>
          )}
          <button
            onClick={handleCopy}
            className="p-1 px-1.5 rounded-md hover:bg-theme-text/5 text-theme-text-muted hover:text-theme-text transition-all"
            title={t('ai.message.copy_code')}
          >
            {copied ? (
              <Check className="w-3.5 h-3.5 text-green-500" />
            ) : (
              <Copy className="w-3.5 h-3.5" />
            )}
          </button>
        </div>
      </div>
      <pre className="p-4 overflow-x-auto text-[13px] leading-[1.7] font-mono">
        <code className="text-theme-text block">{code}</code>
      </pre>
    </div>
  );
}

export const ChatMessage = memo(function ChatMessage({ message }: ChatMessageProps) {
  const { t } = useTranslation();
  const isUser = message.role === 'user';

  const renderedContent = useMemo(
    () => renderContent(message.content),
    [message.content]
  );

  return (
    <div
      className={`group flex gap-5 px-5 py-8 transition-colors ${isUser ? 'bg-theme-bg' : 'bg-transparent'
        }`}
    >
      {/* Avatar - More subtle and integrated */}
      <div className="flex-shrink-0 pt-1">
        <div
          className={`w-7 h-7 rounded-md flex items-center justify-center border transition-all ${isUser
            ? 'bg-theme-bg/50 border-theme-border text-theme-text-muted'
            : 'bg-theme-accent/5 border-theme-accent/20 text-theme-accent'
            }`}
        >
          {isUser ? (
            <User className="w-3.5 h-3.5" />
          ) : (
            <Bot className="w-3.5 h-3.5" />
          )}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-3 mb-2.5">
          <span className={`text-[11px] font-bold tracking-[0.05em] uppercase ${isUser ? 'text-theme-text opacity-40' : 'text-theme-accent opacity-80'}`}>
            {isUser ? t('ai.message.you') : t('ai.message.assistant')}
          </span>
          {message.context && !isUser && (
            <span className="text-[9px] text-theme-text-muted px-1.5 py-0.5 rounded bg-theme-accent/10 border border-theme-accent/10 font-medium">
              CONTEXT ATTACHED
            </span>
          )}
        </div>

        <div className="text-[14px] text-theme-text leading-[1.8] font-normal prose prose-invert prose-p:my-2 prose-headings:mb-3 prose-headings:mt-6">
          {renderedContent}
          {message.isStreaming && (
            <span className="inline-block w-1 h-4 ml-1.5 bg-theme-accent animate-pulse align-middle" />
          )}
        </div>
      </div>
    </div>
  );
});
