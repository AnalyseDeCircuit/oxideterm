// src/components/ide/hooks/useCodeMirrorEditor.ts
import { useRef, useEffect, useCallback, useState } from 'react';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from '@codemirror/language';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
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

  // ═══════════════════════════════════════════════════════════════════════════
  // 搜索面板 - 暴力美学重构
  // ═══════════════════════════════════════════════════════════════════════════

  // 1. 容器：强制贴边，强制行布局，消灭换行
  '.cm-search.cm-panel': {
    position: 'absolute !important',
    top: '0 !important',
    right: '0 !important',
    zIndex: '999 !important',

    // UI 风格
    backgroundColor: 'var(--theme-bg-panel) !important',
    border: 'none !important', // 不需要边框，因为它贴边了
    borderLeft: '1px solid var(--theme-border) !important',
    borderBottom: '1px solid var(--theme-border) !important',
    borderBottomLeftRadius: '6px !important',
    boxShadow: '-2px 2px 8px rgba(0,0,0,0.1) !important',

    // 布局核心：拍扁它！
    display: 'flex !important',
    flexDirection: 'row !important',
    flexWrap: 'nowrap !important',
    alignItems: 'center !important',
    padding: '6px 12px !important',
    gap: '6px !important',

    // 尺寸控制
    width: 'auto !important',
    maxWidth: 'calc(100% - 20px) !important',
    minWidth: '400px !important',
  },

  // 2. 彻底消灭所有的换行符
  '.cm-search.cm-panel br': {
    display: 'none !important',
  },

  // 3. 通用控件重置：去掉所有默认样式的干扰
  '.cm-search.cm-panel input, .cm-search.cm-panel button, .cm-search.cm-panel label': {
    margin: '0 !important',
    fontFamily: 'inherit !important',
    fontSize: '12px !important',
    verticalAlign: 'middle !important',
  },

  // 4. 输入框 (Find & Replace) - 紧凑风格
  '.cm-search.cm-panel input[name="search"], .cm-search.cm-panel input[name="replace"]': {
    height: '24px !important',
    width: '120px !important',
    flex: '1 !important', // 允许伸缩
    backgroundColor: 'var(--theme-bg) !important',
    border: '1px solid var(--theme-border) !important',
    borderRadius: '4px !important',
    color: 'var(--theme-text) !important',
    padding: '0 6px !important',
    outline: 'none !important',
    transition: 'all 0.15s !important',
  },
  '.cm-search.cm-panel input:focus': {
    borderColor: 'var(--theme-accent) !important',
    boxShadow: '0 0 0 1px var(--theme-accent) !important',
  },

  // 5. 按钮 (Next, Prev, Replace...) - 图标化/极简风格
  '.cm-search.cm-panel button': {
    height: '24px !important',
    padding: '0 8px !important',
    background: 'transparent !important',
    border: '1px solid transparent !important',
    color: 'var(--theme-text-muted) !important',
    borderRadius: '4px !important',
    cursor: 'pointer !important',
    fontWeight: '500 !important',
    textTransform: 'capitalize !important',
    display: 'flex !important',
    alignItems: 'center !important',
    justifyContent: 'center !important',
  },
  '.cm-search.cm-panel button:hover': {
    color: 'var(--theme-text) !important',
    backgroundColor: 'var(--theme-bg-hover) !important',
  },

  // 6. Checkbox 组 - 缩小并变成透明背景
  '.cm-search.cm-panel label': {
    display: 'flex !important',
    alignItems: 'center !important',
    gap: '4px !important',
    color: 'var(--theme-text-muted) !important',
    fontSize: '11px !important',
    cursor: 'pointer !important',
    padding: '0 4px !important',
  },
  '.cm-search.cm-panel input[type="checkbox"]': {
    appearance: 'none !important',
    width: '12px !important',
    height: '12px !important',
    border: '1px solid currentColor !important',
    borderRadius: '2px !important',
    background: 'transparent !important',
    position: 'relative !important',
  },
  '.cm-search.cm-panel input[type="checkbox"]:checked': {
    background: 'var(--theme-accent) !important',
    borderColor: 'var(--theme-accent) !important',
  },

  // 7. 关闭按钮 - 独立定位到最右边
  '.cm-search.cm-panel button[name="close"]': {
    fontSize: '14px !important',
    width: '20px !important',
    padding: '0 !important',
    marginLeft: '8px !important',
    opacity: '0.6 !important',
  },
  '.cm-search.cm-panel button[name="close"]:hover': {
    opacity: '1 !important',
    backgroundColor: 'var(--theme-bg-hover) !important',
  },

  // 8. 布局调整：让 replace 框如果存在，也尽量挤在一行，或者优雅换行（如果实在太挤）
  // 但为了彻底的单行感，我们尝试让它们 flex-wrap: wrap
  // 并控制 input[name="replace"] 的上一级行为（虽然很难选中父级文本节点）
  // CM6 的结构很扁平，都在一个 div 里。
  // 我们利用 flex order 来重新排序：
  // 顺序: SearchInput -> MatchCase -> Next/Prev -> ReplaceInput -> ReplaceBtns -> Close

  '.cm-search.cm-panel input[name="search"]': { order: '1' },
  '.cm-search.cm-panel label': { order: '2' },
  '.cm-search.cm-panel button[name="next"]': { order: '3' },
  '.cm-search.cm-panel button[name="prev"]': { order: '4' },
  '.cm-search.cm-panel button[name="select"]': { order: '5', display: 'none !important' }, // 隐藏 Select All，一般用不到且占地

  '.cm-search.cm-panel input[name="replace"]': {
    order: '6',
    marginLeft: '8px !important', // 与上一组隔开
  },
  '.cm-search.cm-panel button[name="replace"]': { order: '7' },
  '.cm-search.cm-panel button[name="replaceAll"]': { order: '8' },
  '.cm-search.cm-panel button[name="close"]': { order: '10' },

  // 搜索高亮
  '.cm-searchMatch': {
    backgroundColor: 'color-mix(in srgb, var(--theme-accent) 30%, transparent)',
    borderRadius: '2px',
  },
  '.cm-searchMatch-selected': {
    backgroundColor: 'color-mix(in srgb, var(--theme-accent) 60%, transparent)',
    outline: '1px solid var(--theme-accent)',
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
        oneDark,
        oxideTheme,
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...searchKeymap,
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

  return {
    containerRef: setContainerRef,
    isReady,
    getContent,
    setContent,
    focus,
    getView,
  };
}
