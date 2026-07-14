//! The **up-wire** (client → server): resolve an [`ActionInvoke`] by its
//! ordinal ADDRESS (charter trap T2).
//!
//! The surface never carried a handler — it carried the *address* of one: an
//! ordinal into the class's `ActionDef` set. This module turns that ordinal
//! back into the `ActionDef` it names, gated by the session's class capability.
//! The actual state mutation is the consumer's business (it owns the row); this
//! module owns only *address resolution + authorization* — the exact division
//! the charter draws (`DEFINE EVENT` / `onClick: <lambda>` are the hijack this
//! refuses by construction: there is no handler on the wire to run).

use a2ui_core::ActionInvoke;
use lance_graph_contract::class_view::ClassId;

use crate::session::Session;

/// One entry in a class's `ActionDef` set — the runtime projection of the
/// ruff-codegen'd DO-arm (the same `ActionDef`s
/// `ogar_render_askama::render_class_with_methods_wide` lifts into a
/// struct-of-methods). Its INDEX in the class's action slice IS its ordinal
/// address (`ActionInvoke::action_ordinal`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    /// The action's predicate on the Core node (e.g. `"action_post"`), the
    /// stable name behind the ordinal.
    pub predicate: String,
    /// Human caption for the surface control.
    pub label: String,
}

impl ActionSpec {
    /// A spec with `predicate` and `label`.
    pub fn new(predicate: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            predicate: predicate.into(),
            label: label.into(),
        }
    }
}

/// The resolution of an [`ActionInvoke`] against a class's action set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    /// The node the action targets.
    pub key: [u8; 16],
    /// The concept the key resolves to.
    pub class_id: ClassId,
    /// The ordinal address the client sent.
    pub ordinal: u32,
    /// The `ActionDef` predicate the ordinal names — what the consumer runs.
    pub predicate: String,
    /// The invocation arguments, ClassView/ActionDef-schema carved (opaque).
    pub args: Vec<u8>,
}

/// Why an [`ActionInvoke`] could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// The key's concept is outside the session's granted classid range.
    Unauthorized(ClassId),
    /// The ordinal is past the end of the class's action set — a dangling
    /// address. Refused (never a silent no-op).
    OrdinalOutOfRange {
        /// The ordinal the client sent.
        ordinal: u32,
        /// The number of actions the class actually declares.
        available: usize,
    },
}

impl core::fmt::Display for ActionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unauthorized(c) => write!(f, "session does not authorize class 0x{c:04X}"),
            Self::OrdinalOutOfRange { ordinal, available } => {
                write!(
                    f,
                    "action ordinal {ordinal} >= {available} declared actions"
                )
            }
        }
    }
}

impl std::error::Error for ActionError {}

/// The concept a canonical GUID `key` resolves to, with **zero value decode**
/// (OGAR P0 "the key prerenders nodes"): the classid is the u32 at bytes 0..4
/// (LE), and under the canon-high flip its HIGH u16 is the shared concept
/// (the low u16 is the per-app render prefix). The concept is the [`ClassId`]
/// the [`ClassView`](lance_graph_contract::class_view::ClassView) is keyed by.
#[must_use]
pub fn concept_of_key(key: &[u8; 16]) -> ClassId {
    let classid = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
    (classid >> 16) as ClassId
}

/// Resolve an [`ActionInvoke`] to the `ActionDef` its ordinal names, gated by
/// the session's class capability.
///
/// The `class_id` is read from the invoke's own key (`concept_of_key`) — the
/// address carries its own class. `actions` is the class's `ActionDef` set (in
/// ordinal order); the ordinal indexes it.
///
/// # Errors
///
/// [`ActionError::Unauthorized`] if the key's concept is outside the session's
/// range; [`ActionError::OrdinalOutOfRange`] if the ordinal has no `ActionDef`.
pub fn resolve_action(
    session: &Session,
    invoke: &ActionInvoke,
    actions: &[ActionSpec],
) -> Result<ResolvedAction, ActionError> {
    let class_id = concept_of_key(&invoke.key);
    if !session.authorizes_class(class_id) {
        return Err(ActionError::Unauthorized(class_id));
    }
    let ordinal = invoke.action_ordinal;
    let spec = actions
        .get(ordinal as usize)
        .ok_or(ActionError::OrdinalOutOfRange {
            ordinal,
            available: actions.len(),
        })?;
    Ok(ResolvedAction {
        key: invoke.key,
        class_id,
        ordinal,
        predicate: spec.predicate.clone(),
        args: invoke.args.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::class_view::WideFieldMask;
    use ogar_encryption::KdfParams;

    const FAST: KdfParams = KdfParams {
        m_cost_kib: 32,
        t_cost: 1,
        p_cost: 1,
    };

    /// A key whose classid u32 (bytes 0..4 LE) has concept 0x0102 in its high
    /// u16 and app-prefix 0x0009 in its low u16.
    fn key_for_concept_0102() -> [u8; 16] {
        let classid: u32 = (0x0102u32 << 16) | 0x0009;
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&classid.to_le_bytes());
        k
    }

    fn session() -> Session {
        Session::establish(
            b"s",
            &[1; 16],
            WideFieldMask::from_positions(&[0, 1, 2]),
            (0x0100, 0x01FF),
            &FAST,
        )
        .unwrap()
    }

    fn actions() -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("name_get", "View"),
            ActionSpec::new("action_post", "Post"),
        ]
    }

    #[test]
    fn concept_reads_from_the_key_high_u16() {
        assert_eq!(concept_of_key(&key_for_concept_0102()), 0x0102);
    }

    #[test]
    fn resolves_ordinal_to_predicate_by_address() {
        let s = session();
        let invoke = ActionInvoke {
            key: key_for_concept_0102(),
            action_ordinal: 1,
            args: vec![7, 8],
        };
        let resolved = resolve_action(&s, &invoke, &actions()).unwrap();
        assert_eq!(resolved.class_id, 0x0102);
        assert_eq!(resolved.ordinal, 1);
        assert_eq!(resolved.predicate, "action_post");
        assert_eq!(resolved.args, vec![7, 8]);
    }

    #[test]
    fn out_of_range_ordinal_is_refused() {
        let s = session();
        let invoke = ActionInvoke {
            key: key_for_concept_0102(),
            action_ordinal: 9,
            args: vec![],
        };
        assert_eq!(
            resolve_action(&s, &invoke, &actions()),
            Err(ActionError::OrdinalOutOfRange {
                ordinal: 9,
                available: 2
            })
        );
    }

    #[test]
    fn class_outside_session_range_is_refused() {
        let s = session();
        // concept 0x0500 — outside the (0x0100..=0x01FF) grant.
        let classid: u32 = 0x0500u32 << 16;
        let mut key = [0u8; 16];
        key[0..4].copy_from_slice(&classid.to_le_bytes());
        let invoke = ActionInvoke {
            key,
            action_ordinal: 0,
            args: vec![],
        };
        assert_eq!(
            resolve_action(&s, &invoke, &actions()),
            Err(ActionError::Unauthorized(0x0500))
        );
    }
}
