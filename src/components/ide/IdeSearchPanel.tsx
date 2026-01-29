// src/components/ide/IdeSearchPanel.tsx
import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Search, 
  X, 
  Loader2, 
  File, 
  ChevronRight,
  AlertCircle,
} from 'lucide-react';
import { useIdeStore, useIdeProject } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { Input } from '../ui/input';
import { api } from '../../lib/api';

/**
 * 单个匹配结果
 */
interface SearchMatch {
  /** 文件路径（相对于项目根目录） */
  path: string;
  /** 行号（1-based） */
  line: number;
  /** 列号（0-based） */
  column: number;
  /** 匹配行的预览内容 */
  preview: string;
  /** 匹配开始位置（在 preview 中） */
  matchStart: number;
  /** 匹配结束位置（在 preview 中） */
  matchEnd: number;
}

/**
 * 按文件分组的搜索结果
 */
interface SearchResultGroup {
  /** 文件路径 */
  path: string;
  /** 该文件中的所有匹配 */
  matches: SearchMatch[];
}

interface IdeSearchPanelProps {
  /** 面板是否打开 */
  open: boolean;
  /** 关闭回调 */
  onClose: () => void;
}

/**
 * IDE 搜索面板
 * 
 * 提供项目内文件内容搜索功能。
 * 
 * 注意：当前实现使用 mock 数据，完整实现需要后端支持 SSH exec 功能
 * 来执行 grep 命令搜索文件内容。
 * 
 * @example
 * ```tsx
 * <IdeSearchPanel open={isSearchOpen} onClose={() => setSearchOpen(false)} />
 * ```
 */
