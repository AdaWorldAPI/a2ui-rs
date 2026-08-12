//! `a2ui-solid-web` — the Railway-deployable demo of the **solid tier**.
//!
//! Sibling to `a2ui-paint-web`, deliberately a SEPARATE binary and a separate
//! image: the paint deployable demonstrates the paint tier and should keep
//! doing exactly that. This one demonstrates the claim one dimension up.
//!
//! # What it makes observable
//!
//! - **The wire carries parameters, not geometry.** `/delta.bin` is the whole
//!   part: a `NodeDelta` whose payload is twelve facet bytes. `/model.stl` is
//!   the same part as a mesh, and is thousands of times larger. `/health`
//!   prints the ratio. Refine the mesh with `?res=` and the STL grows while the
//!   frame does not move — that independence is the entire point, and it is why
//!   "don't push pixels" generalises to "don't push meshes".
//! - **A design change is an addressed edit.** `/edit?rail=3&mm=6.5` answers
//!   with the `NodeDelta` that sets rail 3 to 6.50 mm — two bytes at two mask
//!   positions. No CAD script, no document, no diff of a file. The same
//!   move `a2ui-paint-web` makes when a click answers with an ordinal.
//! - **The projection is server-side and vector.** The SVG at `/` is
//!   orthographic isometric, back-face culled, painter-sorted. No GPU, no
//!   raster, no canvas — the container has none of those and does not need
//!   them.
//!
//! # What it is not
//!
//! Not an editor, and not a CAD application. There is no constraint solver, no
//! assembly, no feature history beyond the six rails. The vocabulary is a
//! deliberate POC subset (see `a2ui_solid::sdf::Solid`) shaped to become an
//! OGAR class vocabulary later; nothing here mints a classid.
//!
//! Port: `$PORT` from the environment (Railway injects it); 8080 only as a
//! local fallback.

use std::net::SocketAddr;

use a2ui_core::{Frame, NodeDelta};
use a2ui_solid::{
    FACET_LEN, Facet, Mesh, RAIL_COUNT, marching_tets,
    rail::Rail,
    sdf::{Point, plate_with_bore, plate_with_bore_volume},
    stl::to_binary_stl,
};
use axum::{
    Router,
    extract::RawQuery,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::get,
};

/// The node this demo's frames address.
///
/// Concept `0x0201` in the high u16, app prefix `0x0007` in the low — a
/// different concept from the paint demo's `0x0102`, because it is a different
/// class. The byte order is the canon's: the classid is the LE u32 at 0..4, so
/// `07 00 01 02` reads as `0x0201_0007`.
const SOLID_KEY: [u8; 16] = [
    0x07, 0x00, 0x01, 0x02, // classid u32 LE = 0x0201_0007
    0x00, 0x00, 0x00, 0x00, // HEEL / HIP — unused by this demo, reserved not reclaimed
    0x00, 0x00, // TWIG
    0x00, 0x00, 0x00, 0x01, 0x00, 0x01, // tail
];

/// Labels for the six rails, in mask-position order.
///
/// Rails 4 and 5 are unused by `plate_with_bore` and read zero. They are named
/// anyway: the register has a fixed width, and a reader seeing four labels for
/// six rails would reasonably wonder whether the last two had been reclaimed.
/// They have not — zero means *not consulted*, never *compacted away*.
const RAIL_LABELS: [&str; RAIL_COUNT] = [
    "width",
    "depth",
    "height",
    "bore radius",
    "(reserved)",
    "(reserved)",
];

/// The default part: a 20 × 20 × 10 mm plate with a 5 mm bore.
fn default_facet() -> Facet {
    Facet::from_mm([20.0, 20.0, 10.0, 5.0, 0.0, 0.0])
}

/// Query parameters, parsed by hand.
///
/// `RawQuery` rather than `axum::extract::Query` for the same reason as the
/// paint demo: `Query` deserializes through serde, and neither of these crates
/// carries serde on purpose (charter T3). A demo query string is not a reason
/// to pull a serialization framework into the tier built to avoid one.
#[derive(Debug)]
struct Params {
    facet: Facet,
    res: f32,
    /// Rail index for `/edit`.
    rail: Option<usize>,
    /// Millimetre value for `/edit`.
    mm: Option<f32>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            facet: default_facet(),
            res: 1.0,
            rail: None,
            mm: None,
        }
    }
}

