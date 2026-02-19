/**
 * PasteConfirmOverlay - 粘贴保护组件
 * 
 * 当检测到多行粘贴内容时，显示轻量级确认浮层。
 * 用于防止误粘贴危险命令（如 rm -rf）导致的意外执行。
 * 
 * 设计原则：
 * - 极轻量：不阻断用户流程，类似 Toast 而非 Modal
 * - 快速响应：Enter 确认，Escape 取消
 * - 可预览：显示前几行内容供用户确认
 */

import React, { useEffect, useRef, useCallback } from 'react';
import { AlertTriangle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface PasteConfirmOverlayProps {
  /** 要粘贴的内容 */
  content: string;
  /** 确认粘贴回调 */
  onConfirm: () => void;
  /** 取消粘贴回调 */
  onCancel: () => void;
}

/** 行数阈值：超过此行数才触发确认 */
export const PASTE_LINE_THRESHOLD = 1;
/** 字符阈值：超过此长度才触发确认（配合行数使用） */
export const PASTE_CHAR_THRESHOLD = 50;
/** 预览显示的最大行数 */
const PREVIEW_MAX_LINES = 5;

/**
 * 检查是否需要粘贴确认
 * @param content 粘贴内容
 * @returns 是否需要确认
 */
export function shouldConfirmPaste(content: string): boolean {
  // 检测换行符（包含 \r\n 和 \n）
  const hasNewline = content.includes('\n');
  const lineCount = content.split('\n').length;
  
  // 仅当有换行且超过阈值时才确认
  return hasNewline && (lineCount > PASTE_LINE_THRESHOLD || content.length > PASTE_CHAR_THRESHOLD);
}

export const PasteConfirmOverlay: React.FC<PasteConfirmOverlayProps> = ({
  content,
  onConfirm,
  onCancel,
}) => {
  const { t } = useTranslation();
  const overlayRef = useRef<HTMLDivElement>(null);
  
  // 处理键盘事件：Enter 确认，Escape 取消
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!document.hasFocus()) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      onConfirm();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    }
  }, [onConfirm, onCancel]);
  
  useEffect(() => {
    // 捕获阶段监听，确保优先处理
    window.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => window.removeEventListener('keydown', handleKeyDown, { capture: true });
  }, [handleKeyDown]);
  
  // 生成预览内容
  const lines = content.split('\n');
  const previewLines = lines.slice(0, PREVIEW_MAX_LINES);
  const remainingLines = lines.length - PREVIEW_MAX_LINES;
  
return (
    <div 
        ref={overlayRef}
        className="absolute inset-0 flex items-center justify-center z-50 bg-black/20"
    >
        <div className="bg-zinc-900/95 backdrop-blur-sm border border-yellow-500/50 rounded-lg shadow-xl p-4 max-w-md animate-in fade-in zoom-in-95 duration-150">
            {/* Header */}
            <div className="flex items-center gap-2 mb-3">
                <AlertTriangle className="h-4 w-4 text-yellow-500 shrink-0" />
                <span className="text-sm font-medium text-yellow-100">
                    {t('terminal.paste.title', { count: lines.length })}
                </span>
            </div>
            
            {/* Preview */}
            <div className="bg-zinc-950 rounded border border-zinc-700 p-2 mb-3 max-h-32 overflow-hidden">
                <pre className="text-xs text-zinc-400 font-mono whitespace-pre-wrap break-all">
                    {previewLines.map((line, i) => (
                        <div key={i} className="truncate">
                            {line || '\u00A0'}
                        </div>
                    ))}
                    {remainingLines > 0 && (
                        <div className="text-zinc-500 italic">{t('terminal.paste.more_lines', { count: remainingLines })}</div>
                    )}
                </pre>
            </div>
            
            {/* Actions */}
            <div className="flex items-center justify-between gap-4">
                <span className="text-xs text-zinc-500">
                    <kbd className="px-1.5 py-0.5 bg-zinc-800 rounded text-zinc-400 text-[10px]">Enter</kbd>
                    {' '}{t('terminal.paste.confirm')}
                    <span className="mx-2">·</span>
                    <kbd className="px-1.5 py-0.5 bg-zinc-800 rounded text-zinc-400 text-[10px]">Esc</kbd>
                    {' '}{t('terminal.paste.cancel')}
                </span>
                <div className="flex gap-2">
                    <button
                        onClick={onCancel}
                        className="px-3 py-1 text-xs text-zinc-400 hover:text-zinc-200 transition-colors"
                    >
                        {t('terminal.paste.cancel')}
                    </button>
                    <button
                        onClick={onConfirm}
                        className="px-3 py-1 text-xs bg-yellow-600 hover:bg-yellow-500 text-white rounded transition-colors"
                    >
                        {t('terminal.paste.paste')}
                    </button>
                </div>
            </div>
        </div>
    </div>
);
};
