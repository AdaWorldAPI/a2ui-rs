//! `a2ui-wasm` — the **fieldview client** (W3 of
//! `AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`).
//!
//! *The browser IS the thin client.* The charter's C1.3 claim is concrete
//! here: the SAME Rust the server projects with — a ClassView/template codebook
//! plus `ogar-render-askama`'s fieldview — compiles to wasm, so the client
//! holds the codebook (the *font of the desktop*, amortized once) and renders
//! locally from borrowed memory; the server never rasterizes.
//!
//! # What the client does
//!
//! - **Ingest** a [`NodeDelta`](a2ui_core::NodeDelta) — LE bytes off the wire,
//!   decoded with the W1 load gate (no serde, charter T3). The client repaints
//!   only the delta's masked positions; it accumulates them into per-node state
//!   so a full render shows every position received so far.
//! - **Resolve** `key → ClassView → template` from the codebook with zero value
//!   decode ([`concept_of_key`] reads the class from the GUID's own bytes —
//!   OGAR P0 "the key prerenders nodes").
//! - **Render** the addressed fieldview surface locally
//!   ([`ogar_render_askama::render_field_view`]) — each field at its
//!   `data-field-pos`, each action at its `data-action-ordinal`.
//! - **Act** — a click builds an [`ActionInvoke`](a2ui_core::ActionInvoke)
//!   up-frame carrying only the ordinal ADDRESS (trap T2), never a handler.
//!
//! # RBAC is not the client's job
//!
//! The server already projected `surface ∩ role` BEFORE framing (charter
//! C1.4); the client receives only authorized positions and simply paints what
//! it is given. A field the role may not see never reached the client — it is
//! absent from the wire, not hidden here.
//!
//! # wasm surface
//!
//! The native `rlib` carries all the logic (and is unit-tested natively); the
//! `wasm` feature adds a `wasm-bindgen` wrapper ([`wasm::FieldviewClientWasm`])
//! for browser consumers. `cargo check --target wasm32-unknown-unknown` proves
//! the core compiles to wasm — the "same Rust" claim, mechanically.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use a2ui_core::{ActionInvoke, Frame, mask_positions};
use lance_graph_contract::class_view::{ClassId, WideFieldMask};
use ogar_render_askama::{ActionRef, FieldView, render_field_view};

// Re-export the addressed-surface types so a consumer (e.g. the `a2ui-paint`
// wgpu tier) can name the resolved surface without depending on this crate's
// internal import path. These are OGAR-owned, consumer-agnostic types.
pub use ogar_render_askama::{ActionRef as SurfaceAction, FieldView as SurfaceField};

/// The number of content-blind facet bytes a node carries (V3 le-contract §3).
const FACET_LEN: usize = 12;

/// The concept a canonical GUID `key` resolves to, with **zero value decode**
/// (OGAR P0 "the key prerenders nodes"): the classid is the u32 at bytes 0..4
/// (LE); under the canon-high flip its HIGH u16 is the shared concept.
///
/// This is the client-side twin of `a2ui_server::concept_of_key` — the two
/// tiers read the same canon key layout independently (the client never
/// depends on the server tier).
#[must_use]
pub fn concept_of_key(key: &[u8; 16]) -> ClassId {
    let classid = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
    (classid >> 16) as ClassId
}

/// One field in a class's template codebook — the late-resolved `(label,
/// predicate)` the client uses to render position `i`. Position = the field's
/// index in [`ClientClass::fields`] (the ObjectView order the server projects).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientField {
    /// Display label (the font-of-the-desktop text for this position).
    pub label: String,
    /// The field's predicate IRI (stable key behind the label).
    pub predicate: String,
}

impl ClientField {
    /// A field with `label` and `predicate`.
    pub fn new(label: impl Into<String>, predicate: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            predicate: predicate.into(),
        }
    }
}

