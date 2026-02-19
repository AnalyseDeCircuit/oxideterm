import React, { useState, useEffect, useRef } from 'react';
import { Search, X, ChevronUp, ChevronDown, CaseSensitive, Regex, WholeWord, History, Loader2 } from 'lucide-react';
import { Input } from '../ui/input';
import { Button } from '../ui/button';
import { Checkbox } from '../ui/checkbox';
import { Label } from '../ui/label';
import { SearchMatch } from '../../types';
import { useTranslation } from 'react-i18next';

export type SearchMode = 'active' | 'deep';

export interface DeepSearchState {
  loading: boolean;
  matches: SearchMatch[];
  totalMatches: number;
  durationMs: number;
  error?: string;
}

interface SearchBarProps {
  isOpen: boolean;
  onClose: () => void;
  onSearch: (query: string, options: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) => void;
  onFindNext: () => void;
  onFindPrevious: () => void;
  resultIndex: number;  // -1 if no results or limit exceeded
  resultCount: number;
  // Deep history search (optional - not available for local terminals)
  onDeepSearch?: (query: string, options: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) => void;
  onJumpToMatch?: (match: SearchMatch) => void;
  deepSearchState?: DeepSearchState;
  // Whether to show deep search mode tab (default: true if onDeepSearch is provided)
  showDeepSearch?: boolean;
}

