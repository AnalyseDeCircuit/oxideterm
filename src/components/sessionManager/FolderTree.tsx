import { useTranslation } from 'react-i18next';
import { ChevronRight, ChevronDown, Folder, FolderOpen, Clock, Inbox } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { FolderNode } from './useSessionManager';

type FolderTreeProps = {
  folderTree: FolderNode[];
  selectedGroup: string | null;
  expandedGroups: Set<string>;
  totalCount: number;
  ungroupedCount: number;
  onSelectGroup: (group: string | null) => void;
  onToggleExpand: (path: string) => void;
};

const TreeNode = ({
  node,
  depth,
  selectedGroup,
  expandedGroups,
  onSelectGroup,
  onToggleExpand,
}: {
  node: FolderNode;
  depth: number;
  selectedGroup: string | null;
  expandedGroups: Set<string>;
  onSelectGroup: (group: string | null) => void;
  onToggleExpand: (path: string) => void;
}) => {
  const isExpanded = expandedGroups.has(node.fullPath);
  const isSelected = selectedGroup === node.fullPath;
  const hasChildren = node.children.length > 0;

  return (
    <div>
      <div
        className={cn(
          'flex items-center gap-1 px-2 py-1 cursor-pointer rounded text-sm hover:bg-theme-bg-hover transition-colors min-w-0',
          isSelected && 'bg-theme-bg-active text-theme-text font-medium'
        )}
        style={{ paddingLeft: `${Math.min(depth, 5) * 16 + 8}px` }}
        onClick={() => onSelectGroup(node.fullPath)}
      >
        {hasChildren ? (
          <button
            className="p-0.5 hover:bg-theme-bg-hover rounded"
            onClick={(e) => {
              e.stopPropagation();
              onToggleExpand(node.fullPath);
            }}
          >
            {isExpanded
              ? <ChevronDown className="h-3.5 w-3.5 text-theme-text-muted" />
              : <ChevronRight className="h-3.5 w-3.5 text-theme-text-muted" />}
          </button>
        ) : (
          <span className="w-[18px]" />
        )}
        {isExpanded
          ? <FolderOpen className="h-4 w-4 text-yellow-500 shrink-0" />
          : <Folder className="h-4 w-4 text-yellow-500 shrink-0" />}
        <span className="truncate flex-1">{node.name}</span>
        <span className="text-xs text-theme-text-muted tabular-nums">
          {node.connectionCount}
        </span>
      </div>
      {isExpanded && hasChildren && (
        <div>
          {node.children.map(child => (
            <TreeNode
              key={child.fullPath}
              node={child}
              depth={depth + 1}
              selectedGroup={selectedGroup}
              expandedGroups={expandedGroups}
              onSelectGroup={onSelectGroup}
              onToggleExpand={onToggleExpand}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export const FolderTree = ({
  folderTree,
  selectedGroup,
  expandedGroups,
  totalCount,
  ungroupedCount,
  onSelectGroup,
  onToggleExpand,
}: FolderTreeProps) => {
  const { t } = useTranslation();

  return (
    <div className="h-full flex flex-col text-theme-text select-none min-w-0">
      {/* Pinned top: All Connections */}
      <div className="shrink-0 pt-2 px-1">
        <div
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 cursor-pointer rounded text-sm hover:bg-theme-bg-hover transition-colors min-w-0',
            selectedGroup === null && 'bg-theme-bg-active font-medium'
          )}
          onClick={() => onSelectGroup(null)}
        >
          <Inbox className="h-4 w-4 text-blue-400 shrink-0" />
          <span className="flex-1 truncate">{t('sessionManager.folder_tree.all_connections')}</span>
          <span className="text-xs text-theme-text-muted tabular-nums">{totalCount}</span>
        </div>
      </div>

      {/* Scrollable middle: Group tree + Ungrouped */}
      <div className="flex-1 overflow-y-auto min-h-0 min-w-0 px-1 py-1">
        {folderTree.map(node => (
          <TreeNode
            key={node.fullPath}
            node={node}
            depth={0}
            selectedGroup={selectedGroup}
            expandedGroups={expandedGroups}
            onSelectGroup={onSelectGroup}
            onToggleExpand={onToggleExpand}
          />
        ))}

        {/* Ungrouped (inside scrollable area, right after groups) */}
        {ungroupedCount > 0 && (
          <div
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 cursor-pointer rounded text-sm hover:bg-theme-bg-hover transition-colors mt-0.5 min-w-0',
              selectedGroup === '__ungrouped__' && 'bg-theme-bg-active font-medium'
            )}
            onClick={() => onSelectGroup('__ungrouped__')}
          >
            <Folder className="h-4 w-4 text-theme-text-muted shrink-0" />
            <span className="flex-1 truncate">{t('sessionManager.folder_tree.ungrouped')}</span>
            <span className="text-xs text-theme-text-muted tabular-nums">{ungroupedCount}</span>
          </div>
        )}
      </div>

      {/* Pinned bottom: Recent */}
      <div className="shrink-0 border-t border-theme-border px-1 py-1.5">
        <div
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 cursor-pointer rounded text-sm hover:bg-theme-bg-hover transition-colors min-w-0',
            selectedGroup === '__recent__' && 'bg-theme-bg-active font-medium'
          )}
          onClick={() => onSelectGroup('__recent__')}
        >
          <Clock className="h-4 w-4 text-theme-text-muted shrink-0" />
          <span className="flex-1 truncate">{t('sessionManager.folder_tree.recent')}</span>
        </div>
      </div>
    </div>
  );
};
