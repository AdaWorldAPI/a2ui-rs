//! One optical surface specification: footprint, relief, material, scattering, tint and
//! illumination, each an independent set of parameters. Droplet, capsule and card are
//! [presets](Surface::preset) over this one parameter space, not separate implementations.
//!
//! This module is the **reference**. The SVG path renders its maps from it
//! ([`Surface::render_maps`]); the WebGL2 shader in `a2ui-optics-gl` evaluates the same
//! formulas per pixel and is checked against [`Surface::sample_points`] and
//! [`Surface::coverage`].
//!
//! ## The specification
//!
//! For a point `p` relative to the footprint centre (px, y down):
//!
//! 1. **Footprint.** `w × h` rectangle with corner radius `r = corner · min(w, h)/2`. Signed
//!    distance `sd` (negative inside) and outward unit normal `n` of the nearest boundary
//!    ([`footprint_distance`]). Coverage `clamp(0.5 − sd, 0, 1)` (one-pixel edge ramp).
//!    Interior distance `d = max(−sd, 0)`; inradius `R = min(w, h)/2`.
//! 2. **Relief.** Two height fields over `d`, mixed by the face curvature `c ∈ [0, 1]`:
//!    - shoulder `S(d) = H · s(min(d / W, 1))`, `s(t) = (1 − (1 − t)^p)^(1/p)`
//!      (`W` shoulder width, `p` profile: 1 chamfer, 2 quarter circle, 4 squircle), flat at
//!      `H` beyond the shoulder;
//!    - dome `F(d)`: the spherical cap of apex `min(H, R)` over the inradius, evaluated at
//!      `R − d` (for a circular footprint exactly [`super::Droplet::surface`]).
//!
//!    `h = (1 − c)·S + c·F`, `slope = dh/dd = (1 − c)·S′ + c·F′`, with `t` clamped to
//!    `[1e-4, 1]` so the slope at the contact line is finite and identical in every backend.
//! 3. **Refraction.** Per channel index `n_c` ([`super::Medium::channel_iors`]); a ray
//!    perpendicular to the page refracts at the relief and travels `h + base` down:
//!    `off_c = refract_offset(slope, h + base, n_c)`; the page is sampled at `p + n · off_c`.
//! 4. **Scattering.** Transmitted light is `direct · T(q) + fine · G_σf(q) + wide · G_σw(q)`
//!    with the three weights normalized to sum 1 ([`Scatter::normalized`]). The geometry
//!    reference covers the direct term; blur kernels differ by backend (SVG Gaussian, GPU
//!    mip taps) and are not part of the parity check.
//! 5. **Tint.** `col = col·(1 − a) + tint·a` (the body colour of the material), on the transmitted
//!    light, before the surface reflections of step 6.
//! 6. **Fresnel and light.** Normal `N = (n·slope, 1)/√(slope²+1)`; Schlick
//!    `F = F0 + (1 − F0)(1 − N_z)^5` with `F0` of the green index; rim darkening
//!    `col *= 1 − F·rim`; Blinn-Phong highlight `nh^shininess · min(0.25 + 0.75·F/F0, 4) ·
//!    intensity`, white over.
//!
//! All lengths are in px of the target; a renderer at device pixel ratio `k` uses
//! [`Surface::scaled`]`(k)`.

use super::{KeyLight, Medium, NEUTRAL_DISPLACEMENT, refract_offset};

/// The footprint: a `width × height` rectangle with rounded corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    /// Width in px.
    pub width: f32,
    /// Height in px.
    pub height: f32,
    /// Corner radius as a fraction of half the shorter side, `[0, 1]`; 1 is a circle,
    /// capsule or stadium.
    pub corner: f32,
}

impl Footprint {
    /// A footprint from its shorter side, aspect ratio (`width / height`) and corner fraction.
    #[must_use]
    pub fn from_aspect(short_side: f32, aspect: f32, corner: f32) -> Self {
        let a = aspect.max(1e-3);
        let (width, height) = if a >= 1.0 {
            (short_side * a, short_side)
        } else {
            (short_side, short_side / a)
        };
        Self {
            width,
            height,
            corner,
        }
    }

    /// Corner radius in px.
    #[must_use]
    pub fn corner_px(&self) -> f32 {
        self.corner.clamp(0.0, 1.0) * self.inradius()
    }

    /// Half the shorter side, px: the largest interior distance.
    #[must_use]
    pub fn inradius(&self) -> f32 {
        self.width.min(self.height).max(0.0) / 2.0
    }
}

