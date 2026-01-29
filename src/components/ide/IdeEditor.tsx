// src/components/ide/IdeEditor.tsx
import { useCallback, useEffect, useMemo } from 'react';
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
  
  // 初始内容（使用 useMemo 避免不必要的重新初始化）
  const initialContent = useMemo(() => tab.content || '', [tab.id]);
  
  // CodeMirror hook
  const {
    containerRef,
    isReady,
    focus,
  } = useCodeMirrorEditor({
    initialContent,
    language: tab.language,
    onChange: handleChange,
    onSave: handleSave,
    onCursorChange: handleCursorChange,
  });
  
  // 标签激活时聚焦编辑器
  useEffect(() => {
    if (isReady) {
      // 短暂延迟确保 DOM 已更新
      const timer = setTimeout(() => focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [isReady, focus]);
  
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
