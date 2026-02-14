import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { Sparkles, X, Check, AlertCircle, CornerDownLeft, Play, Copy, RotateCcw } from 'lucide-react';
import { useSettingsStore } from '../../store/settingsStore';
import { useAppStore } from '../../store/appStore';
import { api } from '../../lib/api';
import { useTranslation } from 'react-i18next';
import { platform } from '../../lib/platform';
import { getProvider } from '../../lib/ai/providerRegistry';
import { ModelSelector } from '../ai/ModelSelector';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface CursorPosition {
  x: number;           // Cursor column (0-based)
  y: number;           // Cursor row relative to viewport (0-based)
  absoluteY: number;   // Absolute cursor row in buffer
  lineHeight: number;  // Pixel height per line
  charWidth: number;   // Pixel width per character
  containerRect: DOMRect;  // Terminal container bounding rect
}

interface AiInlinePanelProps {
  isOpen: boolean;
  onClose: () => void;
  // Context providers from TerminalView
  getSelection: () => string;
  getVisibleBuffer: () => string;
  // Action handlers
  onInsert: (text: string) => void;
  onExecute: (command: string) => void;
  // Cursor position for VS Code-style positioning
  cursorPosition: CursorPosition | null;
  // Session/connection context (for SSH remote env detection)
  sessionId?: string;
  terminalType?: 'terminal' | 'local_terminal';
}

