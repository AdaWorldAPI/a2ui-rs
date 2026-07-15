//! The **down-wire** (server → client): project one addressed node into a
//! [`NodeDelta`] frame AND an askama fieldview surface.
//!
//! This is where the four named pieces meet:
//!
//! 1. **ClassView + WideFieldMask** — the class surface
//!    (`WideFieldMask::full_for(field_count)`) is RBAC-projected against the
//!    session role ([`project_surface`](crate::project::project_surface),
//!    fail-closed) BEFORE anything is framed.
//! 2. **graph desktop projection** — the projected, facet-backed positions
//!    become a [`NodeDelta`] `{ key, mask_words, values }`: the wire carries
//!    the changed positions + their LE facet bytes, nothing else. An
//!    unauthorized field is absent from `values`, not hidden on the client.
//! 3. **askama** — the same projection renders through
//!    [`ogar_render_askama::render_field_view`] into the addressed HTML
//!    surface (each field at its `data-field-pos`), the membrane/debug view of
//!    the exact same bytes.
//!
//! The `NodeDelta` is the zero-copy wire truth (charter T3); the HTML is the
//! optional membrane render for a non-wasm client. Both are the SAME
//! projection — that is the charter's "one template projection; serve it as a
//! living screen or re-issue it as a document."
//!
//! # Facet-backed POC scope
//!
//! Values are drawn from the node's V3 content-blind 12-byte facet, so a
//! field at position `i < 12` carries facet byte `i` (le-contract §3 — the
//! byte register the ClassView projects). Positions `>= 12` are value-slab
//! fields read through a different surface; they are skipped here (never
//! folded onto a facet byte), mirroring
//! [`ClassView::facet_rows`](lance_graph_contract::class_view::ClassView::facet_rows)'
//! own 12-byte rule.
//!
//! **Gate/emit asymmetry (fail-safe, documented):** the RBAC gate
//! ([`project_surface`](crate::project::project_surface)) validates over the
//! FULL surface `full_for(field_count)` — including positions `>= 12` — while
//! this emit loop only carries positions `< 12` (facet scope). So a role that
//! is granted ONLY fields `>= 12` clears the gate (its projection is non-empty)
//! yet produces an EMPTY `NodeDelta` — a blank surface, not a
//! [`RbacError::EmptyProjection`] refusal. This is fail-SAFE (nothing
//! unauthorized leaks — the delta is empty, not over-broad) and deliberately
//! not "fixed" to refuse: the `>= 12` value-slab fields are real, and a future
//! value-slab render path WILL emit them, at which point that role renders a
//! real surface. Refusing here would wrongly reject a legitimate value-slab
//! grant. Until that path exists, such a role simply sees an empty facet
//! surface.

use a2ui_core::NodeDelta;
use lance_graph_contract::class_view::{ClassId, ClassView, WideFieldMask};
use ogar_render_askama::{ActionRef, FieldView, render_field_view};

use crate::action_stream::{ActionSpec, concept_of_key};
use crate::hex16;
use crate::project::{RbacError, mask_to_words};
use crate::session::Session;

/// The number of content-blind facet bytes a node carries (V3 le-contract §3).
const FACET_LEN: usize = 12;

/// A projected node — the wire frame plus its rendered surface, both from the
/// same RBAC-projected field set.
#[derive(Debug, Clone)]
pub struct Projection {
    /// The down-wire frame: `{ key, mask_words, values }`. `mask_words` is the
    /// carried (projected ∩ facet-backed) mask; `values` are the facet bytes
    /// at those positions, ascending.
    pub delta: NodeDelta,
    /// The addressed askama fieldview surface (the membrane/debug render of
    /// the same projection). Every field carries its `data-field-pos`.
    pub html: String,
}

/// Why a node could not be projected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    /// The key's concept is outside the session's granted classid range.
    Unauthorized(ClassId),
    /// The RBAC field projection refused (empty role, or role ∩ surface empty).
    Rbac(RbacError),
    /// The askama fieldview render failed (never for well-formed input).
    Render(String),
}

impl core::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unauthorized(c) => write!(f, "session does not authorize class 0x{c:04X}"),
            Self::Rbac(e) => write!(f, "rbac projection refused: {e}"),
            Self::Render(e) => write!(f, "fieldview render failed: {e}"),
        }
    }
}

