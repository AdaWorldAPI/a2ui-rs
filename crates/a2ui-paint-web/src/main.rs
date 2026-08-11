//! `a2ui-paint-web` — the Railway-deployable demo of the **paint tier**.
//!
//! This binary exists to make three charter claims *observable* rather than
//! merely written down:
//!
//! - **T1 — no second vocabulary.** `?skin=form` and `?skin=flow` render the
//!   SAME resolved surface (`&[FieldView]` / `&[ActionRef]`) two ways. A skin
//!   is a render style over one surface, never a new widget enum. Nothing in
//!   the surface changes when the skin does — compare the two responses.
//! - **T2 — behavior travels by ADDRESS.** `POST /hit` takes pixel coordinates,
//!   resolves them through [`PaintLayout::hit_test`], and — when the hit is an
//!   action — answers with an `ActionInvoke` carrying an **ordinal**. There is
//!   no handler, no callback, no lambda anywhere on the wire.
//! - **T3 — no serialization on the hot path.** The up-frame is emitted as
//!   `Frame::to_le_bytes()`, the canonical wire form, served as
//!   `application/octet-stream`. The hex line in the SVG footer is a *human*
//!   convenience printed beside it, not the format.
//!
//! The surface is **not** a fixture. It is resolved the way a browser resolves
//! one: register a class codebook in an [`a2ui_wasm::FieldviewClient`], feed it
//! a real `NodeDelta` as LE bytes, and read back `resolved_fields` /
//! `resolved_actions`. Both renderers then consume that one resolution — `/`
//! paints it as SVG, `/html` renders the askama fieldview — which is what makes
//! *"one surface, two renderers"* an observable property of the deploy rather
//! than a claim in a doc comment.
//!
//! Deliberately NOT here: `wgpu` (the container has no GPU, and the paint-DATA
//! path is pure without it), `a2ui-server` (RBAC projection + sealed transport
//! are a different tier), and any persistence. **Correction to an earlier
//! version of this comment:** it said the server tier "pulls the whole
//! lance-graph tree in". That was wrong — `lance-graph-contract` is a zero-dep
//! trait crate by explicit design, and it builds on 1.95.0. What the server
//! tier actually adds is `ogar-encryption` + `ogar-vocab` and a session loop
//! this demo never drives. `Dockerfile.railway` carries the measurement.
//!
//! Port: `$PORT` from the environment (Railway injects it); 8080 only as a
//! local fallback.

use std::net::SocketAddr;
use std::sync::OnceLock;

