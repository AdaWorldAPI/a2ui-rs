#!/usr/bin/env bash
#
# Build the optical-surface GPU renderer (a2ui-optics-gl) for the browser.
#
#   ./scripts/build-optics-wasm.sh [out-dir]   # default: crates/a2ui-optics-gl/pkg
#
# Same guards as build-graph-wasm.sh (CLI/crate version match, target
# installed for the pinned toolchain, exported symbols present), because
# each of those failures is silent on its own. No +simd128: the per-pixel
# work runs in the fragment shader, the wasm side only sets uniforms.

set -euo pipefail

cd "$(dirname "$0")/.."
OUT="${1:-crates/a2ui-optics-gl/pkg}"
WASM=target/wasm32-unknown-unknown/release/a2ui_optics_gl.wasm

pinned=$(awk -F'"' '/^channel *=/{print $2; exit}' rust-toolchain.toml)
[ -n "$pinned" ] || { echo "no channel in rust-toolchain.toml" >&2; exit 1; }
TOOLCHAIN="${RUST_TOOLCHAIN:-$pinned}"

want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/[",]/,""); print $3; exit}' Cargo.lock)
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen $want not on PATH. Prebuilt binaries:" >&2
  echo "  https://github.com/rustwasm/wasm-bindgen/releases/tag/$want" >&2
  exit 1
fi
have=$(wasm-bindgen --version | awk '{print $2}')
if [ "$have" != "$want" ]; then
  echo "wasm-bindgen CLI is $have but Cargo.lock pins $want." >&2
  exit 1
fi

if ! rustup target list --installed --toolchain "$TOOLCHAIN" 2>/dev/null \
     | grep -qx wasm32-unknown-unknown; then
  echo "wasm32-unknown-unknown is not installed for toolchain $TOOLCHAIN." >&2
  echo "  rustup target add wasm32-unknown-unknown --toolchain $TOOLCHAIN" >&2
  exit 1
fi

echo "building (rust $TOOLCHAIN)…"
cargo "+$TOOLCHAIN" build -p a2ui-optics-gl --target wasm32-unknown-unknown --release

if command -v wasm-objdump >/dev/null 2>&1; then
  syms=$(wasm-objdump -x "$WASM" | grep -ci 'surfacerenderer' || true)
  if [ "$syms" -eq 0 ]; then
    echo "the module contains no SurfaceRenderer symbol — the renderer was linked out." >&2
    exit 1
  fi
  echo "  renderer present ($syms SurfaceRenderer symbols)"
else
  echo "  (wasm-objdump absent — skipping the linked-out check)" >&2
fi

echo "generating JS glue -> $OUT"
wasm-bindgen --target web --out-dir "$OUT" "$WASM"
ls -la "$OUT"
