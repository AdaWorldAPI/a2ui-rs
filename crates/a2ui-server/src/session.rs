//! The **rdp-2graph session** (charter C1.5) — a capability over a classid
//! range + a role mask, keyed by an Argon2id-derived session key.
//!
//! *"The session is a capability over classid ranges + role mask."* — charter
//! C1.5. Establishing a session runs Argon2id ONCE (via `ogar-encryption`'s
//! `kdf`) to turn a shared secret into the 32-byte key the
//! [`SealedTransport`](crate::transport::SealedTransport) uses for every frame
//! thereafter. Authorization is two gates, both consulted before any surface
//! leaves the server:
//!
//! - **class gate** — is the requested `class_id` inside the session's granted
//!   `[lo, hi]` classid range? (the capability's reach across the graph)
//! - **field gate** — the [`role_mask`](Session::role_mask), intersected with
//!   the class surface in [`project_surface`](crate::project::project_surface)
//!   before framing (the capability's depth into each class).

use lance_graph_contract::class_view::{ClassId, WideFieldMask};
use ogar_encryption::KdfParams;
use ogar_encryption::kdf::{self, KdfError};

/// Why a session could not be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    /// Argon2id key derivation failed (bad cost params or internal error).
    Kdf(KdfError),
    /// The granted classid range was inverted (`lo > hi`).
    InvertedRange,
    /// The role mask carried no grant — a session with no field authority is
    /// refused at establishment (fail-closed, the sentinel ban applied at the
    /// session boundary, not just per-projection).
    NoRoleGrant,
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Kdf(e) => write!(f, "session key derivation failed: {e}"),
            Self::InvertedRange => f.write_str("granted classid range is inverted (lo > hi)"),
            Self::NoRoleGrant => f.write_str("session role mask is empty — refused"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A live rdp-2graph session capability.
///
/// The `session_key` is derived once and drives the sealed transport; the
/// `role_mask` + `class_range` are the two authorization gates. Cheap to hold
/// and pass around (the key is 32 bytes; the mask is a small mask).
pub struct Session {
    session_key: [u8; 32],
    role_mask: WideFieldMask,
    class_lo: ClassId,
    class_hi: ClassId,
}

impl Session {
    /// Establish a session: derive the key from `secret` + `salt` with
    /// Argon2id, and bind the `role_mask` + inclusive `class_range` capability.
    ///
    /// `params` chooses the Argon2 cost — [`KdfParams::INTERACTIVE`] for a
    /// browser/wasm client that re-derives on each connect, [`KdfParams::DEFAULT`]
    /// for a server-grade derivation. The derivation runs exactly once per
    /// session; every frame thereafter is a cheap XChaCha20 seal/open.
    ///
    /// # Errors
    ///
    /// [`SessionError::Kdf`] if the derivation fails, [`SessionError::InvertedRange`]
    /// if `class_range.0 > class_range.1`, [`SessionError::NoRoleGrant`] if the
    /// role mask is empty (fail-closed — no capability without an explicit
    /// grant).
    pub fn establish(
        secret: &[u8],
        salt: &[u8; 16],
        role_mask: WideFieldMask,
        class_range: (ClassId, ClassId),
        params: &KdfParams,
    ) -> Result<Self, SessionError> {
        let (class_lo, class_hi) = class_range;
        if class_lo > class_hi {
            return Err(SessionError::InvertedRange);
        }
        if role_mask.is_empty() {
            return Err(SessionError::NoRoleGrant);
        }
        let derived = kdf::derive_key(secret, salt, params).map_err(SessionError::Kdf)?;
        Ok(Self {
            session_key: *derived.as_bytes(),
            role_mask,
            class_lo,
            class_hi,
        })
    }

    /// The 32-byte session key — hand this to a
    /// [`SealedTransport`](crate::transport::SealedTransport).
    #[must_use]
    pub fn key(&self) -> [u8; 32] {
        self.session_key
    }

    /// The session's field-authority mask — intersected with each class
    /// surface before framing (the depth gate).
    #[must_use]
    pub fn role_mask(&self) -> &WideFieldMask {
        &self.role_mask
    }

    /// Is `class_id` within this session's granted classid range (inclusive)?
    /// The reach gate — a class outside the range is not addressable at all in
    /// this session, before any field projection is even considered.
    #[must_use]
    pub fn authorizes_class(&self, class_id: ClassId) -> bool {
        (self.class_lo..=self.class_hi).contains(&class_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Small Argon2 cost so tests stay fast; correctness is cost-independent.
    const FAST: KdfParams = KdfParams {
        m_cost_kib: 32,
        t_cost: 1,
        p_cost: 1,
    };

    fn role() -> WideFieldMask {
        WideFieldMask::from_positions(&[0, 1, 2])
    }

    #[test]
    fn establish_derives_a_deterministic_key() {
        let a = Session::establish(b"secret", &[7; 16], role(), (0x0100, 0x01FF), &FAST).unwrap();
        let b = Session::establish(b"secret", &[7; 16], role(), (0x0100, 0x01FF), &FAST).unwrap();
        assert_eq!(a.key(), b.key(), "same secret+salt → same session key");

        let c = Session::establish(b"secret", &[8; 16], role(), (0x0100, 0x01FF), &FAST).unwrap();
        assert_ne!(a.key(), c.key(), "different salt → different key");
    }

    #[test]
    fn class_range_gate() {
        let s = Session::establish(b"s", &[1; 16], role(), (0x0100, 0x01FF), &FAST).unwrap();
        assert!(s.authorizes_class(0x0100));
        assert!(s.authorizes_class(0x0150));
        assert!(s.authorizes_class(0x01FF));
        assert!(!s.authorizes_class(0x00FF), "below range");
        assert!(!s.authorizes_class(0x0200), "above range");
    }

    #[test]
    fn empty_role_is_refused_at_establishment() {
        let r = Session::establish(b"s", &[1; 16], WideFieldMask::EMPTY, (0, 10), &FAST);
        assert_eq!(r.err(), Some(SessionError::NoRoleGrant));
    }

    #[test]
    fn inverted_range_is_refused() {
        let r = Session::establish(b"s", &[1; 16], role(), (0x0200, 0x0100), &FAST);
        assert_eq!(r.err(), Some(SessionError::InvertedRange));
    }
}
