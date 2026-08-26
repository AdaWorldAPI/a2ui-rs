//! Rail-driven radial placement — the layout that **reads the address**
//! instead of rediscovering it with a solver.
//!
//! # Why this exists beside [`crate::layout`]
//!
//! Every node in an MGRA v3 stream that carries the rail lane already holds
//! a 12-level hierarchical address: a distinguished name, `2\2\34\18\37\1`.
//! [`crate::layout::Layout::from_abi`] takes only `node_count`, the edge
//! pairs and the degrees — it throws that address away and then runs a
//! force simulation to *recover* the structure it was handed for free.
//!
//! The producer's own contract already states the intended rule
//! (`medcare-server/src/views/graph_abi.rs`):
//!
//! > *Ring = Tiefe, Arc = Radix-Bruch der Slots. Die primaere Achse
//! > PLATZIERT, die sekundaere UEBERLAGERT. **Kein Solver**, zwei Abrufe
//! > rendern identisch.*
//!
//! This module is that rule, with two corrections the corpus forced (both
//! measured, both recorded below): the arc comes from **occupancy**, not
//! from the radix, and the low-information head levels are folded out of
//! the *radius* while still ordering the *arc*.
//!
//! # The de-interleave trap — read this before touching a rail byte
//!
//! A rail entry is **24 bytes = two stacked 12-byte registers**, each
//! `6×(u8:u8)`, carrying **two interleaved axes**: taxonomy and mereology.
//! Level `i` lives at byte `2·(i mod 6)` of register `i / 6`, with the
//! taxonomy byte first and the mereology byte second.
//!
//! Read naively as twelve little-endian `u16`s it looks like plausible
//! garbage, because a level's two axis bytes fuse into one number:
//! `uterus` has `tax[0] = 2, mer[0] = 1`, which reads as `0x0102 = 258`.
//! Every level of every node is corrupted the same way and nothing about
//! the result looks wrong — the depths are still monotone, the values are
//! still small-ish, the tree still "works". [`decode_rail`] is the only
//! sanctioned reader, and `naive_u16_read_fuses_the_two_axes` pins the
//! trap two-sided so a future session cannot re-derive it by hand.
//!
//! # What the corpus measured (38 751 nodes, MedCare `wave.abi`)
//!
//! Per-level Shannon entropy of the taxonomy axis:
//!
//! | level | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
//! |---|---|---|---|---|---|---|---|---|---|---|---|---|
//! | bits | 1.17 | 1.31 | 3.50 | 3.65 | 4.48 | **5.05** | 4.08 | 4.49 | 4.09 | 3.02 | 2.78 | 2.51 |
//!
//! The first two levels carry ~2.5 bits between them, against the ~15.2
//! needed to address the corpus at all: level 0 has eight distinct values
//! of which one holds 59.9 %, level 1 has forty of which one holds 66.7 %.
//! Spending two of eleven rings — 16 % of the radius — on 2.5 bits is the
//! waste [`FOLD_BITS`] removes.
//!
//! Three placement choices, each measured against the alternative rather
//! than argued:
//!
//! | variant | span | `is_a` median | p90 | densest cell |
//! |---|---|---|---|---|
//! | radix arc (the contract's literal reading) | 1335 | 282 | 493 | **2739** |
//! | occupancy arc, no fold | 2163 | 475 | 916 | 107 |
//! | occupancy arc + fold 2 (this design) | 1802 | 389 | 693 | **107** |
//! | occupancy arc + fold 3 | 1622 | 369 | 603 | **338** |
//!
//! Those four rows come from a prototype sweep and are kept because they
//! are what the *choice between variants* was made on. The SHIPPED code
//! measures slightly wider on the same file — span 2005, `is_a` median
//! 438, p90 809, densest cell 108, **0 nodes at the origin**, 38 751 nodes
//! placed in **20 ms** — because the origin fix below adds one ring step to
//! every radius. The rows are not re-fitted to hide that: a comparison
//! table and an absolute measurement answer different questions, and
//! silently swapping one for the other is how a table stops being evidence.
//!
//! Read the density column, not the median. A radix arc gives every slot
//! the same width regardless of how many nodes are behind it, so 59.9 % of
//! the graph crowds into one wedge — 2739 nodes in a single ring-degree
//! cell, a 25× worse hot spot, which is what "wobbly soup" looks like once
//! the physics is taken away. And folding one level further than the
//! entropy rule chooses trades a better median for a 3.2× worse hot spot;
//! that the density metric independently stops at the same place the
//! entropy threshold does is why [`FOLD_BITS`] is a threshold rather than
//! a fitted constant.
//!
//! # Romanesco — why the default is recursive phyllotaxis
//!
//! The sunburst above is a good *sunburst*. Measured against the rule that
//! actually reads the hierarchy as self-similar, it loses on every axis.
//! All four scaled to the same span (2500) so the numbers are comparable:
//!
//! | rule | `is_a` median | p90 | nn p10 | nn p50 | stacked |
//! |---|---|---|---|---|---|
//! | sunburst | 518 | 958 | 0.08 | 0.16 | 198 |
//! | **romanesco** | **338** | **817** | **0.85** | **2.48** | **0** |
//!
//! `nn` is nearest-neighbour separation, and it is the column that decides
//! whether a field is legible at all. The sunburst's median of 0.16 means
//! **half of all nodes have a neighbour within a sixth of a pixel** — the
//! rings are simply too short a curve to hold their occupants, so siblings
//! smear along an arc. Romanesco's 2.48 is a 19× improvement, and it stacks
//! **nothing**, because filling a disc gives a subtree area to grow into
//! where an arc gives it only length.
//!
//! The trade is real and is not hidden: **depth stops being readable as
//! distance from the centre.** Under Romanesco depth is encoded by SCALE —
//! measured, the median radius by tree depth is 693, 69, 726, 769… , not
//! monotone at all. You read depth by zooming, which is what a pan-and-zoom
//! field is for, but it is a different affordance and
//! [`RadialStyle::Sunburst`] is kept for when rings are what is wanted.
//!
//! **The rule was already half-present in this crate.** `Layout::seeded`
//! seeds the force simulation by golden-angle phyllotaxis — the right rule,
//! applied once, flat, over *lane order*, which carries no structure at
//! all. Romanesco is that same rule made recursive over the *address*. The
//! constant is shared from `layout.rs` rather than redefined, so the two
//! cannot drift.
//!
//! # What was tried and REJECTED, so it is not re-attempted
//!
//! **Occupancy-driven ring radii** — giving each ring the circumference its
//! population needs — is catastrophic and was measured, not reasoned away:
//! ring 4 holds 7520 nodes, which at any legible arc spacing demands a
//! radius in the thousands, and the span blows out from 1802 to **14 975**
//! with `is_a` median 389 → **5211**. Angular occupancy weighting is
//! relative and self-normalising; radial occupancy weighting is absolute
//! and unbounded. They are not the same idea applied to two axes.
//!
//! **Tribonacci ring spacing** — radii stepping by 1, 2, 4, 7, 13, 24, …,
//! the tribonacci analogue of a golden ladder — fails for the same reason,
//! and the arithmetic says why before any measurement does: the ratio
//! converges on 1.8393, and over eleven levels that compounds to ≈1400×.
//! Normalised to a sane span it crushes the inner rings (radius 4 to 49 for
//! the first five) and the densest cell goes 528 → **3223**; un-normalised
//! the span reaches **111 578**. A geometric ladder cannot span a
//! hierarchy whose depth it does not know about.
//!
//! **The bouquet** — each subtree a local fan around its parent, rather
//! than a global ring — is the closest of the rejected shapes, and it
//! taught the lesson Romanesco then used. It *does* shorten edges (median
//! 434 → 381), but it buys them by concentrating: the same nodes occupy 645
//! of the 40 px cells the sunburst spreads over 1655, i.e. 39 % of the
//! area. Scaling each stem by its subtree's size to relieve that (the
//! standard balloon-layout fix) overcorrects the other way — span 18 598,
//! median 1340. What was missing is that a fan is one-dimensional: children
//! placed along an arc get length, not area. Romanesco keeps the local,
//! recursive idea and gives each subtree a DISC.
//!
//! A sub-linear ring exponent (`r = 110·d^0.85`) DID beat uniform spacing
//! on this corpus at equal density (span 1477, median 337, p90 605). It is
//! exposed as [`RadialParams::ring_gamma`] and left at `1.0`: one corpus is
//! not enough to pin an exponent, and a fitted constant with no second
//! corpus to falsify it is exactly the kind of number this workspace makes
//! a rule of not shipping. The knob is live; the default is honest.

