import { memo, useMemo } from 'react';
import { User, Bot, Copy, Check, Play } from 'lucide-react';
import { useState } from 'react';
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
        className="px-1.5 py-0.5 rounded bg-zinc-700/50 text-orange-300 text-sm font-mono"
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
  const [copied, setCopied] = useState(false);
  const [inserted, setInserted] = useState(false);

  const canInsert = isShellLanguage(language);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleInsert = async () => {
    // Emit event to insert command into active terminal
    await emit('ai-insert-command', { command: code });
    setInserted(true);
    setTimeout(() => setInserted(false), 2000);
  };

  return (
    <div className="my-2 rounded-lg overflow-hidden bg-zinc-900/80 border border-zinc-700/50">
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-800/50 border-b border-zinc-700/50">
        <span className="text-xs text-zinc-400 font-mono">
          {language || 'shell'}
        </span>
        <div className="flex items-center gap-1">
          {canInsert && (
            <button
              onClick={handleInsert}
              className="p-1 rounded hover:bg-zinc-700/50 text-zinc-400 hover:text-green-400 transition-colors"
              title="Insert to terminal"
            >
              {inserted ? (
                <Check className="w-3.5 h-3.5 text-green-400" />
              ) : (
                <Play className="w-3.5 h-3.5" />
              )}
            </button>
          )}
          <button
            onClick={handleCopy}
            className="p-1 rounded hover:bg-zinc-700/50 text-zinc-400 hover:text-zinc-200 transition-colors"
            title="Copy code"
          >
            {copied ? (
              <Check className="w-3.5 h-3.5 text-green-400" />
            ) : (
              <Copy className="w-3.5 h-3.5" />
            )}
          </button>
        </div>
      </div>
      <pre className="p-3 overflow-x-auto text-sm">
        <code className="text-zinc-200 font-mono">{code}</code>
      </pre>
    </div>
  );
}

export const ChatMessage = memo(function ChatMessage({ message }: ChatMessageProps) {
  const isUser = message.role === 'user';

  const renderedContent = useMemo(
    () => renderContent(message.content),
    [message.content]
  );

  return (
    <div
      className={`flex gap-3 px-4 py-3 ${
        isUser ? 'bg-transparent' : 'bg-zinc-800/30'
      }`}
    >
      {/* Avatar */}
      <div
        className={`flex-shrink-0 w-7 h-7 rounded-full flex items-center justify-center ${
          isUser ? 'bg-blue-600' : 'bg-orange-600'
        }`}
      >
        {isUser ? (
          <User className="w-4 h-4 text-white" />
        ) : (
          <Bot className="w-4 h-4 text-white" />
        )}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="text-xs text-zinc-500 mb-1">
          {isUser ? 'You' : 'Assistant'}
        </div>
        <div className="text-sm text-zinc-200 leading-relaxed">
          {renderedContent}
          {message.isStreaming && (
            <span className="inline-block w-2 h-4 ml-1 bg-orange-500 animate-pulse" />
          )}
        </div>
        {message.context && (
          <div className="mt-2 text-xs text-zinc-500 italic">
            Context from terminal attached
          </div>
        )}
      </div>
    </div>
  );
});
