// src/components/ide/hooks/useGitStatus.ts
import { useState, useEffect, useCallback, useRef } from 'react';
import { useIdeStore, registerGitRefreshCallback } from '../../../store/ideStore';
import { api } from '../../../lib/api';

// 防抖函数
function debounce<T extends (...args: unknown[]) => unknown>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  return (...args: Parameters<T>) => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  };
}

/**
 * Git 文件状态类型
 * 基于 git status --porcelain 输出格式
 */
export type GitFileStatus = 
  | 'modified'    // M - 已修改
  | 'added'       // A - 新增（staged）
  | 'deleted'     // D - 已删除
  | 'renamed'     // R - 重命名
  | 'untracked'   // ? - 未跟踪
  | 'ignored'     // ! - 忽略
  | 'conflict';   // U - 冲突

/**
 * Git 仓库状态
 */
export interface GitStatus {
  /** 当前分支名 */
  branch: string;
  /** 领先远程的提交数 */
  ahead: number;
  /** 落后远程的提交数 */
  behind: number;
  /** 文件状态映射（相对于项目根目录的路径 -> 状态） */
  files: Map<string, GitFileStatus>;
}

interface UseGitStatusResult {
  /** Git 状态（非 Git 仓库时为 null） */
  status: GitStatus | null;
  /** 是否正在加载 */
  isLoading: boolean;
  /** 错误信息 */
  error: string | null;
  /** 手动刷新 */
  refresh: () => Promise<void>;
  /** 获取特定文件的 Git 状态 */
  getFileStatus: (relativePath: string) => GitFileStatus | undefined;
}

/**
 * 解析 git status --porcelain=v1 输出
 * 格式: XY filename
 * X = staged 状态, Y = unstaged 状态
 */
function parseGitStatusOutput(output: string): Map<string, GitFileStatus> {
  const files = new Map<string, GitFileStatus>();
  
  for (const line of output.split('\n')) {
    if (!line.trim()) continue;
    
    // 确保至少有 3 个字符: XY + 空格
    if (line.length < 3) continue;
    
    const indexStatus = line[0];   // staged 状态
    const workStatus = line[1];    // unstaged 状态
    let path = line.substring(3);  // 文件路径
    
    // 处理重命名情况: "R  old -> new"
    if (path.includes(' -> ')) {
      path = path.split(' -> ')[1];
    }
    
    // 移除引号（如果有）
    if (path.startsWith('"') && path.endsWith('"')) {
      path = path.slice(1, -1);
    }
    
    let fileStatus: GitFileStatus = 'modified';
    
    // 根据状态码确定文件状态
    if (indexStatus === '?' && workStatus === '?') {
      fileStatus = 'untracked';
    } else if (indexStatus === '!' && workStatus === '!') {
      fileStatus = 'ignored';
    } else if (indexStatus === 'U' || workStatus === 'U' || 
               (indexStatus === 'D' && workStatus === 'D') ||
               (indexStatus === 'A' && workStatus === 'A')) {
      // 冲突状态
      fileStatus = 'conflict';
    } else if (indexStatus === 'A' || workStatus === 'A') {
      fileStatus = 'added';
    } else if (indexStatus === 'D' || workStatus === 'D') {
      fileStatus = 'deleted';
    } else if (indexStatus === 'R' || workStatus === 'R') {
      fileStatus = 'renamed';
    } else if (indexStatus === 'M' || workStatus === 'M') {
      fileStatus = 'modified';
    }
    
    files.set(path, fileStatus);
  }
  
  return files;
}

/**
 * 解析 git branch 信息
 * 格式: ## branch...origin/branch [ahead N, behind M]
 */
function parseBranchInfo(firstLine: string): { branch: string; ahead: number; behind: number } {
  let branch = 'main';
  let ahead = 0;
  let behind = 0;
  
  if (firstLine.startsWith('## ')) {
    const branchPart = firstLine.substring(3);
    
    // 处理 "## branch...origin/branch [ahead 1, behind 2]" 格式
    const bracketMatch = branchPart.match(/\[(.*?)\]/);
    if (bracketMatch) {
      const info = bracketMatch[1];
      const aheadMatch = info.match(/ahead (\d+)/);
      const behindMatch = info.match(/behind (\d+)/);
      if (aheadMatch) ahead = parseInt(aheadMatch[1], 10);
      if (behindMatch) behind = parseInt(behindMatch[1], 10);
    }
    
    // 提取分支名
    const dotIndex = branchPart.indexOf('...');
    const spaceIndex = branchPart.indexOf(' ');
    if (dotIndex > 0) {
      branch = branchPart.substring(0, dotIndex);
    } else if (spaceIndex > 0) {
      branch = branchPart.substring(0, spaceIndex);
    } else if (bracketMatch) {
      branch = branchPart.substring(0, branchPart.indexOf(' ['));
    } else {
      branch = branchPart.trim();
    }
  }
  
  return { branch, ahead, behind };
}

// 刷新间隔（毫秒）- 从 30s 改为 60s 作为保底轮询
const REFRESH_INTERVAL_MS = 60000;

// 防抖延迟（毫秒）- 避免短时间内多次刷新
const DEBOUNCE_DELAY_MS = 1000;

