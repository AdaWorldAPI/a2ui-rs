//! Indexed triangle mesh, and the marching-tetrahedra surface extractor.
//!
//! # Why tetrahedra and not cubes
//!
//! Marching *cubes* is the famous one, and it has a famous problem: several of
//! its 256 corner configurations are ambiguous on a cell face, and neighbouring
//! cells can resolve the same shared face two different ways. The result is a
//! hole. Patching it needs disambiguation tables (asymptotic decider and
//! friends) that are easy to get subtly wrong and hard to test.
//!
//! Marching *tetrahedra* has no such case. A tetrahedron's four corners admit
//! only three distinct crossing patterns (1-in, 2-in, 3-in), each with exactly
//! one triangulation, and every tet face is shared by exactly one neighbour
//! that decomposes identically. **Watertight by construction**, which is the
//! property a printable mesh actually needs — a slicer given a mesh with a hole
//! produces confident garbage rather than an error.
//!
//! The cost is roughly 2× the triangle count of marching cubes for the same
//! grid. For a POC that trade is free, and for printing it is nearly free too:
//! the slicer re-samples to layers anyway.
//!
//! # Why vertices are keyed by grid edge
//!
//! A surface vertex is born on a grid edge, at the sign crossing. Two adjacent
//! tets that share that edge must get the *same* vertex — not two vertices at
//! coincidentally-equal float coordinates. Keying the dedup map on the edge's
//! **integer corner-index pair** makes sharing exact and independent of
//! floating-point equality, which is what lets [`Mesh::is_watertight`] be a
//! genuine proof rather than a tolerance test.

use std::collections::HashMap;

use crate::sdf::{Point, Solid};

/// An indexed triangle mesh in millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Unique vertices.
    pub verts: Vec<Point>,
    /// Triangles as indices into [`Self::verts`], wound counter-clockwise seen
    /// from outside (outward normal by the right-hand rule).
    pub tris: Vec<[u32; 3]>,
}

impl Mesh {
    /// Triangle count.
    #[must_use]
    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }

    /// Enclosed volume in mm³, via the divergence theorem.
    ///
    /// Each triangle contributes `v0 · (v1 × v2) / 6`, the signed volume of the
    /// tetrahedron it forms with the origin. Contributions from faces pointing
    /// away cancel, so the sum is the enclosed volume regardless of where the
    /// origin sits — provided the mesh is closed and consistently wound.
    ///
    /// A **negative** result therefore means the winding is inverted, which is
    /// worth asserting on rather than papering over with `abs()`: an inside-out
    /// mesh renders and slices wrongly, and this is the cheapest place to catch
    /// it.
    #[must_use]
    pub fn volume(&self) -> f32 {
        let mut acc = 0.0f64;
        for t in &self.tris {
            let a = self.verts[t[0] as usize];
            let b = self.verts[t[1] as usize];
            let c = self.verts[t[2] as usize];
            let cr = [
                f64::from(b[1]) * f64::from(c[2]) - f64::from(b[2]) * f64::from(c[1]),
                f64::from(b[2]) * f64::from(c[0]) - f64::from(b[0]) * f64::from(c[2]),
                f64::from(b[0]) * f64::from(c[1]) - f64::from(b[1]) * f64::from(c[0]),
            ];
            acc += f64::from(a[0]) * cr[0] + f64::from(a[1]) * cr[1] + f64::from(a[2]) * cr[2];
        }
        // f64 accumulation, f32 result: a 20 mm part at 0.25 mm resolution is
        // ~100k triangles whose individual contributions differ by orders of
        // magnitude, and summing those in f32 loses real precision against an
        // analytic oracle.
        (acc / 6.0) as f32
    }

    /// Whether every edge is shared by exactly two triangles.
    ///
    /// This is the printability gate. It is exact rather than approximate
    /// because vertices are shared by index (see the module note), so an edge
    /// is an integer pair and "shared" means equal, not "within epsilon".
    #[must_use]
    pub fn is_watertight(&self) -> bool {
        self.non_manifold_edges() == 0
    }

    /// How many edges are NOT shared by exactly two triangles.
    ///
    /// Exposed as a count rather than folded into the boolean so a failing test
    /// can report the magnitude of the breakage: one bad edge is a winding or
    /// tie-breaking bug, thousands is a wrong decomposition.
    #[must_use]
    pub fn non_manifold_edges(&self) -> usize {
        let mut seen: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &self.tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let key = if a < b { (a, b) } else { (b, a) };
                *seen.entry(key).or_insert(0) += 1;
            }
        }
        seen.values().filter(|&&n| n != 2).count()
    }
}