use crate::abi::GraphAbi;
use crate::layout::GOLDEN_ANGLE;

/// Levels carried per axis, per register.
const LEVELS_PER_REGISTER: usize = 6;
/// Bytes in one axis register — `6×(u8:u8)`.
const REGISTER_BYTES: usize = 12;
/// Addressable levels per axis on the wire: two registers of six.
pub const RAIL_DEPTH: usize = 2 * LEVELS_PER_REGISTER;

/// A leading level carrying less than this many bits is folded out of the
/// RADIUS — it still orders the arc, it just does not earn a ring.
///
/// Not a fitted constant: `2.0` sits in the gap between the measured
/// 1.31 bits of level 1 and the 3.50 of level 2, and the density metric
/// (an independent measure the threshold does not see) stops folding at
/// exactly the same level. See the module table.
pub const FOLD_BITS: f32 = 2.0;

/// One node's rail register, de-interleaved into its two axes.
///
/// A zero terminates a path: level `i` is meaningful only while every
/// level before it is non-zero, which is the zero-fallback ladder the
/// canon uses everywhere else — a zero tier means *not consulted*, never
/// *absent*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RailPath {
    /// The primary axis. It PLACES.
    pub taxonomy: [u8; RAIL_DEPTH],
    /// The secondary axis. It OVERLAYS.
    pub mereology: [u8; RAIL_DEPTH],
}

impl RailPath {
    /// How many taxonomy levels are populated before the first zero.
    #[must_use]
    pub fn taxonomy_depth(&self) -> usize {
        Self::depth(&self.taxonomy)
    }
    /// How many mereology levels are populated before the first zero.
    #[must_use]
    pub fn mereology_depth(&self) -> usize {
        Self::depth(&self.mereology)
    }
    fn depth(axis: &[u8; RAIL_DEPTH]) -> usize {
        axis.iter().position(|&b| b == 0).unwrap_or(RAIL_DEPTH)
    }
    /// The populated taxonomy prefix — the node's distinguished name.
    #[must_use]
    pub fn taxonomy_path(&self) -> &[u8] {
        &self.taxonomy[..self.taxonomy_depth()]
    }
}

