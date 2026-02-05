/**
 * Font utilities for terminal and file previews
 * 
 * Font Loading Strategy (v1.4.0+):
 * 1. User-installed system fonts (e.g., from Nerd Fonts downloads)
 * 2. Bundled woff2 fonts as fallback (ensures Nerd Font icons work)
 * 3. Generic monospace as last resort
 * 
 * This approach:
 * - Reduces bundle size (woff2 ~60% smaller than ttf)
 * - Respects user's installed fonts
 * - Guarantees Oh My Posh / Nerd Font icons never show as boxes
 */

/**
 * Get CSS font-family string for a given font setting
 * 
 * Font stack order:
 * 1. System-installed Nerd Font variants (user may have installed)
 * 2. Bundled Nerd Font (woff2 fallback)
 * 3. Generic monospace
 */
export const getFontFamilyCSS = (val: string): string => {
  switch (val) {
    case 'jetbrains':
      // JetBrains Mono: system → bundled fallback → generic
      return '"JetBrainsMono Nerd Font", "JetBrainsMono Nerd Font Mono", "JetBrains Mono NF", "JetBrains Mono", monospace';
    
    case 'meslo':
      // Meslo: system → bundled fallback → generic
      return '"MesloLGM Nerd Font", "MesloLGM Nerd Font Mono", "MesloLGM NF", "Meslo LG M", monospace';
    
    case 'maple':
      // Maple Mono NF CN: bundled CJK-optimized font
      return '"Maple Mono NF CN", "Maple Mono NF", "Maple Mono", monospace';
    
    case 'cascadia':
      // Cascadia Code: Windows Terminal default, system only
      return '"Cascadia Code NF", "Cascadia Mono NF", "Cascadia Code", "Cascadia Mono", monospace';
    
    case 'firacode':
      // Fira Code: popular programming font
      return '"FiraCode Nerd Font", "FiraCode Nerd Font Mono", "Fira Code", monospace';
    
    case 'menlo':
      // Menlo: macOS system font
      return 'Menlo, Monaco, "Courier New", monospace';
    
    case 'consolas':
      // Consolas: Windows system font
      return 'Consolas, "Courier New", monospace';
    
    case 'courier':
      return '"Courier New", Courier, monospace';
    
    case 'monospace':
      return 'monospace';
    
    default:
      // Default: JetBrains Mono with full fallback chain
      return '"JetBrainsMono Nerd Font", "JetBrainsMono Nerd Font Mono", "JetBrains Mono NF", "JetBrains Mono", monospace';
  }
};

/**
 * Check if a font is available in the system
 * Uses canvas measurement technique
 */
export const isFontAvailable = (fontFamily: string): boolean => {
  if (typeof document === 'undefined') return false;
  
  const canvas = document.createElement('canvas');
  const context = canvas.getContext('2d');
  if (!context) return false;
  
  const testString = 'mmmmmmmmmmlli';
  const baseFont = 'monospace';
  
  context.font = `72px ${baseFont}`;
  const baseWidth = context.measureText(testString).width;
  
  context.font = `72px "${fontFamily}", ${baseFont}`;
  const testWidth = context.measureText(testString).width;
  
  return baseWidth !== testWidth;
};
