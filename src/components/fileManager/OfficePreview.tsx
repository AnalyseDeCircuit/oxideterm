/**
 * Office Document Preview Component
 * Supports Word (.docx), Excel (.xlsx), and PowerPoint (.pptx) files
 * 
 * Libraries are loaded dynamically to reduce initial bundle size:
 * - mammoth (~400KB) - only loaded when viewing Word documents
 * - xlsx (~400KB) - only loaded when viewing Excel documents
 */

import React, { useState, useEffect, useMemo } from 'react';
import { FileText, Table, FileJson, Loader2 } from 'lucide-react';

// Dynamic import types for Office libraries
type MammothModule = typeof import('mammoth');
type XLSXModule = typeof import('xlsx');

export interface OfficePreviewProps {
  data?: string; // base64 encoded Office file (legacy)
  url?: string;  // asset:// URL to stream from disk (preferred)
  mimeType: string;
  filename: string;
  className?: string;
}

/**
 * Detect Office document type from MIME type or filename
 */
function getOfficeType(mimeType: string, filename: string): 'word' | 'excel' | 'powerpoint' | null {
  const lowerFilename = filename.toLowerCase();
  const lowerMime = mimeType.toLowerCase();

  // Check by MIME type first (more accurate)
  // Excel MIME types: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet
  //                    application/vnd.ms-excel
  if (lowerMime.includes('spreadsheetml.sheet') || lowerMime.endsWith('vnd.ms-excel') ||
      lowerFilename.endsWith('.xlsx') || lowerFilename.endsWith('.xls') || lowerFilename.endsWith('.xlsm')) {
    return 'excel';
  }

  // PowerPoint MIME types: application/vnd.openxmlformats-officedocument.presentationml.presentation
  //                        application/vnd.ms-powerpoint
  if (lowerMime.includes('presentationml.presentation') || lowerMime.endsWith('vnd.ms-powerpoint') ||
      lowerFilename.endsWith('.pptx') || lowerFilename.endsWith('.ppt')) {
    return 'powerpoint';
  }

  // Word MIME types: application/vnd.openxmlformats-officedocument.wordprocessingml.document
  //                   application/msword
  if (lowerMime.includes('wordprocessingml.document') || lowerMime.endsWith('application/msword') ||
      lowerMime.includes('officedocument.word') || lowerFilename.endsWith('.docx') || lowerFilename.endsWith('.doc')) {
    return 'word';
  }

  // Fallback: check for older MIME type patterns
  if (lowerMime.includes('sheet') || lowerMime.includes('excel')) {
    return 'excel';
  }
  if (lowerMime.includes('presentation') || lowerMime.includes('powerpoint') || lowerMime.includes('ppt')) {
    return 'powerpoint';
  }
  if (lowerMime.includes('word') || lowerMime.includes('document')) {
    return 'word';
  }

  return null;
}

/**
 * Convert base64 to ArrayBuffer
 */
function base64ToArrayBuffer(base64: string): ArrayBuffer {
  const binaryString = atob(base64);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes.buffer;
}

/**
 * Word Document Preview (.docx only, .doc not supported)
 */
