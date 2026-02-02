/**
 * QuickLook Component
 * Smart file preview with support for images, markdown, code, archives, and more
 */

import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  X, 
  FileText, 
  Image, 
  FileCode, 
  FileQuestion,
  ExternalLink,
  Copy,
  Check,
  ZoomIn,
  ZoomOut,
  RotateCw,
  Archive,
  Folder,
  File,
  ChevronLeft,
  ChevronRight,
  Clock,
  HardDrive,
  Shield,
  Info
} from 'lucide-react';
import { Button } from '../ui/button';
import { cn } from '../../lib/utils';
import { formatUnixPermissions, formatFileSize, formatTimestamp, formatRelativeTime } from './utils';
import { CodeHighlight } from './CodeHighlight';
import { VirtualTextPreview } from './VirtualTextPreview';
import { OfficePreview } from './OfficePreview';
import type { FilePreview, PreviewType, ArchiveEntry, FileInfo } from './types';

// Format file size
const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

// Get file type icon
const getPreviewIcon = (type: PreviewType) => {
  switch (type) {
    case 'image':
      return <Image className="h-4 w-4" />;
    case 'code':
    case 'markdown':
      return <FileCode className="h-4 w-4" />;
    case 'text':
    case 'office':
      return <FileText className="h-4 w-4" />;
    case 'archive':
      return <Archive className="h-4 w-4" />;
    default:
      return <FileQuestion className="h-4 w-4" />;
  }
};

