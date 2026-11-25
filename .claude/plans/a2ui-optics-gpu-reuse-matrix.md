# GPU / wasm reuse matrix for a live glass-refraction shader (a2ui-rs, read-only survey 2026-11-25)

All paths relative to /home/user/a2ui-rs. wgpu resolves to the AdaWorldAPI fork, `wgpu 30.0.0`, `trunk` @ `c56b17b` (Cargo.lock:2315-2317; declared once at Cargo.toml:40,43). wasm-bindgen is pinned 0.2.130 (Cargo.lock:2238-2239). Toolchain 1.99.0 (rust-toolchain.toml:2).

## Matrix

| crate | (1) browser device/surface | (2) wasm32 + wasm-bindgen | (3) texture upload + WGSL sample | (4) on-demand vs continuous | (5) resize / DPR / pointer | (6) CPU-vs-GPU test |
|---|---|---|---|---|---|---|
| **a2ui-graph** | **YES, the only one.** `web.rs` `mount_with_preference`: probes `wgpu::util::is_browser_webgpu_supported` before touching the canvas (web.rs:85-104, rationale 210-254), `Backends::BROWSER_WEBGPU` or `Backends::GL` (web.rs:125-129), `create_surface(SurfaceTarget::Canvas)` (web.rs:265), adapter with `compatible_surface` (web.rs:274-297), device at `downlevel_webgl2_defaults()` (web.rs:307), surface config Fifo, non-sRGB format (web.rs:324-347). Module only on wasm32+wgpu (lib.rs:58-59). | **YES.** `crate-type = ["cdylib","rlib"]` (Cargo.toml:18); wasm-bindgen / -futures / web-sys / `wgpu features=["webgl"]` target-gated on wasm32 (Cargo.toml:38-71). Built by `scripts/build-graph-wasm.sh` (+simd128, `--release`, `wasm-bindgen --target web`, lines 78-80, 102). Measured: **4.1 MB .wasm, ~105 KB JS** (docs/WASM-INTEGRATION.md:83); 4 880 319 B with exports (same doc :246). Served by a2ui-field-web. | **NO.** One uniform block only (field.wgsl:2,11); no texture/sampler in gpu.rs (only test render target, gpu.rs:580-592). SDF rings via fragment derivatives (gpu.rs:11-16). | Per-call `frame()`; simulation only while `warm` (web.rs:503-508) but it still uploads + draws every call (web.rs:509-577). `is_warm()` lets a driver stop (web.rs:590). | `resize(w,h)` reconfigures, ignores 0 (web.rs:395-403). Pointer: `pointer_down/move/up`, `zoom` (web.rs:725-747). DPR is the JS host's job (field-web). | `the_field_actually_paints_pixels` (gpu.rs:548-686): renders to texture at webgl2 limits, reads back, asserts 200 < painted < W·H/2. Non-trivial-paint check, **not** a CPU reference comparison. Skips green with no adapter (gpu.rs:551-556). |
| **a2ui-paint** | **NO browser surface.** `gpu::GpuPainter::new` is headless: `compatible_surface: None` (lib.rs:741-768), so it can never get the wasm WebGL2 backend (gpu_lut_probe.rs:26-34 says the same). Renders rects in one flat colour to an offscreen texture (lib.rs:653-662, 822-903). Doc says windowed presentation needs unsafe (lib.rs:635-641); a2ui-graph does canvas surfaces under `forbid(unsafe_code)` (lib.rs:46 there), so that note is stale for wasm. | **NO.** No cdylib, no wasm-bindgen/web-sys (Cargo.toml:10-25). | **Partly, but dead.** `gpu_lut_probe.rs` uploads an `R16Uint` texture via `write_texture` and reads it with `textureLoad` (no sampler) (gpu_lut_probe.rs:18-22, 203-242). It is **not declared** in lib.rs (only `raster`, `optics`, `gpu` modules, lib.rs:38-44, 646), so it never compiles, and it uses pre-wgpu-30 names (`ImageCopyTexture`, gpu_lut_probe.rs:228). | N/A (headless, one call). | Pure `layout` + `hit_test` (lib.rs:1300-1312 test); no DOM events. | `ndc_vertices_are_six_per_rect_in_clip_space` (lib.rs:1315-1333), geometry only; `GpuPainter` itself is never run by a test. Optics math has CPU tests (optics.rs:551-581). |
| **a2ui-paint `raster` feature** | — | fontdue + png are pure Rust (Cargo.toml:22-23,33); never built for wasm32 here (no measurement). | **Text → RGBA8 without DOM: YES.** `render_rgba(layout, viewport, theme) -> (w,h,Vec<u8>)` draws rects + fontdue glyphs from embedded DejaVuSans (raster.rs:13-15, 24, 184-254). Font is re-parsed on every call (raster.rs:192). Font asset is 759 720 B (assets/DejaVuSans.ttf). | — | — | PNG non-empty / deterministic test (raster.rs:286; tests/patient_screen.rs:6,112). |
| **a2ui-paint `optics`** | — | Pure arithmetic, no deps (optics.rs:39-40). | Produces displacement / specular / rear maps as RGBA bytes (optics.rs:205-273, 460). Geometry is **only the rounded rectangle** (`rounded_rect`, optics.rs:421) with a 1-D cross-section reused around it (optics.rs:14-16, 184); profiles Circle/Squircle (optics.rs:48-80). **No raindrop, sphere or prism SDF exists.** | — | — | `optics_timing` example reports µs per stage (examples/optics_timing.rs:1-9). |
| **a2ui-paint-web** | NO; explicitly no wgpu (Cargo.toml:12-17; main.rs:27). | NO (native axum server). | NO GPU. `/lab/optics` applies the CPU maps with SVG `feDisplacementMap` + `backdrop-filter` (lab.html:21,52-59; lab.rs:43-46). Already has the full glass parameter UI incl. a "page of text" backdrop (lab.html:66-96). | Server renders on request. | Browser form controls. | Route tests only (lab.rs:249-253). |
| **a2ui-field-web** | No device itself; serves the a2ui-graph pkg. | Serves `a2ui_graph.js`/`_bg.wasm`/`.d.ts` from a whitelist (main.rs:120-128), dir from `FIELD_PKG` (main.rs:130-135). | — | **Continuous** rAF loop, ignores `is_warm` (main.rs:205-206). | DPR capped at 2 (main.rs:170-174), pointer + wheel + window resize listeners (main.rs:183-203). | Route whitelist test (main.rs:434-453). |
| **a2ui-wasm** | NO GPU. | cdylib+rlib (Cargo.toml:15); wasm-bindgen behind feature `wasm` (Cargo.toml:27-30; lib.rs:576-590). No build script, no size measurement. | NO. | — | — | Native tests only. |
| **a2ui-solid / a2ui-solid-web** | NO. Zero deps (a2ui-solid Cargo.toml:12-21); server-side SVG, "wgpu off" (solid-web main.rs:459). | NO. | NO. | — | — | — |

