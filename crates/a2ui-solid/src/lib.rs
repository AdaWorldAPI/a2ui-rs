//! `a2ui-solid` — parametric solids as **addressed objects**.
//!
//! # Why this exists
//!
//! The a2ui thesis is *don't push pixels — address the screen*. This crate is
//! that thesis one dimension up: **don't push meshes — address the solid.**
//!
//! A printable part here is not a file. It is a `classid` (which primitive or
//! operator) plus six `u8:u8` rails on the 12-byte content-blind facet (its
//! parameters). Changing a dimension is a `NodeDelta` carrying **two bytes**;
//! the mesh is derived, never transmitted. Where the paint tier answers a click
//! with an ordinal instead of a handler, this answers a design change with a
//! rail instead of an STL — the same move, on geometry.
//!
//! That is the "Citrix without pixels" claim made measurable: the wire cost of
//! a parameter edit is independent of how complicated the resulting solid is,
//! and the ratio between the two is something a deploy can print rather than
//! assert.
//!
//! # What is deliberately NOT here
//!
//! - **No a2ui dependency.** The public surface takes plain facet bytes and
//!   returns plain meshes. The frame wiring lives in the binary that consumes
//!   this. That is what makes both of this crate's likely futures cheap: the
//!   vocabulary promoting into OGAR as real classids, and the kernel moving to
//!   its own repo after the POC.
//! - **No geometry dependencies at all.** `csgrs` (the obvious candidate) is
//!   currently unbuildable — it requires the yanked `core2 0.4.0` — but even
//!   without that, an SDF formulation makes the mesh-boolean robustness problem
//!   not arise rather than be handled. See [`sdf`].
//! - **No STL *import*.** Reading a mesh back gives triangles, not parameters;
//!   a solid that arrived as STL cannot be edited by address, which is the only
//!   thing this crate is for. Import belongs in a consumer that wants an opaque
//!   fixture to position against.
//!
//! # Status
//!
//! POC. The vocabulary in [`sdf::Solid`] is shaped to become an OGAR class
//! vocabulary — one classid per variant, parameters as facet rails — but this
//! crate mints nothing and depends on no OGAR type. Promotion is a follow-up
//! with a real mint behind it, not a rename.
//!
//! # Example
//!
//! ```
//! use a2ui_solid::{rail::Facet, sdf, mesh, stl};
//!
//! // Six rails: width, depth, height, bore radius, and two reserved.
//! let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
//! let solid = sdf::plate_with_bore(&facet);
//!
//! let m = mesh::marching_tets(&solid, solid.bounds(), 1.0);
//! assert!(m.is_watertight());
//!
//! let bytes = stl::to_binary_stl(&m, "plate");
//! assert!(stl::length_matches_header(&bytes));
//!
//! // The whole part is addressed by 12 bytes; the STL is orders larger.
//! assert!(bytes.len() > facet.to_facet_bytes().len() * 100);
//! ```

#![forbid(unsafe_code)]

pub mod mesh;
pub mod rail;
pub mod sdf;
pub mod stl;

pub use mesh::{Mesh, marching_tets};
pub use rail::{FACET_LEN, Facet, RAIL_COUNT, Rail};
pub use sdf::{Solid, plate_with_bore, plate_with_bore_volume};
pub use stl::to_binary_stl;

/// How many bytes the wire spends on parameters versus how many the derived
/// mesh would cost, for one solid at one meshing resolution.
///
/// This is the crate's headline number and the reason it exists, so it is a
/// function rather than a paragraph: a claim that recomputes itself cannot go
/// stale the way a README figure does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireCost {
    /// Bytes of parameters — the facet register.
    pub parameter_bytes: usize,
    /// Bytes of binary STL for the derived mesh.
    pub mesh_bytes: usize,
    /// Triangles in that mesh.
    pub triangles: usize,
}

impl WireCost {
    /// Mesh bytes per parameter byte.
    #[must_use]
    pub fn ratio(&self) -> f32 {
        if self.parameter_bytes == 0 {
            return 0.0;
        }
        self.mesh_bytes as f32 / self.parameter_bytes as f32
    }
}

/// Measure [`WireCost`] for a facet at a meshing resolution.
#[must_use]
pub fn wire_cost(facet: &Facet, res: f32) -> WireCost {
    let solid = plate_with_bore(facet);
    let m = marching_tets(&solid, solid.bounds(), res);
    let stl = to_binary_stl(&m, "cost");
    WireCost {
        parameter_bytes: FACET_LEN,
        mesh_bytes: stl.len(),
        triangles: m.tri_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The addressing claim, as a number rather than a sentence.
    ///
    /// Two-sided on purpose. The lower bound is the claim itself — parameters
    /// must be dramatically cheaper than the mesh, or "address the solid" buys
    /// nothing. The upper bound guards the measurement: if `mesh_bytes` ever
    /// collapsed to near zero (an empty mesh from a broken SDF), the ratio
    /// would look *better* while the crate had stopped working.
    #[test]
    fn addressing_a_solid_costs_far_less_than_shipping_its_mesh() {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let c = wire_cost(&facet, 0.5);
        assert!(c.triangles > 1000, "suspiciously few triangles: {c:?}");
        assert!(
            c.ratio() > 1000.0,
            "parameters should be >1000x cheaper than the mesh, got {:.0}x ({c:?})",
            c.ratio()
        );
        assert_eq!(c.parameter_bytes, 12, "the facet register is 12 bytes");
    }

    /// Refining the mesh must not change the parameter cost.
    ///
    /// This is the property that makes the claim interesting: the wire cost is
    /// independent of the geometry's complexity. If both scaled together there
    /// would be nothing to say.
    #[test]
    fn parameter_cost_is_independent_of_mesh_resolution() {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let coarse = wire_cost(&facet, 2.0);
        let fine = wire_cost(&facet, 0.5);
        assert_eq!(coarse.parameter_bytes, fine.parameter_bytes);
        assert!(
            fine.mesh_bytes > coarse.mesh_bytes * 2,
            "refining should cost materially more mesh: {} vs {}",
            coarse.mesh_bytes,
            fine.mesh_bytes
        );
        assert!(fine.ratio() > coarse.ratio());
    }
}
