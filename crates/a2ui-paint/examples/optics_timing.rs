//! Release-mode cost of the optical engine, stage by stage.
//!
//! ```sh
//! cargo run --release -p a2ui-paint --features optics-png --example optics_timing
//! ```
//!
//! For each component size it reports the median of N runs (µs) for: the cross-section, the
//! displacement map, the rim map, the rear-light field (at the half-resolution box the lab
//! requests), PNG encoding of each, and a cache hit (looking up already encoded bytes). A
//! counting allocator reports allocations, bytes and the peak live heap per stage.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use a2ui_paint::optics::{Glass, RearLight, png_rgba, rear_field};

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
    let glass = Glass {
        strength: 0.53,
        ..Glass::DEFAULT
    };
    let light = RearLight {
        hue: 352.0,
        sat: 0.75,
        halo: 0.45,
    };
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
        let m = (0.6 * w.min(h) as f32).round() as u32 + 40;
        let (rw, rh) = ((w + 2 * m).div_ceil(2), (h + 2 * m).div_ceil(2));
        println!("{name} {w}×{h} (rear field {rw}×{rh}, half resolution)");
        row("cross-section", measure(|| glass.cross_section()));
        let cs = glass.cross_section();
        row(
            "displacement map",
            measure(|| glass.render_displacement(w, h, &cs)),
        );
        row("rim (Fresnel) map", measure(|| glass.render_specular(w, h)));
        row(
            "rear-light field",
            measure(|| rear_field(rw, rh, light, glass.strength)),
        );
        let disp = glass.render_displacement(w, h, &cs);
        let spec = glass.render_specular(w, h);
        let rear = rear_field(rw, rh, light, glass.strength);
        row(
            "png displacement",
            measure(|| png_rgba(w, h, &disp).unwrap()),
        );
        row("png rim", measure(|| png_rgba(w, h, &spec).unwrap()));
        row("png rear", measure(|| png_rgba(rw, rh, &rear).unwrap()));
        // cached: the encoded maps kept by key; a hit is a lookup and a reference count bump
        let mut cache: HashMap<String, std::sync::Arc<Vec<u8>>> = HashMap::new();
        let key = format!("disp:{w}x{h}:{:?}", glass);
        cache.insert(
            key.clone(),
            std::sync::Arc::new(png_rgba(w, h, &disp).unwrap()),
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
            measure(|| png_rgba(rw, rh, &rear_field(rw, rh, ocean, glass.strength)).unwrap()),
        );
        // everything from nothing: what an uncached size change costs
        row(
            "uncached, all maps",
            measure(|| {
                let cs = glass.cross_section();
                (
                    png_rgba(w, h, &glass.render_displacement(w, h, &cs)).unwrap(),
                    png_rgba(w, h, &glass.render_specular(w, h)).unwrap(),
                    png_rgba(rw, rh, &rear_field(rw, rh, light, glass.strength)).unwrap(),
                )
            }),
        );
    }
}
