# Knowledge Workspace and Instant Markdown Editor Implementation Record

## Status

- Owner: OxideTerm GPUI application
- State: implementation and repository verification complete
- Canonical content: Markdown source stored by the existing RAG document store
- Primary entry point: the activity toolbar
- Workspace surface: one central Knowledge tab with an internal navigator/editor split
- Initial editing modes: Source, Instant, and Preview

Implemented in the current slice:

- one activity-bar action opens or focuses one central Knowledge tab;
- the tab owns its left collection/document navigator and right editor pane;
- source, instant-rendering, and preview modes share one canonical Markdown buffer;
- Instant blocks reuse document-level reference definitions and footnote order, so links and
  footnote numbering match full Preview without reparsing the complete document for every row;
- document loads and navigator refreshes run off the render path and reject stale generations;
- saves use atomic optimistic versions, debounced autosave, explicit conflict recovery, and dirty-close protection;
- keyword indexing is a coalesced background outcome, so autosave bursts do not synchronously rebuild the global BM25 index;
- persisted content/index revisions resume an interrupted keyword rebuild after restart;
- initial and post-save semantic-index freshness is visible;
- the flat navigator supports document filtering and a narrow-window stacked fallback;
- responsive layout uses the actual central-workspace width and treats detached tabs as full-window surfaces;
- keyboard operation protects native editor input, focuses filtering with `Cmd/Ctrl+F`, and moves through visible documents with arrow keys;
- the formatting toolbar covers undo/redo, headings, emphasis, strike, inline and fenced code,
  quotes, unordered/ordered/task lists, links, images, tables, and horizontal rules;
- tab close and application quit protect dirty drafts in both main and detached windows;
- existing Settings-page external editing remains available as a fallback.

## 1. Goal

Promote Knowledge from an AI settings page into a first-class workspace tool. Users must be able
to open one Knowledge workspace tab, browse collections and documents in its left navigator,
manage those documents, edit the selected Markdown document in the right pane, and understand
whether the saved content is fully searchable.

The editor follows an instant-rendering model:

```text
Markdown source is the only persisted document
                  │
                  ▼
       Parse source into ranged blocks
                  │
        ┌─────────┴─────────┐
        │ Active block      │ Edit its Markdown source
        │ Inactive blocks   │ Use native Markdown rendering
        └───────────────────┘
```

This plan does not introduce a WebView or a JavaScript editor runtime. Existing GPUI editor, Markdown renderer, RAG store, theme, and internationalization infrastructure remain authoritative.

## 2. Product Decisions

### 2.1 Layout

The activity toolbar gains a Knowledge action. Activating it focuses the existing Knowledge tab or
creates one. It does not open the right context sidebar.

The Knowledge tab owns a full-height split workspace:

```text
┌──────────────────────────── Knowledge tab ────────────────────────────┐
│ collection/document navigator │ Markdown editor for selected document │
└────────────────────────────────┴──────────────────────────────────────┘
```

The tab's left navigator owns:

- collection list and current collection;
- document list and current document;
- create, import, rename, and delete actions supported by the domain API;
- full-text filtering of the visible document list;
- import, embedding, and reindex progress;
- saved, saving, conflict, error, and embeddings-stale status;
- an action to open the full Knowledge settings section for embedding configuration;
- a resizable or theme-metric-driven width with a responsive narrow-window fallback.

The navigator is a single continuous workbench surface, not a copy of the Settings page. Its
visual hierarchy is fixed:

1. one compact workspace toolbar for creating collections and importing files;
2. a flat collection section with one row per collection;
3. a flat document section with one row per document and its create action in the section header;
4. infrequent embedding, reindex, and configuration actions beside that document create action.

Embedding configuration cards, collection statistics, explanatory copy, and other Settings-page
content do not appear in the navigator. Selected rows use one restrained background state rather
than bordered cards, and list actions stay available without depending on hover-only disclosure.