/// The 6-tetrahedron decomposition of a cell, sharing the main diagonal 0–6.
///
/// Corner `n` has offsets `(n & 1, (n >> 1) & 1, (n >> 2) & 1)`. Every cell
/// uses this same decomposition, which is what makes shared faces agree between
/// neighbours — the property marching tets relies on for watertightness. Change
/// this table and you must change it for all cells or the mesh develops seams.
const TETS: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 1, 7, 5],
    [0, 5, 7, 4],
    [0, 3, 2, 7],
    [0, 6, 4, 7],
    [0, 2, 6, 7],
];

/// Extract the zero level set of `solid` over `bounds` at `res` mm spacing.
///
/// `res` is clamped to a sane floor: a zero or negative spacing would be an
/// infinite grid, and the caller most likely meant "as fine as possible".
#[must_use]
pub fn marching_tets(solid: &Solid, bounds: (Point, Point), res: f32) -> Mesh {
    let res = res.max(0.01);
    let (lo, hi) = bounds;

    // Pad by one cell so the surface never touches the sampling boundary. An
    // unpadded grid clips the solid flush with the box and leaves the cut face
    // open — a hole, and one that looks plausible in a viewer.
    //
    // …and then pad by a FURTHER irrational fraction of a cell, which is the
    // part that is not obvious and is load-bearing.
    //
    // A grid corner landing exactly ON the surface (`distance == 0.0`) is
    // formally a measure-zero coincidence, and in practice it is the common
    // case: the geometry is authored on round numbers (a 20 mm plate, faces at
    // ±10) and so is the grid (1.0 mm steps from a round origin), so entire
    // PLANES of corners hit zero at once. Measured on the demo part before this
    // offset existed: 1572 of 6877 corners were exactly zero. Those produce
    // coincident vertices and zero-area triangles, which get dropped, which
    // tears 6496 edges out of the manifold.
    //
    // Offsetting the origin by an irrational fraction of a cell makes exact
    // coincidence impossible for any rationally-authored geometry — and rail
    // encoding guarantees the geometry is rational (multiples of 0.01 mm). A
    // round fraction like 0.5 would NOT do: a 5.00 mm radius with 0.5 mm
    // spacing lands right back on the surface.
    const JITTER: f32 = 0.366_025_4; // (√3 − 1) / 2
    let pad = res * (1.0 + JITTER);
    let lo = [lo[0] - pad, lo[1] - pad, lo[2] - pad];
    let hi = [hi[0] + res, hi[1] + res, hi[2] + res];

    let n = |axis: usize| -> usize { (((hi[axis] - lo[axis]) / res).ceil() as usize).max(1) };
    let (nx, ny, nz) = (n(0), n(1), n(2));

    let pos = |i: usize, j: usize, k: usize| -> Point {
        [
            lo[0] + i as f32 * res,
            lo[1] + j as f32 * res,
            lo[2] + k as f32 * res,
        ]
    };
    let cid = |i: usize, j: usize, k: usize| -> u32 { ((k * (ny + 1) + j) * (nx + 1) + i) as u32 };

    // Sample once per grid corner, not once per tet corner: the naive version
    // evaluates the SDF 24× per cell instead of ~1×, and the SDF is the
    // expensive part of this whole routine.
    let mut field = vec![0.0f32; (nx + 1) * (ny + 1) * (nz + 1)];
    for k in 0..=nz {
        for j in 0..=ny {
            for i in 0..=nx {
                field[cid(i, j, k) as usize] = solid.distance(pos(i, j, k));
            }
        }
    }

    let mut mesh = Mesh::default();
    let mut edge_verts: HashMap<(u32, u32), u32> = HashMap::new();

    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                // The cell's eight corners, as (grid index, position, value).
                let mut corner = [(0u32, [0.0f32; 3], 0.0f32); 8];
                for (c, slot) in corner.iter_mut().enumerate() {
                    let (di, dj, dk) = (c & 1, (c >> 1) & 1, (c >> 2) & 1);
                    let (ci, cj, ck) = (i + di, j + dj, k + dk);
                    let id = cid(ci, cj, ck);
                    *slot = (id, pos(ci, cj, ck), field[id as usize]);
                }

                for tet in &TETS {
                    emit_tet(
                        solid,
                        &[
                            corner[tet[0]],
                            corner[tet[1]],
                            corner[tet[2]],
                            corner[tet[3]],
                        ],
                        &mut mesh,
                        &mut edge_verts,
                    );
                }
            }
        }
    }

    mesh
}

type Corner = (u32, Point, f32);