use a2ui_core::{ActionInvoke, Frame, NodeDelta};
use a2ui_paint::{Hit, PaintLayout, Skin, Viewport, layout_with_skin};
use a2ui_wasm::{ClientClass, ClientField, FieldviewClient, concept_of_key};
use axum::{
    Router,
    extract::RawQuery,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use ogar_render_askama::{ActionRef, FieldView};

/// The demo node every frame here addresses — a canonical 16-byte GUID.
///
/// The classid is the u32 at bytes 0..4, **little-endian**, and under the
/// canon-high flip its HIGH u16 is the shared concept while the LOW u16 is the
/// per-app render prefix. So the bytes `07 00 02 01` are the u32 `0x0102_0007`
/// = concept `0x0102`, app prefix `0x0007` — which is what
/// [`concept_of_key`] reads and therefore which class the client resolves.
///
/// An earlier version of this constant wrote the classid bytes as
/// `01 02 00 00`, i.e. the u32 `0x0000_0201`, whose high u16 is `0x0000` — the
/// **default class**, not a concept at all. It went unnoticed because nothing
/// resolved the key: the surface was hand-written, so the classid was decoration.
/// Feeding the key through a real codebook lookup is what surfaced it, which is
/// the general lesson — an address nobody dereferences can hold anything.
const DEMO_KEY: [u8; 16] = [
    0x07, 0x00, 0x02, 0x01, // classid u32 LE = 0x0102_0007 (concept 0x0102, app 0x0007)
    0x00, 0x00, 0x00, 0x00, // HEEL / HIP
    0x00, 0x00, // TWIG
    0x00, 0x00, 0x00, 0x2A, 0x00, 0x07, // tail
];

/// The one position the class declares and the wire never carries.
///
/// It is the demo's most load-bearing detail. The class codebook has a field at
/// this index, but the `NodeDelta`'s mask does not name it, so no value byte is
/// sent and [`FieldviewClient::resolved_fields`] does not emit it. That is
/// exactly the shape RBAC-by-projection takes: an unauthorized field is
/// **absent from the wire**, not present-and-hidden. Under `?skin=grid` the
/// absence is visible as an empty cell.
const UNSENT_POSITION: u8 = 3;

/// Everything the demo resolved ONCE, at first use, from one down-wire frame.
///
/// Holding the askama HTML *and* the paint-side slices together is the point:
/// they are not two renders of two surfaces, they are two renders of the byte
/// array in [`Self::delta`]. If they could ever disagree, this struct is where
/// the disagreement would have to be introduced.
struct Demo {
    /// The exact LE bytes fed to the client — served raw at `/delta.bin`.
    delta: Vec<u8>,
    /// The askama fieldview render, returned by `apply_node_delta` itself.
    html: String,
    /// The resolved fields, cloned out of the client's borrow.
    fields: Vec<FieldView>,
    /// The resolved actions, cloned out of the client's borrow.
    actions: Vec<ActionRef>,
    /// The concept the key resolved to — reported at `/health` so a deploy can
    /// be checked against the class it thinks it is rendering.
    concept: u16,
}

/// Resolve the demo surface the way a browser would.
///
/// The values are **facet bytes**, not strings, and the demo shows them as
/// such. That is not a shortcut — the V3 content-blind facet is 12 bytes
/// (`le-contract` §3), so a field's value on this wire IS one byte. A demo that
/// displayed `"RE-2026-0042"` here would be showing something the register
/// cannot hold, which is a pleasant-looking lie about the substrate.
/// The demo's codebook entry — position-ordered fields (index = mask position)
/// and ordinal-ordered action captions. This is the "font of the desktop": sent
/// once per class, reused by every instance of it.
///
/// Factored out so the falsifier test resolves against the REAL class rather
/// than a lookalike built beside it — a copy would drift and the test would
/// keep passing while proving nothing about what the server serves.
fn demo_class() -> ClientClass {
    ClientClass {
        concept: "vorgang".to_string(),
        title: "Vorgang".to_string(),
        fields: vec![
            ClientField::new("Belegnummer", "name"),
            ClientField::new("Position", "line_no"),
            ClientField::new("Betrag (EUR)", "amount_total"),
            // Declared by the class, never sent down the wire — see
            // `UNSENT_POSITION`.
            ClientField::new("Rabatt (%)", "discount_pct"),
            ClientField::new("Status", "state"),
        ],
        actions: vec![
            "Ansehen".to_string(),
            "Buchen".to_string(),
            "Stornieren".to_string(),
        ],
    }
}

fn build_demo() -> Demo {
    let class_id = concept_of_key(&DEMO_KEY);
    let mut client = FieldviewClient::new();
    client.register_class(class_id, demo_class());

    // Positions {0, 1, 2, 4}: bit 3 is CLEAR, so the mask does not name
    // `UNSENT_POSITION` and carries no byte for it.
    let mask = 0b1_0111u64;
    debug_assert_eq!(
        mask & (1 << UNSENT_POSITION),
        0,
        "the unsent position must be absent from the mask, or the demo proves nothing"
    );
    let delta = Frame::NodeDelta(NodeDelta {
        key: DEMO_KEY,
        mask_words: vec![mask],
        values: vec![42, 7, 240, 1],
    })
    .to_le_bytes();

    // The client refuses malformed frames rather than partially applying them,
    // so a panic here means the frame above is wrong — which is the correct
    // moment to find out, at boot, not on the first request.
    let html = client
        .apply_node_delta(&delta)
        .unwrap_or_else(|e| panic!("the demo's own NodeDelta was refused by the client: {e}"));

    let fields = client
        .resolved_fields(&DEMO_KEY)
        .expect("a delta was just applied to this key")
        .to_vec();
    let actions = client
        .resolved_actions(&DEMO_KEY)
        .expect("a delta was just applied to this key")
        .to_vec();

    Demo {
        delta,
        html,
        fields,
        actions,
        concept: class_id,
    }
}

/// The resolved demo, built once.
fn demo() -> &'static Demo {
    static D: OnceLock<Demo> = OnceLock::new();
    D.get_or_init(build_demo)
}