impl Params {
    fn from_raw(raw: &str) -> Self {
        let mut p = Self::default();
        let mut dims = [
            p.facet.mm(0),
            p.facet.mm(1),
            p.facet.mm(2),
            p.facet.mm(3),
            p.facet.mm(4),
            p.facet.mm(5),
        ];
        for pair in raw.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "w" => set(&mut dims[0], v),
                "d" => set(&mut dims[1], v),
                "h" => set(&mut dims[2], v),
                "r" => set(&mut dims[3], v),
                // Clamped rather than trusted: a `res` of 0.001 on a 20 mm part
                // is ~10^10 sample points and the container dies. The floor is
                // the honest limit of a server-side mesher on a shared box, not
                // a limit of the kernel.
                "res" => {
                    if let Ok(x) = v.parse::<f32>() {
                        p.res = x.clamp(0.4, 8.0);
                    }
                }
                "rail" => p.rail = v.parse().ok(),
                "mm" => p.mm = v.parse().ok(),
                _ => {}
            }
        }
        p.facet = Facet::from_mm(dims);
        p
    }
}

fn set(slot: &mut f32, v: &str) {
    if let Ok(x) = v.parse::<f32>() {
        *slot = x;
    }
}

/// The full-facet `NodeDelta` for a parameter set — the whole part, on the wire.
///
/// All twelve positions are named because a fresh client has no prior state;
/// an incremental edit names only the two positions of the rail it moves, which
/// is what [`edit`] emits.
fn full_delta(facet: &Facet) -> Vec<u8> {
    let bytes = facet.to_facet_bytes();
    Frame::NodeDelta(NodeDelta {
        key: SOLID_KEY,
        // Twelve consecutive positions: 0b1111_1111_1111.
        mask_words: vec![(1u64 << FACET_LEN) - 1],
        values: bytes.to_vec(),
    })
    .to_le_bytes()
}

/// Orthographic isometric projection of a mesh to SVG.
///
/// True isometric: rotate 45° about Z, then tilt by `atan(1/√2)` ≈ 35.264°
/// about X. Back faces are culled by the sign of the projected normal's depth
/// component, and the survivors are painter-sorted — which is exact enough for
/// a convex-ish part and wrong for a self-occluding one. Naming that limit
/// rather than discovering it later: this is a preview, not a renderer.
fn mesh_to_svg(mesh: &Mesh, w: f32, h: f32) -> (String, usize, usize) {
    // Camera basis.
    let a = std::f32::consts::FRAC_PI_4; // 45° about Z
    let b = (1.0f32 / 2.0f32.sqrt()).atan(); // ≈35.264° about X
    let (sa, ca) = a.sin_cos();
    let (sb, cb) = b.sin_cos();
    let project = |p: Point| -> [f32; 3] {
        let x = p[0] * ca - p[1] * sa;
        let y = p[0] * sa + p[1] * ca;
        let y2 = y * cb - p[2] * sb;
        let z2 = y * sb + p[2] * cb; // depth
        [x, y2, z2]
    };

    let projected: Vec<[f32; 3]> = mesh.verts.iter().map(|v| project(*v)).collect();
    if projected.is_empty() {
        return (String::new(), 0, 0);
    }

    // Fit to the viewport.
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &projected {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-3);
    let scale = (w.min(h) * 0.8) / span;
    let ox = w * 0.5 - (lo[0] + hi[0]) * 0.5 * scale;
    let oy = h * 0.5 + (lo[1] + hi[1]) * 0.5 * scale;
    let sx = |p: [f32; 3]| ox + p[0] * scale;
    // SVG y grows downward; the projection's y grows up. Same flip the tile
    // skin needs, for the same reason — omitting it mirrors the part, which
    // still looks like a part.
    let sy = |p: [f32; 3]| oy - p[1] * scale;

    let total = mesh.tris.len();
    let mut faces: Vec<(f32, [u32; 3], f32)> = Vec::with_capacity(total);
    for t in &mesh.tris {
        let (a, b, c) = (
            projected[t[0] as usize],
            projected[t[1] as usize],
            projected[t[2] as usize],
        );
        // Signed area in screen space: negative means the face points away.
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area <= 0.0 {
            continue;
        }
        // Shade by the world-space normal against a fixed light. Cheap Lambert;
        // enough to read the form.
        let (wa, wb, wc) = (
            mesh.verts[t[0] as usize],
            mesh.verts[t[1] as usize],
            mesh.verts[t[2] as usize],
        );
        let u = [wb[0] - wa[0], wb[1] - wa[1], wb[2] - wa[2]];
        let v = [wc[0] - wa[0], wc[1] - wa[1], wc[2] - wa[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
        let light = [0.40f32, -0.40, 0.82];
        let lambert =
            ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / nl).max(0.0) * 0.75 + 0.25;
        let depth = (a[2] + b[2] + c[2]) / 3.0;
        faces.push((depth, *t, lambert));
    }
    let drawn = faces.len();
    // Painter's algorithm: far first.
    faces.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut s = String::with_capacity(drawn * 120 + 512);
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect width="100%" height="100%" fill="#0f1115"/>"##
    ));
    for (_, t, l) in &faces {
        let p = |i: usize| {
            let v = projected[t[i] as usize];
            format!("{:.1},{:.1}", sx(v), sy(v))
        };
        let shade = (*l * 210.0) as u32;
        let g = shade.min(255);
        s.push_str(&format!(
            r##"<polygon points="{} {} {}" fill="rgb({},{},{})"/>"##,
            p(0),
            p(1),
            p(2),
            (g as f32 * 0.62) as u32,
            (g as f32 * 0.76) as u32,
            g
        ));
    }
    s.push_str("</svg>");
    (s, drawn, total)
}