/// One class in the client's template codebook — resolved once (amortized like
/// a font) and reused for every instance of the class. This is the client's
/// half of "the ClassView registry is the font of the desktop."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientClass {
    /// The canonical concept name (the surface's `data-concept`).
    pub concept: String,
    /// The class's window title / headline for the surface.
    pub title: String,
    /// Position-ordered fields (index `i` = mask position `i`).
    pub fields: Vec<ClientField>,
    /// Ordinal-ordered action captions (index = the `ActionInvoke` ordinal).
    pub actions: Vec<String>,
}

/// The accumulated client-side state of one addressed node: its facet bytes and
/// which positions have been received (so a full render shows everything seen
/// so far, while a delta only repaints its own positions).
#[derive(Debug, Clone, Default)]
struct NodeState {
    facet: [u8; FACET_LEN],
    present: WideFieldMask,
    /// The resolved addressed surface for this node — computed once per
    /// `apply_node_delta` and stored so BOTH renderers read one surface: the
    /// askama HTML render and the `a2ui-paint` wgpu tier consume the SAME
    /// `&[FieldView]` / `&[ActionRef]` slice (no divergent second resolution).
    resolved_fields: Vec<FieldView>,
    resolved_actions: Vec<ActionRef>,
}

/// Why the client could not apply a frame or build an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// The incoming bytes were not a valid frame (W1 load gate refused them).
    Frame(a2ui_core::FrameError),
    /// A down-wire frame was expected to be a `NodeDelta` but was not.
    NotANodeDelta,
    /// The frame's class is not in the client's codebook (an unresolved
    /// template — the client must sync the class first). Fail-loud, never a
    /// blank render.
    UnknownClass(ClassId),
    /// A `NodeDelta` carried fewer value bytes than its mask has facet-backed
    /// positions — a malformed frame. Refused (never a partial apply).
    ValueUnderrun {
        /// Facet-backed positions the mask declared.
        need: usize,
        /// Value bytes actually present.
        have: usize,
    },
}

impl core::fmt::Display for ClientError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Frame(e) => write!(f, "frame decode failed: {e}"),
            Self::NotANodeDelta => f.write_str("expected a NodeDelta down the wire"),
            Self::UnknownClass(c) => write!(f, "class 0x{c:04X} not in the client codebook"),
            Self::ValueUnderrun { need, have } => {
                write!(
                    f,
                    "NodeDelta value underrun: need {need} bytes, have {have}"
                )
            }
        }
    }
}

impl std::error::Error for ClientError {}

/// A nested addressed surface — a node's own resolved surface PLUS its child
/// surfaces composed by address. This is the L1-screen / L2++-drill-down tree
/// (charter nesting): a "screen" is just a class whose slot positions link to
/// child node keys; composition is pure codebook-layer resolution, the wire
/// stays a flat `NodeDelta` stream (T3), and no new widget vocabulary is
/// introduced (T1 — a screen is a class, a slot is a linked position).
#[derive(Debug, Clone, PartialEq)]
pub struct NestedSurface {
    /// This node's canonical GUID address.
    pub key: [u8; 16],
    /// The concept the class resolves to (the surface `data-concept`).
    pub concept: String,
    /// The window title / headline.
    pub title: String,
    /// This node's own resolved fields (the leaf surface at this level).
    pub fields: Vec<FieldView>,
    /// This node's ordinal-addressed actions.
    pub actions: Vec<ActionRef>,
    /// Child surfaces, each at the slot `position` that links to it — the
    /// drill-down. Empty for a leaf (an interactive preset).
    pub children: Vec<(u8, NestedSurface)>,
}

impl NestedSurface {
    /// The depth of this subtree (a leaf preset is depth 1).
    #[must_use]
    pub fn depth(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(|(_, c)| c.depth())
            .max()
            .unwrap_or(0)
    }
}

