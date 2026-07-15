//! The rdp-2graph **sealed session transport** (charter C1.5, W5) — security
//! by reuse, no new crypto.
//!
//! The session key is derived ONCE with Argon2id (`ogar-encryption`'s `kdf`,
//! the shared forward-crypto suite `ogar-auth` also builds its password/TOTP
//! flows on — security by reuse, consumer-agnostic). Each frame is then sealed
//! with XChaCha20-Poly1305 (`ogar-encryption`'s `aead`) under that key. The
//! frame's own `to_le_bytes` IS the plaintext — the Firewall holds
//! (charter T3): we AEAD the already-LE wire bytes, we never serialize a
//! struct to transport it.
//!
//! # Nonce = a per-direction monotonic counter (not an RNG draw)
//!
//! A live desktop stream is an ORDERED sequence of frames, which is exactly
//! the case where a counter nonce is the textbook-correct choice — it needs no
//! CSPRNG, it is deterministic (hence testable), and it gives replay/reorder
//! detection for free. The 24-byte XChaCha nonce is
//! `[direction:1][counter:8 LE][zero:15]`; the `direction` byte separates the
//! server→client and client→server half-duplex streams so the two never reuse
//! a `(key, nonce)` pair (the one catastrophic XChaCha failure mode). Because
//! XChaCha's nonce is 192-bit, a u64 counter cannot come close to wrapping in
//! any real session.
//!
//! **This safety rests on one upstream invariant: the key is unique per
//! session.** Counter nonces both start at 0, so if two sessions share a key
//! they produce identical `(key, nonce)` pairs — the very failure the direction
//! byte cannot prevent (it is identical across the two too). The key is unique
//! only if the session salt is a fresh CSPRNG draw per session — see the
//! security note on [`Session::establish`](crate::session::Session::establish)
//! and prefer
//! [`Session::establish_random_salt`](crate::session::Session::establish_random_salt),
//! which guarantees it.
//!
//! # Sealed wire form
//!
//! ```text
//! [direction:1][counter:8 LE][ciphertext ‖ 16-byte Poly1305 tag]
//! ```
//!
//! The receiver reads `direction`+`counter`, reconstructs the nonce, and
//! `open`s the rest. [`open_frame`] REJECTS a counter that is not strictly
//! greater than the last accepted one on that direction — so a replayed or
//! reordered frame fails closed, it is never applied twice.

use a2ui_core::{Frame, FrameError};
use ogar_encryption::aead::{self, NONCE_LEN, TAG_LEN};

/// A sealed frame's fixed prefix: `direction(1) + counter(8)`.
const SEAL_PREFIX: usize = 1 + 8;

/// Which half-duplex stream a sealed frame belongs to — the nonce
/// `direction` byte, so the two directions never share a `(key, nonce)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Direction {
    /// Server → client (the addressed-surface `NodeDelta` stream).
    ServerToClient = 0,
    /// Client → server (the `ActionInvoke` stream).
    ClientToServer = 1,
}

