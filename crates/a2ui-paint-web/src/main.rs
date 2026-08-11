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
//! Deliberately NOT here: `wgpu` (the container has no GPU, and the paint-DATA
//! path is pure without it), `a2ui-server` (RBAC projection + sealed transport
//! are a different tier and pull the whole lance-graph tree in), and any
//! persistence. The surface below is a hardcoded fixture — this demonstrates
//! the *paint* tier, not where surfaces come from.
//!
//! Port: `$PORT` from the environment (Railway injects it); 8080 only as a
//! local fallback.

use std::net::SocketAddr;

use a2ui_core::{ActionInvoke, Frame};
use a2ui_paint::{Hit, PaintLayout, Skin, Viewport, layout_with_skin};
use axum::{
    Router,
    extract::RawQuery,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use ogar_render_askama::{ActionRef, FieldView};

/// The demo node the actions address. A real consumer resolves this from the
/// `NodeDelta` it is rendering; here it is a fixed 16-byte canonical key so the
/// emitted `ActionInvoke` is a complete, well-formed frame rather than a stub.
const DEMO_KEY: [u8; 16] = [
    0x01, 0x02, 0x00, 0x00, // classid
    0x00, 0x00, 0x00, 0x00, // HEEL / HIP
    0x00, 0x00, // TWIG
    0x00, 0x00, 0x00, 0x2A, 0x00, 0x07, // tail
];

/// The resolved surface under demonstration.
///
/// `position` is the field's mask-index ADDRESS (not a layout slot) and
/// `ordinal` is the action's index into the class's `ActionDef` set. Both are
/// addresses the class registry resolves — which is exactly why a click can
/// answer with a number instead of a function.
fn surface() -> (Vec<FieldView>, Vec<ActionRef>) {
    let fields = vec![
        FieldView {
            position: 0,
            label: "Belegnummer".into(),
            predicate: "name".into(),
            value: "RE-2026-0042".into(),
        },
        FieldView {
            position: 1,
            label: "Partner".into(),
            predicate: "partner_id".into(),
            value: "Muster GmbH".into(),
        },
        FieldView {
            position: 2,
            label: "Betrag".into(),
            predicate: "amount_total".into(),
            value: "1.240,00 EUR".into(),
        },
        FieldView {
            position: 4,
            label: "Status".into(),
            predicate: "state".into(),
            value: "Entwurf".into(),
        },
    ];
    let actions = vec![
        ActionRef {
            ordinal: 0,
            label: "Ansehen".into(),
        },
        ActionRef {
            ordinal: 1,
            label: "Buchen".into(),
        },
        ActionRef {
            ordinal: 2,
            label: "Stornieren".into(),
        },
    ];
    (fields, actions)
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
            _ => Skin::Form,
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
    let l = layout_with_skin(&fields, &actions, &vp, skin);
    let svg = layout_to_svg(&l, &vp, skin);

    // The click handler posts coordinates and shows the LE up-frame that comes
    // back. Note what it does NOT do: it never names an action, only a point.
    Html(format!(
        r##"<!doctype html><meta charset="utf-8"><title>a2ui paint tier</title>
<body style="margin:0;background:#0f1115;color:#cbd3e1;font:13px ui-sans-serif,system-ui,sans-serif">
<div style="padding:10px 12px">
  <a href="/?skin=form" style="color:#7aa2f7">form</a> ·
  <a href="/?skin=flow" style="color:#7aa2f7">flow</a> ·
  <a href="/?w=420&amp;h=700&amp;skin=form" style="color:#7aa2f7">mobile</a>
  <span style="color:#5b6478"> — one surface, many skins (T1)</span>
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
        w = vp.width,
        h = vp.height,
        sk = if matches!(skin, Skin::Flow) {
            "flow"
        } else {
            "form"
        },
    ))
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
    let l = layout_with_skin(&fields, &actions, &vp, q.skin());

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
    let l = layout_with_skin(&fields, &actions, &vp, q.skin());

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
    let (f, a) = surface();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "ok\nsurface: {} fields, {} actions\nskins: form, flow\nwgpu: off (no GPU in container)\n",
            f.len(),
            a.len()
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
        .route("/hit", get(hit))
        .route("/hit", post(hit))
        .route("/hit.bin", get(hit_frame))
        .route("/health", get(health));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr} failed: {e}"));

    // Say which state the deploy is in, at boot, on stderr — see `health`.
    let (f, a) = surface();
    eprintln!(
        "a2ui-paint-web listening on {addr} (PORT={}) — surface: {} fields / {} actions, wgpu off",
        std::env::var("PORT").unwrap_or_else(|_| "unset→8080".into()),
        f.len(),
        a.len(),
    );

    axum::serve(listener, app).await.expect("server");
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_paint::DeviceClass;

    /// T1: the skin changes the PLACEMENT, never the surface. Same fields, same
    /// addresses, different rects — which is the whole claim.
    #[test]
    fn both_skins_place_the_same_addressed_surface() {
        let (fields, actions) = surface();
        let vp = Viewport::new(1000.0, 700.0);
        let form = layout_with_skin(&fields, &actions, &vp, Skin::Form);
        let flow = layout_with_skin(&fields, &actions, &vp, Skin::Flow);

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
        let l = layout_with_skin(&fields, &actions, &vp, Skin::Form);
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
        let l = layout_with_skin(&fields, &actions, &vp, Skin::Form);
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

        let d = layout_with_skin(&fields, &actions, &desktop, Skin::Form);
        let m = layout_with_skin(&fields, &actions, &mobile, Skin::Form);
        assert_ne!(d.fields[0].value_rect, m.fields[0].value_rect);
    }
}