/// The resolved surface under demonstration.
///
/// `position` is the field's mask-index ADDRESS (not a layout slot) and
/// `ordinal` is the action's index into the class's `ActionDef` set. Both are
/// addresses the class registry resolves — which is exactly why a click can
/// answer with a number instead of a function.
fn surface() -> (&'static [FieldView], &'static [ActionRef]) {
    let d = demo();
    (&d.fields, &d.actions)
}

/// Query parsing is hand-rolled, and `RawQuery` is used instead of `Query`,
/// deliberately: `axum::extract::Query` deserializes via serde, and this crate
/// carries no serde dependency on purpose (charter T3). The wire format is LE
/// bytes; a demo query string is not a reason to pull a serialization
/// framework into the tier that exists to avoid one.
#[derive(Debug, Default)]
struct ViewQuery {
    w: Option<f32>,
    h: Option<f32>,
    skin: Option<String>,
    cols: Option<u8>,
}

impl ViewQuery {
    fn from_raw(raw: &str) -> Self {
        let mut q = Self::default();
        for pair in raw.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "w" => q.w = v.parse().ok(),
                "h" => q.h = v.parse().ok(),
                "skin" => q.skin = Some(v.to_string()),
                "cols" => q.cols = v.parse().ok(),
                _ => {}
            }
        }
        q
    }

    fn viewport(&self) -> Viewport {
        Viewport::new(self.w.unwrap_or(1000.0), self.h.unwrap_or(700.0))
    }

    fn skin(&self) -> Skin {
        match self.skin.as_deref() {
            Some("flow") => Skin::Flow,
            // The grid skin is the one that reads `position` as a coordinate,
            // so the class's unsent field leaves a visibly empty cell.
            Some("grid") => Skin::Grid {
                cols: self.cols.unwrap_or(3),
            },
            _ => Skin::Form,
        }
    }

    /// The skin as its own query value, so a generated link round-trips.
    fn skin_param(&self) -> String {
        match self.skin() {
            Skin::Flow => "flow".to_string(),
            Skin::Grid { cols } => format!("grid&cols={cols}"),
            _ => "form".to_string(),
        }
    }
}

/// Render the placed layout as SVG. This is the *server-side* raster of the
/// paint-DATA path — the same `PaintLayout` a `wgpu` surface would draw, minus
/// the GPU. Every rect here came from an address, not from a template.
fn layout_to_svg(l: &PaintLayout, vp: &Viewport, skin: Skin) -> String {
    let h = l.content_height.max(vp.height);
    let mut s = String::with_capacity(4096);
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="ui-sans-serif,system-ui,sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#0f1115"/>"##,
        w = vp.width,
        h = h
    ));

    for f in &l.fields {
        // Label and value are placed separately so a skin can move one without
        // the other; both are addressed by the SAME `position`.
        s.push_str(&format!(
            r##"<text x="{lx}" y="{ly}" fill="#8b93a7">{label}</text>"##,
            lx = f.label_rect.x,
            ly = f.label_rect.y + f.label_rect.h * 0.72,
            label = esc(&f.label),
        ));
        s.push_str(&format!(
            r##"<text x="{vx}" y="{vy}" fill="#e6e9ef">{value}</text>"##,
            vx = f.value_rect.x,
            vy = f.value_rect.y + f.value_rect.h * 0.72,
            value = esc(&f.value),
        ));
    }

    for a in &l.actions {
        s.push_str(&format!(
            r##"<g><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="#1d2432" stroke="#39415a"/><text x="{tx}" y="{ty}" fill="#cbd3e1" text-anchor="middle">{label}</text></g>"##,
            x = a.rect.x,
            y = a.rect.y,
            w = a.rect.w,
            h = a.rect.h,
            tx = a.rect.x + a.rect.w / 2.0,
            ty = a.rect.y + a.rect.h * 0.66,
            label = esc(&a.label),
        ));
    }

    s.push_str(&format!(
        r##"<text x="12" y="{y}" fill="#5b6478" font-size="11">{device:?} · {skin:?} · {nf} fields · {na} actions — clicks answer by ordinal, never by handler</text></svg>"##,
        y = h - 10.0,
        device = vp.device,
        skin = skin,
        nf = l.fields.len(),
        na = l.actions.len(),
    ));
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn index(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let q = ViewQuery::from_raw(raw.as_deref().unwrap_or(""));
    let (fields, actions) = surface();
    let vp = q.viewport();
    let skin = q.skin();
    let l = layout_with_skin(fields, actions, &vp, skin);
    let svg = layout_to_svg(&l, &vp, skin);
    let d = demo();

    // The click handler posts coordinates and shows the LE up-frame that comes
    // back. Note what it does NOT do: it never names an action, only a point.
    Html(format!(
        r##"<!doctype html><meta charset="utf-8"><title>a2ui paint tier</title>
<body style="margin:0;background:#0f1115;color:#cbd3e1;font:13px ui-sans-serif,system-ui,sans-serif">
<div style="padding:10px 12px">
  <a href="/?skin=form" style="color:#7aa2f7">form</a> ·
  <a href="/?skin=flow" style="color:#7aa2f7">flow</a> ·
  <a href="/?skin=grid&amp;cols=3" style="color:#7aa2f7">grid</a> ·
  <a href="/?w=420&amp;h=700&amp;skin=form" style="color:#7aa2f7">mobile</a> ·
  <a href="/html" style="color:#7aa2f7">askama html</a> ·
  <a href="/delta.bin" style="color:#7aa2f7">the frame</a>
  <span style="color:#5b6478"> — one surface, many skins (T1)</span>
</div>
<div style="padding:0 12px 10px;color:#5b6478">
  concept 0x{concept:04X} · resolved from a {n}-byte NodeDelta · position
  {unsent} is declared by the class and absent from the wire (try the grid skin)
</div>
<div id="s" style="cursor:crosshair">{svg}</div>
<pre id="o" style="padding:12px;color:#8b93a7"></pre>
<script>
document.getElementById('s').addEventListener('click', async e => {{
  const r = e.currentTarget.getBoundingClientRect();
  const res = await fetch('/hit?x=' + (e.clientX - r.left) + '&y=' + (e.clientY - r.top)
              + '&w={w}&h={h}&skin={sk}');
  document.getElementById('o').textContent = await res.text();
}});
</script>"##,
        svg = svg,
        concept = d.concept,
        n = d.delta.len(),
        unsent = UNSENT_POSITION,
        w = vp.width,
        h = vp.height,
        sk = q.skin_param(),
    ))
}