/// The relief: how the surface rises over the footprint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Relief {
    /// Height `H` of the relief in px.
    pub height: f32,
    /// Face curvature `c`, `[0, 1]`: 0 a flat face behind the shoulder, 1 a dome over the
    /// whole inradius.
    pub face: f32,
    /// Shoulder width `W` in px.
    pub shoulder: f32,
    /// Shoulder profile exponent `p ≥ 1`: 1 chamfer, 2 quarter circle, 4 squircle.
    pub profile: f32,
    /// Flat slab under the relief, px: extra travel of every ray.
    pub base: f32,
}

/// Transmission: direct and two scattered parts, with their kernel widths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scatter {
    /// Weight of the unscattered, refracted image.
    pub direct: f32,
    /// Weight of the finely scattered image.
    pub fine: f32,
    /// Weight of the widely scattered image.
    pub wide: f32,
    /// Fine kernel σ in px.
    pub fine_sigma: f32,
    /// Wide kernel σ in px.
    pub wide_sigma: f32,
}

impl Scatter {
    /// Clear glass: everything direct.
    pub const CLEAR: Self = Self {
        direct: 1.0,
        fine: 0.0,
        wide: 0.0,
        fine_sigma: 0.875,
        wide_sigma: 10.0,
    };

    /// `(direct, fine, wide)` scaled to sum 1. Negative or non-finite weights count as 0;
    /// all-zero weights are all direct.
    #[must_use]
    pub fn normalized(&self) -> [f32; 3] {
        let f = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let w = [f(self.direct), f(self.fine), f(self.wide)];
        let s = w[0] + w[1] + w[2];
        if s <= 0.0 {
            [1.0, 0.0, 0.0]
        } else {
            w.map(|x| x / s)
        }
    }
}

/// The body colour of the material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    /// sRGB colour.
    pub rgb: [u8; 3],
    /// Opacity `[0, 1]`; 0 is colourless.
    pub alpha: f32,
}

impl Tint {
    /// No body colour.
    pub const NONE: Self = Self {
        rgb: [255, 255, 255],
        alpha: 0.0,
    };
}

/// A complete optical surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// Footprint geometry.
    pub footprint: Footprint,
    /// Relief geometry.
    pub relief: Relief,
    /// Material.
    pub medium: Medium,
    /// Dispersion exaggeration: 0 none, 1 physical.
    pub dispersion: f32,
    /// Transmission split.
    pub scatter: Scatter,
    /// Body colour.
    pub tint: Tint,
    /// Key light.
    pub light: KeyLight,
}

/// The named presets, in the visual acceptance order.
pub const PRESETS: [&str; 5] = ["droplet", "capsule", "clear-card", "frosted-card", "aqua"];

/// One evaluated point of the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfacePoint {
    /// Signed distance to the footprint boundary, px (negative inside).
    pub sd: f32,
    /// Outward unit normal of the nearest boundary (screen space, y down).
    pub out: [f32; 2],
    /// Relief height, px.
    pub height: f32,
    /// Relief slope `dh/dd`.
    pub slope: f32,
}

/// Signed distance of `(px, py)` (relative to the centre) to a rectangle of half extents
/// `(hw, hh)` with corner radius `r`, and the outward unit normal of the nearest boundary.
/// Defined inside and outside.
#[must_use]
pub fn footprint_distance(px: f32, py: f32, hw: f32, hh: f32, r: f32) -> (f32, [f32; 2]) {
    let qx = px.abs() - (hw - r);
    let qy = py.abs() - (hh - r);
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    let l = (ox * ox + oy * oy).sqrt();
    let sd = l + qx.max(qy).min(0.0) - r;
    let (nx, ny) = if qx > 0.0 && qy > 0.0 {
        (ox / l.max(f32::MIN_POSITIVE), oy / l.max(f32::MIN_POSITIVE))
    } else if qx > qy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let sx = if px < 0.0 { -1.0 } else { 1.0 };
    let sy = if py < 0.0 { -1.0 } else { 1.0 };
    (sd, [nx * sx, ny * sy])
}

/// Percentile summary of an error population, px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorStats {
    /// Number of samples.
    pub n: usize,
    /// Median.
    pub p50: f32,
    /// 95th percentile.
    pub p95: f32,
    /// 99th percentile.
    pub p99: f32,
    /// Maximum.
    pub max: f32,
}

