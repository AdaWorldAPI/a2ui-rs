//! The solid vocabulary and its signed-distance evaluation.
//!
//! # Why a distance field rather than a mesh-boolean kernel
//!
//! The obvious way to build CSG is to mesh each primitive and run boolean
//! operations on the triangles. That is also where CAD kernels earn their
//! reputation: mesh booleans are a well-known source of degenerate,
//! near-coplanar and self-intersecting failure cases, and getting them robust
//! is a multi-year project, not a POC.
//!
//! A signed-distance formulation removes that entire class of bug rather than
//! managing it. Union is `min`, intersection is `max`, difference is
//! `max(a, -b)` — arithmetic on a scalar field. There is no geometry to get
//! wrong because at this stage there is no geometry at all, only a function
//! `R³ → R` whose zero level set IS the surface. The triangles are produced
//! later, in one place ([`crate::mesh`]), by a method that is watertight by
//! construction.
//!
//! **The one honest caveat**, stated because it matters downstream: `min`/`max`
//! composition gives an exact *sign* everywhere but only a bound on the
//! *magnitude* near edges and fillets (the field becomes non-Euclidean — it
//! under-estimates distance in concave corners). The zero crossing — the only
//! thing meshing reads — is exact. Anything that wants true distance (shelling,
//! offsetting, sphere-tracing with large steps) must not assume this field is
//! metric.
//!
//! # Why these primitives
//!
//! The set is deliberately closed and small: it is the OpenSCAD/CSG core, which
//! is enough to express the overwhelming majority of printable mechanical parts
//! and is a vocabulary that does not need to be discovered empirically. It is
//! shaped to become an OGAR class vocabulary — each variant is a `classid` and
//! its parameters are facet rails — but this crate mints nothing and depends on
//! no OGAR type, so promoting it upstream later is a move, not a rewrite.

use crate::rail::Facet;

/// A point in model space, millimetres.
pub type Point = [f32; 3];

/// The closed solid vocabulary.
///
/// Closed on purpose (charter T1, one level down): a new shape is a new
/// *composition* of these, or a deliberate addition to the vocabulary with a
/// classid behind it — never an escape hatch that lets callers smuggle in
/// arbitrary geometry.
#[derive(Debug, Clone, PartialEq)]
pub enum Solid {
    /// Axis-aligned box centred on the origin, full extents in mm.
    Box {
        /// X extent.
        w: f32,
        /// Y extent.
        d: f32,
        /// Z extent.
        h: f32,
    },
    /// Z-axis cylinder centred on the origin.
    Cylinder {
        /// Radius.
        r: f32,
        /// Height.
        h: f32,
    },
    /// Sphere centred on the origin.
    Sphere {
        /// Radius.
        r: f32,
    },
    /// Everything in either operand.
    Union(Box<Solid>, Box<Solid>),
    /// The first operand with the second removed.
    Difference(Box<Solid>, Box<Solid>),
    /// Only what is in both operands.
    Intersection(Box<Solid>, Box<Solid>),
    /// Rigid translation of the inner solid.
    Translate {
        /// Offset in mm.
        by: Point,
        /// The solid being moved.
        inner: Box<Solid>,
    },
}

impl Solid {
    /// Signed distance from `p` to the surface: negative inside, positive
    /// outside, zero on it.
    #[must_use]
    pub fn distance(&self, p: Point) -> f32 {
        match self {
            Self::Box { w, d, h } => {
                // Exact box SDF: the outside term handles faces/edges/corners
                // uniformly, the inside term is the (negative) distance to the
                // nearest face.
                let q = [
                    p[0].abs() - w * 0.5,
                    p[1].abs() - d * 0.5,
                    p[2].abs() - h * 0.5,
                ];
                let outside = length([q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)]);
                let inside = q[0].max(q[1]).max(q[2]).min(0.0);
                outside + inside
            }
            Self::Cylinder { r, h } => {
                let radial = (p[0] * p[0] + p[1] * p[1]).sqrt() - r;
                let axial = p[2].abs() - h * 0.5;
                let outside = length2([radial.max(0.0), axial.max(0.0)]);
                let inside = radial.max(axial).min(0.0);
                outside + inside
            }
            Self::Sphere { r } => length(p) - r,
            Self::Union(a, b) => a.distance(p).min(b.distance(p)),
            // Difference is `max(a, -b)`: inside A *and* outside B. Negating a
            // signed distance is exactly "the complement of that solid", which
            // is why subtraction needs no special case.
            Self::Difference(a, b) => a.distance(p).max(-b.distance(p)),
            Self::Intersection(a, b) => a.distance(p).max(b.distance(p)),
            Self::Translate { by, inner } => {
                inner.distance([p[0] - by[0], p[1] - by[1], p[2] - by[2]])
            }
        }
    }

    /// A conservative axis-aligned bound, in mm.
    ///
    /// Conservative on purpose, and asymmetrically so: for `Difference` it
    /// returns the *positive* operand's bound (removing material can only
    /// shrink the result) but for `Intersection` it returns the union of both
    /// rather than the tighter intersection-of-bounds. A bound that is too
    /// large costs meshing time; a bound that is too small silently truncates
    /// the model, and a part with a face missing is worse than a slow one.
    #[must_use]
    pub fn bounds(&self) -> (Point, Point) {
        match self {
            Self::Box { w, d, h } => {
                let half = [w * 0.5, d * 0.5, h * 0.5];
                ([-half[0], -half[1], -half[2]], half)
            }
            Self::Cylinder { r, h } => ([-r, -r, -h * 0.5], [*r, *r, h * 0.5]),
            Self::Sphere { r } => ([-r, -r, -r], [*r, *r, *r]),
            Self::Difference(a, _) => a.bounds(),
            Self::Union(a, b) | Self::Intersection(a, b) => {
                let (amin, amax) = a.bounds();
                let (bmin, bmax) = b.bounds();
                (
                    [
                        amin[0].min(bmin[0]),
                        amin[1].min(bmin[1]),
                        amin[2].min(bmin[2]),
                    ],
                    [
                        amax[0].max(bmax[0]),
                        amax[1].max(bmax[1]),
                        amax[2].max(bmax[2]),
                    ],
                )
            }
            Self::Translate { by, inner } => {
                let (lo, hi) = inner.bounds();
                (
                    [lo[0] + by[0], lo[1] + by[1], lo[2] + by[2]],
                    [hi[0] + by[0], hi[1] + by[1], hi[2] + by[2]],
                )
            }
        }
    }
}

