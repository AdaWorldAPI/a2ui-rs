//! The Material Laboratory: one glass element over calibration backgrounds, rendered from
//! the maps [`a2ui_paint::optics`] computes. The page holds no optics of its own; every
//! map is requested from `/lab/optics/map` and the filter parameters come back in response
//! headers, so what is judged here is the Rust engine itself (plan
//! `.claude/plans/a2ui-material-appearance-v1.md` §7, step B).
//!
//! Visual acceptance (gate G1) is the operator's and happens on this page; the unit tests
//! of `a2ui_paint::optics` cover G2, G3 and G5.

use std::time::Instant;

use a2ui_paint::optics::{Glass, Profile, RearLight, png_rgba, rear_field};
use axum::{
    Router,
    extract::RawQuery,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};

/// The page. Static: its controls only build `/lab/optics/map` URLs.
const PAGE: &str = include_str!("lab.html");

/// Largest map edge the server renders, px.
const MAX_EDGE: u32 = 1600;

/// Add the laboratory routes.
pub fn routes(app: Router) -> Router {
    app.route("/lab/optics", get(page))
        .route("/lab/optics/map", get(map))
}

async fn page() -> Html<&'static str> {
    Html(PAGE)
}

/// Which map to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Displacement,
    Specular,
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

/// One map as PNG. Headers carry what the filter needs (`x-a2ui-filter-*`) and how long
/// the map took to compute (`x-a2ui-render-us`, the arithmetic only, not the encoding).
async fn map(RawQuery(raw): RawQuery) -> Response {
    let q = MapQuery::parse(raw.as_deref().unwrap_or(""));
    let t = Instant::now();
    let (rgba, max) = match q.kind {
        Kind::Displacement | Kind::Specular => {
            let m = q.glass.render(q.width, q.height);
            let buf = if q.kind == Kind::Displacement {
                m.displacement
            } else {
                m.specular
            };
            (buf, m.max_displacement)
        }
        Kind::Rear => (rear_field(q.width, q.height, q.rear, q.glass.strength), 0.0),
    };
    let render_us = t.elapsed().as_micros();
    let Ok(png) = png_rgba(q.width, q.height, &rgba) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "png encoding failed").into_response();
    };
    let p = q.glass.filter_params(max);
    let mut res = (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png,
    )
        .into_response();
    let h = res.headers_mut();
    for (name, value) in [
        (
            "x-a2ui-filter-scale",
            format!("{:.3}", p.displacement_scale),
        ),
        ("x-a2ui-filter-fine", format!("{:.3}", p.fine_sigma)),
        ("x-a2ui-filter-wide", format!("{:.3}", p.wide_sigma)),
        ("x-a2ui-filter-share", format!("{:.3}", p.fine_share)),
        ("x-a2ui-max-displacement", format!("{max:.3}")),
        ("x-a2ui-f0", format!("{:.4}", q.glass.f0())),
        ("x-a2ui-render-us", render_us.to_string()),
    ] {
        if let Ok(v) = HeaderValue::from_str(&value) {
            h.insert(HeaderName::from_static(name), v);
        }
    }
    res
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
}