/// The fieldview client — holds the template codebook + per-node state, ingests
/// down-wire frames, and renders the addressed surface locally.
#[derive(Default)]
pub struct FieldviewClient {
    /// The template codebook — `class_id → ClientClass` (the font of the desktop).
    codebook: HashMap<ClassId, ClientClass>,
    /// Per-node accumulated state, keyed by the canonical GUID.
    nodes: HashMap<[u8; 16], NodeState>,
    /// Slot links: `(parent_key, slot_position) → child_key`. A screen's slot
    /// position resolves to a child node; this is the ONLY nesting state, and it
    /// is pure address→address (no values, no wire change).
    child_links: HashMap<([u8; 16], u8), [u8; 16]>,
}

impl FieldviewClient {
    /// An empty client — register classes, then apply frames.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a class in the codebook. In a live session this is
    /// the `SyncCodebook` / `PushCodebookDelta` amortization; here it is a
    /// direct insert.
    pub fn register_class(&mut self, class_id: ClassId, class: ClientClass) {
        self.codebook.insert(class_id, class);
    }

    /// Is `class_id` resolvable (its template in the codebook)?
    #[must_use]
    pub fn knows_class(&self, class_id: ClassId) -> bool {
        self.codebook.contains_key(&class_id)
    }

    /// The current accumulated facet for a node, if any has been received.
    #[must_use]
    pub fn facet(&self, key: &[u8; 16]) -> Option<[u8; FACET_LEN]> {
        self.nodes.get(key).map(|s| s.facet)
    }

    /// Ingest a down-wire frame (LE bytes) and render the node's addressed
    /// surface. The frame MUST be a [`NodeDelta`](a2ui_core::NodeDelta); its
    /// masked facet-backed positions are applied to the node's accumulated
    /// state, then the full current surface is rendered from the codebook.
    ///
    /// # Errors
    ///
    /// [`ClientError::Frame`] if the bytes aren't a valid frame;
    /// [`ClientError::NotANodeDelta`] if they decode to an `ActionInvoke`;
    /// [`ClientError::UnknownClass`] if the class isn't in the codebook;
    /// [`ClientError::ValueUnderrun`] if the mask promises more facet bytes than
    /// the frame carries.
    pub fn apply_node_delta(&mut self, bytes: &[u8]) -> Result<String, ClientError> {
        let frame = Frame::from_le_bytes(bytes).map_err(ClientError::Frame)?;
        let delta = match frame {
            Frame::NodeDelta(d) => d,
            Frame::ActionInvoke(_) => return Err(ClientError::NotANodeDelta),
        };
        let class_id = concept_of_key(&delta.key);
        if !self.codebook.contains_key(&class_id) {
            return Err(ClientError::UnknownClass(class_id));
        }

        // Apply the masked, facet-backed values to the node's accumulated state.
        // Positions come out ascending (mask_positions), matching the wire's
        // ascending value order.
        let state = self.nodes.entry(delta.key).or_default();
        let mut value_iter = delta.values.iter();
        let mut consumed = 0usize;
        let mut needed = 0usize;
        for pos in mask_positions(&delta.mask_words) {
            if (pos as usize) < FACET_LEN {
                needed += 1;
                match value_iter.next() {
                    Some(&b) => {
                        let p = pos as u8;
                        state.facet[pos as usize] = b;
                        // `WideFieldMask::with` consumes self (not Copy) — take
                        // it out of the &mut field, promote, put it back.
                        state.present = std::mem::take(&mut state.present).with(p);
                        consumed += 1;
                    }
                    None => {
                        return Err(ClientError::ValueUnderrun {
                            need: needed,
                            have: consumed,
                        });
                    }
                }
            }
        }

        // Resolve the FULL current surface (every position received so far)
        // ONCE, from the codebook, and STORE it on the node. Both renderers
        // read this one surface: the askama HTML render just below, and the
        // `a2ui-paint` wgpu tier via [`resolved_fields`]/[`resolved_actions`].
        let class = &self.codebook[&class_id];
        let field_views: Vec<FieldView> = class
            .fields
            .iter()
            .enumerate()
            .filter_map(|(i, cf)| {
                let pos = i as u8;
                (i < FACET_LEN && state.present.has(pos)).then(|| FieldView {
                    position: pos,
                    label: cf.label.clone(),
                    predicate: cf.predicate.clone(),
                    value: state.facet[i].to_string(),
                })
            })
            .collect();
        let action_refs: Vec<ActionRef> = class
            .actions
            .iter()
            .enumerate()
            .map(|(i, label)| ActionRef {
                ordinal: i as u32,
                label: label.clone(),
            })
            .collect();
        state.resolved_fields = field_views;
        state.resolved_actions = action_refs;

        // Render the askama HTML surface FROM the stored resolved surface —
        // proving HTML and the paint tier consume the identical slice.
        let key_hex = hex16(&delta.key);
        render_field_view(
            class_id,
            &class.concept,
            &key_hex,
            &class.title,
            &state.resolved_fields,
            &state.resolved_actions,
        )
        .map_err(|_| ClientError::NotANodeDelta) // askama render is infallible for well-formed input
    }