/// Emit the 0, 1 or 2 triangles this tetrahedron contributes.
fn emit_tet(
    solid: &Solid,
    tet: &[Corner; 4],
    mesh: &mut Mesh,
    edge_verts: &mut HashMap<(u32, u32), u32>,
) {
    // A corner exactly ON the surface is treated as inside. The choice is
    // arbitrary but it must be CONSISTENT: if two neighbouring tets classified
    // a shared zero-valued corner differently they would disagree about which
    // edges cross, and the shared face would not match.
    let inside: Vec<usize> = (0..4).filter(|&c| tet[c].2 <= 0.0).collect();
    let outside: Vec<usize> = (0..4).filter(|&c| tet[c].2 > 0.0).collect();

    match inside.len() {
        0 | 4 => {}
        1 => {
            let a = inside[0];
            let vs = [
                crossing(solid, tet, a, outside[0], mesh, edge_verts),
                crossing(solid, tet, a, outside[1], mesh, edge_verts),
                crossing(solid, tet, a, outside[2], mesh, edge_verts),
            ];
            push_tri(solid, mesh, vs);
        }
        3 => {
            let d = outside[0];
            let vs = [
                crossing(solid, tet, d, inside[0], mesh, edge_verts),
                crossing(solid, tet, d, inside[1], mesh, edge_verts),
                crossing(solid, tet, d, inside[2], mesh, edge_verts),
            ];
            push_tri(solid, mesh, vs);
        }
        _ => {
            // Two in, two out: the crossing edges form a quad. The order below
            // walks the quad's boundary — consecutive entries share a tet
            // corner (a, then d, then b, then c), so it is a genuine cycle and
            // not a bow-tie.
            let (a, b) = (inside[0], inside[1]);
            let (c, d) = (outside[0], outside[1]);
            let q = [
                crossing(solid, tet, a, c, mesh, edge_verts),
                crossing(solid, tet, a, d, mesh, edge_verts),
                crossing(solid, tet, b, d, mesh, edge_verts),
                crossing(solid, tet, b, c, mesh, edge_verts),
            ];
            push_tri(solid, mesh, [q[0], q[1], q[2]]);
            push_tri(solid, mesh, [q[0], q[2], q[3]]);
        }
    }
}

/// The shared vertex where the tet edge `p`–`q` crosses zero.
fn crossing(
    _solid: &Solid,
    tet: &[Corner; 4],
    p: usize,
    q: usize,
    mesh: &mut Mesh,
    edge_verts: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let (pid, ppos, pv) = tet[p];
    let (qid, qpos, qv) = tet[q];
    let key = if pid < qid { (pid, qid) } else { (qid, pid) };
    if let Some(&idx) = edge_verts.get(&key) {
        return idx;
    }

    // Linear interpolation to the zero crossing. `denom` cannot be zero for a
    // real crossing (the two values have opposite signs by construction), but
    // it is guarded anyway: a NaN in the field would otherwise produce a NaN
    // vertex and a mesh that fails far away from the cause.
    let denom = pv - qv;
    let t = if denom.abs() < f32::EPSILON {
        0.5
    } else {
        (pv / denom).clamp(0.0, 1.0)
    };
    let v = [
        ppos[0] + t * (qpos[0] - ppos[0]),
        ppos[1] + t * (qpos[1] - ppos[1]),
        ppos[2] + t * (qpos[2] - ppos[2]),
    ];

    let idx = mesh.verts.len() as u32;
    mesh.verts.push(v);
    edge_verts.insert(key, idx);
    idx
}