export const SearchBar: React.FC<SearchBarProps> = ({ 
  isOpen, 
  onClose,
  onSearch,
  onFindNext,
  onFindPrevious,
  resultIndex,
  resultCount,
  onDeepSearch,
  onJumpToMatch,
  deepSearchState,
  showDeepSearch,
}) => {
  const { t } = useTranslation();
  // Determine if deep search should be shown
  const canDeepSearch = showDeepSearch !== false && !!onDeepSearch;
  
  const [query, setQuery] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [useRegex, setUseRegex] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [searchMode, setSearchMode] = useState<SearchMode>('active');
  const inputRef = useRef<HTMLInputElement>(null);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const resultsListRef = useRef<HTMLDivElement>(null);
  // Track IME composition state (for CJK input methods)
  const isComposingRef = useRef(false);
  // Ignore the next Enter after IME composition end (prevents double-trigger)
  const ignoreNextEnterRef = useRef(false);
  // Skip the next debounced search after IME composition end (prevents double search)
  const skipNextDebouncedSearchRef = useRef(false);

  // Focus input when opened
  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isOpen]);

  // Debounced search - triggers on query or options change
  useEffect(() => {
    if (!isOpen) return;
    
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    // Only do active search in active mode
    if (searchMode !== 'active') return;

    searchTimeoutRef.current = setTimeout(() => {
      // Skip search if IME is composing (prevents jumping during CJK input)
      if (isComposingRef.current) return;
      if (skipNextDebouncedSearchRef.current) {
        skipNextDebouncedSearchRef.current = false;
        return;
      }
      onSearch(query, { caseSensitive, regex: useRegex, wholeWord });
    }, 150); // Faster debounce for better responsiveness

    return () => {
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [query, caseSensitive, useRegex, wholeWord, isOpen, onSearch, searchMode]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!isOpen) return;
      if (!document.hasFocus()) return;

      // Esc to close
      if (e.key === 'Escape') {
        onClose();
        e.preventDefault();
        return;
      }

      // Enter to go to next match, Shift+Enter for previous (active mode only)
      // BUT skip if composition just ended or IME is composing (the Enter was to confirm IME)
      if (e.key === 'Enter' && searchMode === 'active' && resultCount > 0) {
        const nativeEvent = e as KeyboardEvent;
        const isNativeComposing =
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (nativeEvent as any)?.isComposing === true ||
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (nativeEvent as any)?.keyCode === 229;
        if (isNativeComposing || ignoreNextEnterRef.current || isComposingRef.current) {
          ignoreNextEnterRef.current = false;
          e.preventDefault();
          return;
        }
        if (e.shiftKey) {
          onFindPrevious();
        } else {
          onFindNext();
        }
        e.preventDefault();
      }
      
      // Enter to trigger deep search in deep mode
      if (e.key === 'Enter' && searchMode === 'deep' && query.trim() && onDeepSearch) {
        onDeepSearch(query, { caseSensitive, regex: useRegex, wholeWord });
        e.preventDefault();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, resultCount, onFindNext, onFindPrevious, onClose, searchMode, query, caseSensitive, useRegex, wholeWord, onDeepSearch]);

  if (!isOpen) return null;

  // Prevent terminal from stealing focus
  const handleKeyDown = (e: React.KeyboardEvent) => {
    e.stopPropagation();
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    e.stopPropagation();
  };

  // Format result display
  const getResultDisplay = () => {
    if (!query.trim()) return null;
    if (searchMode === 'deep') {
      if (deepSearchState?.loading) return t('terminal.search.searching');
      if (deepSearchState?.error) return t('terminal.search.error');
      if (deepSearchState?.totalMatches === 0) return t('terminal.search.no_results_history');
      if (deepSearchState?.totalMatches) return t('terminal.search.matches_count', { count: deepSearchState.totalMatches, ms: deepSearchState.durationMs });
      return null;
    }
    // Active mode
    if (resultCount === 0) return t('terminal.search.no_results');
    if (resultIndex === -1) return t('terminal.search.matches_exceeded', { count: resultCount }); // Limit exceeded
    return `${resultIndex + 1}/${resultCount}`;
  };
  
  // Handle mode switch
  const handleModeChange = (newMode: SearchMode) => {
    setSearchMode(newMode);
    // Clear active search decorations when switching to deep mode
    if (newMode === 'deep') {
      onSearch('', {}); // Clear active search
    }
  };
  
  // Handle deep search button click
  const handleDeepSearchClick = () => {
    if (query.trim() && onDeepSearch) {
      onDeepSearch(query, { caseSensitive, regex: useRegex, wholeWord });
    }
  };
  
  // Truncate line content for display
  const truncateLine = (text: string, match: SearchMatch, maxLength: number = 60) => {
    // Center around the match
    const matchStart = match.column_start;
    const matchEnd = match.column_end;
    const matchLen = matchEnd - matchStart;
    
    if (text.length <= maxLength) return text;
    
    const contextBefore = Math.floor((maxLength - matchLen) / 2);
    const start = Math.max(0, matchStart - contextBefore);
    const end = Math.min(text.length, start + maxLength);
    
    let result = text.slice(start, end);
    if (start > 0) result = '...' + result;
    if (end < text.length) result = result + '...';
    
    return result;
  };

  return (
    <div 
      className="absolute top-4 right-4 z-50 w-96 bg-theme-bg-panel border border-theme-border rounded-sm shadow-2xl"
      onKeyDown={handleKeyDown}
      onMouseDown={handleMouseDown}
    >
      {/* Mode Tabs - only show if deep search is available */}
      {canDeepSearch && (
        <div className="flex border-b border-theme-border">
          <button
            className={`flex-1 px-3 py-1.5 text-xs font-medium transition-colors ${
              searchMode === 'active' 
                ? 'bg-theme-bg-hover text-theme-text border-b-2 border-theme-accent' 
                : 'text-theme-text-muted hover:text-theme-text'
            }`}
            onClick={() => handleModeChange('active')}
          >
            <Search className="w-3 h-3 inline mr-1" />
            {t('terminal.search.visible_buffer')}
          </button>
          <button
            className={`flex-1 px-3 py-1.5 text-xs font-medium transition-colors ${
              searchMode === 'deep' 
                ? 'bg-theme-bg-hover text-theme-text border-b-2 border-theme-accent' 
                : 'text-theme-text-muted hover:text-theme-text'
            }`}
            onClick={() => handleModeChange('deep')}
            title={t('terminal.search.deep_history_tooltip')}
          >
            <History className="w-3 h-3 inline mr-1" />
            {t('terminal.search.deep_history')}
          </button>
        </div>
      )}
      
      {/* Main Search Row */}
      <div className="flex items-center gap-2 p-3 border-b border-theme-border">
        <Search className="w-4 h-4 text-theme-text-muted" />
        <Input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onCompositionStart={() => {
            isComposingRef.current = true;
          }}
          onCompositionEnd={(e) => {
            isComposingRef.current = false;
            // Ignore the next Enter - it was used to confirm IME input
            ignoreNextEnterRef.current = true;
            // Skip the debounced search once - we'll trigger immediately here
            skipNextDebouncedSearchRef.current = true;
            // Trigger search after IME composition ends
            if (searchMode === 'active') {
              onSearch(e.currentTarget.value, { caseSensitive, regex: useRegex, wholeWord });
            }
          }}
          placeholder={searchMode === 'active' ? t('terminal.search.placeholder_active') : t('terminal.search.placeholder_deep')}
          className="flex-1 h-8 text-sm border-0 focus-visible:ring-0 bg-transparent"
        />
        
        {/* Match Counter */}
        {query.trim() && (
          <div className="text-xs text-theme-text-muted whitespace-nowrap">
            {getResultDisplay()}
          </div>
        )}

        {/* Navigation Buttons - only show in active mode */}
        {searchMode === 'active' && (
          <>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0"
              onClick={onFindPrevious}
              disabled={resultCount === 0}
              title={t('terminal.search.previous_match')}
            >
              <ChevronUp className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0"
              onClick={onFindNext}
              disabled={resultCount === 0}
              title={t('terminal.search.next_match')}
            >
              <ChevronDown className="h-4 w-4" />
            </Button>
          </>
        )}
        
        {/* Deep Search Button - only show in deep mode when available */}
        {searchMode === 'deep' && canDeepSearch && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={handleDeepSearchClick}
            disabled={!query.trim() || deepSearchState?.loading}
            title={t('terminal.search.search_full_history')}
          >
            {deepSearchState?.loading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              t('terminal.search.search_button')
            )}
          </Button>
        )}

        {/* Close Button */}
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={onClose}
          title={t('terminal.search.close')}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Options Row */}
      <div className="flex items-center gap-4 px-3 py-2 bg-theme-bg">
        {/* Case Sensitive */}
        <div className="flex items-center gap-1.5">
          <Checkbox
            id="case-sensitive"
            checked={caseSensitive}
            onCheckedChange={(checked: boolean) => setCaseSensitive(checked === true)}
          />
          <Label 
            htmlFor="case-sensitive" 
            className="text-xs cursor-pointer flex items-center gap-1 text-theme-text-muted"
            title={t('terminal.search.case_sensitive')}
          >
            <CaseSensitive className="w-3.5 h-3.5" />
            <span>Aa</span>
          </Label>
        </div>

        {/* Regex */}
        <div className="flex items-center gap-1.5">
          <Checkbox
            id="regex"
            checked={useRegex}
            onCheckedChange={(checked: boolean) => setUseRegex(checked === true)}
          />
          <Label 
            htmlFor="regex" 
            className="text-xs cursor-pointer flex items-center gap-1 text-theme-text-muted"
            title={t('terminal.search.regex')}
          >
            <Regex className="w-3.5 h-3.5" />
            <span>.*</span>
          </Label>
        </div>

        {/* Whole Word */}
        <div className="flex items-center gap-1.5">
          <Checkbox
            id="whole-word"
            checked={wholeWord}
            onCheckedChange={(checked: boolean) => setWholeWord(checked === true)}
          />
          <Label 
            htmlFor="whole-word" 
            className="text-xs cursor-pointer flex items-center gap-1 text-theme-text-muted"
            title={t('terminal.search.whole_word')}
          >
            <WholeWord className="w-3.5 h-3.5" />
            <span>Word</span>
          </Label>
        </div>
      </div>
      
      {/* Deep Search Results List */}
      {searchMode === 'deep' && deepSearchState && !deepSearchState.loading && deepSearchState.matches.length > 0 && (
        <div 
          ref={resultsListRef}
          className="max-h-64 overflow-y-auto border-t border-theme-border"
        >
          <div className="text-xs text-theme-text-muted px-3 py-1 bg-theme-bg sticky top-0">
            {t('terminal.search.click_to_jump')}
          </div>
          {deepSearchState.matches.map((match, idx) => (
            <button
              key={`${match.line_number}-${match.column_start}-${idx}`}
              className="w-full text-left px-3 py-2 hover:bg-theme-bg-hover border-b border-theme-border transition-colors"
              onClick={() => onJumpToMatch?.(match)}
            >
              <div className="flex items-center justify-between text-xs text-theme-text-muted mb-1">
                <span className="font-mono">{t('terminal.search.line_number', { line: match.line_number + 1 })}</span>
              </div>
              <div className="text-sm font-mono text-theme-text truncate">
                {truncateLine(match.line_content, match)}
              </div>
            </button>
          ))}
          {deepSearchState.totalMatches > deepSearchState.matches.length && (
            <div className="text-xs text-theme-text-muted px-3 py-2 text-center">
              {t('terminal.search.showing_first', { total: deepSearchState.totalMatches })}
            </div>
          )}
        </div>
      )}
      
      {/* Deep Search Error */}
      {searchMode === 'deep' && deepSearchState?.error && (
        <div className="px-3 py-2 bg-red-900/20 border-t border-red-800 text-red-400 text-xs">
          {deepSearchState.error}
        </div>
      )}
      
      {/* Deep Search No Results */}
      {searchMode === 'deep' && deepSearchState && !deepSearchState.loading && deepSearchState.totalMatches === 0 && query.trim() && (
        <div className="px-3 py-2 bg-theme-bg border-t border-theme-border text-theme-text-muted text-xs text-center">
          {t('terminal.search.no_matches_in_history')}
        </div>
      )}
    </div>
  );
};
