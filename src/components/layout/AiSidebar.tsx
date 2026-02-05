import { useState, useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Sparkles, PanelRightClose } from 'lucide-react';
import { useSettingsStore } from '../../store/settingsStore';
import { AiChatPanel } from '../ai/AiChatPanel';
import { Button } from '../ui/button';
import { cn } from '../../lib/utils';

/**
 * AiSidebar - Right-side AI chat panel
 * 
 * Features:
 * - Collapsible with toggle button
 * - Resizable width (drag handle on left edge)
 * - Persisted state in settingsStore
 */
export const AiSidebar = () => {
  const { t } = useTranslation();
  
  // Get state from settings store
  const aiSidebarCollapsed = useSettingsStore((s) => s.settings.sidebarUI.aiSidebarCollapsed);
  const aiSidebarWidth = useSettingsStore((s) => s.settings.sidebarUI.aiSidebarWidth);
  const aiEnabled = useSettingsStore((s) => s.settings.ai.enabled);
  const { setAiSidebarWidth, toggleAiSidebar } = useSettingsStore();
  
  // Resize state
  const [isResizing, setIsResizing] = useState(false);
  const sidebarRef = useRef<HTMLDivElement>(null);

  // Handle resize start
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  // Handle resize move/end
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;

      // Calculate new width based on mouse position (from right edge)
      const windowWidth = window.innerWidth;
      const newWidth = windowWidth - e.clientX;
      setAiSidebarWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      // Prevent text selection during resize
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
    };
  }, [isResizing, setAiSidebarWidth]);

  // Collapsed state - hide completely (toggle button is in left sidebar)
  if (aiSidebarCollapsed) {
    return null;
  }

  // Expanded state
  return (
    <div
      ref={sidebarRef}
      className="relative flex flex-col bg-theme-bg border-l border-theme-border/50 h-full"
      style={{ width: aiSidebarWidth }}
    >
      {/* Header with collapse button */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-theme-border/30">
        <div className="flex items-center gap-2">
          <Sparkles className="w-4 h-4 text-theme-accent" />
          <span className="text-sm font-medium text-theme-text">{t('sidebar.panels.ai')}</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={toggleAiSidebar}
          title={t('sidebar.tooltips.collapse')}
          className="w-7 h-7"
        >
          <PanelRightClose className="w-4 h-4" />
        </Button>
      </div>

      {/* AI Chat Panel */}
      <div className="flex-1 overflow-hidden">
        <AiChatPanel />
      </div>

      {/* Resize Handle - on LEFT edge for right sidebar */}
      <div
        className={cn(
          "absolute left-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-theme-accent/50 transition-colors z-10",
          isResizing && "bg-theme-accent"
        )}
        onMouseDown={handleMouseDown}
      />
    </div>
  );
};
