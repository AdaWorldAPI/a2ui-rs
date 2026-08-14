# Taking the Field Client to the Browser

How `a2ui-graph` becomes a `.wasm` that draws a graph field on a canvas,
what each step is for, and the two ways the build silently produces
nothing while reporting success.

Written 2026-08-14, from doing it. Every number below is a measurement
with the command that produced it, so it can be re-checked rather than
believed.

---

## The shape

```
ABI v3 bytes ──(borrow, no copy)──► GraphAbi ──► Layout (SoA f32)
                                       │             │
                                       │      ndarray::simd::F32x16
                                       │             │
                                       └──► Scene ◄──┘
                                             │
                                    instance + index buffers
                                             │
                                       wgpu (WebGPU / WebGL2)
                                             │
                                    #[wasm_bindgen] FieldHandle
                                             │
                                           JS + <canvas>
```

Two accelerations, two different machines, and they are not
interchangeable:

- **The GPU draws.** Instanced SDF rings and an indexed line list. Nothing
  is allocated per primitive; the ABI lanes *are* the buffers.
- **The CPU lays out.** The force simulation is the only per-frame
  arithmetic in the crate, and it runs through `ndarray::simd` — which on
  wasm32 means real SIMD128 (see below), not a hopeful autovectoriser.

The polyfill matters most exactly here. On x86 a scalar loop often
vectorises by accident; on wasm32 without `+simd128` there are **no vector
registers at all**, so "we rely on LLVM" degrades to scalar with no error
and no warning.

---

## Build

```bash
./scripts/build-graph-wasm.sh            # -> crates/a2ui-graph/pkg/
```

That is the whole thing. It runs what the two commands below do, and
checks the four preconditions that otherwise fail silently or confusingly:

```bash
RUSTFLAGS='-C target-feature=+simd128' \
  cargo +1.97.1 build -p a2ui-graph --features web \
              --target wasm32-unknown-unknown --release

wasm-bindgen --target web --out-dir pkg \
  target/wasm32-unknown-unknown/release/a2ui_graph.wasm
```

Output (measured 2026-08-14): a **4.1 MB** `.wasm`, ~105 KB of JS glue, and
TypeScript declarations. It is a build artifact and is git-ignored —
regenerated from the crate, never a source of truth.

### What the script guards, and why each one is worth a check

| Guard | The failure without it |
|---|---|
| `wasm-bindgen` CLI version == `Cargo.lock` pin | glue for a different ABI; surfaces at runtime, in a browser |
| `wasm32-unknown-unknown` installed **for that toolchain** | `can't find crate for core` — reads like a broken install, is a one-line fix |
| `FieldHandle` symbol present in the module | the client was linked out; see §"The two silent failures" |
| toolchain pin (`1.97.1`, override with `RUST_TOOLCHAIN`) | a bare `cargo` names two different rust-version floors and no fix |

Targets are **per-toolchain**, which is why the second one exists: switching
toolchains loses the target, and the resulting error names neither.

The script deliberately does **not** run `wasm-opt`. That is a size/time
trade-off for the consumer to make; it is not a correctness step, and a
build script that quietly changes the shipped bytes is worse than one that
leaves the choice visible.

Three things this command line depends on, each of which was a failure
before it was a flag:

| Piece | Why |
|---|---|
| `crate-type = ["cdylib", "rlib"]` | without `cdylib` the `web` feature builds an **rlib** — no `.wasm` exists to load |
| `#[wasm_bindgen]` on the client | without exports the linker **removes the whole client**, see below |
| `-C target-feature=+simd128` | without it the polyfill takes its scalar arm — correct, but no vectors |

`--features web` implies `wgpu`, because a canvas surface without a device
would be a feature that compiles and cannot draw.

---

## Using it from JS

```js
import init, { FieldHandle } from './pkg/a2ui_graph.js';

await init();

const bytes = new Uint8Array(await (await fetch('/graph.abi')).arrayBuffer());
const field = await FieldHandle.mount(canvas, bytes);

canvas.addEventListener('pointerdown', e => {
  const hit = field.pointerDown(e.offsetX, e.offsetY);
  if (hit) openPreview(hit.classid, hit.identity);   // an ADDRESS, not a handler
});
canvas.addEventListener('pointermove', e => field.pointerMove(e.offsetX, e.offsetY));
canvas.addEventListener('pointerup',   () => field.pointerUp());
canvas.addEventListener('wheel', e => field.zoom(e.offsetX, e.offsetY, e.deltaY < 0 ? 1.1 : 0.9));

(function frame() {
  field.frame();
  requestAnimationFrame(frame);
})();

// The one thing Rust cannot reclaim for you:
// removeEventListener(...) then field.detach();
```

### Why the JS surface has its own shape

`FieldHandle` is a wrapper over `FieldClient`, not attributes on it.
`Gesture` is an enum with payloads (`Down(f32, f32)`), and wasm-bindgen
carries only C-like enums across the boundary. Rather than flatten the Rust
type to suit the FFI, the boundary gets one method per gesture. Rust keeps
the enum it wants; JS gets the calls it wants; neither is bent to the other.

