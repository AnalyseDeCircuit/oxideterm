// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React from 'react';
import { ChevronRight, Home, HardDrive, Server } from 'lucide-react';
import { cn } from '../../lib/utils';
import { detectLocalPathStyle, joinLocalPath, normalizeLocalPath } from '../fileManager/pathUtils';

interface PathBreadcrumbProps {
  path: string;
  isRemote: boolean;
  onNavigate: (path: string) => void;
  className?: string;
}

// Parse path into segments
const parsePathSegments = (path: string, isRemote: boolean): { name: string; fullPath: string }[] => {
  const segments: { name: string; fullPath: string }[] = [];

  if (isRemote) {
    const normalizedPath = path.replace(/\\/g, '/').replace(/\/+/g, '/');
    // Remote paths always start with /
    segments.push({ name: '/', fullPath: '/' });
    const pathWithoutRoot = normalizedPath.replace(/^\/+/, '');

    if (pathWithoutRoot) {
      const parts = pathWithoutRoot.split('/').filter(Boolean);
      let currentPath = '/';

      for (const part of parts) {
        currentPath = currentPath === '/' ? `/${part}` : `${currentPath}/${part}`;
        segments.push({ name: part, fullPath: currentPath });
      }
    }
    return segments;
  }

  const style = detectLocalPathStyle(path);
  const normalizedPath = normalizeLocalPath(path, style);

  if (style === 'windows') {
    if (normalizedPath.startsWith('\\\\')) {
      const parts = normalizedPath.replace(/^\\+/, '').split('\\').filter(Boolean);
      if (parts.length >= 2) {
        const root = `\\\\${parts[0]}\\${parts[1]}`;
        segments.push({ name: root, fullPath: root });
        let currentPath = root;
        for (const part of parts.slice(2)) {
          currentPath = joinLocalPath(currentPath, part, 'windows');
          segments.push({ name: part, fullPath: currentPath });
        }
      }
      return segments;
    }

    const driveMatch = normalizedPath.match(/^([A-Za-z]:\\)(.*)$/);
    if (driveMatch) {
      segments.push({ name: driveMatch[1].slice(0, 2), fullPath: driveMatch[1] });
      let currentPath = driveMatch[1];
      const parts = driveMatch[2].split('\\').filter(Boolean);
      for (const part of parts) {
        currentPath = joinLocalPath(currentPath, part, 'windows');
        segments.push({ name: part, fullPath: currentPath });
      }
      return segments;
    }

    if (normalizedPath) {
      segments.push({ name: normalizedPath, fullPath: normalizedPath });
    }
    return segments;
  }

  segments.push({ name: '/', fullPath: '/' });
  const pathWithoutRoot = normalizedPath.replace(/^\/+/, '');
  if (pathWithoutRoot) {
    const parts = pathWithoutRoot.split('/').filter(Boolean);
    let currentPath = '/';

    for (const part of parts) {
      currentPath = currentPath === '/' ? `/${part}` : `${currentPath}/${part}`;
      segments.push({ name: part, fullPath: currentPath });
    }
  }

  return segments;
};

export const PathBreadcrumb: React.FC<PathBreadcrumbProps> = ({
  path,
  isRemote,
  onNavigate,
  className,
}) => {
  const segments = parsePathSegments(path, isRemote);
  const localPathStyle = isRemote ? 'posix' : detectLocalPathStyle(path);
  
  // Get icon for root
  const RootIcon = isRemote ? Server : (localPathStyle === 'windows' ? HardDrive : Home);
  
  return (
    <div className={cn(
      "flex items-center gap-0.5 text-sm overflow-x-auto scrollbar-thin scrollbar-thumb-theme-border",
      className
    )}>
      {segments.map((segment, index) => (
        <React.Fragment key={index}>
          {index > 0 && (
            <ChevronRight className="h-3.5 w-3.5 text-theme-text-muted flex-shrink-0" />
          )}
          <button
            onClick={() => onNavigate(segment.fullPath)}
            className={cn(
              "flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-theme-bg-hover/50 transition-colors",
              "text-theme-text hover:text-white whitespace-nowrap",
              index === segments.length - 1 && "text-white font-medium bg-theme-bg-hover/30"
            )}
          >
            {index === 0 && <RootIcon className="h-3.5 w-3.5 text-theme-text-muted" />}
            <span className="max-w-[120px] truncate">{segment.name}</span>
          </button>
        </React.Fragment>
      ))}
    </div>
  );
};
