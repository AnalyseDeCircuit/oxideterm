/**
 * Terminal Keyboard Manager
 * 
 * 统一管理终端应用的按键分流机制，解决终端程序（vim, emacs, tmux）
 * 与应用快捷键之间的冲突问题。
 * 
 * 核心原则：
 * 1. 当终端聚焦且没有 UI 面板打开时，大多数按键应传递给终端
 * 2. 只有明确的应用级快捷键（如 Cmd+T 新建标签）才会被拦截
 * 3. Windows 使用 Shift 变体避免与终端程序冲突
 */

import { useEffect, useRef } from 'react';
import { platform } from '../lib/platform';
import { matchPluginShortcut } from '../lib/plugin/pluginTerminalHooks';

/**
 * 快捷键定义
 */
export interface ShortcutDefinition {
  /** 按键（小写） */
  key: string;
  /** 需要 Ctrl/Cmd */
  ctrl?: boolean;
  /** 需要 Shift */
  shift?: boolean;
  /** 需要 Alt/Option */
  alt?: boolean;
  /** 回调函数 */
  action: () => void;
  /** 描述（用于文档） */
  description?: string;
  /** 
   * 是否允许在终端聚焦时触发
   * - 'always': 始终触发（如 Cmd+W 关闭标签）
   * - 'when-panel-open': 仅当有 UI 面板打开时触发
   * - 'never': 终端聚焦时永不触发，让按键传递给终端
   */
  terminalBehavior: 'always' | 'when-panel-open' | 'never';
}

/**
 * 终端键盘上下文
 */
export interface TerminalKeyboardContext {
  /** 终端是否活跃/聚焦 */
  isTerminalActive: boolean;
  /** 是否有 UI 面板打开（搜索、AI 等） */
  isPanelOpen: boolean;
}

/**
 * 检查按键事件是否匹配快捷键定义
 */
function matchesShortcut(e: KeyboardEvent, shortcut: ShortcutDefinition): boolean {
  const keyMatches = e.key.toLowerCase() === shortcut.key.toLowerCase();
  
  // Ctrl/Cmd 检查
  const ctrlMatches = shortcut.ctrl 
    ? (e.ctrlKey || e.metaKey) 
    : (!e.ctrlKey && !e.metaKey);
  
  // Shift 检查
  const shiftMatches = shortcut.shift ? e.shiftKey : !e.shiftKey;
  
  // Alt 检查
  const altMatches = shortcut.alt ? e.altKey : !e.altKey;
  
  return keyMatches && ctrlMatches && shiftMatches && altMatches;
}

/**
 * 根据上下文决定是否应该执行快捷键
 */
function shouldExecuteShortcut(
  shortcut: ShortcutDefinition,
  context: TerminalKeyboardContext
): boolean {
  // 如果终端不活跃，始终允许执行
  if (!context.isTerminalActive) {
    return true;
  }
  
  // 终端活跃时，根据 terminalBehavior 决定
  switch (shortcut.terminalBehavior) {
    case 'always':
      return true;
    case 'when-panel-open':
      return context.isPanelOpen;
    case 'never':
      return false;
    default:
      return false;
  }
}

/**
 * 应用级快捷键 Hook
 * 
 * @param shortcuts 快捷键定义列表
 * @param context 终端键盘上下文
 */
export function useAppShortcuts(
  shortcuts: ShortcutDefinition[],
  context: TerminalKeyboardContext
) {
  const contextRef = useRef(context);
  contextRef.current = context;
  
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Skip if window lost OS-level focus (e.g. another app in front)
      if (!document.hasFocus()) return;

      // Built-in shortcuts take priority
      for (const shortcut of shortcuts) {
        if (matchesShortcut(e, shortcut)) {
          if (shouldExecuteShortcut(shortcut, contextRef.current)) {
            e.preventDefault();
            e.stopPropagation();
            shortcut.action();
          }
          // 即使不执行也要退出循环，避免重复匹配
          return;
        }
      }

      // Plugin shortcuts (lower priority than built-in)
      const pluginHandler = matchPluginShortcut(e);
      if (pluginHandler) {
        e.preventDefault();
        e.stopPropagation();
        pluginHandler();
        return;
      }
    };
    
    window.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => window.removeEventListener('keydown', handleKeyDown, { capture: true });
  }, [shortcuts]);
}

/**
 * 终端视图专用快捷键 Hook
 * 
 * 用于 TerminalView 和 LocalTerminalView 组件，
 * 只有当该终端是活跃标签时才响应快捷键。
 * 
 * @param isActive 当前终端是否活跃
 * @param isPanelOpen 是否有 UI 面板打开
 * @param handlers 处理器
 */