impl ErrorStats {
    /// Nearest-rank percentiles of `errors` (order is not preserved).
    #[must_use]
    pub fn of(errors: &mut [f32]) -> Self {
        errors.sort_by(f32::total_cmp);
        let n = errors.len();
        let at = |q: f32| {
            if n == 0 {
                0.0
            } else {
                errors[((q * n as f32).ceil() as usize).clamp(1, n) - 1]
            }
        };
        Self {
            n,
            p50: at(0.50),
            p95: at(0.95),
            p99: at(0.99),
            max: at(1.0),
        }
    }
}

/// The SVG path's maps for one surface, RGBA8 over a `width × height` box centred on the
/// footprint (footprint plus one pixel each side).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceMaps {
    /// Box width, px.
    pub width: u32,
    /// Box height, px.
    pub height: u32,
    /// Displacement: direction × green offset, R = x, G = y around 128.
    pub displacement: Vec<u8>,
    /// Specular highlight, white, intensity in alpha.
    pub specular: Vec<u8>,
    /// Fresnel rim, black, reflectance × rim in alpha.
    pub rim: Vec<u8>,
    /// `feDisplacementMap` scale per channel: `2 × max |off_c|`.
    pub channel_scales: [f32; 3],
}

impl Surface {
    /// A preset by name ([`PRESETS`]).
    #[must_use]
    pub fn preset(name: &str) -> Option<Self> {
        let card = Self {
            footprint: Footprint::from_aspect(200.0, 1.6, 0.28),
            relief: Relief {
                height: 16.0,
                face: 0.0,
                shoulder: 17.0,
                profile: 4.0,
                base: 8.0,
            },
            medium: Medium::CROWN_GLASS,
            dispersion: 0.0,
            scatter: Scatter::CLEAR,
            tint: Tint::NONE,
            light: KeyLight {
                angle_deg: -110.0,
                elevation_deg: 40.0,
                shininess: 40.0,
                intensity: 0.35,
                rim: 0.35,
            },
        };
        Some(match name {
            "droplet" => Self {
                footprint: Footprint::from_aspect(180.0, 1.0, 1.0),
                relief: Relief {
                    height: 76.5,
                    face: 1.0,
                    shoulder: 20.0,
                    profile: 2.0,
                    base: 0.0,
                },
                medium: Medium::WATER,
                scatter: Scatter::CLEAR,
                light: KeyLight::DEFAULT,
                ..card
            },
            "capsule" => Self {
                footprint: Footprint::from_aspect(90.0, 3.0, 1.0),
                relief: Relief {
                    height: 30.0,
                    face: 1.0,
                    shoulder: 20.0,
                    profile: 2.0,
                    base: 0.0,
                },
                light: KeyLight::DEFAULT,
                ..card
            },
            "clear-card" => card,
            "frosted-card" => Self {
                scatter: Scatter {
                    direct: 0.15,
                    fine: 0.35,
                    wide: 0.5,
                    fine_sigma: 2.0,
                    wide_sigma: 10.0,
                },
                ..card
            },
            "aqua" => Self {
                relief: Relief {
                    face: 0.08,
                    ..card.relief
                },
                scatter: Scatter {
                    direct: 0.45,
                    fine: 0.2,
                    wide: 0.35,
                    fine_sigma: 0.875,
                    wide_sigma: 10.0,
                },
                tint: Tint {
                    rgb: [214, 236, 246],
                    alpha: 0.35,
                },
                ..card
            },
            _ => return None,
        })
    }

    /// Every length multiplied by `k` (device pixel ratio). Angles, ratios, weights and the
    /// material are unchanged.
    #[must_use]
    pub fn scaled(&self, k: f32) -> Self {
        let k = if k.is_finite() && k > 0.0 { k } else { 1.0 };
        let mut s = *self;
        s.footprint.width *= k;
        s.footprint.height *= k;
        s.relief.height *= k;
        s.relief.shoulder *= k;
        s.relief.base *= k;
        s.scatter.fine_sigma *= k;
        s.scatter.wide_sigma *= k;
        s
    }

