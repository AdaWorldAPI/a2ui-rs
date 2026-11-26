//! The surface maps as served PNG images: one request (`kind`, `dpr` and the
//! [`Surface::from_query`] parameters) in, the PNG and the headers that tell an SVG filter how
//! to use it out. Every server that hands the maps to a browser calls this, so the maps and
//! their headers have one source.
//!
//! Headers: `x-a2ui-box` (map size, px), `x-a2ui-scale-r/g/b` (displacement scale per
//! channel), `x-a2ui-corner`, `x-a2ui-weights` (direct, fine, wide), `x-a2ui-sigma`
//! (fine, wide), `x-a2ui-tint` (`r g b alpha`), `x-a2ui-render-us`, and for displacement
//! maps `x-a2ui-svg-error`.

use std::time::Instant;

use super::surface::{ErrorStats, Surface};
use super::{PngError, png_rgba};

/// Which map a request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// The displacement map (`feDisplacementMap`).
    Displacement,
    /// The specular highlight layer.
    Specular,
    /// The Fresnel rim layer.
    Rim,
}

/// One parsed map request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapRequest {
    pub kind: MapKind,
    /// The surface, already scaled to device pixels.
    pub surface: Surface,
}

/// A rendered map and its headers.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPng {
    pub png: Vec<u8>,
    pub headers: Vec<(&'static str, String)>,
}

impl MapRequest {
    /// Parse a query string. Unknown or missing values fall back: `kind` to the displacement
    /// map, `dpr` to 1 (clamped to 0.5..=4), the surface to [`Surface::from_query`]'s rules.
    pub fn parse(raw: &str) -> Self {
        let mut kind = MapKind::Displacement;
        let mut dpr = 1.0f32;
        for pair in raw.split('&') {
            match pair.split_once('=') {
                Some(("kind", "spec")) => kind = MapKind::Specular,
                Some(("kind", "rim")) => kind = MapKind::Rim,
                Some(("dpr", v)) => {
                    dpr = v
                        .parse::<f32>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map_or(1.0, |x| x.clamp(0.5, 4.0))
                }
                _ => {}
            }
        }
        Self {
            kind,
            surface: Surface::from_query(raw).scaled(dpr),
        }
    }

    /// The map size in device pixels, known before rendering (for a size limit).
    pub fn map_box(&self) -> (u32, u32) {
        self.surface.map_box()
    }

    /// Render the requested map. Computes all three maps of the surface and encodes one.
    pub fn render(&self) -> Result<MapPng, PngError> {
        let surface = &self.surface;
        let t = Instant::now();
        let m = surface.render_maps();
        let eff = surface.sanitized().effective();
        let render_us = t.elapsed().as_micros();
        let buf = match self.kind {
            MapKind::Specular => &m.specular,
            MapKind::Rim => &m.rim,
            MapKind::Displacement => &m.displacement,
        };
        let png = png_rgba(m.width, m.height, buf)?;
        let [r, g, b] = m.channel_scales;
        let mut headers = vec![
            ("x-a2ui-scale-r", format!("{r:.3}")),
            ("x-a2ui-scale-g", format!("{g:.3}")),
            ("x-a2ui-scale-b", format!("{b:.3}")),
            ("x-a2ui-box", format!("{}x{}", m.width, m.height)),
            (
                "x-a2ui-corner",
                format!("{:.3}", surface.footprint.corner_px()),
            ),
            ("x-a2ui-weights", {
                let [d, f, w] = surface.scatter.normalized();
                format!("{d:.4} {f:.4} {w:.4}")
            }),
            (
                "x-a2ui-sigma",
                format!("{:.3} {:.3}", eff.fine_sigma, eff.wide_sigma),
            ),
            ("x-a2ui-tint", {
                let [r, g, b] = surface.tint.rgb;
                format!("{r} {g} {b} {:.3}", eff.tint_alpha)
            }),
            ("x-a2ui-render-us", render_us.to_string()),
        ];
        if self.kind == MapKind::Displacement {
            let e = surface.svg_map_error(&m);
            let f = |s: &ErrorStats| format!("{:.3}/{:.3}/{:.3}/{:.3}", s.p50, s.p95, s.p99, s.max);
            headers.push((
                "x-a2ui-svg-error",
                format!("r {} g {} b {}", f(&e[0]), f(&e[1]), f(&e[2])),
            ));
        }
        Ok(MapPng { png, headers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_parses_kind_and_dpr() {
        let r = MapRequest::parse("kind=rim&dpr=2&preset=clear-card&w=200&h=100");
        assert_eq!(r.kind, MapKind::Rim);
        let one = MapRequest::parse("preset=clear-card&w=200&h=100");
        assert_eq!(one.kind, MapKind::Displacement);
        let (w2, _) = r.map_box();
        let (w1, _) = one.map_box();
        assert!(w2 > w1, "dpr 2 doubles the box: {w2} vs {w1}");
        assert_eq!(
            MapRequest::parse("dpr=nan").map_box(),
            MapRequest::parse("").map_box()
        );
    }

    #[test]
    fn the_map_is_a_png_of_the_announced_box() {
        let out = MapRequest::parse("preset=clear-card&w=120&h=60")
            .render()
            .unwrap();
        assert_eq!(&out.png[..8], b"\x89PNG\r\n\x1a\n");
        let bx = &out
            .headers
            .iter()
            .find(|(k, _)| *k == "x-a2ui-box")
            .unwrap()
            .1;
        let w = u32::from_be_bytes(out.png[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(out.png[20..24].try_into().unwrap());
        assert_eq!(bx, &format!("{w}x{h}"));
        assert!(out.headers.iter().any(|(k, _)| *k == "x-a2ui-svg-error"));
        let spec = MapRequest::parse("kind=spec&preset=clear-card&w=120&h=60")
            .render()
            .unwrap();
        assert!(!spec.headers.iter().any(|(k, _)| *k == "x-a2ui-svg-error"));
    }
}
