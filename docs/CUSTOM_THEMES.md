# Custom Theme Engine

> Full-depth theme customization for terminal and application UI.

## Overview

OxideTerm ships with **30+ built-in themes** (Oxide, Dracula, Nord, Catppuccin, Spring Rice, Tokyo Night, and more) and a **visual theme editor** that lets users create fully custom themes covering both terminal colors and application chrome.

Custom themes are stored client-side in `localStorage` under the key `oxide-custom-themes` and are keyed by a `custom:` prefixed slug (e.g. `custom:my-dark-purple`). They survive app updates and can be created, edited, duplicated, and deleted from the Settings panel.

## Architecture

```
                    SettingsView.tsx
                    ├─ Theme dropdown (built-in + custom groups)
                    └─ ThemeEditorModal.tsx
                         ├─ Terminal Colors (22 ITheme fields)
                         └─ UI Colors (19 AppUiColors fields, 5 sections)
                              │
                    ┌─────────▼──────────┐
                    │   themes.ts        │
                    │   ├─ saveCustomTheme()
                    │   ├─ getTerminalTheme()
                    │   ├─ deriveUiColorsFromTerminal()
                    │   ├─ applyCustomThemeCSS()
                    │   └─ clearCustomThemeCSS()
                    └────────────────────┘
                              │
                    ┌─────────▼──────────┐
                    │  settingsStore.ts   │
                    │  subscribeWithSelector()
                    │  on theme change:   │
                    │   custom → applyCustomThemeCSS()
                    │   built-in → clearCustomThemeCSS()
                    │              + data-theme attr
                    └────────────────────┘
                              │
                    ┌─────────▼──────────┐
                    │  CSS Custom Props   │
                    │  --theme-bg         │
                    │  --theme-text       │
                    │  --theme-accent     │
                    │  ... (19 total)     │
                    └─────────┬──────────┘
                              │
                    ┌─────────▼──────────┐
                    │  Tailwind @theme    │
                    │  bg-theme-bg        │
                    │  text-theme-text    │
                    │  border-theme-border│
                    │  ... utility classes│
                    └────────────────────┘
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib/themes.ts` | Type definitions, CRUD, theme resolution, CSS variable injection, auto-derive logic |
| `src/components/settings/ThemeEditorModal.tsx` | Visual editor modal with color pickers, live preview |
| `src/components/settings/SettingsView.tsx` | Theme dropdown with custom group, create/edit buttons |
| `src/store/settingsStore.ts` | Theme subscription — applies CSS on change |
| `src/styles.css` | `[data-theme="..."]` blocks for built-in themes, `@theme` Tailwind mapping |

## Data Model

### `CustomTheme`

```typescript
type CustomTheme = {
  name: string;            // Display name (user-entered)
  terminalColors: ITheme;  // xterm.js terminal color scheme
  uiColors: AppUiColors;   // Application UI chrome colors
};
```

### `AppUiColors` — 19 Fields in 5 Categories

| Category | Field | CSS Variable | Description |
|---|---|---|---|
| **Background** | `bg` | `--theme-bg` | Primary application background |
| | `bgPanel` | `--theme-bg-panel` | Panel / sidebar background |
| | `bgHover` | `--theme-bg-hover` | Hover state background |
| | `bgActive` | `--theme-bg-active` | Active / selected item background |
| | `bgSecondary` | `--theme-bg-secondary` | Secondary panel / alternate row |
| **Text** | `text` | `--theme-text` | Primary text color |
| | `textMuted` | `--theme-text-muted` | Muted / secondary importance text |
| | `textSecondary` | `--theme-text-secondary` | De-emphasized text |
| **Border** | `border` | `--theme-border` | Standard border color |
| | `borderStrong` | `--theme-border-strong` | Emphasized border |
| | `divider` | `--theme-divider` | Section divider lines |
| **Accent** | `accent` | `--theme-accent` | Primary accent / brand color |
| | `accentHover` | `--theme-accent-hover` | Accent hover state |
| | `accentText` | `--theme-accent-text` | Text on accent background |
| | `accentSecondary` | `--theme-accent-secondary` | Secondary accent |
| **Semantic** | `success` | `--theme-success` | Success status color |
| | `warning` | `--theme-warning` | Warning status color |
| | `error` | `--theme-error` | Error status color |
| | `info` | `--theme-info` | Informational status color |