interface Message {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Get OS Name (for local terminals)
// ═══════════════════════════════════════════════════════════════════════════

function getLocalOSName(): string {
  if (platform.isMac) return 'macOS';
  if (platform.isWindows) return 'Windows';
  if (platform.isLinux) return 'Linux';
  return 'Unknown';
}

/**
 * Build OS context string for system prompt.
 * Returns different strings for local vs SSH terminals.
 */
function buildOSContext(
  terminalType: 'terminal' | 'local_terminal' | undefined,
  sessionId: string | undefined,
  connections: Map<string, import('../../types').SshConnectionInfo>,
  sessions: Map<string, import('../../types').SessionInfo>
): string {
  const localOS = getLocalOSName();
  
  // Local terminal: just return local OS
  if (!terminalType || terminalType === 'local_terminal' || !sessionId) {
    return `Local terminal on ${localOS}`;
  }
  
  // SSH terminal: try to get remote env
  const session = sessions.get(sessionId);
  if (!session?.connectionId) {
    return `SSH terminal (remote OS unknown, local: ${localOS})`;
  }
  
  const conn = connections.get(session.connectionId);
  if (!conn) {
    return `SSH terminal (remote OS unknown, local: ${localOS})`;
  }
  
  const remoteEnv = conn.remoteEnv;
  const hostInfo = `${conn.username}@${conn.host}`;
  
  if (remoteEnv) {
    // Full detected environment
    const parts = [`Remote: ${remoteEnv.osType}`];
    if (remoteEnv.osVersion) parts[0] += ` (${remoteEnv.osVersion})`;
    if (remoteEnv.arch) parts.push(`arch: ${remoteEnv.arch}`);
    if (remoteEnv.shell) parts.push(`shell: ${remoteEnv.shell}`);
    parts.push(`host: ${hostInfo}`);
    parts.push(`local: ${localOS}`);
    return `SSH terminal — ${parts.join(', ')}`;
  } else if (remoteEnv === undefined) {
    // Detection in progress
    return `SSH terminal to ${hostInfo} (remote OS detecting..., local: ${localOS})`;
  } else {
    // Detection failed
    return `SSH terminal to ${hostInfo} (remote OS unknown, local: ${localOS})`;
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Component
// ═══════════════════════════════════════════════════════════════════════════

export const AiInlinePanel: React.FC<AiInlinePanelProps> = ({
  isOpen,
  onClose,
  getSelection,
  getVisibleBuffer: _getVisibleBuffer,  // Reserved for future use
  onInsert,
  onExecute,
  cursorPosition,
  sessionId,
  terminalType,
}) => {
  // Suppress unused warning - kept for API compatibility
  void _getVisibleBuffer;
  
  const { t } = useTranslation();
  const { settings } = useSettingsStore();
  const { ai: aiSettings } = settings;
  const createTab = useAppStore((state) => state.createTab);
  const sessions = useAppStore((state) => state.sessions);
  const connections = useAppStore((state) => state.connections);

  // State
  const [prompt, setPrompt] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [response, setResponse] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [hasApiKey, setHasApiKey] = useState(false);
  const [hasSelection, setHasSelection] = useState(false);  // Track if selection exists on open

  const inputRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const copyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const selectionContextRef = useRef<string>('');  // Cache selection at open time

  // ═══════════════════════════════════════════════════════════════════════════
  // Effects
  // ═══════════════════════════════════════════════════════════════════════════

  // Check API key and selection on open
  useEffect(() => {
    if (isOpen) {
      // Check API key for active provider - no fallback to legacy key
      const activeProvider = aiSettings.providers.find(p => p.id === aiSettings.activeProviderId);
      if (activeProvider?.type === 'ollama') {
        setHasApiKey(true);
      } else if (activeProvider) {
        api.hasAiProviderApiKey(activeProvider.id)
          .then(has => setHasApiKey(!!has))
          .catch(() => setHasApiKey(false));
      } else {
        setHasApiKey(false);
      }
      
      // Capture selection at open time (freeze it)
      const selection = getSelection();
      selectionContextRef.current = selection;
      setHasSelection(!!selection.trim());
    }
  }, [isOpen, getSelection, aiSettings.activeProviderId]);

  // Listen for API key updates
  useEffect(() => {
    const handleKeyUpdated = () => {
      const activeProvider = aiSettings.providers.find(p => p.id === aiSettings.activeProviderId);
      if (activeProvider?.type === 'ollama') {
        setHasApiKey(true);
      } else if (activeProvider) {
        api.hasAiProviderApiKey(activeProvider.id)
          .then(has => setHasApiKey(!!has))
          .catch(() => setHasApiKey(false));
      } else {
        setHasApiKey(false);
      }
    };
    window.addEventListener('ai-api-key-updated', handleKeyUpdated);
    return () => window.removeEventListener('ai-api-key-updated', handleKeyUpdated);
  }, []);

  // Focus input when opened
  useEffect(() => {
    if (isOpen && inputRef.current) {
      // Small delay to ensure panel is rendered
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [isOpen]);

  // Reset state when closed
  useEffect(() => {
    if (!isOpen) {
      setPrompt('');
      setResponse('');
      setError(null);
      setIsLoading(false);
      setHasSelection(false);
      selectionContextRef.current = '';
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
        abortControllerRef.current = null;
      }
    }
  }, [isOpen]);

  // ═══════════════════════════════════════════════════════════════════════════
  // Position Calculation (VS Code Style)
  // ═══════════════════════════════════════════════════════════════════════════

  const panelStyle = useMemo((): React.CSSProperties => {
    if (!cursorPosition) {
      // Fallback: center horizontally, near top
      return {
        left: '50%',
        transform: 'translateX(-50%)',
        top: '48px',
      };
    }

    const { y, lineHeight, containerRect } = cursorPosition;
    const PANEL_WIDTH = 520;
    const PANEL_MARGIN = 12;
    const VERTICAL_OFFSET = 4; // Gap between cursor line and panel

    // Calculate top position: below the cursor line
    let top = (y + 1) * lineHeight + VERTICAL_OFFSET;

    // Calculate horizontal position: center with boundary checks
    let left = (containerRect.width - PANEL_WIDTH) / 2;
    
    // Ensure panel stays within container bounds
    if (left < PANEL_MARGIN) {
      left = PANEL_MARGIN;
    } else if (left + PANEL_WIDTH > containerRect.width - PANEL_MARGIN) {
      left = containerRect.width - PANEL_WIDTH - PANEL_MARGIN;
    }

    // If panel would go below viewport, position above cursor
    const panelEstimatedHeight = response ? 160 : 56; // Expanded vs collapsed height
    if (top + panelEstimatedHeight > containerRect.height - PANEL_MARGIN) {
      top = y * lineHeight - panelEstimatedHeight - VERTICAL_OFFSET;
      if (top < PANEL_MARGIN) {
        top = PANEL_MARGIN;
      }
    }

    return {
      left: `${left}px`,
      top: `${top}px`,
    };
  }, [cursorPosition, response]);

  // ═══════════════════════════════════════════════════════════════════════════
  // Handlers
  // ═══════════════════════════════════════════════════════════════════════════

  const handleClose = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    onClose();
  }, [onClose]);

  const handleOpenSettings = useCallback(() => {
    createTab('settings');
  }, [createTab]);

  // Get selection context (cached at open time)
  const getSelectionContext = useCallback((): string => {
    return selectionContextRef.current;
  }, []);

  const handleSend = useCallback(async () => {
    if (!prompt.trim() || isLoading) return;

    setIsLoading(true);
    setError(null);
    setResponse('');

    try {
      // Resolve active provider
      const activeProvider = aiSettings.providers.find(p => p.id === aiSettings.activeProviderId);
      if (!activeProvider) {
        throw new Error('No AI provider configured');
      }

      // Resolve API key
      let apiKey = '';
      if (activeProvider.type !== 'ollama') {
        try {
          apiKey = await api.getAiProviderApiKey(activeProvider.id) || '';
        } catch { /* ignore */ }
        if (!apiKey) {
          throw new Error(t('terminal.ai.api_key_required'));
        }
      }

      // 1. Get OS metadata (aware of SSH remote env)
      const osContext = buildOSContext(terminalType, sessionId, connections, sessions);
      
      // 2. Get selection context (frozen at open time)
      const selectionContext = getSelectionContext();
      
      // 3. Build messages with structured prompt template
      const messages: Message[] = [];
      
      // System prompt with OS context
      messages.push({
        role: 'system',
        content: `You are an expert terminal assistant. Environment: ${osContext}. Respond ONLY with the command or code itself unless asked for explanation.`
      });

      // User message with structured context
      let userContent = '';
      
      if (selectionContext.trim()) {
        // Truncate selection if too long
        let truncatedSelection = selectionContext;
        if (truncatedSelection.length > aiSettings.contextMaxChars) {
          truncatedSelection = truncatedSelection.slice(-aiSettings.contextMaxChars);
        }
        userContent += `### Context (Selected Text):\n${truncatedSelection}\n\n`;
      }
      
      userContent += `### Question/Instruction:\n${prompt}`;
      
      messages.push({ role: 'user', content: userContent });

      abortControllerRef.current = new AbortController();

      const provider = getProvider(activeProvider.type);
      const model = aiSettings.activeModel || activeProvider.defaultModel;
      const config = {
        baseUrl: activeProvider.baseUrl,
        model,
        apiKey,
      };

      let fullContent = '';
      for await (const event of provider.streamCompletion(config, messages, abortControllerRef.current.signal)) {
        if (event.type === 'content' && event.content) {
          fullContent += event.content;
          setResponse(prev => prev + event.content);
        } else if (event.type === 'error') {
          throw new Error(event.message);
        } else if (event.type === 'done') {
          break;
        }
      }
      void fullContent; // Used for potential future logging
    } catch (e: unknown) {
      if (e instanceof Error && e.name === 'AbortError') return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
      abortControllerRef.current = null;
    }
  }, [prompt, isLoading, getSelectionContext, aiSettings, t]);

  // Insert without executing (Tab)
  const handleInsert = useCallback(() => {
    if (!response.trim()) return;
    // Extract command (first non-empty line, strip any backticks)
    const command = extractCommand(response);
    onInsert(command);
    handleClose();
  }, [response, onInsert, handleClose]);

  // Execute command (Enter when response ready)
  const handleExecute = useCallback(() => {
    if (!response.trim()) return;
    const command = extractCommand(response);
    onExecute(command);
    handleClose();
  }, [response, onExecute, handleClose]);

  // Copy response
  const handleCopy = useCallback(async () => {
    if (!response) return;
    await navigator.clipboard.writeText(extractCommand(response));
    setCopied(true);
    // Clear previous timeout to avoid stacking
    if (copyTimeoutRef.current) {
      clearTimeout(copyTimeoutRef.current);
    }
    copyTimeoutRef.current = setTimeout(() => {
      setCopied(false);
      copyTimeoutRef.current = null;
    }, 1500);
  }, [response]);

  // Cleanup copy timeout on unmount
  useEffect(() => {
    return () => {
      if (copyTimeoutRef.current) {
        clearTimeout(copyTimeoutRef.current);
        copyTimeoutRef.current = null;
      }
    };
  }, []);

  // Regenerate
  const handleRegenerate = useCallback(() => {
    if (isLoading) return;
    setResponse('');
    setError(null);
    handleSend();
  }, [isLoading, handleSend]);

  // ═══════════════════════════════════════════════════════════════════════════
  // Keyboard Handling
  // ═══════════════════════════════════════════════════════════════════════════

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    e.stopPropagation();

    // Ignore keydown during IME composition (Chinese/Japanese/Korean input).
    // Without this, pressing Enter to confirm a candidate word sends the message.
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;

    // Esc: Close panel
    if (e.key === 'Escape') {
      e.preventDefault();
      handleClose();
      return;
    }

    // Enter: Send (if no response) or Execute (if response ready)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (!response && prompt.trim() && !isLoading) {
        handleSend();
      } else if (response && !isLoading) {
        handleExecute();
      }
      return;
    }

