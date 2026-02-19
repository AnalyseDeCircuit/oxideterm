import { useRef, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import {
  FilePlus,
  FolderPlus,
  Edit3,
  Trash2,
  Copy,
  Terminal,
} from 'lucide-react';

interface ContextMenuPosition {
  x: number;
  y: number;
}

export interface IdeTreeContextMenuProps {
  position: ContextMenuPosition;
  path: string;
  isDirectory: boolean;
  name: string;
  onNewFile: () => void;
  onNewFolder: () => void;
  onRename: () => void;
  onDelete: () => void;
  onCopyPath: () => void;
  onRevealInTerminal: () => void;
  onClose: () => void;
}

export function IdeTreeContextMenu({
  position,
  // path, isDirectory, name 传入用于未来功能扩展，当前未使用
  onNewFile,
  onNewFolder,
  onRename,
  onDelete,
  onCopyPath,
  onRevealInTerminal,
  onClose,
}: IdeTreeContextMenuProps) {
  const { t } = useTranslation();
  const menuRef = useRef<HTMLDivElement>(null);

  // 点击外部关闭
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    // 用 setTimeout 避免立即触发（因为右键菜单事件本身也是 click）
    const timer = setTimeout(() => {
      document.addEventListener('mousedown', handleClickOutside);
    }, 0);

    return () => {
      clearTimeout(timer);
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [onClose]);

  // ESC 关闭
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!document.hasFocus()) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  // 调整位置避免超出屏幕
  const adjustedPosition = {
    x: Math.min(position.x, window.innerWidth - 200),
    y: Math.min(position.y, window.innerHeight - 280),
  };

  const handleAction = useCallback(
    (action: () => void) => {
      action();
      onClose();
    },
    [onClose]
  );

  return (
    <div
      ref={menuRef}
      className="fixed z-[100] bg-theme-bg border border-theme-border rounded-md shadow-lg py-1 min-w-[180px] animate-in fade-in zoom-in-95 duration-100"
      style={{
        left: adjustedPosition.x,
        top: adjustedPosition.y,
      }}
    >
      {/* 新建区域 */}
      <MenuItem
        icon={<FilePlus className="w-4 h-4" />}
        onClick={() => handleAction(onNewFile)}
      >
        {t('ide.contextMenu.newFile', 'New File')}
      </MenuItem>
      <MenuItem
        icon={<FolderPlus className="w-4 h-4" />}
        onClick={() => handleAction(onNewFolder)}
      >
        {t('ide.contextMenu.newFolder', 'New Folder')}
      </MenuItem>

      <MenuDivider />

      {/* 编辑区域 */}
      <MenuItem
        icon={<Edit3 className="w-4 h-4" />}
        onClick={() => handleAction(onRename)}
        shortcut="F2"
      >
        {t('ide.contextMenu.rename', 'Rename')}
      </MenuItem>
      <MenuItem
        icon={<Trash2 className="w-4 h-4" />}
        onClick={() => handleAction(onDelete)}
        variant="danger"
      >
        {t('ide.contextMenu.delete', 'Delete')}
      </MenuItem>

      <MenuDivider />

      {/* 其他操作 */}
      <MenuItem
        icon={<Copy className="w-4 h-4" />}
        onClick={() => handleAction(onCopyPath)}
      >
        {t('ide.contextMenu.copyPath', 'Copy Path')}
      </MenuItem>
      <MenuItem
        icon={<Terminal className="w-4 h-4" />}
        onClick={() => handleAction(onRevealInTerminal)}
      >
        {t('ide.contextMenu.openInTerminal', 'Open in Terminal')}
      </MenuItem>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Sub-components
// ═══════════════════════════════════════════════════════════════════════════

interface MenuItemProps {
  icon: React.ReactNode;
  children: React.ReactNode;
  onClick: () => void;
  variant?: 'danger';
  shortcut?: string;
  disabled?: boolean;
}

function MenuItem({
  icon,
  children,
  onClick,
  variant,
  shortcut,
  disabled,
}: MenuItemProps) {
  return (
    <button
      type="button"
      className={cn(
        'flex items-center w-full px-3 py-1.5 text-xs text-left transition-colors',
        'hover:bg-theme-bg-hover focus:bg-theme-bg-hover focus:outline-none',
        variant === 'danger' && 'text-red-400 hover:bg-red-500/10',
        disabled && 'opacity-50 cursor-not-allowed'
      )}
      onClick={disabled ? undefined : onClick}
      disabled={disabled}
    >
      <span className="mr-2 opacity-70">{icon}</span>
      <span className="flex-1">{children}</span>
      {shortcut && (
        <span className="ml-4 text-theme-text-muted text-[10px] opacity-60">
          {shortcut}
        </span>
      )}
    </button>
  );
}

function MenuDivider() {
  return <div className="h-px bg-theme-border my-1" />;
}
