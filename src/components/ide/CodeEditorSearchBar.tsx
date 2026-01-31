// src/components/ide/CodeEditorSearchBar.tsx
/**
 * 编辑器内搜索栏
 * 
 * 这是一个完全自定义的 React 组件，用于替换 CodeMirror 默认的搜索面板。
 * 通过 CM6 的底层 API 来执行搜索操作，而不是使用其内置的 UI。
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Search,
    X,
    ChevronUp,
    ChevronDown,
    Replace,
    CaseSensitive,
    Regex,
    ReplaceAll,
} from 'lucide-react';
import { EditorView } from '@codemirror/view';
import {
    SearchQuery,
    setSearchQuery,
    findNext,
    findPrevious,
    replaceNext as cmReplaceNext,
    replaceAll as cmReplaceAll,
} from '@codemirror/search';
import { cn } from '../../lib/utils';

interface CodeEditorSearchBarProps {
    /** 编辑器视图实例 */
    view: EditorView | null;
    /** 是否显示搜索栏 */
    isOpen: boolean;
    /** 关闭回调 */
    onClose: () => void;
}

/**
 * 编辑器内搜索栏
 * 
 * 提供查找/替换功能，完全使用 React 渲染，通过 CM6 API 与编辑器交互。
 */
export function CodeEditorSearchBar({ view, isOpen, onClose }: CodeEditorSearchBarProps) {
    const { t } = useTranslation();

    // 搜索状态
    const [searchText, setSearchText] = useState('');
    const [replaceText, setReplaceText] = useState('');
    const [caseSensitive, setCaseSensitive] = useState(false);
    const [useRegex, setUseRegex] = useState(false);
    const [showReplace, setShowReplace] = useState(false);

    // 匹配信息
    const [matchCount, setMatchCount] = useState(0);
    const [currentMatch, setCurrentMatch] = useState(0);

    const searchInputRef = useRef<HTMLInputElement>(null);
    const replaceInputRef = useRef<HTMLInputElement>(null);

    // 打开时聚焦搜索框
    useEffect(() => {
        if (isOpen) {
            setTimeout(() => {
                searchInputRef.current?.focus();
                searchInputRef.current?.select();
            }, 50);
        }
    }, [isOpen]);

    // 更新搜索查询
    const updateSearch = useCallback(() => {
        if (!view) return;

        try {
            const query = new SearchQuery({
                search: searchText,
                caseSensitive,
                regexp: useRegex,
                replace: replaceText,
            });

            view.dispatch({ effects: setSearchQuery.of(query) });

            // 统计匹配数量
            // 注意：CM6 没有直接提供匹配数量的 API，我们需要遍历
            let count = 0;
            if (searchText) {
                const cursor = query.getCursor(view.state.doc);
                while (!cursor.next().done) {
                    count++;
                    if (count > 999) break; // 限制遍历次数
                }
            }
            setMatchCount(count);
            setCurrentMatch(count > 0 ? 1 : 0);
        } catch {
            // 正则表达式无效时忽略
            setMatchCount(0);
            setCurrentMatch(0);
        }
    }, [view, searchText, caseSensitive, useRegex, replaceText]);

    // 搜索文本变化时更新
    useEffect(() => {
        updateSearch();
    }, [updateSearch]);

    // 查找下一个
    const handleFindNext = useCallback(() => {
        if (!view) return;
        findNext(view);
        setCurrentMatch(prev => (prev < matchCount ? prev + 1 : 1));
        view.focus();
    }, [view, matchCount]);

    // 查找上一个
    const handleFindPrev = useCallback(() => {
        if (!view) return;
        findPrevious(view);
        setCurrentMatch(prev => (prev > 1 ? prev - 1 : matchCount));
        view.focus();
    }, [view, matchCount]);

    // 替换当前
    const handleReplace = useCallback(() => {
        if (!view) return;
        cmReplaceNext(view);
        updateSearch();
    }, [view, updateSearch]);

    // 替换全部
    const handleReplaceAll = useCallback(() => {
        if (!view) return;
        cmReplaceAll(view);
        updateSearch();
    }, [view, updateSearch]);

    // 键盘事件处理
    const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
        if (e.key === 'Escape') {
            onClose();
        } else if (e.key === 'Enter') {
            if (e.shiftKey) {
                handleFindPrev();
            } else {
                handleFindNext();
            }
        } else if (e.key === 'F3') {
            e.preventDefault();
            if (e.shiftKey) {
                handleFindPrev();
            } else {
                handleFindNext();
            }
        }
    }, [onClose, handleFindNext, handleFindPrev]);

    // 关闭时清理搜索状态
    const handleClose = useCallback(() => {
        if (view) {
            // 清除搜索高亮
            view.dispatch({ effects: setSearchQuery.of(new SearchQuery({ search: '' })) });
        }
        onClose();
    }, [view, onClose]);

    if (!isOpen) return null;

    return (
        <div
            className={cn(
                // 容器：贴边悬浮
                "absolute top-0 right-4 z-50",
                // 外观
                "bg-theme-bg-panel border border-theme-border rounded-b-lg shadow-lg",
                // 布局
                "flex flex-col gap-1.5 p-2",
                // 动画
                "animate-in slide-in-from-top-2 duration-150"
            )}
            onKeyDown={handleKeyDown}
        >
            {/* 第一行：搜索 */}
            <div className="flex items-center gap-1.5">
                {/* 展开替换按钮 */}
                <button
                    onClick={() => setShowReplace(!showReplace)}
                    className={cn(
                        "p-1 rounded transition-colors",
                        showReplace
                            ? "bg-theme-accent/20 text-theme-accent"
                            : "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                    )}
                    title={t('ide.toggle_replace')}
                >
                    <Replace className="w-3.5 h-3.5" />
                </button>

                {/* 搜索输入框 */}
                <div className="relative flex-1">
                    <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-theme-text-muted pointer-events-none" />
                    <input
                        ref={searchInputRef}
                        type="text"
                        value={searchText}
                        onChange={(e) => setSearchText(e.target.value)}
                        placeholder={t('ide.find_placeholder')}
                        className={cn(
                            "w-48 h-7 pl-7 pr-2 text-sm",
                            "bg-theme-bg border border-theme-border rounded",
                            "text-theme-text placeholder:text-theme-text-muted/50",
                            "focus:outline-none focus:border-theme-accent focus:ring-1 focus:ring-theme-accent/30",
                            "transition-colors"
                        )}
                    />
                </div>

                {/* 大小写敏感 */}
                <button
                    onClick={() => setCaseSensitive(!caseSensitive)}
                    className={cn(
                        "p-1.5 rounded transition-colors",
                        caseSensitive
                            ? "bg-theme-accent/20 text-theme-accent"
                            : "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                    )}
                    title={t('ide.match_case')}
                >
                    <CaseSensitive className="w-3.5 h-3.5" />
                </button>

                {/* 正则表达式 */}
                <button
                    onClick={() => setUseRegex(!useRegex)}
                    className={cn(
                        "p-1.5 rounded transition-colors",
                        useRegex
                            ? "bg-theme-accent/20 text-theme-accent"
                            : "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                    )}
                    title={t('ide.use_regex')}
                >
                    <Regex className="w-3.5 h-3.5" />
                </button>

                {/* 匹配计数 */}
                <span className="text-xs text-theme-text-muted min-w-[40px] text-center tabular-nums">
                    {searchText ? (matchCount > 999 ? '999+' : `${currentMatch}/${matchCount}`) : ''}
                </span>

                {/* 上一个 */}
                <button
                    onClick={handleFindPrev}
                    disabled={matchCount === 0}
                    className={cn(
                        "p-1.5 rounded transition-colors",
                        matchCount > 0
                            ? "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                            : "text-theme-text-muted/30 cursor-not-allowed"
                    )}
                    title={t('ide.find_previous')}
                >
                    <ChevronUp className="w-3.5 h-3.5" />
                </button>

                {/* 下一个 */}
                <button
                    onClick={handleFindNext}
                    disabled={matchCount === 0}
                    className={cn(
                        "p-1.5 rounded transition-colors",
                        matchCount > 0
                            ? "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                            : "text-theme-text-muted/30 cursor-not-allowed"
                    )}
                    title={t('ide.find_next')}
                >
                    <ChevronDown className="w-3.5 h-3.5" />
                </button>

                {/* 关闭 */}
                <button
                    onClick={handleClose}
                    className="p-1.5 rounded text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover transition-colors"
                    title={t('ide.close_search')}
                >
                    <X className="w-3.5 h-3.5" />
                </button>
            </div>

            {/* 第二行：替换（可折叠） */}
            {showReplace && (
                <div className="flex items-center gap-1.5 animate-in slide-in-from-top-1 duration-100">
                    {/* 占位，与上一行对齐 */}
                    <div className="w-6" />

                    {/* 替换输入框 */}
                    <div className="relative flex-1">
                        <input
                            ref={replaceInputRef}
                            type="text"
                            value={replaceText}
                            onChange={(e) => setReplaceText(e.target.value)}
                            placeholder={t('ide.replace_placeholder')}
                            className={cn(
                                "w-48 h-7 px-2 text-sm",
                                "bg-theme-bg border border-theme-border rounded",
                                "text-theme-text placeholder:text-theme-text-muted/50",
                                "focus:outline-none focus:border-theme-accent focus:ring-1 focus:ring-theme-accent/30",
                                "transition-colors"
                            )}
                        />
                    </div>

                    {/* 替换当前 */}
                    <button
                        onClick={handleReplace}
                        disabled={matchCount === 0}
                        className={cn(
                            "px-2 h-7 rounded text-xs font-medium transition-colors",
                            matchCount > 0
                                ? "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover border border-theme-border"
                                : "text-theme-text-muted/30 cursor-not-allowed border border-theme-border/30"
                        )}
                        title={t('ide.replace')}
                    >
                        {t('ide.replace_btn')}
                    </button>

                    {/* 替换全部 */}
                    <button
                        onClick={handleReplaceAll}
                        disabled={matchCount === 0}
                        className={cn(
                            "px-2 h-7 rounded text-xs font-medium transition-colors flex items-center gap-1",
                            matchCount > 0
                                ? "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover border border-theme-border"
                                : "text-theme-text-muted/30 cursor-not-allowed border border-theme-border/30"
                        )}
                        title={t('ide.replace_all')}
                    >
                        <ReplaceAll className="w-3 h-3" />
                        {t('ide.replace_all_btn')}
                    </button>
                </div>
            )}
        </div>
    );
}