    // Tab: Insert without executing (when response ready)
    if (e.key === 'Tab' && response && !isLoading) {
      e.preventDefault();
      handleInsert();
      return;
    }
  }, [response, prompt, isLoading, handleClose, handleSend, handleExecute, handleInsert]);

  // Prevent terminal from capturing events
  const handleMouseDown = (e: React.MouseEvent) => {
    e.stopPropagation();
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // Render
  // ═══════════════════════════════════════════════════════════════════════════

  if (!isOpen) return null;

  const extractedCommand = response ? extractCommand(response) : '';

  return (
    <div
      ref={panelRef}
      className="absolute z-50 w-[520px] bg-[#1e1e1e] border border-[#3c3c3c] rounded-md shadow-xl"
      style={panelStyle}
      onKeyDown={handleKeyDown}
      onMouseDown={handleMouseDown}
    >
      {/* Loading indicator bar */}
      {isLoading && (
        <div className="absolute top-0 left-0 right-0 h-[2px] overflow-hidden">
          <div className="h-full bg-gradient-to-r from-transparent via-[#0078d4] to-transparent animate-shimmer" />
        </div>
      )}

      {/* Main input row */}
      <div className="flex items-center gap-2 px-3 py-2">
        <Sparkles className="w-4 h-4 text-[#0078d4] flex-shrink-0" />
        <ModelSelector onOpenSettings={handleOpenSettings} />
        
        <input
          ref={inputRef}
          type="text"
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          placeholder={hasSelection 
            ? t('terminal.ai.selection_placeholder', 'Asking about selection...') 
            : t('terminal.ai.inline_placeholder', 'Ask AI for a command...')
          }
          className="flex-1 bg-transparent text-sm text-[#cccccc] placeholder-[#6e6e6e] outline-none font-[var(--terminal-font-family)]"
          disabled={isLoading}
        />

        {/* Action hints */}
        <div className="flex items-center gap-1.5 text-[10px] text-[#6e6e6e]">
          {!response && !isLoading && prompt.trim() && (
            <span className="flex items-center gap-1">
              <kbd className="px-1 py-0.5 bg-[#2d2d2d] rounded text-[9px]">Enter</kbd>
              <span>{t('terminal.ai.to_send', 'send')}</span>
            </span>
          )}
          {response && !isLoading && (
            <>
              <span className="flex items-center gap-1">
                <kbd className="px-1 py-0.5 bg-[#2d2d2d] rounded text-[9px]">Tab</kbd>
                <span>{t('terminal.ai.to_insert', 'insert')}</span>
              </span>
              <span className="flex items-center gap-1 ml-1">
                <kbd className="px-1 py-0.5 bg-[#2d2d2d] rounded text-[9px]">Enter</kbd>
                <span>{t('terminal.ai.to_run', 'run')}</span>
              </span>
            </>
          )}
        </div>

        {/* Close button */}
        <button
          onClick={handleClose}
          className="p-1 text-[#6e6e6e] hover:text-[#cccccc] transition-colors"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* API Key Warning */}
      {!hasApiKey && !isLoading && (
        <div className="mx-3 mb-2 px-2 py-1.5 bg-[#4d3800] border border-[#6e5c00] rounded text-xs text-[#cca700] flex items-center gap-2">
          <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
          <span>{t('terminal.ai.api_key_hint')}</span>
        </div>
      )}

      {/* Error message */}
      {error && (
        <div className="mx-3 mb-2 px-2 py-1.5 bg-[#5a1d1d] border border-[#8b2c2c] rounded text-xs text-[#f48771] flex items-center gap-2">
          <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
          <span className="truncate">{error}</span>
        </div>
      )}

      {/* Response preview */}
      {(response || isLoading) && !error && (
        <div className="border-t border-[#3c3c3c]">
          {/* Command preview */}
          <div className="px-3 py-2 bg-[#252526] font-mono text-sm text-[#9cdcfe] whitespace-pre-wrap break-all max-h-[120px] overflow-y-auto">
            {extractedCommand || (isLoading && <span className="text-[#6e6e6e]">{t('terminal.ai.generating', 'Generating...')}</span>)}
            {isLoading && extractedCommand && (
              <span className="inline-block w-[2px] h-[14px] bg-[#0078d4] ml-0.5 animate-pulse" />
            )}
          </div>

          {/* Action buttons */}
          {response && !isLoading && (
            <div className="flex items-center gap-1 px-2 py-1.5 bg-[#1e1e1e] border-t border-[#3c3c3c]">
              <ActionButton icon={<Play className="w-3 h-3" />} label={t('terminal.ai.execute')} onClick={handleExecute} primary />
              <ActionButton icon={<CornerDownLeft className="w-3 h-3" />} label={t('terminal.ai.insert')} onClick={handleInsert} />
              <ActionButton icon={copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />} label={copied ? t('terminal.ai.copied') : t('terminal.ai.copy')} onClick={handleCopy} />
              <ActionButton icon={<RotateCcw className="w-3 h-3" />} label={t('terminal.ai.regenerate', 'Retry')} onClick={handleRegenerate} />
            </div>
          )}
        </div>
      )}
    </div>
  );
};

