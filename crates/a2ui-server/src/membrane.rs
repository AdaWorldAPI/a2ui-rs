//! `membrane` — the **edge-only JSON adapter** (charter C1.2 / trap T3).
//!
//! The Firewall is absolute on the hot path: server → wasm frames are LE bytes
//! end-to-end, sealed by [`SealedTransport`](crate::transport::SealedTransport)
//! — nothing serializes to transport them. This module is the ONE sanctioned
//! exception the charter names: *"Protobuf/JSON exist only at a browser
//! membrane if a non-wasm client needs them."* It is compiled ONLY under the
//! `json` feature, so a default build cannot even reach it — the hot path stays
//! serde-free by construction.
//!
//! # When to use it (and when not)
//!
//! - **Use** at a service EDGE that must talk to a non-wasm client (a plain
//!   browser `fetch`, a debugging console, an interop bridge). The frame is
//!   decoded/encoded here, at the membrane, once.
//! - **Do NOT use** on the server → wasm stream. That is the hot path; it is
//!   [`Frame::to_le_bytes`](a2ui_core::Frame::to_le_bytes) +
//!   [`SealedTransport`](crate::transport::SealedTransport), and a JSON struct
//!   in that path is exactly the T3 breach the Firewall forbids.
//!
//! The JSON carries the SAME closed frame vocabulary as the wire — it is a
//! re-encoding of the identical [`Frame`], not a second surface (so a frame
//! survives a `LE → Frame → JSON → Frame → LE` round trip byte-for-byte).

use a2ui_core::Frame;

/// Encode a frame as compact JSON — MEMBRANE ONLY (a non-wasm edge client).
///
/// # Errors
///
/// [`serde_json::Error`] if serialization fails (it never should for a
/// well-formed [`Frame`]).
pub fn to_json(frame: &Frame) -> Result<String, serde_json::Error> {
    serde_json::to_string(frame)
}

/// Encode a frame as pretty (indented) JSON — for a human/debug console.
///
/// # Errors
///
/// [`serde_json::Error`] if serialization fails.
pub fn to_json_pretty(frame: &Frame) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(frame)
}

/// Decode a frame from JSON at the membrane.
///
/// This is the edge's inbound gate; the frame it yields then travels the hot
/// path as LE bytes. Note the JSON path does NOT re-run the LE load gate's
/// structural refusals ([`Frame::from_le_bytes`](a2ui_core::Frame::from_le_bytes))
/// — serde enforces the shape instead — so a membrane that forwards to the LE
/// path gets both checks; a membrane that consumes the frame directly relies on
/// serde's typing alone.
///
/// # Errors
///
/// [`serde_json::Error`] if the JSON is malformed or does not match the closed
/// [`Frame`] shape.
pub fn from_json(s: &str) -> Result<Frame, serde_json::Error> {
    serde_json::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_core::{ActionInvoke, NodeDelta};

    fn node() -> Frame {
        Frame::NodeDelta(NodeDelta {
            key: [7; 16],
            mask_words: vec![0b1011, 1 << 5],
            values: vec![10, 20, 30, 40],
        })
    }

    #[test]
    fn json_round_trips_a_frame() {
        let f = node();
        let json = to_json(&f).unwrap();
        // Human-readable field names present (the membrane's whole point).
        assert!(json.contains("NodeDelta"));
        assert!(json.contains("mask_words") && json.contains("values"));
        assert_eq!(from_json(&json).unwrap(), f);
    }

    #[test]
    fn membrane_and_wire_carry_the_same_frame() {
        // The charter invariant: JSON is a re-encoding of the identical frame,
        // so LE → Frame → JSON → Frame → LE is byte-identical.
        let f = node();
        let le = f.to_le_bytes();
        let via_wire = Frame::from_le_bytes(&le).unwrap();
        let json = to_json(&via_wire).unwrap();
        let via_membrane = from_json(&json).unwrap();
        assert_eq!(via_membrane, f);
        assert_eq!(
            via_membrane.to_le_bytes(),
            le,
            "membrane preserves the LE wire form"
        );
    }

    #[test]
    fn action_invoke_survives_the_membrane() {
        let f = Frame::ActionInvoke(ActionInvoke {
            key: [1; 16],
            action_ordinal: 9,
            args: vec![1, 2, 3],
        });
        assert_eq!(from_json(&to_json(&f).unwrap()).unwrap(), f);
        // pretty form also round-trips.
        assert_eq!(from_json(&to_json_pretty(&f).unwrap()).unwrap(), f);
    }

    #[test]
    fn malformed_json_is_refused() {
        assert!(from_json("{not a frame}").is_err());
        assert!(from_json("{\"NodeDelta\":{}}").is_err()); // missing fields
    }
}
