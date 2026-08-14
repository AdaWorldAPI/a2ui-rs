//! Force layout over **struct-of-arrays** f32 lanes — the positions the field
//! draws from, and the only per-frame arithmetic in the crate.
//!
//! # Why SoA and not a `Vec<Node>`
//!
//! The step loop touches x, then y, then degree — three separate sweeps over
//! contiguous f32, which is what a vectorizer wants and what an
//! array-of-structs defeats by interleaving bytes nobody reads on that sweep.
//! It is also exactly the buffer shape wgpu uploads, so the layout's output
//! IS the vertex buffer: no gather, no repack.
//!
//! # Deterministic, never random
//!
//! Seeding is the golden-angle spiral (`i * 2.399963…` rad, radius `√i`), so
//! the same graph lays out identically in every process, every reload, every
//! machine. A random seed would make a screenshot unreproducible and a layout
//! bug unrepeatable — and there is nothing a PRNG buys here that an
//! irrational-angle spiral does not.
//!
//! # Cost
//!
//! Repulsion is the honest O(n²) trap. This uses a **uniform grid**: each node
//! repels only within its own and adjacent cells, so the sweep is O(n · k)
//! with k the local occupancy. That is an approximation and it is stated as
//! one — far-field forces are dropped, which the spiral seeding compensates
//! by starting the graph already spread rather than collapsed at the origin.

/// Golden angle in radians — the seeding spiral's turn per node.
const GOLDEN_ANGLE: f32 = 2.399_963_2;
/// Grid cell edge, in layout units, for the repulsion neighbourhood.
const CELL: f32 = 60.0;
/// Repulsion strength between two nodes in neighbouring cells.
const REPULSION: f32 = 900.0;
/// Spring constant along an edge.
const SPRING: f32 = 0.012;
/// Rest length of an edge's spring.
const REST: f32 = 55.0;
/// Pull toward the origin, so a disconnected component cannot drift away.
const GRAVITY: f32 = 0.004;
/// Velocity retained per step.
const DAMPING: f32 = 0.86;
/// Largest displacement one node may take in one step, in layout units.
/// Without it a dense hub can fling itself off-screen on the first frames.
const MAX_STEP: f32 = 24.0;

/// Positions + velocities as parallel lanes, plus the edge list the springs
/// pull along.
pub struct Layout {
    /// X positions, one per node. This lane is uploaded verbatim.
    pub xs: Vec<f32>,
    /// Y positions, one per node. Uploaded verbatim.
    pub ys: Vec<f32>,
    vx: Vec<f32>,
    vy: Vec<f32>,
    /// Per-node mass, derived from degree: a hub resists being pushed.
    mass: Vec<f32>,
    /// `(from, to)` ordinals — already ghost-filtered by the ABI view.
    edges: Vec<[u32; 2]>,
    /// Nodes the user is holding. A pinned node ignores all forces, which is
    /// what makes "grab and the neighbours wobble" a consequence of the
    /// simulation rather than a special case in the renderer.
    pinned: Vec<bool>,
}

impl Layout {
    /// Seed `n` nodes on the golden-angle spiral and take the edge list.
    #[must_use]
    pub fn seeded(n: usize, edges: Vec<[u32; 2]>, degrees: &[u32]) -> Self {
        let mut xs = Vec::with_capacity(n);
        let mut ys = Vec::with_capacity(n);
        for i in 0..n {
            let a = i as f32 * GOLDEN_ANGLE;
            let r = 26.0 * (i as f32).sqrt();
            xs.push(r * a.cos());
            ys.push(r * a.sin());
        }
        let mass = (0..n)
            .map(|i| 1.0 + degrees.get(i).copied().unwrap_or(0) as f32 * 0.5)
            .collect();
        Layout {
            xs,
            ys,
            vx: vec![0.0; n],
            vy: vec![0.0; n],
            mass,
            edges,
            pinned: vec![false; n],
        }
    }

    /// Build directly from a parsed ABI view — the ordinary path.
    #[must_use]
    pub fn from_abi(g: &crate::GraphAbi<'_>) -> Self {
        Self::seeded(g.node_count(), g.edge_pairs(), &g.degrees())
    }

    /// How many nodes the layout carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.xs.len()
    }
    /// Whether the layout is empty (clippy's companion to `len`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }

    /// Hold a node at a position — the drag. While pinned it ignores every
    /// force; its neighbours do not, and that asymmetry IS the wobble.
    pub fn pin(&mut self, i: usize, x: f32, y: f32) {
        if i < self.len() {
            self.pinned[i] = true;
            self.xs[i] = x;
            self.ys[i] = y;
            self.vx[i] = 0.0;
            self.vy[i] = 0.0;
        }
    }
    /// Release a held node back into the simulation.
    pub fn unpin(&mut self, i: usize) {
        if i < self.len() {
            self.pinned[i] = false;
        }
    }
    /// Whether a node is currently held.
    #[must_use]
    pub fn is_pinned(&self, i: usize) -> bool {
        self.pinned.get(i).copied().unwrap_or(false)
    }
}

