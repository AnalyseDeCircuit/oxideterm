import { useState, useEffect, useCallback, useRef } from 'react';
import { cn } from '@/lib/utils';

const HUD_DURATION = 1200; // ms

/**
 * FontSizeHUD — transient floating overlay showing current font size.
 * Displayed briefly when the user adjusts terminal font size via shortcuts.
 *
 * Usage:
 *   const { showFontSize, FontSizeHUD } = useFontSizeHUD();
 *   // in shortcut action: showFontSize(16);
 *   // in JSX: <FontSizeHUD />
 */
export function useFontSizeHUD() {
  const [visible, setVisible] = useState(false);
  const [size, setSize] = useState(14);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showFontSize = useCallback((fontSize: number) => {
    setSize(fontSize);
    setVisible(true);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setVisible(false), HUD_DURATION);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const FontSizeHUD = useCallback(() => {
    if (!visible) return null;
    return (
      <div className="pointer-events-none fixed inset-0 z-[9999] flex items-center justify-center">
        <div
          className={cn(
            'rounded-sm border border-theme-border bg-theme-bg-panel/90 px-5 py-3 shadow-lg backdrop-blur-sm',
            'animate-in fade-in zoom-in-95 duration-150',
            !visible && 'animate-out fade-out zoom-out-95 duration-200',
          )}
        >
          <span className="font-mono text-2xl font-semibold text-theme-text tabular-nums">
            {size}
            <span className="ml-0.5 text-base font-normal text-theme-text-muted">px</span>
          </span>
        </div>
      </div>
    );
  }, [visible, size]);

  return { showFontSize, FontSizeHUD };
}
