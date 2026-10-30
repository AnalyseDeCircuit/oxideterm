# Native editor memory: findings and measurements

Date: 2026-10-27. Baseline: main at 38fa618ba; subsequent changes are recorded by batch below.

## Fourteenth batch: shared save contents, sparse folds and snapshot metadata (2026-10-30)

Baseline: `c96366ddf` plus batches eleven through thirteen. Each probe was recorded before
its production change, then repeated on the same macOS development profile without concurrent
builds. Measurements below are live malloc allocations or sampled malloc peaks, not process RSS.
Older sections describe their respective batches rather than the latest implementation.

### Save and workspace owners share text pieces

Workspace current/saved contents, close-after-save requests and in-flight saves now retain
`SharedText` slices instead of calling the editor's contiguous `text_snapshot()`. Editor creation
and restoration accept those shared contents, including modified multi-piece documents. Their
initial line index counts newlines before allocating, retaining the exact-capacity opening path. The
persisted workspace format still contains ordinary text strings. Saved revisions, conflict
preconditions, queued saves, edits during upload and dirty-close behavior keep their contracts.
The public MCP workspace editor uses the same shared contents for writes and rollback snapshots.

Local, SFTP and agent writes encode those pieces without rebuilding a document string. Newline
conversion uses an 8 KiB zeroizing buffer; legacy encodings keep one encoder across piece
boundaries and write directly into the final zeroizing byte vector. Unrepresentable text fails
before a file is changed. UTF-8/UTF-16 encoded-size checks include newline expansion and BOMs.
The encoded byte vector remains available for the existing atomic-write and retry paths.
The original contiguous editor save callback is retained alongside the shared callback.

Selection formatting reads only the affected text/lines. Language detection reads the first
line, and workspace language/snippet metadata no longer requires flattening the full document.

The IDE probe opens a file, inserts `中\n`, synchronizes the real editor into its workspace and
captures a workspace snapshot. Allocation and timing cover synchronization plus snapshot capture.

| Source size | Additional retained heap before / after, bytes | Sync before / after, ms |
| ---: | ---: | ---: |
| 16 MiB | 16,794,880 / 1,616 | 2.176 / 0.025 |
| 64 MiB | 67,126,528 / 1,616 | 8.149 / 0.023 |

The encoding probe uses `中文 line\n` repeated 1,400,000 times (16,800,000 UTF-8 bytes).
A separate thread samples allocator usage every 100 microseconds. Expected bytes are checked
after timing; the peak includes the encoded output but excludes the already-prepared source.

| Encoding / ending | Extra peak before / after, MiB | Encode before / after, ms |
| --- | ---: | ---: |
| UTF-8 / LF | 48.096 / 16.033 | 362.530 / 1.304 |
| UTF-8 / CRLF | 66.768 / 17.369 | 519.581 / 453.565 |
| GBK / CRLF | 80.080 / 17.369 | 21213.798 / 20112.636 |

Exact peak-growth pairs: 50,431,984 / 16,812,016; 70,010,864 / 18,212,848;
83,970,032 / 18,212,848 bytes. GBK's output capacity is estimated from UTF-8 input length:
returned-vector allocation is 16,809,984 / 18,202,624 bytes, a 1.33 MiB increase, while eliminating
the much larger conversion peak. UTF-8 output retention is unchanged. An intermediate version
used a separate encoding output buffer and increased GBK time to 34477.531 ms; direct-to-vector
encoding removed that regression. Timings are individual development-profile runs.

### Folding keeps the compact row representation

Folded documents now keep compact widths/wraps plus a sparse list of collapsed intervals.
Hidden physical lines have implicit positions rather than full `DisplayRow` records. Fold
headers stay unwrapped; display-row lookup and caret coordinates account for cumulative hidden
lines. The existing explicit representation remains available for widths exceeding `u32`.

For 250,000 `row\n` lines with physical lines 6 through 10 hidden below header line 5 (zero-based):

| Row map | Retained bytes before / after | Build before / after, ms |
| --- | ---: | ---: |
| Unfolded | 1,015,872 / 1,015,872 | 14.130 / 18.719 |
| Folded | 8,388,672 / 1,015,952 | 17.373 / 15.788 |

The folded representation drops about 87.9% of its allocation in this fixture. Regression
coverage exercises multiple separated folds, wrapped boundary columns, unwrapped headers,
hidden-line lookup, the final visible row and unfolding back to ordinary rows.

### Background snapshots share their metadata arrays

The piece and allocation vectors now use shared copy-on-write owners. A syntax/search snapshot
clones those owners instead of copying every descriptor. Allocation-table sharing also prevents
trimming text still visible to a snapshot; mutations detach the table before appending or remapping.
This keeps the existing flat piece table rather than introducing a new persistent tree.

The fixture replaces `a` with `z` in every `ab\n` line, retains two snapshots, then inserts `中`
in the middle. Snapshots must retain `zb\n`, and undo/redo must restore the corresponding text.

| Pieces | Two-snapshot heap before / after, bytes | Capture before / after, ms | Subsequent edit before / after, ms |
| ---: | ---: | ---: | ---: |
| 8,192 | 655,360 / 0 | 0.212 / <0.001 | 2.296 / 2.344 |
| 32,768 | 2,621,440 / 0 | 0.451 / <0.001 | 5.234 / 5.212 |

The existing batch-edit probe also passed: 16,384 edits apply/undo/redo in 7.430 / 7.500 /
7.412 ms with 1,442,144 retained bytes. Multi-piece deletion/undo retain 288 / 640 extra bytes;
the difference from batch eleven includes the new shared-array owners, not another text copy.

### Verification and remaining structural work

- Editor core, syntax, GPUI editor, IDE state and GPUI IDE regression suites passed. Coverage
  includes search, history, snapshot independence, fragmented workspace serialization,
  save coalescing/failure/races, dirty close, Markdown modes and Unicode selection formatting.
- The focused bounded-snippet regression passed. IDE filesystem: 39 tests passed, including
  exact chunked UTF-8/UTF-16/GBK/stateful-encoder output and existing local/agent write behavior.
- `cargo check -p oxideterm-gpui-app`, workspace formatting and whitespace checks passed.
  No native GUI was launched.
- Ordinary regex search and Markdown parsing still use contiguous text at their API boundaries.
  Active syntax-tree size, full width-dependent wrap initialization and linear piece lookup
  remain structural targets. This batch does not implement a persistent indexed tree or
  background wrap scheduling.

```sh
cargo test -p oxideterm-gpui-ide --lib ide_editor_memory -- --ignored --nocapture --test-threads=1
cargo test -p oxideterm-ide-fs --lib file_encoding_memory -- --ignored --nocapture --test-threads=1
cargo test -p oxideterm-gpui-editor --lib editor_folded_rows_memory -- --ignored --nocapture --test-threads=1
cargo test -p oxideterm-gpui-editor --lib editor_fragmented_snapshot_memory -- --ignored --nocapture --test-threads=1
```

## Thirteenth batch: syntax reads shared text pieces directly (2026-10-30)

Baseline: `c96366ddf` plus the eleventh and twelfth batches. The same reset probe was
expanded and measured before changing syntax input. The editor worker now builds a borrowed
offset index over snapshot chunks instead of joining them into a temporary document string.
Parser callbacks read bytes at the requested offset, including offsets inside UTF-8 characters.
Highlight query predicates iterate the pieces intersecting each captured node. Markdown inline
parsing uses a bounded source view; injected languages retain their global included ranges.
Bracket scans and indentation consume the same source without joining text.

The implementation uses OxideTerm's existing piece table and tree-sitter's callback/provider
interfaces. Zed informed the choice of chunked reads; no Zed implementation was incorporated.
Undo storage, grammar loading and the final syntax-tree representation are unchanged.

### Paired reset measurements

Same macOS development-profile probe and sampling method as the twelfth batch, one paired
run per case, no concurrent builds. The first two fixtures remain approximately 1 MiB.
The third contains an 8 MiB block comment split across roughly 255 pieces by edits that preserve
its contents. All cases replace the initial item with `// rebuilt\n` and perform a full reset
through the real editor worker. The measured peak is additional live malloc allocation above
the already-active pre-reset heap, not process RSS.