impl Layout {
    /// Bucket every node into a uniform grid cell — the structure that turns
    /// repulsion from O(n²) into O(n · k).
    fn bin(&self) -> std::collections::HashMap<(i32, i32), Vec<u32>> {
        let mut grid: std::collections::HashMap<(i32, i32), Vec<u32>> =
            std::collections::HashMap::with_capacity(self.len() / 4 + 1);
        for i in 0..self.len() {
            let c = (
                (self.xs[i] / CELL).floor() as i32,
                (self.ys[i] / CELL).floor() as i32,
            );
            grid.entry(c).or_default().push(i as u32);
        }
        grid
    }

    /// Advance the simulation one step.
    ///
    /// Three sweeps, in this order and never merged: local repulsion, edge
    /// springs, then integrate. Merging them would make the result depend on
    /// node order — the same graph would settle differently depending on how
    /// the producer happened to sort it, which is exactly the determinism
    /// this module exists to keep.
    pub fn step(&mut self) {
        let n = self.len();
        if n == 0 {
            return;
        }
        let mut fx = vec![0.0f32; n];
        let mut fy = vec![0.0f32; n];

        // 1. Repulsion, local only.
        let grid = self.bin();
        for (&(cx, cy), bucket) in &grid {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(other) = grid.get(&(cx + dx, cy + dy)) else {
                        continue;
                    };
                    for &a in bucket {
                        let (ax, ay) = (self.xs[a as usize], self.ys[a as usize]);
                        for &b in other {
                            if a == b {
                                continue;
                            }
                            let (mut ddx, mut ddy) =
                                (ax - self.xs[b as usize], ay - self.ys[b as usize]);
                            // Two nodes at the SAME point have no direction to
                            // be pushed along, so the naive form computes a
                            // large force and multiplies it by a zero vector:
                            // they stay welded together forever. (The earlier
                            // note here only guarded the division; the test
                            // `coincident_nodes_separate…` caught that the
                            // direction is the actual failure.) Substitute a
                            // deterministic unit vector derived from the two
                            // ORDINALS — same pair, same escape direction, in
                            // every process; a PRNG here would trade the bug
                            // for an unreproducible layout.
                            if ddx * ddx + ddy * ddy < 1e-6 {
                                let a = (a ^ (b << 1)) as f32 * GOLDEN_ANGLE;
                                ddx = a.cos() * 0.01;
                                ddy = a.sin() * 0.01;
                            }
                            // The floor keeps the force finite once a real
                            // direction exists.
                            let d2 = (ddx * ddx + ddy * ddy).max(0.75);
                            let f = REPULSION / d2;
                            let inv = f / d2.sqrt();
                            fx[a as usize] += ddx * inv;
                            fy[a as usize] += ddy * inv;
                        }
                    }
                }
            }
        }

        // 2. Springs along the edges.
        for &[f, t] in &self.edges {
            let (f, t) = (f as usize, t as usize);
            let (dx, dy) = (self.xs[t] - self.xs[f], self.ys[t] - self.ys[f]);
            let d = (dx * dx + dy * dy).sqrt().max(0.001);
            let pull = SPRING * (d - REST);
            let (ux, uy) = (dx / d * pull, dy / d * pull);
            fx[f] += ux;
            fy[f] += uy;
            fx[t] -= ux;
            fy[t] -= uy;
        }

        // 3. Integrate — gravity, damping, clamp, and the pin veto.
        for i in 0..n {
            if self.pinned[i] {
                continue;
            }
            fx[i] -= self.xs[i] * GRAVITY * self.mass[i];
            fy[i] -= self.ys[i] * GRAVITY * self.mass[i];
            self.vx[i] = (self.vx[i] + fx[i] / self.mass[i]) * DAMPING;
            self.vy[i] = (self.vy[i] + fy[i] / self.mass[i]) * DAMPING;
            let (mut dx, mut dy) = (self.vx[i], self.vy[i]);
            let d = (dx * dx + dy * dy).sqrt();
            if d > MAX_STEP {
                let s = MAX_STEP / d;
                dx *= s;
                dy *= s;
            }
            self.xs[i] += dx;
            self.ys[i] += dy;
        }
    }

    /// Run `k` steps — the settle a first frame does before it is shown.
    pub fn settle(&mut self, k: usize) {
        for _ in 0..k {
            self.step();
        }
    }

    /// Total kinetic energy — the honest convergence read. A caller stops
    /// settling when this stops falling, rather than guessing an iteration
    /// count that is wrong for both small and large graphs.
    #[must_use]
    pub fn energy(&self) -> f32 {
        (0..self.len())
            .map(|i| self.vx[i] * self.vx[i] + self.vy[i] * self.vy[i])
            .sum()
    }

    /// Axis-aligned bounds `(min_x, min_y, max_x, max_y)` — what a
    /// fit-to-view camera needs. `None` for an empty layout, because a
    /// zero-size box would silently divide a zoom by zero.
    #[must_use]
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        if self.is_empty() {
            return None;
        }
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..self.len() {
            b.0 = b.0.min(self.xs[i]);
            b.1 = b.1.min(self.ys[i]);
            b.2 = b.2.max(self.xs[i]);
            b.3 = b.3.max(self.ys[i]);
        }
        Some(b)
    }

    /// The node under a point, or `None`. Nearest-centre within `radius`;
    /// ties break to the lower ordinal so a click is reproducible.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32, radius: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        let r2 = radius * radius;
        for i in 0..self.len() {
            let (dx, dy) = (self.xs[i] - x, self.ys[i] - y);
            let d2 = dx * dx + dy * dy;
            if d2 <= r2 && best.is_none_or(|(_, bd)| d2 < bd) {
                best = Some((i, d2));
            }
        }
        best.map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ring of `n` nodes plus its closing edge — a shape with a known
    /// answer: it must relax outward, not collapse.
    fn ring(n: usize) -> Layout {
        let edges: Vec<[u32; 2]> = (0..n).map(|i| [i as u32, ((i + 1) % n) as u32]).collect();
        let deg = vec![2u32; n];
        Layout::seeded(n, edges, &deg)
    }

    /// Same input, same output — every run, every process. Without this the
    /// spiral could be swapped for a PRNG and nothing would complain.
    #[test]
    fn the_layout_is_deterministic() {
        let (mut a, mut b) = (ring(40), ring(40));
        a.settle(60);
        b.settle(60);
        assert_eq!(a.xs, b.xs);
        assert_eq!(a.ys, b.ys);
    }

    /// CAN FIRE: a sign error in the springs collapses every graph to a point,
    /// and a layout of 40 coincident nodes still "renders" — silently.
    #[test]
    fn it_settles_without_collapsing_or_exploding() {
        let mut l = ring(60);
        let e0 = {
            l.step();
            l.energy()
        };
        l.settle(400);
        assert!(
            l.energy() < e0,
            "the simulation must lose energy, not gain it ({} -> {})",
            e0,
            l.energy()
        );
        let (x0, y0, x1, y1) = l.bounds().expect("non-empty");
        let (w, h) = (x1 - x0, y1 - y0);
        assert!(w > 40.0 && h > 40.0, "collapsed to {w} x {h}");
        assert!(w < 20_000.0 && h < 20_000.0, "exploded to {w} x {h}");
    }

    /// The grab. A pinned node must NOT move, and its neighbours MUST — that
    /// difference is the wobble, so both halves are asserted.
    #[test]
    fn a_pinned_node_holds_and_its_neighbours_move() {
        let mut l = ring(24);
        l.settle(120);
        let neighbour_before = (l.xs[1], l.ys[1]);
        l.pin(0, 500.0, 500.0);
        l.settle(30);
        assert_eq!(
            (l.xs[0], l.ys[0]),
            (500.0, 500.0),
            "the held node stays held"
        );
        assert!(
            (l.xs[1] - neighbour_before.0).abs() + (l.ys[1] - neighbour_before.1).abs() > 1.0,
            "the neighbour did not react — there is no wobble"
        );
        // And releasing really releases: it must be free to move again.
        l.unpin(0);
        let held = (l.xs[0], l.ys[0]);
        l.settle(30);
        assert!(
            (l.xs[0] - held.0).abs() + (l.ys[0] - held.1).abs() > 0.01,
            "unpin did not return the node to the simulation"
        );
    }

    /// Two nodes at the very same point is the division-by-zero the naive
    /// repulsion hits. They must separate, and stay finite.
    #[test]
    fn coincident_nodes_separate_instead_of_producing_nan() {
        let mut l = Layout::seeded(2, vec![], &[0, 0]);
        l.xs = vec![10.0, 10.0];
        l.ys = vec![10.0, 10.0];
        l.settle(20);
        for i in 0..2 {
            assert!(
                l.xs[i].is_finite() && l.ys[i].is_finite(),
                "NaN/inf escaped"
            );
        }
        let d = ((l.xs[0] - l.xs[1]).powi(2) + (l.ys[0] - l.ys[1]).powi(2)).sqrt();
        assert!(d > 0.5, "coincident nodes never separated (d = {d})");
    }

    #[test]
    fn hit_test_finds_the_nearest_centre_and_misses_outside_the_radius() {
        let mut l = Layout::seeded(3, vec![], &[0, 0, 0]);
        l.xs = vec![0.0, 100.0, 103.0];
        l.ys = vec![0.0, 0.0, 0.0];
        assert_eq!(l.hit_test(1.0, 1.0, 10.0), Some(0));
        assert_eq!(l.hit_test(102.0, 0.0, 10.0), Some(2), "nearest, not first");
        // CAN STAY SILENT: empty space answers None rather than the closest.
        assert_eq!(l.hit_test(500.0, 500.0, 10.0), None);
    }

    /// An empty layout must not pretend to have bounds — a zero box would
    /// divide a fit-to-view zoom by zero.
    #[test]
    fn an_empty_layout_has_no_bounds() {
        assert_eq!(Layout::seeded(0, vec![], &[]).bounds(), None);
    }
}