/// De-interleave one 24-byte rail entry into its two axes.
///
/// Returns `None` for a short entry rather than reading past it — a
/// truncated lane is a producer bug and must not become silent zeros that
/// place every node at the origin.
#[must_use]
pub fn decode_rail(entry: &[u8]) -> Option<RailPath> {
    if entry.len() < 2 * REGISTER_BYTES {
        return None;
    }
    let mut p = RailPath::default();
    for level in 0..RAIL_DEPTH {
        let reg = if level < LEVELS_PER_REGISTER {
            0
        } else {
            REGISTER_BYTES
        };
        let k = level % LEVELS_PER_REGISTER;
        p.taxonomy[level] = entry[reg + 2 * k];
        p.mereology[level] = entry[reg + 2 * k + 1];
    }
    Some(p)
}

/// Which placement rule lays the address out.
///
/// Both read the same address; they differ in what the *picture* encodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadialStyle {
    /// **Romanesco** — recursive phyllotaxis. Each node's children fill its
    /// disc at the golden angle, with a radius proportional to the square
    /// root of the subtree's size, and the same rule applies inside every
    /// child. Self-similar at every scale, like the vegetable.
    ///
    /// This is the default because at equal span it beats [`Self::Sunburst`]
    /// on every measure taken (see the module docs), most decisively on the
    /// one that decides whether a field is readable at all: **nearest-
    /// neighbour separation, 19× better at the median**.
    ///
    /// The trade, stated because it is real: depth stops being readable as
    /// distance-from-centre. Under this rule depth is encoded by SCALE, so
    /// you read it by zooming rather than by counting rings — which is what
    /// a pan-and-zoom field is for, but it IS a different affordance.
    #[default]
    Romanesco,
    /// **Sunburst** — concentric rings, one per unfolded level, each level's
    /// arc split by occupancy. Depth is readable as distance from the
    /// centre, which [`Self::Romanesco`] gives up. Kept for that reason and
    /// because it is the literal reading of the producer's contract.
    Sunburst,
}

/// Placement knobs. The defaults are what the corpus measured; every one
/// of them is a number this module can point at a table for.
#[derive(Clone, Copy, Debug)]
pub struct RadialParams {
    /// Which placement rule to use.
    pub style: RadialStyle,
    /// Radius step between consecutive rings. [`RadialStyle::Sunburst`] only.
    pub ring: f32,
    /// Ring radius exponent: `r = ring · depth^gamma`. `1.0` is uniform.
    /// See the module docs for why the measured-better `0.85` is not the
    /// default. [`RadialStyle::Sunburst`] only.
    pub ring_gamma: f32,
    /// Radius of the outermost disc. [`RadialStyle::Romanesco`] only.
    pub disc: f32,
    /// How much of a parent disc its children may fill, as a linear factor.
    ///
    /// Below `1.0` because circles cannot tile a circle: the children's
    /// areas sum to the parent's exactly at `1.0`, so some slack is what
    /// keeps sibling discs from overlapping. Measured on the live corpus,
    /// this is the legibility knob — `0.88` gives shorter edges, `0.95`
    /// gives more separation, and `0.92` is where both are still better
    /// than the sunburst on every axis.
    pub pack: f32,
    /// Fraction of one ring step the secondary axis may displace a node
    /// radially. The overlay must never reach the neighbouring ring, or
    /// depth stops being readable off the picture.
    pub overlay: f32,
}

impl Default for RadialParams {
    fn default() -> Self {
        Self {
            style: RadialStyle::Romanesco,
            ring: 95.0,
            ring_gamma: 1.0,
            disc: 1200.0,
            pack: 0.92,
            overlay: 0.35,
        }
    }
}

/// Positions, parallel to the node ordinals — the same SoA shape
/// [`crate::layout::Layout`] exposes, so a renderer consumes either.
#[derive(Clone, Debug, Default)]
pub struct RadialLayout {
    xs: Vec<f32>,
    ys: Vec<f32>,
    /// The ring each node landed on, after folding.
    rings: Vec<u16>,
    /// How many leading levels were folded out of the radius.
    folded: usize,
}

impl RadialLayout {
    /// Place every node from its rail address. `None` when the stream
    /// carries no rail lane — the caller falls back to the force layout,
    /// which is the honest thing to do rather than stacking an unaddressed
    /// graph at the origin.
    #[must_use]
    pub fn from_abi(g: &GraphAbi<'_>) -> Option<Self> {
        let n = g.node_count();
        let paths: Vec<RailPath> = (0..n)
            .map(|i| g.rail(i).and_then(decode_rail))
            .collect::<Option<_>>()?;
        Some(Self::from_paths(&paths, RadialParams::default()))
    }

