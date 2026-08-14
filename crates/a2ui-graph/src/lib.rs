//! **a2ui-graph** — the GPU renderer of the node/edge FIELD.
//!
//! # Why this crate exists next to the FieldView paint tier
//!
//! Operator ruling, 2026-08-14, after seeing both surfaces run: the FieldView
//! renderer *"ist für nodes einfach das falsche renderer, das geht gar nicht
//! und wird niemals gehen — nur für den Node preview vielleicht"*, and the
//! reason names the mechanism: *"die dynamischen pfeile der zoomin die
//! relationen das anfassen und wobble, das wird niemals mit SVG gehen."*
//!
//! That is a division of labor, not a rejection:
//!
//! | Surface | Renderer | Why |
//! |---|---|---|
//! | Node **preview** (addressed fields, ClassView × mask) | a2ui-paint FieldView | fields are a form; a form is retained-mode work |
//! | Node **field** (the graph itself) | **this crate**, wgpu | 10^4–10^6 primitives, per-frame motion, grab + wobble |
//!
//! A retained scene graph — SVG or DOM — allocates a node per primitive and
//! walks them every frame. That is survivable for a form of 30 fields and
//! structurally hopeless for a field of 47 000 nodes with live edges. This
//! crate never allocates per primitive: the ABI lanes become vertex, instance
//! and index buffers, and interaction is uniforms and attribute writes.
//!
//! **SVG is allowed ABOVE the canvas** (operator: *"SVG als transparente
//! Fenster ist alles Okay"*) — labels, HUD, tooltips are the consumer's
//! business and live in their own transparent layer. What must never happen
//! is SVG *in* the field.
//!
//! # The path, end to end
//!
//! ```text
//! ABI v3 bytes ──(borrow, no copy)──► GraphAbi ──► Layout (SoA f32)
//!                                        │              │
//!                                        └──► Scene ◄───┘
//!                                              │
//!                                     instance + index buffers
//!                                              │
//!                                        wgpu draw (WebGPU / WebGL2)
//! ```
//!
//! Every stage below the draw is pure and testable with no GPU, which is why
//! the `wgpu` feature is off by default: the arithmetic is what can be wrong;
//! the device only executes it.

#![forbid(unsafe_code)]

pub mod abi;
pub mod layout;
pub mod scene;

#[cfg(feature = "wgpu")]
pub mod gpu;

pub use abi::{AbiError, GraphAbi};
pub use layout::Layout;
pub use scene::{EdgeIndex, Facet, NodeInstance, Scene};

#[cfg(feature = "wgpu")]
pub use gpu::{Camera, FieldRenderer};