function WordPreview({ arrayBuffer, filename }: { arrayBuffer: ArrayBuffer; filename: string }) {
  // Check for legacy .doc format
  const isLegacyFormat = filename.toLowerCase().endsWith('.doc');

  if (isLegacyFormat) {
    return (
      <div className="flex flex-col items-center justify-center h-64 gap-4">
        <FileText className="h-12 w-12 text-zinc-500" />
        <p className="text-zinc-400">Legacy Word format (.doc) not supported</p>
        <p className="text-sm text-zinc-500">Please convert to .docx or download to view</p>
      </div>
    );
  }

  const [html, setHtml] = useState<string>('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Dynamic import mammoth only when needed
    import('mammoth').then((mammoth: MammothModule) => {
      mammoth.convertToHtml({ arrayBuffer })
        .then(result => {
          setHtml(result.value);
          setLoading(false);
        })
        .catch(err => {
          setError(err.message);
          setLoading(false);
        });
    }).catch(err => {
      setError(`Failed to load Word parser: ${err.message}`);
      setLoading(false);
    });
  }, [arrayBuffer]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64 text-zinc-400 gap-2">
        <Loader2 className="h-5 w-5 animate-spin" />
        Loading Word document...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-64 gap-4">
        <FileText className="h-12 w-12 text-zinc-500" />
        <p className="text-zinc-400">Failed to load Word document</p>
        <p className="text-sm text-zinc-500">{error}</p>
      </div>
    );
  }

  return (
    <div
      className="prose prose-invert prose-sm max-w-none p-6 overflow-auto"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/**
 * Excel Spreadsheet Preview (.xlsx, .xls)
 */
function ExcelPreview({ arrayBuffer, filename }: { arrayBuffer: ArrayBuffer; filename: string }) {
  const [xlsxModule, setXlsxModule] = useState<XLSXModule | null>(null);
  const [workbook, setWorkbook] = useState<ReturnType<XLSXModule['read']> | null>(null);
  const [activeSheet, setActiveSheet] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Dynamic import xlsx only when needed
    import('xlsx').then((XLSX: XLSXModule) => {
      setXlsxModule(XLSX);
      try {
        // Support both .xlsx and .xls formats
        const wb = XLSX.read(arrayBuffer, { type: 'array' });
        if (!wb || !wb.SheetNames || wb.SheetNames.length === 0) {
          throw new Error('Invalid workbook: no sheets found');
        }
        setWorkbook(wb);
        setLoading(false);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to load spreadsheet';
        console.error('Excel parse error:', err);
        setError(errorMsg);
        setLoading(false);
      }
    }).catch(err => {
      setError(`Failed to load Excel parser: ${err.message}`);
      setLoading(false);
    });
  }, [arrayBuffer]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64 text-zinc-400 gap-2">
        <Loader2 className="h-5 w-5 animate-spin" />
        Loading spreadsheet...
      </div>
    );
  }

  if (error || !workbook || !xlsxModule) {
    return (
      <div className="flex flex-col items-center justify-center h-64 gap-4">
        <Table className="h-12 w-12 text-zinc-500" />
        <p className="text-zinc-400">Failed to load spreadsheet</p>
        {error && <p className="text-sm text-zinc-500">{error}</p>}
        <p className="text-xs text-zinc-600 mt-2">File: {filename}</p>
      </div>
    );
  }

  const sheetNames = workbook.SheetNames;
  const currentSheet = workbook.Sheets[sheetNames[activeSheet]];
  const html = xlsxModule.utils.sheet_to_html(currentSheet, { editable: false });

  // Inject dark theme styles for the table
  const styledHtml = html.replace(
    '<table',
    '<table style="border-collapse: collapse; width: 100%; background: #1a1a1a; color: #e4e4e7;"'
  ).replace(
    /<td/g,
    '<td style="border: 1px solid #3f3f46; padding: 6px 10px; text-align: left;'
  ).replace(
    /<th/g,
    '<th style="border: 1px solid #3f3f46; padding: 6px 10px; text-align: left; background: #27272a; font-weight: 600;"'
  ).replace(
    /<tr/g,
    '<tr style="border: 1px solid #3f3f46;"'
  );

  return (
    <div className="flex flex-col h-full">
      {/* Sheet tabs */}
      <div className="flex border-b border-theme-border bg-theme-bg-panel">
        {sheetNames.map((name, index) => (
          <button
            key={name}
            onClick={() => setActiveSheet(index)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              index === activeSheet
                ? 'border-theme-accent text-theme-accent'
                : 'border-transparent text-zinc-400 hover:text-zinc-300'
            }`}
          >
            {name}
          </button>
        ))}
      </div>

      {/* Spreadsheet content */}
      <div
        className="flex-1 overflow-auto p-4"
        dangerouslySetInnerHTML={{ __html: styledHtml }}
        style={{
          fontSize: '13px',
        }}
      />
    </div>
  );
}

/**
 * PowerPoint Preview (.pptx not supported, .ppt legacy format also not supported)
 */
function PowerPointPreview({ filename }: { filename: string }) {
  const isLegacyFormat = filename.toLowerCase().endsWith('.ppt');
  const formatName = isLegacyFormat ? 'Legacy PowerPoint format (.ppt)' : 'PowerPoint';

  return (
    <div className="flex flex-col items-center justify-center h-64 gap-4">
      <FileJson className="h-12 w-12 text-zinc-500" />
      <p className="text-zinc-400">{formatName} preview not supported</p>
      <p className="text-sm text-zinc-500">Please download {filename} to view</p>
    </div>
  );
}

/**
 * Main Office Preview Component
 */
export const OfficePreview: React.FC<OfficePreviewProps> = ({
  data,
  url,
  mimeType,
  filename,
  className,
}) => {
  const officeType = useMemo(() => getOfficeType(mimeType, filename), [mimeType, filename]);
  const [arrayBuffer, setArrayBuffer] = useState<ArrayBuffer | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (url) {
      // Fetch binary from asset:// URL — avoids IPC overhead
      fetch(url)
        .then(r => r.arrayBuffer())
        .then(buf => { if (!cancelled) { setArrayBuffer(buf); setLoading(false); } })
        .catch(() => { if (!cancelled) { setError('Failed to fetch document'); setLoading(false); } });
    } else if (data) {
      try {
        const buffer = base64ToArrayBuffer(data);
        setArrayBuffer(buffer);
        setLoading(false);
      } catch {
        setError('Failed to decode file data');
        setLoading(false);
      }
    } else {
      setError('No data or url provided');
      setLoading(false);
    }
    return () => { cancelled = true; };
  }, [data, url]);

  if (loading) {
    return (
      <div className={className}>
        <div className="flex items-center justify-center h-64 text-zinc-400">
          Loading document...
        </div>
      </div>
    );
  }

  if (error || !arrayBuffer) {
    return (
      <div className={className}>
        <div className="flex flex-col items-center justify-center h-64 gap-4">
          <FileText className="h-12 w-12 text-zinc-500" />
          <p className="text-zinc-400">Failed to load document</p>
          {error && <p className="text-sm text-zinc-500">{error}</p>}
        </div>
      </div>
    );
  }

  if (!officeType) {
    return (
      <div className={className}>
        <div className="flex flex-col items-center justify-center h-64 gap-4">
          <FileJson className="h-12 w-12 text-zinc-500" />
          <p className="text-zinc-400">Unsupported Office document type</p>
          <p className="text-sm text-zinc-500">{mimeType}</p>
        </div>
      </div>
    );
  }

  return (
    <div className={`office-preview ${className || ''}`}>
      {officeType === 'word' && <WordPreview arrayBuffer={arrayBuffer} filename={filename} />}
      {officeType === 'excel' && <ExcelPreview arrayBuffer={arrayBuffer} filename={filename} />}
      {officeType === 'powerpoint' && <PowerPointPreview filename={filename} />}
    </div>
  );
};

export default OfficePreview;
