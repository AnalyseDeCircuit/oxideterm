// src/components/ide/hooks/useCodeMirrorEditor.ts
import { useRef, useEffect, useCallback, useState } from 'react';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, EditorSelection, Extension, Transaction, Compartment } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from '@codemirror/language';
import { highlightSelectionMatches, search } from '@codemirror/search';
import { autocompletion, completionKeymap, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { loadLanguage } from '../../../lib/codemirror/languageLoader';
import { useSettingsStore } from '../../../store/settingsStore';
import { getFontFamily } from '../../../lib/fontFamily';
import { symbolComplete as agentSymbolComplete, symbolDefinitions as agentSymbolDefinitions } from '../../../lib/agentService';
import type { AgentSymbolKind } from '../../../types';

export interface UseCodeMirrorEditorOptions {
  /** 初始内容 */
  initialContent: string;
  /** 语言标识（如 'typescript', 'rust'） */
  language: string;
  /** 内容变化回调 */
  onChange?: (content: string) => void;
  /** 保存回调（Cmd+S） */
  onSave?: () => void;
  /** 光标位置变化回调 */
  onCursorChange?: (line: number, col: number) => void;
  /** 是否只读 */
  readOnly?: boolean;
  /** 触发搜索 UI 的回调 */
  onSearchOpen?: () => void;
  /** Node ID for agent-powered symbol completion (optional) */
  nodeId?: string;
  /** Project root path for symbol lookups (optional) */
  projectRoot?: string;
  /** Go-to-definition callback: navigate to file:line:col (optional) */
  onGoToDefinition?: (path: string, line: number, col?: number) => void;
}

export interface UseCodeMirrorEditorResult {
  /** 绑定到容器 div 的 ref */
  containerRef: React.RefCallback<HTMLDivElement>;
  /** 编辑器是否已就绪 */
  isReady: boolean;
  /** 获取当前内容 */
  getContent: () => string;
  /** 设置内容（会重置编辑器 undo 历史） */
  setContent: (content: string) => void;
  /** 聚焦编辑器 */
  focus: () => void;
  /** 获取 EditorView 实例 */
  getView: () => EditorView | null;
  /** 执行命令 (如 findNext) */
  executeCommand: (command: (view: EditorView) => boolean) => boolean;
  /** 滚动到指定行列并设置光标 */
  scrollToLine: (line: number, col?: number) => void;
}

// Oxide 主题构建器 — 字体跟随终端设置 (Compartment 动态切换)
function buildOxideTheme(fontFamily: string, fontSize: number, lineHeight: number) {
  return EditorView.theme({
    '&': {
      height: '100%',
      fontSize: `${fontSize}px`,
      backgroundColor: 'transparent !important',
    },
    '.cm-scroller': {
      fontFamily,
      lineHeight: `${lineHeight}`,
      overflow: 'auto',
      backgroundColor: 'transparent !important',
    },
  '.cm-content': {
    minHeight: '100%',
    backgroundColor: 'transparent !important',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--theme-bg-panel)',
    borderRight: '1px solid var(--theme-border)',
    color: 'var(--theme-text-muted)',
    opacity: 0.7,
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--theme-accent)',
    color: 'var(--theme-bg)',
    opacity: 0.8,
  },
  '.cm-activeLine': {
    backgroundColor: 'color-mix(in srgb, var(--theme-accent) 5%, transparent)',
  },
  '&.cm-focused .cm-cursor': {
    borderLeftColor: 'var(--theme-accent)',
  },
  '&.cm-focused .cm-selectionBackground, ::selection': {
    backgroundColor: 'color-mix(in srgb, var(--theme-accent) 20%, transparent)',
  },

  // 搜索高亮 - 增加“线包边”视觉效果
  '.cm-searchMatch': {
    backgroundColor: 'color-mix(in srgb, var(--theme-accent) 25%, transparent)',
    outline: '1px solid color-mix(in srgb, var(--theme-accent) 50%, transparent)',
    outlineOffset: '-1px', // 向内缩进，实现完美的包边感而不影响行高
    borderRadius: '2px',
    transition: 'all 0.1s',
  },
  '.cm-searchMatch-selected': {
    backgroundColor: 'color-mix(in srgb, var(--theme-accent) 60%, transparent) !important',
    outline: '1px solid var(--theme-accent) !important',
    boxShadow: '0 0 4px var(--theme-accent)', // 给选中的项增加一点呼吸感阴影
    outlineOffset: '0px',
    borderRadius: '2px',
    zIndex: 2,
  },
  '.cm-panels': {
    display: 'none !important',
  },
});
}

// ═══════════════════════════════════════════════════════════════════════════
// Symbol Completion — agent-powered code intelligence
// ═══════════════════════════════════════════════════════════════════════════

