// src/components/ide/IdeEditor.tsx
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { useIdeStore, IdeTab } from '../../store/ideStore';
import { useCodeMirrorEditor } from './hooks/useCodeMirrorEditor';
import { CodeEditorSearchBar } from './CodeEditorSearchBar';
import { cn } from '../../lib/utils';

interface IdeEditorProps {
  tab: IdeTab;
}

export function IdeEditor({ tab }: IdeEditorProps) {
  const { t } = useTranslation();
  const { updateTabContent, updateTabCursor, saveFile } = useIdeStore();

  // 搜索栏状态
  const [isSearchOpen, setIsSearchOpen] = useState(false);

  // 跟踪上一次的 tab.id / language / contentVersion，用于检测变化
  const prevTabIdRef = useRef<string>(tab.id);
  const prevLanguageRef = useRef<string>(tab.language);
  const prevContentVersionRef = useRef<number>(tab.contentVersion); // 跟踪内容版本号
  const contentInitializedRef = useRef<boolean>(false);

  // 内容变化回调
  const handleChange = useCallback((content: string) => {
    updateTabContent(tab.id, content);
  }, [tab.id, updateTabContent]);

  // 保存回调
  const handleSave = useCallback(async () => {
    try {
      await saveFile(tab.id);
    } catch (e) {
      // 错误处理由 store 或上层组件处理
      console.error('[IdeEditor] Save failed:', e);
    }
  }, [tab.id, saveFile]);

  // 光标位置回调
  const handleCursorChange = useCallback((line: number, col: number) => {
    updateTabCursor(tab.id, line, col);
  }, [tab.id, updateTabCursor]);

  // CodeMirror hook - 使用空字符串初始化，内容加载后通过 setContent 设置
  const {
    containerRef,
    isReady,
    setContent,
    focus,
    getView,
  } = useCodeMirrorEditor({
    initialContent: '',
    language: tab.language,
    onChange: handleChange,
    onSave: handleSave,
    onCursorChange: handleCursorChange,
    onSearchOpen: () => setIsSearchOpen(true),
  });

  // 快捷键处理
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd/Ctrl + F: 打开搜索
      if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        e.preventDefault();
        setIsSearchOpen(true);
      }

      // Escape: 关闭搜索（如果搜索栏是通过快捷键打开的）
      // 注意：CodeEditorSearchBar 内部也会处理 Escape，这里主要用于从编辑器区域触发
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // 当文件内容加载完成或切换文件时，更新编辑器内容
  useEffect(() => {
    // 编辑器未就绪时，重置初始化标志以便后续重新设置
    if (!isReady) {
      contentInitializedRef.current = false;
      return;
    }

    const isNewTab = prevTabIdRef.current !== tab.id;
    const languageChanged = prevLanguageRef.current !== tab.language;
    const hasContent = tab.content !== null;

    // 检测内容版本号变化（冲突 reload 等场景会增加版本号）
    const contentVersionChanged = prevContentVersionRef.current !== tab.contentVersion;

    const needsInit = !contentInitializedRef.current || isNewTab || languageChanged;
    const needsUpdate = needsInit || contentVersionChanged;

    if (hasContent && needsUpdate) {
      setContent(tab.content!);
      contentInitializedRef.current = true;
      prevTabIdRef.current = tab.id;
      prevLanguageRef.current = tab.language;
      prevContentVersionRef.current = tab.contentVersion;
    }
  }, [isReady, tab.id, tab.content, tab.language, tab.contentVersion, setContent]);

  // 切换标签时关闭搜索栏
  useEffect(() => {
    setIsSearchOpen(false);
  }, [tab.id]);

  // 标签激活时聚焦编辑器
  useEffect(() => {
    if (isReady && tab.content !== null && !isSearchOpen) {
      // 短暂延迟确保 DOM 已更新
      const timer = setTimeout(() => focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [isReady, tab.content, focus, isSearchOpen]);

  // 关闭搜索栏
  const handleCloseSearch = useCallback(() => {
    setIsSearchOpen(false);
    focus();
  }, [focus]);

  // 加载中状态
  if (tab.isLoading || tab.content === null) {
    return (
      <div className="h-full flex items-center justify-center bg-theme-bg/50">
        <div className="flex flex-col items-center gap-2">
          <Loader2 className="w-6 h-6 animate-spin text-theme-text-muted" />
          <span className="text-xs text-theme-text-muted">{t('ide.loading_file')}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full w-full relative bg-theme-bg/40">
      {/* 编辑器加载中遮罩 */}
      {!isReady && (
        <div className="absolute inset-0 flex items-center justify-center bg-theme-bg z-10">
          <Loader2 className="w-5 h-5 animate-spin text-theme-text-muted" />
        </div>
      )}

      {/* 搜索栏 */}
      <CodeEditorSearchBar
        view={getView()}
        isOpen={isSearchOpen}
        onClose={handleCloseSearch}
      />

      {/* CodeMirror 容器 */}
      <div
        ref={containerRef}
        className={cn(
          'h-full w-full',
          '[&_.cm-editor]:h-full',
          '[&_.cm-editor_.cm-scroller]:h-full',
          '[&_.cm-scroller]:overflow-auto'
        )}
      />
    </div>
  );
}

