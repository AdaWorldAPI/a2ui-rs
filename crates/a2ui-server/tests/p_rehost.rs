//! **P-REHOST-lite** — the killer probe of the A2UI screen-addressing charter
//! (`docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md` C4), run end-to-end in a2ui-rs.
//!
//! > *Harvest the app → re-render the app, no WinForms.*
//!
//! The full charter probe re-hosts one harvested MedCare WinForms screen. This
//! is the *-lite* version: a2ui-rs does not vendor the MedCare harvest, so the
//! probe uses a hand-built harvested-SHAPE class (a `Class` + `ActionDef` set,
//! exactly what the ruff harvest emits) as the stand-in corpus. The MECHANISM
//! it proves is the whole one:
//!
//! 1. **ruff codegen + askama** — the harvested `Class` × its `ActionDef` DO-arm
//!    lowers through `ogar_render_askama::render_class_with_methods_wide` into a
//!    Rust struct-of-methods (the build-time arm).
//! 2. **ClassView + WideFieldMask** — the SAME class is RBAC-projected
//!    (`surface ∩ role`, fail-closed) at runtime.
//! 3. **graph desktop projection + askama** — the projection frames a
//!    `NodeDelta` down AND renders the askama fieldview surface, addressed by
//!    field position.
//! 4. **ogar-encryption** — the frame travels sealed (Argon2 session KDF +
//!    XChaCha20 AEAD) and round-trips.
//! 5. **action round-trip (T2)** — an `ActionInvoke` goes back up by ordinal
//!    ADDRESS, sealed, and resolves to the harvested `ActionDef`'s predicate.
//!
//! GREEN ⇒ the legacy screen, re-rendered from the graph and driven by
//! address, with no WinForms and no pixels on the wire.

use a2ui_server::{
    ActionInvoke, ActionSpec, Direction, Frame, SealedTransport, Session, concept_of_key,
    mask_to_words, project_node, resolve_action,
};
use lance_graph_contract::class_view::{ClassId, ClassView, WideFieldMask};
use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};
use ogar_encryption::KdfParams;
use ogar_render_askama::render_class_with_methods_wide;
use ogar_vocab::{ActionDef, Attribute, Class, EnterEffect};

/// The concept the "harvested screen" projects — a real codebook concept so
/// `render_class_with_methods_wide` emits a `CLASS_ID`, and `concept_of_key`
/// agrees with `canonical_concept_id`.
const CONCEPT: &str = "project_work_item";
const CLASS_ID: ClassId = 0x0102;

/// Fast Argon2 params — a real session KDF, small cost so the test is quick
/// (correctness is cost-independent; production picks DEFAULT/INTERACTIVE).
const FAST_KDF: KdfParams = KdfParams {
    m_cost_kib: 32,
    t_cost: 1,
    p_cost: 1,
};

fn attr(name: &str, ty: &str) -> Attribute {
    let mut a = Attribute::default();
    a.name = name.to_string();
    a.type_name = Some(ty.to_string());
    a
}

/// The harvested class: three fields (subject, status, done_ratio) + two
/// harvested actions (a read `view`, a mutating `post`) — the shape a ruff
/// harvest of a MedCare-style form emits.
fn harvested_class() -> Class {
    let mut c = Class::new("project.work_item");
    c.canonical_concept = Some(CONCEPT.to_string());
    c.attributes = vec![
        attr("subject", "string"),
        attr("status", "string"),
        attr("done_ratio", "integer"),
    ];
    c
}

fn harvested_actions() -> Vec<ActionDef> {
    // ordinal 0 — a read action.
    let mut view = ActionDef::default();
    view.identity = "project.work_item::action_def::view".to_string();
    view.predicate = "view".to_string();
    view.object_class = "project.work_item".to_string();

    // ordinal 1 — a mutating action (crosses the Rubicon: writes `status`).
    let mut post = ActionDef::default();
    post.identity = "project.work_item::action_def::action_post".to_string();
    post.predicate = "action_post".to_string();
    post.object_class = "project.work_item".to_string();
    post.on_enter = Some(EnterEffect::transition("status", "posted"));

    vec![view, post]
}