export function IdeSearchPanel({ open, onClose }: IdeSearchPanelProps) {
  const { t } = useTranslation();
  const project = useIdeProject();
  const { connectionId, openFile } = useIdeStore();
  
  // 搜索状态
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResultGroup[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  
  // Refs
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  
  // 面板打开时聚焦输入框
  useEffect(() => {
    if (open) {
      // 延迟聚焦以确保动画完成
      const timer = setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [open]);
  
  // 清理防抖定时器
  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, []);
  
  /**
   * 执行搜索
   * 使用 grep 命令搜索文件内容
   */
  const doSearch = useCallback(async (searchQuery: string) => {
    if (!searchQuery.trim() || !connectionId || !project) {
      setResults([]);
      return;
    }
    
    setIsSearching(true);
    setError(null);
    
    try {
      // 转义搜索查询中的特殊字符用于 grep
      const escapedQuery = searchQuery.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      
      // 构建 grep 命令
      // -r: 递归搜索
      // -n: 显示行号
      // -I: 忽略二进制文件
      // --include: 只搜索特定类型文件
      // 限制结果数量为 200 以避免过多输出
      const includePatterns = [
        '*.ts', '*.tsx', '*.js', '*.jsx', '*.json',
        '*.rs', '*.toml', '*.md', '*.txt',
        '*.py', '*.go', '*.java', '*.c', '*.cpp', '*.h',
        '*.css', '*.scss', '*.html', '*.vue', '*.svelte',
        '*.yaml', '*.yml', '*.sh', '*.bash',
      ].map(p => `--include='${p}'`).join(' ');
      
      const command = `grep -rn -I ${includePatterns} '${escapedQuery}' . 2>/dev/null | head -200`;
      
      const result = await api.ideExecCommand(
        connectionId,
        command,
        project.rootPath,
        30 // 30秒超时
      );
      
      // 解析 grep 输出
      // 格式: ./path/to/file:line:content
      const matches: SearchMatch[] = [];
      const lines = result.stdout.split('\n');
      
      for (const line of lines) {
        if (!line.trim()) continue;
        
        // 解析格式: ./path:linenum:content
        const firstColonIdx = line.indexOf(':');
        if (firstColonIdx === -1) continue;
        
        const secondColonIdx = line.indexOf(':', firstColonIdx + 1);
        if (secondColonIdx === -1) continue;
        
        let path = line.substring(0, firstColonIdx);
        const lineNum = parseInt(line.substring(firstColonIdx + 1, secondColonIdx), 10);
        const content = line.substring(secondColonIdx + 1);
        
        if (isNaN(lineNum)) continue;
        
        // 移除前导 ./
        if (path.startsWith('./')) {
          path = path.substring(2);
        }
        
        // 查找匹配位置（不区分大小写）
        const lowerContent = content.toLowerCase();
        const lowerQuery = searchQuery.toLowerCase();
        const matchStart = lowerContent.indexOf(lowerQuery);
        const matchEnd = matchStart >= 0 ? matchStart + searchQuery.length : 0;
        
        matches.push({
          path,
          line: lineNum,
          column: matchStart >= 0 ? matchStart : 0,
          preview: content.trim().substring(0, 200), // 限制预览长度
          matchStart: Math.max(0, matchStart),
          matchEnd: matchEnd,
        });
      }
      
      // 按文件分组
      const grouped = new Map<string, SearchMatch[]>();
      for (const match of matches) {
        const existing = grouped.get(match.path) || [];
        existing.push(match);
        grouped.set(match.path, existing);
      }
      
      const resultGroups: SearchResultGroup[] = Array.from(grouped.entries()).map(
        ([path, fileMatches]) => ({ path, matches: fileMatches })
      );
      
      setResults(resultGroups);
      // 默认展开所有文件
      setExpandedPaths(new Set(resultGroups.map(g => g.path)));
      
    } catch (e) {
      const errorMessage = e instanceof Error ? e.message : String(e);
      setError(errorMessage);
      console.error('[IdeSearchPanel] Search failed:', e);
    } finally {
      setIsSearching(false);
    }
  }, [connectionId, project]);
  
  /**
   * 处理输入变化（带防抖）
   */
  const handleQueryChange = useCallback((value: string) => {
    setQuery(value);
    
    // 清除之前的防抖定时器
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }
    
    // 设置新的防抖定时器
    debounceRef.current = setTimeout(() => {
      doSearch(value);
    }, 300);
  }, [doSearch]);
  
  /**
   * 处理键盘事件
   */
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
    } else if (e.key === 'Enter') {
      // 立即执行搜索
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
      doSearch(query);
    }
  }, [onClose, doSearch, query]);
  
  /**
   * 跳转到搜索结果
   */
  const handleMatchClick = useCallback((match: SearchMatch) => {
    if (!project) return;
    
    // 构建完整路径
    const fullPath = `${project.rootPath}/${match.path}`;
    
    openFile(fullPath).then(() => {
      // TODO: 跳转到指定行
      // 需要通过 ideStore 传递目标行号，然后在 IdeEditor 中处理
      // 目前仅打开文件
      console.log(`[IdeSearchPanel] Open file at line ${match.line}`);
    }).catch(console.error);
  }, [project, openFile]);
  
  /**
   * 切换文件展开/折叠
   */
  const togglePath = useCallback((path: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);
  
  /**
   * 清除搜索
   */
  const handleClear = useCallback(() => {
    setQuery('');
    setResults([]);
    setError(null);
    inputRef.current?.focus();
  }, []);
  
  // 计算总匹配数
  const totalMatches = useMemo(() => {
    return results.reduce((sum, group) => sum + group.matches.length, 0);
  }, [results]);
  
  if (!open) return null;
  
  return (
    <div className="w-80 h-full flex flex-col bg-theme-bg border-r border-theme-border">
      {/* 标题栏 */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-theme-border">
        <div className="flex items-center gap-2">
          <Search className="w-4 h-4 text-theme-accent" />
          <span className="text-sm font-medium">{t('ide.search')}</span>
        </div>
        <button
          onClick={onClose}
          className="p-1 hover:bg-theme-bg-hover rounded transition-colors"
          title={t('ide.cancel')}
        >
          <X className="w-4 h-4 text-theme-text-muted" />
        </button>
      </div>
      
      {/* 搜索输入 */}
      <div className="p-2 border-b border-theme-border">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-4 h-4 text-theme-text-muted" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => handleQueryChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('ide.search_placeholder')}
            className="pl-8 pr-8 bg-theme-bg-panel border-theme-border text-sm"
          />
          {/* 加载/清除按钮 */}
          <div className="absolute right-2 top-1/2 -translate-y-1/2">
            {isSearching ? (
              <Loader2 className="w-4 h-4 animate-spin text-theme-text-muted" />
            ) : query && (
              <button
                onClick={handleClear}
                className="p-0.5 hover:bg-theme-bg-hover rounded transition-colors"
              >
                <X className="w-3 h-3 text-theme-text-muted" />
              </button>
            )}
          </div>
        </div>
        
        {/* 结果统计 */}
        {query && !isSearching && results.length > 0 && (
          <div className="mt-2 text-xs text-theme-text-muted">
            {t('ide.search_results_count', { 
              count: totalMatches, 
              files: results.length 
            })}
          </div>
        )}
      </div>
      
      {/* 搜索结果 */}
      <div className="flex-1 overflow-auto">
        {/* 错误状态 */}
        {error && (
          <div className="flex items-center gap-2 p-4 text-red-400 text-sm">
            <AlertCircle className="w-4 h-4 flex-shrink-0" />
            <span>{error}</span>
          </div>
        )}
        
        {/* 空状态：未输入 */}
        {!query && !error && (
          <div className="p-4 text-theme-text-muted text-sm text-center">
            {t('ide.search_hint')}
          </div>
        )}
        
        {/* 空状态：无结果 */}
        {query && results.length === 0 && !isSearching && !error && (
          <div className="p-4 text-theme-text-muted text-sm text-center">
            {t('ide.no_results')}
          </div>
        )}
        
        {/* 搜索结果列表 */}
        {results.map(group => (
          <div key={group.path} className="border-b border-theme-border/50">
            {/* 文件标题 */}
            <div
              className={cn(
                "flex items-center gap-2 px-3 py-1.5 cursor-pointer",
                "hover:bg-theme-bg-hover/50 transition-colors"
              )}
              onClick={() => togglePath(group.path)}
            >
              <ChevronRight 
                className={cn(
                  'w-3 h-3 text-theme-text-muted transition-transform flex-shrink-0',
                  expandedPaths.has(group.path) && 'rotate-90'
                )}
              />
              <File className="w-4 h-4 text-theme-text-muted flex-shrink-0" />
              <span className="text-sm truncate flex-1 text-theme-text">
                {group.path.split('/').pop()}
              </span>
              <span className="text-xs text-theme-text-muted opacity-60">
                {group.matches.length}
              </span>
            </div>
            
            {/* 文件中的匹配项 */}
            {expandedPaths.has(group.path) && (
              <div className="bg-theme-bg/50">
                {group.matches.map((match, idx) => (
                  <div
                    key={`${match.line}-${idx}`}
                    className={cn(
                      "flex items-start gap-2 px-3 py-1 cursor-pointer",
                      "hover:bg-theme-bg-hover/30 transition-colors text-sm"
                    )}
                    onClick={() => handleMatchClick(match)}
                  >
                    {/* 行号 */}
                    <span className="text-theme-text-muted opacity-60 w-8 text-right flex-shrink-0 font-mono text-xs pt-0.5">
                      {match.line}
                    </span>
                    {/* 预览内容（高亮匹配部分） */}
                    <span className="truncate text-theme-text-muted text-xs">
                      {match.preview.substring(0, match.matchStart)}
                      <span className="text-yellow-500 font-medium bg-yellow-500/10">
                        {match.preview.substring(match.matchStart, match.matchEnd)}
                      </span>
                      {match.preview.substring(match.matchEnd)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
      
      {/* 底部提示 */}
      <div className="px-3 py-2 border-t border-theme-border text-[10px] text-theme-text-muted opacity-60">
        {t('ide.search_shortcut_hint')}
      </div>
    </div>
  );
}
