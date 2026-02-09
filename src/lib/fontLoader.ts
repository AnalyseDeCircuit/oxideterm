/**
 * Font Preloader for OxideTerm
 * 
 * Implements lazy loading for bundled fonts, especially the large CJK font (Maple Mono NF CN).
 * Uses document.fonts API for efficient on-demand font loading.
 * 
 * Strategy (v1.4.1+):
 * - JetBrains Mono / Meslo: Loaded eagerly (small, ~4MB each)
 * - Maple Mono NF CN (Subset): Tiered loading
 *   - Priority 1: Regular weight (~6MB) - loaded first, triggers terminal refresh
 *   - Priority 2: Bold/Italic/BoldItalic (~19MB) - loaded via requestIdleCallback
 */

// Font loading state cache
const fontLoadingState = new Map<string, Promise<boolean>>();

// Track if Regular weight is loaded (for deduping terminal refresh)
let mapleRegularLoaded = false;
let mapleRegularLoadedCallbacks: (() => void)[] = [];

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
        console.log(`[FontLoader] ${fontFamily} (weights: ${weights.join(',')}) loaded: ${isLoaded}`);
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
 * Preload Maple Mono NF CN Regular weight only (Priority 1)
 * This is the critical path - ~6MB, needed for initial CJK rendering
 */
export async function preloadMapleMonoRegular(): Promise<boolean> {
  if (mapleRegularLoaded) {
    return true;
  }
  
  const result = await preloadFont('Maple Mono NF CN (Subset)', [400], ['normal']);
  
  if (result && !mapleRegularLoaded) {
    mapleRegularLoaded = true;
    // Notify all waiting callbacks
    const callbacks = [...mapleRegularLoadedCallbacks];
    mapleRegularLoadedCallbacks = [];
    callbacks.forEach(cb => cb());
    
    if (import.meta.env.DEV) {
      console.log('[FontLoader] Maple Mono Regular loaded, triggering callbacks');
    }
  }
  
  return result;
}

/**
 * Preload Maple Mono NF CN secondary weights (Priority 2)
 * Uses requestIdleCallback for non-blocking loading
 */
export function preloadMapleMonoSecondary(): void {
  const loadSecondary = async () => {
    // Bold
    await preloadFont('Maple Mono NF CN (Subset)', [700], ['normal']);
    // Italic
    await preloadFont('Maple Mono NF CN (Subset)', [400], ['italic']);
    // BoldItalic
    await preloadFont('Maple Mono NF CN (Subset)', [700], ['italic']);
    
    if (import.meta.env.DEV) {
      console.log('[FontLoader] Maple Mono secondary weights loaded');
    }
  };
  
  // Use requestIdleCallback if available, otherwise setTimeout
  if ('requestIdleCallback' in window) {
    requestIdleCallback(() => loadSecondary(), { timeout: 5000 });
  } else {
    setTimeout(loadSecondary, 1000);
  }
}

/**
 * Preload Maple Mono NF CN with tiered strategy
 * 1. Load Regular first (critical)
 * 2. Load other weights in idle time
 */
export async function preloadMapleMono(): Promise<boolean> {
  // Step 1: Load Regular (blocking, critical path)
  const regularLoaded = await preloadMapleMonoRegular();
  
  // Step 2: Queue secondary weights for idle loading
  preloadMapleMonoSecondary();
  
  return regularLoaded;
}

/**
 * Preload fonts based on current terminal settings
 * Called on app startup to warm up font cache (with 500ms delay)
 */
export async function preloadTerminalFonts(fontFamily: string): Promise<void> {
  const tasks: Promise<boolean>[] = [];
  
  switch (fontFamily) {
    case 'jetbrains':
      tasks.push(preloadFont('JetBrains Mono NF (Subset)'));
      break;
    case 'meslo':
      tasks.push(preloadFont('MesloLGM NF (Subset)'));
      break;
    case 'maple':
      // Maple is both primary and CJK, preload it with tiered strategy
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
 * Check if Maple Mono Regular weight is loaded
 */
export function isMapleRegularLoaded(): boolean {
  return mapleRegularLoaded;
}

/**
 * Subscribe to Maple Mono Regular weight loading
 * Only fires ONCE when Regular loads, not for each weight
 * This prevents multiple terminal refreshes
 */
export function onMapleRegularLoaded(callback: () => void): () => void {
  if (mapleRegularLoaded) {
    // Already loaded, call immediately
    callback();
    return () => {};
  }
  
  mapleRegularLoadedCallbacks.push(callback);
  
  return () => {
    const index = mapleRegularLoadedCallbacks.indexOf(callback);
    if (index > -1) {
      mapleRegularLoadedCallbacks.splice(index, 1);
    }
  };
}

/**
 * Subscribe to font loading events (generic)
 * Useful for triggering terminal refresh after any font loads
 * WARNING: This fires for EVERY weight, use onMapleRegularLoaded for Maple
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
 * Ensure CJK fallback font is loaded (Regular weight only for speed)
 * Called when terminal needs to render CJK characters
 */
let cjkPreloadPromise: Promise<boolean> | null = null;

export function ensureCJKFallback(): Promise<boolean> {
  if (!cjkPreloadPromise) {
    // Check if already loaded (e.g., user selected maple font)
    if (isFontLoaded('Maple Mono NF CN (Subset)')) {
      cjkPreloadPromise = Promise.resolve(true);
      mapleRegularLoaded = true;
    } else {
      // Load Regular first, then queue secondary weights
      cjkPreloadPromise = preloadMapleMono();
    }
  }
  return cjkPreloadPromise;
}
