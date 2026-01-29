// src/components/ide/IdeEditor.tsx
import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { useIdeStore, IdeTab } from '../../store/ideStore';
import { useCodeMirrorEditor } from './hooks/useCodeMirrorEditor';
import { cn } from '../../lib/utils';

interface IdeEditorProps {
  tab: IdeTab;
}

export function IdeEditor({ tab }: IdeEditorProps) {
  const { t } = useTranslation();
  const { updateTabContent, updateTabCursor, saveFile } = useIdeStore();
  
  // 跟踪上一次的 tab.id，用于检测是否切换了文件
  const prevTabIdRef = useRef<string>(tab.id);
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
  } = useCodeMirrorEditor({
    initialContent: '',
    language: tab.language,
    onChange: handleChange,
    onSave: handleSave,
    onCursorChange: handleCursorChange,
  });
  
  // 当文件内容加载完成或切换文件时，更新编辑器内容
  useEffect(() => {
    if (!isReady) return;
    
    const isNewTab = prevTabIdRef.current !== tab.id;
    const hasContent = tab.content !== null;
    const needsInit = !contentInitializedRef.current || isNewTab;
    
    if (hasContent && needsInit) {
      setContent(tab.content!);
      contentInitializedRef.current = true;
      prevTabIdRef.current = tab.id;
    }
  }, [isReady, tab.id, tab.content, setContent]);
  
  // 标签激活时聚焦编辑器
  useEffect(() => {
    if (isReady && tab.content !== null) {
      // 短暂延迟确保 DOM 已更新
      const timer = setTimeout(() => focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [isReady, tab.content, focus]);
  
  // 加载中状态
  if (tab.isLoading || tab.content === null) {
    return (
      <div className="h-full flex items-center justify-center bg-zinc-900">
        <div className="flex flex-col items-center gap-2">
          <Loader2 className="w-6 h-6 animate-spin text-zinc-500" />
          <span className="text-xs text-zinc-500">{t('ide.loading_file')}</span>
        </div>
      </div>
    );
  }
  
  return (
    <div className="h-full w-full relative bg-zinc-900">
      {/* 编辑器加载中遮罩 */}
      {!isReady && (
        <div className="absolute inset-0 flex items-center justify-center bg-zinc-900 z-10">
          <Loader2 className="w-5 h-5 animate-spin text-zinc-500" />
        </div>
      )}
      
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
