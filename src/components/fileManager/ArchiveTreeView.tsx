/**
 * ArchiveTreeView Component
 *
 * Renders archive contents as a recursive, collapsible directory tree.
 * Builds a tree structure from flat entries using their `path` field.
 */

import React, { useState, useMemo, useCallback } from 'react';
import { ChevronRight, ChevronDown, Folder, File, FolderOpen } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { ArchiveEntry, ArchiveInfo } from './types';
import type { TFunction } from 'i18next';

// ── Tree node ────────────────────────────────────────────────────────────────

interface TreeNode {
  name: string;
  /** Full archive path */
  path: string;
  isDir: boolean;
  entry?: ArchiveEntry;
  children: Map<string, TreeNode>;
}

/** Build a tree from a flat list of archive entries */
function buildTree(entries: ArchiveEntry[]): TreeNode {
  const root: TreeNode = { name: '', path: '', isDir: true, children: new Map() };

  for (const entry of entries) {
    const parts = entry.path.replace(/\/$/, '').split('/').filter(Boolean);
    let current = root;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLast = i === parts.length - 1;

      if (!current.children.has(part)) {
        current.children.set(part, {
          name: part,
          path: parts.slice(0, i + 1).join('/'),
          isDir: isLast ? entry.isDir : true,
          entry: isLast ? entry : undefined,
          children: new Map(),
        });
      } else if (isLast) {
        // Update existing node with entry data
        const node = current.children.get(part)!;
        node.entry = entry;
        node.isDir = entry.isDir;
      }

      current = current.children.get(part)!;
    }
  }

  return root;
}

/** Sort children: directories first, then alphabetically */
function sortedChildren(node: TreeNode): TreeNode[] {
  return [...node.children.values()].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

// ── Formatting ───────────────────────────────────────────────────────────────

const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

// ── Tree row (grid-based, matching original archive table style) ──────────────

interface TreeRowProps {
  node: TreeNode;
  depth: number;
  defaultOpen?: boolean;
  forceExpanded?: boolean;
  /** Row index ref — shared mutable counter for alternating stripes */
  rowIdx: { current: number };
}

const TreeRow: React.FC<TreeRowProps> = ({ node, depth, defaultOpen = false, forceExpanded, rowIdx }) => {
  const [expanded, setExpanded] = useState(defaultOpen);
  const children = useMemo(() => sortedChildren(node), [node]);
  const hasChildren = children.length > 0;
  const idx = rowIdx.current++;

  const toggle = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    if (hasChildren) setExpanded(v => !v);
  }, [hasChildren]);

  return (
    <>
      <div
        className={cn(
          'grid grid-cols-[1fr_80px_80px_120px] gap-2 px-3 py-1.5 text-xs',
          idx % 2 === 0 ? 'bg-zinc-900/20' : 'bg-transparent',
          'hover:bg-zinc-800/50',
        )}
      >
        {/* Name column — tree structure with indentation */}
        <div
          className="flex items-center gap-1.5 min-w-0 cursor-default select-none"
          style={{ paddingLeft: `${depth * 16}px` }}
          onClick={toggle}
        >
          {/* Chevron */}
          <span className="w-4 h-4 flex items-center justify-center shrink-0">
            {node.isDir && hasChildren ? (
              expanded ? (
                <ChevronDown className="h-3 w-3 text-zinc-500" />
              ) : (
                <ChevronRight className="h-3 w-3 text-zinc-500" />
              )
            ) : (
              <span className="w-3" />
            )}
          </span>

          {/* Icon */}
          {node.isDir ? (
            expanded ? (
              <FolderOpen className="h-3.5 w-3.5 text-amber-400 shrink-0" />
            ) : (
              <Folder className="h-3.5 w-3.5 text-amber-400 shrink-0" />
            )
          ) : (
            <File className="h-3.5 w-3.5 text-zinc-500 shrink-0" />
          )}

          {/* Name */}
          <span className="truncate text-zinc-300" title={node.path}>
            {node.name}
          </span>
        </div>

        {/* Size */}
        <span className="text-right text-zinc-500">
          {node.isDir ? '-' : node.entry ? formatSize(node.entry.size) : '-'}
        </span>

        {/* Compressed */}
        <span className="text-right text-zinc-500">
          {node.isDir ? '-' : node.entry ? formatSize(node.entry.compressedSize) : '-'}
        </span>

        {/* Modified */}
        <span className="text-right text-zinc-600">
          {node.entry?.modified || '-'}
        </span>
      </div>

      {/* Children */}
      {expanded &&
        children.map(child => (
          <TreeRow
            key={child.path}
            node={child}
            depth={depth + 1}
            defaultOpen={forceExpanded}
            forceExpanded={forceExpanded}
            rowIdx={rowIdx}
          />
        ))}
    </>
  );
};