fn build(params: &Params) -> (Mesh, Vec<u8>) {
    let solid = plate_with_bore(&params.facet);
    let mesh = marching_tets(&solid, solid.bounds(), params.res);
    let stl = to_binary_stl(&mesh, "plate");
    (mesh, stl)
}

async fn index(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let p = Params::from_raw(raw.as_deref().unwrap_or(""));
    let (mesh, stl) = build(&p);
    let (svg, drawn, total) = mesh_to_svg(&mesh, 720.0, 520.0);
    let delta = full_delta(&p.facet);
    let volume = plate_with_bore_volume(&p.facet);

    let rails = (0..RAIL_COUNT)
        .map(|i| {
            let r = p.facet.rail(i);
            format!(
                r##"<tr><td style="color:#5b6478">{i}</td><td>{label}</td>
<td style="color:#e6e9ef">{mm}.{cmm:02} mm</td>
<td style="color:#5b6478">pos {a},{b} = {ba:02x} {bb:02x}</td>
<td><a style="color:#7aa2f7" href="/edit?rail={i}&amp;mm={next}">+0.5</a></td></tr>"##,
                i = i,
                label = RAIL_LABELS[i],
                mm = r.mm,
                cmm = r.cmm,
                a = i * 2,
                b = i * 2 + 1,
                ba = r.mm,
                bb = r.cmm,
                next = r.mm_f32() + 0.5,
            )
        })
        .collect::<String>();

    Html(format!(
        r##"<!doctype html><meta charset="utf-8"><title>a2ui solid tier</title>
<body style="margin:0;background:#0f1115;color:#cbd3e1;font:13px ui-sans-serif,system-ui,sans-serif">
<div style="padding:10px 12px">
  <a href="/" style="color:#7aa2f7">default</a> ·
  <a href="/?w=40&amp;d=25&amp;h=6&amp;r=8" style="color:#7aa2f7">wide plate</a> ·
  <a href="/?res=0.6" style="color:#7aa2f7">finer mesh</a> ·
  <a href="/model.stl" style="color:#7aa2f7">model.stl</a> ·
  <a href="/delta.bin" style="color:#7aa2f7">the frame</a> ·
  <a href="/health" style="color:#7aa2f7">health</a>
</div>
{svg}
<div style="padding:12px">
  <div style="color:#5b6478;padding-bottom:6px">
    the wire carries <b style="color:#e6e9ef">{dn} bytes</b> of NodeDelta ·
    this mesh is <b style="color:#e6e9ef">{sn} bytes</b> of STL
    ({ratio:.0}x) · {tris} triangles, {drawn} front-facing of {total} ·
    res {res} mm · volume {vol}
  </div>
  <table style="border-collapse:collapse" cellpadding="4">
    <tr style="color:#8b93a7"><th align="left">rail</th><th align="left">parameter</th>
    <th align="left">value</th><th align="left">mask positions</th><th></th></tr>
    {rails}
  </table>
  <p style="color:#5b6478;max-width:70ch">
    Each parameter is one <code>u8:u8</code> rail — two separate bytes at two
    mask positions, read as whole millimetres and hundredths. Editing one is a
    NodeDelta naming those two positions; the mesh above is derived, never sent.
  </p>
</div>"##,
        svg = svg,
        dn = delta.len(),
        sn = stl.len(),
        ratio = stl.len() as f32 / delta.len() as f32,
        tris = mesh.tri_count(),
        drawn = drawn,
        total = total,
        res = p.res,
        vol = volume.map_or_else(
            || "n/a (bore breaks the wall)".to_string(),
            |v| format!("{v:.0} mm³")
        ),
        rails = rails,
    ))
}

