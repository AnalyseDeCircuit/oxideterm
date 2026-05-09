# Native IDE Visual Source Map

This file pins the Tauri IDE UI source used by the GPUI port. Tauri remains the
visual source of truth; native constants should either map to a class below or
be called out as unfinished.

## Source Files Read

- `tauri版本代码/src/components/ide/IdeWorkspace.tsx`
- `tauri版本代码/src/components/ide/IdeTree.tsx`
- `tauri版本代码/src/components/ide/IdeEditorTabs.tsx`
- `tauri版本代码/src/components/ide/IdeEditorArea.tsx`
- `tauri版本代码/src/components/ide/IdeEditor.tsx`
- `tauri版本代码/src/components/ide/IdeTreeContextMenu.tsx`
- `tauri版本代码/src/components/ide/hooks/useCodeMirrorEditor.ts`
- `tauri版本代码/src/lib/fileIcons.tsx`
- `tauri版本代码/src/components/ide/dialogs/IdeRemoteFolderDialog.tsx`
- `tauri版本代码/src/components/settings/tabs/IdeTab.tsx`
- `tauri版本代码/src/store/ideStore.ts`
- `tauri版本代码/src/store/settingsStore.ts`

## Settings Runtime Contract

Tauri editor typography is built in `useCodeMirrorEditor.ts`:

- `ide.fontSize ?? terminal.fontSize`
- `ide.lineHeight ?? terminal.lineHeight`
- `ide.wordWrap ? EditorView.lineWrapping : []`

Native mapping:

- `WorkspaceApp::ide_runtime_settings`
- `IdeSurface::set_visual_and_runtime_settings`
- `TextEditorView::apply_ide_runtime_settings`

Tauri auto-save is owned by `ideStore.ts`:

- active tab change saves the previous dirty tab
- window blur saves all dirty tabs

Native status:

- active tab change is mapped in `IdeSurface::activate_tab`
- window blur save-all still needs a GPUI focus/window-lost subscription

## Workspace

Tauri:

- root: `flex flex-col h-full bg-theme-bg relative`
- disconnected overlay: `absolute inset-0 z-40 bg-black/50 ... backdrop-blur-sm`
- main split: tree panel, 1px resize handle, editor area
- status bar always at bottom

Native status:

- root/background and disconnected overlay are present
- search panel, terminal split panel, drag resize handles are not complete
- status bar exists but needs Tauri stat item parity

## Tree

Tauri `TreeNode`:

- row classes: `flex items-center gap-1 py-0.5 px-1 cursor-pointer rounded-sm`
- hover: `hover:bg-theme-bg-hover/50`
- active/open: `bg-theme-accent/10 text-theme-accent`
- indent: `depth * 12 + 4`
- chevron slot: `w-4 h-4`
- chevron size: `w-3.5 h-3.5`
- icon slot: `w-4 h-4`
- folder icon: `FolderIcon size={16}`
- file icon: `FileIcon size={14}` with git status override
- label: `text-xs truncate flex-1`

Native status:

- virtualized tree is present
- flattened row cache is keyed by FileTree structural revision
- row height/text size and `theme-accent/10` active background are mapped
- file/folder icons use the same lucide groups as Tauri `FileIcon`/`FolderIcon`
- file icon colors use named Tailwind source constants where Tauri uses
  `text-*` classes, and theme tokens where Tauri uses CSS variables
- right-click context menu shape is mapped from `IdeTreeContextMenu.tsx`
  including width, z-index, danger row, shortcut text, dividers, and copy path
- new file, new folder, rename, delete, and open-in-terminal commands are
  visible but still need the file-system/terminal action hooks
- git status marker and inline input parity are not complete

## Tabs

Tauri `TabItem`:

