# Sixel lifecycle, decoding and compatibility

Updated: 2026-11-01. Baseline: `a1499807e` on macOS/Apple Silicon.

## Protocol choices

The base format follows the [DEC VT330/VT340 graphics manual, chapter 14](https://vt100.net/docs/vt3xx-gp/chapter14.html).
Capability queries and extensions follow [XTerm control sequences, patch 411](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html).
Mode 80 follows XTerm's polarity: set means fixed display, reset means scrolling.
The DEC manual describes the opposite polarity; using XTerm's behavior matches current applications.

| Behavior | Implementation | Evidence |
| --- | --- | --- |
| Validate complete canvas before pixel allocation; preserve session, frame and shared CPU limits | `graphics/sixel.rs`, `graphics/ingress.rs` in `oxideterm-terminal-graphics` | Oversized raster/repeat rejection; shared-budget integration test |
| Allocate final canvas once, including streams without raster dimensions | Sixel preflight and normalized decoder input | 800×600 benchmark below |
| Move the text cursor without writing spaces or line breaks into the document | `Term::advance_graphics_cursor`, common terminal event handler | Explicit text and cursor assertions; local model and real SSH packet-split regression |
| Follow full and local scrolling, history trimming, alternate screens, clear-all, reset and reflow | `graphics/sixel_grid.rs`, existing grid anchor records | Lifecycle test; local-region fragments have explicit visible row ranges and share one image owner |
| Text overwrite, combining and wide characters, ECH, EL and partial ED | Ordered rectangular erase records and disjoint visibility fragments | Explicit visible rectangles, final text and final-owner release assertions |
| ICH, DCH and insert mode | Ordered column shifts, preserving source coordinates | Explicit text, surviving rectangles and shifted source origins; oversized DCH clamps at the cursor |
| Native-size fragments within tmux panes | Pane-local clipping in `tmux.rs` | Actual composed pane snapshot retains 12×12 source pixels and clips every fragment to pane bounds |
| Preserve native pixel dimensions and macro/raster pixel aspect ratios | Preflight geometry and terminal image paint path | Macro ratio, raster override, source dimensions and partial-cell clipping assertions |
| Advertise Sixel only when enabled; answer primary DA, DECRQM and XTSMGRAPHICS | `vte::ansi`, `term/graphics.rs`, terminal handler | Exact response bytes, disabled capability, mode state, query ordering and error status assertions |
| Modes 80, 1070 and 8452 | Shared terminal graphics state | Fixed origin and unchanged cursor; private/shared palette behavior; cursor-right mode |
| RGB/HLS color definitions and shared register reuse | Existing image codec plus bounded session palette definitions | Explicit red/green pixels; DEC HLS red; private palette isolation |
| Opaque and transparent backgrounds | Transparent decoded pixels plus palette-resolved placement background | Current RGB background and transparent placement assertions |
| CAN/SUB cancellation, raw C1 DCS/ST, and UTF-8 continuation safety | Ingress framing and bounded discard state | Explicit bytes across every input split; cancellation and UTF-8 preservation |

Ordinary terminal cells and compact history rows gain no fields. Sixel anchors,
placement fragments and palette definitions are held separately. The palette is
created on first use and bounded to 256 entries. Fragments retain the original
shared pixels; removing the final placement retires the resource, with its memory
allowance released when the last snapshot/render owner drops it.

Queries are answered by the terminal processor in input order. Grid changes are
applied before the next graphics command creates resources, so a reset and upload
within one input packet cannot clear the new upload's anchors afterward.

Text erasures merge only when their union is an exact rectangle. Images wholly
in history retain anchors but do not enable text-erasure tracking. Fixed images
disable scroll-before-write line batching, because that transformation would
erase their pixels at the wrong rows. The regression sends 192 bytes through the
real parser to exceed its line-batching threshold, including after mode 80 is
reset while a previously fixed image remains.

## Performance and memory

Measurements use the existing `graphics_ingress` Criterion entry point in release
mode. The pre-change production sources were measured with the same model
benchmark, then the candidate sources were restored and compared. No application
window was launched.

The Sixel workload is 100 bands of `!800~-`, one red color, and no raster size.
The image is 800×600. Criterion mean decode/ingress time fell from **2.331 ms to
0.103 ms** (about **95.6% less time**). This workload specifically exposes repeated
canvas growth; color-heavy images need their own workload before assigning them
the same speedup.

Pixel allocation quantities below are calculated from the codec's allocation
sites and the explicit image dimensions:

| Quantity | Before | After |
| --- | ---: | ---: |
| Retained RGBA pixels | 1,920,000 bytes | 1,920,000 bytes |
| Cumulative canvas allocations plus final shared-pixel allocation | 98,880,004 bytes | 3,840,004 bytes |
| Largest simultaneous canvas/shared-pixel pair | Approximately 3.66 MiB | Approximately 3.66 MiB |

The cumulative reduction comes from replacing 100 growing canvases with one
preallocated canvas. Process RSS was not measured. Existing 256 MiB session
retention, 64 MiB single-frame and 512 MiB shared CPU allowances remain in force.

The full-model output benchmark feeds 1 MiB in 32 KiB packets through the graphics
ingress, the real ANSI processor and a 120×40 terminal with 1,000 history rows.
It includes grid writes and scrolling, with no renderer or PTY transport. Values
are Criterion mean estimates; changes compare the same workload against the
pre-change production sources.

| Workload | Before | After | Time change |
| --- | ---: | ---: | ---: |
| Plain text | 3.750 ms | 3.834 ms | +2.23% |
| ANSI colors | 4.401 ms | 4.398 ms | −0.06% |
| Unicode | 4.491 ms | 4.626 ms | +3.01% |
| Long CSI sequences | 3.962 ms | 3.923 ms | −1.00% |

Plain text was repeated with 30 samples after a noisier first comparison; its
95% change interval was +1.19% to +3.18%. Unicode's interval was +2.20% to +3.79%.
The added C1 detection has a measurable cost. In the scanner-only 1 MiB benchmark,
which omits grid parsing entirely, plain text increased from about 10.5 to 18.3 μs
and ANSI from 55.5 to 70.0 μs. The full-model measurements above include that cost.

Reproduce the comparisons with:

```sh
cargo bench -p oxideterm-terminal-graphics --bench graphics_ingress -- \
  --sample-size 20 --measurement-time 2 --warm-up-time 1
```

Criterion results are local build outputs under `target/criterion`; saved baseline
names used here are `sixel-before` and `sixel-model-before`.

### Text-erasure follow-up

This stage compares against the first integrated Sixel candidate described above,
using the same 1 MiB full-model workloads, 32 KiB packets, 20 samples, 2-second
measurement and 1-second warmup. The saved baseline is `sixel-erasure-before`.

| Workload | Before erasure support | After erasure support |
| --- | ---: | ---: |
| Plain text | 4.001 ms | 3.532 ms |
| ANSI colors | 4.830 ms | 4.227 ms |
| Unicode | 5.146 ms | 4.779 ms |
| Long CSI sequences | 4.414 ms | 4.027 ms |

No output regression was observed in these runs. Whole-machine conditions were
not fixed between runs, so the lower times are recorded without assigning an
acceleration percentage to this feature.

The focused erasure example splits a 16×6 image into four visible rectangles.
They share its original **384-byte** RGBA buffer. Clearing the final fragments
removes the resource; releasing the remaining snapshot then drops the last pixel
owner. Horizontal edits and pane clipping do not create cropped pixel buffers or
new source textures.

Two adjacent terminal boundary errors were corrected alongside the image updates:
oversized DCH now deletes only from the cursor to the right margin, and ED-above
on the second row clears the first row as well. Both have explicit final-text
expectations in the same command table as the image geometry.

## Validation and remaining acceptance

### Chafa display feedback

The first native Chafa screenshots exposed three integration errors:

- Protocol dimensions and offsets are device pixels, while GPUI paints in logical
  pixels. Native Sixel/Kitty sizes and Kitty offsets now convert using the window
  scale factor. At 2× scale a 40×40 image occupies 20×20 logical pixels, without
  enlarging it and clipping away half the source. Explicit cell-box scaling keeps
  its aspect-fit behavior.
- Kitty's cursor advance now uses the same grid-aware linefeed operation as Sixel.
  An image near the bottom scrolls into place before subsequent text is written.
- Local color, color-scheme and pixel-size replies are resolved by the PTY reader,
  instead of waiting for the UI event drain. Delayed replies previously arrived
  after Chafa left raw mode and appeared as shell input. SSH now also answers
  pixel-size queries, which its parser previously discarded.

The added regressions failed before their respective repairs: exact Retina bounds,
image/text positions at the bottom margin, and a real local PTY waiting for exact
color/size replies without any UI drain. The existing real SSH palette test now
also checks an explicit 960×1600-device-pixel response. No extra pixel buffers or
animation decode work are introduced. After rebuilding, the maintainer reported
successful GUI acceptance of the updated Kitty and Sixel paths.

Executed on macOS:

- Terminal regression suite: 215 passed, 5 ignored. This includes real SSH
  packet splitting, text erasure and composed tmux fragments. The revised
  fixed-image batch fixture also passed its focused rerun.
- Terminal graphics: 36 passed, 1 manual benchmark ignored; shared-budget integration passed.
- Alacritty terminal core: 150 passed; VTE: 38 passed.
- GPUI terminal: 218 passed, including Retina sizing and horizontal/vertical partial-cell clipping.
- Final focused Sixel cases, graphics/SSH regressions, application `cargo check`,
  formatting and whitespace checks passed.

The maintainer confirmed GUI acceptance in the existing macOS application on
2026-11-01. Windows and Linux runtime checks were not performed in this batch.
