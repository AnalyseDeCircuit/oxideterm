/**
 * Font Preloader for OxideTerm
 * 
 * Implements lazy loading for bundled fonts, especially the large CJK font (Maple Mono NF CN).
 * Uses document.fonts API for efficient on-demand font loading.
 * 
 * Strategy:
 * - JetBrains Mono / Meslo: Loaded eagerly (small, ~4MB each)
 * - Maple Mono NF CN: Loaded lazily when needed as CJK fallback or primary font (~25MB)
 */

// Font loading state cache
const fontLoadingState = new Map<string, Promise<boolean>>();

/**
 * Preload a specific font family using document.fonts.load()
 * Returns true if font loaded successfully, false otherwise.
 */
export async function preloadFont(
  fontFamily: string,
  weights: number[] = [400, 700],
  styles: ('normal' | 'italic')[] = ['normal']
): Promise<boolean> {
  const cacheKey = `${fontFamily}-${weights.join(',')}-${styles.join(',')}`;
  
  // Return cached promise if already loading/loaded
  if (fontLoadingState.has(cacheKey)) {
    return fontLoadingState.get(cacheKey)!;
  }
  
  const loadPromise = (async () => {
    try {
      const loadPromises: Promise<FontFace[]>[] = [];
      
      for (const weight of weights) {
        for (const style of styles) {
          // Use document.fonts.load() which triggers @font-face download
          const fontSpec = `${style === 'italic' ? 'italic ' : ''}${weight} 16px "${fontFamily}"`;
          loadPromises.push(document.fonts.load(fontSpec));
        }
      }
      
      await Promise.all(loadPromises);
      
      // Verify font is actually available
      const isLoaded = document.fonts.check(`16px "${fontFamily}"`);
      
      if (import.meta.env.DEV) {
        console.log(`[FontLoader] ${fontFamily} loaded: ${isLoaded}`);
      }
      
      return isLoaded;
    } catch (error) {
      console.warn(`[FontLoader] Failed to preload ${fontFamily}:`, error);
      return false;
    }
  })();
  
  fontLoadingState.set(cacheKey, loadPromise);
  return loadPromise;
}

/**
 * Preload Maple Mono NF CN (CJK font)
 * Only called when user selects maple font or when CJK fallback is needed
 */
export async function preloadMapleMono(): Promise<boolean> {
  return preloadFont('Maple Mono NF CN', [400, 700], ['normal', 'italic']);
}

/**
 * Preload fonts based on current terminal settings
 * Called on app startup to warm up font cache
 */
export async function preloadTerminalFonts(fontFamily: string): Promise<void> {
  const tasks: Promise<boolean>[] = [];
  
  switch (fontFamily) {
    case 'jetbrains':
      tasks.push(preloadFont('JetBrains Mono NF'));
      break;
    case 'meslo':
      tasks.push(preloadFont('MesloLGM NF'));
      break;
    case 'maple':
      // Maple is both primary and CJK, preload it
      tasks.push(preloadMapleMono());
      break;
    case 'cascadia':
    case 'consolas':
    case 'menlo':
    case 'custom':
      // System fonts don't need preloading, but CJK fallback does
      // Defer CJK preload to first actual use
      break;
  }
  
  await Promise.all(tasks);
}

/**
 * Check if a font family is currently loaded
 */
export function isFontLoaded(fontFamily: string): boolean {
  return document.fonts.check(`16px "${fontFamily}"`);
}

/**
 * Subscribe to font loading events
 * Useful for triggering terminal refresh after CJK font loads
 */
export function onFontLoaded(
  fontFamily: string,
  callback: () => void
): () => void {
  const handler = (event: FontFaceSetLoadEvent) => {
    for (const fontFace of event.fontfaces) {
      if (fontFace.family.includes(fontFamily)) {
        callback();
        break;
      }
    }
  };
  
  document.fonts.addEventListener('loadingdone', handler);
  
  return () => {
    document.fonts.removeEventListener('loadingdone', handler);
  };
}

/**
 * Ensure CJK fallback font is loaded
 * Called when terminal needs to render CJK characters
 */
let cjkPreloadPromise: Promise<boolean> | null = null;

export function ensureCJKFallback(): Promise<boolean> {
  if (!cjkPreloadPromise) {
    // Check if already loaded (e.g., user selected maple font)
    if (isFontLoaded('Maple Mono NF CN')) {
      cjkPreloadPromise = Promise.resolve(true);
    } else {
      cjkPreloadPromise = preloadMapleMono();
    }
  }
  return cjkPreloadPromise;
}
