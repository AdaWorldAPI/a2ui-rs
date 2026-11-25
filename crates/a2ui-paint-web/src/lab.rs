//! The Material Laboratory: one glass element over calibration backgrounds, rendered from
//! the maps [`a2ui_paint::optics`] computes. The page holds no optics of its own: every map
//! comes from `/lab/optics/map` and the filter parameters from `/lab/optics/params` (in
//! response headers), so what is judged here is the Rust engine itself (plan
//! `.claude/plans/a2ui-material-appearance-v1.md` §7, step B).
//!
//! Each request computes only the map it names, off the async runtime, at most
//! [`CONCURRENT_RENDERS`] at a time. The page keys each map by the parameters that map depends
//! on, so a colour change fetches only the rear light; the glass maps stay cached.
//!
//! Visual acceptance (gate G1) is the operator's and happens on this page; the unit tests
//! of `a2ui_paint::optics` cover G2, G3 and G5.

use std::sync::OnceLock;
use std::time::Instant;

use tokio::sync::Semaphore;

use a2ui_paint::optics::{
    Droplet, Glass, KeyLight, Medium, Profile, RearLight, png_rgba, rear_field,
};
use axum::{
    Router,
    extract::RawQuery,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};

/// The page. Static: its controls only build `/lab/optics/map` URLs.
const PAGE: &str = include_str!("lab.html");

/// The raindrop experiment: a movable droplet over a page of text.
const DROP_PAGE: &str = include_str!("drop.html");

/// The raindrop on the GPU: the same droplet evaluated by the WebGL2 shader of
/// `a2ui-optics-gl` (wasm, raw WebGL2, no wgpu).
const DROP_GL_PAGE: &str = include_str!("drop-gl.html");

/// Largest map edge the server renders, px.
const MAX_EDGE: u32 = 1600;

/// Maps computed at the same time; further requests wait.
const CONCURRENT_RENDERS: usize = 2;

fn permits() -> &'static Semaphore {
    static P: OnceLock<Semaphore> = OnceLock::new();
    P.get_or_init(|| Semaphore::new(CONCURRENT_RENDERS))
}

/// Add the laboratory routes.
pub fn routes(app: Router) -> Router {
    app.route("/lab/optics", get(page))
        .route("/lab/optics/map", get(map))
        .route("/lab/optics/params", get(params))
        .route("/lab/optics/drop", get(drop_page))
        .route("/lab/optics/drop/map", get(drop_map))
        .route("/lab/optics/drop-gl", get(drop_gl_page))
        .route("/lab/optics/gl/{file}", get(gl_file))
}

async fn drop_gl_page() -> Html<&'static str> {
    Html(DROP_GL_PAGE)
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

async fn drop_page() -> Html<&'static str> {
    Html(DROP_PAGE)
}

/// The query of `/lab/optics/drop/map`: geometry, medium, dispersion exaggeration, light.
#[derive(Debug, Clone, PartialEq)]
struct DropQuery {
    /// `disp`, `spec` or `rim`.
    kind: String,
    droplet: Droplet,
    medium: Medium,
    dispersion: f32,
    light: KeyLight,
}

