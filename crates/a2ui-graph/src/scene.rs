//! The scene: ABI lanes + layout → the exact buffers the GPU binds, plus the
//! interaction grammar that answers by ORDINAL ADDRESS.
//!
//! # What makes this cheap
//!
//! Interaction never rebuilds geometry. Dimming, spreading and tracing write
//! **one f32 per node** into the alpha lane; positions are the layout's own
//! arrays. So a selection change re-uploads `n * 4` bytes, not a scene.
//!
//! That is the structural difference from a retained-mode graph view, where
//! the same operation walks n objects and touches their style. It is why the
//! field can carry 10^4–10^6 primitives and a form renderer cannot.
//!
//! # Addresses, never handlers
//!
//! Every query returns an **ordinal** (an index into the node lane), and the
//! consumer resolves it through the ABI view to `(classid, identity)`. The
//! scene knows no callbacks, no node objects, no domain meaning — the same
//! charter the FieldView paint tier follows for clicks.

use crate::{GraphAbi, Layout};

/// Alpha of a node the current selection has pushed to the background.
///
/// **Dim, never delete.** The surrounding universe stays on screen and stays
/// addressable; removing it would answer "what is near this?" by destroying
/// the evidence. Ported from the interaction grammar this crate replaces.
pub const DIM_ALPHA: f32 = 0.12;
/// Alpha of a node the selection lit.
pub const LIT_ALPHA: f32 = 1.0;
/// Alpha when nothing is selected — the resting field.
pub const REST_ALPHA: f32 = 0.85;

/// One node's per-instance record. `#[repr(C)]` + `Pod` so the lane casts to
/// bytes with the one sanctioned `bytemuck` cast and the crate keeps
/// `forbid(unsafe_code)`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodeInstance {
    /// Layout position.
    pub pos: [f32; 2],
    /// Ring radius in layout units, derived from degree.
    pub radius: f32,
    /// Current alpha — the only field interaction writes.
    pub alpha: f32,
    /// The OPAQUE domain byte, widened for the vertex format. The shader
    /// indexes a palette with it; neither shader nor crate interprets it.
    pub palette: u32,
    /// Selection state: 0 rest, 1 lit, 2 pinned. Drives the ring's stroke.
    pub state: u32,
}

/// A directed edge as a pair of node ordinals — used verbatim as the index
/// buffer of a `LineList` draw. No per-edge object, no geometry expansion on
/// the CPU: the line's two endpoints ARE two nodes' positions.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EdgeIndex {
    /// Source node ordinal.
    pub from: u32,
    /// Target node ordinal.
    pub to: u32,
}

/// The scene's CPU-side state.
pub struct Scene {
    /// Per-node instances, in node-lane order. Uploaded as one buffer.
    pub nodes: Vec<NodeInstance>,
    /// Edge endpoints, in ABI order minus ghosts. Uploaded as one index buffer.
    pub edges: Vec<EdgeIndex>,
    /// CSR row offsets into [`Scene::adj`], length `n + 1`.
    row: Vec<u32>,
    /// CSR neighbour ordinals — the adjacency a spread walks.
    adj: Vec<u32>,
    /// The current selection, as ordinals. Empty means the resting field.
    selection: Vec<u32>,
}