| Rust fixture | Extra rebuild peak before / after, MiB | Rebuild before / after, ms |
| --- | ---: | ---: |
| Single-line functions | 14.692 / 13.677 | 1749.210 / 1728.948 |
| Multi-line functions | 2.872 / 1.872 | 1920.928 / 1886.507 |
| Fragmented 8 MiB comment | 16.543 / 0.536 | 2753.295 / 2669.216 |

Exact peak-growth pairs in bytes: 15,405,872 / 14,340,960; 3,011,664 / 1,963,136;
17,346,576 / 562,192. Post-reset retained growth remains effectively unchanged: 1,664 / 1,728;
560 / 560; 2,384 / 2,384 bytes. The main benefit is eliminating temporary text allocations,
particularly with fragmented documents; the observed timing differences are from individual runs.

### Verification

- Syntax: 29 tests passed, including fragmented UTF-8 incremental parsing, exact highlight
  ranges, folds, indentation, bracket positions and immutable snapshots. Markdown inline ranges,
  cross-piece query predicates and embedded-language edits/removal/cancellation are covered.
- GPUI editor: 41 tests passed, including existing search, edit/history, worker publication,
  cancellation and hidden-cache restoration cases. The reset memory probe also passed.
- `cargo check -p oxideterm-gpui-app`, workspace formatting and diff whitespace checks passed.
- No native GUI was started. Persistent piece-tree indexing and wrap scheduling are unchanged.

## Twelfth batch: release obsolete state before a full syntax rebuild (2026-10-30)

The baseline includes the eleventh batch's working changes, with the new probe recorded before
changing syntax-state disposal. Only the full-rebuild branch now releases its old tree, parser,
highlight/structure caches and bracket index before creating replacements on the syntax worker.
Same-version reuse and incremental parsing retain their existing paths and required old trees.
Cancellation and publication still use the existing generation/version checks.

### Paired peak measurements

macOS, development profile, the real editor request/completion path, one paired run per case,
no concurrent builds. Each fixture is approximately 1 MiB of Rust, using either single-line
functions or multi-line functions with a nested `if`. After the first parse finishes, the probe
replaces the first function with `// rebuilt\n` and requests a full syntax reset. It checks
the new comment highlight, buffer version and multi-line fixture's new fold position.

A separate thread samples live malloc allocation every 100 microseconds. The table reports
the maximum growth above that fixture's already-active pre-reset heap, including temporary
text, new parser/tree, indexes and runtime allocations. These are sampled heap peaks, not RSS.

| Rust fixture | Extra rebuild peak before / after, MiB | Rebuild before / after, ms |
| --- | ---: | ---: |
| Single-line functions | 40.94 / 14.69 | 1732.360 / 1695.977 |
| Multi-line functions | 42.28 / 2.87 | 1947.758 / 1830.027 |

Using the earlier document-only baseline, total peak deltas were 96,448,400 / 68,930,400 bytes
and 50,735,248 / 9,412,448 bytes, respectively. The cases have independent baselines; the
second fixture runs after language/runtime initialization from the first fixture. Post-reset
retained heap is unchanged on both sides: 53,526,256 bytes and 6,401,344 bytes above their
respective document-only baselines. The improvement removes overlap during rebuilding rather
than shrinking the final syntax representation.

```sh
cargo test -p oxideterm-gpui-editor --lib editor_syntax_reset_memory -- --ignored --nocapture --test-threads=1
```

### Verification and scope

- GPUI editor: 41 tests passed. Existing cases cover incremental edits, queued work, full
  document/language replacement, plugin updates, cancellation, hidden-cache restoration,
  highlighting, folds, indentation, search, editing and history.
- The peak probe was run explicitly before and after the production change. No native GUI
  was started.
- At this batch, grammar/highlight consumers still used a temporary contiguous snapshot. First document
  layout and the final active syntax-tree size are unchanged.

## Eleventh batch: single-pass transactions and cross-piece undo (2026-10-30)

Baseline: `c96366ddf`, with `editor_batch_edit_memory` added before production changes.

### Transactions build the piece table once

After sorting and rejecting overlapping edits, UTF-8 boundary validation walks the original
chunks in order. A forward piece cursor then copies unchanged ranges, records deleted shared
slices and inserts replacements into one new piece vector. Undo and redo use that same path.
An allocation-identity map reuses replacement sources without repeatedly searching the chunk
list. The finished vector releases spare capacity because each subsequent transaction builds
another vector rather than extending it.

### Cross-piece undo no longer joins deleted text

`SharedText` stores an empty value, one slice or a shared array of slices. Line-index updates
read those chunks directly, locating newline bytes within each chunk. Undo restores the source
allocations and byte ranges; it does not flatten a multi-piece deletion. Public transaction
constructors, byte coordinates, transaction boundaries and saved-revision tracking are unchanged.
The internal replacement type now exposes chunk reads instead of dereferencing to contiguous text.

### Same-workload results

macOS, development profile, one paired run per case, no concurrent builds. Each batch replaces
every `word` in `word\n` repeated N times with `中`, with change tracking enabled. Setup and
expected-text assertions are outside timing. Undo must restore `word\n` and the saved state;
redo must restore `中\n`. Heap is live malloc allocation above the pre-transaction baseline,
including inverse edits, piece metadata and tracked coordinate changes, while consuming the
already-prepared transaction. It is not process RSS or a sampled peak.

| Edits | Apply before / after, ms | Undo before / after, ms | Redo before / after, ms | Retained heap before / after, bytes |
| ---: | ---: | ---: | ---: | ---: |
| 1,024 | 23.718 / 1.248 | 31.795 / 1.181 | 12.639 / 1.173 | 90,496 / 90,464 |
| 4,096 | 171.090 / 4.617 | 359.509 / 4.190 | 176.003 / 4.099 | 360,832 / 360,800 |
| 16,384 | 2764.518 / 12.482 | 5888.644 / 10.302 | 2781.034 / 9.227 | 1,442,176 / 1,442,144 |

The multi-piece fixture inserts three independently allocated ASCII/CJK blocks totalling
5,767,171 bytes between `head\n` and `tail`, then retains an immutable snapshot while deleting
those blocks. The blocks and snapshot are outside the allocation baseline.

| Cross-piece operation | Before | After |
| --- | ---: | ---: |
| Net live heap after deletion, bytes | 5,783,552 | -32 |
| Net live heap after undo, bytes | 5,783,792 | 320 |
| Delete, ms | 0.406 | 0.034 |
| Undo, ms | 27.471 | 0.185 |

The negative 32-byte delta is freed metadata, not released document content. Source bytes
remain owned by the snapshot and undo history; the eliminated allocation is the joined copy.
An intermediate implementation made undo slower by flattening chunk iterators one byte at a
time. Per-chunk newline searches removed that regression without reintroducing a text copy.

The previous search/snapshot probe also ran: search retained 5,013,824 bytes, UI dispatch was
0.050 ms and completion 174.817 ms. Single-character append with an 8 MiB retained snapshot
still added 160 bytes and took 0.003 ms; single-piece deletion added 256 bytes and took 0.009 ms.

```sh
cargo test -p oxideterm-gpui-editor --lib editor_batch_edit_memory -- --ignored --nocapture --test-threads=1
```

### Verification and scope

- Editor core: 29 tests passed; GPUI editor: 41 passed. The transaction table now includes
  repeated insertions at one position, adjacent deletion/insertion, unsorted edits, Unicode,
  empty documents and trailing newlines. Rejected UTF-8 edits on a fragmented buffer leave
  both text and undo behavior intact.
- The cross-piece regression checks exact deleted chunks, shared source pointers, restored
  line contents, saved state, immutable snapshots and edit-coordinate notifications.
- `cargo check -p oxideterm-gpui-app` passed after the storage/API changes. The subsequent
  line-index scan and piece-capacity refinements were verified by core/editor tests and the
  final paired probe; they do not change caller interfaces.
- Transactions still apply on the UI executor. Regex/grammar consumers that require a full
  contiguous snapshot and initial document layout are unchanged. No native GUI was launched.

## Tenth batch: editor search and shared edit text (2026-10-29)

Search and snapshot-append baselines were recorded at `3321229af` with the probe added before
production changes. The deletion baseline was recorded after the search changes but before
changing buffer storage; that probe uses `TextBuffer` directly, without an editor or search.

### Search leaves the UI executor and stores compact ranges