Dockerfiles (`Dockerfile.railway`, `.pwa.railway`, `.solid.railway`, repo root) build only native server binaries (`cargo build --release -p ${BIN_NAME}`, Dockerfile.railway:160); **none builds a wasm target or a2ui-field-web.**

## WebGL2 fallback: is it exercised?
Only in a2ui-graph. The browser path drives it explicitly (web.rs:85-104, 125-129, 265-307) and the doc records a headless-Chromium measurement where `navigator.gpu` exists but yields no adapter (web.rs:211-223). The native GPU test uses `downlevel_webgl2_defaults` limits (gpu.rs:561-564) but runs on whatever native adapter exists; no automated browser test exists.

## Known limits
- WebGL2 forbids storage buffers, compute, storage-texture `textureLoad`, `first_instance != 0` (gpu.rs:1-9). A sampled `texture_2d<f32>` + filtering sampler in a fragment shader is GLES3-core and fine.
- Uniform blocks must be 16-byte aligned (gpu.rs:33-37).
- One canvas holds one context family forever; choose backend before `create_surface` (web.rs:226-237; WASM-INTEGRATION.md:188-191).
- Fork behaviour relied on: null WebGPU adapter mapped to `RequestAdapterError`, public `is_browser_webgpu_supported` (web.rs:239-254); API shape is wgpu 30 (`queue.present(frame)`, web.rs:577; `PollType`, gpu.rs:669).
- Docs are partly stale: WASM-INTEGRATION.md:57 and :96 say toolchain 1.98.1; the pin is 1.99.0 (the script reads the pin, build-graph-wasm.sh:36-41).

## Disk / compile cost
Recorded: output size only (4.1 MB wasm, 105 KB JS; WASM-INTEGRATION.md:83). Compile time and target-dir size for wasm32: **not recorded anywhere; unknown.** Current `target/` is 595 MB with only `debug/` and `release/` (no `wasm32-unknown-unknown/` present).

## Recommendation: smallest path
1. **Reuse a2ui-graph's surface code, not its crate charter.** a2ui-graph is ruled the graph-field renderer (a2ui-graph lib.rs:1-27); the glass material belongs to the paint tier, next to `optics`. Host the shader in **a2ui-paint**: add a `glass` WGSL + pipeline under the existing `wgpu` feature, and a wasm32-target-gated `web` module that copies `BackendPreference::resolve` + `mount_with_preference` + `resize` + `frame` (web.rs:62-129, 201-403, 503-583). Needs: `crate-type = ["cdylib","rlib"]` and the same target-gated deps block as a2ui-graph Cargo.toml:38-71.
2. **Background texture of text:** call `raster::render_rgba` (raster.rs:184) on the resolved `PaintLayout`, upload once with `queue.write_texture` to an `Rgba8Unorm` texture, sample with a linear sampler. Re-render only when the surface changes (cache the parsed `Font`; today it is re-parsed per call, raster.rs:192).
3. **Optics in the shader:** port `rounded_rect` SDF, `Profile::height/slope`, Snell displacement and Schlick F0 (optics.rs:57-69, 143, 178, 421) to WGSL; use the CPU maps (`render_displacement`, optics.rs:205) as the CPU reference for a readback parity test modelled on gpu.rs:548-686. Raindrop/sphere/prism need new SDFs; none exist.
4. **Build:** generalise `scripts/build-graph-wasm.sh` (hardcodes `-p a2ui-graph`, `a2ui_graph.wasm`, `FieldHandle` symbol check: lines 29, 79, 88) to take crate + export symbol, or add a sibling script.
5. **Serve:** add the pkg routes beside `/lab/optics` in a2ui-paint-web (whitelist pattern of field-web main.rs:120-128) so the GPU path can be compared with the existing SVG-filter lab on the same parameters. Drive frames on demand (pointer/param change) rather than field-web's unconditional rAF loop (main.rs:205).