// ═══════════════════════════════════════════════════════════════════════════
// Helper Components
// ═══════════════════════════════════════════════════════════════════════════

interface ActionButtonProps {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  primary?: boolean;
}

function ActionButton({ icon, label, onClick, primary }: ActionButtonProps) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-1 px-2 py-1 rounded text-[11px] transition-colors ${
        primary
          ? 'bg-[#0078d4] text-white hover:bg-[#106ebe]'
          : 'text-[#cccccc] hover:bg-[#2d2d2d]'
      }`}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Extract command from AI response.
 * Handles markdown code blocks, strips explanation text.
 */
function extractCommand(text: string): string {
  // Try to extract from code block first
  const codeBlockMatch = text.match(/```(?:\w*\n)?([\s\S]*?)```/);
  if (codeBlockMatch) {
    return codeBlockMatch[1].trim();
  }

  // Try inline code
  const inlineCodeMatch = text.match(/`([^`]+)`/);
  if (inlineCodeMatch) {
    return inlineCodeMatch[1].trim();
  }

  // Just use first non-empty line
  const lines = text.split('\n').filter(l => l.trim());
  if (lines.length > 0) {
    // Strip common prefixes like "$ ", "> ", etc.
    return lines[0].replace(/^[$>]\s*/, '').trim();
  }

  return text.trim();
}
