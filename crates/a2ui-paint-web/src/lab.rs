//! The Material Laboratory: optical surfaces over calibration scenes, rendered from the one
//! specification in [`a2ui_paint::optics::surface`]. Two backends: SVG filters over the CPU
//! maps (`/lab/optics/surface/map`, the native-DOM path report-rs would use) and the WebGL2
//! shader of `a2ui-optics-gl` (`/lab/optics/gl/…`, for A2UI-owned scenes). The pages hold no
//! optics of their own; every number comes from Rust.
//!
//! Pages: `/lab/optics` (one surface, calibration backgrounds, stages, both backends;
//! `/lab/optics/drop` and `/lab/optics/drop-gl` open it on the droplet) and
//! `/lab/optics/hub` (the Service Hub fixture of report-rs #208/#209: neutral cards and the
//! Ruby-lit Dashboard card, native SVG/DOM composition).
//!
//! Each map request computes only the map it names, off the async runtime, at most
//! [`CONCURRENT_RENDERS`] at a time. Visual acceptance (gate G1b) is the operator's; the unit
//! tests of `a2ui_paint::optics` cover G2, G3 and G5.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use tokio::sync::Semaphore;

use a2ui_paint::optics::{RearLight, png_rgba, rear_field};
use axum::{
    Router,
    extract::RawQuery,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};

/// One surface over calibration backgrounds, on both backends.
const SURFACE_PAGE: &str = include_str!("surface.html");

/// The Service Hub fixture (report-rs #208/#209) under the shared surface specification.
const HUB_PAGE: &str = include_str!("hub.html");

/// Largest map edge the server renders, px.
const MAX_EDGE: u32 = 1600;

/// Maps computed at the same time; further requests wait.
const CONCURRENT_RENDERS: usize = 2;

type Headers = Vec<(&'static str, String)>;

/// The render permits. Each render takes an owned permit into its blocking task, so a
/// request that is cancelled mid-render still holds its permit until the render ends.
fn permits() -> Arc<Semaphore> {
    static P: OnceLock<Arc<Semaphore>> = OnceLock::new();
    P.get_or_init(|| Arc::new(Semaphore::new(CONCURRENT_RENDERS)))
        .clone()
}

/// Add the laboratory routes.
pub fn routes(app: Router) -> Router {
    app.route("/lab/optics", get(surface_page))
        // the earlier droplet pages open the surface page on the droplet preset
        .route("/lab/optics/drop", get(surface_page))
        .route("/lab/optics/drop-gl", get(surface_page))
        .route("/lab/optics/surface", get(surface_page))
        .route("/lab/optics/surface/map", get(surface_map))
        .route("/lab/optics/surface/params", get(surface_params))
        .route("/lab/optics/rear", get(rear_map))
        .route("/lab/optics/hub", get(hub_page))
        .route("/lab/optics/gl/{file}", get(gl_file))
}

async fn hub_page() -> Html<&'static str> {
    Html(HUB_PAGE)
}

/// The rear-light query: box size and the light (`hue`, `sat`, `halo`) at `strength`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RearQuery {
    width: u32,
    height: u32,
    light: RearLight,
    strength: f32,
}

impl RearQuery {
    fn parse(raw: &str) -> Self {
        let mut q = Self {
            width: 420,
            height: 300,
            light: RearLight {
                hue: 352.0,
                sat: 0.75,
                halo: 0.45,
            },
            strength: 1.0,
        };
        for pair in raw.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let n = || v.parse::<f32>().ok().filter(|x| x.is_finite());
            let edge = |x: f32| x.clamp(1.0, MAX_EDGE as f32) as u32;
            match k {
                "w" => q.width = n().map_or(q.width, edge),
                "h" => q.height = n().map_or(q.height, edge),
                "hue" => q.light.hue = n().map_or(q.light.hue, |x| x.rem_euclid(360.0)),
                "sat" => q.light.sat = n().map_or(q.light.sat, |x| x.clamp(0.0, 1.0)),
                "halo" => q.light.halo = n().map_or(q.light.halo, |x| x.clamp(0.0, 1.0)),
                "strength" => q.strength = n().map_or(q.strength, |x| x.clamp(0.0, 1.0)),
                _ => {}
            }
        }
        q
    }

    fn render(&self) -> Vec<u8> {
        rear_field(self.width, self.height, self.light, self.strength)
    }
}