- row: `flex items-stretch border-b border-theme-border/50 bg-theme-bg/60 overflow-x-auto`
- tab: `gap-1.5 px-3 py-1.5 border-r border-theme-border/50`
- hover: `hover:bg-theme-bg-hover/30`
- active: `bg-theme-bg-hover border-b-2 border-b-theme-accent`
- inactive: `bg-theme-bg/50`
- filename: `text-xs truncate max-w-[120px]`
- file icon: `FileIcon size={14}`
- dirty indicator: lucide `Circle w-2 h-2 fill-theme-accent`
- close icon: lucide `X w-3 h-3`, button `w-4 h-4 rounded`

Native status:

- tab strip and active border exist
- file icon maps through the Tauri `fileIcons.tsx` extension/special-name table
- dirty indicator is a plain dot, not lucide `Circle`
- hover-only close button behavior is incomplete
- pin state, double-click pin toggle, middle-click close, and context menu
  pin/close are present
- reorder follows Tauri's 5px activation threshold and applies dnd-kit-style
  `arrayMove` to the final hovered tab on mouse-up

## Editor

Tauri CodeMirror setup:

- font family: terminal font stack via `getFontFamily`
- font size: IDE override or terminal fallback
- line height: IDE override or terminal fallback
- active line, active gutter, selection drawing, bracket matching, fold gutter,
  indentation markers, minimap, search, and word wrap compartments

Native status:

- font size, line height, and word wrap now consume IDE settings
- code cell width is measured through GPUI's text system on render; the fixed
  `font_size * 0.62` ratio is only the startup fallback before a Window exists
- syntax highlight, selection, current line, bracket rectangles, and search
  rectangles exist
- active line, active gutter, focused cursor width, rounded selection, and
  search match fill/outline use Tauri CodeMirror accent alpha semantics
- when tab backgrounds are active, the editor scroller/root is transparent and
  the gutter keeps the Tauri panel tint
- click selection, shift-drag extension, drag selection, and alt-click extra
  cursor are present
- fold gutter, indentation markers, minimap, full search bar parity, and
  selection/cursor behavior beyond the current CodeMirror visual paint mapping
  are not complete

## Editor Core Storage

Native `TextBuffer` uses a piece table for edit storage. Edits update the piece
table and line index without rebuilding or incrementally maintaining a full
`String`. The contiguous text cache is invalidated on edit and materialized only
when a boundary API asks for `text()`/`with_text()`, such as syntax, save,
search, or IME. Line reads slice from the piece table.

Remaining storage work: syntax/search still need contiguous text snapshots
because tree-sitter and current search helpers operate on `&str`; making those
fully piece-table streaming would require a second API layer.

## Screenshot Audit

No screenshot artifact is checked into this pass. The native GPUI IDE requires a
running window plus an open project/remote folder to exercise tree, tabs, and
editor states. Compile and unit verification were run; visual screenshot parity
should still be captured against the Tauri IDE once a reproducible fixture
workspace is available.

## Remote Folder Dialog

Tauri `IdeRemoteFolderDialog`:

- `DialogContent className="sm:max-w-lg"`
- body: `space-y-4 px-4`
- path input: `flex-1 font-mono text-sm`
- nav buttons: outline/sm, `Home w-4 h-4`, `ChevronUp w-4 h-4`
- list: `border border-theme-border rounded-md h-64 overflow-auto`
- loading: `Loader2 w-6 h-6 text-theme-text-muted`
- error: `AlertCircle w-6 h-6 text-red-400`, text `text-sm`
- rows: `gap-2 px-2 py-1.5 rounded transition-colors`
- selected: `bg-theme-accent/20 text-theme-accent`
- icons: `FolderOpen/Folder/ChevronRight w-4 h-4`
- selected path: `text-xs text-theme-text-muted`, code `font-mono bg-theme-bg-panel px-1 rounded`

Native status:

- dialog shell and list are present
- uses semantic radii/tokens and virtualized rows
- should still be screenshot-compared against Tauri for exact footer spacing,
  body padding, button widths, and option row hover/selected alpha
