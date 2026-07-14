//! `a2ui-server` — the **graph desktop projection** service tier (W2/W5 of
//! `AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`).
//!
//! *Don't push pixels — address the screen.* This crate is the server half of
//! that thesis: it projects a graph node into an ADDRESSED surface and carries
//! it over a sealed session, reusing the whole Ada stack rather than inventing
//! a pixel codec.
//!
//! ```text
//!   ClassView + WideFieldMask                     ogar-encryption
//!   ───────────────────────                       ───────────────
//!   surface ∩ role  (project.rs, fail-closed)     Argon2id session KDF
//!            │                                     + XChaCha20 per-frame AEAD
//!            ▼                                     (session.rs + transport.rs)
//!   ┌──────────────────────────┐                          │
//!   │  render_stream.rs        │                          │
//!   │  node → NodeDelta (down) │──── frame LE bytes ──────┤ seal_frame
//!   │       + askama fieldview │                          ▼
//!   └──────────────────────────┘                    sealed session wire
//!   ┌──────────────────────────┐                          │
//!   │  action_stream.rs        │◀─── ActionInvoke ────────┤ open_frame
//!   │  ordinal → ActionDef     │      (up, by address)    │
//!   └──────────────────────────┘                          ▼
//! ```
//!
//! # The named pieces, and where each lives
//!
//! | goal piece | here |
//! |---|---|
//! | **ClassView + WideFieldMask** | [`project`] — `surface ∩ role`, fail-closed (charter C1.4) |
//! | **askama** | [`render_stream`] calls `ogar_render_askama::render_field_view` (the fieldview) |
//! | **graph desktop projection** | [`render_stream::project_node`] → [`NodeDelta`](a2ui_core::NodeDelta) down |
//! | **ruff codegen** | [`action_stream::ActionSpec`] = the runtime projection of the ruff-codegen'd `ActionDef` set; the P-REHOST test drives `render_class_with_methods_wide` (the same DO-arm → askama struct-of-methods) |
//! | **ogar-encryption** | [`session::Session`] (Argon2 KDF) + [`transport::SealedTransport`] (XChaCha20 AEAD) |
//!
//! # The charter traps, honored by construction
//!
//! - **T1 (no second vocabulary):** the surface is a [`FieldView`](ogar_render_askama::FieldView)
//!   list rendered by a ClassView *template* — never a new closed widget enum.
//! - **T2 (behavior never on the surface):** actions travel as ordinal
//!   ADDRESSES ([`ActionInvoke`](a2ui_core::ActionInvoke)); [`action_stream`]
//!   resolves the ordinal to an `ActionDef` on the Core node. There is no
//!   handler on the wire to run.
//! - **T3 (no serialization in the hot path):** the frame's `to_le_bytes` IS
//!   the plaintext; [`transport`] AEADs those bytes — nothing serializes a
//!   struct to transport it.

#![forbid(unsafe_code)]

pub mod action_stream;
pub mod desktop;
pub mod project;
pub mod render_stream;
pub mod session;
pub mod transport;

pub use action_stream::{ActionError, ActionSpec, ResolvedAction, concept_of_key, resolve_action};
pub use desktop::{
    ClassCodebook, ClassEntry, CodebookSnapshot, DesktopError, DesktopSession, KlickwegEdge,
    RenderedFrame,
};
pub use project::{RbacError, mask_to_words, project_surface};
pub use render_stream::{Projection, ProjectionError, project_node};
pub use session::{Session, SessionError};
pub use transport::{Direction, SealedTransport, TransportError};

// Re-export the frame + mask surfaces so a consumer imports one crate.
pub use a2ui_core::{ActionInvoke, Frame, FrameKind, NodeDelta};
pub use lance_graph_contract::class_view::{ClassId, ClassView, WideFieldMask};
pub use ogar_encryption::KdfParams;
