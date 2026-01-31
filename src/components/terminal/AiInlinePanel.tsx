import React, { useState, useEffect, useRef, useCallback } from 'react';
import { Sparkles, X, Send, Loader2, Copy, Check, Play, CornerDownLeft, AlertCircle } from 'lucide-react';
import { Button } from '../ui/button';
import { useSettingsStore } from '../../store/settingsStore';
import { api } from '../../lib/api';
import { useTranslation } from 'react-i18next';

// Context source for AI
export type ContextSource = 'selection' | 'visible' | 'none';

interface AiInlinePanelProps {
  isOpen: boolean;
  onClose: () => void;
  // Context providers from TerminalView
  getSelection: () => string;
  getVisibleBuffer: () => string;
  // Action handlers
  onInsert: (text: string) => void;
  onExecute: (command: string) => void;
}

interface Message {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

export const AiInlinePanel: React.FC<AiInlinePanelProps> = ({
  isOpen,
  onClose,
  getSelection,
  getVisibleBuffer,
  onInsert,
  onExecute,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettingsStore();
  const { ai: aiSettings } = settings;

  // State
  const [prompt, setPrompt] = useState('');
  const [contextSource, setContextSource] = useState<ContextSource>('selection');
  const [isLoading, setIsLoading] = useState(false);
  const [response, setResponse] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [hasApiKey, setHasApiKey] = useState(false);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const responseRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Check API key on open
  useEffect(() => {
    if (isOpen) {
      console.log('[AiInlinePanel] Panel opened, checking API key...');
      api.getAiApiKey()
        .then((key) => {
          console.log('[AiInlinePanel] API key check result:', key ? `found (length: ${key.length})` : 'not found');
          setHasApiKey(!!key);
        })
        .catch((e) => {
          console.error('[AiInlinePanel] Failed to check API key:', e);
          setHasApiKey(false);
        });
    }
  }, [isOpen]);

  useEffect(() => {
    const handleKeyUpdated = () => {
      console.log('[AiInlinePanel] API key updated event received');
      api.getAiApiKey()
        .then((key) => {
          console.log('[AiInlinePanel] API key after update:', key ? `found (length: ${key.length})` : 'not found');
          setHasApiKey(!!key);
        })
        .catch((e) => {
          console.error('[AiInlinePanel] Failed to check API key after update:', e);
          setHasApiKey(false);
        });
    };
    window.addEventListener('ai-api-key-updated', handleKeyUpdated);
    return () => window.removeEventListener('ai-api-key-updated', handleKeyUpdated);
  }, []);

  // Focus input when opened
  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  // Auto-scroll response
  useEffect(() => {
    if (responseRef.current && response) {
      responseRef.current.scrollTop = responseRef.current.scrollHeight;
    }
  }, [response]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!isOpen) return;

      // Esc to close
      if (e.key === 'Escape') {
        handleClose();
        e.preventDefault();
        return;
      }

      // Cmd/Ctrl + Enter to send
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        handleSend();
        e.preventDefault();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, prompt, isLoading]);

  const handleClose = useCallback(() => {
    // Abort any ongoing request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setPrompt('');
    setResponse('');
    setError(null);
    setIsLoading(false);
    onClose();
  }, [onClose]);

  // Get context based on source
  const getContext = useCallback((): string => {
    let context = '';

    if (contextSource === 'selection') {
      context = getSelection();
      // Fall back to visible buffer if no selection
      if (!context.trim()) {
        context = getVisibleBuffer();
      }
    } else if (contextSource === 'visible') {
      context = getVisibleBuffer();
    }

    // Truncate to max chars
    if (context.length > aiSettings.contextMaxChars) {
      context = context.slice(-aiSettings.contextMaxChars);
    }

    return context;
  }, [contextSource, getSelection, getVisibleBuffer, aiSettings.contextMaxChars]);

  // Estimate tokens (Method A: ~4 chars per token)
  const estimateTokens = (text: string): number => {
    return Math.ceil(text.length / 4);
  };

