//! Binary STL output — the printer's format, written by hand.
//!
//! Binary STL is 84 bytes of header plus 50 bytes per triangle, all
//! little-endian. That is small enough that a dependency would cost more than
//! it saves, and writing it here keeps this crate at **zero geometry
//! dependencies** — which matters twice: the deploy image stays lean, and
//! lifting the crate to its own repo later stays a move rather than a
//! dependency negotiation.
//!
//! It is also, pleasingly, the same discipline as the rest of the stack:
//! `to_le_bytes` IS the format (charter T3). STL is just another
//! little-endian wire.
//!
//! **What STL is not:** a source of truth. It has no parameters, no features,
//! no history — a mesh exported here cannot be edited back into the solid that
//! produced it. The solid is the truth; this is the printer's projection of it,
//! the same way an SVG is the screen's projection of a surface.

use crate::mesh::Mesh;

/// Bytes per triangle in binary STL: normal + 3 vertices + attribute count.
const TRI_BYTES: usize = 12 * 4 + 2;

/// Header bytes: 80-byte comment field + u32 triangle count.
const HEADER_BYTES: usize = 80 + 4;

/// Serialize a mesh as binary STL.
///
/// The 80-byte header is filled with `banner`, truncated or zero-padded. It is
/// free-form by the format's definition — but NOT free of consequence: a header
/// beginning with the ASCII word `solid` makes some readers try to parse the
/// file as *ASCII* STL and fail. The banner is therefore prefixed rather than
/// used raw.
#[must_use]
pub fn to_binary_stl(mesh: &Mesh, banner: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_BYTES + mesh.tris.len() * TRI_BYTES);

    let mut header = [0u8; 80];
    // "a2ui-solid " prefix guarantees the file cannot begin with "solid", which
    // is the ASCII-STL sniff token. Cheap insurance against a reader guessing
    // the wrong format on a valid file.
    let text = format!("a2ui-solid {banner}");
    let bytes = text.as_bytes();
    let n = bytes.len().min(80);
    header[..n].copy_from_slice(&bytes[..n]);
    out.extend_from_slice(&header);

    out.extend_from_slice(&(mesh.tris.len() as u32).to_le_bytes());

    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize];
        let b = mesh.verts[t[1] as usize];
        let c = mesh.verts[t[2] as usize];
        let n = face_normal(a, b, c);
        for v in [n, a, b, c] {
            for component in v {
                out.extend_from_slice(&component.to_le_bytes());
            }
        }
        // Attribute byte count. Zero is the only portable value; some tools
        // abuse it to carry colour, which other tools then misread.
        out.extend_from_slice(&0u16.to_le_bytes());
    }

    out
}

/// The number of triangles a binary STL claims in its header.
///
/// Exists so a round-trip test can verify the file against the mesh that
/// produced it without a full parser — the header count and the actual length
/// disagreeing is the single most common way a hand-written STL is broken.
#[must_use]
pub fn declared_tri_count(stl: &[u8]) -> Option<u32> {
    if stl.len() < HEADER_BYTES {
        return None;
    }
    Some(u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]))
}

/// Whether the file's length matches its declared triangle count exactly.
#[must_use]
pub fn length_matches_header(stl: &[u8]) -> bool {
    match declared_tri_count(stl) {
        Some(n) => stl.len() == HEADER_BYTES + n as usize * TRI_BYTES,
        None => false,
    }
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < f32::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [n[0] / len, n[1] / len, n[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::marching_tets;
    use crate::rail::Facet;
    use crate::sdf::plate_with_bore;

    fn demo_mesh() -> Mesh {
        let facet = Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0]);
        let s = plate_with_bore(&facet);
        marching_tets(&s, s.bounds(), 1.0)
    }

    #[test]
    fn the_file_length_matches_its_declared_triangle_count() {
        let m = demo_mesh();
        let stl = to_binary_stl(&m, "test");
        assert_eq!(declared_tri_count(&stl), Some(m.tri_count() as u32));
        assert!(
            length_matches_header(&stl),
            "header claims {:?} triangles but the file is {} bytes",
            declared_tri_count(&stl),
            stl.len()
        );
    }

    /// The ASCII-STL sniff hazard, pinned two-sided.
    #[test]
    fn the_header_never_begins_with_the_ascii_stl_token() {
        // Even when the caller hands us exactly the dangerous word.
        let stl = to_binary_stl(&demo_mesh(), "solid");
        assert!(
            !stl.starts_with(b"solid"),
            "a binary STL beginning with 'solid' gets sniffed as ASCII STL"
        );
        assert!(stl.starts_with(b"a2ui-solid"));
    }

    /// An over-long banner must truncate, not overrun the fixed header.
    #[test]
    fn an_over_long_banner_cannot_corrupt_the_triangle_count() {
        let m = demo_mesh();
        let stl = to_binary_stl(&m, &"x".repeat(500));
        assert_eq!(
            declared_tri_count(&stl),
            Some(m.tri_count() as u32),
            "the banner overflowed into the count field"
        );
        assert!(length_matches_header(&stl));
    }

    /// Normals are unit length and point the same way as the winding.
    #[test]
    fn face_normals_are_unit_and_agree_with_the_winding() {
        let m = demo_mesh();
        let stl = to_binary_stl(&m, "test");
        // First triangle's normal sits right after the header.
        let f =
            |off: usize| f32::from_le_bytes([stl[off], stl[off + 1], stl[off + 2], stl[off + 3]]);
        let n = [f(84), f(88), f(92)];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-4, "normal is not unit length: {len}");

        let a = mesh_vert(&stl, 0, 0);
        let b = mesh_vert(&stl, 0, 1);
        let c = mesh_vert(&stl, 0, 2);
        let want = super::face_normal(a, b, c);
        for i in 0..3 {
            assert!(
                (n[i] - want[i]).abs() < 1e-4,
                "stored normal disagrees with the vertex winding"
            );
        }
    }

    fn mesh_vert(stl: &[u8], tri: usize, v: usize) -> [f32; 3] {
        let base = 84 + tri * 50 + 12 + v * 12;
        let f =
            |off: usize| f32::from_le_bytes([stl[off], stl[off + 1], stl[off + 2], stl[off + 3]]);
        [f(base), f(base + 4), f(base + 8)]
    }
}
