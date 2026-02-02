/**
 * VirtualTextPreview
 * Streaming + virtualized preview for large text/code files
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import Prism from 'prismjs';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../../store/settingsStore';
import { getFontFamilyCSS } from './fontUtils';
import './prismLanguages';

interface FileChunk {
  data: number[];
  eof: boolean;
}

export interface VirtualTextPreviewProps {
  path: string;
  size: number;
  language?: string;
  showLineNumbers?: boolean;
  highlight?: boolean;
  className?: string;
}

const CHUNK_SIZE = 128 * 1024; // 128KB
const OVERSCAN_LINES = 20;
const PREFETCH_LINES = 60;

export const VirtualTextPreview: React.FC<VirtualTextPreviewProps> = ({
  path,
  size,
  language,
  showLineNumbers = true,
  highlight = false,
  className,
}) => {
  const { t } = useTranslation();
  const fontFamily = useSettingsStore(s => s.settings.terminal.fontFamily);
  const fontSize = useSettingsStore(s => s.settings.terminal.fontSize);
  const lineHeight = useSettingsStore(s => s.settings.terminal.lineHeight) || 1.5;

  const containerRef = useRef<HTMLDivElement>(null);
  const decoderRef = useRef<TextDecoder>(new TextDecoder());
  const carryRef = useRef<string>('');
  const [lines, setLines] = useState<string[]>([]);
  const [offset, setOffset] = useState<number>(0);
  const [eof, setEof] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(false);
  const [scrollTop, setScrollTop] = useState<number>(0);
  const [viewportHeight, setViewportHeight] = useState<number>(0);

  const linePx = useMemo(() => Math.max(14, Math.round(fontSize * lineHeight)), [fontSize, lineHeight]);

  const reset = useCallback(() => {
    setLines([]);
    setOffset(0);
    setEof(false);
    carryRef.current = '';
    decoderRef.current = new TextDecoder();
  }, []);

  const appendChunk = useCallback((text: string, isEof: boolean) => {
    const combined = carryRef.current + text;
    const parts = combined.split('\n');

    if (!isEof) {
      // 保留最后一个不完整的行（可能跨越 chunk 边界）
      carryRef.current = parts.pop() ?? '';
      // 添加完整的行
      if (parts.length > 0) {
        setLines(prev => [...prev, ...parts]);
      }
    } else {
      // 文件结束：添加所有剩余行
      carryRef.current = '';
      setLines(prev => [...prev, ...parts]);
    }
  }, []);

  const loadMore = useCallback(async () => {
    if (loading || eof) return;
    setLoading(true);

    try {
      const length = Math.min(CHUNK_SIZE, Math.max(0, size - offset));
      if (length <= 0) {
        setEof(true);
        return;
      }

      const chunk = await invoke<FileChunk>('local_read_file_range', {
        path,
        offset,
        length,
      });

      const bytes = new Uint8Array(chunk.data);
      const decoded = decoderRef.current.decode(bytes, { stream: !chunk.eof });
      appendChunk(decoded, chunk.eof);
      setOffset(prev => prev + bytes.length);
      if (chunk.eof || bytes.length === 0) setEof(true);
    } catch (err) {
      console.error('Stream preview load error:', err);
      setEof(true);
    } finally {
      setLoading(false);
    }
  }, [appendChunk, eof, loading, offset, path, size]);

  // Initial load and reset on path change
  useEffect(() => {
    reset();
    loadMore();
  }, [path, reset, loadMore]);

  // Resize observer for viewport height
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const ro = new ResizeObserver(entries => {
      const entry = entries[0];
      if (entry) setViewportHeight(entry.contentRect.height);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Scroll handler + prefetch
  const onScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    const target = e.currentTarget;
    const nextTop = target.scrollTop;
    setScrollTop(nextTop);

    const remaining = target.scrollHeight - (nextTop + target.clientHeight);
    if (remaining < linePx * PREFETCH_LINES) {
      loadMore();
    }
  }, [linePx, loadMore]);

  const visibleRange = useMemo(() => {
    const start = Math.max(0, Math.floor(scrollTop / linePx) - OVERSCAN_LINES);
    const end = Math.min(lines.length, Math.ceil((scrollTop + viewportHeight) / linePx) + OVERSCAN_LINES);
    return { start, end };
  }, [scrollTop, viewportHeight, lines.length, linePx]);

  const visibleLines = useMemo(() => lines.slice(visibleRange.start, visibleRange.end), [lines, visibleRange]);

  const highlightedLines = useMemo(() => {
    if (!highlight || !language) {
      return visibleLines.map(line => escapeHtml(line || ' '));
    }

    const grammar = Prism.languages[language];
    if (!grammar) {
      return visibleLines.map(line => escapeHtml(line || ' '));
    }

    return visibleLines.map(line => {
      try {
        return Prism.highlight(line || ' ', grammar, language);
      } catch {
        return escapeHtml(line || ' ');
      }
    });
  }, [highlight, language, visibleLines]);

  const paddingTop = visibleRange.start * linePx;
  const paddingBottom = Math.max(0, (lines.length - visibleRange.end) * linePx);
  const gutterWidth = Math.max(lines.length.toString().length, 2);

  return (
    <div
      ref={containerRef}
      className={`overflow-auto bg-zinc-950 ${className || ''}`}
      onScroll={onScroll}
      style={{
        fontFamily: getFontFamilyCSS(fontFamily),
        fontSize: `${fontSize}px`,
        lineHeight: lineHeight,
      }}
    >
      <div style={{ paddingTop, paddingBottom }}>
        {highlightedLines.map((lineHtml, idx) => {
          const lineNumber = visibleRange.start + idx + 1;
          return (
            <div key={lineNumber} className="flex" style={{ minHeight: `${linePx}px` }}>
              {showLineNumbers && (
                <span
                  className="flex-shrink-0 text-right select-none pr-3"
                  style={{
                    width: `${gutterWidth + 1}ch`,
                    color: 'rgba(255, 255, 255, 0.3)',
                  }}
                >
                  {lineNumber}
                </span>
              )}
              <span
                className="flex-1"
                style={{ whiteSpace: 'pre' }}
                dangerouslySetInnerHTML={{ __html: lineHtml }}
              />
            </div>
          );
        })}

        {loading && (
          <div className="text-xs text-zinc-500 py-2">{t('fileManager.loadingMore', 'Loading...')}</div>
        )}

        {!loading && eof && lines.length === 0 && (
          <div className="text-xs text-zinc-500 py-2">{t('fileManager.emptyFile', 'Empty file')}</div>
        )}
      </div>
    </div>
  );
};

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