    /// The resolved addressed field surface for a node — the SAME `&[FieldView]`
    /// the askama render consumed, exposed for a second renderer (the
    /// `a2ui-paint` wgpu tier) to paint WITHOUT re-parsing HTML. `None` if no
    /// delta has been applied to `key` yet.
    ///
    /// The fields are OGAR-owned, consumer-agnostic types; this accessor is a
    /// plain borrowed slice — no serialization (charter T3). If `FieldView`
    /// ever gains serde, it MUST be feature-gated exactly like `Frame`.
    #[must_use]
    pub fn resolved_fields(&self, key: &[u8; 16]) -> Option<&[FieldView]> {
        self.nodes.get(key).map(|s| s.resolved_fields.as_slice())
    }

    /// The resolved ordinal-addressed action set for a node — the SAME
    /// `&[ActionRef]` the askama render consumed, for the paint tier. `None` if
    /// no delta has been applied to `key` yet. See [`resolved_fields`].
    ///
    /// [`resolved_fields`]: FieldviewClient::resolved_fields
    #[must_use]
    pub fn resolved_actions(&self, key: &[u8; 16]) -> Option<&[ActionRef]> {
        self.nodes.get(key).map(|s| s.resolved_actions.as_slice())
    }

    /// Link a child node into a parent's slot `position` — the drill-down edge
    /// (charter nesting). A "screen" is a class whose slot positions link to
    /// child node keys; [`resolve_nested`](FieldviewClient::resolve_nested)
    /// composes the tree. Pure address→address: no values, no wire change.
    pub fn link_child(&mut self, parent: [u8; 16], slot_position: u8, child: [u8; 16]) {
        self.child_links.insert((parent, slot_position), child);
    }

    /// The child node linked at a parent's slot `position`, if any.
    #[must_use]
    pub fn child_at(&self, parent: &[u8; 16], slot_position: u8) -> Option<[u8; 16]> {
        self.child_links.get(&(*parent, slot_position)).copied()
    }

    /// Compose the nested surface rooted at `key` — the L1-screen → L2++-drill-
    /// down tree. Each node contributes its own resolved surface (leaf preset);
    /// each slot position that has a linked child recurses. Returns `None` if
    /// `key`'s class is unknown. Cycle-safe via `max_depth` (a linked cycle
    /// stops descending rather than looping).
    ///
    /// This is pure codebook-layer composition — the wire only ever carried flat
    /// `NodeDelta`s; nesting is a client-side walk over addresses (T1/T3 hold).
    #[must_use]
    pub fn resolve_nested(&self, key: &[u8; 16], max_depth: usize) -> Option<NestedSurface> {
        let class_id = concept_of_key(key);
        let class = self.codebook.get(&class_id)?;
        let (fields, actions) = match self.nodes.get(key) {
            Some(s) => (s.resolved_fields.clone(), s.resolved_actions.clone()),
            None => (Vec::new(), Vec::new()),
        };
        let mut children = Vec::new();
        if max_depth > 1 {
            // Descend each slot position that links to a child, in field order.
            for f in &fields {
                if let Some(child_key) = self.child_at(key, f.position)
                    && let Some(sub) = self.resolve_nested(&child_key, max_depth - 1)
                {
                    children.push((f.position, sub));
                }
            }
        }
        Some(NestedSurface {
            key: *key,
            concept: class.concept.clone(),
            title: class.title.clone(),
            fields,
            actions,
            children,
        })
    }

