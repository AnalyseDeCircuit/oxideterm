// src/components/ide/IdeEditorArea.tsx
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Code2 } from 'lucide-react';
import { useIdeTabs, useIdeActiveTab, useIdeStore } from '../../store/ideStore';
import { IdeEditorTabs } from './IdeEditorTabs';
import { IdeEditor } from './IdeEditor';
import { IdeConflictDialog, ConflictResolution } from './dialogs/IdeConflictDialog';

export function IdeEditorArea() {
  const { t } = useTranslation();
  const tabs = useIdeTabs();
  const activeTab = useIdeActiveTab();
  const { conflictState, resolveConflict, clearConflict } = useIdeStore();
  
  // 获取冲突文件信息
  const conflictTab = conflictState 
    ? tabs.find(t => t.id === conflictState.tabId) 
    : null;
  
  // 处理冲突解决
  const handleConflictResolve = useCallback(async (resolution: ConflictResolution) => {
    if (resolution === 'cancel') {
      clearConflict();
      return;
    }
    
    try {
      await resolveConflict(resolution === 'overwrite' ? 'overwrite' : 'reload');
    } catch (e) {
      console.error('[IdeEditorArea] Conflict resolution failed:', e);
    }
  }, [resolveConflict, clearConflict]);
  
  if (tabs.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-theme-text-muted bg-theme-bg/50 text-center px-4">
        <Code2 className="w-16 h-16 mb-4 opacity-20 shrink-0" />
        <p className="text-sm">{t('ide.no_open_files')}</p>
        <p className="text-xs mt-1 opacity-60">{t('ide.click_to_open')}</p>
      </div>
    );
  }
  
  return (
    <div className="h-full flex flex-col bg-theme-bg">
      {/* 标签栏 */}
      <IdeEditorTabs />
      
      {/* 编辑器区域 */}
      <div className="flex-1 min-h-0 relative">
        {activeTab && <IdeEditor tab={activeTab} />}
      </div>
      
      {/* 冲突对话框 */}
      <IdeConflictDialog
        open={!!conflictState && !!conflictTab}
        fileName={conflictTab?.name || ''}
        localTime={new Date((conflictState?.localMtime || 0) * 1000)}
        remoteTime={new Date((conflictState?.remoteMtime || 0) * 1000)}
        onResolve={handleConflictResolve}
      />
    </div>
  );
}
