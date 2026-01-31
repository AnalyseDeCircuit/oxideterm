// src/components/ide/hooks/useCodeMirrorEditor.ts
import { useRef, useEffect, useCallback, useState } from 'react';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from '@codemirror/language';
import { highlightSelectionMatches, search } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { loadLanguage } from '../../../lib/codemirror/languageLoader';

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
}

export interface UseCodeMirrorEditorResult {
  /** 绑定到容器 div 的 ref */
  containerRef: React.RefCallback<HTMLDivElement>;
  /** 编辑器是否已就绪 */
  isReady: boolean;
  /** 获取当前内容 */
  getContent: () => string;
  /** 设置内容（会重置编辑器状态） */
  setContent: (content: string) => void;
  /** 聚焦编辑器 */
  focus: () => void;
  /** 获取 EditorView 实例 */
  getView: () => EditorView | null;
  /** 执行命令 (如 findNext) */
  executeCommand: (command: (view: EditorView) => boolean) => boolean;
}

// Oxide 主题覆盖（与 RemoteFileEditor 保持一致）
const oxideTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '13px',
    backgroundColor: 'transparent',
  },
  '.cm-scroller': {
    fontFamily: '"JetBrains Mono", "Fira Code", "Menlo", monospace',
    overflow: 'auto',
  },
  '.cm-content': {
    minHeight: '100%',
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

export function useCodeMirrorEditor(options: UseCodeMirrorEditorOptions): UseCodeMirrorEditorResult {
  const {
    initialContent,
    language,
    onChange,
    onSave,
    onCursorChange,
    readOnly = false,
  } = options;

  const viewRef = useRef<EditorView | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [isReady, setIsReady] = useState(false);

  // 保存最新的回调引用，避免闭包问题
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  const onCursorChangeRef = useRef(onCursorChange);

  useEffect(() => {
    onChangeRef.current = onChange;
    onSaveRef.current = onSave;
    onCursorChangeRef.current = onCursorChange;
  }, [onChange, onSave, onCursorChange]);

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

      // 构建扩展
      const extensions: Extension[] = [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        foldGutter(),
        indentOnInput(),
        bracketMatching(),
        autocompletion(),
        highlightSelectionMatches(),
        // 保持搜索逻辑激活，但通过 CSS 隐藏原生面板
        search(),
        oneDark,
        oxideTheme,
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
          }
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
  }, [language, initialContent, readOnly]);

  // 获取内容
  const getContent = useCallback(() => {
    return viewRef.current?.state.doc.toString() || '';
  }, []);

  // 设置内容
  const setContent = useCallback((content: string) => {
    if (!viewRef.current) return;

    const view = viewRef.current;
    view.dispatch({
      changes: {
        from: 0,
        to: view.state.doc.length,
        insert: content,
      },
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

  return {
    containerRef: setContainerRef,
    isReady,
    getContent,
    setContent,
    focus,
    getView,
    executeCommand,
  };
}