/// The runtime ClassView over the harvested class — its `fields()` mirror the
/// harvested attributes in ObjectView order (the harvest IS the manifest).
struct HarvestView {
    fields: Vec<FieldRef>,
}
impl HarvestView {
    fn from(class: &Class) -> Self {
        Self {
            fields: class
                .attributes
                .iter()
                .map(|a| FieldRef::new(a.name.clone(), title_case(&a.name)))
                .collect(),
        }
    }
}
impl ClassView for HarvestView {
    fn fields(&self, _class: ClassId) -> &[FieldRef] {
        &self.fields
    }
    fn template(&self, _class: ClassId) -> DisplayTemplate {
        DisplayTemplate::Detail
    }
    fn dolce_category_id(&self, _class: ClassId) -> u8 {
        0
    }
}

fn title_case(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in s.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// The node's canonical 16-byte GUID: classid u32 = (concept << 16) | app,
/// then some identity tail. The class resolves from the key alone.
fn node_key() -> [u8; 16] {
    let classid: u32 = ((CLASS_ID as u32) << 16) | 0x0009; // app-prefix 0x0009
    let mut k = [0u8; 16];
    k[0..4].copy_from_slice(&classid.to_le_bytes());
    k[10..16].copy_from_slice(&[0xAB, 0xCD, 0xEF, 0x01, 0x02, 0x03]); // identity
    k
}

#[test]
fn p_rehost_lite_harvest_to_rehost_no_winforms() {
    let class = harvested_class();
    let actions = harvested_actions();

    // ── (1) ruff codegen + askama: the harvested DO-arm → struct-of-methods ──
    // The FULL surface (every field) lowered through askama. This is the
    // build-time proof that the harvested Class + ActionDefs compile to code.
    let full = WideFieldMask::full_for(class.attributes.len());
    let codegen = render_class_with_methods_wide(&class, &full, &actions).unwrap();
    assert!(
        codegen.contains("pub struct ProjectWorkItem"),
        "codegen struct:\n{codegen}"
    );
    assert!(
        codegen.contains("pub const CLASS_ID: u16 = 0x0102;"),
        "{codegen}"
    );
    // The DO-arm: each ActionDef is a method; the mutating one takes &mut self.
    assert!(
        codegen.contains("pub fn view(&self)"),
        "read action:\n{codegen}"
    );
    assert!(
        codegen.contains("pub fn action_post(&mut self)"),
        "mutating action → &mut self:\n{codegen}"
    );
    // T2 sanity: behavior is Rust methods, never SurrealQL DDL.
    assert!(
        !codegen.contains("DEFINE EVENT"),
        "no DDL behavior:\n{codegen}"
    );

    // ── (2) the rdp-2graph session (ogar-encryption Argon2 KDF) ──────────────
    // A role that may see fields 0 (subject) and 1 (status) but NOT 2
    // (done_ratio) — the RBAC projection will drop field 2 from the wire.
    let role = WideFieldMask::from_positions(&[0, 1]);
    let server = Session::establish(
        b"shared-session-secret",
        &[0x11; 16],
        role.clone(),
        (0x0100, 0x01FF), // the granted classid range covers 0x0102
        &FAST_KDF,
    )
    .unwrap();
    let client = Session::establish(
        b"shared-session-secret",
        &[0x11; 16],
        role,
        (0x0100, 0x01FF),
        &FAST_KDF,
    )
    .unwrap();
    assert_eq!(
        server.key(),
        client.key(),
        "both ends share the session key"
    );

    // ── (3) graph desktop projection + askama fieldview (down the wire) ──────
    let view = HarvestView::from(&class);
    let key = node_key();
    assert_eq!(
        concept_of_key(&key),
        CLASS_ID,
        "class resolves from the key"
    );
    // The node's 12-byte content-blind facet: subject=1, status=2, done=87…
    let facet = [1u8, 2, 87, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let action_specs = vec![
        ActionSpec::new("view", "View"),
        ActionSpec::new("action_post", "Post"),
    ];

    let projection = project_node(
        &server,
        &view,
        CONCEPT,
        key,
        &facet,
        &action_specs,
        "Work item #42",
        |_pos, b| b.to_string(),
    )
    .unwrap();

    // RBAC by projection: field 2 (done_ratio=87) is ABSENT from the wire.
    assert_eq!(
        projection.delta.mask_words,
        mask_to_words(&WideFieldMask::from_positions(&[0, 1])),
        "only subject+status positions on the wire"
    );
    assert_eq!(
        projection.delta.values,
        vec![1, 2],
        "only subject+status bytes on the wire; done_ratio=87 absent"
    );
    // The askama fieldview surface is addressed by position; done_ratio absent.
    assert!(projection.html.contains("data-field-pos=\"0\""));
    assert!(projection.html.contains("data-field-pos=\"1\""));
    assert!(
        !projection.html.contains("data-field-pos=\"2\""),
        "done_ratio must be masked off the surface:\n{}",
        projection.html
    );
    assert!(projection.html.contains("Subject") && projection.html.contains("Status"));
    assert!(
        !projection.html.contains(">87<"),
        "done_ratio value must not appear anywhere:\n{}",
        projection.html
    );
    // Actions surfaced by ordinal address (no inline handler).
    assert!(projection.html.contains("data-action-ordinal=\"1\""));
    assert!(!projection.html.contains("onclick"));

    // ── (4) ogar-encryption sealed transport (server → client) ───────────────
    let mut server_down = SealedTransport::new(server.key(), Direction::ServerToClient);
    let mut client_down = SealedTransport::new(client.key(), Direction::ServerToClient);
    let down_frame = Frame::NodeDelta(projection.delta.clone());
    let sealed = server_down.seal_frame(&down_frame);
    // The plaintext bytes are not visible in the sealed blob.
    assert!(
        !sealed.windows(16).any(|w| w == key),
        "node key leaked in the sealed blob"
    );
    let received = client_down.open_frame(&sealed).unwrap();
    assert_eq!(received, down_frame, "the client opens the exact frame");
    // The client sees the same addressed positions after unseal.
    if let Frame::NodeDelta(nd) = &received {
        assert_eq!(nd.key, key);
        assert_eq!(nd.values, vec![1, 2]);
    } else {
        panic!("expected a NodeDelta down the wire");
    }

    // ── (5) action round-trip up the wire, by ADDRESS (T2) ───────────────────
    // The client clicks the "Post" control → an ActionInvoke carrying only the
    // ordinal ADDRESS (1), sealed, sent up.
    let mut client_up = SealedTransport::new(client.key(), Direction::ClientToServer);
    let mut server_up = SealedTransport::new(server.key(), Direction::ClientToServer);
    let invoke = Frame::ActionInvoke(ActionInvoke {
        key,
        action_ordinal: 1, // "Post" — an address, never a handler
        args: vec![],
    });
    let sealed_up = client_up.seal_frame(&invoke);
    let received_up = server_up.open_frame(&sealed_up).unwrap();
    let Frame::ActionInvoke(ai) = received_up else {
        panic!("expected an ActionInvoke up the wire");
    };

    // The server resolves the ordinal to the harvested ActionDef's predicate —
    // the T2 resolution: behavior lives on the Core node, the wire carried only
    // its address.
    let resolved = resolve_action(&server, &ai, &action_specs).unwrap();
    assert_eq!(resolved.class_id, CLASS_ID);
    assert_eq!(resolved.ordinal, 1);
    assert_eq!(
        resolved.predicate, "action_post",
        "ordinal 1 resolves to the harvested mutating action"
    );

    // GREEN: harvested class → codegen'd struct, projected + rendered + framed +
    // sealed down, action addressed + sealed + resolved up. No WinForms, no
    // pixels — the screen was addressed, not rasterized.
}

/// A second probe: a session whose role is DISJOINT from the class surface
/// must fail closed — nothing frames, nothing renders, no partial surface
/// leaks. The RBAC-by-projection guarantee, proven negatively.
#[test]
fn p_rehost_lite_unauthorized_role_leaks_nothing() {
    let class = harvested_class();
    let view = HarvestView::from(&class);
    // Role grants only field 9 — the 3-field class has no such field.
    let role = WideFieldMask::from_positions(&[9]);
    let session = Session::establish(b"s", &[1; 16], role, (0x0100, 0x01FF), &FAST_KDF).unwrap();
    let result = project_node(
        &session,
        &view,
        CONCEPT,
        node_key(),
        &[1u8, 2, 87, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        &[],
        "Work item",
        |_p, b| b.to_string(),
    );
    // Fail-closed: an empty projection is a refusal, not an empty surface.
    assert!(
        result.is_err(),
        "a role disjoint from the surface must leak nothing"
    );
}