### Terminal Colors (xterm.js `ITheme`) — 22 Fields

| Field | Description |
|---|---|
| `background` | Terminal background |
| `foreground` | Default text color |
| `cursor` | Cursor color |
| `selectionBackground` | Text selection highlight |
| `black` / `brightBlack` | ANSI color 0 / 8 |
| `red` / `brightRed` | ANSI color 1 / 9 |
| `green` / `brightGreen` | ANSI color 2 / 10 |
| `yellow` / `brightYellow` | ANSI color 3 / 11 |
| `blue` / `brightBlue` | ANSI color 4 / 12 |
| `magenta` / `brightMagenta` | ANSI color 5 / 13 |
| `cyan` / `brightCyan` | ANSI color 6 / 14 |
| `white` / `brightWhite` | ANSI color 7 / 15 |

## Theme Application Flow

### Built-in Themes

```
User selects "dracula" →
  settingsStore writes terminal.theme = "dracula" →
  subscription fires →
    clearCustomThemeCSS()       // remove any inline --theme-* props
    document.documentElement.dataset.theme = "dracula"
    xterm.js receives themes["dracula"] as ITheme
```

Built-in themes derive their UI variables from CSS `[data-theme="dracula"]` blocks in `styles.css`.

### Custom Themes

```
User selects "custom:my-neon" →
  settingsStore writes terminal.theme = "custom:my-neon" →
  subscription fires →
    applyCustomThemeCSS("custom:my-neon")
      → reads customThemesRegistry["custom:my-neon"].uiColors
      → root.style.setProperty("--theme-bg", "#0a0a0f")
      → root.style.setProperty("--theme-accent", "#ff00ff")
      → ... (all 19 CSS variables)
    xterm.js receives customTheme.terminalColors as ITheme
```

Custom themes bypass `data-theme` entirely — inline `style.setProperty()` on `<html>` takes CSS specificity precedence.

## Auto-Derive Algorithm

When creating a new custom theme, `deriveUiColorsFromTerminal(ITheme)` generates all 19 UI colors from the terminal palette:

```
Terminal background → bg, bgPanel(+15), bgHover(+30), bgActive(+40), bgSecondary(+10)
Terminal foreground → text
Terminal brightBlack → textMuted
  mix(fg, muted, 0.5) → textSecondary
Terminal background → border(+30), divider(+20)
  mix(cursor, fg, 0.6) → borderStrong
Terminal cursor → accent, accentHover(-20)
  mix(cursor, bg, 0.7) → accentText
Terminal brightBlack → accentSecondary
Terminal green → success
Terminal yellow → warning
Terminal red → error
Terminal blue → info
```

The `shift(hex, amount)` helper adds a fixed delta to each RGB channel (clamped 0–255). The `mix(c1, c2, ratio)` helper blends two colors.

Users can always manually override any derived color.

## Theme Editor UI

The `ThemeEditorModal` component provides:

### Header
- **Theme name** input — free text, auto-generates slug for ID
- **Based On** dropdown — duplicate from any built-in theme when creating new

### Live Preview
- Mini terminal simulation with colored prompt, git output, and blinking cursor
- UI chrome bar showing accent button, hover state, panel, and semantic status dots (success/warning/error/info)

### Terminal Colors Tab
- 4-column grid of 20 color swatches (background through brightWhite)
- Each swatch: native color picker + click-to-edit hex input (`#RRGGBB`)

### UI Colors Tab
Five grouped sections, each with a section header:

1. **Background** — bg, panel, hover, active, secondary
2. **Text** — primary, muted, secondary
3. **Borders & Dividers** — border, strong border, divider
4. **Accent** — primary, hover, text-on-accent, secondary
5. **Status Colors** — success, warning, error, info