impl DropQuery {
    fn parse(raw: &str) -> Self {
        let mut q = Self {
            kind: "disp".into(),
            droplet: Droplet {
                radius: 90.0,
                height: 28.0,
            },
            medium: Medium::WATER,
            dispersion: 0.0,
            light: KeyLight::DEFAULT,
        };
        let mut curv = 0.85;
        for pair in raw.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let n = || v.parse::<f32>().ok().filter(|x| x.is_finite());
            match k {
                "kind" if matches!(v, "disp" | "spec" | "rim") => q.kind = v.into(),
                "r" => q.droplet.radius = n().map_or(q.droplet.radius, |x| x.clamp(8.0, 400.0)),
                // curvature: apex height as a fraction of the footprint radius
                "curv" => curv = n().map_or(curv, |x| x.clamp(0.02, 1.0)),
                "medium" => {
                    q.medium = match v {
                        "glass" => Medium::CROWN_GLASS,
                        "diamond" => Medium::DIAMOND,
                        _ => Medium::WATER,
                    }
                }
                "disp" => q.dispersion = n().map_or(q.dispersion, |x| x.clamp(0.0, 40.0)),
                "light" => {
                    q.light.angle_deg = n().map_or(q.light.angle_deg, |x| x.clamp(-360.0, 360.0))
                }
                "spec" => q.light.intensity = n().map_or(q.light.intensity, |x| x.clamp(0.0, 1.0)),
                "rim" => q.light.rim = n().map_or(q.light.rim, |x| x.clamp(0.0, 1.0)),
                _ => {}
            }
        }
        q.droplet.height = curv * q.droplet.radius;
        q
    }
}

/// One droplet map as PNG; the per-channel filter scales come back as headers.
async fn drop_map(RawQuery(raw): RawQuery) -> Response {
    let q = DropQuery::parse(raw.as_deref().unwrap_or(""));
    let Ok(_permit) = permits().acquire().await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "renderer closed").into_response();
    };
    let out = tokio::task::spawn_blocking(move || {
        let t = Instant::now();
        let m = q.droplet.render(q.medium, q.light, q.dispersion);
        let render_us = t.elapsed().as_micros();
        let buf = match q.kind.as_str() {
            "spec" => &m.specular,
            "rim" => &m.rim,
            _ => &m.displacement,
        };
        let png = png_rgba(m.side, m.side, buf)?;
        let [r, g, b] = m.channel_scales;
        Ok::<_, a2ui_paint::optics::PngError>((
            png,
            vec![
                ("x-a2ui-scale-r", format!("{r:.3}")),
                ("x-a2ui-scale-g", format!("{g:.3}")),
                ("x-a2ui-scale-b", format!("{b:.3}")),
                ("x-a2ui-side", m.side.to_string()),
                (
                    "x-a2ui-magnification",
                    format!("{:.3}", q.droplet.centre_magnification(q.medium.ior)),
                ),
                ("x-a2ui-render-us", render_us.to_string()),
            ],
        ))
    })
    .await;
    match out {
        Ok(Ok((png, headers))) => {
            let mut res = (
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                png,
            )
                .into_response();
            set_headers(&mut res, headers);
            res
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "map rendering failed").into_response(),
    }
}

async fn page() -> Html<&'static str> {
    Html(PAGE)
}

/// Which map to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Displacement,
    Specular,
    Fresnel,
    Rear,
}

/// The query of `/lab/optics/map`: size, glass, rear light. Unknown keys are ignored and
/// out-of-range values clamped, so any URL renders something.
#[derive(Debug, Clone, PartialEq)]
struct MapQuery {
    kind: Kind,
    width: u32,
    height: u32,
    glass: Glass,
    rear: RearLight,
}