    /// The placement itself, split out so a test can drive it from hand-
    /// built paths without assembling a whole ABI stream.
    #[must_use]
    pub fn from_paths(paths: &[RailPath], p: RadialParams) -> Self {
        let n = paths.len();
        let folded = fold_depth(paths);
        let mut xs = vec![0.0; n];
        let mut ys = vec![0.0; n];
        let mut rings = vec![0u16; n];

        if p.style == RadialStyle::Romanesco {
            // The children of the root, and their children, and so on down —
            // the same rule at every scale.
            let nodes: Vec<usize> = order_of(paths);
            let root = Group::split(&nodes, paths, 0);
            bloom(
                &root, paths, 0.0, 0.0, p.disc, 0.0, p, &mut xs, &mut ys, &mut rings,
            );
            return Self {
                xs,
                ys,
                rings,
                folded,
            };
        }

        // One occupancy-weighted sunburst sweep. Sorting by the FULL path
        // — folded levels included — is what keeps a folded subtree
        // contiguous in arc while costing it no radius.
        let order: Vec<usize> = order_of(paths);
        // Scoped so the mutable borrows of `xs`/`ys` end before the struct
        // is built, without a no-op `drop` of a non-Drop closure.
        {
            let mut emit = |i: usize, ring: u16, angle: f32| {
                // `ring + 1`, not `ring`. A bare `ring` puts EVERY ring-0 node at
                // exactly the origin, where its angle — correctly computed, and
                // discarded a line later — cannot be seen. On the live corpus
                // that silently stacked 93 root concepts on one pixel, and with
                // the head levels folded it can be the whole graph. A layout
                // must never place a node at a coordinate that encodes nothing.
                let radius =
                    p.ring * (f32::from(ring) + 1.0).powf(p.ring_gamma) + overlay_of(&paths[i], p);
                xs[i] = radius * angle.cos();
                ys[i] = radius * angle.sin();
            };
            place_inner(
                &order,
                paths,
                0,
                0.0,
                std::f32::consts::TAU,
                0,
                &mut rings,
                &mut emit,
                folded,
            );
        }
        Self {
            xs,
            ys,
            rings,
            folded,
        }
    }

    /// X positions, parallel to node ordinals.
    #[must_use]
    pub fn xs(&self) -> &[f32] {
        &self.xs
    }
    /// Y positions, parallel to node ordinals.
    #[must_use]
    pub fn ys(&self) -> &[f32] {
        &self.ys
    }
    /// The ring a node landed on, after folding.
    #[must_use]
    pub fn ring(&self, i: usize) -> u16 {
        self.rings.get(i).copied().unwrap_or(0)
    }
    /// How many leading levels the entropy rule folded out of the radius.
    #[must_use]
    pub fn folded_levels(&self) -> usize {
        self.folded
    }
    /// Node count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.xs.len()
    }
    /// Whether the layout is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }
    /// Axis-aligned bounds `(min_x, min_y, max_x, max_y)`, `None` when empty.
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
}

/// The secondary axis, as a bounded radial nudge inside the node's own ring.
fn overlay_of(path: &RailPath, p: RadialParams) -> f32 {
    let mut acc = 0.0f32;
    let mut scale = 1.0f32;
    for &v in &path.mereology[..path.mereology_depth()] {
        scale /= 16.0;
        acc += f32::from(v) * scale;
    }
    acc.min(1.0) * p.ring * p.overlay
}

/// How many LEADING levels fall below [`FOLD_BITS`] of Shannon entropy.
///
/// Leading only, and it stops at the first informative level: folding an
/// interior level would merge siblings that the address deliberately
/// separates, which is a different and much worse operation than declining
/// to spend radius on a near-constant prefix.
fn fold_depth(paths: &[RailPath]) -> usize {
    let mut folded = 0;
    while folded < RAIL_DEPTH {
        let mut hist = [0u32; 256];
        let mut total = 0u32;
        for p in paths {
            if p.taxonomy_depth() > folded {
                hist[p.taxonomy[folded] as usize] += 1;
                total += 1;
            }
        }
        if total == 0 {
            break;
        }
        let n = total as f32;
        let bits: f32 = hist
            .iter()
            .filter(|&&c| c > 0)
            .map(|&c| {
                let q = c as f32 / n;
                -q * q.log2()
            })
            .sum();
        if bits >= FOLD_BITS {
            break;
        }
        folded += 1;
    }
    folded
}

/// Recursive sunburst sweep over a pre-sorted ordinal slice.
///
/// The arc is split among the children in proportion to how many nodes each
/// carries — the occupancy weighting. A child at a folded level receives
/// its parent's ring rather than the next one.
#[expect(clippy::too_many_arguments, reason = "a recursive sweep's own frame")]
fn place_inner(
    order: &[usize],
    paths: &[RailPath],
    level: usize,
    a0: f32,
    a1: f32,
    ring: u16,
    rings: &mut [u16],
    emit: &mut dyn FnMut(usize, u16, f32),
    folded: usize,
) {
    if order.is_empty() {
        return;
    }
    // Nodes whose path ENDS here sit on this ring, at the wedge's middle.
    let mid = 0.5 * (a0 + a1);
    let mut start = 0;
    while start < order.len() && paths[order[start]].taxonomy_depth() <= level {
        let i = order[start];
        rings[i] = ring;
        emit(i, ring, mid);
        start += 1;
    }
    let rest = &order[start..];
    if rest.is_empty() {
        return;
    }
    // The deeper nodes, grouped by this level's value — the slice is
    // already sorted by the full path, so each group is contiguous.
    let child_ring = if level < folded { ring } else { ring + 1 };
    let total = rest.len() as f32;
    let mut cursor = 0usize;
    let mut angle = a0;
    while cursor < rest.len() {
        let v = paths[rest[cursor]].taxonomy[level];
        let mut end = cursor;
        while end < rest.len() && paths[rest[end]].taxonomy[level] == v {
            end += 1;
        }
        let width = (a1 - a0) * (end - cursor) as f32 / total;
        place_inner(
            &rest[cursor..end],
            paths,
            level + 1,
            angle,
            angle + width,
            child_ring,
            rings,
            emit,
            folded,
        );
        angle += width;
        cursor = end;
    }
}