    /// Build an up-wire [`ActionInvoke`](a2ui_core::ActionInvoke) frame (LE
    /// bytes) for a click — behavior invoked by ordinal ADDRESS only (charter
    /// T2). The client never sends a handler; it sends the address of one.
    #[must_use]
    pub fn invoke_action(&self, key: [u8; 16], ordinal: u32, args: Vec<u8>) -> Vec<u8> {
        Frame::ActionInvoke(ActionInvoke {
            key,
            action_ordinal: ordinal,
            args,
        })
        .to_le_bytes()
    }
}

/// Lowercase hex of a 16-byte key — the node's `data-key` address.
fn hex16(key: &[u8; 16]) -> String {
    use core::fmt::Write;
    let mut s = String::with_capacity(32);
    for b in key {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// wasm-bindgen surface for browser consumers — a thin wrapper over
/// [`FieldviewClient`]. The browser registers classes (the codebook sync),
/// feeds NodeDelta bytes in, gets rendered HTML out, and asks for ActionInvoke
/// bytes to send up. All the logic is the native [`FieldviewClient`]; this is
/// only the JS boundary.
#[cfg(feature = "wasm")]
pub mod wasm {
    use super::{ClientClass, ClientField, FieldviewClient};
    use wasm_bindgen::prelude::*;

    /// Browser-facing handle to a [`FieldviewClient`].
    #[wasm_bindgen]
    pub struct FieldviewClientWasm {
        inner: FieldviewClient,
    }

    #[wasm_bindgen]
    impl FieldviewClientWasm {
        /// A new, empty client.
        #[wasm_bindgen(constructor)]
        #[must_use]
        pub fn new() -> Self {
            Self {
                inner: FieldviewClient::new(),
            }
        }

        /// Register a class in the codebook. `labels`/`predicates` are aligned
        /// position-ordered field data; `actions` are ordinal-ordered captions.
        pub fn register_class(
            &mut self,
            class_id: u16,
            concept: String,
            title: String,
            labels: Vec<String>,
            predicates: Vec<String>,
            actions: Vec<String>,
        ) {
            let fields = labels
                .into_iter()
                .zip(predicates)
                .map(|(l, p)| ClientField::new(l, p))
                .collect();
            self.inner.register_class(
                class_id,
                ClientClass {
                    concept,
                    title,
                    fields,
                    actions,
                },
            );
        }

        /// Ingest a NodeDelta (LE bytes) and return the rendered surface HTML.
        ///
        /// # Errors
        ///
        /// Returns the client error's message as a `JsError` if the frame is
        /// invalid, the class is unknown, or the values underrun.
        pub fn apply_node_delta(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.inner
                .apply_node_delta(bytes)
                .map_err(|e| JsError::new(&e.to_string()))
        }

        /// Build an ActionInvoke up-frame (LE bytes) for a click.
        #[must_use]
        pub fn invoke_action(&self, key: &[u8], ordinal: u32, args: Vec<u8>) -> Vec<u8> {
            let mut k = [0u8; 16];
            let n = key.len().min(16);
            k[..n].copy_from_slice(&key[..n]);
            self.inner.invoke_action(k, ordinal, args)
        }
    }

    impl Default for FieldviewClientWasm {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_core::NodeDelta;

    fn key_for(concept: u16) -> [u8; 16] {
        let classid: u32 = ((concept as u32) << 16) | 0x0009;
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&classid.to_le_bytes());
        k[10..16].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        k
    }

    fn invoice_class() -> ClientClass {
        ClientClass {
            concept: "commercial_document".to_string(),
            title: "Invoice".to_string(),
            fields: vec![
                ClientField::new("Total", "amount_total"),
                ClientField::new("Status", "state"),
                ClientField::new("Partner", "partner_id"),
            ],
            actions: vec!["View".to_string(), "Post".to_string()],
        }
    }

    /// A NodeDelta carrying positions {0, 2} with bytes {42, 7} — the exact
    /// shape the server's `project_node` emits under a role that dropped
    /// position 1.
    fn server_delta(key: [u8; 16]) -> Vec<u8> {
        Frame::NodeDelta(NodeDelta {
            key,
            mask_words: vec![0b101], // positions 0 and 2
            values: vec![42, 7],
        })
        .to_le_bytes()
    }

    #[test]
    fn concept_reads_from_key() {
        assert_eq!(concept_of_key(&key_for(0x0102)), 0x0102);
    }

    #[test]
    fn applies_delta_and_renders_addressed_surface() {
        let mut client = FieldviewClient::new();
        client.register_class(0x0102, invoice_class());
        let key = key_for(0x0102);
        let html = client.apply_node_delta(&server_delta(key)).unwrap();

        // The addressed surface: positions 0 and 2 present, 1 absent (the
        // server projected it out; the client never received it).
        assert!(html.contains("data-class-id=\"0x0102\""), "{html}");
        assert!(html.contains("data-field-pos=\"0\""), "{html}");
        assert!(html.contains("data-field-pos=\"2\""), "{html}");
        assert!(
            !html.contains("data-field-pos=\"1\""),
            "pos 1 absent:\n{html}"
        );
        assert!(html.contains("Total") && html.contains("Partner"));
        assert!(!html.contains(">Status<"), "Status never arrived:\n{html}");
        // The values from the wire.
        assert!(html.contains(">42<") && html.contains(">7<"), "{html}");
        // Actions by ordinal address.
        assert!(html.contains("data-action-ordinal=\"1\""), "{html}");
        // The client cached the facet.
        let facet = client.facet(&key).unwrap();
        assert_eq!(facet[0], 42);
        assert_eq!(facet[2], 7);
    }

    #[test]
    fn resolved_surface_accessor_is_data_identical_to_the_render() {
        // G6: the resolved-surface accessor exposes the SAME `&[FieldView]` /
        // `&[ActionRef]` the askama render consumed — one resolution, two
        // renderers (HTML here; the a2ui-paint wgpu tier via the accessor).
        let mut client = FieldviewClient::new();
        client.register_class(0x0102, invoice_class());
        let key = key_for(0x0102);
        let html = client.apply_node_delta(&server_delta(key)).unwrap();

        let fields = client.resolved_fields(&key).expect("resolved after apply");
        let actions = client.resolved_actions(&key).expect("resolved after apply");

        // Positions {0, 2} present (pos 1 was projected out server-side), 42/7.
        let positions: Vec<u8> = fields.iter().map(|f| f.position).collect();
        assert_eq!(positions, vec![0, 2]);
        let values: Vec<&str> = fields.iter().map(|f| f.value.as_str()).collect();
        assert_eq!(values, vec!["42", "7"]);

        // Data-identity with the HTML: every field/action the paint tier would
        // read is exactly what the HTML addressed.
        for f in fields {
            assert!(html.contains(&format!("data-field-pos=\"{}\"", f.position)));
            assert!(html.contains(&format!(">{}<", f.value)));
        }
        assert_eq!(actions.len(), 2, "View + Post ordinal-addressed");
        assert!(html.contains("data-action-ordinal=\"1\""));

        // Fail-quiet: an unseen node has no resolved surface (no panic).
        assert!(client.resolved_fields(&key_for(0x0999)).is_none());
        assert!(client.resolved_actions(&key_for(0x0999)).is_none());
    }

    #[test]
    fn deltas_accumulate_across_frames() {
        let mut client = FieldviewClient::new();
        client.register_class(0x0102, invoice_class());
        let key = key_for(0x0102);
        // First delta: position 0 = 42.
        client
            .apply_node_delta(
                &Frame::NodeDelta(NodeDelta {
                    key,
                    mask_words: vec![0b1],
                    values: vec![42],
                })
                .to_le_bytes(),
            )
            .unwrap();
        // Second delta: position 2 = 7. The surface must now show BOTH.
        let html = client
            .apply_node_delta(
                &Frame::NodeDelta(NodeDelta {
                    key,
                    mask_words: vec![0b100],
                    values: vec![7],
                })
                .to_le_bytes(),
            )
            .unwrap();
        assert!(
            html.contains("data-field-pos=\"0\""),
            "pos 0 retained:\n{html}"
        );
        assert!(
            html.contains("data-field-pos=\"2\""),
            "pos 2 added:\n{html}"
        );
        assert!(html.contains(">42<") && html.contains(">7<"), "{html}");
    }

    #[test]
    fn unknown_class_fails_loud() {
        let mut client = FieldviewClient::new(); // empty codebook
        let key = key_for(0x0102);
        assert_eq!(
            client.apply_node_delta(&server_delta(key)),
            Err(ClientError::UnknownClass(0x0102))
        );
    }

    #[test]
    fn action_invoke_up_carries_only_the_ordinal_address() {
        let client = FieldviewClient::new();
        let key = key_for(0x0102);
        let bytes = client.invoke_action(key, 1, vec![]);
        // Round-trips back to an ActionInvoke with ordinal 1 (T2 — an address).
        match Frame::from_le_bytes(&bytes).unwrap() {
            Frame::ActionInvoke(ai) => {
                assert_eq!(ai.key, key);
                assert_eq!(ai.action_ordinal, 1);
            }
            Frame::NodeDelta(_) => panic!("expected ActionInvoke"),
        }
    }

    /// G5 — the reusability falsifier. A SECOND, non-MedCare synthetic class
    /// (a sensor reading, nothing like the invoice above) renders AND paints
    /// through the IDENTICAL `FieldviewClient` + `a2ui-paint` with ZERO
    /// client-side code change: `register_class` → `apply_node_delta` (HTML) →
    /// `resolved_fields`/`resolved_actions` (the accessor) → `a2ui_paint::layout`
    /// (pixels) → hit-test → up-frame. Only public API is touched.
    #[test]
    fn second_synthetic_consumer_renders_and_paints_unchanged() {
        // A wholly different class — different concept id, fields, actions.
        let sensor = ClientClass {
            concept: "sensor_reading".to_string(),
            title: "Sensor".to_string(),
            fields: vec![
                ClientField::new("Temp", "temperature_c"),
                ClientField::new("Humidity", "humidity_pct"),
            ],
            actions: vec!["Calibrate".to_string()],
        };
        let mut client = FieldviewClient::new();
        client.register_class(0x0205, sensor);
        let key = key_for(0x0205);

        // RENDER (client): positions {0,1} = 21, 55.
        let html = client
            .apply_node_delta(
                &Frame::NodeDelta(NodeDelta {
                    key,
                    mask_words: vec![0b11],
                    values: vec![21, 55],
                })
                .to_le_bytes(),
            )
            .unwrap();
        assert!(html.contains("data-concept=\"sensor_reading\""), "{html}");

        // PAINT (via the accessor → a2ui-paint, zero client change).
        let fields = client.resolved_fields(&key).unwrap();
        let actions = client.resolved_actions(&key).unwrap();
        let vp = a2ui_paint::Viewport::new(1024.0, 768.0);
        let lay = a2ui_paint::layout(fields, actions, &vp);
        assert_eq!(lay.fields.len(), 2);
        assert_eq!(lay.fields[0].position, 0);
        // A click on the "Calibrate" action (ordinal 0) → the addressed up-frame.
        let a0 = &lay.actions[0];
        let up = lay
            .click_to_action_frame(key, a0.rect.x + 1.0, a0.rect.y + 1.0)
            .expect("action hit → up-frame");
        match Frame::from_le_bytes(&up).unwrap() {
            Frame::ActionInvoke(ai) => assert_eq!(ai.action_ordinal, 0),
            Frame::NodeDelta(_) => panic!("expected ActionInvoke"),
        }
        // The falsifier: NO branch on the class anywhere above. A brand-new
        // synthetic ClassView flowed through the identical client + paint.
    }

    #[test]
    fn nested_surface_composes_screen_and_drilldown() {
        // Nesting (L1 screen / L2++ drill-down): a screen is a class whose slot
        // position links to a child node; the tree is composed by ADDRESS from
        // flat per-node surfaces — no wire change, no new vocabulary.
        let mut client = FieldviewClient::new();
        // L2 leaf preset — the invoice class (fields 0,2).
        client.register_class(0x0102, invoice_class());
        // L1 screen — field 0 is a SLOT that embeds the invoice child.
        client.register_class(
            0x0300,
            ClientClass {
                concept: "workspace".to_string(),
                title: "Desktop".to_string(),
                fields: vec![ClientField::new("Document", "embeds")],
                actions: vec!["Close".to_string()],
            },
        );

        let screen_key = key_for(0x0300);
        let invoice_key = key_for(0x0102);
        // Both nodes stream independently (flat wire).
        client
            .apply_node_delta(
                &Frame::NodeDelta(NodeDelta {
                    key: screen_key,
                    mask_words: vec![0b1],
                    values: vec![1],
                })
                .to_le_bytes(),
            )
            .unwrap();
        client.apply_node_delta(&server_delta(invoice_key)).unwrap();
        // Drill-down edge: the invoice sits in the screen's slot position 0.
        client.link_child(screen_key, 0, invoice_key);

        let tree = client
            .resolve_nested(&screen_key, 8)
            .expect("screen resolves");
        assert_eq!(tree.concept, "workspace");
        assert_eq!(tree.depth(), 2, "screen → invoice drill-down");
        assert_eq!(tree.children.len(), 1);
        let (slot_pos, child) = &tree.children[0];
        assert_eq!(*slot_pos, 0, "nested at the slot position ADDRESS");
        assert_eq!(child.concept, "commercial_document");
        assert_eq!(
            child.fields.iter().map(|f| f.position).collect::<Vec<_>>(),
            vec![0, 2],
            "child carries the invoice's own leaf surface"
        );
        assert_eq!(child.depth(), 1, "leaf preset");

        // max_depth caps descent: depth 1 shows the screen alone.
        let shallow = client.resolve_nested(&screen_key, 1).unwrap();
        assert!(shallow.children.is_empty());
    }

    #[test]
    fn value_underrun_is_refused() {
        let mut client = FieldviewClient::new();
        client.register_class(0x0102, invoice_class());
        let key = key_for(0x0102);
        // mask promises positions 0 and 2 (two facet bytes) but carries one.
        let bad = Frame::NodeDelta(NodeDelta {
            key,
            mask_words: vec![0b101],
            values: vec![42], // one byte, need two
        })
        .to_le_bytes();
        assert_eq!(
            client.apply_node_delta(&bad),
            Err(ClientError::ValueUnderrun { need: 2, have: 1 })
        );
    }

    #[test]
    fn action_invoke_frame_is_rejected_as_a_down_delta() {
        let mut client = FieldviewClient::new();
        client.register_class(0x0102, invoice_class());
        let up = client.invoke_action(key_for(0x0102), 0, vec![]);
        assert_eq!(
            client.apply_node_delta(&up),
            Err(ClientError::NotANodeDelta)
        );
    }
}
