//! Measures the same bounded-copy and result-resolution path used by background search.
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use oxideterm_terminal::{
    GraphicsOptions, TerminalSearchOptions, TerminalSearchQuery, TerminalSession,
};

struct TrackedAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn allocated(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}

// The search runs synchronously here. Count requested heap, not process RSS or allocator slack.
unsafe impl GlobalAlloc for TrackedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            LIVE.fetch_sub(layout.size(), Relaxed);
            allocated(size);
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: TrackedAllocator = TrackedAllocator;

fn main() {
    println!(
        "case,sample,mode,search_us,resolve_us,peak_bytes,result_bytes,resolved_bytes,matches"
    );
    for case in ["full400", "color400", "alternating400", "unicode400"] {
        let mut input = String::new();
        for _ in 0..20_040 {
            for col in 0..400 {
                if case == "color400" && col % 40 == 0 {
                    input.push_str(&format!("\x1b[38;5;{}m", 1 + (col / 40) % 8));
                } else if case == "alternating400" {
                    input.push_str(if col % 2 == 0 { "\x1b[31m" } else { "\x1b[32m" });
                }
                if case == "unicode400" {
                    if col % 4 == 0 {
                        input.push_str("界e\u{301} ");
                    }
                } else {
                    input.push(char::from(b'a' + (col % 26) as u8));
                }
            }
            input.push_str("\x1b[0m\r\n");
        }
        let mut terminal =
            TerminalSession::recording_playback(400, 40, GraphicsOptions::default(), 20_000);
        terminal.feed_recording_output(input.as_bytes());
        let source = terminal.search_source().unwrap();
        for sample in 0..6 {
            for (mode, query, regex) in [
                ("literal-miss", "NEVER_MATCH_012345", false),
                ("literal-hit", "zabcdefghijklm", false),
                ("regex-miss", "[A-Z]{12}", true),
                (
                    "regex-hit",
                    if case == "unicode400" { "^界" } else { "^abc" },
                    true,
                ),
                (
                    "cross-line",
                    if case == "unicode400" {
                        "界e\u{301} \n界"
                    } else {
                        "hij\nabc"
                    },
                    false,
                ),
            ] {
                let query = TerminalSearchQuery::new(
                    query,
                    TerminalSearchOptions {
                        case_sensitive: true,
                        regex,
                    },
                )
                .unwrap();
                let before = LIVE.load(Relaxed);
                PEAK.store(before, Relaxed);
                let start = Instant::now();
                let result = source.search(&query, &|| false);
                let search_us = start.elapsed().as_micros();
                let result_bytes = LIVE.load(Relaxed).saturating_sub(before);
                let start = Instant::now();
                let matches = source.resolve_search_result(result);
                let resolve_us = start.elapsed().as_micros();
                let resolved_bytes = LIVE.load(Relaxed).saturating_sub(before);
                let peak = PEAK.load(Relaxed).saturating_sub(before);
                if sample != 0 {
                    println!(
                        "{case},{sample},{mode},{search_us},{resolve_us},{peak},{result_bytes},{resolved_bytes},{}",
                        matches.len()
                    );
                }
                black_box(matches);
            }
        }
    }
}
