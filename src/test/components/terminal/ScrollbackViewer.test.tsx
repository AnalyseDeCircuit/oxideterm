// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const apiMocks = vi.hoisted(() => ({
  getBufferStats: vi.fn(),
  getScrollBuffer: vi.fn(),
  clearBuffer: vi.fn(),
  startTerminalHistorySearch: vi.fn(),
  getTerminalHistorySearchResults: vi.fn(),
  cancelTerminalHistorySearch: vi.fn(),
  startTerminalSearchModel: vi.fn(),
  getTerminalSearchModelSnapshot: vi.fn(),
  selectTerminalSearchMatch: vi.fn(),
  stepTerminalSearchMatch: vi.fn(),
  closeTerminalSearchModel: vi.fn(),
  getArchivedHistoryExcerpt: vi.fn(),
  getCommandFacts: vi.fn(),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) => {
      const labels: Record<string, string> = {
        'terminal.scrollback_viewer.title': 'Scrollback Viewer',
        'terminal.scrollback_viewer.open': 'Open Scrollback Viewer',
        'terminal.scrollback_viewer.live_hint': 'Showing live scrollback. Archived matches open as excerpts.',
        'terminal.scrollback_viewer.search_placeholder': 'Search live and archived history...',
        'terminal.scrollback_viewer.search': 'Search',
        'terminal.scrollback_viewer.clear': 'Clear scrollback',
        'terminal.scrollback_viewer.clear_confirm_title': 'Clear current scrollback buffer?',
        'terminal.scrollback_viewer.clear_confirm_description': 'Archived history is not removed.',
        'terminal.scrollback_viewer.clear_confirm_action': 'Clear scrollback',
        'terminal.scrollback_viewer.live_buffer_badge': 'Live buffer',
        'terminal.scrollback_viewer.archive_badge': 'Archive',
        'terminal.scrollback_viewer.archive_excerpt': 'Archive excerpt',
        'terminal.scrollback_viewer.line_number': `Line ${params?.line ?? ''}`,
      };
      return labels[key] ?? key;
    },
  }),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector({
    settings: {
      terminal: {
        fontFamily: 'jetbrains',
        customFontFamily: '',
        fontSize: 13,
        lineHeight: 1.2,
      },
    },
  }),
}));

vi.mock('@/hooks/useConfirm', () => ({
  useConfirm: () => ({
    confirm: vi.fn(() => Promise.resolve(true)),
    ConfirmDialog: null,
  }),
}));

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (options: { count: number }) => ({
    getVirtualItems: () => {
      if (options.count <= 0) return [];
      return [
        { index: 0, key: '0', start: 0, size: 22 },
        { index: options.count - 1, key: String(options.count - 1), start: (options.count - 1) * 22, size: 22 },
      ];
    },
    getTotalSize: () => options.count * 22,
    scrollToIndex: vi.fn(),
  }),
}));

import { ScrollbackViewer, enforceScrollbackPageCacheLimit } from '@/components/terminal/ScrollbackViewer';