The tab's right pane owns the selected document editor. Selecting another document replaces the
right-pane document after applying dirty-draft protection; it does not create another application
tab. The Knowledge tab itself is the unit of tab focus, detach, close, and restoration.

### 2.2 Editing modes

1. **Source** uses the existing `TextEditorView` with Markdown syntax highlighting.
2. **Instant** is a block-level hybrid view: it keeps the Markdown source canonical, renders inactive blocks with `oxideterm-gpui-markdown`, and exposes the active block as editable source. It is not a true WYSIWYG editor because the active block still exposes Markdown syntax.
3. **Preview** is read-only and uses the existing virtualized Markdown renderer.

The first release treats complex blocks as atomic in Instant mode:

- fenced and indented code blocks;
- tables;
- block quotes and callouts;
- display math;
- Mermaid code blocks;
- raw HTML containers;
- footnote definitions.

Selecting an inactive block activates its Markdown source editor. Escape returns that block to its
rendered presentation after the canonical draft has received the edits.

### 2.3 Persistence

- The editor loads raw content and the document version together.
- A local draft becomes dirty immediately after an edit.
- `Cmd+S`/`Ctrl+S` performs an explicit save.
- Autosave runs after a short idle debounce only when the document remains open, the draft is valid UTF-8, and no save is already in flight.
- Saves call a typed document-save API with the loaded version. The API must commit the version
  comparison and document update atomically; it must not infer conflicts from an error string.
- A successful save installs the returned version before accepting another save result.
- A stale response is ignored using an editor generation number.
- A version conflict never overwrites remote content silently. The user can reload the stored document or copy the local draft before retrying.
- Closing a dirty document requires Save, Discard, or Cancel.
- Document persistence and keyword-index rebuilding are separate outcomes. A post-commit indexing
  failure must never be reported as though the document itself failed to save.

### 2.4 Search index and embeddings

The existing update API re-chunks the document and rebuilds BM25, while removing embeddings belonging to replaced chunks. Therefore the UI distinguishes these states:

- **Saved, updating search index**: source is durable while the coalesced BM25 rebuild is pending;
- **Saved and searchable**: source and BM25 are current;
- **Embeddings pending**: source and BM25 are current but vector embeddings are incomplete;
- **Generating embeddings**: a collection embedding job is active;
- **Save conflict** or **Save failed**.

The first release must not claim that semantic search is current after a save unless embeddings have been regenerated. Automatic embedding regeneration may be queued when a usable embedding provider is configured; otherwise the sidebar exposes the existing Generate Embeddings action and the stale state remains visible.

## 3. Architecture

### 3.1 Ownership boundaries

```text
oxideterm-ai
├─ RAG document CRUD
├─ optimistic version checks
├─ chunking and BM25 rebuild
└─ embedding completeness/status queries

oxideterm-gpui-editor
├─ canonical Markdown TextBuffer
├─ cursor, selection, IME, clipboard
├─ undo/redo and save shortcut
└─ source editing surface

oxideterm-gpui-markdown
├─ Markdown parsing and native rendering
├─ ranged block projection for Instant mode
└─ preview and rendered inactive blocks

oxideterm-gpui-app
├─ Knowledge workspace tab and internal navigator
├─ selected-document editor lifecycle
├─ asynchronous load/save coordination
├─ mode switching and active-block orchestration
└─ localized user-facing state
```

Reusable business state must not be duplicated between the settings page and the new workspace
tab. Collection selection, operation progress, and RAG error state stay on the existing AI
workspace entity until a narrower shared Knowledge entity is justified. The tab navigator and
settings page are two views of that state.

### 3.2 Ranged Markdown blocks

The current Markdown model is renderer-oriented and does not expose source ranges. Instant mode adds a separate projection rather than contaminating every render node with editor state.

Proposed public model:

```rust
pub struct MarkdownSourceBlock {
    pub range: Range<usize>,
    pub kind: MarkdownSourceBlockKind,
}
```