/// The rear light behind an active surface as PNG (the only place the atmosphere's colour
/// enters, gate G2).
async fn rear_map(RawQuery(raw): RawQuery) -> Response {
    let q = RearQuery::parse(raw.as_deref().unwrap_or(""));
    let Ok(permit) = permits().acquire_owned().await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "renderer closed").into_response();
    };
    let out = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let t = Instant::now();
        let rgba = q.render();
        let us = t.elapsed().as_micros();
        png_rgba(q.width, q.height, &rgba).map(|png| (png, us))
    })
    .await;
    match out {
        Ok(Ok((png, us))) => {
            let mut res = (
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                png,
            )
                .into_response();
            set_headers(&mut res, vec![("x-a2ui-render-us", us.to_string())]);
            res
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "map rendering failed").into_response(),
    }
}

async fn surface_page() -> Html<&'static str> {
    Html(SURFACE_PAGE)
}

/// A preset's full parameter set as a query string (`Surface::to_query`): the page fills its
/// controls from it, so preset values exist only in Rust.
async fn surface_params(RawQuery(raw): RawQuery) -> Response {
    let s = a2ui_paint::optics::surface::Surface::from_query(raw.as_deref().unwrap_or(""));
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        s.to_query(),
    )
        .into_response()
}

/// One map of a surface as PNG (`kind` = `disp|spec|rim`; every other key is the shared
/// `Surface::from_query` contract, at the page's device pixel ratio `dpr`). Headers carry the
/// per-channel filter scales, the box, and the SVG encoding's error against the reference.
async fn surface_map(RawQuery(raw): RawQuery) -> Response {
    let req = a2ui_paint::optics::map_service::MapRequest::parse(&raw.unwrap_or_default());
    let (bw, bh) = req.map_box();
    if bw.max(bh) > 4 * MAX_EDGE {
        return (StatusCode::BAD_REQUEST, "surface too large").into_response();
    }
    let Ok(permit) = permits().acquire_owned().await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "renderer closed").into_response();
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        req.render()
    })
    .await
    {
        Ok(Ok(out)) => {
            let mut res = (
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                out.png,
            )
                .into_response();
            set_headers(&mut res, out.headers);
            res
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "map rendering failed").into_response(),
    }
}

/// The only files the GPU page loads, and the only ones served. A whitelist, not a
/// sanitiser: three known names cannot traverse anywhere.
fn gl_file_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "a2ui_optics_gl.js" => "text/javascript; charset=utf-8",
        "a2ui_optics_gl_bg.wasm" => "application/wasm",
        "a2ui_optics_gl.d.ts" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

/// `OPTICS_GL_PKG`, else the build output of `scripts/build-optics-wasm.sh`.
fn gl_pkg_dir() -> std::path::PathBuf {
    std::env::var("OPTICS_GL_PKG").map_or_else(
        |_| concat!(env!("CARGO_MANIFEST_DIR"), "/../a2ui-optics-gl/pkg").into(),
        Into::into,
    )
}