Two deliberate choices at that boundary:

- **`mount` takes owned bytes.** The handle outlives the call; a borrowed
  slice would tie it to a buffer JS is free to release. One copy at mount,
  never per frame.
- **`trace` returns an empty array, never `null`.** "No path" and "a path of
  no nodes" are the same thing to a renderer, and an *either-array-or-null*
  return is a forgotten null-check waiting to happen.

### Memory

There is no per-node object, so there is nothing to leak on the Rust side,
and wgpu frees device, surface and buffers on drop with no GC in the way.
`detach()` exists so the consumer picks the moment — and because **DOM event
listeners are the one thing Rust cannot reclaim**. They are named here rather
than hidden behind an "it cleans itself up" claim, which is how leaks ship.

---

## The two silent failures

Neither produces a warning. Both were hit here.

### (a) `web` builds an rlib

Without `[lib] crate-type = ["cdylib", "rlib"]`, `--features web --target
wasm32-unknown-unknown` succeeds and emits **no `.wasm` at all**. The feature
compiles; nothing can load it. A feature that builds and cannot ship is worse
than one that is missing, because it looks finished.

### (b) The client is linked out

Nastier, and it survives (a). **Only exported items survive the link into a
cdylib.** `web.rs` originally carried zero `#[wasm_bindgen]` attributes, so
nothing in JS could construct a `FieldClient`, so the linker dropped the whole
client — `Layout`, `Scene`, `FieldRenderer`, everything.

Measured, release, `--features web`:

| | before exports | after exports |
|---|---|---|
| module size | 1 269 153 B | **4 880 319 B** |
| SIMD instructions | **2** | **13 375** |
| `Layout` / `FieldHandle` symbols | **0** | 121 |

The size difference alone is the tell: a module that shrinks when you add a
subsystem did not include it.

---

## Verifying, without fooling yourself

`llvm-objdump` is unusably slow on `.wasm`; use wabt.

```bash
apt-get install -y wabt

# Is the code even in the module?
wasm-objdump -x a2ui_graph.wasm | grep -ci 'a2ui_graph.*layout\|FieldHandle'
# 121

# Does it carry vectors?
wasm-objdump -d a2ui_graph.wasm | grep -cE 'f32x4|i32x4|v128\.'
# 13375     with -C target-feature=+simd128
# 0         without
```

**The zero is what makes the other number mean anything.** A count on its
own proves nothing — any large module contains vectorised dependency code,
and a raw `0xFD` byte scan is dominated by data. Always build both ways.

### The trap in measuring one function

Do not isolate a function with a text window around its name:

```bash
# WRONG: /integrate/ matches a string, not a function boundary, and the rlib
# is full of ndarray's own vectorised code.
llvm-objdump -d lib.rlib | awk '/integrate/{f=1} f&&/^$/{f=0} f' | grep -c v128
```

Ask the symbol table:

```bash
SYM=$(llvm-nm target/wasm32-unknown-unknown/debug/liba2ui_graph.rlib \
       | grep 'Layout9integrate17' | awk '{print $3}')
llvm-objdump -d --disassemble-symbols="$SYM" \
  target/wasm32-unknown-unknown/debug/liba2ui_graph.rlib \
  | grep -cE 'f32x4|v128\.'
# 800
```

The text-window form was used here first and returned **801** where the
symbol-scoped answer is **800**. Right by luck — and a measurement that is
right by luck is not a measurement. What exposed it was not the number but a
*contradiction*: 2 SIMD instructions in the shipped module against 801 in the
rlib. Chasing that gap found failure (b). A sloppy receipt cost a real bug
its early detection.

---

## What is vectorised, and what deliberately is not

Only the **integrate** phase of `Layout::step`. Of the three phases it is the
only elementwise one: eight parallel lanes in, two out, no gather. Repulsion
walks a uniform grid and springs scatter along an edge list — both irregular,
and lanes would cost more in shuffles than the arithmetic saves.

Vectorising the phase that is shaped for it, and saying plainly why the others
are left alone, is worth more than a blanket claim.

The pin veto is branchless: a lane has no `continue`, so the veto becomes a
`select` — compute for every node, keep the old value where the node is
pinned. The mask is built from `pinned` on the spot rather than kept as a
mirrored `f32` lane, because a second copy of the same fact is a desync
waiting for its day.

### What is NOT claimed

**No speedup is measured.** These numbers show the vector path *exists* and
is bit-parity with the scalar reference (tolerance `1e-4`, because the
polyfill documents ULP divergence between backends). Whether it is faster in
a browser is a different measurement, on a real device, and it has not been
made. An instruction count is not a benchmark.

---

## Companion docs

- `AdaWorldAPI/ndarray` `.claude/knowledge/wasm-simd-consumer-guide.md` —
  the polyfill half: feature gates, the wasm32 backend's type coverage, and
  the cross-backend semantic divergences a parity test must tolerate.
- `AdaWorldAPI/OGAR` `docs/WASM-CONSUMER-GUIDE.md` — the substrate half:
  what an ABI-fed browser consumer may assume about addresses and ClassViews.