Required invariants:

- ranges are byte offsets into the exact UTF-8 source supplied to the parser;
- ranges are ordered and non-overlapping;
- every non-empty source byte belongs to a block or an explicit inter-block gap;
- block ranges include Markdown markers needed for lossless editing;
- malformed or incomplete Markdown still produces an editable fallback block;
- metadata/front matter is preserved even if it is not rendered;
- source mapping never reconstructs Markdown from the render model.

Use `pulldown-cmark` offset iteration where it provides reliable event ranges. A small block-boundary scanner may be used only to preserve gaps, incomplete constructs, or syntax that the renderer intentionally suppresses.

### 3.3 Editor state

The Knowledge workspace surface owns:

- the one Knowledge tab identifier;
- collection list, selected collection, document list, and selected document;
- navigator loading, filtering, scrolling, resize, and operation states;
- a bounded cache of document drafts only if switching documents requires it.

The selected Knowledge document owns:

- document and collection identifiers;
- title and format;
- loaded RAG version;
- canonical Markdown draft;
- last successfully saved content hash or revision;
- editor mode;
- active Instant-mode block;
- parse revision and ranged block projection;
- load/save generation;
- dirty and save status;
- preview scroll state;
- save debounce task;
- background parse task when the size threshold is exceeded.

Workspace and selected-document state must be owned by the Knowledge tab entity. It must not be
stored in render-local fields, duplicated in the global context sidebar, or reconstructed every
frame.

### 3.4 Asynchronous rules

- Disk/database access and large Markdown parsing do not block the GPUI render callback.
- Only the newest load, parse, and save generation may update visible state.
- Dropping the Knowledge tab cancels its debounce and ignores outstanding results.
- Opening Knowledge reuses its existing workspace tab.
- Navigator refreshes do not replace a dirty editor draft.
- Switching or deleting the selected document resolves its dirty draft through one explicit event.
- A new-document dialog, its format selector, and its native text-input route belong to the
  window that opened them. Returning a detached Knowledge tab transfers the dialog to the main
  window; releasing that window dismisses the dialog and clears its transient draft.

## 4. Implemented Work

### Completed foundation — Documentation and regression baseline

- Add this plan.
- Record the existing Knowledge CRUD, external editor, reindex, and embedding behavior.
- Run focused baseline tests before changing ownership or rendering.

Exit criteria:

- plan is reviewed against the current repository;
- baseline tests are known;
- no vendored crate modification is required. If that changes, update the relevant `OXIDETERM_PATCHES.md` ledger in the same change.

### Completed foundation — Atomic save and observable index state

- Move the document version comparison into the same redb write transaction as the content,
  chunks, metadata, and version update.
- Add a typed save error so callers can match version conflicts without parsing display text.
- Return document persistence separately from keyword and semantic index state.
- Stop rebuilding the global BM25 index synchronously inside the persistence result path.
- Add a coalescing background BM25 rebuild owner before enabling editor autosave.
- Add direct document metadata/content lookup so opening a tab does not scan every collection.
- Add document-level pending-embedding status.
- Add regression tests for stale versions, concurrent same-version saves, embedding invalidation,
  and a committed document whose later keyword-index rebuild fails.
- Persist content and indexed revisions in the RAG database so a crash between document commit and
  keyword rebuild is detected and recovered when the store reopens.

Exit criteria:

- two same-version writers cannot both commit;
- a stale version leaves content, chunks, embeddings, and metadata unchanged;
- a successful document commit remains successful even if later indexing fails;
- callers receive typed conflict and index states;
- automatic saves can be coalesced without rebuilding the entire index on every keystroke pause.

### Completed surface — Knowledge workspace tab and internal navigator

- Add one Knowledge tab kind and make the activity action open or focus that tab.
- Do not add a Knowledge `ContextSidebarPanel`; the global right context sidebar remains available
  for Assistant and Host Tools independently of the Knowledge workspace.