## Deviation, 2026-11-25: raw WebGL2, not wgpu

The operator ruled raw WebGL2 from Rust/wasm over wgpu: wgpu's adapter and device
warm-up shows as a first-frame flicker. The recommendation above (host in a2ui-paint under
the `wgpu` feature) is therefore not followed. Built instead:

- `crates/a2ui-optics-gl` — `DropUniforms` (every shader constant, computed in Rust from
  `a2ui_paint::optics::{Droplet, Medium, KeyLight}`), `cpu_sample_points` (CPU reference),
  `drop.frag` (GLSL ES 3.00, `refract_offset` ported line for line), and a wasm32-only
  `DropRenderer` over `web-sys` WebGL2. No wgpu, no second optical model.
- `scripts/build-optics-wasm.sh` — the build-graph-wasm.sh guards for this crate.
- `/lab/optics/drop-gl` in a2ui-paint-web, plus a three-file whitelist at
  `/lab/optics/gl/{file}` (`OPTICS_GL_PKG`, else the script's output). The Railway image
  does not build the pkg yet; there the page reports that the GPU renderer did not start.

The GPU canvas covers only the drop's square; the page is a 2-D canvas shown as itself and
uploaded once as the texture. A first version shaded the whole viewport per frame.

## Measurements (headless Chromium, SwiftShader software WebGL — CPU, not a GPU)

| | desktop 1280×800 @1× | phone 390×844 @3× |
|---|---|---|
| wasm | 117.8 KiB (uncompressed, no wasm-opt) | same |
| wasm fetch + compile + instantiate | 20–63 ms | 17–26 ms |
| WebGL2 context + shader compile | 19–136 ms | 129–140 ms |
| wasm linear memory | 1.13 MiB | 1.13 MiB |
| page raster (2-D canvas) | 4–6 ms | 1.5 ms |
| page texture | 3.9 MiB | 11.3 MiB |
| texture upload call | 2.0–2.5 ms | 0.3–0.4 ms (a lower bound: the copy may be deferred) |
| load to first frame | 240–400 ms | 475–566 ms |
| moving frame, whole viewport shaded | mean 23.8 ms | mean 72.6 ms |
| moving frame, drop square only (184² / 544²) | **mean 1.4–1.9 ms**, p95 1.6–3.9 ms | **mean 9.3–10.0 ms**, p95 11.5–12.3 ms |
| static | 0 frames: nothing is drawn without a change | same |

Frame times are GPU-synced (a 1-pixel readback after the draw; `gl.finish()` alone timed
0.1 ms, i.e. it did not wait). The mean over 120 frames is the reliable figure; the browser
clock is 100 µs without cross-origin isolation. All of this is a software rasterizer; a real
GPU, and a real phone, are not measured.

## Parity

`DropRenderer::parity()` draws the drop over a 256×256 coordinate texture (nearest
filtering), reads it back and compares every channel's sampled texel with
`cpu_sample_points`. Result at dispersion 0 and ×12: 31,072 interior pixels, `mismatch 0`,
`off_by_one 0`, `leak 0` — exact to the texel. `control_mismatch` (CPU without dispersion)
is 0 at ×0 and 23,096 at ×12, so the check can see dispersion.

Disable run: the shader changed to use the green index for red. With a one-texel tolerance
parity still reported `mismatch 0` (the red/green difference is ≤ 1 texel for water), so the
tolerance alone was vacuous. Counting exact-texel differences separately: the broken shader
gives `off_by_one 9746`, the real one 0. Both numbers are reported.

## Comparison of the three paths (so far)

| path | per-change cost measured | moving the drop |
|---|---|---|
| cached server PNG + SVG filter (`/lab/optics/drop`) | map 0.8 ms + PNG 6.3 ms server-side, cache hit 0.1 µs | no recompute; Chromium-only (SVG backdrop-filter) |
| client wasm fields | not built | — |
| GPU shader (`/lab/optics/drop-gl`) | 1.4 ms per frame (software GL, desktop) | one uniform + one draw; any WebGL2 browser |

Open: the client-wasm-fields path; a real-GPU and real-phone measurement; whether the
page texture should come from the DOM text (here it is a 2-D canvas raster, so the text
under the drop is not selectable DOM text).
