//! Repeatable retained-heap and throughput measurements for the terminal history owner.
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{CellView, Dimensions, HistoryStorage};
use alacritty_terminal::index::{Column, Direction, Line, Point, Side};
use alacritty_terminal::term::cell::PackedHistory;
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

struct TrackedAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn allocated(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}

// The benchmark has no background workers; counters include requested heap bytes, not allocator
// size classes or process/GPU memory. The same instrumentation is used on both revisions.
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

#[derive(Clone, Copy)]
struct Size(usize);
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        40
    }
    fn screen_lines(&self) -> usize {
        40
    }
    fn columns(&self) -> usize {
        self.0
    }
}

fn main() {
    println!(
        "case,sample,cell_bytes,retained_bytes,after_read_bytes,resize_peak_bytes,fill_us,steady_us,read_us,resize_us,search_us,after_resize_bytes,after_search_bytes,reverse_search_us,narrow_us,encode_us"
    );
    for (name, cols) in [
        ("short120", 120),
        ("short400", 400),
        ("full400", 400),
        ("color400", 400),
        ("alternating400", 400),
        ("palette17", 400),
        ("palette64", 400),
        ("unique400", 400),
        ("unicode400", 400),
    ] {
        let mut input = String::new();
        let mut random = 0x12345678_u32;
        for line in 0..20_040 {
            if name.starts_with("short") {
                input
                    .push_str("INFO request completed path=/api/sessions status=200 elapsed_ms=12");
            } else {
                for col in 0..cols {
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    if name == "color400" && col % 40 == 0 {
                        input.push_str(&format!("\x1b[38;5;{}m", 1 + (col / 40) % 8));
                    } else if name == "alternating400" {
                        input.push_str(if col % 2 == 0 { "\x1b[31m" } else { "\x1b[32m" });
                    } else if name.starts_with("palette") {
                        let colors = if name == "palette17" { 17 } else { 64 };
                        input.push_str(&format!("\x1b[38;5;{}m", col % colors));
                    } else if name == "unique400" {
                        input.push_str(&format!(
                            "\x1b[38;2;{};{};{}m",
                            line % 256,
                            col / 256,
                            col % 256
                        ));
                    }
                    if name == "unicode400" {
                        if col % 4 == 0 {
                            input.push('界');
                        } else if col % 4 != 1 {
                            input.push(if col % 4 == 2 { 'e' } else { '\u{301}' });
                            if col % 4 == 3 {
                                input.push(' ');
                            }
                        }
                    } else {
                        input.push(char::from(b'a' + (random % 26) as u8));
                    }
                }
            }
            input.push_str("\x1b[0m\r\n");
        }
        let mut regex = RegexSearch::new("NEVER_MATCH_012345").unwrap();
        for sample in 0..6 {
            let before = LIVE.load(Relaxed);
            let mut term = Term::new(
                Config {
                    scrolling_history: 20_000,
                    ..Config::default()
                },
                &Size(cols),
                VoidListener,
            );
            let mut parser: Processor = Processor::new();
            let start = Instant::now();
            parser.advance(&mut term, input.as_bytes());
            let fill = start.elapsed().as_micros();
            // Isolate conversion from parser/grid work, reusing the same history allocation.
            let encode = {
                let source: Vec<_> = term.grid()[Line(0)]
                    .iter()
                    .map(CellView::into_owned)
                    .collect();
                let mut packed = None;
                let start = Instant::now();
                for _ in 0..20_040 {
                    packed = PackedHistory::encode(black_box(&source), packed);
                    black_box(&packed);
                }
                start.elapsed().as_micros()
            };
            let start = Instant::now();
            parser.advance(&mut term, input.as_bytes());
            let steady = start.elapsed().as_micros();
            let retained = LIVE.load(Relaxed) - before;
            let start = Instant::now();
            for line in -(term.grid().history_size() as i32)..40 {
                let text: String = (&term.grid()[Line(line)])
                    .into_iter()
                    .map(|cell| cell.c)
                    .collect();
                black_box(text);
            }
            let read = start.elapsed().as_micros();
            let after_read = LIVE.load(Relaxed) - before;
            let start = Instant::now();
            assert!(
                term.search_next(
                    &mut regex,
                    Point::new(Line(-(term.grid().history_size() as i32)), Column(0)),
                    Direction::Right,
                    Side::Left,
                    None
                )
                .is_none()
            );
            let search = start.elapsed().as_micros();
            let after_search = LIVE.load(Relaxed) - before;
            let start = Instant::now();
            assert!(
                term.search_next(
                    &mut regex,
                    Point::new(Line(39), Column(cols - 1)),
                    Direction::Left,
                    Side::Right,
                    None,
                )
                .is_none()
            );
            let reverse_search = start.elapsed().as_micros();
            PEAK.store(LIVE.load(Relaxed), Relaxed);
            let start = Instant::now();
            term.resize(Size(cols * 2));
            term.resize(Size(cols));
            let resize = start.elapsed().as_micros();
            let resize_peak = PEAK.load(Relaxed) - before;
            let after_resize = LIVE.load(Relaxed) - before;
            let start = Instant::now();
            term.resize(Size(cols / 2));
            term.resize(Size(cols));
            let narrow = start.elapsed().as_micros();
            if sample != 0 {
                println!(
                    "{name},{sample},{},{retained},{after_read},{resize_peak},{fill},{steady},{read},{resize},{search},{after_resize},{after_search},{reverse_search},{narrow},{encode}",
                    std::mem::size_of::<alacritty_terminal::term::cell::Cell>()
                );
            }
            black_box(term);
        }
    }
}