impl Scene {
    /// Build from a parsed ABI view and its settled layout.
    ///
    /// # Panics
    /// If `layout` was not built from `abi` (different node counts) — a
    /// mismatch would silently draw one graph at another's positions.
    #[must_use]
    pub fn build(abi: &GraphAbi<'_>, layout: &Layout) -> Self {
        assert_eq!(
            abi.node_count(),
            layout.len(),
            "layout and ABI describe different graphs"
        );
        let n = abi.node_count();
        let degrees = abi.degrees();
        let pairs = abi.edge_pairs();

        let nodes = (0..n)
            .map(|i| NodeInstance {
                pos: [layout.xs[i], layout.ys[i]],
                // sqrt so a hub of degree 100 is ~3x the radius of degree 10,
                // not 10x: area, not length, carries the impression of size.
                radius: 3.0 + (degrees[i] as f32).sqrt() * 1.6,
                alpha: REST_ALPHA,
                palette: u32::from(abi.domain(i)),
                state: 0,
            })
            .collect();

        // CSR, built by counting then scattering — two linear passes, no
        // per-node Vec. This is the structure `spread` walks, so building it
        // once here is what keeps a spread from being a scan over all edges.
        let mut row = vec![0u32; n + 1];
        for &[f, t] in &pairs {
            row[f as usize + 1] += 1;
            row[t as usize + 1] += 1;
        }
        for i in 0..n {
            row[i + 1] += row[i];
        }
        let mut cursor = row.clone();
        let mut adj = vec![0u32; row[n] as usize];
        for &[f, t] in &pairs {
            adj[cursor[f as usize] as usize] = t;
            cursor[f as usize] += 1;
            adj[cursor[t as usize] as usize] = f;
            cursor[t as usize] += 1;
        }

        Scene {
            nodes,
            edges: pairs
                .into_iter()
                .map(|[from, to]| EdgeIndex { from, to })
                .collect(),
            row,
            adj,
            selection: Vec::new(),
        }
    }

    /// Refresh positions from the layout after a step — the per-frame upload.
    /// Only `pos` changes; alpha/state/palette survive, so a drag does not
    /// drop the current selection.
    pub fn sync_positions(&mut self, layout: &Layout) {
        for (i, nd) in self.nodes.iter_mut().enumerate() {
            nd.pos = [layout.xs[i], layout.ys[i]];
        }
    }

    /// The instance lane as bytes — what `queue.write_buffer` takes.
    #[must_use]
    pub fn node_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.nodes)
    }
    /// The index lane as bytes.
    #[must_use]
    pub fn edge_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.edges)
    }

    /// The neighbours of a node — one CSR row, borrowed.
    #[must_use]
    pub fn neighbours(&self, i: usize) -> &[u32] {
        let (a, b) = (self.row[i] as usize, self.row[i + 1] as usize);
        &self.adj[a..b]
    }
}

impl Scene {
    /// Clear the selection — back to the resting field, nothing dimmed.
    pub fn clear(&mut self) {
        self.selection.clear();
        for nd in &mut self.nodes {
            nd.alpha = REST_ALPHA;
            nd.state = if nd.state == 2 { 2 } else { 0 };
        }
    }

    /// Light exactly this set and dim everything else.
    ///
    /// The dimmed nodes stay in the buffers and stay hit-testable — the
    /// "preserve the surrounding universe" rule, enforced by construction
    /// because there is no code path here that removes an instance.
    pub fn light(&mut self, lit: &[u32]) {
        self.selection = lit.to_vec();
        for nd in &mut self.nodes {
            nd.alpha = DIM_ALPHA;
            if nd.state == 1 {
                nd.state = 0;
            }
        }
        for &i in lit {
            if let Some(nd) = self.nodes.get_mut(i as usize) {
                nd.alpha = LIT_ALPHA;
                nd.state = 1;
            }
        }
    }

