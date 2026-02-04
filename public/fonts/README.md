# OxideTerm Fonts

This directory contains bundled **fallback** fonts for terminal use.

## Font Loading Strategy: System-First with Bundled Fallback

OxideTerm uses a **"system fonts first"** strategy for optimal performance and compatibility:

1. **System Nerd Fonts** (if installed) → Best icons/glyphs support
2. **Bundled WOFF2 fonts** (this folder) → Guaranteed fallback
3. **Generic monospace** → Ultimate fallback

This approach:
- Reduces initial bundle load when users have Nerd Fonts installed
- Ensures Nerd Font icons **always work** via bundled fallback
- Provides better font rendering via native system fonts

## Bundled Fonts

### JetBrains Mono Nerd Font
**Location**: `JetBrainsMono/`  
**Format**: WOFF2 (compressed)  
**License**: OFL (Open Font License) - see `JetBrainsMono/OFL.txt`  
**Variants**: Regular, Bold, Italic, BoldItalic  
**Size**: ~4.0 MB total

### MesloLGM Nerd Font
**Location**: `Meslo/`  
**Format**: WOFF2 (compressed)  
**License**: Apache 2.0 - see `Meslo/LICENSE.txt`  
**Variants**: Regular, Bold, Italic, BoldItalic  
**Size**: ~4.7 MB total

## Font Family Options

Available in **Settings → Terminal → Font Family**:

| Option | Font Stack | Notes |
|--------|-----------|-------|
| JetBrains Mono NF | System → Bundled → monospace | **Default**, excellent readability |
| MesloLGM Nerd Font | System → Bundled → monospace | Apple Menlo-based |
| Cascadia Code NF | System only | Windows Terminal default |
| Fira Code NF | System only | Popular ligature font |
| Menlo | macOS system font | No Nerd Font icons |
| Consolas | Windows system font | No Nerd Font icons |
| Courier New | Cross-platform | Basic monospace |
| System Monospace | OS default | Basic monospace |

## Technical Details

- All bundled fonts are **WOFF2** format (~58% smaller than TTF)
- Fonts declared in `src/styles.css` via `@font-face`
- Font stack logic in terminal components ensures graceful fallback
- Nerd Font glyphs require system or bundled NF variants

## Bundle Size Optimization

| Before (TTF) | After (WOFF2) | Reduction |
|--------------|---------------|-----------|
| 20.5 MB | 8.7 MB | **~58%** |

---

Last updated: 2025-01-13