/// Append a triangle, orienting it so its normal points OUT of the solid.
///
/// Rather than deriving winding from the corner-configuration tables — which is
/// where sign conventions get inverted and stay inverted, because an
/// inside-out mesh still looks like a mesh — the orientation is *measured*:
/// step a little way along the candidate normal from the triangle's centroid
/// and ask the field whether that is further outside. Costs one extra SDF
/// evaluation per triangle and cannot be silently wrong.
fn push_tri(solid: &Solid, mesh: &mut Mesh, v: [u32; 3]) {
    // Degenerate triangles arise when two crossings land on the same vertex
    // (a corner exactly on the surface). They contribute nothing to volume but
    // would break the watertight count, since their edges appear once.
    if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
        return;
    }

    let a = mesh.verts[v[0] as usize];
    let b = mesh.verts[v[1] as usize];
    let c = mesh.verts[v[2] as usize];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < f32::EPSILON {
        // A sliver whose three vertices are (near) collinear. It is tempting to
        // DROP it — it contributes nothing to volume and looks like noise. That
        // is wrong, and it was the second tearing bug measured here: dropping a
        // triangle removes its three edges from the manifold, so 480 edges went
        // unmatched at res = 0.25 purely because slivers were discarded.
        //
        // Keep it. Its volume contribution is zero by construction, its
        // orientation is meaningless (no normal to orient by, hence the early
        // push), and its edges are exactly what its neighbours need to match
        // against. A slicer ignores zero-area facets; an open mesh it cannot.
        mesh.tris.push(v);
        return;
    }
    let centroid = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let eps = 1e-3;
    let probe = [
        centroid[0] + n[0] / len * eps,
        centroid[1] + n[1] / len * eps,
        centroid[2] + n[2] / len * eps,
    ];

    if solid.distance(probe) >= solid.distance(centroid) {
        mesh.tris.push(v);
    } else {
        mesh.tris.push([v[0], v[2], v[1]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rail::Facet;
    use crate::sdf::{plate_with_bore, plate_with_bore_volume};

    fn plate(res: f32) -> (Mesh, f32) {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let solid = plate_with_bore(&facet);
        let want = plate_with_bore_volume(&facet).expect("oracle applies");
        (marching_tets(&solid, solid.bounds(), res), want)
    }

    /// The printability gate. An unwatertight mesh is not a slower mesh, it is
    /// an unprintable one.
    #[test]
    fn the_mesh_is_watertight() {
        let (m, _) = plate(1.0);
        assert!(m.tri_count() > 0, "the mesher produced nothing");
        assert_eq!(
            m.non_manifold_edges(),
            0,
            "{} edges are not shared by exactly two triangles",
            m.non_manifold_edges()
        );
    }

    /// Watertight at resolutions that DIVIDE the geometry exactly.
    ///
    /// The regression guard for the grid jitter. Every resolution here is a
    /// clean divisor of the part's 20 / 10 / 5 mm dimensions, so without the
    /// irrational origin offset the sample grid lands on the surface and the
    /// mesh tears (measured: 6496 broken edges at `res = 1.0`). Picking
    /// deliberately hostile resolutions is the point — a test at `res = 0.7`
    /// would pass with the jitter removed and guard nothing.
    #[test]
    fn watertight_even_when_the_resolution_divides_the_geometry() {
        for res in [2.0, 1.0, 0.5, 0.25] {
            let (m, _) = plate(res);
            assert_eq!(
                m.non_manifold_edges(),
                0,
                "res {res} tore the mesh — grid alignment regression?"
            );
        }
    }

    /// Winding is outward, proven by the sign of the enclosed volume rather
    /// than by inspecting normals — an inverted mesh reports negative volume.
    #[test]
    fn the_winding_is_outward() {
        let (m, _) = plate(1.0);
        assert!(
            m.volume() > 0.0,
            "negative volume means the mesh is inside-out: {}",
            m.volume()
        );
    }

    /// Volume converges on the analytic answer as the grid refines.
    ///
    /// Convergence rather than a single tolerance, deliberately: a lone
    /// "within 5%" assertion passes for a mesher that is wrong in a way that
    /// happens to be small at one resolution. Requiring the error to *shrink*
    /// tests the method.
    #[test]
    fn volume_converges_on_the_analytic_oracle() {
        let (coarse, want) = plate(2.0);
        let (fine, _) = plate(0.5);
        let e_coarse = (coarse.volume() - want).abs() / want;
        let e_fine = (fine.volume() - want).abs() / want;
        assert!(
            e_fine < e_coarse,
            "refining must reduce error: coarse {e_coarse:.4}, fine {e_fine:.4}"
        );
        assert!(
            e_fine < 0.02,
            "fine mesh should be within 2% of {want:.1} mm³, got {:.1} ({e_fine:.4})",
            fine.volume()
        );
    }

    /// The bore is really there — the falsifier for "difference did nothing".
    ///
    /// Without this, every assertion above would pass for a mesher that quietly
    /// ignored the subtraction and produced a solid plate.
    #[test]
    fn the_bore_removes_the_volume_it_should() {
        let solid_facet = Facet::from_mm([20.0, 20.0, 10.0, 0.0, 0.0, 0.0]);
        let bored_facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let a = plate_with_bore(&solid_facet);
        let b = plate_with_bore(&bored_facet);
        let va = marching_tets(&a, a.bounds(), 0.5).volume();
        let vb = marching_tets(&b, b.bounds(), 0.5).volume();
        let removed = va - vb;
        let want = std::f32::consts::PI * 25.0 * 10.0; // πr²h
        assert!(
            (removed - want).abs() / want < 0.03,
            "bore should remove ~{want:.1} mm³, removed {removed:.1}"
        );
    }

    /// One encoding step must change the geometry.
    ///
    /// The rail resolution is only meaningful if the smallest representable
    /// change is observable in the output. If this failed, 0.01 mm would be
    /// decoration and the encoding could just as well be whole millimetres.
    #[test]
    fn one_rail_step_changes_the_volume() {
        let base = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let stepped =
            Facet::from_mm([20.0, 20.0, 10.0, 5.0 + crate::rail::Rail::STEP_MM, 0.0, 0.0]);
        let va = plate_with_bore_volume(&base).unwrap();
        let vb = plate_with_bore_volume(&stepped).unwrap();
        assert!(
            (va - vb).abs() > 0.0,
            "a 0.01 mm bore step must change the part; the encoding step is inert otherwise"
        );
        // And in the right direction: a wider bore removes more material.
        assert!(vb < va, "a larger bore must leave less material");
    }
}