/** Map agent SymbolKind → CodeMirror completion type */
function symbolKindToCompletionType(kind: AgentSymbolKind): string {
  switch (kind) {
    case 'function': return 'function';
    case 'method':   return 'method';
    case 'class':    return 'class';
    case 'struct':   return 'class';
    case 'interface': return 'interface';
    case 'enum':     return 'enum';
    case 'trait':    return 'interface';
    case 'typeAlias': return 'type';
    case 'constant': return 'constant';
    case 'variable': return 'variable';
    case 'module':   return 'namespace';
    default:         return 'text';
  }
}

/**
 * Create a CodeMirror CompletionSource powered by the remote agent.
 * Only triggers when a word prefix of ≥2 chars is typed.
 */
function createAgentCompletionSource(
  nodeId: string,
  projectRoot: string,
): (ctx: CompletionContext) => Promise<CompletionResult | null> {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    // Extract the word being typed
    const word = ctx.matchBefore(/[\w$]+/);
    if (!word || word.from === word.to) return null;

    const prefix = word.text;
    if (prefix.length < 2) return null;

    // Don't block on explicit completion if too short
    if (!ctx.explicit && prefix.length < 3) return null;

    try {
      const symbols = await agentSymbolComplete(nodeId, projectRoot, prefix, 30);
      if (symbols.length === 0) return null;

      return {
        from: word.from,
        options: symbols.map((s) => ({
          label: s.name,
          type: symbolKindToCompletionType(s.kind),
          detail: s.container ? `${s.container} · ${s.path.split('/').pop()}` : s.path.split('/').pop(),
          boost: s.kind === 'function' || s.kind === 'method' ? 1 : 0,
        })),
        filter: false, // Agent already filtered by prefix
      };
    } catch {
      return null;
    }
  };
}

/**
 * Extract the word at the given position in the editor state.
 */
function wordAtPos(state: EditorState, pos: number): { word: string; from: number; to: number } | null {
  const line = state.doc.lineAt(pos);
  const lineText = line.text;
  const col = pos - line.from;

  // Scan backwards for word start
  let start = col;
  while (start > 0 && /[\w$]/.test(lineText[start - 1])) start--;

  // Scan forwards for word end
  let end = col;
  while (end < lineText.length && /[\w$]/.test(lineText[end])) end++;

  if (start === end) return null;
  return {
    word: lineText.slice(start, end),
    from: line.from + start,
    to: line.from + end,
  };
}

// Compartment for dynamic theme reconfiguration
const themeCompartment = new Compartment();