- Build the tab's internal left navigator and right editor pane.
- Add the localized activity tooltip, tab title, navigator empty states, actions, and status labels to every shipped locale.
- Render collection and document navigation with GPUI virtual-list primitives when content can overflow.
- Reuse existing collection selection, import, deletion, reindex, and embedding operations.
- Keep the settings Knowledge page operational and synchronized.
- Add an Open Settings action for provider and embedding configuration.

Exit criteria:

- Knowledge can be opened or focused from the activity toolbar;
- opening Knowledge does not mutate the right context-sidebar selection or visibility;
- the Knowledge tab visibly contains both the document navigator and editor pane;
- all existing collections and documents are visible;
- create/import/delete/reindex actions work from the new surface;
- every shipped locale contains the new keys.

### Completed editor — Selected-document Source and Preview modes

- Load content and version from the RAG store without blocking render.
- Host `TextEditorView` in Source mode with Markdown syntax highlighting.
- Host the existing virtualized Markdown component in Preview mode.
- Add mode actions and keyboard shortcuts.
- Implement dirty state, explicit save, debounced autosave, version advancement, and stale-result rejection.
- Add close protection for unsaved drafts.

Exit criteria:

- selecting a document in the tab navigator loads it into the tab's right pane;
- selecting another document does not create an additional workspace tab;
- edits survive mode switches;
- Source and Preview show the same canonical draft;
- save updates raw content, chunks, and the editor version, then queues the coalesced BM25 rebuild;
- conflicts are visible and never overwrite silently;
- reopening a saved document returns identical source.

### Completed parser work — Ranged block projection

- Add source-range parsing to `oxideterm-gpui-markdown`.
- Cover headings, paragraphs, lists, code blocks, quotes, tables, HTML, front matter, math, footnotes, and malformed input.
- Preserve blank-line gaps and trailing source.
- Add property-oriented tests for ordered non-overlapping ranges and exact source slicing.
- Cache the projection by source revision.

Exit criteria:

- every supported fixture has stable lossless source slices;
- incomplete Markdown remains editable;
- parsing does not mutate or normalize source;
- large documents do not reparse during unrelated renders.

### Completed editor — Instant mode

- Render inactive source blocks through the existing Markdown renderer.
- Render the active block with the existing editor buffer and syntax highlighting.
- Activate blocks with pointer and keyboard navigation.
- Keep the active block visible across small edits and recover predictably after structural edits.
- Commit block edits back to the canonical document without round-trip serialization.
- Treat complex blocks as atomic in the first release.
- Preserve input method composition and clipboard behavior by delegating active editing to the native editor.

Exit criteria:

- clicking rendered text opens the correct source block;
- leaving a block renders the updated result;
- undo/redo operates on the canonical Markdown history;
- source, instant, and preview modes remain byte-for-byte consistent;
- no supported Markdown feature requires conversion from a rich document model.

### Completed indexing — Index and embedding correctness

- Add an embedding completeness query for a document or collection if the existing response cannot express it.
- Mark saved documents with missing embeddings as pending.
- Queue or expose regeneration according to configured provider capability.
- Refresh navigator and editor status after embedding jobs complete.
- Ensure failure diagnostics never contain provider secrets or document contents.

Exit criteria:

- the UI never labels stale vector data as current;
- BM25 search sees saved edits when the observable keyword-index state becomes current, and the UI does not claim search freshness before that point;
- semantic search sees them after the embedding state becomes current;
- failures are actionable and redacted.

### Completed UX work — Formatting and lifecycle hardening

- Add formatting actions for heading, emphasis, strike, link, inline code, code block, quote, ordered list, unordered list, and task list.
- Preserve selection when applying inline formatting.
- Support paste as plain text and Markdown without hidden HTML conversion.
- Add document search and replace through the existing editor.
- Add accessibility labels, focus order, tooltips, empty states, and narrow-width behavior.
- Validate large documents, rapid mode switching, repeated saves, collection deletion, and app shutdown.

