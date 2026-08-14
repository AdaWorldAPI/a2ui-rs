//! Zero-copy view over the graph-ABI **v3** wire (`"MGRA"`).
//!
//! The wire is little-endian bytes: a 16-byte header, a 16-byte node lane, a
//! 12-byte edge lane, then tail lanes dispatched by their own 4-byte magic.
//! This module never builds a per-node object: every accessor reads at an
//! offset into the ONE byte slice the caller fetched. What it does build are
//! **indices** (per-node `&str` slices into the label lane) — access
//! structures over borrowed content, not copies of it.
//!
//! # Version gate
//!
//! This viewer reads EXACTLY version 3 and refuses everything else loudly.
//! Reserve-don't-reclaim: only the version may declare a byte consulted, so a
//! v2 stream under a v3 reader (or the reverse) must be an error, never a
//! silent reinterpretation.
//!
//! # Lane layout (v3)
//!
//! ```text
//! header 16 B : magic "MGRA" | version u16 | flags u16 | node_count u32 | edge_count u32
//! node   16 B : classid u32 @0 | identity u32 @4 | vocab u8 @8 | role u8 @9
//!               | flags u8 @10 | (reserved @11) | domain u8 @12 | evidence u8 @13
//!               | reserved u16 @14
//! edge   12 B : from u32 @0 | to u32 @4 | kind u8 @8 | role u8 @9
//!               | flags u8 @10 | predicate u8 @11
//! tails       : "MGL1" labels (always) · "MGT1" titles (flags bit1)
//!               · "MGR1" rails, 24 B/node (bit2) · "MGC1" curies (bit3)
//!               — dispatched by magic, so the order on the wire is not a
//!               contract this reader depends on.
//! text lane   : 4 B magic, then per node: u16 len | len bytes (UTF-8)
//! ```
//!
//! `domain` and `vocab` are OPAQUE palette/codebook indices here. What a
//! domain *means* belongs to the consumer's codebook — this crate colors by
//! the byte and never interprets it (consumer-agnostic, the paint charter).

/// The 4-byte stream magic.
pub const MAGIC: [u8; 4] = *b"MGRA";
/// The one version this viewer reads.
pub const WIRE_VERSION: u16 = 3;
/// Header length in bytes.
pub const HEADER_LEN: usize = 16;
/// Node record length in bytes.
pub const NODE_LEN: usize = 16;
/// Edge record length in bytes.
pub const EDGE_LEN: usize = 12;
/// Rails record length per node (two stacked 6x(hi:lo) registers).
pub const RAIL_LEN: usize = 24;

const LABEL_MAGIC: [u8; 4] = *b"MGL1";
const TITLE_MAGIC: [u8; 4] = *b"MGT1";
const RAIL_MAGIC: [u8; 4] = *b"MGR1";
const CURIE_MAGIC: [u8; 4] = *b"MGC1";

/// Why a byte stream was refused. Every variant names the place, because a
/// truncated stream that fails with "too short" and nothing else costs a
/// debugging session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiError {
    /// Shorter than one header.
    TooShort,
    /// First four bytes are not `MGRA`.
    BadMagic([u8; 4]),
    /// The version gate: carries what the stream claimed.
    WrongVersion(u16),
    /// A fixed lane runs past the end of the buffer.
    Truncated(&'static str),
    /// A text lane's length prefix points past the end.
    TextOverrun(&'static str),
    /// A text lane carries bytes that are not UTF-8.
    NotUtf8(&'static str),
    /// A tail magic this version does not know — loud, never skipped:
    /// skipping would need the lane's length, which an unknown lane by
    /// definition does not declare.
    UnknownLane([u8; 4]),
}

impl core::fmt::Display for AbiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AbiError::TooShort => write!(f, "stream shorter than one header"),
            AbiError::BadMagic(m) => write!(f, "bad magic {m:?} (want MGRA)"),
            AbiError::WrongVersion(v) => {
                write!(
                    f,
                    "wire version {v}, this viewer reads exactly {WIRE_VERSION}"
                )
            }
            AbiError::Truncated(lane) => write!(f, "lane truncated: {lane}"),
            AbiError::TextOverrun(lane) => write!(f, "text length overruns stream: {lane}"),
            AbiError::NotUtf8(lane) => write!(f, "text lane is not UTF-8: {lane}"),
            AbiError::UnknownLane(m) => write!(f, "unknown tail lane magic {m:?}"),
        }
    }
}

impl std::error::Error for AbiError {}