// Simple markdown renderer (basic support)
const renderMarkdown = (text: string): string => {
  let html = text
    // Escape HTML
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    // Headers
    .replace(/^### (.*$)/gim, '<h3 class="text-lg font-semibold text-zinc-200 mt-4 mb-2">$1</h3>')
    .replace(/^## (.*$)/gim, '<h2 class="text-xl font-semibold text-zinc-100 mt-5 mb-2">$1</h2>')
    .replace(/^# (.*$)/gim, '<h1 class="text-2xl font-bold text-zinc-50 mt-6 mb-3">$1</h1>')
    // Bold & Italic
    .replace(/\*\*\*(.+?)\*\*\*/g, '<strong><em>$1</em></strong>')
    .replace(/\*\*(.+?)\*\*/g, '<strong class="text-zinc-100">$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/___(.+?)___/g, '<strong><em>$1</em></strong>')
    .replace(/__(.+?)__/g, '<strong class="text-zinc-100">$1</strong>')
    .replace(/_(.+?)_/g, '<em>$1</em>')
    // Code blocks
    .replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="bg-zinc-900 rounded p-3 my-3 overflow-x-auto"><code class="text-xs text-emerald-400">$2</code></pre>')
    // Inline code
    .replace(/`([^`]+)`/g, '<code class="bg-zinc-800 px-1 py-0.5 rounded text-xs text-amber-400">$1</code>')
    // Links
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="text-theme-accent hover:underline" target="_blank" rel="noopener">$1</a>')
    // Images (rendered as placeholder)
    .replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<span class="inline-block bg-zinc-800 px-2 py-1 rounded text-xs text-zinc-400">[Image: $1]</span>')
    // Blockquotes
    .replace(/^&gt; (.*$)/gim, '<blockquote class="border-l-2 border-theme-accent pl-3 my-2 text-zinc-400 italic">$1</blockquote>')
    // Horizontal rule
    .replace(/^---$/gim, '<hr class="my-4 border-theme-border" />')
    // Lists
    .replace(/^\* (.*)$/gim, '<li class="ml-4 list-disc">$1</li>')
    .replace(/^- (.*)$/gim, '<li class="ml-4 list-disc">$1</li>')
    .replace(/^\d+\. (.*)$/gim, '<li class="ml-4 list-decimal">$1</li>')
    // Paragraphs (lines with content)
    .replace(/^(?!<[hpuol]|<li|<bl|<hr|<pre)(.+)$/gim, '<p class="my-2">$1</p>');

  return html;
};

export interface QuickLookProps {
  preview: FilePreview | null;
  onClose: () => void;
  onOpenExternal?: (path: string) => void;
  /** List of previewable files for navigation */
  fileList?: FileInfo[];
  /** Current index in the file list */
  currentIndex?: number;
  /** Callback when navigating to another file */
  onNavigate?: (file: FileInfo, index: number) => void;
}

export const QuickLook: React.FC<QuickLookProps> = ({
  preview,
  onClose,
  onOpenExternal,
  fileList,
  currentIndex,
  onNavigate,
}) => {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [imageZoom, setImageZoom] = useState(1);
  const [imageRotation, setImageRotation] = useState(0);
  const [showMetadata, setShowMetadata] = useState(true);

  // Filter file list to only include previewable files (not directories)
  const previewableFiles = useMemo(() => {
    if (!fileList) return [];
    return fileList.filter(f => f.file_type !== 'Directory');
  }, [fileList]);

  // Calculate actual current index in the previewable list
  const actualIndex = useMemo(() => {
    if (currentIndex === undefined || !preview) return -1;
    return previewableFiles.findIndex(f => f.path === preview.path);
  }, [previewableFiles, preview, currentIndex]);

  // Navigation helpers
  const canNavigate = previewableFiles.length > 1;

  // Navigate to previous/next file
  const navigatePrev = useCallback(() => {
    if (!canNavigate || !onNavigate) return;
    const newIndex = actualIndex <= 0 ? previewableFiles.length - 1 : actualIndex - 1;
    onNavigate(previewableFiles[newIndex], newIndex);
  }, [canNavigate, onNavigate, actualIndex, previewableFiles]);

  const navigateNext = useCallback(() => {
    if (!canNavigate || !onNavigate) return;
    const newIndex = actualIndex >= previewableFiles.length - 1 ? 0 : actualIndex + 1;
    onNavigate(previewableFiles[newIndex], newIndex);
  }, [canNavigate, onNavigate, actualIndex, previewableFiles]);

  // Reset states when preview changes
  useEffect(() => {
    setCopied(false);
    setImageZoom(1);
    setImageRotation(0);

  }, [preview?.path]);

  // Handle keyboard shortcuts
  useEffect(() => {
    if (!preview) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Escape or Space to close
      if (e.key === 'Escape' || e.key === ' ') {
        e.preventDefault();
        onClose();
      }
      // Left/Right arrow for navigation
      if (e.key === 'ArrowLeft' && canNavigate) {
        e.preventDefault();
        navigatePrev();
      }
      if (e.key === 'ArrowRight' && canNavigate) {
        e.preventDefault();
        navigateNext();
      }
      // Toggle metadata panel with 'i'
      if (e.key === 'i') {
        e.preventDefault();
        setShowMetadata(s => !s);
      }
      // Zoom controls for images
      if (preview.type === 'image') {
        if (e.key === '+' || e.key === '=') {
          e.preventDefault();
          setImageZoom(z => Math.min(z + 0.25, 3));
        }
        if (e.key === '-') {
          e.preventDefault();
          setImageZoom(z => Math.max(z - 0.25, 0.25));
        }
        if (e.key === '0') {
          e.preventDefault();
          setImageZoom(1);
          setImageRotation(0);
        }
        if (e.key === 'r') {
          e.preventDefault();
          setImageRotation(r => (r + 90) % 360);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [preview, onClose, canNavigate, navigatePrev, navigateNext]);

  // Copy content to clipboard
  const handleCopy = async () => {
    if (!preview?.data) return;
    try {
      await navigator.clipboard.writeText(preview.data);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  };

  // Rendered markdown content
  const markdownHtml = useMemo(() => {
    if (preview?.type !== 'markdown') return '';
    return renderMarkdown(preview.data);
  }, [preview?.type, preview?.data]);

  if (!preview) return null;

  return (
    <div 
      className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center"
      onClick={onClose}
    >
      <div 
        className="relative w-full max-w-4xl max-h-[90vh] m-4 bg-theme-bg-panel border border-theme-border rounded-lg shadow-2xl flex flex-col overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-theme-border bg-zinc-900/50">
          {/* Navigation buttons (left side) */}
          {canNavigate && (
            <div className="flex items-center gap-1 mr-2">
              <Button 
                size="icon" 
                variant="ghost" 
                className="h-7 w-7" 
                onClick={navigatePrev}
                title={t('fileManager.previousFile', 'Previous (←)')}
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
              <span className="text-xs text-zinc-500 min-w-[3rem] text-center">
                {actualIndex + 1} / {previewableFiles.length}
              </span>
              <Button 
                size="icon" 
                variant="ghost" 
                className="h-7 w-7" 
                onClick={navigateNext}
                title={t('fileManager.nextFile', 'Next (→)')}
              >
                <ChevronRight className="h-4 w-4" />
              </Button>
            </div>
          )}

          {getPreviewIcon(preview.type)}
          <div className="flex-1 min-w-0">
            <h3 className="text-sm font-medium text-zinc-200 truncate">{preview.name}</h3>
            <p className="text-xs text-zinc-500 truncate">{preview.path}</p>
          </div>
          
          {/* Actions */}
          <div className="flex items-center gap-1">
            {/* Image zoom controls */}
            {preview.type === 'image' && (
              <>
                <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => setImageZoom(z => Math.max(z - 0.25, 0.25))} title={t('fileManager.zoomOut')}>
                  <ZoomOut className="h-3.5 w-3.5" />
                </Button>
                <span className="text-xs text-zinc-500 w-12 text-center">{Math.round(imageZoom * 100)}%</span>
                <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => setImageZoom(z => Math.min(z + 0.25, 3))} title={t('fileManager.zoomIn')}>
                  <ZoomIn className="h-3.5 w-3.5" />
                </Button>
                <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => setImageRotation(r => (r + 90) % 360)} title={t('fileManager.rotate')}>
                  <RotateCw className="h-3.5 w-3.5" />
                </Button>
                <div className="w-px h-4 bg-theme-border mx-1" />
              </>
            )}
            
            {/* Copy button (for text content) */}
            {(preview.type === 'text' || preview.type === 'code' || preview.type === 'markdown') && (
              <Button size="icon" variant="ghost" className="h-7 w-7" onClick={handleCopy} title={t('fileManager.copyContent')}>
                {copied ? <Check className="h-3.5 w-3.5 text-green-400" /> : <Copy className="h-3.5 w-3.5" />}
              </Button>
            )}

            {/* Toggle metadata */}
            <Button 
              size="icon" 
              variant="ghost" 
              className={cn("h-7 w-7", showMetadata && "bg-zinc-800")} 
              onClick={() => setShowMetadata(s => !s)} 
              title={t('fileManager.toggleInfo', 'Toggle Info (i)')}
            >
              <Info className="h-3.5 w-3.5" />
            </Button>
            
            {/* Open external */}
            {onOpenExternal && (
              <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => onOpenExternal(preview.path)} title={t('fileManager.openExternal')}>
                <ExternalLink className="h-3.5 w-3.5" />
              </Button>
            )}
            
            {/* Close */}
            <Button size="icon" variant="ghost" className="h-7 w-7" onClick={onClose}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto">
          {/* Image Preview */}
          {preview.type === 'image' && (
            <div className="flex items-center justify-center min-h-[300px] p-4 bg-zinc-950">
              <img
                src={preview.data}
                alt={preview.name}
                className="max-w-full max-h-full object-contain transition-transform duration-200"
                style={{
                  transform: `scale(${imageZoom}) rotate(${imageRotation}deg)`,
                }}
              />
            </div>
          )}

          {/* Markdown Preview */}
          {preview.type === 'markdown' && (
            <div 
              className="p-6 prose prose-invert prose-sm max-w-none"
              dangerouslySetInnerHTML={{ __html: markdownHtml }}
            />
          )}

          {/* Code Preview with Syntax Highlighting */}
          {preview.type === 'code' && (
            preview.stream ? (
              <VirtualTextPreview
                path={preview.stream.path}
                size={preview.stream.size}
                language={preview.stream.language}
                highlight={true}
                showLineNumbers={true}
                className="p-4"
              />
            ) : (
              <div className="overflow-auto bg-zinc-950">
                <CodeHighlight
                  code={preview.data}
                  language={preview.language || undefined}
                  filename={preview.name}
                  showLineNumbers={true}
                  className="p-4"
                />
              </div>
            )
          )}

          {/* Text Preview */}
          {preview.type === 'text' && (
            preview.stream ? (
              <VirtualTextPreview
                path={preview.stream.path}
                size={preview.stream.size}
                highlight={false}
                showLineNumbers={true}
                className="p-4"
              />
            ) : (
              <pre className="p-4 text-xs font-mono text-zinc-300 whitespace-pre-wrap break-words">
                {preview.data}
              </pre>
            )
          )}

          {/* PDF Preview (iframe) */}
          {preview.type === 'pdf' && (
            <iframe
              src={preview.data}
              className="w-full h-[70vh]"
              title={preview.name}
            />
          )}

          {/* Office Document Preview */}
          {preview.type === 'office' && (
            <OfficePreview
              data={preview.data}
              mimeType={preview.mimeType || 'application/octet-stream'}
              filename={preview.name}
              className="h-[70vh]"
            />
          )}

          {/* Unsupported */}
          {preview.type === 'unsupported' && (
            <div className="flex flex-col items-center justify-center py-16 text-zinc-500">
              <FileQuestion className="h-12 w-12 mb-4 opacity-50" />
              <p className="text-sm">{preview.reason || t('fileManager.unsupportedFormat')}</p>
              {onOpenExternal && (
                <Button 
                  variant="outline" 
                  className="mt-4"
                  onClick={() => onOpenExternal(preview.path)}
                >
                  <ExternalLink className="h-4 w-4 mr-2" />
                  {t('fileManager.openExternal')}
                </Button>
              )}
            </div>
          )}

          {/* Too Large */}
          {preview.type === 'too-large' && (
            <div className="flex flex-col items-center justify-center py-16 text-zinc-500">
              <FileQuestion className="h-12 w-12 mb-4 opacity-50" />
              <p className="text-sm">{t('fileManager.fileTooLarge')}</p>
              {preview.fileSize && (
                <p className="text-xs text-zinc-600 mt-1">
                  {t('fileManager.fileSize')}: {(preview.fileSize / 1024 / 1024).toFixed(2)} MB
                </p>
              )}
            </div>
          )}

          {/* Archive Preview */}
          {preview.type === 'archive' && preview.archiveInfo && (
            <div className="p-4">
              {/* Archive Stats */}
              <div className="flex items-center gap-4 mb-4 p-3 bg-zinc-900/50 rounded-lg text-xs text-zinc-400">
                <div className="flex items-center gap-1.5">
                  <Folder className="h-3.5 w-3.5" />
                  <span>{preview.archiveInfo.totalDirs} {t('fileManager.folders')}</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <File className="h-3.5 w-3.5" />
                  <span>{preview.archiveInfo.totalFiles} {t('fileManager.files')}</span>
                </div>
                <div className="ml-auto flex items-center gap-3">
                  <span>{t('fileManager.originalSize')}: {formatSize(preview.archiveInfo.totalSize)}</span>
                  <span>{t('fileManager.compressedSize')}: {formatSize(preview.archiveInfo.compressedSize)}</span>
                  <span className="text-emerald-400">
                    {preview.archiveInfo.totalSize > 0 
                      ? `${Math.round((1 - preview.archiveInfo.compressedSize / preview.archiveInfo.totalSize) * 100)}%`
                      : '0%'
                    } {t('fileManager.saved')}
                  </span>
                </div>
              </div>

              {/* File List */}
              <div className="border border-theme-border rounded-lg overflow-hidden">
                <div className="grid grid-cols-[1fr_80px_80px_120px] gap-2 px-3 py-2 bg-zinc-900/80 border-b border-theme-border text-xs font-medium text-zinc-400">
                  <span>{t('fileManager.name')}</span>
                  <span className="text-right">{t('fileManager.size')}</span>
                  <span className="text-right">{t('fileManager.compressed')}</span>
                  <span className="text-right">{t('fileManager.modified')}</span>
                </div>
                <div className="max-h-[400px] overflow-y-auto">
                  {preview.archiveInfo.entries.map((entry: ArchiveEntry, idx: number) => (
                    <div 
                      key={idx}
                      className={cn(
                        "grid grid-cols-[1fr_80px_80px_120px] gap-2 px-3 py-1.5 text-xs",
                        idx % 2 === 0 ? "bg-zinc-900/20" : "bg-transparent",
                        "hover:bg-zinc-800/50"
                      )}
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        {entry.isDir ? (
                          <Folder className="h-3.5 w-3.5 text-amber-400 shrink-0" />
                        ) : (
                          <File className="h-3.5 w-3.5 text-zinc-500 shrink-0" />
                        )}
                        <span className="truncate text-zinc-300" title={entry.path}>
                          {entry.name}
                        </span>
                      </div>
                      <span className="text-right text-zinc-500">
                        {entry.isDir ? '-' : formatSize(entry.size)}
                      </span>
                      <span className="text-right text-zinc-500">
                        {entry.isDir ? '-' : formatSize(entry.compressedSize)}
                      </span>
                      <span className="text-right text-zinc-600">
                        {entry.modified || '-'}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Metadata Panel */}
        {showMetadata && preview.metadata && (
          <div className="px-4 py-3 border-t border-theme-border bg-zinc-900/50">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-x-6 gap-y-2 text-xs">
              {/* Size */}
              <div className="flex items-center gap-2">
                <HardDrive className="h-3.5 w-3.5 text-zinc-500" />
                <span className="text-zinc-500">{t('fileManager.size')}:</span>
                <span className="text-zinc-300">{formatFileSize(preview.metadata.size)}</span>
              </div>
              
              {/* Modified */}
              <div className="flex items-center gap-2">
                <Clock className="h-3.5 w-3.5 text-zinc-500" />
                <span className="text-zinc-500">{t('fileManager.modified')}:</span>
                <span className="text-zinc-300" title={formatTimestamp(preview.metadata.modified)}>
                  {formatRelativeTime(preview.metadata.modified)}
                </span>
              </div>
              
              {/* Created (if available) */}
              {preview.metadata.created && (
                <div className="flex items-center gap-2">
                  <Clock className="h-3.5 w-3.5 text-zinc-500" />
                  <span className="text-zinc-500">{t('fileManager.created', 'Created')}:</span>
                  <span className="text-zinc-300" title={formatTimestamp(preview.metadata.created)}>
                    {formatRelativeTime(preview.metadata.created)}
                  </span>
                </div>
              )}
              
              {/* Permissions (Unix) or Readonly (Windows) */}
              {preview.metadata.mode !== undefined ? (
                <div className="flex items-center gap-2">
                  <Shield className="h-3.5 w-3.5 text-zinc-500" />
                  <span className="text-zinc-500">{t('fileManager.permissions', 'Permissions')}:</span>
                  <span className="text-zinc-300 font-mono">
                    {formatUnixPermissions(preview.metadata.mode)}
                  </span>
                </div>
              ) : (
                <div className="flex items-center gap-2">
                  <Shield className="h-3.5 w-3.5 text-zinc-500" />
                  <span className="text-zinc-500">{t('fileManager.permissions', 'Permissions')}:</span>
                  <span className="text-zinc-300">
                    {preview.metadata.readonly ? t('fileManager.readonly', 'Read-only') : t('fileManager.readwrite', 'Read/Write')}
                  </span>
                </div>
              )}
              
              {/* MIME Type */}
              {preview.metadata.mimeType && (
                <div className="flex items-center gap-2">
                  <FileText className="h-3.5 w-3.5 text-zinc-500" />
                  <span className="text-zinc-500">{t('fileManager.type', 'Type')}:</span>
                  <span className="text-zinc-300 truncate" title={preview.metadata.mimeType}>
                    {preview.metadata.mimeType}
                  </span>
                </div>
              )}
              
              {/* Symlink indicator */}
              {preview.metadata.isSymlink && (
                <div className="flex items-center gap-2">
                  <span className="text-amber-400 text-xs">↪ {t('fileManager.symlink', 'Symbolic Link')}</span>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Footer hint */}
        <div className="px-4 py-2 border-t border-theme-border bg-zinc-900/30 text-xs text-zinc-600">
          {canNavigate ? (
            <span>{t('fileManager.quickLookHintNav', 'Press ← → to navigate, Space or Esc to close, i to toggle info')}</span>
          ) : (
            <span>{t('fileManager.quickLookHint')}</span>
          )}
        </div>
      </div>
    </div>
  );
};