Documents over 64 KiB use one background search per editor. New requests invalidate the worker
and retain only the latest request state until it finishes. Completion checks both generation
and document version. Navigation and replacement requested while searching wait for current
results; changing the query cancels those queued actions. Small inputs still finish inline.
Closing the editor cancels its work. A result for a hidden editor does not rebuild its released
row cache; revealing the selected match waits until the editor is visible again.

Equal-length matches store one shared length and 32-bit start offsets. Variable-length matches
add end offsets, and documents with larger offsets retain full-width coordinates. Painting and
navigation read this representation directly. Literal replacement reuses the current match
index and one shared replacement allocation instead of rescanning into another match vector
and cloning the replacement string for every occurrence.

### Snapshots and undo share inserted text allocations

Small inserts append only to an exclusively owned chunk, up to 64 KiB. Large pastes keep their
owned allocation. A snapshot holding an older chunk no longer forces the next edit to clone
the complete accumulated insertion buffer. Undo records for ranges within one piece share the
underlying allocation and range; restoring them reuses that allocation in the piece table.
Reclamation keeps bytes needed by snapshots or history and removes unused current-buffer
references. Multi-piece deleted ranges still copy their replacement text for contiguous reads.

### Paired measurements

macOS, the same development profile, no concurrent builds, one run per side. Values below are
live malloc allocation deltas after each operation, not process RSS or sampled peak memory.
The search fixture is 5,000,000 bytes with 1,000,000 matches; the document row index is populated
before timing. Search allocation includes completion-related runtime allocations. Each buffer
fixture holds an immutable snapshot of 8 MiB text while appending one character or deleting all
but its first and last bytes. The retained snapshot and source bytes are outside those deltas.

| Operation | Additional live heap before / after | Operation time before / after, ms |
| --- | ---: | ---: |
| Million-match search | 16.000 / 4.782 MiB | UI dispatch: 171.792 / 0.049 |
| Append with a retained snapshot | 16,777,328 / 160 bytes | 0.164 / 0.002 |
| Delete with a retained snapshot and undo | 8,388,928 / 320 bytes | 0.173 / 0.009 |

Search completion took 171.821 ms before and 175.773 ms after: the benefit is 70.1% less retained
search-related heap and moving the scan off the UI executor, rather than faster total scanning.
The buffer probes validate final text and undo/redo. They measure elimination of additional
copies; the original 8 MiB remains available to the snapshot and history.

```sh
cargo test -p oxideterm-gpui-editor --lib editor_search_snapshot_memory -- --ignored --nocapture --test-threads=1
```

### Verification and remaining work

- Editor core: 28 tests passed; GPUI editor: 41 passed. Existing coverage exercises Unicode,
  cross-piece and cross-line searches, captures, replacement, saved revisions, undo/redo,
  coordinates, wrapping, syntax and hidden-tab restoration.
- The asynchronous search regression checks obsolete queries, editing during a pending search,
  queued navigation/replacement, regex, clearing search and hidden-tab layout retention.
- `cargo check -p oxideterm-gpui-app` passed. No separate GUI instance was started.
- Regex matching still needs contiguous text and checks cancellation between iterator results;
  one regex search operation is not forcibly interrupted. Replace-all application still runs
  on the UI executor. Multi-piece undo text and initial full-document layout remain candidates
  for subsequent work. Native scrolling and typing feel remain manual acceptance.

## Ninth batch: reduce work within the existing memory budgets (2026-10-28)

Baseline: `df7538086`, with the paced-playback and statistics probes added before production
changes. The database page budgets, image-cache budgets, three-frame playback cache, decoded
resolution and pixel transforms are unchanged.

### Knowledge statistics no longer scan embeddings during rendering

The settings document card requests statistics from its AI workspace entity. One background
read serves repeated renders; the entity retains only the last collection's numeric statistics
and request/revision keys. A shared in-memory revision advances after successful collection,
document and embedding writes. A result for an obsolete selection or revision cannot publish.
Only one statistics worker runs at a time. Entity release cancels delivery, and failed reads
can retry on a subsequent render without scheduling a self-sustaining retry loop.

The probe creates one document with 1,024 chunks and 768 stored 1,536-dimensional vectors.
It checks the expected counts on every request. Fixture construction is excluded.

| Statistics operation | Time, ms |
| --- | ---: |
| Before: 120 synchronous statistics reads | 332.494 |
| After: same direct-read comparator, 120 calls | 327.585 |
| After: first asynchronous request through completion | 2.941 |
| After: 120 cache-hit requests through the entity | 0.017 |

These times cover statistics retrieval, not complete page rendering. Collection listing and
document pagination remain on their existing paths. The optimization removes repeated reads;
it does not make a full database scan faster or increase the redb page budget.

### Animated uploads borrow cached pixels and prefetch separately

The normal upload path holds an `Arc` lease on a streamed frame instead of allocating and
copying its pixel vector. Atlas misses acquire that lease; atlas hits do not request pixels.
The lease survives eviction until upload returns. Explicit persistent `as_bytes` consumers
keep their existing borrowed-read behavior.

Each decoder request now prepares only its target frame. The image element and upload reader
request the next frame separately, including frame zero at the loop boundary. Already-ready
prefetches are skipped. This avoids waiting for a future frame before replying and prevents
queued prefetches from doing an extra frame of work ahead of a demand read. The channel remains
bounded to one queued request and the cache to three frames.

The same 240-frame, 640 × 360 GIF is measured both as a continuous sequence of reads and at its
40 ms frame interval. The paced probe uses the production prefetch contract, checks readiness
at each deadline and validates pixels. Timing covers CPU frame acquisition, not GPU upload or
native presentation. Each table entry is a single paired run; no builds ran concurrently with
the retained baseline or final measurements.

| Streaming measurement | Before | After |
| --- | ---: | ---: |
| Continuous read P95, ms | 1.567 | 0.826 |
| Continuous read maximum, ms | 1.621 | 0.901 |
| Paced read P95, ms | 0.253 | 0.029 |
| Paced read maximum, ms | 0.523 | 0.061 |
| Frames unavailable at their paced deadline | 0 / 240 | 0 / 240 |
| Retained cached pixels after playback, MiB | 2.64 | 2.64 |
| Sampled initial decode heap peak delta, MiB | 36.94 | 36.94 |
| Initial decode, ms | 259.011 | 260.690 |

The all-frames-cached control's continuous P95 was 0.225 ms before and 0.234 ms after, while
retaining 52.99 MiB of pixel payload. Streaming still pays for decoding during playback; the
improvements remove copying and queue delays rather than restoring full-frame retention.

### Verification

- GPUI: 425 passed, 6 probes ignored by default. The existing GIF disposal/transparency test
  now also checks that two upload reads share one allocation and that a held upload remains
  valid after cache eviction. Its allocation assertion failed before the fix and passes now.
  Existing tests cover resizing, playback position, pause/resume, looping, WebP and atlas reuse.
- AI: 238 passed, 3 probes ignored; application knowledge filter: 17 passed, 1 probe ignored.
  The new entity regression exercises selection changes before delivery, a write during an
  outstanding read, document editing, embedding insertion through a cloned store, deletion,
  repeated cache hits and owner release. The native application test target compiled.
- Both probes below were run explicitly. No native application launch or GPU presentation
  measurement was performed for this batch.

```sh
cargo test --locked -p oxideterm-gpui-app --bin oxideterm-native knowledge_statistics_redraw_cost -- --ignored --nocapture --test-threads=1
cargo test --locked -p gpui-pre --lib long_animation_memory -- --ignored --nocapture --test-threads=1
```

## Eighth batch: application-wide memory owners (2026-10-28)

### Default image and database caches

The default image cache now uses a shared 64 MiB reclamation budget instead of retaining every
resource indefinitely. Accounting includes encoded input, retained pixels, an estimated texture
and persistent animation-decoder working storage. Visible sources are pinned across windows,
including images in reused paint subtrees, and can exceed the budget. Hidden pending/failed loads
are discarded even below budget. SFTP previews and chat image cards use view-owned caches, so
their decoded resources do not remain in the default global cache after the views disappear.

Database page budgets are now 32 MiB for chat history, 64 MiB for RAG, and 8 MiB for transfer
progress. History creation, opening, recovery/reopening and legacy migration use the same budget.
These limits are separate from the existing decoded-history-body cache.

