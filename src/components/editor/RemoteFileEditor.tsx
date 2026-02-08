import { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import {
  indentOnInput,
  bracketMatching,
  foldGutter,
  foldKeymap,
} from '@codemirror/language';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { Save, X, AlertCircle, Check, Loader2, WifiOff, RefreshCw } from 'lucide-react';
import { api } from '../../lib/api';
import { guardSessionConnection, guardNodeCapability, isConnectionGuardError } from '../../lib/connectionGuard';
import {
  loadLanguage,
  normalizeLanguage,
  getLanguageDisplayName,
} from '../../lib/codemirror/languageLoader';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';

// 检测是否是网络错误
function isNetworkError(error: unknown): boolean {
  const msg = String(error).toLowerCase();
  return (
    msg.includes('network') ||
    msg.includes('connection') ||
    msg.includes('timeout') ||
    msg.includes('disconnected') ||
    msg.includes('eof') ||
    msg.includes('broken pipe') ||
    msg.includes('reset by peer') ||
    msg.includes('channel closed')
  );
}

interface RemoteFileEditorProps {
  /** 是否显示 */
  open: boolean;
  /** 关闭回调 */
  onClose: () => void;
  /** SSH Session ID (resolved, may rotate on reconnect) */
  sessionId: string;
  /** Stable node ID for connection guard */
  nodeId?: string;
  /** 远程文件路径 */
  filePath: string;
  /** 初始内容 */
  initialContent: string;
  /** 后端检测的语言 */
  language: string | null;
  /** 检测到的文件编码 */
  encoding?: string;
  /** 文件的 mtime（可选，用于冲突检测） */
  serverMtime?: number | null;
  /** 保存成功回调 */
  onSaved?: () => void;
}

export function RemoteFileEditor({
  open,
  onClose,
  sessionId,
  nodeId,
  filePath,
  initialContent,
  language,
  encoding = 'UTF-8',
  serverMtime,
  onSaved,
}: RemoteFileEditorProps) {
  const { t } = useTranslation();
  // 使用 useState + callback ref 而非 useRef，因为 Dialog 使用 Portal
  // Portal 挂载时 useRef.current 不会触发重渲染，但 useState 会
  const [editorContainer, setEditorContainer] = useState<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const initialContentRef = useRef(initialContent);
  
  // 文件编码状态
  const [currentEncoding] = useState(encoding);

  // 编辑器状态
  const [content, setContent] = useState(initialContent);
  const [isDirty, setIsDirty] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [isNetworkErr, setIsNetworkErr] = useState(false);
  const [lastSavedMtime, setLastSavedMtime] = useState(serverMtime);
  const [showSavedTick, setShowSavedTick] = useState(false);
  const [retryCount, setRetryCount] = useState(0);
  const [editorReady, setEditorReady] = useState(false);

  // 文件名
  const fileName = useMemo(
    () => filePath.split('/').pop() || filePath,
    [filePath]
  );

  // 标准化语言
  const normalizedLang = useMemo(
    () => normalizeLanguage(language),
    [language]
  );

  // 光标位置
  const [cursorInfo, setCursorInfo] = useState({ line: 1, col: 1 });

  // 保存文件的 ref（避免闭包问题）
  const saveRef = useRef<(() => Promise<void>) | undefined>(undefined);

  // 保存文件
  const handleSave = useCallback(async () => {
    if (isSaving || !isDirty) return;

    setIsSaving(true);
    setSaveError(null);
    setIsNetworkErr(false);

    try {
      let effectiveSessionId = sessionId;
      try {
        if (nodeId) {
          const resolved = await guardNodeCapability(nodeId, 'sftp');
          effectiveSessionId = resolved.sessionId;
          if (resolved.sessionId !== sessionId) {
            console.info(`[RemoteFileEditor] Session rotated: ${sessionId} → ${resolved.sessionId}`);
          }
        } else {
          await guardSessionConnection(sessionId);
        }
      } catch (err) {
        if (isConnectionGuardError(err)) {
          return;
        }
        throw err;
      }
      const currentContent = viewRef.current?.state.doc.toString() || content;
      // 传递当前编码，保存时转回原编码
      const result = await api.sftpWriteContent(effectiveSessionId, filePath, currentContent, currentEncoding);
      setLastSavedMtime(result.mtime);
      setIsDirty(false);
      initialContentRef.current = currentContent;
      setRetryCount(0);

      // 显示保存成功勾选
      setShowSavedTick(true);
      setTimeout(() => setShowSavedTick(false), 2000);

      onSaved?.();
    } catch (e) {
      console.error('[Editor] Save failed:', e);
      const errorMsg = e instanceof Error ? e.message : String(e);
      
      // 检测网络错误
      if (isNetworkError(e)) {
        setIsNetworkErr(true);
        setSaveError(t('editor.network_error'));
      } else {
        setSaveError(errorMsg);
      }
    } finally {
      setIsSaving(false);
    }
  }, [sessionId, nodeId, filePath, content, currentEncoding, isDirty, isSaving, onSaved, t]);

  // 重试保存
  const handleRetry = useCallback(async () => {
    setRetryCount(prev => prev + 1);
    setIsNetworkErr(false);
    setSaveError(null);
    
    // 短暂延迟后重试
    setTimeout(() => {
      handleSave();
    }, 500);
  }, [handleSave]);

  // 更新 saveRef
  useEffect(() => {
    saveRef.current = handleSave;
  }, [handleSave]);

  // 初始化 CodeMirror
  useEffect(() => {
    // editorContainer 由 callback ref 设置，Portal 挂载后才会有值
    if (!editorContainer || !open) return;

    const container = editorContainer;
    let view: EditorView | null = null;
    let mounted = true;

    console.log('[RemoteFileEditor] Initializing with content length:', initialContent.length);
    console.log('[RemoteFileEditor] Content preview:', initialContent.substring(0, 100));

    const initEditor = async () => {
      // 加载语言支持
      const langSupport = await loadLanguage(normalizedLang);
      
      // 检查组件是否还挂载
      if (!mounted || !container) {
        console.log('[RemoteFileEditor] Aborted: mounted=', mounted, 'container=', !!container);
        return;
      }

      console.log('[RemoteFileEditor] Creating EditorState with doc:', initialContent.length, 'chars');

      // 基础扩展
      const extensions: Extension[] = [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        foldGutter(),
        indentOnInput(),
        bracketMatching(),
        autocompletion(),
        highlightSelectionMatches(),
        oneDark, // 暗色主题
        // 自定义样式适配 Oxide 主题
        EditorView.theme({
          '&': {
            height: '100%',
            fontSize: '13px',
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
          },
          
          // 3. 选项 Checkboxes - 放在搜索框和导航按钮之后
          '.cm-search.cm-panel label': {
            order: '3',
            display: 'inline-flex',
            alignItems: 'center',
            gap: '6px',
            fontSize: '12px',
            color: 'var(--theme-text-muted)',
            cursor: 'pointer',
            height: '24px',
            marginRight: '8px',
            marginTop: '4px',
            userSelect: 'none',
          },
          
          // Checkbox 本身样式
          '.cm-search.cm-panel input[type="checkbox"]': {
            appearance: 'none',
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: '14px',
            height: '14px',
            border: '1px solid var(--theme-border)',
            borderRadius: '3px',
            backgroundColor: 'var(--theme-bg)',
            position: 'relative',
            cursor: 'pointer',
            margin: '0',
          },
          '.cm-search.cm-panel input[type="checkbox"]:checked': {
            backgroundColor: 'var(--theme-accent)',
            borderColor: 'var(--theme-accent)',
          },
          '.cm-search.cm-panel input[type="checkbox"]:checked::after': {
            content: '""',
            width: '4px',
            height: '8px',
            border: 'solid white',
            borderWidth: '0 2px 2px 0',
            transform: 'rotate(45deg) translate(0px, -1px)',
          },
          
          // 4. 替换输入框 - 强制新行
          '.cm-panel input[name="replace"]': {
            flex: '1 1 100%', 
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
            flex: '0 0 auto', // 不再自动拉伸
            order: '5',
            marginTop: '8px',
            height: '32px',
            padding: '0 16px',
            background: 'var(--theme-bg-panel)', 
            backgroundImage: 'none',
            border: '1px solid var(--theme-border)',
            borderRadius: '4px',
            color: 'var(--theme-text)',
            cursor: 'pointer',
            fontSize: '12px',
            fontWeight: '500',
            boxShadow: 'none',
          },

          // 6. 关闭按钮 - 移除伪元素，使用默认的 x 但调整样式
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

          
          // 聚焦状态统一
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
        }),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...searchKeymap,
          ...completionKeymap,
          indentWithTab,
          // ⌘S 保存
          {
            key: 'Mod-s',
            run: () => {
              saveRef.current?.();
              return true;
            },
          },
        ]),
        // 监听内容变化
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const newContent = update.state.doc.toString();
            setContent(newContent);
            setIsDirty(newContent !== initialContentRef.current);
            setSaveError(null);
          }
          // 更新光标位置
          const pos = update.state.selection.main.head;
          const line = update.state.doc.lineAt(pos);
          setCursorInfo({
            line: line.number,
            col: pos - line.from + 1,
          });
        }),
      ];

      // 添加语言支持（如果有）
      if (langSupport) {
        extensions.push(langSupport);
      }

      // 创建编辑器
      const state = EditorState.create({
        doc: initialContent,
        extensions,
      });

      // 清空容器（防止重复挂载）
      container.innerHTML = '';

      console.log('[RemoteFileEditor] Creating EditorView in container:', container);

      view = new EditorView({
        state,
        parent: container,
      });

      console.log('[RemoteFileEditor] EditorView created, doc length:', view.state.doc.length);

      viewRef.current = view;
      setEditorReady(true);

      // 延迟聚焦，避免 Dialog 动画冲突
      setTimeout(() => {
        if (mounted && view) {
          view.focus();
        }
      }, 100);
    };

    initEditor();

    // 清理
    return () => {
      mounted = false;
      setEditorReady(false);
      if (view) {
        view.destroy();
        view = null;
      }
      viewRef.current = null;
    };
  }, [open, editorContainer, initialContent, normalizedLang]);

  // 关闭前检查
  const handleClose = useCallback(() => {
    if (isDirty) {
      const confirmed = window.confirm(t('editor.unsaved_changes_confirm'));
      if (!confirmed) {
        return;
      }
    }
    onClose();
  }, [isDirty, onClose, t]);

  // 处理 Escape 键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && open) {
        e.preventDefault();
        handleClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [open, handleClose]);

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent 
        className="max-w-6xl w-[90vw] h-[85vh] !flex !flex-col p-0 gap-0 bg-zinc-900 overflow-hidden"
        aria-describedby={undefined}
      >
        {/* 标题栏 */}
        <DialogHeader className="flex-shrink-0 px-4 py-2 border-b border-zinc-700/50 bg-zinc-800/80">
          <div className="flex items-center justify-between">
            <DialogTitle className="flex items-center gap-2 text-sm font-medium">
              <span className="text-zinc-200">{fileName}</span>
              {isDirty && (
                <span
                  className="w-2 h-2 rounded-full bg-orange-500 animate-pulse"
                  title={t('editor.modified')}
                />
              )}
              {isSaving && (
                <Loader2 className="w-3.5 h-3.5 animate-spin text-orange-400" />
              )}
              {showSavedTick && !isDirty && !isSaving && (
                <Check className="w-3.5 h-3.5 text-green-500" />
              )}
            </DialogTitle>
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={handleSave}
                disabled={!isDirty || isSaving}
                className="h-7 px-3 text-xs hover:bg-zinc-700/50"
              >
                <Save className="w-3.5 h-3.5 mr-1.5" />
                {isSaving ? t('editor.saving') : t('editor.save')}
                <kbd className="ml-2 text-[10px] text-zinc-500 bg-zinc-700/50 px-1 rounded">
                  ⌘S
                </kbd>
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleClose}
                className="h-7 w-7 p-0 hover:bg-zinc-700/50"
              >
                <X className="w-4 h-4" />
              </Button>
            </div>
          </div>
        </DialogHeader>

        {/* 编辑器区域 */}
        <div className="flex-1 min-h-0 overflow-hidden relative">
          {/* Loading 状态 */}
          {!editorReady && (
            <div className="absolute inset-0 flex items-center justify-center bg-zinc-900">
              <Loader2 className="w-6 h-6 animate-spin text-zinc-500" />
            </div>
          )}
          <div
            ref={setEditorContainer}
            className="h-full w-full [&_.cm-editor]:h-full [&_.cm-editor_.cm-scroller]:h-full [&_.cm-scroller]:overflow-auto"
          />
        </div>

        {/* 状态栏 */}
        <div className="flex-shrink-0 px-4 py-1.5 border-t border-zinc-700/50 bg-zinc-800/80 flex items-center justify-between text-xs">
          <div className="flex items-center gap-4 text-zinc-400">
            <span>
              {t('editor.line')} {cursorInfo.line}, {t('editor.column')} {cursorInfo.col}
            </span>
            <span className="px-1.5 py-0.5 bg-zinc-700/50 rounded text-zinc-300">
              {getLanguageDisplayName(normalizedLang)}
            </span>
            <span 
              className="px-1.5 py-0.5 bg-zinc-700/50 rounded text-zinc-300 cursor-default"
              title={t('editor.encoding_detected', { encoding: currentEncoding })}
            >
              {currentEncoding}
            </span>
          </div>
          <div className="flex items-center gap-3">
            {/* 网络错误显示 + 重试按钮 */}
            {isNetworkErr && (
              <div className="flex items-center gap-2">
                <span className="text-orange-400 flex items-center gap-1.5">
                  <WifiOff className="w-3.5 h-3.5 flex-shrink-0" />
                  <span className="truncate">{saveError}</span>
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleRetry}
                  disabled={isSaving}
                  className="h-5 px-2 text-xs text-orange-400 hover:text-orange-300 hover:bg-orange-500/10"
                >
                  <RefreshCw className={`w-3 h-3 mr-1 ${isSaving ? 'animate-spin' : ''}`} />
                  {t('editor.retry')} {retryCount > 0 && `(${retryCount})`}
                </Button>
              </div>
            )}
            {/* 普通错误显示 */}
            {saveError && !isNetworkErr && (
              <span className="text-red-400 flex items-center gap-1.5 max-w-xs truncate">
                <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
                <span className="truncate">{saveError}</span>
              </span>
            )}
            {lastSavedMtime && !saveError && (
              <span className="text-zinc-500">
                {t('editor.last_saved')}: {new Date(lastSavedMtime * 1000).toLocaleTimeString()}
              </span>
            )}
            <span className="text-zinc-500 max-w-[300px] truncate" title={filePath}>
              {filePath}
            </span>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
