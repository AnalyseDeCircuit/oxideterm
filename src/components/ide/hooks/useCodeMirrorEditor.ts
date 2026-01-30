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
    backgroundColor: 'rgb(39 39 42 / 0.5)',
    borderRight: '1px solid rgb(63 63 70 / 0.5)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'rgb(234 88 12 / 0.1)',
  },
  '.cm-activeLine': {
    backgroundColor: 'rgb(234 88 12 / 0.05)',
  },
  '&.cm-focused .cm-cursor': {
    borderLeftColor: '#f97316',
  },
  '&.cm-focused .cm-selectionBackground, ::selection': {
    backgroundColor: 'rgb(234 88 12 / 0.2)',
  },
  
  // ═══════════════════════════════════════════════════════════════════════════
  // 搜索面板主题 - 深度统一 Shadcn UI 风格
  // ═══════════════════════════════════════════════════════════════════════════
  
  // 面板容器 - 使用 Flex Flow 布局
  '.cm-search.cm-panel': {
    backgroundColor: 'var(--theme-bg-panel)',
    color: 'var(--theme-text)',
    padding: '12px',
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
    alignItems: 'center',
    borderBottom: '1px solid var(--theme-border)',
    minWidth: '350px',
  },
  
  // 隐藏默认的换行
  '.cm-search.cm-panel > br': {
    display: 'none',
  },
  
  // 1. 搜索框 - 占据主导位置
  '.cm-panel input[name="search"]': {
    flex: '1 1 200px', // 自适应宽度，最小200px
    height: '32px',
    backgroundColor: 'var(--theme-bg)',
    border: '1px solid var(--theme-border)',
    borderRadius: '4px', // Shadcn radius-sm
    color: 'var(--theme-text)',
    padding: '0 10px',
    fontSize: '13px',
    outline: 'none',
    fontFamily: 'inherit',
    order: '1',
  },
  '.cm-panel input[name="search"]:focus': {
    borderColor: 'var(--theme-accent)',
    boxShadow: '0 0 0 1px var(--theme-accent)',
  },
  
  // 2. 导航按钮组 (Prev/Next/Select)
  '.cm-panel button[name="next"], .cm-panel button[name="prev"], .cm-panel button[name="select"]': {
    flex: '0 0 auto',
    order: '2',
    height: '32px',
    padding: '0 12px',
            background: 'transparent', // Reset background (image & color)
            backgroundImage: 'none',   // Explicitly remove default gradient
            border: '1px solid var(--theme-border)',
            borderRadius: '4px',
            color: 'var(--theme-text)',
            fontSize: '12px',
            fontWeight: '500',
            cursor: 'pointer',
            textTransform: 'capitalize',
            boxShadow: 'none',
          },
          '.cm-panel button:hover': {
            background: 'var(--theme-bg-hover)',
    alignItems: 'center',
    gap: '6px',
    fontSize: '12px',
    color: 'var(--theme-text-muted)',
    cursor: 'pointer',
    height: '24px',
    marginRight: '8px',
    marginTop: '4px', // 稍微与上方隔开
    userSelect: 'none',
  },
  // Hack: 让第一个 Label 强制换行 (如果空间不足，Flex wrap 会自动处理，但我们可以强制)
  // 由于无法精准选中"第一个label"，利用宽度填充机制
  
  // Checkbox 本身样式
  '.cm-panel input[type="checkbox"]': {
    appearance: 'none',
    width: '14px',
    height: '14px',
    border: '1px solid var(--theme-border)',
    borderRadius: '3px',
    backgroundColor: 'var(--theme-bg)',
    position: 'relative',
    cursor: 'pointer',
    margin: '0',
  },
  '.cm-panel input[type="checkbox"]:checked': {
    backgroundColor: 'var(--theme-accent)',
    borderColor: 'var(--theme-accent)',
  },
  '.cm-panel input[type="checkbox"]:checked::after': {
    content: '""',
    position: 'absolute',
    left: '4px',
    top: '1px',
    width: '4px',
    height: '8px',
    border: 'solid white',
    borderWidth: '0 2px 2px 0',
    transform: 'rotate(45deg)',
  },
  
  // 4. 替换输入框 - 强制新行
  '.cm-panel input[name="replace"]': {
    flex: '1 1 100%', // 强制占满整行
    order: '4',
    marginTop: '8px',
    height: '32px',
    backgroundColor: 'var(--theme-bg)',
    border: '1px solid var(--theme-border)',
    borderRadius: '4px',
    color: 'var(--theme-text)',
    padding: '0 10px',
    fontSize: '13px',
    outline: 'none',
  },
  
  // 5. 替换按钮
  '.cm-panel button[name="replace"], .cm-panel button[name="replaceAll"]': {
            flex: '0 0 auto', // 均分宽度
            order: '5',
            marginTop: '8px',
            height: '32px',
            background: 'var(--theme-bg-panel)', // Reset all background props
            backgroundImage: 'none',
            border: '1px solid var(--theme-border)',
            padding: '0 16px',
            borderRadius: '4px',
            color: 'var(--theme-text)',
            cursor: 'pointer',
            fontSize: '12px',
            fontWeight: '500',
            boxShadow: 'none',
          },

          // 6. 关闭按钮 - 绝对定位
          '.cm-panel button[name="close"]': {
            position: 'absolute',
            top: '8px',
            right: '8px',
            padding: '4px',
            background: 'transparent',
            backgroundImage: 'none',
            border: 'none',
            boxShadow: 'none',
            color: 'var(--theme-text-muted)',
            cursor: 'pointer',
            borderRadius: '4px',
            fontSize: '16px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: '24px',
            height: '24px',
            opacity: '0.7',
          },
          '.cm-panel button[name="close"]:hover': {
            backgroundColor: 'var(--theme-bg-hover)',
            color: 'var(--theme-text)',
            opacity: '1',
          },
          // 移除之前的伪元素定义，避免双重 X
  '.cm-panel *:focus-visible': {
     outline: 'none',
     boxShadow: '0 0 0 1px var(--theme-accent)',
     borderColor: 'var(--theme-accent)',
  },

  
  // 搜索高亮
  '.cm-searchMatch': {
    backgroundColor: 'rgba(234, 88, 12, 0.3)',
    borderRadius: '2px',
  },
  '.cm-searchMatch-selected': {
    backgroundColor: 'rgba(234, 88, 12, 0.6)',
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
