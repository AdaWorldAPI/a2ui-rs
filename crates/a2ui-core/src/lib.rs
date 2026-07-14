//! `a2ui-core` — the render-target core of **a2ui-rs**: the OGAR target for
//! desktop addressing/rendering via ClassView (charter:
//! `AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`, merged
//! OGAR #204 + #205).
//!
//! **Don't push pixels — address the screen.** Down the wire go
//! [`NodeDelta`] frames (16-byte GUID key, wide mask delta, ClassView-carved
//! LE values); up go [`ActionInvoke`] frames (behavior by address, never
//! inline). The frames themselves live upstream in `ogar-a2ui-frame` (W1) —
//! this crate re-exports them and will grow the W2 service shape
//! (RenderStream / ActionStream / codebook sync, the shape kept from the
//! A2UI fork's `hamming.proto` with its payload replaced by the V3 facet).
//!
//! Boundary (charter traps): the surface never carries behavior (T2), the
//! hot path never serializes (T3 — LE end-to-end; the wasm fieldview client
//! ingests these bytes zero-copy), and widget skins are ClassView templates,
//! never a second vocabulary (T1).

#![forbid(unsafe_code)]

pub use ogar_a2ui_frame::{
    ActionInvoke, FRAME_VERSION, Frame, FrameError, FrameKind, HEADER_LEN, NodeDelta,
    mask_positions,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The seed's drift fuse: the upstream frames roundtrip through this
    /// consumer — proves the git-dep wiring end-to-end (and pins the wire
    /// version this core was built against).
    #[test]
    fn upstream_frames_roundtrip_through_the_consumer() {
        assert_eq!(FRAME_VERSION, 1, "wire version this core targets");
        let f = Frame::NodeDelta(NodeDelta {
            key: [7; 16],
            mask_words: vec![1 << 40, 1], // positions 40 and 64 — wide-native
            values: vec![1, 2, 3],
        });
        let back = Frame::from_le_bytes(&f.to_le_bytes()).expect("roundtrip");
        assert_eq!(f, back);
        let pos: Vec<u32> = match &back {
            Frame::NodeDelta(d) => mask_positions(&d.mask_words).collect(),
            Frame::ActionInvoke(_) => unreachable!(),
        };
        assert_eq!(pos, vec![40, 64]);
    }
}
