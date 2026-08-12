//! Dimensions as `u8:u8` rails on the 12-byte content-blind facet.
//!
//! The V3 facet register is carved `6×(u8:u8)` (`lance-graph`
//! `le-contract.md` §3), and the canon is emphatic that a rail is **two
//! separate bytes, never widened to u16**. So a dimension is not "a u16 in
//! centi-millimetres" — it is a pair, read as *(whole mm, hundredths of mm)*.
//!
//! That lands on a range of **0.00 – 255.99 mm at 0.01 mm resolution**, which
//! is not a compromise but a good fit twice over: 255 mm covers the desktop
//! printer envelope (a Prusa MK4 bed is 250 × 210 × 220), and 0.01 mm is finer
//! than any FDM machine's positional accuracy, so the encoding is never the
//! limiting factor on a printable part.
//!
//! Six rails per node means **six parameters per solid** — enough for a box
//! plus a bore, a cylinder plus a fillet, and so on. A feature needing more is
//! a feature needing a child node, which is what the `EdgeBlock` is for.

/// The number of content-blind facet bytes a node carries (V3 le-contract §3).
pub const FACET_LEN: usize = 12;

/// Rails per facet: `6 × (u8:u8) = 12`.
pub const RAIL_COUNT: usize = FACET_LEN / 2;

/// The largest whole-millimetre value a rail can carry.
pub const MAX_MM: u8 = u8::MAX;

/// One dimension: `(whole mm, hundredths of mm)`.
///
/// Deliberately NOT a `u16`. Keeping the two bytes separate is what makes this
/// the canon's rail rather than a widened integer that merely happens to fit in
/// the same space — and it is why the two halves can be addressed as two
/// distinct mask positions on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rail {
    /// Whole millimetres.
    pub mm: u8,
    /// Hundredths of a millimetre. Values above 99 are not representable as a
    /// fraction; see [`Rail::from_bytes`] for how they are handled.
    pub cmm: u8,
}

impl Rail {
    /// A rail from millimetres and hundredths.
    ///
    /// `cmm` is taken modulo 100 with the overflow carried into `mm`, so
    /// `Rail::new(1, 250)` is 3.50 mm rather than a nonsense 1.250. The carry
    /// saturates at [`MAX_MM`].
    #[must_use]
    pub fn new(mm: u8, cmm: u8) -> Self {
        let carry = cmm / 100;
        Self {
            mm: mm.saturating_add(carry),
            cmm: cmm % 100,
        }
    }

    /// Read a rail from two wire bytes.
    ///
    /// The wire is a dumb byte register — a sender may put any `u8` in the
    /// fraction lane. Normalising here (rather than rejecting) keeps the
    /// decoder total: every 2-byte pair denotes SOME length, so a malformed
    /// frame cannot produce an unrepresentable dimension. The alternative —
    /// refusing — would put a validation policy in a content-blind register,
    /// which is exactly what content-blind means it must not hold.
    #[must_use]
    pub fn from_bytes(mm: u8, cmm: u8) -> Self {
        Self::new(mm, cmm)
    }

    /// The two wire bytes, in mask-position order (whole, then fraction).
    #[must_use]
    pub fn to_bytes(self) -> [u8; 2] {
        [self.mm, self.cmm]
    }

    /// The dimension in millimetres.
    #[must_use]
    pub fn mm_f32(self) -> f32 {
        f32::from(self.mm) + f32::from(self.cmm) / 100.0
    }

    /// The smallest representable step, in millimetres.
    ///
    /// Named rather than written as a literal `0.01` at call sites, because a
    /// test that asserts "one step changes the geometry" must step by exactly
    /// the encoding's own quantum or it proves nothing about the encoding.
    pub const STEP_MM: f32 = 0.01;
}

/// The six rails of one node's facet, in mask-position order.
///
/// Rail `i` occupies mask positions `2i` and `2i + 1` — the same
/// `rail*2` / `rail*2 + 1` convention `a2ui-paint`'s `Skin::Tile` uses to read
/// a geographic coordinate out of a surface. One convention, two domains: there
/// is no geometry-specific addressing rule here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Facet {
    rails: [Rail; RAIL_COUNT],
}