impl MapQuery {
    fn parse(raw: &str) -> Self {
        let mut q = Self {
            kind: Kind::Displacement,
            width: 420,
            height: 200,
            glass: Glass::DEFAULT,
            rear: RearLight {
                hue: 352.0,
                sat: 0.75,
                halo: 0.45,
            },
        };
        for pair in raw.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let n = || v.parse::<f32>().ok().filter(|x| x.is_finite());
            let g = &mut q.glass;
            match k {
                "kind" => {
                    q.kind = match v {
                        "spec" => Kind::Specular,
                        "fresnel" => Kind::Fresnel,
                        "rear" => Kind::Rear,
                        _ => Kind::Displacement,
                    }
                }
                "w" => q.width = n().map_or(q.width, |x| x as u32),
                "h" => q.height = n().map_or(q.height, |x| x as u32),
                "profile" => g.profile = Profile::parse(v).unwrap_or(g.profile),
                "radius" => g.radius = n().map_or(g.radius, |x| x.clamp(0.0, 400.0)),
                "bezel" => g.bezel = n().map_or(g.bezel, |x| x.clamp(1.0, 200.0)),
                "thickness" => g.thickness = n().map_or(g.thickness, |x| x.clamp(0.0, 200.0)),
                "depth" => g.depth = n().map_or(g.depth, |x| x.clamp(0.0, 200.0)),
                "ior" => g.ior = n().map_or(g.ior, |x| x.clamp(1.0, 3.0)),
                "refraction" => g.refraction = n().map_or(g.refraction, |x| x.clamp(0.0, 4.0)),
                "fine" => g.fine_sigma = n().map_or(g.fine_sigma, |x| x.clamp(0.0, 20.0)),
                "wide" => g.wide_sigma = n().map_or(g.wide_sigma, |x| x.clamp(0.0, 60.0)),
                "share" => g.fine_share = n().map_or(g.fine_share, |x| x.clamp(0.0, 1.0)),
                "tint" => g.tint = n().map_or(g.tint, |x| x.clamp(0.0, 1.0)),
                "reflect" => g.reflection = n().map_or(g.reflection, |x| x.clamp(0.0, 4.0)),
                "light" => {
                    g.light_angle_deg = n().map_or(g.light_angle_deg, |x| x.clamp(-360.0, 360.0))
                }
                "specexp" => g.spec_exp = n().map_or(g.spec_exp, |x| x.clamp(1.0, 64.0)),
                "specop" => g.spec_opacity = n().map_or(g.spec_opacity, |x| x.clamp(0.0, 1.0)),
                "strength" => g.strength = n().map_or(g.strength, |x| x.clamp(0.0, 1.0)),
                "hue" => q.rear.hue = n().map_or(q.rear.hue, |x| x.rem_euclid(360.0)),
                "sat" => q.rear.sat = n().map_or(q.rear.sat, |x| x.clamp(0.0, 1.0)),
                "halo" => q.rear.halo = n().map_or(q.rear.halo, |x| x.clamp(0.0, 1.0)),
                _ => {}
            }
        }
        q.width = q.width.clamp(1, MAX_EDGE);
        q.height = q.height.clamp(1, MAX_EDGE);
        q
    }
}

/// The filter parameters and the cross-section's maximum for a glass, in response headers.
/// Cheap (one 129-sample cross-section), so the page fetches it on every change.
async fn params(RawQuery(raw): RawQuery) -> Response {
    let q = MapQuery::parse(raw.as_deref().unwrap_or(""));
    let max = q.glass.cross_section().max;
    let mut res = (
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response();
    set_headers(&mut res, filter_headers(&q.glass, max));
    res
}

/// One map as PNG: only the map named by `kind`. `x-a2ui-render-us` is the arithmetic only,
/// `x-a2ui-encode-us` the PNG encoding.
async fn map(RawQuery(raw): RawQuery) -> Response {
    let q = MapQuery::parse(raw.as_deref().unwrap_or(""));
    let Ok(_permit) = permits().acquire().await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "renderer closed").into_response();
    };
    match tokio::task::spawn_blocking(move || render_png(&q)).await {
        Ok(Ok((png, headers))) => {
            let mut res = (
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                png,
            )
                .into_response();
            set_headers(&mut res, headers);
            res
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "map rendering failed").into_response(),
    }
}

/// The RGBA buffer of the requested map.
fn render_rgba(q: &MapQuery) -> Vec<u8> {
    match q.kind {
        Kind::Displacement => {
            let cs = q.glass.cross_section();
            q.glass.render_displacement(q.width, q.height, &cs)
        }
        Kind::Specular => q.glass.render_specular(q.width, q.height),
        Kind::Fresnel => q.glass.render_fresnel(q.width, q.height),
        Kind::Rear => rear_field(q.width, q.height, q.rear, q.glass.strength),
    }
}