  const handleSend = async () => {
    if (!prompt.trim() || isLoading) return;

    setIsLoading(true);
    setError(null);
    setResponse('');

    try {
      // Get API key
      const apiKey = await api.getAiApiKey();
      if (!apiKey) {
        throw new Error('API key not found. Please configure it in Settings > AI.');
      }

      // Build messages
      const context = getContext();
      const messages: Message[] = [];

      // System prompt
      messages.push({
        role: 'system',
        content: `You are a helpful terminal assistant. You help users with shell commands, scripts, and terminal operations. Be concise and direct. When providing commands, just give the command without markdown code blocks unless the user is asking for an explanation.`
      });

      // Context as system message if available
      if (context.trim()) {
        messages.push({
          role: 'system',
          content: `Current terminal context:\n\`\`\`\n${context}\n\`\`\``
        });
      }

      // User prompt
      messages.push({
        role: 'user',
        content: prompt
      });

      // Create abort controller
      abortControllerRef.current = new AbortController();

      // Call API
      const result = await fetchChatCompletion({
        baseUrl: aiSettings.baseUrl,
        model: aiSettings.model,
        apiKey,
        messages,
        signal: abortControllerRef.current.signal,
        onChunk: (chunk) => {
          setResponse(prev => prev + chunk);
        }
      });

      if (!abortControllerRef.current?.signal.aborted) {
        setResponse(result);
      }
    } catch (e: unknown) {
      if (e instanceof Error && e.name === 'AbortError') {
        // Request was cancelled
        return;
      }
      const errorMessage = e instanceof Error ? e.message : String(e);
      setError(errorMessage);
    } finally {
      setIsLoading(false);
      abortControllerRef.current = null;
    }
  };

  const handleCopy = async () => {
    if (!response) return;
    await navigator.clipboard.writeText(response);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleInsert = () => {
    if (!response) return;
    onInsert(response);
    handleClose();
  };

  const handleExecute = () => {
    if (!response) return;
    // Extract first line as command (common pattern)
    const command = response.trim().split('\n')[0];
    onExecute(command);
    handleClose();
  };

  // Prevent terminal from stealing focus
  const handleKeyDown = (e: React.KeyboardEvent) => {
    e.stopPropagation();
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    e.stopPropagation();
  };

  if (!isOpen) return null;

  const contextPreview = getContext();
  const contextTokens = estimateTokens(contextPreview);

  return (
    <div
      className="absolute top-4 right-4 z-50 w-[420px] bg-theme-bg-panel border border-theme-border rounded-lg shadow-2xl"
      onKeyDown={handleKeyDown}
      onMouseDown={handleMouseDown}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-theme-border bg-theme-bg/50 rounded-t-lg">
        <div className="flex items-center gap-2">
          <Sparkles className="h-4 w-4 text-theme-accent" />
          <span className="text-sm font-medium text-theme-text">{t('terminal.ai.title')}</span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0 text-theme-text-muted hover:text-theme-text"
          onClick={handleClose}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* API Key Warning */}
      {!hasApiKey && (
        <div className="mx-3 mt-3 p-3 bg-yellow-500/10 border border-yellow-500/30 rounded-md">
          <div className="flex items-start gap-2 text-yellow-500/80">
            <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
            <div className="text-sm">
              <p className="font-medium">{t('terminal.ai.api_key_required')}</p>
              <p className="text-xs opacity-80 mt-1">
                {t('terminal.ai.api_key_hint')}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Context Source Selector */}
      <div className="px-3 pt-3 pb-2">
        <div className="flex items-center gap-2 text-xs text-theme-text-muted">
          <span>{t('terminal.ai.context')}</span>
          <div className="flex gap-1">
            {(['selection', 'visible', 'none'] as ContextSource[]).map((source) => (
              <button
                key={source}
                className={`px-2 py-1 rounded transition-colors ${contextSource === source
                    ? 'bg-theme-accent/10 text-theme-accent border border-theme-accent/30'
                    : 'bg-theme-bg text-theme-text-muted hover:text-theme-text border border-theme-border/50'
                  }`}
                onClick={() => setContextSource(source)}
              >
                {source === 'selection' ? t('terminal.ai.context_selection') : source === 'visible' ? t('terminal.ai.context_visible') : t('terminal.ai.context_none')}
              </button>
            ))}
          </div>
          {contextSource !== 'none' && contextPreview && (
            <span className="ml-auto text-theme-text-muted opacity-50">
              {t('terminal.ai.tokens_estimate', { count: contextTokens })}
            </span>
          )}
        </div>

        {/* Context Preview */}
        {contextSource !== 'none' && contextPreview && (
          <div className="mt-2 p-2 bg-theme-bg/50 border border-theme-border rounded text-xs font-mono text-theme-text-muted max-h-20 overflow-y-auto">
            <pre className="whitespace-pre-wrap break-all">
              {contextPreview.length > 200
                ? contextPreview.slice(0, 200) + '...'
                : contextPreview}
            </pre>
          </div>
        )}
      </div>

      {/* Prompt Input */}
      <div className="px-3 pb-2">
        <div className="relative">
          <textarea
            ref={inputRef}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder={t('terminal.ai.prompt_placeholder')}
            className="w-full h-20 px-3 py-2 pr-12 bg-theme-bg border border-theme-border rounded-md text-sm text-theme-text placeholder-theme-text-muted/50 resize-none focus:outline-none focus:border-theme-accent/50"
            disabled={isLoading}
          />
          <Button
            variant="ghost"
            size="sm"
            className="absolute right-2 bottom-2 h-7 w-7 p-0 text-theme-accent hover:text-theme-accent hover:bg-theme-accent/10"
            onClick={handleSend}
            disabled={!prompt.trim() || isLoading}
          >
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Send className="h-4 w-4" />
            )}
          </Button>
        </div>
        <div className="flex items-center justify-between mt-1 text-xs text-theme-text-muted">
          <span>
            <kbd className="px-1 py-0.5 bg-theme-bg border border-theme-border rounded text-[10px]">⌘</kbd>
            <span className="mx-0.5">+</span>
            <kbd className="px-1 py-0.5 bg-theme-bg border border-theme-border rounded text-[10px]">Enter</kbd>
            {' '}{t('terminal.ai.send_shortcut')}
          </span>
          <span>
            <kbd className="px-1 py-0.5 bg-theme-bg border border-theme-border rounded text-[10px]">Esc</kbd>
            {' '}{t('terminal.ai.close_shortcut')}
          </span>
        </div>
      </div>