// ── Main component ───────────────────────────────────────────────────────────

interface ArchiveTreeViewProps {
  archiveInfo: ArchiveInfo;
  t: TFunction;
}

export const ArchiveTreeView: React.FC<ArchiveTreeViewProps> = ({ archiveInfo, t }) => {
  const tree = useMemo(() => buildTree(archiveInfo.entries), [archiveInfo.entries]);
  const rootChildren = useMemo(() => sortedChildren(tree), [tree]);

  // Expand all / collapse all — key remount forces all TreeRow states to reset
  const [expandKey, setExpandKey] = useState(0);
  const [allExpanded, setAllExpanded] = useState(false);

  const toggleAll = useCallback(() => {
    setAllExpanded(v => !v);
    setExpandKey(k => k + 1);
  }, []);

  // Shared mutable counter for alternating row stripes
  const rowIdx = { current: 0 };

  return (
    <div className="p-4">
      {/* Archive Stats */}
      <div className="flex items-center gap-4 mb-4 p-3 bg-zinc-900/50 rounded-lg text-xs text-zinc-400">
        <div className="flex items-center gap-1.5">
          <Folder className="h-3.5 w-3.5" />
          <span>{archiveInfo.totalDirs} {t('fileManager.folders')}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <File className="h-3.5 w-3.5" />
          <span>{archiveInfo.totalFiles} {t('fileManager.files')}</span>
        </div>
        <div className="ml-auto flex items-center gap-3">
          <span>{t('fileManager.originalSize')}: {formatSize(archiveInfo.totalSize)}</span>
          <span>{t('fileManager.compressedSize')}: {formatSize(archiveInfo.compressedSize)}</span>
          <span className="text-emerald-400">
            {archiveInfo.totalSize > 0
              ? `${Math.round((1 - archiveInfo.compressedSize / archiveInfo.totalSize) * 100)}%`
              : '0%'
            } {t('fileManager.saved')}
          </span>
        </div>
      </div>

      {/* Header */}
      <div className="grid grid-cols-[1fr_80px_80px_120px] gap-2 px-3 py-2 bg-zinc-900/80 border-b border-theme-border text-xs font-medium text-zinc-400 sticky top-0 z-10">
          <div className="flex items-center justify-between">
            <span>{t('fileManager.name')}</span>
            <button
              className="text-zinc-500 hover:text-zinc-300 transition-colors text-[10px]"
              onClick={toggleAll}
            >
              {allExpanded ? t('fileManager.collapseAll') : t('fileManager.expandAll')}
            </button>
          </div>
          <span className="text-right">{t('fileManager.size')}</span>
          <span className="text-right">{t('fileManager.compressed')}</span>
          <span className="text-right">{t('fileManager.modified')}</span>
      </div>

      {/* Rows */}
      {rootChildren.map(child => (
        <TreeRow
          key={`${expandKey}-${child.path}`}
          node={child}
          depth={0}
          defaultOpen={allExpanded}
          forceExpanded={allExpanded}
          rowIdx={rowIdx}
        />
      ))}
    </div>
  );
};