impl Facet {
    /// Decode all six rails from a node's 12 facet bytes.
    #[must_use]
    pub fn from_facet_bytes(bytes: &[u8; FACET_LEN]) -> Self {
        let mut rails = [Rail::default(); RAIL_COUNT];
        for (i, rail) in rails.iter_mut().enumerate() {
            *rail = Rail::from_bytes(bytes[i * 2], bytes[i * 2 + 1]);
        }
        Self { rails }
    }

    /// Encode the six rails back to 12 facet bytes.
    #[must_use]
    pub fn to_facet_bytes(self) -> [u8; FACET_LEN] {
        let mut out = [0u8; FACET_LEN];
        for (i, rail) in self.rails.iter().enumerate() {
            let [a, b] = rail.to_bytes();
            out[i * 2] = a;
            out[i * 2 + 1] = b;
        }
        out
    }

    /// Rail `i`, or a zero rail if `i` is past the register.
    #[must_use]
    pub fn rail(&self, i: usize) -> Rail {
        self.rails.get(i).copied().unwrap_or_default()
    }

    /// Rail `i` in millimetres.
    #[must_use]
    pub fn mm(&self, i: usize) -> f32 {
        self.rail(i).mm_f32()
    }

    /// Replace rail `i`. Out-of-range indices are ignored — the register has a
    /// fixed width and growing it is a layout change, not a setter.
    pub fn set_rail(&mut self, i: usize, rail: Rail) {
        if let Some(slot) = self.rails.get_mut(i) {
            *slot = rail;
        }
    }

    /// Build a facet from millimetre values, one per rail.
    ///
    /// Values are clamped into the representable range rather than wrapping: a
    /// 300 mm request becoming 44.00 mm silently would be a far worse failure
    /// than it becoming 255.99 mm visibly.
    #[must_use]
    pub fn from_mm(values: [f32; RAIL_COUNT]) -> Self {
        let mut rails = [Rail::default(); RAIL_COUNT];
        for (rail, v) in rails.iter_mut().zip(values) {
            let clamped = v.clamp(0.0, f32::from(MAX_MM) + 0.99);
            let mm = clamped.trunc();
            // `.round()` on the remainder, not `.trunc()`: 4.999 mm should read
            // as 5.00, not 4.99. The half-step bias is the difference between a
            // hole that fits and one that does not.
            let cmm = ((clamped - mm) * 100.0).round();
            *rail = Rail::new(mm as u8, cmm as u8);
        }
        Self { rails }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rail_is_two_bytes_and_round_trips() {
        let r = Rail::new(20, 50);
        assert_eq!(r.to_bytes(), [20, 50]);
        assert!((r.mm_f32() - 20.5).abs() < 1e-6);
        assert_eq!(Rail::from_bytes(20, 50), r);
    }

    /// The fraction lane is a dumb byte; decoding must stay total.
    #[test]
    fn an_over_full_fraction_carries_instead_of_lying() {
        let r = Rail::from_bytes(1, 250);
        assert_eq!((r.mm, r.cmm), (3, 50), "1 mm + 250/100 mm = 3.50 mm");
        assert!((r.mm_f32() - 3.5).abs() < 1e-6);
    }

    #[test]
    fn the_carry_saturates_rather_than_wrapping() {
        let r = Rail::from_bytes(255, 200);
        assert_eq!(
            r.mm, MAX_MM,
            "carry must clamp, never wrap to a small value"
        );
    }

    #[test]
    fn a_facet_is_six_rails_and_round_trips_through_bytes() {
        let f = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let bytes = f.to_facet_bytes();
        assert_eq!(bytes.len(), FACET_LEN);
        assert_eq!(Facet::from_facet_bytes(&bytes), f);
        assert!((f.mm(0) - 20.0).abs() < 1e-6);
        assert!((f.mm(3) - 5.0).abs() < 1e-6);
    }

    /// The rounding half-step is load-bearing: truncating would make a 5 mm
    /// bore encode as 4.99 mm, and a bore that is 0.01 mm undersize is a part
    /// that does not assemble.
    #[test]
    fn millimetre_conversion_rounds_rather_than_truncates() {
        let f = Facet::from_mm([4.999, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(
            (f.mm(0) - 5.0).abs() < 1e-6,
            "4.999 must encode as 5.00, got {}",
            f.mm(0)
        );
    }

    #[test]
    fn out_of_range_millimetres_clamp_visibly_rather_than_wrapping() {
        let f = Facet::from_mm([300.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(
            f.mm(0) > 255.0,
            "300 mm must clamp high, not wrap low — got {}",
            f.mm(0)
        );
    }
}