type Headers = Vec<(&'static str, String)>;

fn render_png(q: &MapQuery) -> Result<(Vec<u8>, Headers), a2ui_paint::optics::PngError> {
    let t = Instant::now();
    let rgba = render_rgba(q);
    let render_us = t.elapsed().as_micros();
    let t = Instant::now();
    let png = png_rgba(q.width, q.height, &rgba)?;
    let encode_us = t.elapsed().as_micros();
    let headers = vec![
        ("x-a2ui-render-us", render_us.to_string()),
        ("x-a2ui-encode-us", encode_us.to_string()),
        ("x-a2ui-size", format!("{}x{}", q.width, q.height)),
    ];
    Ok((png, headers))
}

fn filter_headers(glass: &Glass, max: f32) -> Headers {
    let p = glass.filter_params(max);
    vec![
        (
            "x-a2ui-filter-scale",
            format!("{:.3}", p.displacement_scale),
        ),
        ("x-a2ui-filter-fine", format!("{:.3}", p.fine_sigma)),
        ("x-a2ui-filter-wide", format!("{:.3}", p.wide_sigma)),
        ("x-a2ui-filter-share", format!("{:.3}", p.fine_share)),
        ("x-a2ui-filter-tint", format!("{:.3}", p.tint_alpha)),
        ("x-a2ui-max-displacement", format!("{max:.3}")),
        ("x-a2ui-f0", format!("{:.4}", glass.sanitized().f0())),
    ]
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

    #[test]
    fn the_query_defaults_to_the_prototype_and_clamps() {
        let q = MapQuery::parse("");
        assert_eq!(q.glass, Glass::DEFAULT);
        assert_eq!((q.width, q.height, q.kind), (420, 200, Kind::Displacement));
        let q =
            MapQuery::parse("kind=rear&w=99999&h=0&ior=9&strength=-1&profile=circle&hue=-10&bogus");
        assert_eq!((q.width, q.height, q.kind), (MAX_EDGE, 1, Kind::Rear));
        assert_eq!(
            (q.glass.ior, q.glass.strength, q.glass.profile),
            (3.0, 0.0, Profile::Circle)
        );
        assert_eq!(q.rear.hue, 350.0);
    }

    #[test]
    fn non_finite_values_keep_the_default() {
        let q = MapQuery::parse("ior=NaN&radius=inf");
        assert_eq!(
            (q.glass.ior, q.glass.radius),
            (Glass::DEFAULT.ior, Glass::DEFAULT.radius)
        );
    }

    /// The page holds no optics of its own: no SDF, no Snell, no map arithmetic. It only
    /// builds `/lab/optics/map` URLs and reads the filter headers.
    #[test]
    fn the_page_renders_the_engine_not_a_copy() {
        assert!(PAGE.contains("/lab/optics/map"));
        assert!(PAGE.contains("/lab/optics/params"));
        assert!(PAGE.contains("x-a2ui-filter-scale"));
        for forbidden in ["Math.sqrt", "Math.hypot", "createImageData", "putImageData"] {
            assert!(
                !PAGE.contains(forbidden),
                "lab.html computes optics itself: {forbidden}"
            );
        }
    }

    /// G5 on the page: the rear-light layer carries no mask and no clip.
    #[test]
    fn the_rear_layer_has_no_mask_or_clip() {
        let css = PAGE
            .split("<style>")
            .nth(1)
            .and_then(|s| s.split("</style>").next())
            .unwrap();
        let rear = css
            .split('}')
            .find(|r| r.trim_start().starts_with("#rear"))
            .expect("#rear rule");
        for forbidden in ["mask", "clip", "overflow"] {
            assert!(!rear.contains(forbidden), "#rear uses {forbidden}: {rear}");
        }
    }

    /// G2 at the HTTP layer: the page sends colour with every request, so two presets must
    /// yield byte-identical glass maps and different rear lights.
    #[test]
    fn colour_reaches_only_the_rear_light() {
        let ruby = "w=180&h=90&hue=352&sat=0.75&halo=0.45&strength=0.53";
        let ocean = "w=180&h=90&hue=212&sat=0.80&halo=0.45&strength=0.53";
        for kind in ["disp", "spec"] {
            let a = render_rgba(&MapQuery::parse(&format!("kind={kind}&{ruby}")));
            let b = render_rgba(&MapQuery::parse(&format!("kind={kind}&{ocean}")));
            assert_eq!(a, b, "{kind} depends on colour");
        }
        let a = render_rgba(&MapQuery::parse(&format!("kind=rear&{ruby}")));
        let b = render_rgba(&MapQuery::parse(&format!("kind=rear&{ocean}")));
        assert_ne!(a, b, "the rear light must carry the colour");
    }

    /// The page keys the glass maps without colour, so a colour change keeps them cached.
    #[test]
    fn the_page_keys_glass_maps_without_colour() {
        let deps = |kind: &str| {
            let start = PAGE
                .find(&format!("    {kind}: ["))
                .unwrap_or_else(|| panic!("no DEPS.{kind}"));
            let line = &PAGE[start..];
            line[..line.find(']').unwrap()].to_string()
        };
        for kind in ["params", "disp", "spec"] {
            let d = deps(kind);
            for colour in ["'hue'", "'sat'", "'halo'"] {
                assert!(!d.contains(colour), "{kind} is keyed by {colour}: {d}");
            }
        }
        assert!(deps("rear").contains("'hue'"));
    }

    /// Each request renders only its own map, at its own size (the rear box is not the
    /// glass size).
    #[test]
    fn a_request_renders_only_its_map_at_its_size() {
        let q = MapQuery::parse("kind=rear&w=370&h=260");
        assert_eq!(render_rgba(&q).len(), 370 * 260 * 4);
        let (png, headers) = render_png(&MapQuery::parse("kind=spec&w=64&h=40")).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(
            headers
                .iter()
                .any(|(k, v)| *k == "x-a2ui-size" && v == "64x40")
        );
    }

    #[test]
    fn the_drop_query_reads_geometry_medium_and_dispersion() {
        let q = DropQuery::parse("kind=rim&r=120&curv=0.5&medium=diamond&disp=4&bogus=1");
        assert_eq!(q.kind, "rim");
        assert_eq!((q.droplet.radius, q.droplet.height), (120.0, 60.0));
        assert_eq!((q.medium, q.dispersion), (Medium::DIAMOND, 4.0));
        let d = DropQuery::parse("curv=NaN&r=9999&medium=x");
        assert_eq!((d.droplet.radius, d.medium), (400.0, Medium::WATER));
        assert!((d.droplet.height / d.droplet.radius - 0.85).abs() < 1e-6);
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
        assert!(DROP_GL_PAGE.contains("/lab/optics/gl/a2ui_optics_gl.js"));
        assert!(DROP_GL_PAGE.contains("new mod.DropRenderer"));
        for forbidden in [
            "Math.sqrt",
            "Math.hypot",
            "createImageData",
            "putImageData",
            "getContext('webgl",
        ] {
            assert!(
                !DROP_GL_PAGE.contains(forbidden),
                "drop-gl.html computes optics itself: {forbidden}"
            );
        }
    }

    /// The droplet page, like the card lab, computes no optics itself.
    #[test]
    fn the_drop_page_renders_the_engine_not_a_copy() {
        assert!(DROP_PAGE.contains("/lab/optics/drop/map") && DROP_PAGE.contains("x-a2ui-scale-b"));
        for forbidden in ["Math.sqrt", "Math.hypot", "createImageData", "putImageData"] {
            assert!(
                !DROP_PAGE.contains(forbidden),
                "drop.html computes optics itself: {forbidden}"
            );
        }
    }
}