      {/* Response Area */}
      {(response || error) && (
        <div className="border-t border-theme-border">
          {error ? (
            <div className="p-3 text-sm text-red-500 bg-red-500/10">
              <div className="flex items-start gap-2">
                <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
                <span>{error}</span>
              </div>
            </div>
          ) : (
            <>
              <div
                ref={responseRef}
                className="p-3 max-h-48 overflow-y-auto text-sm text-theme-text font-mono whitespace-pre-wrap"
              >
                {response}
                {isLoading && (
                  <span className="inline-block w-2 h-4 ml-1 bg-theme-accent animate-pulse" />
                )}
              </div>

              {/* Action Buttons */}
              {!isLoading && response && (
                <div className="flex items-center gap-2 px-3 py-2 border-t border-theme-border bg-theme-bg/50">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-xs text-theme-text-muted hover:text-theme-text"
                    onClick={handleCopy}
                  >
                    {copied ? (
                      <Check className="h-3 w-3 mr-1" />
                    ) : (
                      <Copy className="h-3 w-3 mr-1" />
                    )}
                    {copied ? t('terminal.ai.copied') : t('terminal.ai.copy')}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-xs text-theme-text-muted hover:text-theme-text"
                    onClick={handleInsert}
                  >
                    <CornerDownLeft className="h-3 w-3 mr-1" />
                    {t('terminal.ai.insert')}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-xs text-theme-accent hover:text-theme-accent hover:bg-theme-accent/10"
                    onClick={handleExecute}
                  >
                    <Play className="h-3 w-3 mr-1" />
                    {t('terminal.ai.execute')}
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
};

// ============ OpenAI-compatible API Call ============

interface ChatCompletionOptions {
  baseUrl: string;
  model: string;
  apiKey: string;
  messages: Message[];
  signal?: AbortSignal;
  onChunk?: (chunk: string) => void;
}

async function fetchChatCompletion(options: ChatCompletionOptions): Promise<string> {
  const { baseUrl, model, apiKey, messages, signal, onChunk } = options;

  // Ensure baseUrl ends without trailing slash
  const cleanBaseUrl = baseUrl.replace(/\/+$/, '');
  const url = `${cleanBaseUrl}/chat/completions`;

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model,
      messages,
      stream: true,
    }),
    signal,
  });

  if (!response.ok) {
    const errorText = await response.text();
    let errorMessage = `API error: ${response.status}`;
    try {
      const errorJson = JSON.parse(errorText);
      errorMessage = errorJson.error?.message || errorJson.message || errorMessage;
    } catch {
      if (errorText) {
        errorMessage = errorText.slice(0, 200);
      }
    }
    throw new Error(errorMessage);
  }

  // Handle streaming response
  const reader = response.body?.getReader();
  if (!reader) {
    throw new Error('No response body');
  }

  const decoder = new TextDecoder();
  let fullContent = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      const chunk = decoder.decode(value, { stream: true });
      const lines = chunk.split('\n');

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          const data = line.slice(6);
          if (data === '[DONE]') continue;

          try {
            const json = JSON.parse(data);
            const content = json.choices?.[0]?.delta?.content || '';
            if (content) {
              fullContent += content;
              onChunk?.(content);
            }
          } catch {
            // Ignore parse errors for partial chunks
          }
        }
      }
    }
  } finally {
    reader.releaseLock();
  }

  return fullContent;
}