/// The POC's demonstration part: a plate with a centred through-bore.
///
/// Chosen because its volume has a closed form — `w·d·h − π·r²·h` — so the
/// mesher can be checked against arithmetic rather than against a previous run
/// of itself. A golden-mesh comparison would only prove the code still does
/// what it did last time, including if that was wrong.
///
/// Rails, in mask-position order: `0` width, `1` depth, `2` height,
/// `3` bore radius. Rails 4 and 5 are unused and read zero — reserved, not
/// reclaimed, exactly as the canon's zero-fallback ladder specifies.
#[must_use]
pub fn plate_with_bore(facet: &Facet) -> Solid {
    let (w, d, h, r) = (facet.mm(0), facet.mm(1), facet.mm(2), facet.mm(3));
    let plate = Solid::Box { w, d, h };
    if r <= 0.0 {
        return plate;
    }
    Solid::Difference(
        Box::new(plate),
        // The bore is made deliberately taller than the plate. A cylinder of
        // exactly `h` would leave the two solids' faces coplanar at both ends,
        // where the difference's zero set is degenerate and the mesher has to
        // resolve a tie it cannot resolve stably. Over-length cutters are the
        // standard CSG idiom for precisely this reason.
        Box::new(Solid::Cylinder { r, h: h + 2.0 }),
    )
}

/// The analytic volume of [`plate_with_bore`], mm³ — the mesher's oracle.
///
/// Returns `None` when the bore is not strictly inside the plate footprint,
/// because then the closed form above stops being the answer (the cylinder
/// would break the side wall and the removed volume is no longer a full disc).
/// Returning `None` rather than a wrong number is what keeps this usable as a
/// test oracle.
#[must_use]
pub fn plate_with_bore_volume(facet: &Facet) -> Option<f32> {
    let (w, d, h, r) = (facet.mm(0), facet.mm(1), facet.mm(2), facet.mm(3));
    if w <= 0.0 || d <= 0.0 || h <= 0.0 {
        return None;
    }
    if r <= 0.0 {
        return Some(w * d * h);
    }
    if r * 2.0 >= w.min(d) {
        return None;
    }
    Some(w * d * h - std::f32::consts::PI * r * r * h)
}

fn length(v: Point) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn length2(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_reports_inside_outside_and_surface() {
        let b = Solid::Box {
            w: 20.0,
            d: 20.0,
            h: 10.0,
        };
        assert!(b.distance([0.0, 0.0, 0.0]) < 0.0, "centre is inside");
        assert!(b.distance([50.0, 0.0, 0.0]) > 0.0, "far away is outside");
        assert!(
            b.distance([10.0, 0.0, 0.0]).abs() < 1e-5,
            "the +x face is the surface"
        );
        // The inside term must report distance to the NEAREST face (5 mm in z),
        // not to the furthest — a bug that would be invisible on a cube.
        assert!(
            (b.distance([0.0, 0.0, 0.0]) + 5.0).abs() < 1e-5,
            "centre of a 20×20×10 plate is 5 mm from the nearest face"
        );
    }

    #[test]
    fn difference_removes_material_where_the_cutter_is() {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let s = plate_with_bore(&facet);
        assert!(
            s.distance([0.0, 0.0, 0.0]) > 0.0,
            "the bore axis must be OUTSIDE the solid — that is what a hole is"
        );
        assert!(
            s.distance([8.0, 0.0, 0.0]) < 0.0,
            "material remains between the bore wall and the plate edge"
        );
    }

    /// A zero-radius bore must yield the plain plate, not a degenerate
    /// difference — the branch exists so the surface stays sane at the bottom
    /// of the parameter range, which is exactly where an editor will drag it.
    #[test]
    fn a_zero_radius_bore_is_just_the_plate() {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            plate_with_bore(&facet),
            Solid::Box {
                w: 20.0,
                d: 20.0,
                h: 10.0
            }
        );
    }

    #[test]
    fn the_volume_oracle_refuses_when_its_closed_form_stops_applying() {
        // A bore wider than the plate is not "a plate with a big hole" — the
        // closed form would over-subtract. Refusing beats answering.
        let blown = Facet::from_mm([20.0, 20.0, 10.0, 12.0, 0.0, 0.0]);
        assert!(plate_with_bore_volume(&blown).is_none());

        let ok = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let v = plate_with_bore_volume(&ok).expect("a 5 mm bore fits a 20 mm plate");
        let expect = 20.0 * 20.0 * 10.0 - std::f32::consts::PI * 25.0 * 10.0;
        assert!((v - expect).abs() < 1e-2, "got {v}, want {expect}");
    }

    #[test]
    fn bounds_contain_the_surface() {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let (lo, hi) = plate_with_bore(&facet).bounds();
        assert!(lo[0] <= -10.0 && hi[0] >= 10.0);
        assert!(lo[2] <= -5.0 && hi[2] >= 5.0);
    }
}