The database probe writes 128 MiB in 16 transactions, then performs two sequential scans with
content assertions. Each case uses a fresh synthetic database; operating-system file caches are
warm after writing. Memory is the live malloc allocation delta after writing/scanning, not a
sampled peak or process RSS.

| Page budget | Live heap delta, MiB | Write, ms | Two scans, ms |
| --- | ---: | ---: | ---: |
| Previous default: 1 GiB | 288.91 | 1072.405 | 35.929 |
| Chat: 32 MiB | 32.98 | 1097.490 | 60.769 |
| RAG: 64 MiB | 65.37 | 1121.021 | 61.007 |
| Progress: 8 MiB | 8.59 | 1098.207 | 67.636 |

The lower budgets trade additional page reads for substantially lower retained memory. The
shared-image-cache regression also checks two live windows, cached paints, and cancellation of
hidden pending loads without evicting the other window's visible image.

### HNSW construction and persistence

The production rebuild consumes its embedding records and transfers vector allocations into the
index. The borrowed public build API remains available. For 50,000 vectors of 1,536 floats, the
eliminated vector copy is 292.97 MiB by allocation size; this is a structural calculation.

Index persistence streams MessagePack through Zstandard with a 64 KiB buffer and durable atomic
replacement. Loading limits decompression while reading, uses a 64 KiB buffer, and preserves the
existing file format. Partial stream failures leave the previous file intact. The workspace
checks every 60 seconds for an index unused for at least 60 seconds. Release runs on a background
executor, skips indices held by active queries, and keeps the disk index for subsequent loading.

The I/O probe uses the same prebuilt 1,024 × 1,536 index for the old full-buffer comparator and
the new production save/load path. Index construction is outside the measured I/O interval.

| Index I/O | Sampled peak delta, MiB | Save, ms | Load, ms |
| --- | ---: | ---: | ---: |
| Full-buffer comparator | 22.34 | 174.592 | 118.554 |
| Buffered streaming | 10.13 | 134.119 | 106.394 |

### Large animations retain three playback frames

Animations whose compact payload stays within 32 MiB retain the existing cached representation.
When actual retained payload exceeds that budget, the initial pass discards it and retains frame
timing information instead. Initial loading still walks all composed frames to preserve existing
error-skipping and delay behavior. Subsequent playback uses a dedicated, owner-bound decoder,
one queued request and at most three decoded frames. Closing the last image closes its channel
and cancels further decoding. The codec's reference canvas remains intact for transparency and
disposal. Explicit borrowed `as_bytes` reads still retain their requested pixels; normal painting
uses temporary reads from the bounded frame cache.

The image element prefetches upcoming frames and retains the current display while a sharper
resolution prepares the same playback position. Small animations need no permanent decoder
thread. Pixel conversion is shared with the optimized raster helpers; GIF/LZW hot paths are
also optimized in development builds because they now run during playback.

Encoded input is shared, while each stateful image element owns its playback cache and decoder.
Weak playback leases release a closed or replaced consumer without affecting another view of the
same animation. This prevents two independently positioned views from repeatedly seeking and
evicting one another's frames. Cache accounting counts the shared input once and includes each
live player's pixels and estimated renderer/decoder resources.

The probe uses 240 frames at 640 × 360 with 40 ms delays. Both cases use the same encoded input;
the baseline calls the retained full-image decoding API. Every frame's color is checked during
one complete playback traversal.

| Animation | Retained pixel payload after playback, MiB | Sampled decode peak delta, MiB | Initial decode, ms | Frame preparation P95 / max, ms |
| --- | ---: | ---: | ---: | ---: |
| All frames cached | 52.99 | 59.43 | 276.732 | 0.249 / 0.389 |
| Adaptive streaming | 2.64 | 36.94 | 240.039 | 1.777 / 1.790 |

Pixel retention falls 95.0%; the source bytes and native compositor are additional allocations.
Playback preparation costs more CPU than reading a predecoded frame, while remaining below the
40 ms frame interval in this workload. GPU presentation time is not included.

### Background posters and WSL frame ownership

`oxideterm-image-decoder` owns shared metadata, PNG/JPEG reduced decoding and pixel transforms;
it contains no UI/cache state. GPUI and background posters both use it. Background fit/blur and
limits remain in the existing processor. Fill requests decode enough pixels for both axes;
tile/native-size requests retain original dimensions, and background EXIF handling retains its
existing raw-pixel orientation. The 4K PNG downscale regression failed before this change with
`ResourceExhausted` and now passes, along with fill and native tile cases.

The poster probe reads a 3000 × 2000 RGB file and produces 960 × 640 pixels. Its baseline uses
the original full decoder and processor; both timings include file loading. The final pixel
payload is 2.34 MiB in both cases.

| Poster | Sampled peak before / after, MiB | Decode before / after, ms |
| --- | ---: | ---: |
| PNG | 55.62 / 9.31 | 232.770 / 220.055 |
| JPEG | 55.62 / 31.64 | 272.699 / 96.505 |

WSL graphics transfers the received pixel vector into `RenderImage`; the UI stores dimensions
instead of retaining a second full frame. At 3840 × 2160 this removes 31.64 MiB per displayed
connection by allocation size. The protocol canvas, latest-frame delivery slot and GPU texture
remain independently owned. The regression verifies both displayed bytes and reuse of the
original pixel allocation, without requiring a Windows session for that ownership check.

### Reproduction and verification

```sh
cargo test --locked -p oxideterm-ai --lib database_page_cache_memory -- --ignored --nocapture --test-threads=1
cargo test --locked -p oxideterm-ai --lib index_io_memory -- --ignored --nocapture --test-threads=1
cargo test --locked -p gpui-pre --lib long_animation_memory -- --ignored --nocapture --test-threads=1
cargo test --locked -p oxideterm-background-media --lib poster_decode_memory -- --ignored --nocapture --test-threads=1
```

Peak probes sample live malloc allocation above their starting baseline. Comparators run in the
same process/build with the same fixtures; results are paired single runs, not whole-application
RSS or GPU measurements. Configuration budgets and the two allocation-size calculations above
are distinguished from these measurements.

- GPUI: 425 passed, 6 probes ignored by default; image-decoder: 2 passed. The two moved PNG/JPEG
  tests are counted in the new crate, not removed. GIF worker tests cover partial frames,
  disposal, transparency, backward requests and looping. The image-element test also exercises
  the real streaming representation through delay advancement, pause and resume.
  A two-consumer regression advances one stream to frame 4 and the other to frame 200, checks
  both images, then closes one consumer and verifies that its image is released while the other
  player's frame remains ready.
- AI: 238 passed, 3 probes ignored; atomic-file: 3 passed; SFTP progress: 7 passed. Index tests
  check ranked/filtered results, legacy files, active-query protection, eviction and reload.
- Background-media: 15 passed, 1 probe ignored; GPUI background: 25 passed, 3 ignored;
  Markdown: 44 passed. The four probes above were run explicitly.
- Native application filters: graphics 6 passed, AI state 35 passed, SFTP 45 passed.
  `cargo check --locked -p oxideterm-gpui-app` passed.
- Native GPU presentation and a live Windows/WSL session remain manual acceptance. No
  application launch was performed for this batch.

## Seventh batch: animated images and rotated file previews (2026-10-28)

GIF and animated WebP resource images now use the display-size request established in the
sixth batch. Each decoder still composes its native-resolution reference canvas, including
partial rectangles, transparency and disposal. Its owned output frame is immediately resized
before BGRA conversion and compact storage. The resize worker reuses its convolution workspace
and premultiplies the owned frame in place, avoiding another full-size alpha buffer.

Frame order, delays, looping and the image element's pause/reduced-motion behavior are unchanged.
Enlargement keeps the current frame set visible while decoding sharper frames from the original
encoded bytes, without resetting playback position. Once ready, the new animation replaces the
old frame set; animations no longer retain two complete resolution tiers. Native-size requests
retain the original pixels. Static WebP thumbnails also use the owned-frame resizer; declared
in-memory WebP images continue to show their first frame, matching their existing API behavior.

File-manager rotation now uses the same background image loader and size-aware cache instead
of synchronously decoding, rotating and copying a full image during rendering. Quarter turns
swap the requested axes; rotation follows reduced decoding and the file's EXIF orientation.
The preview therefore rotates relative to the image already displayed, fixing the former EXIF
orientation jump. A preview-local cache retains only visible resources and drops old paths,
angles and pending entries; closing the preview releases the cache with its element state.
Zoom limits, controls, containment and fallback text are unchanged.