/**
 * Git 状态管理 Hook
 * 
 * 用于获取当前项目的 Git 状态信息，包括分支名和文件修改状态。
 * 
 * 注意：当前实现使用 mock 数据，完整实现需要后端支持执行 SSH 命令。
 * 实际实现应该：
 * 1. 执行 `git status --porcelain=v1 --branch` 命令
 * 2. 解析输出获取分支和文件状态
 * 
 * @example
 * ```tsx
 * function FileTreeItem({ path }: { path: string }) {
 *   const { getFileStatus } = useGitStatus();
 *   const status = getFileStatus(path);
 *   return <span className={getStatusColor(status)}>{path}</span>;
 * }
 * ```
 */
export function useGitStatus(): UseGitStatusResult {
  const { project, connectionId } = useIdeStore();
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  
  /**
   * 刷新 Git 状态
   * 执行 git status --porcelain=v1 --branch 并解析输出
   */
  const refresh = useCallback(async () => {
    if (!project?.isGitRepo || !connectionId) {
      setStatus(null);
      return;
    }
    
    setIsLoading(true);
    setError(null);
    
    try {
      // 执行 git status 命令
      const result = await api.ideExecCommand(
        connectionId,
        'git status --porcelain=v1 --branch 2>/dev/null',
        project.rootPath,
        10 // 10秒超时
      );
      
      // 检查命令是否成功
      if (result.exitCode !== 0) {
        // git 命令失败，可能不是 git 仓库或其他错误
        console.warn('[useGitStatus] git status failed:', result.stderr);
        setStatus({
          branch: project.gitBranch || 'main',
          ahead: 0,
          behind: 0,
          files: new Map(),
        });
        return;
      }
      
      const lines = result.stdout.split('\n');
      const { branch, ahead, behind } = parseBranchInfo(lines[0] || '');
      const files = parseGitStatusOutput(lines.slice(1).join('\n'));
      
      setStatus({ branch, ahead, behind, files });
      
    } catch (e) {
      const errorMessage = e instanceof Error ? e.message : String(e);
      setError(errorMessage);
      console.error('[useGitStatus] Failed to refresh git status:', e);
      
      // 出错时仍然设置一个基本状态
      setStatus({
        branch: project.gitBranch || 'main',
        ahead: 0,
        behind: 0,
        files: new Map(),
      });
    } finally {
      setIsLoading(false);
    }
  }, [project, connectionId]);
  
  /**
   * 获取特定文件的 Git 状态
   * @param relativePath 相对于项目根目录的文件路径
   */
  const getFileStatus = useCallback((relativePath: string): GitFileStatus | undefined => {
    if (!status) return undefined;
    
    // 移除前导斜杠（如果有）
    const normalizedPath = relativePath.startsWith('/') 
      ? relativePath.substring(1) 
      : relativePath;
    
    // 直接查找
    if (status.files.has(normalizedPath)) {
      return status.files.get(normalizedPath);
    }
    
    // 检查是否有子文件被修改（对于目录）
    for (const [path] of status.files) {
      if (path.startsWith(normalizedPath + '/')) {
        return 'modified'; // 目录下有修改的文件
      }
    }
    
    return undefined;
  }, [status]);
  
  // 防抖刷新函数（供外部事件调用）
  const debouncedRefresh = useCallback(
    debounce(() => {
      refresh();
    }, DEBOUNCE_DELAY_MS),
    [refresh]
  );
  
  // 注册 Git 刷新回调（saveFile、终端回车等行为触发）
  useEffect(() => {
    registerGitRefreshCallback(debouncedRefresh);
    return () => {
      registerGitRefreshCallback(() => {}); // 清理时注册空函数
    };
  }, [debouncedRefresh]);
  
  // 监听窗口聚焦事件（用户切回时可能有外部变更）
  useEffect(() => {
    if (!project?.isGitRepo) return;
    
    const handleFocus = () => {
      debouncedRefresh();
    };
    
    window.addEventListener('focus', handleFocus);
    return () => {
      window.removeEventListener('focus', handleFocus);
    };
  }, [project?.isGitRepo, debouncedRefresh]);
  
  // 项目变化或成为 Git 仓库时加载状态
  useEffect(() => {
    if (project?.isGitRepo) {
      refresh();
    } else {
      setStatus(null);
    }
  }, [project?.isGitRepo, project?.rootPath, refresh]);
  
  // 定期刷新（保底轮询，60s）
  useEffect(() => {
    if (project?.isGitRepo) {
      refreshIntervalRef.current = setInterval(refresh, REFRESH_INTERVAL_MS);
      
      return () => {
        if (refreshIntervalRef.current) {
          clearInterval(refreshIntervalRef.current);
          refreshIntervalRef.current = null;
        }
      };
    }
  }, [project?.isGitRepo, refresh]);
  
  // 清理定时器
  useEffect(() => {
    return () => {
      if (refreshIntervalRef.current) {
        clearInterval(refreshIntervalRef.current);
      }
    };
  }, []);
  
  return { 
    status, 
    isLoading, 
    error, 
    refresh,
    getFileStatus,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// 导出辅助常量和函数
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Git 状态对应的 CSS 颜色类名
 */
export const GIT_STATUS_COLORS: Record<GitFileStatus, string> = {
  modified: 'text-yellow-500',
  added: 'text-green-500',
  deleted: 'text-red-500',
  renamed: 'text-blue-500',
  untracked: 'text-zinc-500',
  ignored: 'text-zinc-600',
  conflict: 'text-red-600',
};

/**
 * Git 状态对应的简短标识
 */
export const GIT_STATUS_LABELS: Record<GitFileStatus, string> = {
  modified: 'M',
  added: 'A',
  deleted: 'D',
  renamed: 'R',
  untracked: 'U',
  ignored: '!',
  conflict: '‼',
};