/// Node ordinals sorted by their full address, ties broken by ordinal.
///
/// Sorting by the FULL path — folded levels included — is what keeps a
/// folded subtree contiguous while costing it no radius, and it is what
/// lets both placement rules find their groups by a linear scan.
fn order_of(paths: &[RailPath]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..paths.len()).collect();
    order.sort_by(|&a, &b| {
        paths[a]
            .taxonomy_path()
            .cmp(paths[b].taxonomy_path())
            .then(a.cmp(&b))
    });
    order
}

/// One subtree: the ordinals that terminate here, and the child subtrees.
struct Group<'a> {
    here: &'a [usize],
    kids: Vec<Group<'a>>,
    /// Total ordinals in this subtree, `here` included — the AREA weight.
    size: usize,
}

impl<'a> Group<'a> {
    /// Carve a pre-sorted slice into "ends at this level" + one group per
    /// distinct value of this level. Linear, because the slice is sorted.
    fn split(order: &'a [usize], paths: &[RailPath], level: usize) -> Self {
        let mut end_here = 0;
        while end_here < order.len() && paths[order[end_here]].taxonomy_depth() <= level {
            end_here += 1;
        }
        let (here, rest) = order.split_at(end_here);
        let mut kids = Vec::new();
        let mut cursor = 0;
        while cursor < rest.len() {
            let v = paths[rest[cursor]].taxonomy[level];
            let mut end = cursor;
            while end < rest.len() && paths[rest[end]].taxonomy[level] == v {
                end += 1;
            }
            kids.push(Self::split(&rest[cursor..end], paths, level + 1));
            cursor = end;
        }
        let size = here.len() + kids.iter().map(|k| k.size).sum::<usize>();
        Self { here, kids, size }
    }
}

