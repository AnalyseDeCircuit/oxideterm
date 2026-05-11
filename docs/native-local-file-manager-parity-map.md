# Native Local File Manager Parity Map

This map is the working source map for translating the Tauri local file manager UI into GPUI. It is not a completion claim; it exists so future changes stay source-driven instead of screenshot-driven.

## Tauri Sources

- `tauri版本代码/src/components/fileManager/LocalFileManager.tsx`
- `tauri版本代码/src/components/fileManager/QuickLook.tsx`
- `tauri版本代码/src/components/fileManager/FilePropertiesDialog.tsx`
- `tauri版本代码/src/components/ui/dialog.tsx`
- `tauri版本代码/src/components/ui/context-menu.tsx`
- `tauri版本代码/src/components/ui/tooltip.tsx`
- `tauri版本代码/src/locales/zh-CN/fileManager.json`

## Native Targets

- `crates/oxideterm-gpui-app/src/workspace/file_manager.rs`
- `crates/oxideterm-gpui-app/src/workspace/file_manager/actions.rs`
- `crates/oxideterm-gpui-app/src/workspace/file_manager/dialogs.rs`
- `crates/oxideterm-gpui-ui/src/modal.rs`
- `crates/oxideterm-gpui-ui/src/confirm.rs`
- `crates/oxideterm-gpui-ui/src/tooltip.rs`

## Overlay Semantics

- Tauri `DialogOverlay bg-black/60` maps to `dialog_backdrop()`.
- Tauri QuickLook `fixed inset-0 bg-black/80` maps to `quicklook_backdrop()`.
- Tauri Radix popover/context-menu portal outside-click capture maps to `popover_backdrop()`.
- Shared confirm dialogs use `dialog_backdrop()` instead of hand-rolled overlays.

## QuickLook Behavior

- Size follows Tauri: `width: min(90vw, 1000px)`, `height: min(90vh, 800px)`, guarded by `95vw/95vh` and `400x300` minimums.
- Backdrop left-click closes QuickLook; clicks inside the panel stop propagation.
- Escape closes.
- Space closes except for video previews, where the video player owns play/pause.
- ArrowLeft and ArrowRight navigate except for video previews, where the video player owns seeking.
- `i` toggles metadata.
- `u` toggles Markdown source/render mode.
- Text/code and Markdown preview bodies must stay virtualized.

## Properties Dialog

- Tauri source is `FilePropertiesDialog.tsx`.
- Dialog must remain a modal dialog, not a QuickLook overlay.
- Property labels/values should preserve Tauri's compact two-column row semantics and break long paths/MIME values instead of overflowing.
- Expensive details such as directory statistics and checksums remain loaded on demand.

## Tooltip And Menu Notes

- Global tooltip delay follows Tauri `TooltipProvider delayDuration={300}`.
- Context menus and select popovers should close on outside left/right click and block background focus while open.
- Tooltip coverage is still a separate parity pass: every icon-only command ported from Tauri should have the matching Tauri label.
