//! The **RBAC hot-plug gates** — the session's two gates, answered by the
//! consuming app's bound [`RbacBinding`] as well.
//!
//! a2ui-rs owns no classes and no roles: it renders whatever app it serves.
//! So it declares no `RbacPlug` of its own; the app does (e.g. medcare-rs'
//! `RBAC_PLUG`), binds it against its authority, and hands the
//! [`RbacBinding`] here. The two functions below add that binding's verdict to
//! the existing [`Session`] gates, which they call unchanged:
//!
//! - [`project_plugged`] — the field gate for one node: READ must be granted
//!   on the node's class, and the role's declared field mask (if any) narrows
//!   the session's role mask before [`project_surface`] intersects it with the
//!   surface.
//! - [`resolve_action_plugged`] — the action gate: after [`resolve_action`]
//!   (class range + ordinal), ACT must be granted for the resolved predicate.
//!
//! Every failure refuses. A class outside the plug is
//! [`RbacDrift::NotPlugged`], never "allowed because unknown".

use a2ui_core::ActionInvoke;
use lance_graph_contract::class_view::WideFieldMask;
use lance_graph_contract::property::PrefetchDepth;
use lance_graph_contract::rbac::{ClassId, Operation, RoleId};
use lance_graph_contract::rbac_plug::{RbacBinding, RbacDrift};

use crate::action_stream::{ActionError, ActionSpec, ResolvedAction, resolve_action};
use crate::project::{RbacError, project_surface};
use crate::session::Session;

/// Why a plugged gate refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluggedError {
    /// The class or role lies outside the app's plug.
    Drift(RbacDrift),
    /// The role holds no grant for this verb on this class.
    Denied {
        /// The full classid that was asked about.
        class: ClassId,
        /// The verb that was refused (`"read"` or `"act"`).
        verb: &'static str,
    },
    /// The field projection refused (see [`RbacError`]).
    Projection(RbacError),
    /// The session refused the action (see [`ActionError`]).
    Action(ActionError),
}