/// The parsed view: borrowed lanes + per-node text indices. Nothing owned but
/// the index vectors themselves.
pub struct GraphAbi<'a> {
    node_lane: &'a [u8],
    edge_lane: &'a [u8],
    node_count: usize,
    edge_count: usize,
    flags: u16,
    labels: Vec<&'a str>,
    titles: Vec<&'a str>,
    curies: Vec<&'a str>,
    rails: Option<&'a [u8]>,
}

#[inline]
fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
#[inline]
fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Read one text lane: `magic(4)` then `count` records of `u16 len | bytes`.
/// Returns the slices and how many bytes the lane consumed.
fn read_text_lane<'a>(
    buf: &'a [u8],
    at: usize,
    count: usize,
    lane: &'static str,
) -> Result<(Vec<&'a str>, usize), AbiError> {
    let mut p = at + 4;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if p + 2 > buf.len() {
            return Err(AbiError::Truncated(lane));
        }
        let len = u16le(buf, p) as usize;
        p += 2;
        if p + len > buf.len() {
            return Err(AbiError::TextOverrun(lane));
        }
        out.push(core::str::from_utf8(&buf[p..p + len]).map_err(|_| AbiError::NotUtf8(lane))?);
        p += len;
    }
    Ok((out, p - at))
}

impl<'a> GraphAbi<'a> {
    /// Parse a v3 stream. Borrows `buf` for the view's whole life; the only
    /// allocations are the text indices (one `&str` per node per present
    /// lane), never the text itself.
    pub fn parse(buf: &'a [u8]) -> Result<Self, AbiError> {
        if buf.len() < HEADER_LEN {
            return Err(AbiError::TooShort);
        }
        let magic: [u8; 4] = [buf[0], buf[1], buf[2], buf[3]];
        if magic != MAGIC {
            return Err(AbiError::BadMagic(magic));
        }
        // The gate, before ANY offset below is trusted: every offset in this
        // function is a v3 offset, so reading them out of a v2 stream would
        // be the silent reinterpretation `I-LEGACY-API-FEATURE-GATED` forbids.
        let version = u16le(buf, 4);
        if version != WIRE_VERSION {
            return Err(AbiError::WrongVersion(version));
        }
        let flags = u16le(buf, 6);
        let node_count = u32le(buf, 8) as usize;
        let edge_count = u32le(buf, 12) as usize;

        let nodes_end = HEADER_LEN + node_count * NODE_LEN;
        let edges_end = nodes_end + edge_count * EDGE_LEN;
        if buf.len() < edges_end {
            return Err(AbiError::Truncated("fixed lanes"));
        }
        let node_lane = &buf[HEADER_LEN..nodes_end];
        let edge_lane = &buf[nodes_end..edges_end];

        // Tails by MAGIC, not by declared order: the flags say a lane is
        // present, the magic says which one is next. A reader that trusted
        // the order would break the first time a producer reorders lanes it
        // is entitled to reorder.
        let (mut labels, mut titles, mut curies, mut rails) =
            (Vec::new(), Vec::new(), Vec::new(), None);
        let mut p = edges_end;
        while p + 4 <= buf.len() {
            let m: [u8; 4] = [buf[p], buf[p + 1], buf[p + 2], buf[p + 3]];
            match m {
                LABEL_MAGIC => {
                    let (v, used) = read_text_lane(buf, p, node_count, "MGL1 labels")?;
                    labels = v;
                    p += used;
                }
                TITLE_MAGIC => {
                    let (v, used) = read_text_lane(buf, p, node_count, "MGT1 titles")?;
                    titles = v;
                    p += used;
                }
                CURIE_MAGIC => {
                    let (v, used) = read_text_lane(buf, p, node_count, "MGC1 curies")?;
                    curies = v;
                    p += used;
                }
                RAIL_MAGIC => {
                    let end = p + 4 + node_count * RAIL_LEN;
                    if end > buf.len() {
                        return Err(AbiError::Truncated("MGR1 rails"));
                    }
                    rails = Some(&buf[p + 4..end]);
                    p = end;
                }
                other => return Err(AbiError::UnknownLane(other)),
            }
        }

        Ok(GraphAbi {
            node_lane,
            edge_lane,
            node_count,
            edge_count,
            flags,
            labels,
            titles,
            curies,
            rails,
        })
    }
}