    /// Bounded breadth-first spread from `seed`, `hops` deep — "show me the
    /// neighbourhood" without loading the whole component.
    ///
    /// Returns the lit set in BFS order (seed first), which is also the order
    /// a readout should narrate it.
    pub fn spread(&mut self, seed: u32, hops: u32) -> Vec<u32> {
        let n = self.nodes.len();
        if seed as usize >= n {
            return Vec::new();
        }
        let mut seen = vec![false; n];
        let mut out = vec![seed];
        seen[seed as usize] = true;
        let mut frontier = vec![seed];
        for _ in 0..hops {
            let mut next = Vec::new();
            for &f in &frontier {
                for &nb in self.neighbours(f as usize) {
                    if !seen[nb as usize] {
                        seen[nb as usize] = true;
                        out.push(nb);
                        next.push(nb);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        self.light(&out);
        out
    }

    /// The shortest path between two nodes — the TRACE.
    ///
    /// Breadth-first, so the path is shortest by hop count, and deterministic
    /// because the CSR rows are in build order. `None` when the two sit in
    /// different components: an honest "no path" beats a plausible detour.
    pub fn trace(&mut self, from: u32, to: u32) -> Option<Vec<u32>> {
        let n = self.nodes.len();
        if from as usize >= n || to as usize >= n {
            return None;
        }
        let mut prev = vec![u32::MAX; n];
        let mut seen = vec![false; n];
        seen[from as usize] = true;
        let mut q = std::collections::VecDeque::from([from]);
        while let Some(cur) = q.pop_front() {
            if cur == to {
                let mut path = vec![to];
                let mut p = to;
                while p != from {
                    p = prev[p as usize];
                    path.push(p);
                }
                path.reverse();
                self.light(&path);
                return Some(path);
            }
            for &nb in self.neighbours(cur as usize) {
                if !seen[nb as usize] {
                    seen[nb as usize] = true;
                    prev[nb as usize] = cur;
                    q.push_back(nb);
                }
            }
        }
        None
    }

    /// Light every node whose byte on `axis` equals `value` — the facet
    /// filter. The axis is read from the ABI view, so the scene still knows
    /// nothing about what a domain or an evidence role MEANS.
    pub fn facet(&mut self, abi: &GraphAbi<'_>, axis: Facet, value: u8) -> Vec<u32> {
        let lit: Vec<u32> = (0..self.nodes.len())
            .filter(|&i| {
                value
                    == match axis {
                        Facet::Domain => abi.domain(i),
                        Facet::Vocab => abi.vocab(i),
                        Facet::Evidence => abi.evidence(i),
                    }
            })
            .map(|i| i as u32)
            .collect();
        self.light(&lit);
        lit
    }

    /// Mark a node as held, so the ring draws its grab state.
    pub fn set_pinned(&mut self, i: u32, pinned: bool) {
        if let Some(nd) = self.nodes.get_mut(i as usize) {
            nd.state = if pinned {
                2
            } else if self.selection.contains(&i) {
                1
            } else {
                0
            };
        }
    }

    /// The current selection, as ordinals.
    #[must_use]
    pub fn selection(&self) -> &[u32] {
        &self.selection
    }
}

/// Which opaque node byte a facet filter reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facet {
    /// The semantic domain byte (node lane offset 12).
    Domain,
    /// The vocabulary/projection byte (offset 8).
    Vocab,
    /// The evidence-role byte (offset 13).
    Evidence,
}

/// The shared fixture, reachable from the GPU tests too — one graph, so a
/// CPU assertion and a pixel assertion are talking about the same thing.
#[cfg(test)]
pub(crate) mod tests_support {
    /// A path graph 0-1-2-3-4 plus an isolated node 5, with distinct domain
    /// bytes — enough shape to make every assertion falsifiable.
    pub(crate) fn fixture() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"MGRA");
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&6u32.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        for i in 0..6u32 {
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&i.to_le_bytes());
            b.extend_from_slice(&[1, 0, 0, 0]);
            b.push(if i < 3 { 1 } else { 2 }); // domain
            b.push(0);
            b.extend_from_slice(&0u16.to_le_bytes());
        }
        for (f, t) in [(0u32, 1u32), (1, 2), (2, 3), (3, 4)] {
            b.extend_from_slice(&f.to_le_bytes());
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&[0, 0, 0, 1]);
        }
        b.extend_from_slice(b"MGL1");
        for n in ["a", "b", "c", "d", "e", "lonely"] {
            b.extend_from_slice(&(n.len() as u16).to_le_bytes());
            b.extend_from_slice(n.as_bytes());
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::fixture;

    fn scene(buf: &[u8]) -> (GraphAbi<'_>, Scene) {
        let abi = GraphAbi::parse(buf).expect("parses");
        let l = Layout::from_abi(&abi);
        let s = Scene::build(&abi, &l);
        (abi, s)
    }

    #[test]
    fn instances_and_indices_come_out_in_lane_order() {
        let buf = fixture();
        let (_, s) = scene(&buf);
        assert_eq!(s.nodes.len(), 6);
        assert_eq!(s.edges.len(), 4);
        assert_eq!(s.edges[0], EdgeIndex { from: 0, to: 1 });
        // The palette IS the domain byte, uninterpreted.
        assert_eq!(s.nodes[0].palette, 1);
        assert_eq!(s.nodes[5].palette, 2);
        // A degree-0 node still gets a drawable radius.
        assert!(s.nodes[5].radius > 0.0);
        // And the byte lanes are the real buffers, not a re-encode.
        assert_eq!(s.node_bytes().len(), 6 * std::mem::size_of::<NodeInstance>());
        assert_eq!(s.edge_bytes().len(), 4 * std::mem::size_of::<EdgeIndex>());
    }

    #[test]
    fn csr_neighbours_are_undirected_and_complete() {
        let buf = fixture();
        let (_, s) = scene(&buf);
        assert_eq!(s.neighbours(0), &[1]);
        let mut mid = s.neighbours(2).to_vec();
        mid.sort_unstable();
        assert_eq!(mid, vec![1, 3], "an edge is walkable from both ends");
        assert!(s.neighbours(5).is_empty(), "the isolated node has none");
    }

    /// Dim, never delete — the rule the whole grammar rests on.
    /// CAN FIRE: an implementation that filtered the instance buffer would
    /// pass every "the right nodes are lit" test and fail this one.
    #[test]
    fn dimming_keeps_every_node_in_the_buffers() {
        let buf = fixture();
        let (_, mut s) = scene(&buf);
        let before = s.nodes.len();
        s.light(&[0, 1]);
        assert_eq!(s.nodes.len(), before, "nodes were REMOVED, not dimmed");
        assert_eq!(s.edges.len(), 4, "edges were removed too");
        assert_eq!(s.nodes[0].alpha, LIT_ALPHA);
        assert_eq!(s.nodes[4].alpha, DIM_ALPHA);
        assert!(s.nodes[4].alpha > 0.0, "a dimmed node must stay visible");
        // …and clearing restores the resting field.
        s.clear();
        assert!(s.nodes.iter().all(|n| n.alpha == REST_ALPHA));
    }

    #[test]
    fn spread_is_bounded_by_its_hop_count() {
        let buf = fixture();
        let (_, mut s) = scene(&buf);
        assert_eq!(s.spread(0, 1), vec![0, 1], "one hop is one hop");
        assert_eq!(s.spread(0, 2), vec![0, 1, 2]);
        // CAN STAY SILENT: more hops than the graph has does not wrap around
        // or duplicate; it stops when the frontier is empty.
        assert_eq!(s.spread(0, 99), vec![0, 1, 2, 3, 4]);
        assert_eq!(s.spread(5, 3), vec![5], "isolated stays alone");
    }

    #[test]
    fn trace_finds_the_shortest_path_and_admits_when_there_is_none() {
        let buf = fixture();
        let (_, mut s) = scene(&buf);
        assert_eq!(s.trace(0, 4), Some(vec![0, 1, 2, 3, 4]));
        assert_eq!(s.trace(2, 2), Some(vec![2]), "a node traces to itself");
        // CAN FIRE: a path-finder that returned a plausible detour instead of
        // None would light nodes that are not connected to the target at all.
        assert_eq!(s.trace(0, 5), None, "different components have no path");
        assert_eq!(s.trace(0, 99), None, "an out-of-range ordinal is not a path");
    }

    #[test]
    fn a_facet_lights_exactly_the_matching_byte() {
        let buf = fixture();
        let (abi, mut s) = scene(&buf);
        assert_eq!(s.facet(&abi, Facet::Domain, 1), vec![0, 1, 2]);
        assert_eq!(s.facet(&abi, Facet::Domain, 2), vec![3, 4, 5]);
        // CAN STAY SILENT: a value nothing carries lights nothing — and the
        // nodes are all still there, dimmed.
        assert!(s.facet(&abi, Facet::Domain, 200).is_empty());
        assert_eq!(s.nodes.len(), 6);
    }

    /// A drag must not silently drop the selection — the two states are
    /// independent and both have to survive a position sync.
    #[test]
    fn syncing_positions_preserves_selection_state() {
        let buf = fixture();
        let (abi, mut s) = scene(&buf);
        let mut l = Layout::from_abi(&abi);
        s.light(&[1, 2]);
        l.settle(5);
        s.sync_positions(&l);
        assert_eq!(s.nodes[1].alpha, LIT_ALPHA);
        assert_eq!(s.nodes[0].alpha, DIM_ALPHA);
        assert_eq!(s.nodes[1].pos, [l.xs[1], l.ys[1]], "position did move");
    }
}