describe('ScrollbackViewer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      disconnect() {}
    });
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation((contextId: string) => {
      if (contextId !== '2d') return null;
      return {
        clearRect: vi.fn(),
        fillRect: vi.fn(),
        fillStyle: '',
      } as unknown as RenderingContext;
    });
    vi.spyOn(HTMLCanvasElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      width: 12,
      height: 220,
      top: 0,
      right: 12,
      bottom: 220,
      left: 0,
      toJSON: () => ({}),
    });
    apiMocks.getBufferStats.mockResolvedValue({
      current_lines: 800,
      total_lines: 1200,
      max_lines: 800,
      memory_usage_mb: 0.1,
    });
    apiMocks.getScrollBuffer.mockResolvedValue([{ text: 'tail', timestamp: 1 }]);
    apiMocks.getCommandFacts.mockResolvedValue([]);
    apiMocks.clearBuffer.mockResolvedValue(undefined);
    apiMocks.startTerminalHistorySearch.mockResolvedValue({ search_id: 'search-1' });
    apiMocks.getTerminalHistorySearchResults.mockResolvedValue({
      search_id: 'search-1',
      session_id: 'session-1',
      cursor: 0,
      next_cursor: 0,
      matches: [],
      total_buffered_matches: 0,
      total_matches: 0,
      duration_ms: 0,
      searched_layers: [],
      searched_chunks: 0,
      truncated: false,
      partial_failure: false,
      archive_status: {
        available: false,
        degraded: false,
        closing: false,
        queued_commands: 0,
        max_queue_depth: 0,
        dropped_appends: 0,
        dropped_lines: 0,
        sealed_chunks: 0,
      },
      done: true,
    });
    apiMocks.cancelTerminalHistorySearch.mockResolvedValue(undefined);
    apiMocks.startTerminalSearchModel.mockResolvedValue({ search_id: 'search-1' });
    apiMocks.getTerminalSearchModelSnapshot.mockResolvedValue({
      search_id: 'search-1',
      session_id: 'session-1',
      query: '',
      options: { query: '', case_sensitive: false, regex: false, whole_word: false, max_matches: 1000 },
      revision: 1,
      created_at: 1,
      updated_at: 1,
      loading: false,
      done: true,
      matches: [],
      max_matches: 1000,
      total_matches: 0,
      total_buffered_matches: 0,
      duration_ms: 0,
      searched_layers: [],
      searched_chunks: 0,
      truncated: false,
      partial_failure: false,
      archive_status: {
        available: false,
        degraded: false,
        closing: false,
        queued_commands: 0,
        max_queue_depth: 0,
        dropped_appends: 0,
        dropped_lines: 0,
        sealed_chunks: 0,
      },
      hot_match_count: 0,
      cold_match_count: 0,
    });
    apiMocks.selectTerminalSearchMatch.mockImplementation(async (_searchId: string, matchIndex: number) => {
      const snapshot = await apiMocks.getTerminalSearchModelSnapshot();
      return { ...snapshot, active_match_index: matchIndex, active_match: snapshot.matches[matchIndex] };
    });
    apiMocks.stepTerminalSearchMatch.mockImplementation(async () => apiMocks.getTerminalSearchModelSnapshot());
    apiMocks.closeTerminalSearchModel.mockResolvedValue(undefined);
    apiMocks.getArchivedHistoryExcerpt.mockResolvedValue({
      chunk_id: 'chunk-1',
      start_line_number: 40,
      end_line_number: 42,
      lines: [{ line_number: 41, text: 'archived hit', is_match: true }],
    });
  });

  it('converts global cache ranges to hot buffer indexes before paging', async () => {
    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        onClose={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(apiMocks.getScrollBuffer).toHaveBeenCalledWith('session-1', 400, 400);
    });
  });

  it('uses terminal font inheritance with a compact live-row gutter', async () => {
    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        onClose={vi.fn()}
      />,
    );

    await screen.findByText('tail');
    const livePre = screen
      .getAllByTestId('scrollback-live-line-text')
      .find((element) => element.textContent?.includes('tail'));
    const liveRow = livePre?.closest('[data-testid="scrollback-live-row"]');

    expect(livePre?.style.fontFamily).toBe('inherit');
    expect(livePre?.style.fontSize).toBe('inherit');
    expect(livePre?.style.lineHeight).toBe('inherit');
    expect(liveRow?.className).toContain('grid-cols-[3.25rem_minmax(0,1fr)]');
  });

  it('loads command facts for the live window and marks matching rows', async () => {
    apiMocks.getCommandFacts.mockResolvedValueOnce([{
      factId: 'fact-1',
      clientMarkId: 'mark-1',
      sessionId: 'session-1',
      nodeId: 'node-1',
      source: 'command_bar',
      command: 'tail',
      startGlobalLine: 400,
      commandGlobalLine: 400,
      endGlobalLine: 400,
      bufferGeneration: 0,
      runtimeEpoch: 'test-runtime',
      status: 'closed',
      confidence: 'high',
      createdAt: 1,
      closedAt: 2,
    }]);

    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        onClose={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(apiMocks.getCommandFacts).toHaveBeenCalledWith('session-1', 400, 1199);
    });
    const liveText = await screen.findByText('tail');
    await waitFor(() => {
      expect(liveText.closest('div')?.className).toContain('border-theme-accent');
    });
  });

  it('clears live scrollback only after confirmation and refreshes stats', async () => {
    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        onClose={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTitle('Clear scrollback'));

    await waitFor(() => {
      expect(apiMocks.clearBuffer).toHaveBeenCalledWith('session-1');
    });
    expect(apiMocks.getBufferStats).toHaveBeenCalledTimes(2);
  });

  it('shows archive matches as excerpts instead of scrolling the live list', async () => {
    apiMocks.getTerminalSearchModelSnapshot.mockResolvedValueOnce({
      search_id: 'search-1',
      session_id: 'session-1',
      query: 'archived',
      options: { query: 'archived', case_sensitive: false, regex: false, whole_word: false, max_matches: 1000 },
      revision: 1,
      created_at: 1,
      updated_at: 1,
      loading: false,
      done: true,
      matches: [{
        match_index: 0,
        source: 'cold',
        line_number: 41,
        column_start: 0,
        column_end: 8,
        matched_text: 'archived',
        line_content: 'archived hit',
        chunk_id: 'chunk-1',
      }],
      active_match_index: 0,
      active_match: {
        match_index: 0,
        source: 'cold',
        line_number: 41,
        column_start: 0,
        column_end: 8,
        matched_text: 'archived',
        line_content: 'archived hit',
        chunk_id: 'chunk-1',
      },
      max_matches: 1000,
      total_matches: 1,
      total_buffered_matches: 1,
      duration_ms: 1,
      searched_layers: ['cold'],
      searched_chunks: 1,
      truncated: false,
      partial_failure: false,
      archive_status: {
        available: true,
        degraded: false,
        closing: false,
        queued_commands: 0,
        max_queue_depth: 0,
        dropped_appends: 0,
        dropped_lines: 0,
        sealed_chunks: 1,
      },
      hot_match_count: 0,
      cold_match_count: 1,
      excerpt: {
        chunk_id: 'chunk-1',
        start_line_number: 40,
        end_line_number: 42,
        lines: [{ line_number: 41, text: 'archived hit', is_match: true }],
      },
    });

    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        onClose={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByPlaceholderText('Search live and archived history...'), {
      target: { value: 'archived' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));

    await waitFor(() => {
      expect(apiMocks.getTerminalSearchModelSnapshot).toHaveBeenCalledWith('search-1');
    });
    expect(apiMocks.getArchivedHistoryExcerpt).not.toHaveBeenCalled();
    expect(await screen.findByText('Archive excerpt')).toBeInTheDocument();
    expect((await screen.findAllByText('archived hit')).length).toBeGreaterThan(0);
  });

  it('opens directly on an external archived match excerpt', async () => {
    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        initialMatch={{
          source: 'cold',
          line_number: 41,
          column_start: 0,
          column_end: 8,
          matched_text: 'archived',
          line_content: 'archived hit',
          chunk_id: 'chunk-1',
        }}
        onClose={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(apiMocks.getArchivedHistoryExcerpt).toHaveBeenCalledWith('session-1', 'chunk-1', 41, 6);
    });
    expect(await screen.findByText('Archive excerpt')).toBeInTheDocument();
  });

  it('opens directly on an external live match and requests its hot-buffer page', async () => {
    render(
      <ScrollbackViewer
        sessionId="session-1"
        nodeId="node-1"
        isOpen
        initialMatch={{
          source: 'hot',
          line_number: 1199,
          column_start: 0,
          column_end: 4,
          matched_text: 'tail',
          line_content: 'tail',
        }}
        onClose={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(apiMocks.getScrollBuffer).toHaveBeenCalledWith('session-1', 400, 400);
    });
  });

  it('evicts least recently used pages while protecting the current viewport neighborhood', () => {
    const pages = new Map<number, { lastAccessedAt: number }>();
    for (let index = 0; index < 12; index += 1) {
      pages.set(index * 800, { lastAccessedAt: index });
    }

    const limited = enforceScrollbackPageCacheLimit(
      pages,
      new Set([0, 800]),
      10,
    );

    expect(limited.size).toBe(10);
    expect(limited.has(0)).toBe(true);
    expect(limited.has(800)).toBe(true);
    expect(limited.has(1600)).toBe(false);
    expect(limited.has(2400)).toBe(false);
  });
});
