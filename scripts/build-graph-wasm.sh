#!/usr/bin/env bash
#
# Build the a2ui-graph field client for the browser.
#
#   ./scripts/build-graph-wasm.sh [out-dir]     # default: crates/a2ui-graph/pkg
#
# Produces an ES module (`--target web`): a `.wasm`, its JS glue, and the
# TypeScript declarations. Consumers `import init, { FieldHandle } from
# './pkg/a2ui_graph.js'`.
#
# Every step below exists because its absence is SILENT. See
# docs/WASM-INTEGRATION.md for the measurements; the short version:
#
#   no cdylib          -> builds an rlib, no .wasm at all
#   no #[wasm_bindgen] -> the linker removes the whole client, and the
#                         module SHRINKS while looking like it built
#   no +simd128        -> the ndarray polyfill takes its scalar arm; wasm32
#                         has no vector registers otherwise
#   CLI/crate mismatch -> wasm-bindgen fails, or worse, emits glue for a
#                         different ABI
#
# None of those produce a warning on their own, so this script checks for
# them and fails loudly instead.

set -euo pipefail

cd "$(dirname "$0")/.."
OUT="${1:-crates/a2ui-graph/pkg}"
WASM=target/wasm32-unknown-unknown/release/a2ui_graph.wasm

# The workspace toolchain. 1.97.1 by ALIGNMENT — Dockerfile.railway documents
# it as this workspace's toolchain, and the ndarray fork's own manifest now
# requires 1.97, so a bare `cargo` (whatever the default happens to be) fails
# with a rust-version error that names two different floors and no fix.
# Override for a one-off: `RUST_TOOLCHAIN=1.98 ./scripts/build-graph-wasm.sh`.
TOOLCHAIN="${RUST_TOOLCHAIN:-1.97.1}"

# ── 1. The CLI must match the crate ────────────────────────────────────────
# wasm-bindgen's generated glue is tied to the exact version of the crate
# that produced the module. A drifted CLI is the single most common failure
# in this toolchain, so read the pin from Cargo.lock rather than trusting
# whatever is on PATH.
want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/[",]/,""); print $3; exit}' Cargo.lock)
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen $want not on PATH. Prebuilt binaries:" >&2
  echo "  https://github.com/rustwasm/wasm-bindgen/releases/tag/$want" >&2
  exit 1
fi
have=$(wasm-bindgen --version | awk '{print $2}')
if [ "$have" != "$want" ]; then
  echo "wasm-bindgen CLI is $have but Cargo.lock pins $want." >&2
  echo "Install the matching one; a mismatched CLI emits glue for a" >&2
  echo "different ABI and the failure surfaces at runtime, in a browser." >&2
  exit 1
fi

# ── 1b. The target must be installed FOR THAT TOOLCHAIN ────────────────────
# Targets are per-toolchain, so switching toolchains loses the target and the
# failure is `can't find crate for core` — which reads like a broken
# installation rather than a one-line fix.
if ! rustup target list --installed --toolchain "$TOOLCHAIN" 2>/dev/null \
     | grep -qx wasm32-unknown-unknown; then
  echo "wasm32-unknown-unknown is not installed for toolchain $TOOLCHAIN." >&2
  echo "  rustup target add wasm32-unknown-unknown --toolchain $TOOLCHAIN" >&2
  exit 1
fi

# ── 2. Build with SIMD128 ──────────────────────────────────────────────────
# The per-frame layout integration goes through ndarray::simd. Without this
# flag the dispatch falls through to scalar: correct results, no vectors,
# no error.
echo "building (simd128, rust $TOOLCHAIN)…"
RUSTFLAGS='-C target-feature=+simd128' \
  cargo "+$TOOLCHAIN" build -p a2ui-graph \
              --target wasm32-unknown-unknown --release

# ── 3. Guard: is the client actually IN the module? ────────────────────────
# Only exported items survive the link into a cdylib. This has already
# happened once here: a module that built, linked, and contained none of
# the renderer. wabt is optional, so this degrades to a warning rather than
# blocking a build on a machine that lacks it.
if command -v wasm-objdump >/dev/null 2>&1; then
  syms=$(wasm-objdump -x "$WASM" | grep -ci 'FieldHandle' || true)
  if [ "$syms" -eq 0 ]; then
    echo "the module contains no FieldHandle symbol — the client was" >&2
    echo "linked out. Check that the #[wasm_bindgen] exports are present." >&2
    exit 1
  fi
  echo "  client present ($syms FieldHandle symbols)"
else
  echo "  (wasm-objdump absent — skipping the linked-out check;" >&2
  echo "   install wabt to enable it)" >&2
fi

# ── 4. Glue ────────────────────────────────────────────────────────────────
echo "generating JS glue -> $OUT"
wasm-bindgen --target web --out-dir "$OUT" "$WASM"

echo
echo "done:"
ls -la "$OUT"
echo
echo "  import init, { FieldHandle } from './$(basename "$OUT")/a2ui_graph.js';"
echo
echo "Note: no wasm-opt pass is run here. Adding one is a size/time"
echo "trade-off for the consumer to make, not a correctness step."