impl std::error::Error for ProjectionError {}

impl From<RbacError> for ProjectionError {
    fn from(e: RbacError) -> Self {
        Self::Rbac(e)
    }
}

/// Project one node down the wire: RBAC-project the class surface, then emit
/// the [`NodeDelta`] frame + the askama fieldview surface.
///
/// - `view` is the [`ClassView`] (labels resolve late from it, above the SoA).
/// - `concept` is the class's canonical concept name (the `data-concept` on
///   the surface; the client resolves its template codebook by class).
/// - `key` is the node's canonical 16-byte GUID — the class resolves from it
///   with zero value decode (`concept_of_key`).
/// - `facet` is the node's 12-byte content-blind value register.
/// - `actions` are the class's `ActionDef` set (surfaced by ordinal address).
/// - `fmt` formats a facet byte at a given `(position, byte)` for display
///   (e.g. an enum label); pass `|_, b| b.to_string()` for the raw byte.
///
/// # Errors
///
/// [`ProjectionError::Unauthorized`] if the session's class gate rejects the
/// key's concept; [`ProjectionError::Rbac`] if the field projection is refused
/// (fail-closed); [`ProjectionError::Render`] if the template render fails.
// The projection genuinely needs all eight inputs (session, view, concept,
// key, facet, actions, title, value formatter) — the same shape as OGAR's own
// `render_detail` kit, which carries the identical allow.
#[allow(clippy::too_many_arguments)]
pub fn project_node<V: ClassView>(
    session: &Session,
    view: &V,
    concept: &str,
    key: [u8; 16],
    facet: &[u8; FACET_LEN],
    actions: &[ActionSpec],
    title: &str,
    mut fmt: impl FnMut(u8, u8) -> String,
) -> Result<Projection, ProjectionError> {
    let class_id = concept_of_key(&key);
    if !session.authorizes_class(class_id) {
        return Err(ProjectionError::Unauthorized(class_id));
    }

    // 1. The class surface, RBAC-projected against the session role — BEFORE
    //    framing. Fail-closed: an empty role never yields the full surface.
    let field_count = view.field_count(class_id);
    let surface = WideFieldMask::full_for(field_count);
    let projected = crate::project::project_surface(&surface, session.role_mask())?;

    // 2. Walk the class fields in ObjectView order; carry ONLY the projected,
    //    facet-backed positions — building the frame values and the fieldview
    //    rows from the one pass (mask ↔ values stay consistent).
    let fields_meta = view.fields(class_id);
    let mut carried_positions: Vec<u8> = Vec::new();
    let mut values: Vec<u8> = Vec::new();
    let mut field_views: Vec<FieldView> = Vec::new();
    for (i, f) in fields_meta.iter().enumerate() {
        if i >= FACET_LEN {
            break; // value-slab field, out of facet scope (never folded).
        }
        let pos = i as u8;
        if projected.has(pos) {
            let byte = facet[i];
            carried_positions.push(pos);
            values.push(byte);
            field_views.push(FieldView {
                position: pos,
                label: f.label.clone(),
                predicate: f.predicate_iri.clone(),
                value: fmt(pos, byte),
            });
        }
    }

    // 3a. The wire frame — the carried mask (exactly the positions we emit a
    //     byte for) + those bytes.
    let carried_mask = WideFieldMask::from_positions(&carried_positions);
    let delta = NodeDelta {
        key,
        mask_words: mask_to_words(&carried_mask),
        values,
    };

    // 3b. The askama fieldview surface — the same projection, addressed.
    let action_refs: Vec<ActionRef> = actions
        .iter()
        .enumerate()
        .map(|(i, a)| ActionRef {
            ordinal: i as u32,
            label: a.label.clone(),
        })
        .collect();
    let key_hex = hex16(&key);
    let html = render_field_view(
        class_id,
        concept,
        &key_hex,
        title,
        &field_views,
        &action_refs,
    )
    .map_err(|e| ProjectionError::Render(e.to_string()))?;

    Ok(Projection { delta, html })
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_core::mask_positions;
    use lance_graph_contract::class_view::ClassId as CvClassId;
    use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};
    use ogar_encryption::KdfParams;

    const FAST: KdfParams = KdfParams {
        m_cost_kib: 32,
        t_cost: 1,
        p_cost: 1,
    };

    /// A 3-field invoice-shaped ClassView at concept 0x0102.
    struct DemoView {
        fields: Vec<FieldRef>,
    }
    impl DemoView {
        fn new() -> Self {
            Self {
                fields: vec![
                    FieldRef::new("amount_total", "Total"),
                    FieldRef::new("state", "Status"),
                    FieldRef::new("partner_id", "Partner"),
                ],
            }
        }
    }
    impl ClassView for DemoView {
        fn fields(&self, _class: CvClassId) -> &[FieldRef] {
            &self.fields
        }
        fn template(&self, _class: CvClassId) -> DisplayTemplate {
            DisplayTemplate::Detail
        }
        fn dolce_category_id(&self, _class: CvClassId) -> u8 {
            0
        }
    }

    fn key_0102() -> [u8; 16] {
        let classid: u32 = (0x0102u32 << 16) | 0x0009;
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&classid.to_le_bytes());
        // some identity bytes in the tail
        k[10..16].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        k
    }

    fn session(role: WideFieldMask) -> Session {
        Session::establish(b"s", &[1; 16], role, (0x0100, 0x01FF), &FAST).unwrap()
    }

    #[test]
    fn projects_only_role_authorized_fields_to_wire_and_surface() {
        // Role may see fields {0, 2} (Total, Partner) but NOT field 1 (Status).
        let s = session(WideFieldMask::from_positions(&[0, 2]));
        let view = DemoView::new();
        let facet = [42u8, 99, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let actions = [ActionSpec::new("action_post", "Post")];

        let p = project_node(
            &s,
            &view,
            "commercial_document",
            key_0102(),
            &facet,
            &actions,
            "Invoice",
            |_, b| b.to_string(),
        )
        .unwrap();

        // Wire: only positions 0 and 2 carried; their bytes 42 and 7. Field 1
        // (Status=99) is ABSENT from the wire — the RBAC-by-projection headline.
        let positions: Vec<u32> = mask_positions(&p.delta.mask_words).collect();
        assert_eq!(
            positions,
            vec![0, 2],
            "only authorized positions on the wire"
        );
        assert_eq!(p.delta.values, vec![42, 7], "byte 99 (Status) absent");

        // Surface: Total + Partner addressed; Status neither addressed nor its
        // value present anywhere.
        assert!(p.html.contains("data-field-pos=\"0\""));
        assert!(p.html.contains("data-field-pos=\"2\""));
        assert!(
            !p.html.contains("data-field-pos=\"1\""),
            "Status masked out"
        );
        assert!(p.html.contains("Total") && p.html.contains("Partner"));
        assert!(
            !p.html.contains(">Status<"),
            "Status label absent:\n{}",
            p.html
        );
        assert!(!p.html.contains(">99<"), "Status value absent:\n{}", p.html);
        // Action surfaced by ordinal address.
        assert!(p.html.contains("data-action-ordinal=\"0\""));
        // The node key addresses the surface.
        assert!(p.html.contains("data-key="));
    }

    #[test]
    fn empty_role_fails_closed_no_surface_leaks() {
        let s = session(WideFieldMask::from_positions(&[0])); // established ok...
        // ...but hand project_surface an empty role via a fresh session? The
        // session refuses an empty role at establishment, so instead prove the
        // disjoint case: role grants only field 9, which this 3-field class
        // doesn't have → EmptyProjection, nothing framed.
        let _ = &s;
        let s2 = session(WideFieldMask::from_positions(&[9]));
        let view = DemoView::new();
        let facet = [0u8; 12];
        let r = project_node(&s2, &view, "c", key_0102(), &facet, &[], "T", |_, b| {
            b.to_string()
        });
        assert!(matches!(
            r,
            Err(ProjectionError::Rbac(RbacError::EmptyProjection))
        ));
    }

    #[test]
    fn class_outside_session_range_is_refused() {
        let s = session(WideFieldMask::from_positions(&[0]));
        let view = DemoView::new();
        // concept 0x0500 key — outside the grant.
        let classid: u32 = 0x0500u32 << 16;
        let mut key = [0u8; 16];
        key[0..4].copy_from_slice(&classid.to_le_bytes());
        let r = project_node(&s, &view, "c", key, &[0u8; 12], &[], "T", |_, b| {
            b.to_string()
        });
        assert_eq!(r.err(), Some(ProjectionError::Unauthorized(0x0500)));
    }
}
