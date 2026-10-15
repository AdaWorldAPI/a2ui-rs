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
pub use ogar_encryption::stream::Direction;
use ogar_encryption::stream::{SealedStream, StreamError};

/// Why a sealed frame could not be produced or accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The sealed blob is shorter than the `direction+counter` prefix.
    Truncated,
    /// The `direction` byte was not a known [`Direction`].
    BadDirection(u8),
    /// The `direction` byte names the other half of the session: a frame
    /// offered to the transport that does not receive it, for example a
    /// sender's own frame reflected back at it. Refused, never applied.
    WrongDirection {
        /// The direction this transport receives.
        expected: Direction,
        /// The direction the frame carries.
        got: Direction,
    },
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
    /// The send counter is exhausted; the session must be re-keyed.
    CounterExhausted,
    /// The decrypted plaintext was not a valid frame.
    Frame(FrameError),
}

impl From<StreamError> for TransportError {
    fn from(e: StreamError) -> Self {
        match e {
            StreamError::Truncated => Self::Truncated,
            StreamError::BadDirection(b) => Self::BadDirection(b),
            StreamError::WrongDirection { expected, got } => Self::WrongDirection { expected, got },
            StreamError::Replay { got, last } => Self::Replay { got, last },
            StreamError::Decrypt => Self::Decrypt,
            StreamError::CounterExhausted => Self::CounterExhausted,
        }
    }
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("sealed frame shorter than its prefix"),
            Self::BadDirection(b) => write!(f, "unknown transport direction {b}"),
            Self::WrongDirection { expected, got } => {
                write!(f, "frame for {got:?} offered to the {expected:?} receiver")
            }
            Self::Decrypt => f.write_str("sealed frame failed to open (wrong key or tampered)"),
            Self::Replay { got, last } => {
                write!(f, "replayed/reordered frame: counter {got} <= last {last}")
            }
            Self::CounterExhausted => f.write_str("send counter exhausted; re-key the session"),
            Self::Frame(e) => write!(f, "decrypted plaintext is not a valid frame: {e}"),
        }
    }
}

impl std::error::Error for TransportError {}

fn other(d: Direction) -> Direction {
    match d {
        Direction::ServerToClient => Direction::ClientToServer,
        Direction::ClientToServer => Direction::ServerToClient,
    }
}

/// A sealed session transport over one direction of the stream.
///
/// A thin frame layer over [`ogar_encryption::stream::SealedStream`]: it seals
/// frames onto `direction` and opens only frames that carry `direction`.
/// Construct one per direction per endpoint, all with the session key (derived
/// once, see [`crate::session::Session`]).
pub struct SealedTransport {
    sender: SealedStream,
    receiver: SealedStream,
}

impl SealedTransport {
    /// A transport for `direction`, keyed by the session's derived key.
    #[must_use]
    pub fn new(key: [u8; 32], direction: Direction) -> Self {
        Self {
            sender: SealedStream::new(key, direction),
            // The receiving half of `direction` is the stream that sends the
            // other way: it accepts only frames whose direction byte is
            // `direction`.
            receiver: SealedStream::new(key, other(direction)),
        }
    }

    /// Seal one [`Frame`] for transmission. Advances this transport's send
    /// counter; the returned bytes are `[direction][counter][ciphertext‖tag]`.
    ///
    /// The frame's own `to_le_bytes` is the plaintext — nothing serializes to
    /// wrap it (charter T3). `FRAME_VERSION` is bound as AEAD associated data,
    /// so a version mismatch on the wire fails the open.
    ///
    /// # Errors
    ///
    /// [`TransportError::CounterExhausted`] after 2^64 frames, or
    /// [`TransportError::Decrypt`] if the AEAD reports an internal failure.
    pub fn seal_frame(&mut self, frame: &Frame) -> Result<Vec<u8>, TransportError> {
        let aad = [a2ui_core::FRAME_VERSION];
        Ok(self.sender.seal(&aad, &frame.to_le_bytes())?)
    }

    /// Open one sealed frame, enforcing strictly-monotonic counters (replay /
    /// reorder rejection) on the direction this transport receives.
    ///
    /// # Errors
    ///
    /// See [`TransportError`] — every arm is a refusal (truncation, wrong or
    /// unknown direction, AEAD failure, replay, or an invalid inner frame). On
    /// success the accepted counter becomes the new watermark.
    pub fn open_frame(&mut self, sealed: &[u8]) -> Result<Frame, TransportError> {
        let aad = [a2ui_core::FRAME_VERSION];
        let plaintext = self.receiver.open(&aad, sealed)?;
        Frame::from_le_bytes(&plaintext).map_err(TransportError::Frame)
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
        let sealed = tx.seal_frame(&f).unwrap();
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
        let sealed = tx.seal_frame(&node_frame()).unwrap();
        assert_eq!(rx.open_frame(&sealed), Err(TransportError::Decrypt));
    }

    #[test]
    fn tamper_fails_to_open() {
        let mut tx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut rx = SealedTransport::new(key(), Direction::ServerToClient);
        let mut sealed = tx.seal_frame(&node_frame()).unwrap();
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
        let sealed = tx.seal_frame(&f).unwrap();
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
        let s0 = tx.seal_frame(&node_frame()).unwrap();
        let s1 = tx.seal_frame(&node_frame()).unwrap();
        // Accept #1 first (counter 1); then #0 (counter 0) must be refused.
        assert!(rx.open_frame(&s1).is_ok());
        assert_eq!(
            rx.open_frame(&s0),
            Err(TransportError::Replay { got: 0, last: 1 })
        );
    }

    /// A server's own frame reflected back at its receiving half is refused.
    /// Both directions share the key, so only the direction check stops it.
    #[test]
    fn a_reflected_frame_is_refused() {
        let mut down = SealedTransport::new(key(), Direction::ServerToClient);
        let mut up = SealedTransport::new(key(), Direction::ClientToServer);
        let own = down.seal_frame(&node_frame()).unwrap();
        assert_eq!(
            up.open_frame(&own),
            Err(TransportError::WrongDirection {
                expected: Direction::ClientToServer,
                got: Direction::ServerToClient,
            })
        );
    }

    #[test]
    fn truncated_is_refused() {
        let mut rx = SealedTransport::new(key(), Direction::ServerToClient);
        assert_eq!(rx.open_frame(&[0, 1, 2]), Err(TransportError::Truncated));
    }
}
