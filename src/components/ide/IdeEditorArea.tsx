// src/components/ide/IdeEditorArea.tsx
import { useTranslation } from 'react-i18next';
import { Code2 } from 'lucide-react';
import { useIdeTabs, useIdeActiveTab } from '../../store/ideStore';

export function IdeEditorArea() {
  const { t } = useTranslation();
  const tabs = useIdeTabs();
  const activeTab = useIdeActiveTab();
  
  if (tabs.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-zinc-500">
        <Code2 className="w-16 h-16 mb-4 opacity-20" />
        <p>{t('ide.no_open_files')}</p>
        <p className="text-sm mt-1">{t('ide.click_to_open')}</p>
      </div>
    );
  }
  
  return (
    <div className="flex-1 flex flex-col">
      {/* 标签栏（Phase 2 实现） */}
      <div className="h-9 bg-zinc-900 border-b border-zinc-800 flex items-center px-2 text-sm text-zinc-400">
        {tabs.map(tab => (
          <span key={tab.id} className="px-2">{tab.name}</span>
        ))}
      </div>
      
      {/* 编辑器（Phase 2 实现） */}
      <div className="flex-1 bg-zinc-950 p-4 text-zinc-500">
        {activeTab ? `Editing: ${activeTab.path}` : 'No file selected'}
      </div>
    </div>
  );
}