impl<'a> GraphAbi<'a> {
    /// How many nodes the header declared (and the lanes carry).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.node_count
    }
    /// How many edges the header declared.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }
    /// The raw header flags — which optional tails the producer wrote.
    #[must_use]
    pub fn flags(&self) -> u16 {
        self.flags
    }

    #[inline]
    fn node_at(&self, i: usize) -> &'a [u8] {
        &self.node_lane[i * NODE_LEN..(i + 1) * NODE_LEN]
    }
    #[inline]
    fn edge_at(&self, i: usize) -> &'a [u8] {
        &self.edge_lane[i * EDGE_LEN..(i + 1) * EDGE_LEN]
    }

    /// The node's ADDRESS: `(classid, identity)`. This is the identity the
    /// consumer resolves — this crate never interprets it.
    #[must_use]
    pub fn address(&self, i: usize) -> (u32, u32) {
        let r = self.node_at(i);
        (u32le(r, 0), u32le(r, 4))
    }
    /// The domain byte — an OPAQUE palette index. The field colors by it and
    /// never asks what it means.
    #[must_use]
    pub fn domain(&self, i: usize) -> u8 {
        self.node_at(i)[12]
    }
    /// The evidence byte — a second opaque axis (facet filtering).
    #[must_use]
    pub fn evidence(&self, i: usize) -> u8 {
        self.node_at(i)[13]
    }
    /// The vocabulary byte — the projection the address is read through.
    #[must_use]
    pub fn vocab(&self, i: usize) -> u8 {
        self.node_at(i)[8]
    }
    /// The node's label, or `""` when the lane is absent. Borrowed from the
    /// stream — no allocation, no copy.
    #[must_use]
    pub fn label(&self, i: usize) -> &'a str {
        self.labels.get(i).copied().unwrap_or("")
    }
    /// The node's hover title, `""` when the lane is absent.
    #[must_use]
    pub fn title(&self, i: usize) -> &'a str {
        self.titles.get(i).copied().unwrap_or("")
    }
    /// The node's CURIE, `""` when the lane is absent.
    #[must_use]
    pub fn curie(&self, i: usize) -> &'a str {
        self.curies.get(i).copied().unwrap_or("")
    }
    /// The node's 24-byte rail register, `None` when the lane is absent.
    #[must_use]
    pub fn rail(&self, i: usize) -> Option<&'a [u8]> {
        self.rails.map(|r| &r[i * RAIL_LEN..(i + 1) * RAIL_LEN])
    }

    /// An edge as `(from_ordinal, to_ordinal, predicate)`. The ordinals index
    /// the node lane directly — which is exactly what an index buffer wants,
    /// so the edge lane feeds the GPU with no translation table.
    #[must_use]
    pub fn edge(&self, i: usize) -> (u32, u32, u8) {
        let r = self.edge_at(i);
        (u32le(r, 0), u32le(r, 4), r[11])
    }
    /// The edge's renderer class (line style), distinct from its predicate.
    #[must_use]
    pub fn edge_kind(&self, i: usize) -> u8 {
        self.edge_at(i)[8]
    }

    /// Every edge whose BOTH endpoints are inside the node lane.
    ///
    /// A ghost edge (an ordinal past `node_count`) would index a GPU buffer
    /// out of bounds, so it is dropped HERE, once, rather than guarded at
    /// every draw. Returned as `(from, to)` pairs ready to become an index
    /// buffer.
    #[must_use]
    pub fn edge_pairs(&self) -> Vec<[u32; 2]> {
        let n = self.node_count as u32;
        (0..self.edge_count)
            .map(|i| self.edge(i))
            .filter(|&(f, t, _)| f < n && t < n)
            .map(|(f, t, _)| [f, t])
            .collect()
    }

    /// Undirected degree per node, counting only non-ghost edges — the radius
    /// channel of the ring instance, and the layout's mass.
    #[must_use]
    pub fn degrees(&self) -> Vec<u32> {
        let mut d = vec![0u32; self.node_count];
        for [f, t] in self.edge_pairs() {
            d[f as usize] += 1;
            d[t as usize] += 1;
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal v3 encoder — the test's own, so the viewer is checked
    /// against the SPEC rather than against a producer that could drift with
    /// it. `version` is a parameter precisely so the gate can be exercised.
    fn encode(
        version: u16,
        nodes: &[(u32, u32, u8, u8)],
        edges: &[(u32, u32, u8)],
        labels: &[&str],
    ) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&MAGIC);
        b.extend_from_slice(&version.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // flags: labels only
        b.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
        b.extend_from_slice(&(edges.len() as u32).to_le_bytes());
        for &(classid, identity, domain, evidence) in nodes {
            b.extend_from_slice(&classid.to_le_bytes());
            b.extend_from_slice(&identity.to_le_bytes());
            b.extend_from_slice(&[1, 0, 0, 0]); // vocab, role, flags, reserved
            b.push(domain);
            b.push(evidence);
            b.extend_from_slice(&0u16.to_le_bytes());
        }
        for &(from, to, predicate) in edges {
            b.extend_from_slice(&from.to_le_bytes());
            b.extend_from_slice(&to.to_le_bytes());
            b.extend_from_slice(&[0, 0, 0, predicate]);
        }
        b.extend_from_slice(&LABEL_MAGIC);
        for l in labels {
            b.extend_from_slice(&(l.len() as u16).to_le_bytes());
            b.extend_from_slice(l.as_bytes());
        }
        b
    }

    fn sample() -> Vec<u8> {
        encode(
            WIRE_VERSION,
            &[(0x0901, 10, 1, 2), (0x0901, 11, 3, 0), (0x0903, 12, 4, 1)],
            &[(0, 1, 1), (1, 2, 3)],
            &["diabetes mellitus", "pancreas", "HbA1c"],
        )
    }

    #[test]
    fn reads_every_declared_field_at_its_declared_offset() {
        let buf = sample();
        let g = GraphAbi::parse(&buf).expect("v3 parses");
        assert_eq!((g.node_count(), g.edge_count()), (3, 2));
        assert_eq!(g.address(0), (0x0901, 10));
        assert_eq!(g.address(2), (0x0903, 12));
        // domain and evidence live at 12/13 — the v3 bytes v2 kept reserved.
        assert_eq!((g.domain(0), g.evidence(0)), (1, 2));
        assert_eq!((g.domain(2), g.evidence(2)), (4, 1));
        assert_eq!(g.vocab(1), 1);
        assert_eq!(g.label(0), "diabetes mellitus");
        assert_eq!(g.label(2), "HbA1c");
        // predicate is byte 11 of the edge record — v2's `_pad`.
        assert_eq!(g.edge(1), (1, 2, 3));
    }

    /// The label lane is BORROWED, not copied. A viewer that allocated per
    /// node would still pass every equality test above, so the property is
    /// asserted directly: the slice must point INTO the caller's buffer.
    #[test]
    fn labels_point_into_the_callers_buffer() {
        let buf = sample();
        let g = GraphAbi::parse(&buf).expect("parses");
        let lo = buf.as_ptr() as usize;
        let hi = lo + buf.len();
        for i in 0..g.node_count() {
            let p = g.label(i).as_ptr() as usize;
            assert!(
                (lo..hi).contains(&p),
                "label {i} was copied out of the stream instead of borrowed"
            );
        }
    }

    /// CAN FIRE: the whole point of the gate. A v2 stream must be REFUSED,
    /// not read with v3 offsets — bytes 12/13 are v2's reserved zone, so a
    /// silent read would report domain 0 for every node and look plausible.
    #[test]
    fn the_version_gate_refuses_both_directions() {
        let v2 = encode(2, &[(1, 1, 9, 9)], &[], &["x"]);
        assert!(matches!(
            GraphAbi::parse(&v2),
            Err(AbiError::WrongVersion(2))
        ));
        let v4 = encode(4, &[(1, 1, 9, 9)], &[], &["x"]);
        assert!(matches!(
            GraphAbi::parse(&v4),
            Err(AbiError::WrongVersion(4))
        ));
        // CAN STAY SILENT: the version it does read is accepted.
        assert!(GraphAbi::parse(&sample()).is_ok());
    }

    #[test]
    fn a_truncated_or_mislabelled_stream_is_refused_not_guessed() {
        assert!(matches!(
            GraphAbi::parse(&[1, 2, 3]),
            Err(AbiError::TooShort)
        ));
        let mut bad = sample();
        bad[0] = b'X';
        assert!(matches!(GraphAbi::parse(&bad), Err(AbiError::BadMagic(_))));
        let cut = &sample()[..HEADER_LEN + NODE_LEN];
        assert!(matches!(GraphAbi::parse(cut), Err(AbiError::Truncated(_))));
    }

    /// A ghost edge must be dropped at the seam, not at every draw call.
    /// CAN FIRE: without the filter this indexes a GPU buffer out of bounds.
    #[test]
    fn ghost_edges_are_dropped_and_degrees_count_only_real_ones() {
        let buf = encode(
            WIRE_VERSION,
            &[(1, 1, 0, 0), (1, 2, 0, 0)],
            &[(0, 1, 1), (0, 99, 1), (7, 0, 1)],
            &["a", "b"],
        );
        let g = GraphAbi::parse(&buf).expect("parses");
        assert_eq!(g.edge_count(), 3, "the header still declares all three");
        assert_eq!(g.edge_pairs(), vec![[0, 1]], "only the real one survives");
        assert_eq!(g.degrees(), vec![1, 1], "the ghosts add no degree");
    }
}