export function useTerminalViewShortcuts(
  isActive: boolean,
  isPanelOpen: boolean,
  handlers: {
    onOpenSearch?: () => void;
    onCloseSearch?: () => void;
    onOpenAiPanel?: () => void;
    onCloseAiPanel?: () => void;
    onToggleRecording?: () => void;
    onFocusTerminal?: () => void;
    searchOpen: boolean;
    aiPanelOpen: boolean;
  }
) {
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;
  
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // 只有活跃终端才处理
      if (!isActive) return;
      // Skip if window lost OS-level focus
      if (!document.hasFocus()) return;
      
      const h = handlersRef.current;
      
      // === 搜索快捷键 ===
      // Windows: Ctrl+Shift+F（避免与浏览器冲突）
      // Mac/Linux: Cmd+F
      const isSearchShortcut = platform.isWindows
        ? (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'f')
        : (e.metaKey && e.key.toLowerCase() === 'f');
      
      if (isSearchShortcut && h.onOpenSearch) {
        e.preventDefault();
        e.stopPropagation();
        h.onOpenSearch();
        return;
      }
      
      // === AI 面板快捷键 ===
      // Windows: Ctrl+Shift+I（避免冲突）
      // Mac/Linux: Cmd+I
      const isAiShortcut = platform.isWindows
        ? (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'i')
        : (e.metaKey && e.key.toLowerCase() === 'i');
      
      if (isAiShortcut && h.onOpenAiPanel) {
        e.preventDefault();
        e.stopPropagation();
        h.onOpenAiPanel();
        return;
      }
      
      // === Recording toggle shortcut ===
      // Windows: Ctrl+Shift+R
      // Mac/Linux: Cmd+Shift+R
      const isRecordingShortcut = platform.isWindows
        ? (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'r')
        : (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'r');
      
      if (isRecordingShortcut && h.onToggleRecording) {
        e.preventDefault();
        e.stopPropagation();
        h.onToggleRecording();
        return;
      }
      
      // === Escape 关闭面板 ===
      // 只有当面板打开时才拦截，否则让 ESC 传递给终端
      if (e.key === 'Escape') {
        if (h.searchOpen && h.onCloseSearch) {
          e.preventDefault();
          e.stopPropagation();
          h.onCloseSearch();
          h.onFocusTerminal?.();
          return;
        }
        if (h.aiPanelOpen && h.onCloseAiPanel) {
          e.preventDefault();
          e.stopPropagation();
          h.onCloseAiPanel();
          h.onFocusTerminal?.();
          return;
        }
        // 没有面板打开时，不拦截 ESC，让它传递给终端
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isActive, isPanelOpen]);
}

/**
 * 判断按键是否应该被终端捕获（不应被应用拦截）
 * 
 * 这些按键在终端程序中非常重要，不应被应用快捷键覆盖：
 * - Ctrl+C/D/Z: 进程控制
 * - Ctrl+A/E: 行首/行尾（bash/emacs）
 * - Ctrl+R: 反向搜索历史
 * - Ctrl+L: 清屏
 * - Ctrl+U/K/W: 删除操作
 * - Ctrl+P/N: 上一条/下一条命令
 * - Ctrl+B/F: 光标移动
 * - F1-F12: 功能键
 * - Alt+任意键: 终端元键
 */
export function isTerminalReservedKey(e: KeyboardEvent): boolean {
  // F1-F12 功能键 - 永远传递给终端
  if (e.key.startsWith('F') && /^F([1-9]|1[0-2])$/.test(e.key)) {
    return true;
  }
  
  // Alt 组合键 - 终端的元键
  if (e.altKey && !e.ctrlKey && !e.metaKey) {
    return true;
  }
  
  // Ctrl + 单个字母（但不是 Cmd）
  // 这些是终端程序的核心快捷键
  if (e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) {
    const key = e.key.toLowerCase();
    // 保留给终端的 Ctrl+字母
    const terminalCtrlKeys = [
      'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k', 'l',
      'n', 'o', 'p', 'q', 'r', 's', 'u', 'v', 'w', 'x', 'y', 'z'
    ];
    if (terminalCtrlKeys.includes(key)) {
      return true;
    }
  }
  
  return false;
}

/**
 * 快捷键帮助文档
 */
export const SHORTCUT_DOCS = {
  app: {
    newTab: platform.isWindows ? 'Ctrl+T' : 'Cmd+T',
    shellLauncher: platform.isWindows ? 'Ctrl+Shift+T' : 'Cmd+Shift+T',
  },
  terminal: {
    search: platform.isWindows ? 'Ctrl+Shift+F' : 'Cmd+F',
    aiPanel: platform.isWindows ? 'Ctrl+Shift+I' : 'Cmd+I',
    closePanel: 'Escape',
  },
} as const;