/// The OTHER renderer, over the SAME resolution.
///
/// This is `render_field_view`'s output — produced by `apply_node_delta` at
/// resolve time and merely stored, never re-derived. Comparing it with `/` is
/// the observable form of the "one surface, two renderers" claim: two
/// projections, one byte array, no second resolution between them.
async fn fieldview_html() -> impl IntoResponse {
    Html(demo().html.clone())
}

/// The down-wire frame itself, raw (T3, the direction `/hit.bin` doesn't cover).
///
/// `/hit.bin` shows bytes going UP. This shows the bytes that came DOWN and
/// produced everything else on this server. Between them the demo never
/// serializes anything: `to_le_bytes` / `from_le_bytes` are the format.
async fn delta_frame() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        demo().delta.clone(),
    )
}

/// Hit-test → ordinal → `ActionInvoke`. **This is the T2 proof.**
///
/// The request carries a point. The response carries an ordinal address and the
/// LE bytes of the up-frame built from it. At no stage does a handler, callback
/// or expression cross the wire — the click resolves against the class's
/// `ActionDef` set by index.
async fn hit(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let (x, y, q) = parse_point(raw.as_deref().unwrap_or(""));
    let (fields, actions) = surface();
    let vp = q.viewport();
    let l = layout_with_skin(fields, actions, &vp, q.skin());

    let body = match l.hit_test(x, y) {
        Some(Hit::Action(ordinal)) => {
            let frame = Frame::ActionInvoke(ActionInvoke {
                key: DEMO_KEY,
                action_ordinal: ordinal,
                args: Vec::new(),
            });
            let bytes = frame.to_le_bytes();
            format!(
                "hit: action ordinal {ordinal}\n\
                 up-frame: ActionInvoke {{ key: {key}, action_ordinal: {ordinal}, args: [] }}\n\
                 wire ({n} bytes LE, THIS is the format): {hex}\n\
                 \n\
                 note: the ordinal is an index into the class's ActionDef set.\n\
                 no handler, no lambda, no expression crossed this wire (T2).",
                key = hex(&DEMO_KEY),
                n = bytes.len(),
                hex = hex(&bytes),
            )
        }
        Some(Hit::Field(position)) => format!(
            "hit: field at mask-index position {position}\n\
             fields are addressed but not invocable — only actions carry behavior."
        ),
        None => "hit: nothing".to_string(),
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

/// The same up-frame as raw LE bytes — the actual wire form, unadorned, for a
/// consumer that wants to feed it straight into a transport (T3).
async fn hit_frame(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let (x, y, q) = parse_point(raw.as_deref().unwrap_or(""));
    let (fields, actions) = surface();
    let vp = q.viewport();
    let l = layout_with_skin(fields, actions, &vp, q.skin());

    match l.hit_test(x, y) {
        Some(Hit::Action(ordinal)) => {
            let bytes = Frame::ActionInvoke(ActionInvoke {
                key: DEMO_KEY,
                action_ordinal: ordinal,
                args: Vec::new(),
            })
            .to_le_bytes();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/octet-stream")],
                bytes,
            )
        }
        _ => (
            StatusCode::NO_CONTENT,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            Vec::new(),
        ),
    }
}

/// Split a raw query into the click point and the view parameters.
fn parse_point(raw: &str) -> (f32, f32, ViewQuery) {
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut rest = Vec::new();
    for pair in raw.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "x" => x = v.parse().unwrap_or(0.0),
            "y" => y = v.parse().unwrap_or(0.0),
            _ => rest.push(pair.to_string()),
        }
    }
    (x, y, ViewQuery::from_raw(&rest.join("&")))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Liveness. Reports the deploy's shape rather than a bare "ok" — the