### Memory and worker-time comparison

The GIF fixture contains 24 indexed 1920 × 1080 frames with 40 ms delays and a moving color
pattern; its display request is 640 × 360. Before modification that request retained exactly
the same 49,810,148 bytes as a native-size request. The rotated fixture is the same deterministic
6000 × 4000 RGB PNG used in the sixth batch, rotated clockwise and requested at 640 × 960.
The rotation baseline reproduces the removed helper's full decode, rotation, RGBA copy and raw
storage, including the overlapping original and rotated buffers. The after case calls the
production `SourceInfo::decode` path. Encoded fixtures are prepared outside measurement.

As above, peak means sampled live malloc allocation above the starting baseline, not RSS or GPU
memory. Time covers the first worker decode and retained-frame construction, excluding file
loading, an existing display tier during zoom, and GPU upload. These are paired single runs.

| Workload | Sampled peak before / after, MiB | Retained frames before / after, MiB | Worker time before / after, ms |
| --- | ---: | ---: | ---: |
| GIF, 24 frames, 1920 × 1080 → 640 × 360 | 75.39 / 32.64 | 47.50 / 11.56 | 2879.229 / 2862.171 |
| PNG, 6000 × 4000, 90° rotation → 640 × 960 | 228.91 / 5.28 | 91.55 / 1.31 | 845.404 / 830.814 |

GIF retained storage falls 75.7% and sampled decode peak falls 56.7%, with similar worker time.
Rotated PNG peak falls 97.7% and retained storage falls 98.6%; its decode no longer blocks the
interface thread. GIF/WebP still need native reference canvases and one composed frame. This
batch reduces accumulated frames and upload sizes, rather than replacing codec compositing.

```sh
cargo test --locked -p gpui-pre --lib animated_decode_memory -- --ignored --nocapture --test-threads=1
cargo test --locked -p gpui-pre --lib rotated_decode_memory -- --ignored --nocapture --test-threads=1
```

```text
GIF native:  retained=49810148 peak_delta=79047168 ms=2879.229
GIF sized:   retained=12126285 peak_delta=34227328 ms=2862.171
PNG original rotation: retained=96000000 peak_delta=240027136 ms=845.404
PNG sized rotation:    retained=1369363  peak_delta=5540112   ms=830.814
```

### Regression evidence

- GPUI: 425 passed, 5 manual probes ignored by default. Both new probes above were run
  explicitly. GIF coverage checks partial-frame composition, restore-previous and background
  disposal, transparency and unequal/zero delays at small and native sizes. WebP coverage checks
  exact frame colors/delays and the separate resource-animation/in-memory-static contracts.
- A real image-element regression checks sharper pixels, unchanged playback position during
  resolution replacement, release of the obsolete frame set, delay-based advancement, looping,
  reduced-motion pause, inactive-window pause and resume. It seeds the element's existing
  wall-clock timestamp, then exercises the normal layout transition, without sleeping.
- Rotated preview decoding checks all three quarter turns after EXIF orientation at thumbnail
  and native sizes. The recent-cache regression also verifies eviction of hidden pending loads
  with a visible-only budget. Existing native-pixel, DPI, crop and atlas-lifetime checks pass.
- File-manager regressions: 8 passed, including preview read-only behavior and close cleanup.
  Markdown: 44 passed. Background-media library: 14 passed. The GIF regression additionally
  checks the exact color and alpha of a partially covered edge after downsampling.
- `cargo check --locked -p oxideterm-gpui-app`, formatting and whitespace checks passed.
- Native-window visual acceptance and GPU frame timing remain manual; the application was not
  launched for this batch.

## Sixth batch: display-sized PNG/JPEG decoding (2026-10-28)

Generic image elements now obtain intrinsic dimensions before allocating pixel buffers.
After layout, the image's actual object-fit bounds and the window's device scale determine
the decode request. Requests round upward in 64-device-pixel steps, preserving detail while
avoiding a new decode at every intermediate resize. Intrinsic layout size and decoded pixel
size are stored separately, so receiving a thumbnail does not change layout or aspect ratio.

The source/cache entry owns encoded bytes, one outstanding decode, and at most two ready
resolutions. An enlarged image keeps its previous pixels while a sharper version loads.
Switching to a higher-DPI display requests more pixels; requesting native resolution uses
the existing full-resolution decoder rather than enlarging a thumbnail. Obsolete CPU tiers
are released. Window paint tracking now also retires superseded static-image atlas entries,
while preserving entries referenced by cached paint subtrees.

### Decode paths and supported cases

- Non-interlaced PNG with up to 8-bit samples is read row by row. Area downsampling keeps
  source-row buffers and target-sized accumulators, not a complete source-size image. Colour
  accumulation uses premultiplied alpha to avoid fringes around transparent pixels.