/// The mesh, as the printer wants it.
async fn model_stl(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let p = Params::from_raw(raw.as_deref().unwrap_or(""));
    let (_, stl) = build(&p);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "model/stl"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"plate.stl\"",
            ),
        ],
        stl,
    )
}

/// The part itself: twelve facet bytes in a `NodeDelta`, raw LE (T3).
async fn delta_bin(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let p = Params::from_raw(raw.as_deref().unwrap_or(""));
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        full_delta(&p.facet),
    )
}

/// A design change as an ADDRESSED edit.
///
/// The response frame names only the two mask positions of the rail being
/// moved — not the whole facet. That is the geometric analogue of the paint
/// tier answering a click with an ordinal: the change travels as an address
/// plus two bytes, and nothing about the resulting shape rides along.
async fn edit(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let p = Params::from_raw(raw.as_deref().unwrap_or(""));
    let (Some(rail), Some(mm)) = (p.rail, p.mm) else {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "usage: /edit?rail=<0..5>&mm=<millimetres>\n".to_string(),
        );
    };
    if rail >= RAIL_COUNT {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("rail {rail} is past the register ({RAIL_COUNT} rails)\n"),
        );
    }

    let mut facet = p.facet;
    let whole = mm.clamp(0.0, 255.99);
    let r = Rail::new(whole.trunc() as u8, ((whole.fract()) * 100.0).round() as u8);
    facet.set_rail(rail, r);

    let (lo, hi) = (rail * 2, rail * 2 + 1);
    let bytes = facet.to_facet_bytes();
    let frame = Frame::NodeDelta(NodeDelta {
        key: SOLID_KEY,
        mask_words: vec![(1u64 << lo) | (1u64 << hi)],
        values: vec![bytes[lo], bytes[hi]],
    })
    .to_le_bytes();

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "rail {rail} ({label}) := {mm}.{cmm:02} mm\n\
             mask positions {lo},{hi}  values {a:02x} {b:02x}\n\
             wire ({n} bytes LE, THIS is the format): {hex}\n\
             \n\
             the whole design change is two bytes at two addresses.\n\
             no script, no document, no mesh.\n",
            label = RAIL_LABELS[rail],
            mm = r.mm,
            cmm = r.cmm,
            a = bytes[lo],
            b = bytes[hi],
            n = frame.len(),
            hex = frame.iter().map(|x| format!("{x:02x}")).collect::<String>(),
        ),
    )
}