impl core::fmt::Display for PluggedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Drift(d) => write!(f, "outside the RBAC plug: {d:?}"),
            Self::Denied { class, verb } => {
                write!(f, "role holds no {verb} grant on class 0x{class:08X}")
            }
            Self::Projection(e) => e.fmt(f),
            Self::Action(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for PluggedError {}

impl From<RbacDrift> for PluggedError {
    fn from(d: RbacDrift) -> Self {
        Self::Drift(d)
    }
}

/// The full classid a canonical GUID `key` carries (bytes 0..4, LE). The
/// binding is asked with this, so the app's render prefix (the low u16) is
/// read by the same canon-compat rule as everywhere else.
#[must_use]
pub fn classid_of_key(key: &[u8; 16]) -> ClassId {
    u32::from_le_bytes([key[0], key[1], key[2], key[3]])
}

/// The field gate for the node at `key`: what may leave the server for `role`.
///
/// Requires a READ grant on the node's class, narrows the session's role mask
/// by the role's declared field mask for that class (none declared = no
/// narrowing), then intersects with `surface` through [`project_surface`] —
/// so every fail-closed rule of the unplugged path still holds.
///
/// The class-range gate is NOT re-checked here; callers already hold the
/// session for the frame they are projecting into.
///
/// # Errors
///
/// [`PluggedError::Drift`] if the class or role is outside the plug,
/// [`PluggedError::Denied`] without a READ grant, [`PluggedError::Projection`]
/// if the narrowed projection is empty.
pub fn project_plugged(
    session: &Session,
    binding: &RbacBinding,
    role: RoleId,
    key: &[u8; 16],
    surface: &WideFieldMask,
) -> Result<WideFieldMask, PluggedError> {
    let class = classid_of_key(key);
    let read = Operation::Read {
        depth: PrefetchDepth::Identity,
    };
    if !binding.permits(role, class, &read)? {
        return Err(PluggedError::Denied {
            class,
            verb: "read",
        });
    }
    let role_mask = match binding.field_mask_for(role, class)? {
        Some(declared) => session.role_mask().intersect(declared),
        None => session.role_mask().clone(),
    };
    project_surface(surface, &role_mask).map_err(PluggedError::Projection)
}

/// The action gate: resolve `invoke` through the session (class range and
/// ordinal, unchanged), then require an ACT grant for the resolved predicate.
///
/// # Errors
///
/// [`PluggedError::Action`] if the session refuses, [`PluggedError::Drift`]
/// if the class or role is outside the plug, [`PluggedError::Denied`] without
/// an ACT grant.
pub fn resolve_action_plugged(
    session: &Session,
    binding: &RbacBinding,
    role: RoleId,
    invoke: &ActionInvoke,
    actions: &[ActionSpec],
) -> Result<ResolvedAction, PluggedError> {
    let resolved = resolve_action(session, invoke, actions).map_err(PluggedError::Action)?;
    let class = classid_of_key(&invoke.key);
    let act = Operation::Act {
        action: &resolved.predicate,
    };
    if !binding.permits(role, class, &act)? {
        return Err(PluggedError::Denied { class, verb: "act" });
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::rbac::{ClassGrant, OpMask};
    use ogar_encryption::KdfParams;

    const FAST: KdfParams = KdfParams {
        m_cost_kib: 32,
        t_cost: 1,
        p_cost: 1,
    };
    const PATIENT: u16 = 0x0901;
    const DIAGNOSIS: u16 = 0x0902;
    const LAB: u16 = 0x0903; // in the session range, NOT in the plug

    fn key(concept: u16) -> [u8; 16] {
        let classid = (u32::from(concept) << 16) | 0x0042; // app render prefix
        let mut k = [0u8; 16];
        k[..4].copy_from_slice(&classid.to_le_bytes());
        k
    }

    /// physician: read+act on both classes, Diagnosis narrowed to fields 0..3.
    /// cashier: read Patient only.
    fn binding() -> RbacBinding {
        RbacBinding::new(
            "test-app",
            vec![PATIENT, DIAGNOSIS],
            vec![
                (
                    "physician",
                    vec![
                        ClassGrant::new(PATIENT, OpMask::READ.union(OpMask::ACT)),
                        ClassGrant::new(DIAGNOSIS, OpMask::READ.union(OpMask::ACT)),
                    ],
                ),
                ("cashier", vec![ClassGrant::new(PATIENT, OpMask::READ)]),
            ],
            vec![(
                "physician",
                DIAGNOSIS,
                WideFieldMask::from_positions(&[0, 1, 2]),
            )],
        )
    }

    fn session() -> Session {
        let all = WideFieldMask::from_positions(&[0, 1, 2, 3, 4, 5]);
        Session::establish(b"s", &[1; 16], all, (0x0900, 0x09FF), &FAST).unwrap()
    }

    fn surface() -> WideFieldMask {
        WideFieldMask::from_positions(&[1, 2, 3, 4])
    }

    #[test]
    fn a_granted_class_projects_the_session_mask() {
        let got = project_plugged(&session(), &binding(), "cashier", &key(PATIENT), &surface());
        assert_eq!(got, Ok(surface()), "no declared mask: no narrowing");
    }

    #[test]
    fn a_declared_field_mask_narrows_the_projection() {
        let got = project_plugged(
            &session(),
            &binding(),
            "physician",
            &key(DIAGNOSIS),
            &surface(),
        );
        assert_eq!(got, Ok(WideFieldMask::from_positions(&[1, 2])));
    }

    #[test]
    fn a_class_without_a_read_grant_is_denied() {
        let got = project_plugged(
            &session(),
            &binding(),
            "cashier",
            &key(DIAGNOSIS),
            &surface(),
        );
        assert!(matches!(
            got,
            Err(PluggedError::Denied { verb: "read", .. })
        ));
    }

    // The session range covers LAB; the plug does not. Fail closed.
    #[test]
    fn a_class_outside_the_plug_is_refused_not_allowed() {
        let got = project_plugged(&session(), &binding(), "physician", &key(LAB), &surface());
        assert_eq!(got, Err(PluggedError::Drift(RbacDrift::NotPlugged(LAB))));
        let undeclared =
            project_plugged(&session(), &binding(), "janitor", &key(PATIENT), &surface());
        assert!(matches!(
            undeclared,
            Err(PluggedError::Drift(RbacDrift::RoleNotPlugged(_)))
        ));
    }

    fn invoke(concept: u16) -> ActionInvoke {
        ActionInvoke {
            key: key(concept),
            action_ordinal: 0,
            args: Vec::new(),
        }
    }

    #[test]
    fn act_needs_an_act_grant() {
        let actions = [ActionSpec::new("discharge", "Discharge")];
        let ok = resolve_action_plugged(
            &session(),
            &binding(),
            "physician",
            &invoke(PATIENT),
            &actions,
        );
        assert_eq!(ok.map(|r| r.predicate), Ok("discharge".to_string()));
        let denied = resolve_action_plugged(
            &session(),
            &binding(),
            "cashier",
            &invoke(PATIENT),
            &actions,
        );
        assert!(matches!(
            denied,
            Err(PluggedError::Denied { verb: "act", .. })
        ));
    }

    #[test]
    fn the_session_gates_still_run_first() {
        let actions = [ActionSpec::new("discharge", "Discharge")];
        let outside = resolve_action_plugged(
            &session(),
            &binding(),
            "physician",
            &invoke(0x0A01),
            &actions,
        );
        assert!(matches!(
            outside,
            Err(PluggedError::Action(ActionError::Unauthorized(_)))
        ));
        let dangling =
            resolve_action_plugged(&session(), &binding(), "physician", &invoke(PATIENT), &[]);
        assert!(matches!(
            dangling,
            Err(PluggedError::Action(ActionError::OrdinalOutOfRange { .. }))
        ));
    }
}