    /// Parse a query string: `preset=<name>` first (default `droplet`), then overrides:
    /// `size` (shorter side, px), `aspect`, `corner`, `height`, `face`, `shoulder`, `profile`,
    /// `base`, `medium` (`water|glass|diamond`), `ior`, `abbe`, `disp`, `direct`, `fine`,
    /// `wide`, `fsig`, `wsig`, `tint` (`rrggbb`), `tinta`, `light`, `elev`, `shine`, `spec`,
    /// `rim`. Unknown keys and unparsable or non-finite values are ignored; values are
    /// clamped to safe ranges. The one parameter contract of the server maps and the GPU.
    #[must_use]
    pub fn from_query(raw: &str) -> Self {
        let pairs: Vec<(&str, &str)> = raw
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|p| p.split_once('=').unwrap_or((p, "")))
            .collect();
        let preset = pairs
            .iter()
            .find(|(k, _)| *k == "preset")
            .and_then(|(_, v)| Self::preset(v));
        let mut s = preset.unwrap_or_else(|| Self::preset("droplet").expect("droplet preset"));
        let short = s.footprint.width.min(s.footprint.height);
        let aspect = s.footprint.width / s.footprint.height.max(1e-3);
        let (mut size, mut asp) = (short, aspect);
        for (k, v) in pairs {
            let n = v.parse::<f32>().ok().filter(|x| x.is_finite());
            let set = |t: &mut f32, lo: f32, hi: f32| {
                if let Some(x) = n {
                    *t = x.clamp(lo, hi);
                }
            };
            match k {
                "size" => set(&mut size, 8.0, 1200.0),
                "aspect" => set(&mut asp, 0.1, 10.0),
                "corner" => set(&mut s.footprint.corner, 0.0, 1.0),
                "height" => set(&mut s.relief.height, 0.0, 600.0),
                "face" => set(&mut s.relief.face, 0.0, 1.0),
                "shoulder" => set(&mut s.relief.shoulder, 0.0, 600.0),
                "profile" => set(&mut s.relief.profile, 1.0, 8.0),
                "base" => set(&mut s.relief.base, 0.0, 200.0),
                "medium" => {
                    s.medium = match v {
                        "glass" => Medium::CROWN_GLASS,
                        "diamond" => Medium::DIAMOND,
                        _ => Medium::WATER,
                    }
                }
                "ior" => set(&mut s.medium.ior, 1.0, 3.0),
                "abbe" => {
                    if let Some(x) = n {
                        s.medium.abbe = Some(x.clamp(5.0, 200.0));
                    }
                }
                "disp" => set(&mut s.dispersion, 0.0, 40.0),
                "direct" => set(&mut s.scatter.direct, 0.0, 1.0),
                "fine" => set(&mut s.scatter.fine, 0.0, 1.0),
                "wide" => set(&mut s.scatter.wide, 0.0, 1.0),
                "fsig" => set(&mut s.scatter.fine_sigma, 0.0, 40.0),
                "wsig" => set(&mut s.scatter.wide_sigma, 0.0, 80.0),
                "tint" => {
                    let hex = v.trim_start_matches('#').trim_start_matches("%23");
                    if hex.len() == 6
                        && let Ok(x) = u32::from_str_radix(hex, 16)
                    {
                        s.tint.rgb = [(x >> 16) as u8, (x >> 8) as u8, x as u8];
                    }
                }
                "tinta" => set(&mut s.tint.alpha, 0.0, 1.0),
                "light" => set(&mut s.light.angle_deg, -360.0, 360.0),
                "elev" => set(&mut s.light.elevation_deg, 1.0, 90.0),
                "shine" => set(&mut s.light.shininess, 1.0, 1000.0),
                "spec" => set(&mut s.light.intensity, 0.0, 1.0),
                "rim" => set(&mut s.light.rim, 0.0, 1.0),
                _ => {}
            }
        }
        s.footprint = Footprint {
            corner: s.footprint.corner,
            ..Footprint::from_aspect(size, asp, 0.0)
        };
        s
    }

    /// The full parameter set as a query string, every key of [`Self::from_query`] (medium
    /// as `ior` + `abbe`). `from_query(to_query(s))` reproduces `s` to the printed precision.
    #[must_use]
    pub fn to_query(&self) -> String {
        let fp = self.footprint;
        let short = fp.width.min(fp.height);
        let aspect = fp.width / fp.height.max(1e-3);
        let (rl, sc, l) = (self.relief, self.scatter, self.light);
        let [r, g, b] = self.tint.rgb;
        let mut q = format!(
            "size={short}&aspect={aspect}&corner={}&height={}&face={}&shoulder={}&profile={}&base={}&ior={}",
            fp.corner, rl.height, rl.face, rl.shoulder, rl.profile, rl.base, self.medium.ior
        );
        if let Some(v) = self.medium.abbe {
            q.push_str(&format!("&abbe={v}"));
        }
        q.push_str(&format!(
            "&disp={}&direct={}&fine={}&wide={}&fsig={}&wsig={}&tint={r:02x}{g:02x}{b:02x}&tinta={}&light={}&elev={}&shine={}&spec={}&rim={}",
            self.dispersion, sc.direct, sc.fine, sc.wide, sc.fine_sigma, sc.wide_sigma,
            self.tint.alpha, l.angle_deg, l.elevation_deg, l.shininess, l.intensity, l.rim
        ));
        q
    }

    /// Normal-incidence reflectance of the green index.
    #[must_use]
    pub fn f0(&self) -> f32 {
        let g = self.medium.channel_iors(self.dispersion)[1];
        ((g - 1.0) / (g + 1.0)).powi(2)
    }

    /// Evaluate the surface at `p` relative to the footprint centre (spec steps 1–2).
    #[must_use]
    pub fn point(&self, p: [f32; 2]) -> SurfacePoint {
        let fp = &self.footprint;
        let (hw, hh) = (fp.width.max(0.0) / 2.0, fp.height.max(0.0) / 2.0);
        let (sd, out) = footprint_distance(p[0], p[1], hw, hh, fp.corner_px());
        let (height, slope) = self.relief_at((-sd).max(0.0));
        SurfacePoint {
            sd,
            out,
            height,
            slope,
        }
    }

    /// Relief height and slope at interior distance `d` (spec step 2).
    #[must_use]
    pub fn relief_at(&self, d: f32) -> (f32, f32) {
        let rl = &self.relief;
        let big_h = rl.height.max(0.0);
        let c = rl.face.clamp(0.0, 1.0);
        let p = rl.profile.clamp(1.0, 8.0);
        // shoulder
        let (s, ds) = if rl.shoulder > 0.0 {
            let t = (d / rl.shoulder).clamp(1e-4, 1.0);
            if t >= 1.0 {
                (big_h, 0.0)
            } else {
                let u = 1.0 - (1.0 - t).powf(p);
                let sv = u.powf(1.0 / p);
                let sd = u.powf(1.0 / p - 1.0) * (1.0 - t).powf(p - 1.0);
                (big_h * sv, big_h * sd / rl.shoulder)
            }
        } else {
            (big_h, 0.0)
        };
        // dome over the inradius: the droplet's cap
        let rin = self.footprint.inradius().max(1e-3);
        let hd = big_h.clamp(1e-3, rin);
        let rs = (rin * rin + hd * hd) / (2.0 * hd);
        let r = (rin - d).clamp(0.0, rin);
        let root = (rs * rs - r * r).max(1e-6).sqrt();
        let (f, df) = (root - (rs - hd), r / root);
        ((1.0 - c) * s + c * f, (1.0 - c) * ds + c * df)
    }

    /// Edge coverage at `p`: `clamp(0.5 − sd, 0, 1)`.
    #[must_use]
    pub fn coverage(&self, p: [f32; 2]) -> f32 {
        (0.5 - self.point(p).sd).clamp(0.0, 1.0)
    }

    /// Per-channel offset (px, along the outward normal) at `p` (spec step 3).
    #[must_use]
    pub fn offsets(&self, p: [f32; 2]) -> [f32; 3] {
        let sp = self.point(p);
        let travel = sp.height + self.relief.base.max(0.0);
        self.medium
            .channel_iors(self.dispersion)
            .map(|n| refract_offset(sp.slope, travel, n))
    }

    /// Where the page is sampled for the pixel at `p` (relative to the centre), per channel:
    /// the reference of the GPU parity check. Outside the footprint, `p` itself.
    #[must_use]
    pub fn sample_points(&self, p: [f32; 2]) -> [[f32; 2]; 3] {
        let sp = self.point(p);
        if sp.sd >= 0.5 {
            return [p; 3];
        }
        let off = self.offsets(p);
        off.map(|o| [p[0] + sp.out[0] * o, p[1] + sp.out[1] * o])
    }

    /// The box the maps are rendered over: footprint plus one pixel each side, px.
    #[must_use]
    pub fn map_box(&self) -> (u32, u32) {
        let f = |v: f32| (v.max(1.0)).ceil() as u32 + 2;
        (f(self.footprint.width), f(self.footprint.height))
    }

    /// The SVG path's maps (spec steps 3 and 5; scattering and tint are filter / CSS terms).
    #[must_use]
    pub fn render_maps(&self) -> SurfaceMaps {
        let (bw, bh) = self.map_box();
        let (w, h) = (bw as usize, bh as usize);
        let mut offs = vec![[0.0f32; 3]; w * h];
        let mut pts = vec![None; w * h];
        let mut maxes = [0.0f32; 3];
        for y in 0..h {
            for x in 0..w {
                let p = self.box_point(x, y);
                let sp = self.point(p);
                if sp.sd >= 0.0 {
                    continue;
                }
                let o = self.offsets(p);
                for c in 0..3 {
                    maxes[c] = maxes[c].max(o[c].abs());
                }
                offs[y * w + x] = o;
                pts[y * w + x] = Some(sp);
            }
        }
        let mut displacement = NEUTRAL_DISPLACEMENT.repeat(w * h);
        let mut specular = vec![0u8; w * h * 4];
        let mut rim = vec![0u8; w * h * 4];
        let f0 = self.f0();
        let hv = half_vector(&self.light);
        for (k, sp) in pts.iter().enumerate() {
            let Some(sp) = sp else { continue };
            let i = k * 4;
            let m = if maxes[1] > 0.0 {
                offs[k][1] / maxes[1]
            } else {
                0.0
            };
            displacement[i] = (128.0 + 127.0 * m * sp.out[0]).round() as u8;
            displacement[i + 1] = (128.0 + 127.0 * m * sp.out[1]).round() as u8;
            let (fres, spec) = shade(sp, f0, hv, &self.light);
            specular[i..i + 4].copy_from_slice(&[255, 255, 255, (spec * 255.0).round() as u8]);
            rim[i + 3] = ((fres * self.light.rim).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        SurfaceMaps {
            width: bw,
            height: bh,
            displacement,
            specular,
            rim,
            channel_scales: maxes.map(|m| 2.0 * m),
        }
    }

    /// The error the SVG path's 8-bit, single-map encoding makes against the reference:
    /// per channel, the distance between where `feDisplacementMap` samples
    /// (`p + scale_c · (byte/255 − 0.5)`) and [`Self::sample_points`], over the footprint.
    #[must_use]
    pub fn svg_map_error(&self, maps: &SurfaceMaps) -> [ErrorStats; 3] {
        let (w, h) = (maps.width as usize, maps.height as usize);
        let mut errs: [Vec<f32>; 3] = Default::default();
        for y in 0..h {
            for x in 0..w {
                let p = self.box_point(x, y);
                if self.point(p).sd >= 0.0 {
                    continue;
                }
                let want = self.sample_points(p);
                let i = (y * w + x) * 4;
                let (bx, by) = (
                    f32::from(maps.displacement[i]),
                    f32::from(maps.displacement[i + 1]),
                );
                for c in 0..3 {
                    let s = maps.channel_scales[c];
                    let got = [p[0] + s * (bx / 255.0 - 0.5), p[1] + s * (by / 255.0 - 0.5)];
                    errs[c].push((got[0] - want[c][0]).hypot(got[1] - want[c][1]));
                }
            }
        }
        errs.map(|mut e| ErrorStats::of(&mut e))
    }

    fn box_point(&self, x: usize, y: usize) -> [f32; 2] {
        let (bw, bh) = self.map_box();
        [
            x as f32 + 0.5 - bw as f32 / 2.0,
            y as f32 + 0.5 - bh as f32 / 2.0,
        ]
    }
}

/// Blinn-Phong half vector of the key light and the view (0, 0, 1).
#[must_use]
pub fn half_vector(light: &KeyLight) -> [f32; 3] {
    let (az, el) = (
        light.angle_deg.to_radians(),
        light.elevation_deg.to_radians(),
    );
    let v = [az.cos() * el.cos(), az.sin() * el.cos(), el.sin() + 1.0];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    v.map(|x| x / len)
}

/// Fresnel reflectance and highlight intensity at a surface point (spec step 5).
#[must_use]
pub fn shade(sp: &SurfacePoint, f0: f32, hv: [f32; 3], light: &KeyLight) -> (f32, f32) {
    let nl = (sp.slope * sp.slope + 1.0).sqrt();
    let n = [
        sp.out[0] * sp.slope / nl,
        sp.out[1] * sp.slope / nl,
        1.0 / nl,
    ];
    let fres = f0 + (1.0 - f0) * (1.0 - n[2]).powi(5);
    let nh = (n[0] * hv[0] + n[1] * hv[1] + n[2] * hv[2]).max(0.0);
    let spec =
        (nh.powf(light.shininess) * (0.25 + 0.75 * fres / f0.max(1e-3)).min(4.0) * light.intensity)
            .clamp(0.0, 1.0);
    (fres, spec)
}

#[cfg(test)]
mod tests {
    use super::super::Droplet;
    use super::*;

    fn droplet() -> Surface {
        Surface::preset("droplet").unwrap()
    }

    /// The droplet preset is the droplet: same height, slope and offsets as `Droplet`.
    #[test]
    fn the_droplet_preset_is_the_droplet() {
        let s = droplet();
        let d = Droplet {
            radius: 90.0,
            height: 76.5,
        };
        for r in [0.0f32, 10.0, 45.5, 80.0, 89.0] {
            let sp = s.point([r, 0.0]);
            let (h, slope) = d.surface(r).unwrap();
            assert!(
                (sp.height - h).abs() < 1e-3,
                "h at {r}: {} vs {h}",
                sp.height
            );
            assert!(
                (sp.slope - slope).abs() < 1e-3 * slope.max(1.0),
                "slope at {r}"
            );
        }
        // diagonal: the footprint is a circle, so only the radius matters
        let a = s.point([30.0, 40.0]);
        let b = s.point([50.0, 0.0]);
        assert!((a.height - b.height).abs() < 1e-3 && (a.sd - b.sd).abs() < 1e-3);
        assert!((a.out[0] - 0.6).abs() < 1e-5 && (a.out[1] - 0.8).abs() < 1e-5);
    }

    /// Footprint and relief are independent: the corner changes the outline, never the
    /// relief law; face curvature changes the relief, never the outline.
    #[test]
    fn footprint_and_relief_are_independent() {
        let card = Surface::preset("clear-card").unwrap();
        let mut round = card;
        round.footprint.corner = 1.0;
        let p = [0.0, 40.0]; // on the axis, well inside: same interior distance
        assert_eq!(card.point(p).sd, round.point(p).sd);
        assert_eq!(card.point(p).height, round.point(p).height);
        let mut sharp = card;
        sharp.footprint.corner = 0.0;
        let corner = [
            card.footprint.width / 2.0 - 2.0,
            card.footprint.height / 2.0 - 2.0,
        ];
        assert!(sharp.point(corner).sd < 0.0 && round.point(corner).sd > 0.0);
        assert_eq!(sharp.point(p).height, round.point(p).height);
        let mut domed = card;
        domed.relief.face = 1.0;
        assert_eq!(card.point(p).sd, domed.point(p).sd);
        assert_ne!(card.point(p).height, domed.point(p).height);
    }

    /// A flat face does not refract; a curved face does. Both share the shoulder law.
    #[test]
    fn face_curvature_decides_whether_the_plateau_refracts() {
        let card = Surface::preset("clear-card").unwrap();
        let mid = [0.0, 0.0];
        assert_eq!(card.offsets(mid), [0.0; 3]);
        let shoulder = [card.footprint.width / 2.0 - 5.0, 0.0];
        assert!(
            card.offsets(shoulder)[1] < -0.1,
            "{:?}",
            card.offsets(shoulder)
        );
        let mut domed = card;
        domed.relief.face = 0.5;
        // off the ridge (the medial axis is the dome's apex line, slope 0 there)
        let near = [0.0, 30.0];
        assert!(domed.offsets(near)[1] < -0.01 && card.offsets(near)[1] == 0.0);
    }

    /// The shoulder profile changes the slope law (and so the refraction), not the outline.
    #[test]
    fn the_shoulder_profile_changes_the_slope() {
        let card = Surface::preset("clear-card").unwrap();
        let mut chamfer = card;
        chamfer.relief.profile = 1.0;
        let (_, s4) = card.relief_at(8.5);
        let (_, s1) = chamfer.relief_at(8.5);
        // chamfer: constant slope H/W; squircle: flatter in the middle of the shoulder
        assert!((s1 - 16.0 / 17.0).abs() < 1e-4, "{s1}");
        assert!(s4 < s1, "{s4} vs {s1}");
    }

    /// Capsule: a stadium; the distance along the long axis's flat side is the half height
    /// minus the offset, and the rounded ends are circles.
    #[test]
    fn the_capsule_footprint_is_a_stadium() {
        let c = Surface::preset("capsule").unwrap();
        assert_eq!((c.footprint.width, c.footprint.height), (270.0, 90.0));
        assert!((c.point([0.0, 30.0]).sd + 15.0).abs() < 1e-4);
        let end = [135.0 - 45.0 + 45.0 * 0.6, 45.0 * 0.8];
        assert!(c.point(end).sd.abs() < 1e-3);
    }

    #[test]
    fn scatter_weights_normalize() {
        let s = Scatter {
            direct: 2.0,
            fine: 1.0,
            wide: 1.0,
            ..Scatter::CLEAR
        };
        assert_eq!(s.normalized(), [0.5, 0.25, 0.25]);
        let z = Scatter {
            direct: 0.0,
            fine: f32::NAN,
            wide: -1.0,
            ..Scatter::CLEAR
        };
        assert_eq!(z.normalized(), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_query_is_the_one_parameter_contract() {
        let s = Surface::from_query(
            "preset=clear-card&size=100&aspect=2&corner=1&face=0.3&medium=diamond&disp=2&tint=%23ff8000&tinta=0.5&bogus=1&height=NaN",
        );
        assert_eq!((s.footprint.width, s.footprint.height), (200.0, 100.0));
        assert_eq!((s.footprint.corner, s.relief.face), (1.0, 0.3));
        assert_eq!(s.relief.height, 16.0, "NaN ignored");
        assert_eq!((s.medium, s.dispersion), (Medium::DIAMOND, 2.0));
        assert_eq!((s.tint.rgb, s.tint.alpha), ([255, 128, 0], 0.5));
        assert_eq!(Surface::from_query(""), droplet());
        assert_eq!(
            Surface::from_query("preset=aqua"),
            Surface::preset("aqua").unwrap()
        );
        for p in PRESETS {
            let q = Surface::from_query(&format!("preset={p}"));
            assert_eq!(Some(q), Surface::preset(p), "{p}");
        }
    }

    #[test]
    fn to_query_round_trips_every_preset() {
        for p in PRESETS {
            let s = Surface::preset(p).unwrap();
            let back = Surface::from_query(&s.to_query());
            assert_eq!(back, s, "{p}: {}", s.to_query());
        }
        // and a surface off every preset
        let mut s = Surface::preset("aqua").unwrap();
        s.footprint = Footprint::from_aspect(77.0, 0.5, 0.6);
        s.relief.profile = 3.0;
        s.dispersion = 7.5;
        s.medium.abbe = None;
        let back = Surface::from_query(&s.to_query());
        assert_eq!(back.footprint, s.footprint);
        assert_eq!((back.relief, back.dispersion), (s.relief, s.dispersion));
        assert_eq!(
            back.medium.abbe,
            s.medium.abbe.or(Medium::WATER.abbe),
            "no abbe key keeps the preset's"
        );
    }

    #[test]
    fn scaling_multiplies_lengths_only() {
        let s = Surface::preset("aqua").unwrap();
        let k = s.scaled(2.0);
        assert_eq!(k.footprint.width, 2.0 * s.footprint.width);
        assert_eq!(k.relief.height, 2.0 * s.relief.height);
        assert_eq!(k.scatter.wide_sigma, 2.0 * s.scatter.wide_sigma);
        assert_eq!(
            (k.relief.face, k.footprint.corner, k.medium),
            (s.relief.face, s.footprint.corner, s.medium)
        );
        // the relief law is scale-invariant: offsets scale with the lengths
        let (a, b) = (s.offsets([100.0, 0.0])[1], k.offsets([200.0, 0.0])[1]);
        assert!((b - 2.0 * a).abs() < 1e-3 * a.abs().max(1.0), "{a} {b}");
    }

    /// The SVG map encodes the green channel within half a pixel's quantisation, and the
    /// shared-map approximation of red and blue stays within a few pixels; a broken scale is
    /// seen.
    #[test]
    fn the_svg_map_error_is_measured() {
        let mut s = Surface::preset("clear-card").unwrap();
        s.dispersion = 12.0;
        let maps = s.render_maps();
        let e = s.svg_map_error(&maps);
        assert!(e[1].n > 1000);
        assert!(e[1].max < 0.5, "{:?}", e[1]);
        assert!(e[0].p99 < 2.0 && e[2].p99 < 2.0, "{:?}", e);
        let mut broken = maps.clone();
        broken.channel_scales[1] *= 1.5;
        assert!(
            s.svg_map_error(&broken)[1].max > 1.0,
            "a wrong scale must show"
        );
    }

    #[test]
    fn coverage_ramps_over_one_pixel() {
        let s = Surface::preset("clear-card").unwrap();
        let edge = s.footprint.width / 2.0;
        assert_eq!(s.coverage([0.0, 0.0]), 1.0);
        assert!((s.coverage([edge, 0.0]) - 0.5).abs() < 1e-5);
        assert_eq!(s.coverage([edge + 1.0, 0.0]), 0.0);
    }

    #[test]
    fn percentiles_are_nearest_rank() {
        let mut v: Vec<f32> = (1..=100).map(|x| x as f32).collect();
        let e = ErrorStats::of(&mut v);
        assert_eq!(
            (e.n, e.p50, e.p95, e.p99, e.max),
            (100, 50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!(ErrorStats::of(&mut []).max, 0.0);
    }
}