/// three-state startup discipline (`CLAUDE.md`, frontend assets): a probe that
/// cannot distinguish "running correctly" from "running degraded" is the same
/// silent-fallback trap one layer up.
async fn health() -> impl IntoResponse {
    let d = demo();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "ok\n\
             source: NodeDelta ({delta_len} bytes LE) → FieldviewClient → resolved surface\n\
             concept: 0x{concept:04X}\n\
             surface: {nf} fields, {na} actions (class declares {declared}; position {unsent} absent from the wire)\n\
             skins: form, flow, grid\n\
             renderers: svg (/) + askama (/html), one resolution\n\
             wgpu: off (no GPU in container)\n",
            delta_len = d.delta.len(),
            concept = d.concept,
            nf = d.fields.len(),
            na = d.actions.len(),
            declared = d.fields.len() + 1,
            unsent = UNSENT_POSITION,
        ),
    )
}

#[tokio::main]
async fn main() {
    // Railway injects PORT. 8080 is a LOCAL fallback and must never be treated
    // as the contract — a container listening where nothing is routed looks
    // identical to a container that crashed.
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let app = Router::new()
        .route("/", get(index))
        .route("/html", get(fieldview_html))
        .route("/delta.bin", get(delta_frame))
        .route("/hit", get(hit))
        .route("/hit", post(hit))
        .route("/hit.bin", get(hit_frame))
        .route("/health", get(health));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr} failed: {e}"));

    // Resolve BEFORE announcing readiness. `build_demo` panics on a frame the
    // client refuses, and a process that answers /health while its only surface
    // failed to resolve is the silent-degradation shape this log exists to
    // rule out.
    let d = demo();
    eprintln!(
        "a2ui-paint-web listening on {addr} (PORT={port_env}) — concept 0x{concept:04X} resolved \
         from a {delta_len}-byte NodeDelta: {nf} fields / {na} actions, wgpu off",
        port_env = std::env::var("PORT").unwrap_or_else(|_| "unset→8080".into()),
        concept = d.concept,
        delta_len = d.delta.len(),
        nf = d.fields.len(),
        na = d.actions.len(),
    );

    axum::serve(listener, app).await.expect("server");
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_paint::DeviceClass;

    /// The key resolves to a REAL concept, not the default class.
    ///
    /// This is a regression test with a specific history: the first version of
    /// `DEMO_KEY` wrote its classid bytes in an order whose high u16 was
    /// `0x0000`. Nothing caught it because nothing dereferenced the address.
    /// The `!= 0` half is the part that would have failed then, so it is the
    /// half that earns its place.
    #[test]
    fn the_demo_key_resolves_to_a_real_concept() {
        let c = concept_of_key(&DEMO_KEY);
        assert_ne!(
            c, 0x0000,
            "classid high u16 is the default class — byte order?"
        );
        assert_eq!(
            c, 0x0102,
            "concept 0x0102 (app prefix 0x0007 in the low half)"
        );
    }

    /// The surface came off the wire, and the wire is the format (T3).
    ///
    /// Decoding the stored bytes back into a frame proves `/delta.bin` serves
    /// something a client could actually ingest, rather than a debug dump.
    #[test]
    fn the_served_delta_decodes_back_to_the_frame_that_built_the_surface() {
        let d = demo();
        match Frame::from_le_bytes(&d.delta).expect("the demo's own frame must decode") {
            Frame::NodeDelta(nd) => {
                assert_eq!(nd.key, DEMO_KEY);
                assert_eq!(
                    nd.values.len(),
                    d.fields.len(),
                    "one value byte per resolved field"
                );
            }
            Frame::ActionInvoke(_) => panic!("the down-wire frame is a NodeDelta"),
        }
    }

    /// **The RBAC-by-projection shape, and its anti-vacuity twin.**
    ///
    /// A field the class declares but the mask does not name is ABSENT from the
    /// resolved surface — not present-and-hidden. The first half asserts the
    /// absence; on its own that proves nothing, because a field could be
    /// missing for a dozen reasons (a truncated class, an off-by-one, a codebook
    /// that never had it).
    ///
    /// So the second half sets exactly that mask bit and re-resolves through the
    /// SAME client: the field appears. The mask is therefore established as the
    /// cause. A guard that cannot be made to fire is decoration.
    #[test]
    fn the_unsent_position_is_absent_and_the_mask_is_why() {
        let d = demo();
        let positions: Vec<u8> = d.fields.iter().map(|f| f.position).collect();
        assert_eq!(positions, vec![0, 1, 2, 4], "a gap at the unsent position");
        assert!(
            !positions.contains(&UNSENT_POSITION),
            "position {UNSENT_POSITION} must not be on the resolved surface"
        );
        // It is a GAP, not a truncation: a later position still arrives.
        assert!(
            positions.contains(&4),
            "position 4 must survive — otherwise this is a truncation test, not an absence test"
        );

        // Now make it fire: same class, same key, mask WITH bit 3 set.
        let mut client = FieldviewClient::new();
        client.register_class(concept_of_key(&DEMO_KEY), demo_class());
        let bytes = Frame::NodeDelta(NodeDelta {
            key: DEMO_KEY,
            mask_words: vec![0b1_1111],
            values: vec![42, 7, 240, 15, 1],
        })
        .to_le_bytes();
        client.apply_node_delta(&bytes).expect("well-formed frame");
        let with: Vec<u8> = client
            .resolved_fields(&DEMO_KEY)
            .expect("just applied")
            .iter()
            .map(|f| f.position)
            .collect();
        assert!(
            with.contains(&UNSENT_POSITION),
            "naming the position in the mask MUST make it appear — else the absence above \
             is caused by something other than the mask, and proves nothing about projection"
        );
    }

    /// One surface, two renderers — asserted against each other.
    ///
    /// Every label the paint tier places must occur in the askama HTML, because
    /// both read the same stored resolution. A second, divergent resolution
    /// introduced on either side would break this without breaking either
    /// renderer's own tests.
    #[test]
    fn both_renderers_agree_because_they_share_one_resolution() {
        let d = demo();
        assert!(d.html.contains("data-concept=\"vorgang\""), "{}", d.html);
        for f in &d.fields {
            assert!(
                d.html.contains(&f.label),
                "label {:?} is painted but missing from the askama render",
                f.label
            );
        }
        // And the field the wire never carried is in NEITHER projection.
        assert!(
            !d.html.contains("Rabatt"),
            "the unsent field leaked into the HTML render"
        );
    }

    /// The grid skin makes the wire's absence VISIBLE — the cell stays empty.
    ///
    /// `Form` and `Flow` place by iteration order, so a gap in the position
    /// sequence moves nothing and the missing field is invisible. `Grid` reads
    /// `position` as a row-major cell, so with `cols = 3` position 4 must land
    /// at row 1 **column 1**, leaving row 1 column 0 — the unsent position's
    /// cell — empty.
    ///
    /// The x-coordinate is the assertion because it is what distinguishes the
    /// two readings: if the layout had merely *packed* the four surviving
    /// fields, the fourth would sit in column 0 at the left margin, exactly
    /// where a reader would then wrongly conclude position 3 had rendered.
    #[test]
    fn the_grid_skin_leaves_the_unsent_cell_empty() {
        const COLS: u8 = 3;
        let (fields, actions) = surface();
        let vp = Viewport::new(1000.0, 700.0);
        let l = layout_with_skin(fields, actions, &vp, Skin::Grid { cols: COLS });

        let cell = |p: u8| l.fields.iter().find(|f| f.position == p).expect("placed");
        let col0_x = cell(0).label_rect.x;
        let col1_x = cell(1).label_rect.x;
        assert!(col1_x > col0_x, "columns must advance left to right");

        let last = cell(4);
        assert!(
            (last.label_rect.x - col1_x).abs() < 0.5,
            "position 4 belongs in column {} (x≈{col1_x}), not column 0 — it landed at x={}",
            4 % COLS,
            last.label_rect.x
        );
        assert!(
            last.label_rect.y > cell(0).label_rect.y,
            "position 4 belongs on row {}, below the first row",
            4 / COLS
        );
        // Nothing occupies the unsent position's cell (row 1, column 0).
        assert!(
            !l.fields
                .iter()
                .any(|f| (f.label_rect.x - col0_x).abs() < 0.5
                    && (f.label_rect.y - last.label_rect.y).abs() < 0.5),
            "row 1 column 0 is the unsent position's cell and must stay empty"
        );
    }

    /// T1: the skin changes the PLACEMENT, never the surface. Same fields, same
    /// addresses, different rects — which is the whole claim.
    #[test]
    fn both_skins_place_the_same_addressed_surface() {
        let (fields, actions) = surface();
        let vp = Viewport::new(1000.0, 700.0);
        let form = layout_with_skin(fields, actions, &vp, Skin::Form);
        let flow = layout_with_skin(fields, actions, &vp, Skin::Flow);

        let addr = |l: &PaintLayout| l.fields.iter().map(|f| f.position).collect::<Vec<_>>();
        assert_eq!(addr(&form), addr(&flow), "same addresses under both skins");
        assert_eq!(addr(&form), vec![0, 1, 2, 4], "the surface's own positions");
        assert_ne!(
            form.fields[0].value_rect, flow.fields[0].value_rect,
            "the skins must actually place differently, or this proves nothing"
        );
    }

    /// T2: a pixel resolves to an ordinal, and the ordinal builds a real frame.
    /// Falsifier: clicking each action's own centre must yield ITS ordinal —
    /// an off-by-one in placement or hit-test fails here rather than silently
    /// invoking the neighbouring action.
    #[test]
    fn every_action_centre_hits_its_own_ordinal_and_encodes() {
        let (fields, actions) = surface();
        let vp = Viewport::new(1000.0, 700.0);
        let l = layout_with_skin(fields, actions, &vp, Skin::Form);
        assert_eq!(l.actions.len(), 3, "fixture has three actions");

        for placed in &l.actions {
            let cx = placed.rect.x + placed.rect.w / 2.0;
            let cy = placed.rect.y + placed.rect.h / 2.0;
            assert_eq!(
                l.hit_test(cx, cy),
                Some(Hit::Action(placed.ordinal)),
                "centre of {:?} must resolve to its own ordinal",
                placed.label
            );

            let bytes = Frame::ActionInvoke(ActionInvoke {
                key: DEMO_KEY,
                action_ordinal: placed.ordinal,
                args: Vec::new(),
            })
            .to_le_bytes();
            assert!(!bytes.is_empty(), "the up-frame must encode to LE bytes");
        }
    }

    /// The can-it-stay-silent twin: a point outside every rect must hit
    /// nothing. Without this, a hit-test that returned `Some` unconditionally
    /// would pass the test above.
    #[test]
    fn a_click_in_empty_space_hits_nothing() {
        let (fields, actions) = surface();
        let vp = Viewport::new(1000.0, 700.0);
        let l = layout_with_skin(fields, actions, &vp, Skin::Form);
        assert_eq!(l.hit_test(990.0, 690.0), None, "far corner is empty");
    }

    /// The viewport's device class drives the adaptive layout, so a mobile
    /// width must actually change the placement — otherwise `DeviceClass` is
    /// decoration.
    #[test]
    fn mobile_width_changes_the_placement() {
        let (fields, actions) = surface();
        let desktop = Viewport::new(1000.0, 700.0);
        let mobile = Viewport::new(420.0, 700.0);
        assert_eq!(desktop.device, DeviceClass::Desktop);
        assert_eq!(mobile.device, DeviceClass::Mobile);

        let d = layout_with_skin(fields, actions, &desktop, Skin::Form);
        let m = layout_with_skin(fields, actions, &mobile, Skin::Form);
        assert_ne!(d.fields[0].value_rect, m.fields[0].value_rect);
    }
}
