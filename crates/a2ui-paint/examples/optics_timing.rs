//! Release-mode cost of the optical engine, stage by stage.
//!
//! ```sh
//! cargo run --release -p a2ui-paint --features optics-png --example optics_timing
//! ```
//!
//! For each component size it reports the median of N runs (µs) for: the surface maps of
//! the Aqua preset (`Surface::render_maps`: displacement, highlight, rim in one pass), the
//! rear-light field (at the half-resolution box the lab requests), PNG encoding of each, and a
//! cache hit (looking up already encoded bytes). A counting allocator reports allocations,
//! bytes and the peak live heap per stage.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use a2ui_paint::optics::surface::Surface;
use a2ui_paint::optics::{RearLight, png_rgba, rear_field};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size(), Relaxed);
        let live = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(live, Relaxed);
        // SAFETY: same layout the caller passed.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        // SAFETY: `p` came from `alloc` with this layout.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

const RUNS: usize = 41;

/// Median µs over RUNS, plus allocations / bytes / peak growth of one run.
fn measure<T>(mut f: impl FnMut() -> T) -> (f64, usize, usize, usize) {
    let mut times = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let t = Instant::now();
        black_box(f());
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(f64::total_cmp);
    let (a0, b0) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
    let live0 = LIVE.load(Relaxed);
    PEAK.store(live0, Relaxed);
    black_box(f());
    let peak = PEAK.load(Relaxed) - live0;
    (
        times[RUNS / 2],
        ALLOCS.load(Relaxed) - a0,
        BYTES.load(Relaxed) - b0,
        peak,
    )
}

fn row(stage: &str, (us, n, bytes, peak): (f64, usize, usize, usize)) {
    println!(
        "  {stage:<22} {us:>10.1} µs  {n:>4} allocs  {:>9.1} KiB  peak {:>9.1} KiB",
        bytes as f64 / 1024.0,
        peak as f64 / 1024.0
    );
}

fn main() {
    let light = RearLight {
        hue: 352.0,
        sat: 0.75,
        halo: 0.45,
    };
    let strength = 0.53;
    println!(
        "optics_timing: median of {RUNS} runs, release build expected (debug assertions: {})",
        cfg!(debug_assertions)
    );
    for (name, w, h) in [
        ("mobile control", 120u32, 56u32),
        ("mobile card", 343, 120),
        ("desktop card", 420, 200),
        ("panel", 900, 560),
        ("large panel", 1200, 800),
    ] {
        let surface = Surface::from_query(&format!(
            "preset=aqua&w={w}&h={h}&cornerpx=14&strength={strength}"
        ));
        let m = (0.6 * w.min(h) as f32).round() as u32 + 40;
        let (rw, rh) = ((w + 2 * m).div_ceil(2), (h + 2 * m).div_ceil(2));
        let (bw, bh) = surface.map_box();
        println!("{name} {w}×{h} (map box {bw}×{bh}, rear field {rw}×{rh}, half resolution)");
        row("surface maps", measure(|| surface.render_maps()));
        row(
            "rear-light field",
            measure(|| rear_field(rw, rh, light, strength)),
        );
        let maps = surface.render_maps();
        let rear = rear_field(rw, rh, light, strength);
        row(
            "png displacement",
            measure(|| png_rgba(bw, bh, &maps.displacement).unwrap()),
        );
        row(
            "png highlight",
            measure(|| png_rgba(bw, bh, &maps.specular).unwrap()),
        );
        row("png rear", measure(|| png_rgba(rw, rh, &rear).unwrap()));
        // cached: the encoded maps kept by key; a hit is a lookup and a reference count bump
        let mut cache: HashMap<String, std::sync::Arc<Vec<u8>>> = HashMap::new();
        let key = surface.to_query();
        cache.insert(
            key.clone(),
            std::sync::Arc::new(png_rgba(bw, bh, &maps.displacement).unwrap()),
        );
        row("cache hit (lookup)", measure(|| cache.get(&key).cloned()));
        // a colour change: only the rear field is recomputed and encoded
        let ocean = RearLight {
            hue: 212.0,
            sat: 0.8,
            halo: 0.45,
        };
        row(
            "colour change",
            measure(|| png_rgba(rw, rh, &rear_field(rw, rh, ocean, strength)).unwrap()),
        );
        row(
            "uncached, all maps",
            measure(|| {
                let m = surface.render_maps();
                (
                    png_rgba(bw, bh, &m.displacement).unwrap(),
                    png_rgba(bw, bh, &m.specular).unwrap(),
                    png_rgba(rw, rh, &rear_field(rw, rh, light, strength)).unwrap(),
                )
            }),
        );
    }
}