export function useCodeMirrorEditor(options: UseCodeMirrorEditorOptions): UseCodeMirrorEditorResult {
  const {
    initialContent,
    language,
    onChange,
    onSave,
    onCursorChange,
    readOnly = false,
    nodeId,
    projectRoot,
    onGoToDefinition,
  } = options;

  const viewRef = useRef<EditorView | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [isReady, setIsReady] = useState(false);

  // 保存最新的回调引用，避免闭包问题
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  const onCursorChangeRef = useRef(onCursorChange);
  const onGoToDefinitionRef = useRef(onGoToDefinition);

  useEffect(() => {
    onChangeRef.current = onChange;
    onSaveRef.current = onSave;
    onCursorChangeRef.current = onCursorChange;
    onGoToDefinitionRef.current = onGoToDefinition;
  }, [onChange, onSave, onCursorChange, onGoToDefinition]);

  // Callback ref for container
  const setContainerRef = useCallback((node: HTMLDivElement | null) => {
    containerRef.current = node;

    if (!node) {
      // 清理
      if (viewRef.current) {
        viewRef.current.destroy();
        viewRef.current = null;
        setIsReady(false);
      }
      return;
    }

    // 初始化编辑器
    let mounted = true;

    const initEditor = async () => {
      // 加载语言支持
      const langSupport = await loadLanguage(language);

      if (!mounted || !node) return;

      // 构建扩展 — 字体从 settingsStore 读取
      const { settings } = useSettingsStore.getState();
      const fontStack = getFontFamily(settings.terminal.fontFamily, settings.terminal.customFontFamily);
      const initialTheme = buildOxideTheme(fontStack, settings.terminal.fontSize, settings.terminal.lineHeight);

      // Agent symbol completion source (only when nodeId + projectRoot provided)
      const completionOverrides = (nodeId && projectRoot)
        ? [createAgentCompletionSource(nodeId, projectRoot)]
        : [];

      const extensions: Extension[] = [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        foldGutter(),
        indentOnInput(),
        bracketMatching(),
        autocompletion({
          override: completionOverrides.length > 0 ? completionOverrides : undefined,
        }),
        highlightSelectionMatches(),
        // 保持搜索逻辑激活，但通过 CSS 隐藏原生面板
        search(),
        oneDark,
        themeCompartment.of(initialTheme),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          // 移除默认 searchKeymap
          ...completionKeymap,
          indentWithTab,
          // Cmd/Ctrl+S 保存
          {
            key: 'Mod-s',
            run: () => {
              onSaveRef.current?.();
              return true;
            },
          },
        ]),
        // 拦截编辑器内部的 Cmd+F
        EditorView.domEventHandlers({
          keydown(event) {
            if ((event.metaKey || event.ctrlKey) && event.key === 'f') {
              event.preventDefault();
              options.onSearchOpen?.();
              return true;
            }
            return false;
          },
          // Cmd/Ctrl + Click → go-to-definition (agent-powered)
          click(event, view) {
            if (!(event.metaKey || event.ctrlKey)) return false;
            if (!nodeId || !projectRoot) return false;

            const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
            if (pos === null) return false;

            const hit = wordAtPos(view.state, pos);
            if (!hit) return false;

            event.preventDefault();
            // Fire async — don't block the event handler
            agentSymbolDefinitions(nodeId, projectRoot, hit.word).then((defs) => {
              if (defs.length > 0) {
                const def = defs[0]; // Take the first match
                onGoToDefinitionRef.current?.(def.path, def.line, def.column);
              }
            });
            return true;
          },
        }),
        // 监听内容变化
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const newContent = update.state.doc.toString();
            onChangeRef.current?.(newContent);
          }
          // 更新光标位置
          if (update.selectionSet || update.docChanged) {
            const pos = update.state.selection.main.head;
            const line = update.state.doc.lineAt(pos);
            onCursorChangeRef.current?.(line.number, pos - line.from + 1);
          }
        }),
      ];

      // 添加语言支持
      if (langSupport) {
        extensions.push(langSupport);
      }

      // 只读模式
      if (readOnly) {
        extensions.push(EditorState.readOnly.of(true));
      }

      // 创建状态
      const state = EditorState.create({
        doc: initialContent,
        extensions,
      });

      // 清空容器
      node.innerHTML = '';

      // 创建视图
      const view = new EditorView({
        state,
        parent: node,
      });

      viewRef.current = view;
      setIsReady(true);

      // 初始光标位置
      onCursorChangeRef.current?.(1, 1);
    };

    initEditor();

    // 返回清理函数不是必要的，因为 callback ref 会在 node 变为 null 时处理
  }, [language, initialContent, readOnly, nodeId, projectRoot]);

  // 动态响应终端字体/大小设置变更 → 通过 Compartment 重新配置主题
  useEffect(() => {
    const unsub = useSettingsStore.subscribe(
      (state) => ({
        fontFamily: state.settings.terminal.fontFamily,
        customFontFamily: state.settings.terminal.customFontFamily,
        fontSize: state.settings.terminal.fontSize,
        lineHeight: state.settings.terminal.lineHeight,
      }),
      (curr, prev) => {
        if (
          curr.fontFamily === prev.fontFamily &&
          curr.customFontFamily === prev.customFontFamily &&
          curr.fontSize === prev.fontSize &&
          curr.lineHeight === prev.lineHeight
        ) return;

        const view = viewRef.current;
        if (!view) return;

        const fontStack = getFontFamily(curr.fontFamily, curr.customFontFamily);
        view.dispatch({
          effects: themeCompartment.reconfigure(
            buildOxideTheme(fontStack, curr.fontSize, curr.lineHeight),
          ),
        });
      },
    );
    return unsub;
  }, []);

  // 获取内容
  const getContent = useCallback(() => {
    return viewRef.current?.state.doc.toString() || '';
  }, []);

  // 设置内容（重置 undo 历史，防止 Ctrl+Z 回退到旧内容）
  const setContent = useCallback((content: string) => {
    if (!viewRef.current) return;

    const view = viewRef.current;
    // Replace content without adding to undo history
    view.dispatch({
      changes: {
        from: 0,
        to: view.state.doc.length,
        insert: content,
      },
      annotations: Transaction.addToHistory.of(false),
    });
  }, []);

  // 聚焦
  const focus = useCallback(() => {
    viewRef.current?.focus();
  }, []);

  // 获取视图
  const getView = useCallback(() => viewRef.current, []);

  // 执行命令
  const executeCommand = useCallback((command: (view: EditorView) => boolean) => {
    if (!viewRef.current) return false;
    return command(viewRef.current);
  }, []);

  // 滚动到指定行列
  const scrollToLine = useCallback((line: number, col?: number) => {
    const view = viewRef.current;
    if (!view) return;
    const doc = view.state.doc;
    const clampedLine = Math.max(1, Math.min(line, doc.lines));
    const lineInfo = doc.line(clampedLine);
    const clampedCol = Math.max(0, Math.min((col ?? 1) - 1, lineInfo.length));
    const pos = lineInfo.from + clampedCol;
    view.dispatch({
      selection: EditorSelection.cursor(pos),
      scrollIntoView: true,
    });
    view.focus();
  }, []);

  return {
    containerRef: setContainerRef,
    isReady,
    getContent,
    setContent,
    focus,
    getView,
    executeCommand,
    scrollToLine,
  };
}
