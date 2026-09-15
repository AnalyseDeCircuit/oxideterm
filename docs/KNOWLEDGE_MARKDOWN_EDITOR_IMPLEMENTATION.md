# Notes workspace

The notes workspace uses OxideTerm's existing native text editor, Markdown renderer, UI
components, theme metrics, and RAG document store. It supports source editing and read-only
preview. There is no WYSIWYG mode or additional browser runtime.

## Layout and navigation

A single notes tab contains a compact header and a navigator/editor split. The navigator has
a notebook selector, a More menu, title/body search, and one virtualized note list.
The More menu contains notebook management, note rename/move, import, and AI indexing actions.
Document titles, controls, and status labels use the existing UI text sizes.

The desktop navigator can be collapsed or resized using the workspace pointer-capture mechanism.
Narrow windows show either navigation or the document; selecting a note returns to the document.
Detached notes windows use their own window coordinates and pointer capture.
Menus are mounted outside scrolling lists and own their keyboard focus.

Search is debounced and runs against stored titles and raw content on the background executor.
Stale results are rejected when the query or notebook changes. Keyboard navigation uses the same
result set as the visible list. This is a full-text scan, not an incremental search index.

## Editing and persistence

Source mode owns the canonical text buffer and undo history. Markdown documents receive syntax
highlighting and a compact formatting toolbar. Plain-text documents remain plain text.
Preview parsing happens in the background and is cached until the draft changes. Imported
documents retain their source path for relative image resolution.

Autosave waits for an idle interval. Explicit save reads the current editor buffer.
Each successful save advances both the persisted version and the saved-text baseline, even if
the user changed or undid text while the save was in flight. A subsequent draft remains dirty
until that exact content is saved.

Version conflicts do not overwrite stored content. Reload and copy-draft actions remain available.
Switching notes or notebooks, closing the tab, and quitting resolve unsaved drafts through the
existing save/discard/cancel flow. Notebook rename and note rename/move use atomic store writes;
note metadata changes also check the expected document version.

## AI retrieval

Notes still use the existing RAG collections and document database. No new database or migration
is introduced. Save status and AI indexing status are displayed separately.

Saving reuses chunk identities and embeddings only when text, section path, and context prefix
are unchanged. Changed or removed chunks lose stale vectors. Renaming a note updates chunk
context and invalidates its vectors; moving it preserves vectors while changing collection
membership. Separate notes may contain identical text.

BM25 rebuilds remain global, coalesced background work. The UI stops polling once keyword
indexing settles; missing embeddings do not create a permanent timer. Embedding generation
remains an explicit existing action, and its completion refreshes the selected note's status.

## Verification boundaries

Focused regressions cover the in-flight-save/undo race, title/body matching, formatting selection
boundaries, unchanged-vector reuse, duplicate note content, metadata moves, and stale versions.
Existing notes, RAG, editor, and Markdown regressions remain applicable.

Compilation and state tests cannot establish visual quality, IME behavior, drag behavior, or
retrieval quality. These require a native-window walkthrough or a separate retrieval evaluation.
The implementation does not claim incremental BM25 updates or measured retrieval-quality gains.
