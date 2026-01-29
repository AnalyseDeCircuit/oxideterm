// src/components/ide/IdeEditorTabs.tsx
import { useCallback, useState, useRef } from 'react';
import { X, Circle, Loader2 } from 'lucide-react';
import { useIdeTabs, useIdeStore, IdeTab } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { FileIcon } from '../../lib/fileIcons';
import { IdeSaveConfirmDialog } from './dialogs/IdeSaveConfirmDialog';

interface TabItemProps {
  tab: IdeTab;
  isActive: boolean;
  onActivate: () => void;
  onClose: () => void;
}

function TabItem({ tab, isActive, onActivate, onClose }: TabItemProps) {
  const handleClose = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onClose();
  }, [onClose]);
  
  // 中键点击关闭
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button === 1) {
      e.preventDefault();
      onClose();
    }
  }, [onClose]);
  
  return (
    <div
      className={cn(
        'group flex items-center gap-1.5 px-3 py-1.5 cursor-pointer',
        'border-r border-theme-border/50 transition-colors',
        'hover:bg-theme-bg-hover/30',
        isActive 
          ? 'bg-theme-bg-hover border-b-2 border-b-theme-accent' 
          : 'bg-theme-bg/50'
      )}
      onClick={onActivate}
      onMouseDown={handleMouseDown}
    >
      {/* 文件图标 */}
      <span className="flex-shrink-0">
        <FileIcon filename={tab.name} size={14} />
      </span>
      
      {/* 文件名 */}
      <span className={cn(
        'text-xs truncate max-w-[120px]',
        isActive ? 'text-theme-text' : 'text-theme-text-muted',
        tab.isDirty && 'italic'
      )}>
        {tab.name}
      </span>
      
      {/* 状态指示器 / 关闭按钮 */}
      <div className="w-4 h-4 flex items-center justify-center flex-shrink-0 ml-1">
        {tab.isLoading ? (
          <Loader2 className="w-3 h-3 animate-spin text-theme-text-muted" />
        ) : tab.isDirty ? (
          // 未保存指示器（hover 时显示关闭按钮）
          <>
            <Circle 
              className={cn(
                'w-2 h-2 fill-theme-accent text-theme-accent',
                'group-hover:hidden'
              )} 
            />
            <button
              className="hidden group-hover:flex items-center justify-center w-4 h-4 rounded hover:bg-theme-bg-hover/50"
              onClick={handleClose}
            >
              <X className="w-3 h-3 text-theme-text-muted" />
            </button>
          </>
        ) : (
          // 关闭按钮
          <button
            className={cn(
              'flex items-center justify-center w-4 h-4 rounded',
              'opacity-0 group-hover:opacity-100 transition-opacity',
              'hover:bg-theme-bg-hover/50'
            )}
            onClick={handleClose}
          >
            <X className="w-3 h-3 text-theme-text-muted" />
          </button>
        )}
      </div>
    </div>
  );
}

export function IdeEditorTabs() {
  const tabs = useIdeTabs();
  const { activeTabId, setActiveTab, closeTab, saveFile } = useIdeStore();
  
  // 保存确认对话框状态
  const [confirmDialog, setConfirmDialog] = useState<{
    open: boolean;
    tabId: string;
    fileName: string;
  }>({ open: false, tabId: '', fileName: '' });
  
  // 滚动容器 ref
  const scrollRef = useRef<HTMLDivElement>(null);
  
  // 处理标签关闭
  const handleCloseTab = useCallback(async (tabId: string) => {
    const tab = tabs.find(t => t.id === tabId);
    if (!tab) return;
    
    const closed = await closeTab(tabId);
    if (!closed) {
      // 需要确认
      setConfirmDialog({
        open: true,
        tabId,
        fileName: tab.name,
      });
    }
  }, [tabs, closeTab]);
  
  // 保存确认对话框的操作
  const handleSaveAndClose = useCallback(async () => {
    const { tabId } = confirmDialog;
    try {
      await saveFile(tabId);
      await closeTab(tabId);
    } catch (e) {
      console.error('[IdeEditorTabs] Save failed:', e);
    }
    setConfirmDialog({ open: false, tabId: '', fileName: '' });
  }, [confirmDialog, saveFile, closeTab]);
  
  const handleDiscardAndClose = useCallback(async () => {
    const { tabId } = confirmDialog;
    // 强制关闭（不保存）
    useIdeStore.setState(state => ({
      tabs: state.tabs.filter(t => t.id !== tabId),
      activeTabId: state.activeTabId === tabId 
        ? (state.tabs.length > 1 ? state.tabs.find(t => t.id !== tabId)?.id || null : null)
        : state.activeTabId,
    }));
    setConfirmDialog({ open: false, tabId: '', fileName: '' });
  }, [confirmDialog]);
  
  const handleCancelClose = useCallback(() => {
    setConfirmDialog({ open: false, tabId: '', fileName: '' });
  }, []);
  
  // 横向滚动
  const handleWheel = useCallback((e: React.WheelEvent) => {
    if (scrollRef.current) {
      e.preventDefault();
      scrollRef.current.scrollLeft += e.deltaY;
    }
  }, []);
  
  if (tabs.length === 0) {
    return null;
  }
  
  return (
    <>
      <div
        ref={scrollRef}
        className="flex items-stretch border-b border-theme-border/50 bg-theme-bg/80 overflow-x-auto scrollbar-none"
        onWheel={handleWheel}
      >
        {tabs.map(tab => (
          <TabItem
            key={tab.id}
            tab={tab}
            isActive={tab.id === activeTabId}
            onActivate={() => setActiveTab(tab.id)}
            onClose={() => handleCloseTab(tab.id)}
          />
        ))}
      </div>
      
      {/* 保存确认对话框 */}
      <IdeSaveConfirmDialog
        open={confirmDialog.open}
        fileName={confirmDialog.fileName}
        onSave={handleSaveAndClose}
        onDiscard={handleDiscardAndClose}
        onCancel={handleCancelClose}
      />
    </>
  );
}
