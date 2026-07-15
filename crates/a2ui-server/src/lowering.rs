//! The **#209 Klickwege lowering** — plain compile-time value construction.
//!
//! A [`KlickwegEdge`](crate::desktop::KlickwegEdge) is the runtime form of a
//! click (charter C1.6). This module lowers one into OGAR-shaped VALUES, at
//! COMPILE TIME, with nothing running:
//!
//! - [`lower_action_fire`] — an action-fire click → an *already-minted* OGAR
//!   [`ActionInvocation`](ogar_vocab::ActionInvocation). We build a value of a
//!   type OGAR owns; we do NOT mint IR (that stays OGAR-side).
//! - [`lower_screen_jump`] — a screen-jump click → a [`NavWitness`] value (the
//!   nav-witnessed-shaped value #209 names).
//!
//! # This is a seam, not a write (charter T3 / the door-knocking-compiler test)
//!
//! Both fns are pure `&KlickwegEdge → Value`: every output field derives from the
//! five owned scalars already on the edge, so NOTHING asks a running engine
//! anything — no ingest, no SPO stamp, no mailbox, no `nav_witnessed` predicate
//! const. SPO emission from an `ActionInvocation` is OGAR's own
//! `ogar-emitter::emit_action_invocation`, a SEPARATE phase; the canonical
//! `nav_witnessed` vocabulary term (OGAR's `ogar-emitter` already gates codegen
//! on a `BTreeSet<String>` of witnessed concepts) is an OGAR follow-up — a2ui-rs
//! never mints it. These fns are unit-testable with nothing running (gate G4).

use lance_graph_contract::class_view::ClassId;

use crate::desktop::KlickwegEdge;

/// The app-scoped canonical classid for an edge: the concept in the high u16
/// (canon-high), the app render prefix in the low u16. Pure arithmetic.
#[must_use]
fn canonical_classid(concept: ClassId, app_prefix: u16) -> u32 {
    ((concept as u32) << 16) | (app_prefix as u32)
}

/// Lowercase hex of a 16-byte key — the node's stable object address.
fn hex16(key: &[u8; 16]) -> String {
    use core::fmt::Write;
    let mut s = String::with_capacity(32);
    for b in key {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Lower an **action-fire** click into an OGAR [`ActionInvocation`](ogar_vocab::ActionInvocation).
///
/// Value derivation (all from the edge + `app_prefix`, no lookup):
/// - `object_instance` = the clicked node's GUID (hex of `from_key`);
/// - `action_def` = the app-scoped classid + the ActionDef predicate the ordinal
///   named (`edge.predicate` already carries it — no ActionDef-list lookup);
/// - `identity` = a deterministic per-click id from `(from_key, seq)`;
/// - `subject` = [`ActionSubject::User`](ogar_vocab::ActionSubject::User) — a
///   human UI click (#209 design point: the `submit_to_invocation` default is
///   `System`; a click is `User`). Every other field is the OGAR default
///   (`ActionState::Pending`, immediate temporal, provenance `None`) via
///   [`ActionInvocation::new`](ogar_vocab::ActionInvocation::new) — we reuse the
///   constructor, we do not re-implement defaulting.
#[must_use]
pub fn lower_action_fire(edge: &KlickwegEdge, app_prefix: u16) -> ogar_vocab::ActionInvocation {
    let classid = canonical_classid(edge.class_id, app_prefix);
    let object = hex16(&edge.from_key);
    let action_def = format!("ogar:action:{classid:08x}:{}", edge.predicate);
    let identity = format!("ogar:invocation:{object}:{}", edge.seq);

    let mut inv = ogar_vocab::ActionInvocation::new(identity, action_def, object);
    inv.subject = ogar_vocab::ActionSubject::User;
    inv
}

/// The nav-witnessed-shaped value a **screen-jump** click lowers to (#209). A
/// plain value derived purely from the edge — NOT an SPO triple, NOT a predicate
/// constant. It records that a navigation was witnessed from `from_concept` under
/// the Klickweg edge label `predicate`. The canonical `nav_witnessed` vocabulary
/// term (for any downstream SPO framing) is OGAR-owned (an OGAR follow-up).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavWitness {
    /// The concept the click originated on (the navigation's source).
    pub from_concept: ClassId,
    /// The Klickweg edge label the ordinal named (the navigation predicate).
    pub predicate: String,
    /// The monotonic per-session sequence of this witnessed navigation.
    pub seq: u64,
}

/// Lower a **screen-jump** click into a [`NavWitness`] value. Pure: reads only
/// the edge. No SPO, no const-mint, no lookup.
#[must_use]
pub fn lower_screen_jump(edge: &KlickwegEdge) -> NavWitness {
    NavWitness {
        from_concept: edge.class_id,
        predicate: edge.predicate.clone(),
        seq: edge.seq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed synthetic edge — a click on node `0x…` in concept 0x0102, ordinal
    /// 3 named predicate "action_post", session seq 7. Corpus-free (SoC: the
    /// a2ui-rs golden never reads the MedCare harvest; the harvested≡live parity
    /// is a MedCare-side test).
    fn synthetic_edge() -> KlickwegEdge {
        KlickwegEdge {
            from_key: [
                0x02, 0x01, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            ],
            class_id: 0x0102,
            ordinal: 3,
            predicate: "action_post".to_string(),
            seq: 7,
        }
    }

    /// GOLDEN: the synthetic edge lowers to an exact, deterministic
    /// `ActionInvocation`. This digest IS the golden — same edge, same value,
    /// forever, with nothing running (gate G4).
    #[test]
    fn action_fire_golden_is_deterministic() {
        let inv = lower_action_fire(&synthetic_edge(), 0x0007);

        // classid = (0x0102 << 16) | 0x0007 = 0x01020007.
        assert_eq!(
            inv.action_def, "ogar:action:01020007:action_post",
            "app-scoped classid + the ordinal's ActionDef predicate"
        );
        assert_eq!(
            inv.object_instance, "02010000000000000000112233445566",
            "object_instance = the clicked node's GUID hex"
        );
        assert_eq!(
            inv.identity,
            "ogar:invocation:02010000000000000000112233445566:7"
        );
        // A human click is a User subject (the #209 design point).
        assert_eq!(inv.subject, ogar_vocab::ActionSubject::User);
        // Everything else is the OGAR default (Pending, no provenance).
        assert_eq!(inv.state, ogar_vocab::ActionState::Pending);
        assert_eq!(inv.trace_id, None);
        assert_eq!(inv.parent_invocation, None);
    }

    /// Two edges differing only in `seq` produce distinct identities (a click is
    /// uniquely addressable) but the SAME action_def (same behavior address).
    #[test]
    fn identity_is_per_click_but_action_def_is_stable() {
        let mut e2 = synthetic_edge();
        e2.seq = 8;
        let a = lower_action_fire(&synthetic_edge(), 0x0007);
        let b = lower_action_fire(&e2, 0x0007);
        assert_ne!(a.identity, b.identity, "distinct clicks, distinct ids");
        assert_eq!(a.action_def, b.action_def, "same behavior address");
    }

    /// GOLDEN: the screen-jump lowers to an exact `NavWitness` value — pure,
    /// derived only from the edge.
    #[test]
    fn screen_jump_golden_is_a_pure_value() {
        let nav = lower_screen_jump(&synthetic_edge());
        assert_eq!(
            nav,
            NavWitness {
                from_concept: 0x0102,
                predicate: "action_post".to_string(),
                seq: 7,
            }
        );
    }
}