"Auto from Terminal" button re-derives all UI colors from the current terminal palette.

### Actions
- **Save** — persists to localStorage and applies immediately
- **Delete** — removes custom theme (edit mode only), reverts to default
- **Cancel** — discards changes

## Storage Format

Custom themes are stored in `localStorage` under the key `oxide-custom-themes`:

```json
{
  "custom:my-neon": {
    "name": "My Neon",
    "terminalColors": {
      "background": "#0a0a0f",
      "foreground": "#e0e0ff",
      "cursor": "#ff00ff",
      "black": "#1a1a2e",
      "red": "#ff3366",
      ...
    },
    "uiColors": {
      "bg": "#0a0a0f",
      "bgPanel": "#14142a",
      "bgHover": "#1e1e3f",
      "bgActive": "#28284e",
      "bgSecondary": "#10102a",
      "text": "#e0e0ff",
      "textMuted": "#8080aa",
      "textSecondary": "#b0b0d5",
      "border": "#1e1e3f",
      "borderStrong": "#4040aa",
      "divider": "#18183a",
      "accent": "#ff00ff",
      "accentHover": "#cc00cc",
      "accentText": "#4a004a",
      "accentSecondary": "#8080aa",
      "success": "#00ff88",
      "warning": "#ffcc00",
      "error": "#ff3366",
      "info": "#3388ff"
    }
  }
}
```

Theme IDs follow the pattern `custom:{slug}` where the slug is derived from the display name via `slugify()` (lowercase, non-alphanumeric replaced with hyphens, CJK characters preserved).

## CSS Variable Integration

The 19 UI CSS variables are mapped into Tailwind utility classes via the `@theme` block in `styles.css`:

```css
@theme {
  --color-theme-bg: var(--theme-bg);
  --color-theme-bg-panel: var(--theme-bg-panel);
  --color-theme-text: var(--theme-text);
  --color-theme-accent: var(--theme-accent);
  /* ... etc */
}
```

Components use standard Tailwind utilities: `bg-theme-bg`, `text-theme-text-muted`, `border-theme-border`, etc.

## Internationalization

The theme editor is fully localized across all 11 supported languages. Translation keys are organized under `settings_view.custom_theme` in each locale's `settings_view.json`:

- Section titles: `section_background`, `section_text`, `section_border`, `section_accent`, `section_semantic`
- Color labels: `colors.ui_bg`, `colors.ui_panel`, `colors.ui_hover`, `colors.ui_active`, `colors.ui_accent`, etc.
- UI strings: `create_title`, `edit_title`, `auto_derive`, `click_to_edit_hex`, etc.

## Adding New CSS Variables

To expand custom theme coverage:

1. Add the field to the `AppUiColors` type in `themes.ts`
2. Add the CSS variable mapping to the `UI_CSS_PROPS` array in `themes.ts`
3. Add the derivation rule in `deriveUiColorsFromTerminal()`
4. Add the field to the appropriate section in `UI_COLOR_SECTIONS` in `ThemeEditorModal.tsx`
5. Update the default `uiColors` state in `ThemeEditorModal.tsx`
6. Add `--theme-*` variable definition at the end of each `[data-theme]` block in `styles.css` (for built-in themes)
7. Map the variable in the `@theme` block: `--color-theme-*: var(--theme-*)`
8. Add i18n label to all 11 locale files under `settings_view.custom_theme.colors`

## Relation to Built-in Themes

Built-in themes define their UI variables in CSS (`styles.css`), not in TypeScript. The two systems coexist:

| Aspect | Built-in | Custom |
|---|---|---|
| Terminal colors | `themes` object in `themes.ts` | `customThemesRegistry[id].terminalColors` |
| UI colors | CSS `[data-theme]` blocks in `styles.css` | Inline `style.setProperty()` via `applyCustomThemeCSS()` |
| Selection | `data-theme` attribute on `<html>` | CSS variables override inline |
| Storage | Compiled into bundle | `localStorage` |
| Editable | No | Yes (ThemeEditorModal) |

---

*Last updated: 2026-02-16*