/// Liveness, reporting the claim rather than a bare "ok".
async fn health() -> impl IntoResponse {
    let facet = default_facet();
    let c = a2ui_solid::wire_cost(&facet, 1.0);
    let delta = full_delta(&facet);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "ok\n\
             tier: solid (parametric CSG -> SDF -> marching tets -> STL)\n\
             parameters: {pb} facet bytes in a {db}-byte NodeDelta\n\
             mesh: {tris} triangles, {mb} bytes STL at 1.0 mm\n\
             ratio: {ratio:.0}x\n\
             rails: {rails} x (u8:u8), 0.00-255.99 mm at 0.01 mm\n\
             deps: zero geometry crates\n\
             wgpu: off (server-side SVG projection, no GPU in container)\n",
            pb = c.parameter_bytes,
            db = delta.len(),
            tris = c.triangles,
            mb = c.mesh_bytes,
            ratio = c.ratio(),
            rails = RAIL_COUNT,
        ),
    )
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let app = Router::new()
        .route("/", get(index))
        .route("/model.stl", get(model_stl))
        .route("/delta.bin", get(delta_bin))
        .route("/edit", get(edit))
        .route("/health", get(health));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr} failed: {e}"));

    let c = a2ui_solid::wire_cost(&default_facet(), 1.0);
    eprintln!(
        "a2ui-solid-web listening on {addr} (PORT={port_env}) — {pb} parameter bytes vs \
         {mb} bytes of STL ({ratio:.0}x), {tris} triangles, wgpu off",
        port_env = std::env::var("PORT").unwrap_or_else(|_| "unset→8080".into()),
        pb = c.parameter_bytes,
        mb = c.mesh_bytes,
        ratio = c.ratio(),
        tris = c.triangles,
    );

    axum::serve(listener, app).await.expect("server");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Query parameters really drive the geometry.
    #[test]
    fn dimensions_come_from_the_query() {
        let p = Params::from_raw("w=40&d=25&h=6&r=8");
        assert!((p.facet.mm(0) - 40.0).abs() < 1e-6);
        assert!((p.facet.mm(3) - 8.0).abs() < 1e-6);
        // …and an absent parameter keeps its default rather than zeroing.
        let q = Params::from_raw("w=40");
        assert!(
            (q.facet.mm(2) - 10.0).abs() < 1e-6,
            "height must keep its default when the query omits it"
        );
    }

    /// The resolution clamp is real, not decoration.
    ///
    /// A `res` of 0.001 on a 20 mm part is roughly 10^10 sample points; without
    /// the clamp one request takes the container down.
    #[test]
    fn the_resolution_clamp_bounds_both_ends() {
        assert!(Params::from_raw("res=0.001").res >= 0.4);
        assert!(Params::from_raw("res=1000").res <= 8.0);
        assert!((Params::from_raw("res=1.5").res - 1.5).abs() < 1e-6);
    }

    /// The headline claim, as an assertion.
    #[test]
    fn the_frame_is_orders_of_magnitude_smaller_than_the_mesh() {
        let p = Params::default();
        let (_, stl) = build(&p);
        let delta = full_delta(&p.facet);
        assert!(
            stl.len() > delta.len() * 500,
            "STL {} vs frame {} — the ratio is the whole point",
            stl.len(),
            delta.len()
        );
        // The frame is the facet plus framing overhead, and nothing else. If
        // geometry ever leaked into it this bound is what would catch it.
        assert!(
            delta.len() < FACET_LEN + 40,
            "the down-wire frame grew: {} bytes",
            delta.len()
        );
    }

    /// Refining the mesh must not move the frame.
    #[test]
    fn frame_size_is_independent_of_mesh_resolution() {
        let coarse = Params::from_raw("res=4");
        let fine = Params::from_raw("res=0.5");
        assert_eq!(
            full_delta(&coarse.facet).len(),
            full_delta(&fine.facet).len()
        );
        assert!(build(&fine).1.len() > build(&coarse).1.len() * 2);
    }

    /// An edit names only the two positions of the rail it moves.
    #[test]
    fn an_edit_addresses_two_positions_and_carries_two_bytes() {
        let mut facet = default_facet();
        facet.set_rail(3, Rail::new(6, 50));
        let bytes = facet.to_facet_bytes();
        let frame = Frame::NodeDelta(NodeDelta {
            key: SOLID_KEY,
            mask_words: vec![(1u64 << 6) | (1u64 << 7)],
            values: vec![bytes[6], bytes[7]],
        })
        .to_le_bytes();

        match Frame::from_le_bytes(&frame).expect("round-trips") {
            Frame::NodeDelta(d) => {
                assert_eq!(d.values, vec![6, 50], "6.50 mm as (whole, hundredths)");
                assert_eq!(d.mask_words, vec![0b1100_0000]);
                assert_eq!(d.key, SOLID_KEY);
            }
            Frame::ActionInvoke(_) => panic!("expected a NodeDelta"),
        }
    }

    /// Back-face culling removes a real fraction — and not everything.
    ///
    /// Two-sided on purpose. "Culled something" would pass for a bug that
    /// dropped one triangle; "culled everything" would pass for a projection
    /// that produced an empty picture. A closed solid should hide roughly half.
    #[test]
    fn back_face_culling_hides_about_half_the_part() {
        let p = Params::default();
        let (mesh, _) = build(&p);
        let (svg, drawn, total) = mesh_to_svg(&mesh, 720.0, 520.0);
        assert!(total > 100, "not enough geometry to judge: {total}");
        assert!(
            drawn * 4 > total && drawn * 4 < total * 3,
            "expected roughly half of {total} front-facing, got {drawn}"
        );
        assert_eq!(
            svg.matches("<polygon").count(),
            drawn,
            "every surviving face must reach the SVG"
        );
    }

    /// The SVG is well-formed enough to render, and is not empty.
    #[test]
    fn the_projection_produces_a_non_degenerate_picture() {
        let p = Params::default();
        let (mesh, _) = build(&p);
        let (svg, drawn, _) = mesh_to_svg(&mesh, 720.0, 520.0);
        assert!(svg.starts_with("<svg"), "not an svg");
        assert!(svg.ends_with("</svg>"), "unterminated svg");
        assert!(drawn > 0);
        assert!(
            !svg.contains("NaN"),
            "a NaN coordinate reached the SVG — the projection divided by a \
             degenerate span"
        );
    }

    /// An empty mesh must not panic the projection.
    #[test]
    fn an_empty_mesh_projects_to_nothing_rather_than_panicking() {
        let (svg, drawn, total) = mesh_to_svg(&Mesh::default(), 720.0, 520.0);
        assert_eq!((drawn, total), (0, 0));
        assert!(svg.is_empty());
    }
}
