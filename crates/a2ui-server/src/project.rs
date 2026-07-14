//! RBAC by projection (charter C1.4) — the security headline of screen
//! addressing.
//!
//! **What leaves the server is `surface ∩ role` — an unauthorized field is
//! ABSENT from the wire, not hidden on the client.** A pixel stream cannot
//! make that guarantee; an address stream gets it for free, because the
//! projection happens HERE, before framing, exactly once.
//!
//! Both the class surface and the role capability are
//! [`WideFieldMask`](lance_graph_contract::class_view::WideFieldMask)s (the
//! wide-native mask the #205 correction mandated — surfaces past 64 fields are
//! covered). The projection is a plain [`WideFieldMask::intersect`].
//!
//! # Fail-closed (the sentinel ban)
//!
//! A missing or empty role mask is a **refusal**, never a full grant.
//! `WideFieldMask::full_for` is a *render* convenience (the "emit everything"
//! sentinel for an unmasked local render); it is FORBIDDEN as an RBAC fallback
//! (charter C1.4(c)). [`project_surface`] therefore refuses an empty role mask
//! rather than returning the surface unmodified.

use lance_graph_contract::class_view::WideFieldMask;

/// Why a projection was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RbacError {
    /// The role carries no field grants at all. Fail-closed: a missing role
    /// mask is a refusal, never a full grant (the sentinel ban — charter
    /// C1.4(c)). Establish a role with an explicit non-empty grant.
    NoRoleGrant,
    /// The role's grant and the class surface share no field — the role may
    /// see the class exist, but none of its fields. Distinct from
    /// [`RbacError::NoRoleGrant`] so a caller can tell "role unconfigured"
    /// from "role deliberately projects nothing here".
    EmptyProjection,
}

impl core::fmt::Display for RbacError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoRoleGrant => {
                f.write_str("role carries no field grant — refused (no full-grant fallback)")
            }
            Self::EmptyProjection => {
                f.write_str("role ∩ surface is empty — no field of this class is authorized")
            }
        }
    }
}

impl std::error::Error for RbacError {}

/// Project a class `surface` mask through a `role` capability mask — the RBAC
/// intersection that decides what may leave the server.
///
/// Returns `surface ∩ role`. **Fail-closed:**
///
/// - an EMPTY `role` mask is [`RbacError::NoRoleGrant`] (never the surface
///   unmodified — the sentinel ban);
/// - an intersection that is empty (role and surface disjoint) is
///   [`RbacError::EmptyProjection`].
///
/// The returned mask is what a [`NodeDelta`](a2ui_core::NodeDelta) carries;
/// the fields whose bits are clear are simply not in `values` — absent from
/// the wire.
///
/// # Errors
///
/// See [`RbacError`] — both arms are refusals.
pub fn project_surface(
    surface: &WideFieldMask,
    role: &WideFieldMask,
) -> Result<WideFieldMask, RbacError> {
    if role.is_empty() {
        // Fail-closed. A role with no grant NEVER falls back to the surface.
        return Err(RbacError::NoRoleGrant);
    }
    let projected = surface.intersect(role);
    if projected.is_empty() {
        return Err(RbacError::EmptyProjection);
    }
    Ok(projected)
}

/// The u64 mask words a [`WideFieldMask`] carries — the wire form a
/// [`NodeDelta.mask_words`](a2ui_core::NodeDelta::mask_words) needs.
///
/// `WideFieldMask` exposes only `has`/`max_fields`/`count` publicly (no chunk
/// accessor), so this reconstructs the word vector from the public `has` over
/// the mask's capacity, then trims trailing all-zero words (canonical form —
/// matching `mask_positions`' expectation on the wire). Positions are `u8`
/// (≤ 255), so the scan is bounded at 256 bits regardless of the class.
#[must_use]
pub fn mask_to_words(mask: &WideFieldMask) -> Vec<u64> {
    // Positions are u8 → at most 256 bit positions (4 u64 words).
    let mut words = vec![0u64; 4];
    for p in 0u16..=255 {
        let pb = p as u8;
        if mask.has(pb) {
            words[(pb / 64) as usize] |= 1u64 << (pb % 64);
        }
    }
    while words.len() > 1 && *words.last().expect("non-empty") == 0 {
        words.pop();
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_the_intersection() {
        // surface has fields {0,1,2,3}; role may see {1,3,5}. Projection = {1,3}.
        let surface = WideFieldMask::from_positions(&[0, 1, 2, 3]);
        let role = WideFieldMask::from_positions(&[1, 3, 5]);
        let projected = project_surface(&surface, &role).unwrap();
        assert!(projected.has(1) && projected.has(3));
        assert!(!projected.has(0) && !projected.has(2) && !projected.has(5));
    }

    #[test]
    fn empty_role_is_refused_never_full_granted() {
        // The sentinel ban: an empty role mask must NOT fall back to the
        // surface. This is THE security property of screen addressing.
        let surface = WideFieldMask::from_positions(&[0, 1, 2]);
        let role = WideFieldMask::EMPTY;
        assert_eq!(
            project_surface(&surface, &role),
            Err(RbacError::NoRoleGrant)
        );
    }

    #[test]
    fn disjoint_role_yields_empty_projection_error() {
        let surface = WideFieldMask::from_positions(&[0, 1, 2]);
        let role = WideFieldMask::from_positions(&[7, 8]);
        assert_eq!(
            project_surface(&surface, &role),
            Err(RbacError::EmptyProjection)
        );
    }

    #[test]
    fn wide_projection_covers_positions_past_64() {
        // A wide surface (a field at position 133) intersected with a role
        // that grants it — the #205 correction made concrete.
        let surface = WideFieldMask::from_positions(&[0, 133]);
        let role = WideFieldMask::from_positions(&[133, 200]);
        let projected = project_surface(&surface, &role).unwrap();
        assert!(projected.has(133), "wide bit survives projection");
        assert!(!projected.has(0));
        // And its wire words carry the high bit.
        let words = mask_to_words(&projected);
        assert_eq!(words.len(), 3, "position 133 lives in word 2");
        assert_eq!(words[2], 1u64 << (133 - 128));
        assert_eq!(words[0], 0);
    }

    #[test]
    fn mask_to_words_round_trips_through_frame_positions() {
        use a2ui_core::mask_positions;
        let mask = WideFieldMask::from_positions(&[1, 3, 64, 133]);
        let words = mask_to_words(&mask);
        let positions: Vec<u32> = mask_positions(&words).collect();
        assert_eq!(positions, vec![1, 3, 64, 133]);
    }
}