/// Why a sealed frame could not be produced or accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The sealed blob is shorter than the `direction+counter` prefix.
    Truncated,
    /// The `direction` byte was not a known [`Direction`].
    BadDirection(u8),
    /// AEAD open failed — wrong key, or the blob was tampered with
    /// (indistinguishable by design).
    Decrypt,
    /// The frame's counter was not strictly greater than the last accepted
    /// one on its direction — a replay or reorder. Fail-closed: never
    /// applied.
    Replay {
        /// The counter the frame carried.
        got: u64,
        /// The highest counter already accepted on that direction.
        last: u64,
    },
    /// The decrypted plaintext was not a valid frame.
    Frame(FrameError),
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("sealed frame shorter than its prefix"),
            Self::BadDirection(b) => write!(f, "unknown transport direction {b}"),
            Self::Decrypt => f.write_str("sealed frame failed to open (wrong key or tampered)"),
            Self::Replay { got, last } => {
                write!(f, "replayed/reordered frame: counter {got} <= last {last}")
            }
            Self::Frame(e) => write!(f, "decrypted plaintext is not a valid frame: {e}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// A sealed session transport over one direction of the stream.
///
/// Holds the 32-byte session key (derived once, see
/// [`crate::session::Session`]), the stream `direction` this half owns, its
/// own send counter, and the highest counter it has accepted on receive (for
/// replay rejection). Construct one per direction per endpoint.
pub struct SealedTransport {
    key: [u8; 32],
    direction: Direction,
    send_counter: u64,
    last_recv_counter: Option<u64>,
}

impl SealedTransport {
    /// A transport for `direction`, keyed by the session's derived key.
    #[must_use]
    pub fn new(key: [u8; 32], direction: Direction) -> Self {
        Self {
            key,
            direction,
            send_counter: 0,
            last_recv_counter: None,
        }
    }

    /// Build the 24-byte XChaCha nonce for `(direction, counter)`:
    /// `[direction:1][counter:8 LE][zero:15]`.
    fn nonce(direction: Direction, counter: u64) -> [u8; NONCE_LEN] {
        let mut n = [0u8; NONCE_LEN];
        n[0] = direction as u8;
        n[1..9].copy_from_slice(&counter.to_le_bytes());
        n
    }

    /// Seal one [`Frame`] for transmission. Advances this half's send counter;
    /// the returned bytes are `[direction][counter][ciphertext‖tag]`.
    ///
    /// The frame's own `to_le_bytes` is the plaintext — nothing serializes to
    /// wrap it (charter T3). `FRAME_VERSION` is bound as AEAD associated data,
    /// so a version mismatch on the wire fails the open.
    ///
    /// # Panics
    ///
    /// Never in practice: `aead::seal_with_key` only fails on an internal
    /// AEAD error, which cannot occur for a valid 32-byte key and 24-byte
    /// nonce; the `expect` documents that invariant.
    #[must_use]
    pub fn seal_frame(&mut self, frame: &Frame) -> Vec<u8> {
        let counter = self.send_counter;
        self.send_counter += 1;
        let nonce = Self::nonce(self.direction, counter);
        let plaintext = frame.to_le_bytes();
        let aad = [a2ui_core::FRAME_VERSION];
        let ciphertext = aead::seal_with_key(&self.key, &nonce, &aad, &plaintext)
            .expect("XChaCha20-Poly1305 seal cannot fail for a valid key+nonce");

        let mut out = Vec::with_capacity(SEAL_PREFIX + ciphertext.len());
        out.push(self.direction as u8);
        out.extend_from_slice(&counter.to_le_bytes());
        out.extend_from_slice(&ciphertext);
        out
    }

    /// Open one sealed frame, enforcing strictly-monotonic counters (replay /
    /// reorder rejection) on the direction this transport receives.
    ///
    /// # Errors
    ///
    /// See [`TransportError`] — every arm is a refusal (truncation, bad
    /// direction, AEAD failure, replay, or an invalid inner frame). On success
    /// the accepted counter becomes the new watermark.
    pub fn open_frame(&mut self, sealed: &[u8]) -> Result<Frame, TransportError> {
        if sealed.len() < SEAL_PREFIX + TAG_LEN {
            return Err(TransportError::Truncated);
        }
        let dir_byte = sealed[0];
        let direction = match dir_byte {
            0 => Direction::ServerToClient,
            1 => Direction::ClientToServer,
            other => return Err(TransportError::BadDirection(other)),
        };
        let mut ctr = [0u8; 8];
        ctr.copy_from_slice(&sealed[1..9]);
        let counter = u64::from_le_bytes(ctr);

        // Replay / reorder rejection — BEFORE spending the AEAD open, so a
        // flood of stale counters is cheap to shed. (The AEAD tag still
        // authenticates the counter via the nonce, so a forged counter can't
        // pass open anyway; this is defense-in-depth + ordering.)
        if let Some(last) = self.last_recv_counter
            && counter <= last
        {
            return Err(TransportError::Replay { got: counter, last });
        }

        let nonce = Self::nonce(direction, counter);
        let aad = [a2ui_core::FRAME_VERSION];
        let plaintext = aead::open_with_key(&self.key, &nonce, &aad, &sealed[SEAL_PREFIX..])
            .map_err(|_| TransportError::Decrypt)?;
        let frame = Frame::from_le_bytes(&plaintext).map_err(TransportError::Frame)?;
        self.last_recv_counter = Some(counter);
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_core::{ActionInvoke, NodeDelta};

    fn key() -> [u8; 32] {
        [0x5Au8; 32]
    }

    fn node_frame() -> Frame {
        Frame::NodeDelta(NodeDelta {
            key: [9; 16],
            mask_words: vec![0b1011, 1 << 5],
            values: vec![1, 2, 3, 4],
        })
    }

    #[test]
    fn seal_open_round_trips_a_frame() {
        let mut tx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut rx = SealedTransport::new(key(), Direction::ServerToClient);
        let f = node_frame();
        let sealed = tx.seal_frame(&f);
        // The plaintext frame is NOT visible in the sealed bytes.
        assert!(
            !sealed.windows(4).any(|w| w == [1, 2, 3, 4]),
            "values leaked"
        );
        let back = rx.open_frame(&sealed).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn wrong_key_fails_to_open() {
        let mut tx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut rx = SealedTransport::new([0xFFu8; 32], Direction::ServerToClient);
        let sealed = tx.seal_frame(&node_frame());
        assert_eq!(rx.open_frame(&sealed), Err(TransportError::Decrypt));
    }

    #[test]
    fn tamper_fails_to_open() {
        let mut tx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut rx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut sealed = tx.seal_frame(&node_frame());
        let n = sealed.len();
        sealed[n - 1] ^= 0x01; // flip a ciphertext/tag bit
        assert_eq!(rx.open_frame(&sealed), Err(TransportError::Decrypt));
    }

    #[test]
    fn replay_is_rejected() {
        let mut tx = SealedTransport::new(key(), Direction::ClientToServer);
        let mut rx = SealedTransport::new(key(), Direction::ClientToServer);
        let f = Frame::ActionInvoke(ActionInvoke {
            key: [1; 16],
            action_ordinal: 3,
            args: vec![],
        });
        let sealed = tx.seal_frame(&f);
        assert!(rx.open_frame(&sealed).is_ok());
        // Re-delivering the SAME sealed bytes must be refused (counter <= last).
        assert_eq!(
            rx.open_frame(&sealed),
            Err(TransportError::Replay { got: 0, last: 0 })
        );
    }

    #[test]
    fn reorder_is_rejected() {
        let mut tx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut rx = SealedTransport::new(key(), Direction::ServerToClient);
        let s0 = tx.seal_frame(&node_frame());
        let s1 = tx.seal_frame(&node_frame());
        // Accept #1 first (counter 1); then #0 (counter 0) must be refused.
        assert!(rx.open_frame(&s1).is_ok());
        assert_eq!(
            rx.open_frame(&s0),
            Err(TransportError::Replay { got: 0, last: 1 })
        );
    }

    #[test]
    fn truncated_is_refused() {
        let mut rx = SealedTransport::new(key(), Direction::ServerToClient);
        assert_eq!(rx.open_frame(&[0, 1, 2]), Err(TransportError::Truncated));
    }
}
