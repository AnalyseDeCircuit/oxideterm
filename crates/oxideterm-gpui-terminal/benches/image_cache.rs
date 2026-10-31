//! Measures the actual terminal image preparation/cache path, excluding protocol pixels.
#![allow(dead_code)]
// Including the owning module also includes its test imports under cargo bench.
#![allow(unused_imports)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

#[path = "../src/app/image_animation.rs"]
mod image_animation;
#[path = "../src/image_budget.rs"]
mod image_budget;
#[path = "../src/app/image_cache.rs"]
mod image_cache;

struct Allocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}
unsafe impl GlobalAlloc for Allocator {
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
static ALLOCATOR: Allocator = Allocator;

fn main() {
    use image_cache::ImageRenderCache;
    use oxideterm_terminal::*;
    use std::{sync::Arc, time::Instant};
    println!("frames,sample,prepare_us,peak_bytes,retained_bytes,steady_1000_us,steady_peak_bytes");
    for count in [1, 48] {
        let pixels: Arc<[u8]> = vec![127; 512 * 256 * 4].into();
        let frames = if count == 1 {
            Vec::new()
        } else {
            (0..count)
                .map(|index| TerminalImageFrame {
                    memory: None,
                    rgba: vec![index as u8; pixels.len()].into(),
                    delay_ms_numerator: 40,
                    delay_ms_denominator: 1,
                    gapless: false,
                })
                .collect::<Vec<_>>()
        };
        let snapshot = TerminalImageSnapshot {
            id: TerminalImageId(1),
            protocol: TerminalImageProtocol::Kitty,
            row: 0,
            col: 0,
            cols: 40,
            rows: 20,
            pixel_width: 512,
            pixel_height: 256,
            source_x: 0,
            source_y: 0,
            source_width: 512,
            source_height: 256,
            z_index: 0,
            placeholder: true,
            kitty: None,
            clipped_rows: 0,
            version: 1,
            data: Some(Arc::new(TerminalImageData {
                memory: None,
                id: TerminalImageId(1),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 512,
                height: 256,
                rgba: frames.first().map_or(pixels, |frame| frame.rgba.clone()),
                frames,
                animation: TerminalImageAnimationState {
                    running: count > 1,
                    ..Default::default()
                },
                name: None,
            })),
        };
        for sample in 0..6 {
            let before = LIVE.load(Relaxed);
            PEAK.store(before, Relaxed);
            let start = Instant::now();
            let mut cache = ImageRenderCache::default();
            let requests = cache.take_preparation_requests(std::slice::from_ref(&snapshot), true);
            let prepared = requests
                .iter()
                .cloned()
                .filter_map(ImageRenderCache::prepare_snapshot)
                .collect();
            cache.finish_preparations(&requests, prepared);
            let rendered = cache.cached_images(std::slice::from_ref(&snapshot), true);
            let elapsed = start.elapsed().as_micros();
            assert_eq!(
                rendered[0].render_image.as_ref().unwrap().frame_count(),
                count
            );
            let peak = PEAK.load(Relaxed) - before;
            let retained = LIVE.load(Relaxed) - before;
            let steady_before = LIVE.load(Relaxed);
            PEAK.store(steady_before, Relaxed);
            let start = Instant::now();
            for _ in 0..1000 {
                assert!(
                    cache
                        .take_preparation_requests(std::slice::from_ref(&snapshot), true)
                        .is_empty()
                );
                std::hint::black_box(cache.cached_images(std::slice::from_ref(&snapshot), true));
            }
            let steady = start.elapsed().as_micros();
            let steady_peak = PEAK.load(Relaxed) - steady_before;
            if sample > 0 {
                println!("{count},{sample},{elapsed},{peak},{retained},{steady},{steady_peak}");
            }
            std::hint::black_box(rendered);
        }
    }
}