Exit criteria:

- keyboard-only operation covers navigation, editing, saving, mode switching, and closing;
- common paste and input method workflows are stable;
- no unbounded per-frame work is introduced;
- the feature is ready for normal release rather than an experimental flag.

## 5. Test Plan

### 5.1 Markdown source ranges

- one block and adjacent blocks;
- blank lines and trailing newline;
- nested lists and task lists;
- fenced code containing Markdown markers;
- GFM table alignment;
- block quote and callout;
- inline and display math;
- Mermaid fenced block;
- footnotes and front matter;
- raw HTML;
- incomplete fence, link, emphasis, list, and table;
- Unicode, combining characters, emoji, and CRLF input.

### 5.2 Document state

- load success, not found, and load failure;
- dirty state transitions;
- save success increments version;
- unchanged save avoids unnecessary work;
- concurrent save result ordering;
- optimistic version conflict;
- reload and preserve-local-copy conflict actions;
- autosave cancellation on close;
- collection/document deletion while open.

### 5.3 UI behavior

- activity button opens or focuses the single Knowledge workspace tab;
- opening Knowledge leaves the global right context sidebar unchanged;
- collection and document rows remain available in the tab's overflowing navigator;
- selecting a document updates the right editor without creating another tab;
- selecting the current document preserves its editor and scroll state;
- source/instant/preview mode switching retains content and scroll state;
- save shortcut works on macOS, Windows, and Linux;
- all dialogs and tooltips are localized;
- dirty close confirmation cannot be bypassed accidentally.

### 5.4 Performance

- parsing is cached by document revision;
- only visible Instant-mode blocks are rendered;
- large lists use virtual scrolling;
- 1 MiB Markdown documents remain responsive during typing and scrolling;
- stale background parses and saves are discarded;
- no document content is cloned per render frame.

## 6. Internationalization

Every new user-facing string must be added to all locales under `crates/oxideterm-i18n/locales/`. New copy includes:

- Knowledge activity, tab, and navigator labels;
- editor mode labels;
- saved, saving, dirty, conflict, failed, and embeddings-pending states;
- save/reload/discard/cancel actions;
- empty collection and empty document states;
- indexing and embedding status;
- formatting action labels and tooltips.

After changes, validate every locale JSON file and search for English fallback text outside the English locale.

## 7. Security and Data Integrity

- Knowledge content is user data and must not be added to logs, diagnostics, telemetry, or AI prompts unless the existing explicit RAG retrieval policy selects and redacts it.
- Error messages must identify document and operation safely without including document content.
- Temporary external-edit files retain private permissions while the legacy action exists.
- No background task captures provider API keys beyond the existing zeroizing provider boundary.
- Version conflicts fail closed.
- Markdown rendering keeps existing URL, image, HTML, and code-action safety policies.

## 8. Compatibility

- Existing RAG storage remains unchanged; no data migration is required for the editor itself.
- Existing settings-page management remains available for configuration and legacy external editing.
- External editor support remains available as a fallback.
- Source, Instant, and Preview ship as three views over the same canonical Markdown draft.
- Any new persisted UI preference uses a semantic setting rather than a hardcoded default scattered through rendering code.

## 9. Definition of Done

The feature is complete when:

1. Knowledge is a first-class workspace tab opened from the activity toolbar.
2. All collection and document management remains functional.
3. The tab contains a left document navigator and a right native Markdown editor with Source, block-level Instant, and Preview views.
4. Source, Instant, and Preview modes share one canonical Markdown draft.
5. Saving is version-safe and dirty close is protected.
6. BM25 and embedding freshness are accurately represented.
7. All shipped locales are complete.
8. Focused unit and UI-state tests pass.
9. `cargo fmt --check`, relevant crate tests, and workspace checks pass.
10. A final adversarial review finds no unresolved correctness, data-loss, security, performance, or lifecycle issue.