- RGB/grayscale DCT JPEG uses `jpeg-decoder`'s decoder-level 1/8, 1/4, or 1/2 scaling and then
  resamples the reduced output to the target. EXIF orientation is applied after resizing,
  with target axes exchanged for quarter turns. The library's scaling contract is documented
  in [Decoder::scale](https://docs.rs/jpeg-decoder/0.3.2/jpeg_decoder/struct.Decoder.html#method.scale).
- Full-size requests, 16-bit/interlaced/APNG inputs, CMYK/lossless JPEG, animated formats and
  other formats retain the existing decoder. Already-decoded `RenderImage` and custom-loader
  callers also keep their existing behavior. In particular, the file manager's separate
  synchronous rotated-preview helper still decodes at full resolution.

Public full-image loading/read APIs remain available. Built-in caches support the new layout/
size requests; third-party cache implementations default to their original load operation.
Declared formats on encoded `Image` sources are respected, and debug formatting for the new
in-memory resource variant prints its identity rather than the image bytes. JPEG output uses
the image library's existing default allocation limit.

### Production-path memory and latency comparison

Command:

```sh
cargo test --locked -p gpui-pre --lib sized_decode_memory -- --ignored --nocapture --test-threads=1
```

The same deterministic 6000 × 4000 RGB source is encoded as PNG and JPEG, then requested at
native size or 960 × 640. Both variants call the production decode path, including RGBA/BGRA
conversion and the previous batch's compact upload storage. Encoded input is prepared before
measurement. Peak is sampled live malloc allocation minus the starting baseline at 100 µs
intervals; it is not process RSS, GPU memory, or a guaranteed exact peak. Timing covers the
synchronous worker operation, excluding network/file loading, layout and GPU upload.

| Format | Sampled peak before / after, MiB | Retained CPU data before / after, MiB | Worker time before / after, ms |
| --- | ---: | ---: | ---: |
| PNG | 232.52 / 6.58 | 23.27 / 1.29 | 850.827 / 835.748 |
| JPEG | 246.24 / 32.58 | 38.62 / 1.49 | 1134.710 / 97.927 |

PNG peak falls about 97.2%, with similar measured latency. JPEG peak falls about 86.8%, and
the reduced decode is substantially faster in this workload. JPEG entropy/coefficient work
still requires memory beyond the final thumbnail, especially for progressive inputs.

```text
PNG native: retained=24404789 peak_delta=243812624 ms=850.827
PNG sized:  retained=1357479  peak_delta=6899984   ms=835.748
JPEG native: retained=40494867 peak_delta=258197776 ms=1134.710
JPEG sized:  retained=1560505  peak_delta=34161824  ms=97.927
```

### Regression evidence

- GPUI: 421 passed, 3 manual probes ignored by default; the new production-path probe was
  run explicitly. The final declared-format refinement passed all 8 focused image tests.
- The real image-element test exercises first small display, enlargement while retaining old
  pixels, a DPI change, exact native checkerboard pixels, stable intrinsic size, and retirement
  of an obsolete resolution. PNG tests cover fractional coverage and transparency; JPEG tests
  cover both 180-degree and 90-degree EXIF orientation.
- Markdown 44 passed, background rendering 25 passed/3 ignored, remote-desktop state/rendering
  26 passed, and terminal image-cache regression 3 passed.
- `cargo check --locked -p oxideterm-gpui-app` passed. Native-window zoom/colour appearance and
  platform GPU timing remain manual acceptance; no application launch was performed.

## Fifth batch: preview lifetimes, image storage and element-arena capacity

### SFTP and Knowledge ownership

SFTP's completed dialog exit now releases `preview_content` and its path/error/loading state.
The exit animation still owns the content until it finishes; the existing generation check
rejects old load completions. The regression was run before the repair and failed with a live
1,048,576-byte preview payload after close. It passes after the repair with zero payload bytes
retained by the closed preview.

Knowledge source/preview/split modes now select which editor and preview caches are active.
Tab-host visibility includes detached surfaces; hiding the page or locking the app suspends
the caches, while saves and document ownership continue. Preview delivery and debounce tasks
are cancelled, document copies and navigation targets are released, and unchanged-document
heights/disclosure/scroll state are retained. Changed hidden documents invalidate retained
geometry before resuming. Initial text and later editor snapshots share allocations with the
draft instead of constructing an additional full string.

The Knowledge lifecycle regression uses 71,680 bytes of unsaved Markdown. The parsed-document
payload estimate is 2,902,008 bytes while active and zero after hiding. This is an owner-level
before/after suspension measurement, not a process heap comparison; text, history and small
restoration state remain. The same scenario verifies restored scroll position, unchanged draft,
undo, source-mode release, and a parse cancelled before its hidden completion.

### Lossless images and bounded retention

The generic image loaders now store large decoded frames with the existing QOI codec when
the encoded result is smaller than the raw pixels. Frames below 64 KiB and incompressible
frames remain raw. GIF/WebP loader loops compact each decoded frame immediately, avoiding a
complete raw-frame array followed by a second conversion pass. Dimensions, BGRA bytes, frame
order and delays remain unchanged. Existing `RenderImage::new` users such as streaming media
and remote-desktop producers keep their original raw path.

Painting decodes compact frames into temporary upload buffers. The existing borrowed
`as_bytes` API remains available and retains a decoded buffer when explicitly requested.
Each window retires animation atlas entries no longer used by its current scene. Cached paint
subtrees carry their image-frame usage forward, so two visible instances at different playback
positions are both retained rather than overwriting one another.

Markdown resource images use a 64 MiB recent-image budget accounting for CPU storage plus an
estimate of one full decoded texture per image. Visible images remain pinned even when they
exceed the budget. Cold eviction removes image resources from window atlases as well as the
CPU cache. Existing callers of `retain_all` keep that API's original behavior. A focused cache
test retains two of three 2 MiB images under an 8 MiB CPU-plus-texture budget: the visible oldest
image stays, the cold middle image is evicted, and the newest image stays. CPU payload is
6,291,456 bytes before and 4,194,304 bytes afterward.

#### Paired frame-storage comparison

`image_memory_lifecycle` constructs the same 60 frames at 640 × 360, with changing coloured
tiles, in raw and compact representations. It then reads every frame once as an atlas upload
would. This comparison runs both representations in the same process and development profile.

| Representation | Retained CPU frame bytes | MiB | Build time, ms | Read all 60 frames, ms |
| --- | ---: | ---: | ---: | ---: |
| Raw | 55,296,000 | 52.73 | 7.044 | 0.002 |
| Lossless compact | 1,733,460 | 1.65 | 15.234 | 2.475 |

The synthetic tiled workload saves 96.9% of frame storage. Encoding adds about 8 ms; reading
60 compact frames adds about 2.47 ms in this run. Noisy photographic content may save less or
keep raw storage. All encoded frames still exist; this is compact random-access storage, not
a streaming decoder with a fixed total-frame budget. This batch left initial full-resolution
decode and HTTP body peaks unchanged; the sixth batch addresses the common PNG/JPEG display path.
GPU lifecycle tests establish which frame entries survive, not actual
driver allocation sizes or playback timing on a native window.

### Element arena retires transient high-water capacity

After 32 completed low-demand clears, the arena retains twice the recent block demand and
shrinks its element-registration array. Nested active scopes still defer clearing entirely.
Normal frame capacity remains warm; it is not freed and reallocated on every draw.

The same `arena_memory_lifecycle` benchmark allocates a 64 MiB burst, then draws 120 small
synthetic allocation batches:

| Version | Peak blocks | Retained after small batches | Burst + clear, ms | 120 small batches, ms |
| --- | ---: | ---: | ---: | ---: |
| Before | 64 MiB | 64 MiB | 11.166 | 0.170 |
| After | 64 MiB | 2 MiB | 4.437 | 0.094 |

These single-run timings show no slowdown in this workload; the main result is the 62 MiB
capacity release. The figures are owned storage/capacity, not RSS or total application savings.

### Verification

- GPUI: 418 tests passed; both ignored memory probes were run explicitly afterward.
- Knowledge: 11 focused tests passed; SFTP runtime: 5 passed, including the previously failing close regression.
- Markdown: 44 passed; background rendering: 25 passed, 3 manual probes ignored;
  remote-desktop rendering/state: 26 passed.
- The final image-budget accounting change passed its focused eviction regression.
- `cargo check --locked -p oxideterm-gpui-app` passed.
- Native image playback/zoom, detached-window interaction and platform GPU timing still need
  manual acceptance; the application was not launched for this batch.

## Fourth batch: inactive IDE tabs and Markdown previews

The IDE now suspends source-editor caches when another file is selected, when Markdown
switches to preview, or when the owning IDE surface becomes hidden. Both main and detached
windows count as visible. Source text, undo/redo history, cursor/selection, folds, search
state and viewport offsets remain with the editor. Syntax trees, highlight/structure/bracket
indexes, shaped chunks, segment indexes and wrap measurements are released. In-flight syntax
work is invalidated cooperatively; a quick return queues one fresh parse behind its cancellation.
Hidden edits do not restart syntax parsing or eagerly rebuild row layouts.

Markdown suspension releases its parsed document, copied selectable text, fragment map,
visible text layouts and pending parse delivery. The scroll handle also held cloned document
blocks; those are now released while measured heights, scroll offsets and disclosure choices
remain. Selection endpoints are restored only for the same buffer revision. A changed revision
discards the old geometry and selection. Copy/select-all immediately after returning can build
the text index synchronously, without waiting for the next paint.

### A retained allocation found during the lifecycle comparison

The previous selection-index constructor called the full selectable renderer outside a window
draw and discarded the returned element. GPUI allocates those elements in its thread-local
fallback arena, which is not cleared by the window's frame-arena cleanup. Rebuilding the preview
therefore accumulated an entire temporary element tree on each return. The index now walks
document text without creating elements, highlighting code, loading images or rendering formulas.
It shares inline flattening and segment boundaries with the renderer; regression coverage compares
the fragment identities and text for nested lists, quotes, tables, code, disclosures, footnotes,
Unicode, inline images and formulas.

### Same-workload before/after comparison

Command:

```sh
cargo test --locked -p oxideterm-gpui-ide --lib ide_background_tab_memory -- --ignored --nocapture --test-threads=1
```

Development profile, macOS live malloc allocations, completed headless frames. Each case opens
a document, switches to a small plain-text tab, returns, and closes its window. Background heap
below is the live allocation minus that case's starting baseline; it includes the IDE fixture,
retained document/history and shared runtime allocations, not just one cache. These are not RSS
or GPU measurements.

| Workload | Background heap before / after, MiB | Reduction | Return until parsing and paint complete before / after, ms |
| --- | ---: | ---: | ---: |
| 917,507-byte JSON source with syntax enabled | 52.75 / 9.92 | 81.2% | 8.016 / 671.684 |
| 933,888-byte Markdown, 16,384 heading/paragraph pairs | 394.14 / 3.37 | 99.1% | 78.361 / 153.654 |

JSON's return now includes rebuilding its syntax tree asynchronously, rather than reusing a
resident tree. That 672 ms is total headless completion time, not a measurement of one blocked
UI frame. Markdown's reduction includes removal of the off-frame element retention above,
not merely eviction of the parsed document. Returning restores active allocations to approximately
the first active level instead of accumulating another document-sized element tree.

Raw bytes and timings:

```text
before JSON: baseline=2301504 active=57620608 background=57610128 restored=57674224 closed=4468576 restore_ms=8.016
after  JSON: baseline=2301504 active=56548224 background=12705936 restored=56777664 closed=3414832 restore_ms=671.684
before Markdown: baseline=4468640 active=417743408 background=417754016 restored=417930896 closed=297820480 restore_ms=78.361
after  Markdown: baseline=3414896 active=48896160 background=6946576 restored=49086160 closed=3415312 restore_ms=153.654
```

Checks: editor-core 26 passed, editor 40 passed, IDE 42 passed, Markdown 44 passed.
Coverage includes stale syntax delivery, hiding with a parse in flight, hidden edits, undo,
unsaved source, source/preview switching, cross-tab copy, selection/scroll restoration, detached
surface return, and release of document blocks without losing measured geometry or disclosures.
Native window/IME/GPU acceptance remains a manual check.

## Third batch: shared snapshots and incremental layout

The syntax worker now receives a shared piece snapshot. Original and added text allocations
are shared, while only piece descriptors are copied; undo stacks and line indexes stay with
the editor. Existing contiguous snapshot APIs remain available to save/workspace consumers.
The legacy syntax collectors still materialize one temporary string on the worker, which is
released on completion rather than retained in the document cache. In the existing Rust
memory workloads this removes about 1.02 MiB per edited 1 MiB document at rest.

Long unwrapped lines now retain a compact segment index and paint only horizontal viewport
segments plus neighbouring segments. Pointer hit testing and caret bounds use the same index.
The first index construction compares independently shaped segments against the complete
native glyph sequence, including fonts, cluster positions and widths. Context-dependent or
reordered glyph sequences that do not agree retain the original full-line path. IME preedit
and rows containing control characters also retain that path because their displayed text and
advances differ from the underlying buffer. Initial full
measurement still has a document-line-sized transient allocation; this batch reduces retained
layout and repeated scroll/resize work, not every long-line peak.

Wrapped documents now store ordinary line widths implicitly and record only actual extra
wraps. Exact measured advances are run-length encoded; variable-advance payloads have a 1 MiB
budget. Ordinary range edits update the affected lines and adjust following wrap offsets.
Changing width reuses measured advances instead of reshaping the document. Undo, broad edits,
font changes and uncached variable-width lines can still require fresh measurement. Initial
layout still computes accurate document height rather than estimating unseen rows.

### Layout comparison against the second batch

Same development profile and completed headless window frames, 1000 × 700 resized to
600 × 700. Both sides use 10 px text and 1.2 line height: the no-op test shaper accumulates
floating-point advances on long lines, so integral test advances isolate editor costs from
that synthetic drift. The default fractional no-op metrics can fail the equivalence check
and correctly keep full-line rendering. These results do not establish native-font/GPU cost.
The baseline was rerun at the same metrics; earlier fractional-metric timings are not used
in this comparison. Initialization timing includes view construction, configuration and resize.

| Workload | Retained heap before / after, MiB | First layout before / after, ms | Edit + paint before / after, ms | Resize before / after, ms |
| --- | ---: | ---: | ---: | ---: |
| 288 KiB ASCII long line | 12.705 / 0.511 | 127.515 / 44.733 | 64.988 / 39.859 | 94.852 / 1.525 |
| CJK, combining marks and joined emoji long line | 1.902 / 0.301 | 48.687 / 31.273 | 28.349 / 20.626 | 29.110 / 0.989 |
| 16,384 short wrapped physical lines plus final empty line | 5.189 / 4.658 | 111.495 / 129.660 | 100.925 / 3.082 | 101.476 / 12.159 |

Heap is the final live allocation minus the starting baseline. It is not RSS or peak memory.
Wrapped initialization increased by about 18 ms in this run; the retained measurements reduce
later edit/resize work. No original text, matches or undo history were discarded for these results.

### The 27 MiB syntax allocation is mostly the tree

The ownership probe uses the same Rust source pattern and 32 inserted characters as the
editor benchmark. It releases the parser/store first, then the remaining session:

| Owner | Bytes released | Size |
| --- | ---: | ---: |
| Parser and execution store | 160800 | 157.03 KiB |
| Tree and remaining session | 28176992 | 26.87 MiB |
| Total | 28337792 | 27.03 MiB |

This explains why pooling parser instances would barely affect this measured heap cost.
Tree representation or the lifetime of inactive documents' trees is the substantive target;
incremental parsing and full-context highlights/folds must remain available when needed.

Commands:

```sh
cargo test --locked -p oxideterm-gpui-editor --lib editor_layout_memory -- --ignored --nocapture --test-threads=1
cargo test --locked -p oxideterm-editor-syntax --lib highlight_rebuild_memory -- --ignored --nocapture --test-threads=1
```

## Second batch results

The second optimization batch removes ordinary-search and platform-input full-document copies,
and reduces physical-line offsets and unwrapped widths from machine words to 32-bit values.
Normal 16–64 MiB plain-text workloads now retain about half the memory they used after the
first cache/search-index optimization batch. Syntax-heavy files remain dominated by parsing
state, not the viewport cache.

### Comparable measurements

macOS arm64, Rust 1.97.0, development test profile. Both candidates use the same existing
`editor_memory_performance` benchmark, 1000 × 700 headless window, 32 edits, 128 pages forward
and backward, and the same case-sensitive overlapping search. The first-batch implementation
was remeasured using the identical command to remove profile/feature ambiguity.

Memory is live malloc allocation after subtracting the workload's starting baseline, in MiB.
It is neither process RSS nor sampled peak memory. Window frames are completed using
`window.draw(cx).clear(cx)`; the headless text system does not measure native font or GPU cost.

| Workload | Search-state heap before | After | Reduction | Search before / after, ms | Scroll frame P95 before / after, ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust, 1 MiB | 44.99 | 44.49 | 0.50 | 2.011 / 2.018 | 1.945 / 1.447 |
| Multiline Rust, 1 MiB | 60.51 | 59.53 | 0.98 | 2.302 / 1.905 | 1.492 / 1.343 |
| Plain text, 16 MiB | 47.10 | 22.88 | 24.22 | 10.777 / 9.223 | 1.072 / 1.006 |
| Plain text, 64 MiB | 163.24 | 81.85 | 81.39 | 41.948 / 33.535 | 1.559 / 1.021 |

These are individual runs, not a statistical latency estimate. The stable allocation changes
are the main result. For 64 MiB text, 64 MiB of the reduction comes from avoiding a retained
search snapshot; line storage and allocation capacity account for the remaining reduction.
Simply opening that workload now uses 81.25 MiB above baseline, versus 98.62 MiB before.

Raw allocation values for reproduction:

| Workload | Baseline bytes | Opened before / after | Searched before / after | Closed before / after |
| --- | ---: | ---: | ---: | ---: |
| Rust | 2297248 | 47609744 / 47085456 | 49471424 / 48947136 | 8650144 / 8650144 |
| Multiline Rust | 8648096 | 69563968 / 68531776 | 72099328 / 71067136 | 8650384 / 8650384 |
| Plain 16 | 8648336 | 40612016 / 32010416 | 58034976 / 32639776 | 8650624 / 8650624 |
| Plain 64 | 8648576 | 112062880 / 93843872 | 179817488 / 94473232 | 8650864 / 8650864 |

The first workload initializes shared runtime state. Subsequent closed-window allocations stay
near that initialized baseline, and the benchmark verifies that the editor entity is released.

## Changes implemented

- **Literal search:** [search.rs](../../crates/oxideterm-editor-core/src/search.rs)
  reads piece-table chunks through a 64 KiB window with query-length overlap and Unicode
  neighbours. It preserves overlapping matches, cross-line matches, ASCII-insensitive matching
  and whole-word boundaries. Literal replace-all uses this same path.
  Memory for scanning is bounded by the chunk size and query length, separately from results.
- **Regular expressions:** [surface/search.rs](../../crates/oxideterm-gpui-editor/src/surface/search.rs)
  uses temporary contiguous access. The regex engine still needs a full input string, but
  this operation no longer installs that string as a persistent buffer cache.
- **Platform input:** [surface/input.rs](../../crates/oxideterm-gpui-editor/src/surface/input.rs)
  converts UTF-16 coordinates over pieces and copies only the requested range. Composition
  reads combine the requested prefix, marked text and suffix; they do not construct the whole
  composed document. Unicode surrogate boundaries and replacement/undo remain covered.
- **Cursor and word reads:** [buffer.rs](../../crates/oxideterm-editor-core/src/buffer.rs)
  limits grapheme context to the relevant physical line, including its newline for CRLF
  navigation. Word lookup uses the relevant line rather than a full snapshot.
- **Line offsets:** [line_index.rs](../../crates/oxideterm-editor-core/src/line_index.rs)
  stores 32-bit offsets while they fit, widens without truncation if required, allocates the
  initial index to the exact line count, and releases disproportionate capacity after deletions.
- **Display widths:** [surface/wrap.rs](../../crates/oxideterm-gpui-editor/src/surface/wrap.rs)
  stores ordinary widths as 32-bit values with exact initial capacity. Wider exceptional
  rows use the existing explicit representation. Folding and wrapping semantics are unchanged.
- **Earlier batch retained:** the 1 MiB estimated layout/highlight cache budget pins currently
  rendered rows; search uses sorted results without a separate index for every document line.

## Remaining large allocations, in priority order

### 1. Explicit snapshots still duplicate edited text

`TextBuffer::text_snapshot` calls `with_text`, which materializes the current piece table
as an `Arc<str>`. The original text and added text remain separately owned by storage.
Save, workspace synchronization and Markdown preview consume this contiguous snapshot.
[syntax_task.rs](../../crates/oxideterm-gpui-editor/src/surface/syntax_task.rs) now captures a
shared piece snapshot and flattens it only temporarily inside the worker.

The workspace's current and saved versions share allocations when equal, but an edited
version and its saved predecessor can both remain alive for valid dirty-state, save and
conflict handling. See [editor_actions.rs](../../crates/oxideterm-gpui-ide/src/surface/editor_actions.rs)
and [model.rs](../../crates/oxideterm-ide-core/src/model.rs).

**Next structural change:** extend the shared snapshot through syntax query collectors and
workspace consumers, followed by chunk-fed syntax input. The current added-text buffer uses
copy-on-write while a snapshot is alive; independently frozen append blocks would avoid copying
a large added-text buffer on its next edit. Serialize contiguous text only at boundaries
that require it. Preserve saved-version identity, in-flight save contents, cancellation and
old snapshot independence. Merely dropping saved text would break existing behavior.

Other `with_text` users, including Markdown prefix commands and language detection, can still
materialize a full buffer. The single-line fast path in `with_line_text` also retains one.

### 2. Syntax state dominates source-code files

Releasing owners separately in the 1 MiB Rust workload measured:

| Owner | Live bytes released |
| --- | ---: |
| Syntax session, including parser, tree and runtime state | 28337792 |
| Highlight cache | 4121728 |
| Bracket index | 966656 |
| Unwrapped display widths after this batch | 163888 |

The third-batch ownership probe above separates the parser/store from the rest of this aggregate.
[session.rs](../../crates/oxideterm-editor-syntax/src/session.rs) retains a parser and tree;
[plugin.rs](../../crates/oxideterm-editor-syntax/src/plugin.rs) shares immutable language/query
data but gives each document parser an execution store. Injected languages own additional
sessions.

Tree-sitter 0.27.0 already calls `ts_parser_reset` after a successful parse and releases its
old-tree reference there. Adding another reset is not a demonstrated memory fix.

**Next structural decision:** change tree retention only where inactive-view lifecycle permits
it; separately profile injected-language sessions. Retain full syntax semantics: parsing only visible lines can lose
multiline comments, nested scopes, folding and ancestor-dependent query results.

The highlight cache already uses relative 32-bit offsets. It still retains all highlights
and per-root blocks. The bracket index retains machine-word endpoints and a close-order index.
Both can be compacted further, but that alone cannot remove the much larger syntax-session cost.

### 3. Initial and context-dependent long-line layout still need full measurement

[segments.rs](../../crates/oxideterm-gpui-editor/src/surface/segments.rs) now bounds retained
horizontal layout when segment shaping agrees with full-line geometry. Its initial validation
still shapes the whole line, and contextual/reordered text can stay on the original path.
That path may retain full text, highlight chunks and glyph positions while only a small portion
is visible. The 1 MiB cache intentionally cannot evict the current frame's working set.

**Next rendering change:** a context-aware shaping interface that avoids initial full glyph
storage and supports more contextual sequences. Character-count clipping
alone would break combining characters, joined emoji, fallback fonts and bidirectional shaping.
Native rendering validation is needed for this change.

Ordinary wrapping now has a sparse per-line wrap index and incremental measured advances.
Folded layout still uses explicit records. Width changes still scan compact measurements to
recompute break positions, but reuse cached native advances; uncached variable-width lines
are measured again within the bounded cache policy.

### 4. Dense matches and replace-all scale with match count

Each `FindMatch` stores two machine-word byte positions, or 16 bytes on this host, before
vector spare capacity. A million matches therefore require at least about 15.3 MiB.
Replace-all additionally builds one owned replacement string per edit and inverse text for
undo. This batch deliberately preserves the complete result set and transaction behavior.

**Next change:** compact match storage and shared replacement payloads, preserving navigation,
counts, overlapping-match behavior and one-step undo. Silently capping results is not equivalent.

### 5. Undo retains independently allocated text

[buffer.rs](../../crates/oxideterm-editor-core/src/buffer.rs) stores a vector of inverse
`TextEdit` values per transaction; each replacement owns a `String`. Small edits have
allocation/metadata overhead, and large deletions can retain text also present in a shared
saved snapshot. The piece table already reclaims unused added text and sufficiently dead
unshared original text; that does not eliminate independently owned history payloads.

**Next change:** share immutable text ranges for history and group storage allocations while
preserving the existing transaction boundaries, saved revision, redo invalidation and exact
restoration. Do not save memory by discarding supported undo history.

### 6. File opening and saving have separate transient peaks

[local.rs](../../crates/oxideterm-ide-fs/src/local.rs) reads file bytes before decoding.
[text.rs](../../crates/oxideterm-ide-fs/src/text.rs) holds decoded text while constructing the
line-ending-normalized output. Encoding restores line endings, encodes, and copies into the
output byte vector. These stages can overlap several document-sized allocations.

**Next change:** transfer owned buffers on unchanged UTF-8/LF paths and stream conversion where
appropriate, retaining explicit encoding/BOM behavior, size limits, zeroization and atomic-save
semantics. The editor benchmark above starts with an in-memory string and does not measure
these file-I/O peaks.

### 7. Coordinate conversion now saves memory but still scans prefixes

UTF-16 conversion over pieces avoids allocation, but remains linear in the examined prefix.
Many edits also make the flat piece vector more expensive to traverse. Cached summaries per
text block could provide line/UTF-16 lookup without full-prefix scans. This is a reason to
consider an indexed piece tree, not to add another whole-document cache.

## Lessons from mature implementations

- [VS Code's buffer reimplementation](https://code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation)
  shows why per-line objects and repeated conversion can dominate memory, and how piece-based
  storage and line metadata must be designed together.
- [Scintilla's cache controls](https://scintilla.org/ScintillaDoc.html#SCI_SETLAYOUTCACHE)
  distinguish caret, page and whole-document layout retention. Keeping visible working data
  is useful; caching an entire document's layout is a separate, expensive choice.
- [Zed's Rope and SumTree](https://zed.dev/blog/zed-decoded-rope-sumtree)
  shares immutable structure between snapshots and uses summaries for coordinate lookup.
  Its [chunk-index discussion](https://zed.dev/blog/zed-decoded-rope-optimizations-part-1)
  illustrates compact local indexes rather than heavyweight objects for every text position.

These references inform the ownership and representation decisions. The changes here are
implemented against OxideTerm's existing piece table and editor contracts.

## Verification

- Editor core: 26 ordinary tests passed; existing manual memory benchmarks remain ignored by default.
- Syntax: 25 ordinary tests passed, including cancellation and injected-language behavior.
- GPUI editor: 39 ordinary tests passed, including horizontal hit testing, search/undo and
  unchanged wrap measurement retention across edits and width changes.
  The refined composition case also passed with a different UTF-16 length from the replaced text.
- IDE: 42 ordinary tests passed, including shared versions, save completion and preview behavior.
- Fold/wrap regression: all 5 cases passed after width compaction and incremental mapping.
- Application check: `cargo check --locked -p oxideterm-gpui-app` passed.
- Memory comparison command:

```sh
cargo test --locked -p oxideterm-gpui-editor --lib editor_memory_performance -- --ignored --nocapture --test-threads=1
```

Native font/GPU behavior and the original periodic scrolling pause still require application
acceptance. The measurements above establish allocation reductions and headless behavior;
they do not establish a native frame-rate result.