async fn gl_file(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    let Some(mime) = gl_file_name(&file) else {
        return (StatusCode::NOT_FOUND, "no such renderer file").into_response();
    };
    let path = gl_pkg_dir().join(&file);
    match tokio::task::spawn_blocking(move || std::fs::read(&path).map_err(|e| (path, e))).await {
        Ok(Ok(bytes)) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        Ok(Err((path, e))) => (
            StatusCode::NOT_FOUND,
            format!(
                "{} is missing: {e} — run ./scripts/build-optics-wasm.sh",
                path.display()
            ),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn set_headers(res: &mut Response, headers: Headers) {
    let h = res.headers_mut();
    for (name, value) in headers {
        if let Ok(v) = HeaderValue::from_str(&value) {
            h.insert(HeaderName::from_static(name), v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_paint::optics::surface::Surface;

    /// The pages hold no optics of their own: no SDF, no Snell, no map arithmetic. They only
    /// build map URLs and read the headers.
    #[test]
    fn the_pages_render_the_engine_not_a_copy() {
        for (name, page) in [("surface.html", SURFACE_PAGE), ("hub.html", HUB_PAGE)] {
            assert!(page.contains("/lab/optics/surface/map"), "{name}");
            for forbidden in [
                "Math.sqrt",
                "Math.hypot",
                "createImageData",
                "putImageData",
                "getContext('webgl",
            ] {
                assert!(
                    !page.contains(forbidden),
                    "{name} computes optics itself: {forbidden}"
                );
            }
        }
        assert!(SURFACE_PAGE.contains("/lab/optics/surface/params"));
    }

    /// Every background candidate the Hub fixture offers has its own rule, and the plain Hub
    /// gradient stays the first (default) choice.
    #[test]
    fn every_hub_background_candidate_has_a_rule() {
        let select = HUB_PAGE
            .split("<select id=\"scenebg\">")
            .nth(1)
            .and_then(|s| s.split("</select>").next())
            .expect("the background selector");
        let values: Vec<&str> = select
            .split("<option value=\"")
            .skip(1)
            .filter_map(|o| o.split('"').next())
            .collect();
        assert_eq!(values.first(), Some(&"plain"));
        assert!(values.len() > 1);
        for v in &values[1..] {
            assert!(
                HUB_PAGE.contains(&format!(".scene[data-bg={v}]{{")),
                "no rule for {v}"
            );
        }
    }

    /// G5 on the pages: the rear-light layer carries no mask and no clip.
    #[test]
    fn the_rear_layer_has_no_mask_or_clip() {
        for page in [SURFACE_PAGE, HUB_PAGE] {
            let css = page
                .split("<style>")
                .nth(1)
                .and_then(|s| s.split("</style>").next())
                .unwrap();
            let rear = css
                .split('}')
                .find(|r| r.contains(".rear"))
                .expect(".rear rule");
            for forbidden in ["mask", "clip", "overflow"] {
                assert!(!rear.contains(forbidden), "rear uses {forbidden}: {rear}");
            }
        }
    }

    /// G2 at the HTTP layer: colour keys reach only the rear light; the surface ignores them.
    #[test]
    fn colour_reaches_only_the_rear_light() {
        let a = Surface::from_query("preset=aqua&hue=352&sat=0.75").render_maps();
        let b = Surface::from_query("preset=aqua&hue=212&sat=0.80").render_maps();
        assert_eq!(a, b);
        let ruby = RearQuery::parse("w=180&h=90&hue=352&sat=0.75&halo=0.45&strength=0.53");
        let ocean = RearQuery::parse("w=180&h=90&hue=212&sat=0.80&halo=0.45&strength=0.53");
        assert_ne!(
            ruby.render(),
            ocean.render(),
            "the rear light must carry the colour"
        );
    }

    #[test]
    fn the_rear_query_renders_at_its_size_and_clamps() {
        let q = RearQuery::parse("w=370&h=260&halo=NaN&strength=2");
        assert_eq!(q.render().len(), 370 * 260 * 4);
        assert_eq!((q.light.halo, q.strength), (0.45, 1.0));
        assert_eq!(RearQuery::parse("w=99999").width, MAX_EDGE);
    }

    /// The GPU page serves exactly the renderer's files and nothing else.
    #[test]
    fn the_gl_page_serves_only_the_renderer_files() {
        for ok in [
            "a2ui_optics_gl.js",
            "a2ui_optics_gl_bg.wasm",
            "a2ui_optics_gl.d.ts",
        ] {
            assert!(gl_file_name(ok).is_some(), "{ok}");
        }
        for bad in [
            "../Cargo.toml",
            "a2ui_optics_gl.js/../../x",
            "package.json",
            "",
        ] {
            assert!(gl_file_name(bad).is_none(), "{bad}");
        }
        assert_eq!(
            gl_file_name("a2ui_optics_gl_bg.wasm"),
            Some("application/wasm")
        );
    }

    /// The GPU page drives the shared renderer; it computes no optics in JavaScript.
    #[test]
    fn the_gl_page_drives_the_renderer_not_a_copy() {
        assert!(SURFACE_PAGE.contains("/lab/optics/gl/a2ui_optics_gl.js"));
        assert!(SURFACE_PAGE.contains("new mod.SurfaceRenderer"));
        assert!(SURFACE_PAGE.contains("/lab/optics/surface/map"));
        for forbidden in [
            "Math.sqrt",
            "Math.hypot",
            "createImageData",
            "putImageData",
            "getContext('webgl",
        ] {
            assert!(
                !SURFACE_PAGE.contains(forbidden),
                "drop-gl.html computes optics itself: {forbidden}"
            );
        }
    }
}