/// The Romanesco sweep: fill this disc with this subtree, then recurse.
///
/// # The two rules, and why each is the one it is
///
/// **Radius grows as √(cumulative share).** A disc's area is `πr²`, so a
/// radius proportional to the square root of a share makes the AREA
/// proportional to the share — which is the only scaling that neither
/// crowds a big subtree nor strands a small one. It is also why a
/// geometric ladder fails here: a tribonacci step compounds its ratio
/// (1.839¹¹ ≈ 1400×) and a linear stem does not shrink at all, so both
/// were measured and rejected (module docs).
///
/// **Angle advances by the golden angle.** Consecutive items land at
/// 137.508°, the classic phyllotaxis packing: because the golden ratio is
/// the irrational hardest to approximate by a rational, no two items ever
/// fall on the same spoke, at any count. Any rational fraction of a turn
/// produces spokes and therefore gaps.
///
/// Together they are Vogel's sunflower rule, generalised to weighted items
/// and applied recursively — the same rule at every scale, which is what
/// makes it self-similar.
///
/// The phase is inherited from the parent's own angle rather than reset to
/// zero. That is what stops every subtree starting its spiral at due east,
/// which reads as a visible seam through the whole field.
#[expect(clippy::too_many_arguments, reason = "a recursive sweep's own frame")]
fn bloom(
    g: &Group<'_>,
    paths: &[RailPath],
    cx: f32,
    cy: f32,
    radius: f32,
    phase: f32,
    p: RadialParams,
    xs: &mut [f32],
    ys: &mut [f32],
    rings: &mut [u16],
) {
    let total = g.size.max(1) as f32;
    let span = radius * p.pack;
    let mut cumulative = 0.0f32;
    let mut slot = 0.0f32;

    let step = |share: f32, cumulative: &mut f32, slot: &mut f32| {
        // The item's own midpoint in the cumulative area, so a large child
        // sits at the centre of the band it occupies rather than at its edge.
        let r = span * ((*cumulative + share * 0.5) / total).sqrt();
        let a = phase + *slot * GOLDEN_ANGLE;
        *cumulative += share;
        *slot += 1.0;
        (cx + r * a.cos(), cy + r * a.sin(), a)
    };

    for &i in g.here {
        let (x, y, _) = step(1.0, &mut cumulative, &mut slot);
        xs[i] = x;
        ys[i] = y;
        rings[i] = u16::try_from(paths[i].taxonomy_depth()).unwrap_or(u16::MAX);
    }
    for kid in &g.kids {
        let share = kid.size as f32;
        let (x, y, a) = step(share, &mut cumulative, &mut slot);
        // Area-proportional child disc: its area is its share of this one.
        let r = span * (share / total).sqrt();
        bloom(kid, paths, x, y, r, a, p, xs, ys, rings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a rail entry the way the producer does: level `i` writes
    /// `(tax[i], mer[i])` into register `i / 6` at byte `2·(i mod 6)`.
    fn entry(tax: &[u8], mer: &[u8]) -> [u8; 24] {
        let mut out = [0u8; 24];
        for level in 0..RAIL_DEPTH {
            let reg = if level < LEVELS_PER_REGISTER {
                0
            } else {
                REGISTER_BYTES
            };
            let k = level % LEVELS_PER_REGISTER;
            out[reg + 2 * k] = tax.get(level).copied().unwrap_or(0);
            out[reg + 2 * k + 1] = mer.get(level).copied().unwrap_or(0);
        }
        out
    }

    /// Sunburst params. The tests below predate [`RadialStyle::Romanesco`]
    /// becoming the default and test the SUNBURST rule specifically — rings,
    /// arcs, the radial overlay. Pinning the style is the honest fix: the
    /// alternative, re-pointing them at whatever the default happens to be,
    /// would quietly turn them into tests of a rule they were never written
    /// for.
    fn sun() -> RadialParams {
        RadialParams {
            style: RadialStyle::Sunburst,
            ..Default::default()
        }
    }

    fn path(tax: &[u8]) -> RailPath {
        decode_rail(&entry(tax, &[])).expect("full-width entry")
    }

    #[test]
    fn decode_splits_the_two_axes_at_every_level() {
        // Distinct values on both axes at every one of the twelve levels,
        // so a decoder that dropped, doubled, or crossed a level fails.
        let tax: Vec<u8> = (1..=12).collect();
        let mer: Vec<u8> = (101..=112).collect();
        let p = decode_rail(&entry(&tax, &mer)).expect("decodes");
        assert_eq!(&p.taxonomy[..], &tax[..], "taxonomy axis");
        assert_eq!(&p.mereology[..], &mer[..], "mereology axis");
        assert_eq!(p.taxonomy_depth(), 12);
    }

    /// The trap this module exists to stop a future session re-deriving.
    ///
    /// The naive read is not merely different — it is PLAUSIBLE, which is
    /// what makes it expensive. So this asserts both halves: the correct
    /// decode recovers the real bytes, AND the naive `u16` read produces
    /// the specific wrong number (`tax | mer << 8`) that looks like data.
    #[test]
    fn naive_u16_read_fuses_the_two_axes() {
        // `uterus` from the live MedCare stream.
        let tax = [2u8, 2, 34, 18, 37, 1];
        let mer = [1u8, 4, 1, 15];
        let raw = entry(&tax, &mer);

        let p = decode_rail(&raw).expect("decodes");
        assert_eq!(&p.taxonomy[..6], &tax, "the correct read");
        assert_eq!(&p.mereology[..4], &mer, "the correct read");

        let naive: Vec<u16> = raw[..12]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(
            naive[0], 258,
            "level 0 fuses to 0x0102 — the number that fooled a session"
        );
        assert_ne!(
            u16::from(p.taxonomy[0]),
            naive[0],
            "if these ever agree the de-interleave has been lost"
        );
    }

    #[test]
    fn a_short_entry_decodes_to_nothing_rather_than_to_the_origin() {
        assert!(decode_rail(&[1, 2, 3]).is_none());
        assert!(decode_rail(&[]).is_none());
    }

    #[test]
    fn a_zero_terminates_the_path() {
        let p = path(&[5, 2, 15]);
        assert_eq!(p.taxonomy_depth(), 3);
        assert_eq!(p.taxonomy_path(), &[5, 2, 15]);
    }

    /// Occupancy weighting: a wedge is as wide as the nodes behind it.
    ///
    /// The fixture is shaped so level 0 clears [`FOLD_BITS`] (2.73 bits over
    /// eight roots) — otherwise everything folds, every node lands on the
    /// innermost ring, and the test measures nothing. That is exactly how
    /// this test failed on its first writing, and it found the ring-0
    /// origin collapse rather than a weighting bug.
    #[test]
    fn arc_is_proportional_to_occupancy_not_to_slot_count() {
        // Root 1 carries four nodes, split 3:1 at level 1; roots 2..=8 carry
        // one node each. Eleven nodes, so root 1's wedge is 4/11 of the
        // circle and the 3-node group takes three quarters of THAT.
        let mut paths = vec![path(&[1, 1]); 3];
        paths.push(path(&[1, 2]));
        for v in 2..=8u8 {
            paths.push(path(&[v]));
        }
        let l = RadialLayout::from_paths(&paths, sun());
        assert_eq!(
            l.folded_levels(),
            0,
            "the fixture must not fold, or it measures nothing"
        );

        let angle = |i: usize| l.ys()[i].atan2(l.xs()[i]).rem_euclid(std::f32::consts::TAU);
        let wedge = std::f32::consts::TAU * 4.0 / 11.0;

        // Occupancy split: the 3-node group spans [0, 0.75·wedge), centred at
        // 0.375·wedge. An EQUAL split of two children would span [0, 0.5·wedge)
        // and centre at 0.25·wedge — which is what this assertion rules out.
        let big = angle(0);
        assert!(
            (big - 0.375 * wedge).abs() < 0.05,
            "3-node wedge centred at {big:.3} rad, expected {:.3}; an equal \
             two-way split would centre it at {:.3}",
            0.375 * wedge,
            0.25 * wedge
        );
        // …and the 1-node group takes the remaining quarter, centred at
        // 0.875·wedge (an equal split would put it at 0.75·wedge).
        let small = angle(3);
        assert!(
            (small - 0.875 * wedge).abs() < 0.05,
            "1-node wedge centred at {small:.3} rad, expected {:.3}",
            0.875 * wedge
        );
        assert!(big < small, "sorted order must survive the sweep");
    }

    /// The fold must not merge subtrees. Two nodes under DIFFERENT folded
    /// roots stay separated in arc; two nodes differing only in a folded
    /// level still share a ring.
    #[test]
    fn folding_costs_radius_but_never_ordering() {
        // Level 0 carries ~0 bits (one value dominates 90 %), level 1 is
        // rich — exactly the shipped corpus's shape.
        let mut paths = Vec::new();
        for v in 1..=9u8 {
            paths.push(path(&[1, v]));
        }
        paths.push(path(&[2, 1]));

        let l = RadialLayout::from_paths(&paths, sun());
        assert_eq!(l.folded_levels(), 1, "level 0 must fold, level 1 must not");

        // A folded level costs no ring: every node here is at taxonomy
        // depth 2 but only ONE unfolded level was traversed.
        for i in 0..paths.len() {
            assert_eq!(l.ring(i), 1, "node {i} should sit on ring 1, not 2");
        }
        // …and the lone level-0 = 2 node is still ordered apart: it is the
        // last group in the sweep, so its angle exceeds every `1`-rooted one.
        let angle = |i: usize| l.ys()[i].atan2(l.xs()[i]).rem_euclid(std::f32::consts::TAU);
        let outsider = angle(9);
        assert!(
            (0..9).all(|i| angle(i) < outsider),
            "the folded root still orders the arc; it just earns no radius"
        );
    }

    #[test]
    fn an_informative_first_level_is_not_folded() {
        // Eight roots, one node each: 3 bits, comfortably over FOLD_BITS.
        let paths: Vec<RailPath> = (1..=8u8).map(|v| path(&[v, 1])).collect();
        let l = RadialLayout::from_paths(&paths, sun());
        assert_eq!(l.folded_levels(), 0, "3 bits must survive the fold rule");
        assert!((0..8).all(|i| l.ring(i) == 2), "two levels, two rings");
    }

    /// The innermost ring must still be a RING.
    ///
    /// This exists because the fix it guards shipped unguarded: computing
    /// the radius as `ring · step` puts every ring-0 node at exactly the
    /// origin, so its angle is computed correctly and then thrown away.
    /// On the live corpus that stacked 93 root concepts on one pixel, and
    /// once the head levels fold it can swallow the whole graph — the
    /// failure looks like "the layout did nothing", not like a bug.
    #[test]
    fn ring_zero_nodes_do_not_collapse_onto_the_origin() {
        // Level 0 carries 0.47 bits, so it folds and these depth-1 nodes
        // land on ring 0 — distinct addresses, therefore distinct places.
        let mut paths = vec![path(&[1]); 9];
        paths.push(path(&[2]));
        let l = RadialLayout::from_paths(&paths, sun());
        assert_eq!(
            l.folded_levels(),
            1,
            "the fixture must fold, or ring 0 is empty"
        );
        assert_eq!(l.ring(0), 0);
        assert_eq!(l.ring(9), 0);

        let r = |i: usize| l.xs()[i].hypot(l.ys()[i]);
        assert!(
            r(0) > 0.0,
            "a ring-0 node sits ON the innermost ring, not at its centre"
        );
        assert!(
            (l.xs()[0] - l.xs()[9]).abs() + (l.ys()[0] - l.ys()[9]).abs() > 1.0,
            "two different addresses must not share a coordinate"
        );
    }

    /// "Kein Solver, zwei Abrufe rendern identisch" — the contract's own
    /// claim, made falsifiable.
    #[test]
    fn two_builds_of_the_same_paths_are_bit_identical() {
        let paths: Vec<RailPath> = (1..=40u8).map(|v| path(&[v % 7 + 1, v, v / 3])).collect();
        let a = RadialLayout::from_paths(&paths, RadialParams::default());
        let b = RadialLayout::from_paths(&paths, RadialParams::default());
        assert_eq!(a.xs(), b.xs());
        assert_eq!(a.ys(), b.ys());
    }

    /// The secondary axis OVERLAYS: it displaces within the ring and must
    /// never reach the next one, or depth stops being readable.
    #[test]
    fn the_overlay_never_crosses_into_the_neighbouring_ring() {
        let p = sun();
        let saturated = decode_rail(&entry(&[3, 1], &[255; 12])).expect("decodes");
        let bare = path(&[3, 1]);
        let l = RadialLayout::from_paths(&[bare, saturated], p);
        let r = |i: usize| l.xs()[i].hypot(l.ys()[i]);
        assert!(r(1) > r(0), "a populated secondary axis must displace");
        assert!(
            r(1) - r(0) < p.ring,
            "overlay {} must stay inside one ring step {}",
            r(1) - r(0),
            p.ring
        );
    }

    #[test]
    fn romanesco_is_the_default_style() {
        assert_eq!(RadialParams::default().style, RadialStyle::Romanesco);
    }

    /// Area proportionality: a subtree with 4x the nodes gets a disc 2x the
    /// radius, because area is what must scale, not radius.
    ///
    /// Measured through the leaves rather than through an internal radius:
    /// the spread of a subtree's own nodes IS its disc.
    #[test]
    fn a_child_disc_scales_with_the_square_root_of_its_share() {
        // Root 1 carries 64 nodes, root 2 carries 16 — a 4:1 share, so the
        // discs should differ 2:1, not 4:1.
        let mut paths = Vec::new();
        for i in 0..64u8 {
            paths.push(path(&[1, i / 8 + 1, i % 8 + 1]));
        }
        for i in 0..16u8 {
            paths.push(path(&[2, i / 4 + 1, i % 4 + 1]));
        }
        let l = RadialLayout::from_paths(&paths, RadialParams::default());
        let spread = |from: usize, to: usize| {
            let (mut cx, mut cy) = (0.0f32, 0.0f32);
            for i in from..to {
                cx += l.xs()[i];
                cy += l.ys()[i];
            }
            let n = (to - from) as f32;
            let (cx, cy) = (cx / n, cy / n);
            (
                (from..to)
                    .map(|i| (l.xs()[i] - cx).hypot(l.ys()[i] - cy))
                    .fold(0.0, f32::max),
                n,
            )
        };
        let (big, _) = spread(0, 64);
        let (small, _) = spread(64, 80);
        let ratio = big / small;
        assert!(
            (ratio - 2.0).abs() < 0.45,
            "4:1 share should give a ~2:1 disc (sqrt), measured {ratio:.2}; a \
             linear share would give ~4.0 and an equal one ~1.0"
        );
    }

    /// The golden angle is load-bearing, not decorative.
    ///
    /// Any RATIONAL fraction of a turn puts items back on the same spokes
    /// after its denominator, leaving wedge-shaped gaps and stacked nodes.
    /// The golden ratio is the irrational hardest to approximate by a
    /// rational, so no two items ever share a spoke at any count.
    #[test]
    fn siblings_never_land_on_the_same_spoke() {
        // 24 siblings under one parent: enough that a rational step of
        // TAU/8 or TAU/12 would have wrapped onto itself twice.
        let paths: Vec<RailPath> = (1..=24u8).map(|v| path(&[3, v])).collect();
        let l = RadialLayout::from_paths(&paths, RadialParams::default());
        // About the subtree's own centre — siblings spiral around their
        // PARENT, so an angle taken from the origin is not their spoke.
        let (mut cx, mut cy) = (0.0f32, 0.0f32);
        for i in 0..paths.len() {
            cx += l.xs()[i];
            cy += l.ys()[i];
        }
        let n = paths.len() as f32;
        let (cx, cy) = (cx / n, cy / n);
        let mut angles: Vec<f32> = (0..paths.len())
            .map(|i| {
                (l.ys()[i] - cy)
                    .atan2(l.xs()[i] - cx)
                    .rem_euclid(std::f32::consts::TAU)
            })
            .collect();
        angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let closest = angles
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(f32::MAX, f32::min);
        assert!(
            closest > 0.05,
            "closest sibling spoke gap {closest:.4} rad — a rational angle \
             step collapses siblings onto shared spokes"
        );
        // …and no two share a coordinate at all.
        for i in 0..paths.len() {
            for j in i + 1..paths.len() {
                assert!(
                    (l.xs()[i] - l.xs()[j]).hypot(l.ys()[i] - l.ys()[j]) > 1e-3,
                    "nodes {i} and {j} are stacked"
                );
            }
        }
    }

    /// Each subtree inherits its parent's angle as its spiral phase. Reset
    /// it to zero and every subtree starts due east, which reads as one
    /// bright seam straight through the field.
    #[test]
    fn a_subtree_inherits_its_parents_phase_rather_than_starting_due_east() {
        // Four sibling subtrees, each with several children. If the phase
        // were reset, each subtree's FIRST child would sit at angle 0
        // relative to its own centre — all four pointing the same way.
        let mut paths = Vec::new();
        for r in 1..=4u8 {
            for c in 1..=6u8 {
                paths.push(path(&[9, r, c]));
            }
        }
        let l = RadialLayout::from_paths(&paths, RadialParams::default());
        // Direction from each subtree's centroid to its first child.
        let first_dirs: Vec<f32> = (0..4)
            .map(|r| {
                let (from, to) = (r * 6, r * 6 + 6);
                let (mut cx, mut cy) = (0.0f32, 0.0f32);
                for i in from..to {
                    cx += l.xs()[i];
                    cy += l.ys()[i];
                }
                let (cx, cy) = (cx / 6.0, cy / 6.0);
                (l.ys()[from] - cy).atan2(l.xs()[from] - cx)
            })
            .collect();
        let spread = first_dirs
            .iter()
            .flat_map(|a| first_dirs.iter().map(move |b| (a - b).abs()))
            .fold(0.0f32, f32::max);
        assert!(
            spread > 0.3,
            "the four subtrees all open in the same direction (spread \
             {spread:.3} rad) — the phase was reset instead of inherited"
        );
    }

    /// The sunburst is still reachable and still means what it meant.
    #[test]
    fn the_sunburst_style_still_places_depth_as_distance() {
        // Eight roots so level 0 clears FOLD_BITS — otherwise everything
        // folds onto ring 0 and there is no depth left to measure.
        let mut paths = vec![path(&[1]), path(&[1, 1]), path(&[1, 1, 1])];
        for v in 2..=8u8 {
            paths.push(path(&[v]));
        }
        let l = RadialLayout::from_paths(&paths, sun());
        let r = |i: usize| l.xs()[i].hypot(l.ys()[i]);
        assert!(r(0) < r(1) && r(1) < r(2), "depth must grow outward");
    }

    #[test]
    fn an_empty_layout_has_no_bounds_rather_than_a_zero_box() {
        let l = RadialLayout::from_paths(&[], RadialParams::default());
        assert!(l.is_empty());
        assert!(l.bounds().is_none());
    }
}
