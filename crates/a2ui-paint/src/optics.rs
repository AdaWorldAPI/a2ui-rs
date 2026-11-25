//! The optical engine of the glass material: geometry in, light-interaction maps out.
//!
//! One material, computed from a model of glass rather than imitated with gradients
//! (plan `.claude/plans/a2ui-material-appearance-v1.md` §6, ported from the report-rs
//! #209 optics prototype):
//!
//! 1. signed distance `d` to the rounded-rectangle boundary, outward normal from its
//!    gradient ([`rounded_rect`]);
//! 2. height over the bevel, `x` = distance from the edge / bevel width, by a convex
//!    profile ([`Profile`]); flat inside the bevel;
//! 3. surface normal from the derivative of the height along the outward direction;
//! 4. a ray perpendicular to the page refracts at the curved face (Snell, n1 = 1,
//!    n2 = IOR) and travels to the back face; its lateral travel is the displacement.
//!    Rays at the same distance from the edge share one magnitude, so one cross-section
//!    of [`CROSS_SECTION_SAMPLES`] samples is computed and reused around the shape
//!    ([`CrossSection`]);
//! 5. the displacement is encoded in R/G around 128, normalized by its maximum; a
//!    consumer applies it with `scale = 2 × max` ([`FilterParams::displacement_scale`]);
//! 6. scattering after refraction: a fine and a wide Gaussian mixed by the fine share;
//! 7. a specular rim from the normal tilt and the key-light facing, weighted by
//!    Schlick Fresnel `F0 = ((n − 1)/(n + 1))²`;
//! 8. rear illumination as a separate field behind the surface, fading to transparent
//!    inside its own box: no mask, no clip, no silhouette ([`rear_field`]).
//!
//! **Colour is not a property of the glass.** [`Glass`] has no hue: the neutral
//! highlights and the refraction are the same under every colour preset, and only
//! [`RearLight`] carries a colour (plan gate G2).
//!
//! **Effect strength 0 is plain glass.** Every term scales with [`Glass::strength`];
//! at 0 the displacement map is neutral, the filter is the identity, and the specular
//! and rear fields are fully transparent (plan gate G3).
//!
//! Parameter provenance (third-party measurements, not verified by us): refraction band
//! 0.6 × corner radius, scattering σ 0.875 / 10 with fine share 0.336, regular tint grey
//! 241 at 56.5 %, rim exponent 6 — measured by shayann07/liquidglass (Apache-2.0); IOR
//! 1.5, light direction ≈ −110°, clear depth 16 — authored there. No third-party code
//! is copied; the physics is standard (signed distance, Snell, Schlick).
//!
//! Pure arithmetic, no dependencies. The `optics-png` feature adds PNG encoding of the
//! maps for consumers that serve them as image assets.

/// Samples in the one cross-section reused around the shape.
pub const CROSS_SECTION_SAMPLES: usize = 128;

/// The height profile over the bevel, `x ∈ [0, 1]` from the edge inwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Quarter circle, `√(1 − (1 − x)²)`.
    Circle,
    /// Convex squircle, `(1 − (1 − x)⁴)^¼` — flatter top, steeper rim.
    Squircle,
}

impl Profile {
    /// Height at normalized bevel position `x` (clamped to `[0, 1]`).
    #[must_use]
    pub fn height(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Circle => (1.0 - (1.0 - x).powi(2)).max(0.0).sqrt(),
            Self::Squircle => (1.0 - (1.0 - x).powi(4)).max(0.0).powf(0.25),
        }
    }

    /// Slope `dh/dx` by central difference. At the ends the difference is taken over the
    /// half-width step inside `[0, 1]` and still divided by the full width, so it is half the
    /// true slope there (the prototype's convention, kept for parity).
    #[must_use]
    pub fn slope(self, x: f32) -> f32 {
        let dx = 1e-3;
        (self.height((x + dx).min(1.0)) - self.height((x - dx).max(0.0))) / (2.0 * dx)
    }

    /// Parse `"circle"` / `"squircle"`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "circle" => Some(Self::Circle),
            "squircle" => Some(Self::Squircle),
            _ => None,
        }
    }
}

/// The glass: shape, optical constants and the effect strength. No colour (see module doc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glass {
    /// Corner radius in px (clamped to half the smaller side when rendering).
    pub radius: f32,
    /// Width of the curved edge zone in px.
    pub bezel: f32,
    /// Height of the curved face over the bevel in px.
    pub thickness: f32,
    /// Extra travel from the face to the back face in px.
    pub depth: f32,
    /// Refractive index.
    pub ior: f32,
    /// Height profile over the bevel.
    pub profile: Profile,
    /// Multiplier on the physical displacement (1 = as computed).
    pub refraction: f32,
    /// Fine scattering σ in px.
    pub fine_sigma: f32,
    /// Wide scattering σ in px.
    pub wide_sigma: f32,
    /// Share of the fine kernel in the mix, `[0, 1]`.
    pub fine_share: f32,
    /// Key-light angle in degrees, screen space (y down); −110° ≈ (−0.35, −0.94).
    pub light_angle_deg: f32,
    /// Phong-style exponent of the rim's key-light facing.
    pub spec_exp: f32,
    /// Peak opacity of the specular rim, `[0, 1]`.
    pub spec_opacity: f32,
    /// Opacity of the glass body's neutral tint (grey 241), `[0, 1]`. Scaled by strength
    /// like every other term, so strength 0 shows no tint.
    pub tint: f32,
    /// Effect strength, `[0, 1]`. Scales every optical term; 0 is plain glass.
    pub strength: f32,
}

impl Glass {
    /// The prototype's defaults (report-rs `/view/material-lab/optics`).
    pub const DEFAULT: Self = Self {
        radius: 28.0,
        bezel: 17.0,
        thickness: 16.0,
        depth: 8.0,
        ior: 1.5,
        profile: Profile::Squircle,
        refraction: 1.0,
        fine_sigma: 0.875,
        wide_sigma: 10.0,
        fine_share: 0.336,
        light_angle_deg: -110.0,
        spec_exp: 6.0,
        spec_opacity: 0.35,
        tint: 0.565,
        strength: 1.0,
    };

    /// Normal-incidence reflectance, `((n − 1)/(n + 1))²` (≈ 0.04 at n = 1.5).
    #[must_use]
    pub fn f0(&self) -> f32 {
        ((self.ior - 1.0) / (self.ior + 1.0)).powi(2)
    }

    /// This glass with every non-finite value replaced by the default's, so a NaN or an
    /// infinity from a caller can never reach the maps (a NaN cast to `u8` is 0, which would
    /// read as maximum displacement).
    #[must_use]
    pub fn sanitized(self) -> Self {
        let d = Self::DEFAULT;
        let f = |v: f32, dv: f32| if v.is_finite() { v } else { dv };
        Self {
            radius: f(self.radius, d.radius),
            bezel: f(self.bezel, d.bezel),
            thickness: f(self.thickness, d.thickness),
            depth: f(self.depth, d.depth),
            ior: f(self.ior, d.ior),
            profile: self.profile,
            refraction: f(self.refraction, d.refraction),
            fine_sigma: f(self.fine_sigma, d.fine_sigma),
            wide_sigma: f(self.wide_sigma, d.wide_sigma),
            fine_share: f(self.fine_share, d.fine_share),
            light_angle_deg: f(self.light_angle_deg, d.light_angle_deg),
            spec_exp: f(self.spec_exp, d.spec_exp),
            spec_opacity: f(self.spec_opacity, d.spec_opacity),
            tint: f(self.tint, d.tint),
            strength: f(self.strength, 0.0),
        }
    }

    /// Lateral displacement (px) of a perpendicular ray entering at normalized bevel
    /// position `x`, along the outward normal. Negative on a convex bevel: the ray bends
    /// inwards, so the edge shows background from further inside, as a lens does. Zero on
    /// the flat face and on total internal reflection.
    #[must_use]
    pub fn displacement(&self, x: f32) -> f32 {
        self.sanitized().displacement_inner(x)
    }

    /// The one cross-section, reused around the shape.
    #[must_use]
    pub fn cross_section(&self) -> CrossSection {
        self.sanitized().cross_section_inner()
    }

    /// The filter parameters for this glass at its strength, given the cross-section's
    /// maximum. At strength 0 every value is the identity.
    #[must_use]
    pub fn filter_params(&self, max_displacement: f32) -> FilterParams {
        let max = if max_displacement.is_finite() {
            max_displacement
        } else {
            0.0
        };
        self.sanitized().filter_params_inner(max)
    }

    /// The displacement map alone. It depends on the shape, the profile, the thickness, the
    /// depth and the index only: not on colour, light direction, rim or scattering, and on
    /// strength only in that strength 0 gives the neutral map (the filter scale carries the
    /// strength otherwise). A renderer can therefore keep it cached across those changes.
    #[must_use]
    pub fn render_displacement(&self, width: u32, height: u32, cs: &CrossSection) -> Vec<u8> {
        self.sanitized()
            .render_displacement_inner(width, height, cs)
    }

    /// The specular rim alone: white, intensity in alpha. Depends on the shape, the profile,
    /// the index, the light direction, the rim exponent and opacity and the strength; not on
    /// colour.
    #[must_use]
    pub fn render_specular(&self, width: u32, height: u32) -> Vec<u8> {
        self.sanitized().render_specular_inner(width, height)
    }

    fn strength(&self) -> f32 {
        self.strength.clamp(0.0, 1.0)
    }

    /// The body of [`Self::displacement`]; expects a sanitized glass.
    #[must_use]
    fn displacement_inner(&self, x: f32) -> f32 {
        let bezel = self.bezel.max(1e-3);
        let h = self.profile.height(x);
        // slope of the surface along the outward direction, in px/px
        let d = self.profile.slope(x) * (self.thickness / bezel);
        // surface normal in the (outward, z) plane
        let nl = (d * d + 1.0).sqrt();
        let (nx, nz) = (d / nl, 1.0 / nl);
        // incident ray (0, −1); Snell in vector form, eta = 1 / IOR, cos i = nz
        let eta = 1.0 / self.ior.max(1e-3);
        let cosi = nz;
        let k = 1.0 - eta * eta * (1.0 - cosi * cosi);
        if k < 0.0 {
            return 0.0;
        }
        let m = eta * cosi - k.sqrt();
        let tx = m * nx;
        let tz = -eta + m * nz;
        let travel = h * self.thickness + self.depth;
        tx / tz.abs() * travel
    }

    /// The body of [`Self::cross_section`]; expects a sanitized glass.
    #[must_use]
    fn cross_section_inner(&self) -> CrossSection {
        let mut samples = [0.0f32; CROSS_SECTION_SAMPLES + 1];
        let mut max = 0.0f32;
        for (i, s) in samples.iter_mut().enumerate() {
            *s = self.displacement_inner(i as f32 / CROSS_SECTION_SAMPLES as f32);
            max = max.max(s.abs());
        }
        CrossSection { samples, max }
    }

    /// The body of [`Self::filter_params`]; expects a sanitized glass.
    #[must_use]
    fn filter_params_inner(&self, max_displacement: f32) -> FilterParams {
        let s = self.strength();
        FilterParams {
            displacement_scale: 2.0 * max_displacement * self.refraction.max(0.0) * s,
            fine_sigma: self.fine_sigma.max(0.0) * s,
            wide_sigma: self.wide_sigma.max(0.0) * s,
            fine_share: self.fine_share.clamp(0.0, 1.0),
            tint_alpha: self.tint.clamp(0.0, 1.0) * s,
        }
    }

    /// Displacement and specular maps for a `width × height` element.
    #[must_use]
    pub fn render(&self, width: u32, height: u32) -> GlassMaps {
        let g = self.sanitized();
        let cs = g.cross_section_inner();
        GlassMaps {
            width,
            height,
            displacement: g.render_displacement_inner(width, height, &cs),
            specular: g.render_specular_inner(width, height),
            max_displacement: cs.max,
        }
    }

    /// The body of [`Self::render_displacement`]; expects a sanitized glass.
    #[must_use]
    fn render_displacement_inner(&self, width: u32, height: u32, cs: &CrossSection) -> Vec<u8> {
        let mut map = NEUTRAL_DISPLACEMENT.repeat(width as usize * height as usize);
        if self.strength() == 0.0 {
            return map;
        }
        self.for_each_bevel_pixel(width, height, |i, e, xb| {
            let idx = (xb * CROSS_SECTION_SAMPLES as f32).round() as usize;
            let m = if cs.max > 0.0 {
                cs.samples[idx] / cs.max
            } else {
                0.0
            };
            map[i] = (128.0 + 127.0 * m * e.nx).round() as u8;
            map[i + 1] = (128.0 + 127.0 * m * e.ny).round() as u8;
        });
        map
    }

    /// The body of [`Self::render_specular`]; expects a sanitized glass.
    #[must_use]
    fn render_specular_inner(&self, width: u32, height: u32) -> Vec<u8> {
        let mut map = vec![0u8; width as usize * height as usize * 4];
        let s = self.strength();
        if s == 0.0 {
            return map;
        }
        let la = self.light_angle_deg.to_radians();
        let (lx, ly) = (la.cos(), la.sin());
        let f0 = self.f0();
        let norm = f0 + (1.0 - f0) * 0.03; // authored normalisation of the rim weight
        let slope_scale = self.thickness / self.bezel.max(1e-3);
        self.for_each_bevel_pixel(width, height, |i, e, xb| {
            let tilt = (self.profile.slope(xb) * slope_scale).atan();
            let facing = (e.nx * lx + e.ny * ly).max(0.0);
            let fres = f0 + (1.0 - f0) * (1.0 - tilt.cos()).powi(5);
            let rim = facing.powf(self.spec_exp) * tilt.sin() * (0.35 + 0.65 * fres / norm);
            let a = (rim * self.spec_opacity * s).clamp(0.0, 1.0);
            map[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
        });
        map
    }

    /// Visit every pixel inside the bevel band: `f(byte index, edge sample, bevel position)`.
    fn for_each_bevel_pixel(
        &self,
        width: u32,
        height: u32,
        mut f: impl FnMut(usize, EdgeSample, f32),
    ) {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0 {
            return;
        }
        let (wf, hf) = (width as f32, height as f32);
        let r = self.radius.min(wf / 2.0).min(hf / 2.0).max(0.0);
        let bezel = self.bezel.max(1e-3);
        for y in 0..h {
            for x in 0..w {
                let cx = x as f32 + 0.5 - wf / 2.0;
                let cy = y as f32 + 0.5 - hf / 2.0;
                let Some(e) = rounded_rect(cx, cy, wf, hf, r) else {
                    continue;
                };
                let xb = e.depth / bezel;
                if xb < 1.0 {
                    f((y * w + x) * 4, e, xb);
                }
            }
        }
    }
}

impl Default for Glass {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The R/G/B/A value of "no displacement" (`feDisplacementMap` reads 128 as zero).
pub const NEUTRAL_DISPLACEMENT: [u8; 4] = [128, 128, 128, 255];

/// One cross-section of the displacement over the bevel, `x = i / CROSS_SECTION_SAMPLES`.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossSection {
    /// Displacement in px per sample.
    pub samples: [f32; CROSS_SECTION_SAMPLES + 1],
    /// Largest absolute displacement in px.
    pub max: f32,
}

/// What a browser filter needs besides the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterParams {
    /// `feDisplacementMap` scale (`2 × max × refraction × strength`).
    pub displacement_scale: f32,
    /// Fine Gaussian σ.
    pub fine_sigma: f32,
    /// Wide Gaussian σ.
    pub wide_sigma: f32,
    /// Weight of the fine kernel; the wide kernel gets `1 − fine_share`.
    pub fine_share: f32,
    /// Opacity of the neutral body tint at this strength.
    pub tint_alpha: f32,
}

/// The rendered maps of one element, RGBA8, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassMaps {
    /// Width in px.
    pub width: u32,
    /// Height in px.
    pub height: u32,
    /// Displacement, R = x and G = y around 128, B = 128, A = 255.
    pub displacement: Vec<u8>,
    /// Specular rim, white with the rim intensity in alpha.
    pub specular: Vec<u8>,
    /// Largest displacement of the cross-section in px (feeds [`Glass::filter_params`]).
    pub max_displacement: f32,
}

/// A point's relation to the rounded rectangle's boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSample {
    /// Distance inside the boundary in px (≥ 0).
    pub depth: f32,
    /// Outward unit normal of the nearest boundary, x component (screen space).
    pub nx: f32,
    /// Outward unit normal, y component (y down).
    pub ny: f32,
}

/// Signed distance of the centred point `(cx, cy)` to a `w × h` rectangle with corner
/// radius `r`, and the outward normal of the nearest boundary. `None` outside the shape.
#[must_use]
pub fn rounded_rect(cx: f32, cy: f32, w: f32, h: f32, r: f32) -> Option<EdgeSample> {
    let qx = cx.abs() - (w / 2.0 - r);
    let qy = cy.abs() - (h / 2.0 - r);
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    let sd = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r;
    if sd > 0.0 {
        return None;
    }
    let (mut nx, mut ny) = if qx > 0.0 && qy > 0.0 {
        let l = (ox * ox + oy * oy).sqrt().max(f32::MIN_POSITIVE);
        (ox / l, oy / l)
    } else if qx > qy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    nx *= if cx < 0.0 { -1.0 } else { 1.0 };
    ny *= if cy < 0.0 { -1.0 } else { 1.0 };
    Some(EdgeSample { depth: -sd, nx, ny })
}

/// The smallest rear-light box edge (px) that is drawn; smaller boxes stay transparent.
pub const REAR_MIN_EDGE: usize = 32;

/// The coloured light behind an active surface. The only place colour enters the material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RearLight {
    /// Hue in degrees.
    pub hue: f32,
    /// Saturation, `[0, 1]`.
    pub sat: f32,
    /// Intensity of the field, `[0, 1]`.
    pub halo: f32,
}

/// The rear-light field for a `width × height` box, RGBA8. Its alpha depends only on
/// geometry, `halo` and `strength`, never on the colour, and is exactly 0 on the box's
/// border: the field ends inside its own box without a mask, clip or edge.
#[must_use]
pub fn rear_field(width: u32, height: u32, light: RearLight, strength: f32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; w * h * 4];
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let s = finite(strength).clamp(0.0, 1.0) * finite(light.halo).clamp(0.0, 1.0);
    // Below this size the field cannot fade to zero without a visible step, so it is not drawn.
    if s == 0.0 || w < REAR_MIN_EDGE || h < REAR_MIN_EDGE {
        return out;
    }
    let light = RearLight {
        hue: finite(light.hue),
        sat: finite(light.sat),
        halo: light.halo,
    };
    let sat = light.sat.clamp(0.0, 1.0);
    let rgb = hsl(light.hue, sat, 0.62 + (1.0 - sat) * 0.32);
    // authored: the light sits behind the lower left, a wide soft lobe
    let (fx, fy) = (0.3 * width as f32, 0.72 * height as f32);
    let sigma = 0.3 * width.min(height) as f32 + 0.12 * width.max(height) as f32;
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d2 = (px - fx).powi(2) + (py - fy).powi(2);
            let lobe = (-d2 / (2.0 * sigma * sigma)).exp();
            // separable window: 0 on the border row/column, 1 in the middle
            let u = x as f32 / (w - 1) as f32;
            let v = y as f32 / (h - 1) as f32;
            let window = smooth(4.0 * u * (1.0 - u)) * smooth(4.0 * v * (1.0 - v));
            let a = (s * lobe * window).clamp(0.0, 1.0);
            let i = (y * w + x) * 4;
            out[i..i + 3].copy_from_slice(&rgb);
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// HSL (degrees, 0..1, 0..1) to sRGB bytes.
#[must_use]
pub fn hsl(h: f32, s: f32, l: f32) -> [u8; 3] {
    let k = |n: f32| (n + h / 30.0).rem_euclid(12.0);
    let a = s * l.min(1.0 - l);
    let f = |n: f32| l - a * (k(n) - 3.0).min(9.0 - k(n)).clamp(-1.0, 1.0);
    [f(0.0), f(8.0), f(4.0)].map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// The error of [`png_rgba`].
#[cfg(feature = "optics-png")]
pub type PngError = png::EncodingError;

/// Encode an RGBA8 buffer as PNG.
#[cfg(feature = "optics-png")]
pub fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, PngError> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESETS: [(&str, f32, f32); 4] = [
        ("ruby", 352.0, 0.75),
        ("ocean", 212.0, 0.80),
        ("mint", 158.0, 0.55),
        ("vanilla", 45.0, 0.70),
    ];

    fn alpha(rgba: &[u8]) -> Vec<u8> {
        rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect()
    }

    #[test]
    fn f0_is_four_percent_at_ior_one_and_a_half() {
        assert!((Glass::DEFAULT.f0() - 0.04).abs() < 1e-6);
    }

    #[test]
    fn profiles_rise_from_the_edge_to_the_flat_face() {
        for p in [Profile::Circle, Profile::Squircle] {
            assert_eq!(p.height(0.0), 0.0);
            assert!((p.height(1.0) - 1.0).abs() < 1e-6);
            assert!(p.height(0.25) < p.height(0.75));
        }
        // the squircle is steeper at the rim than the circle a little way in
        assert!(Profile::Squircle.height(0.1) > Profile::Circle.height(0.1));
    }

    /// Light bends inwards on the convex slope and not at all on the flat face; the
    /// displacement grows with the refractive index. Reference values from the report-rs
    /// prototype (`static/optics-lab.js`, same defaults): x = 0.05 → −11.43 px, 0.5 → −0.976 px.
    #[test]
    fn refraction_bends_inwards_and_follows_the_index() {
        let g = Glass::DEFAULT;
        assert!(
            (g.displacement(0.05) + 11.429).abs() < 0.05,
            "{}",
            g.displacement(0.05)
        );
        assert!(
            (g.displacement(0.5) + 0.9757).abs() < 0.01,
            "{}",
            g.displacement(0.5)
        );
        assert!(
            g.displacement(1.0).abs() < 1e-3,
            "flat face must not displace"
        );
        let air = Glass { ior: 1.0, ..g };
        assert!(air.cross_section().max < 1e-3, "n = 1 refracts nothing");
        let dense = Glass { ior: 1.9, ..g };
        assert!(dense.cross_section().max > g.cross_section().max);
    }

    #[test]
    fn the_maps_displace_only_inside_the_bevel() {
        let g = Glass::DEFAULT;
        let m = g.render(200, 120);
        let at = |x: usize, y: usize| &m.displacement[(y * 200 + x) * 4..][..4];
        assert_eq!(
            at(100, 60),
            &NEUTRAL_DISPLACEMENT,
            "centre is the flat face"
        );
        assert_eq!(
            at(0, 0),
            &NEUTRAL_DISPLACEMENT,
            "the corner pixel is outside the shape"
        );
        // just inside the right edge the map samples from further left (inwards, −x)
        assert!(at(193, 60)[0] < 128 && at(193, 60)[1] == 128);
        // and just inside the top edge, from further down (inwards, +y)
        assert!(at(100, 6)[1] > 128 && at(100, 6)[0] == 128);
        let touched = m
            .displacement
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p != NEUTRAL_DISPLACEMENT)
            .count();
        assert!(
            touched > 0 && touched * 3 < 200 * 120,
            "only the band is displaced: {touched}"
        );
    }

    /// The rim faces the key light: with light from the upper left, the top edge glints and
    /// the bottom edge does not.
    #[test]
    fn the_rim_follows_the_key_light() {
        let m = Glass::DEFAULT.render(200, 120);
        let a = |x: usize, y: usize| m.specular[(y * 200 + x) * 4 + 3];
        let top = (2..10).map(|y| a(100, y)).max().unwrap();
        let bottom = (110..118).map(|y| a(100, y)).max().unwrap();
        assert!(top > 0, "top edge must glint");
        assert_eq!(bottom, 0, "bottom edge faces away from the light");
    }

    /// G3: strength 0 equals a neutral baseline in which every term is disabled, compared
    /// against a baseline built independently, not by switching terms off one by one.
    #[test]
    fn strength_zero_equals_the_neutral_baseline() {
        let (w, h) = (160u32, 90u32);
        let off = Glass {
            strength: 0.0,
            ..Glass::DEFAULT
        }
        .render(w, h);
        let n = (w * h) as usize;
        let baseline_disp: Vec<u8> = NEUTRAL_DISPLACEMENT.repeat(n);
        let baseline_spec = vec![0u8; n * 4];
        assert_eq!(off.displacement, baseline_disp);
        assert_eq!(off.specular, baseline_spec);
        let p = Glass {
            strength: 0.0,
            ..Glass::DEFAULT
        }
        .filter_params(off.max_displacement);
        assert_eq!(
            (
                p.displacement_scale,
                p.fine_sigma,
                p.wide_sigma,
                p.tint_alpha
            ),
            (0.0, 0.0, 0.0, 0.0)
        );
        let light = RearLight {
            hue: 352.0,
            sat: 0.75,
            halo: 0.45,
        };
        assert!(rear_field(w, h, light, 0.0).iter().all(|&b| b == 0));
        // and the same material at full strength is NOT the baseline (the check can fail)
        let on = Glass::DEFAULT.render(w, h);
        assert_ne!(on.displacement, baseline_disp);
        assert_ne!(on.specular, baseline_spec);
        assert!(rear_field(w, h, light, 1.0).iter().any(|&b| b != 0));
    }

    /// G2: at equal strength the colour presets differ only in the rear light's colour; its
    /// alpha and the glass maps are identical.
    #[test]
    fn colour_presets_change_only_the_rear_colour() {
        let (w, h) = (240u32, 140u32);
        let glass = Glass {
            strength: 0.53,
            ..Glass::DEFAULT
        }
        .render(w, h);
        let fields: Vec<Vec<u8>> = PRESETS
            .iter()
            .map(|&(_, hue, sat)| {
                rear_field(
                    w,
                    h,
                    RearLight {
                        hue,
                        sat,
                        halo: 0.45,
                    },
                    0.53,
                )
            })
            .collect();
        for f in &fields[1..] {
            assert_eq!(
                alpha(f),
                alpha(&fields[0]),
                "rear alpha must not depend on colour"
            );
            assert_ne!(f, &fields[0], "the colour itself must differ");
        }
        // the glass has no colour input at all; rendering twice is identical
        assert_eq!(
            glass,
            Glass {
                strength: 0.53,
                ..Glass::DEFAULT
            }
            .render(w, h)
        );
    }

    /// G5: the rear field ends inside its own box: every border pixel is transparent, the
    /// field is visible inside, and it fades without a step in either direction. Checked at
    /// the smallest drawn size and at the sizes the lab uses (a control's rear box is about
    /// 200 px, a panel's about 1600 px).
    #[test]
    fn the_rear_field_has_no_silhouette() {
        let light = RearLight {
            hue: 212.0,
            sat: 0.8,
            halo: 1.0,
        };
        for (w, h) in [
            (REAR_MIN_EDGE, REAR_MIN_EDGE),
            (202, 186),
            (300, 180),
            (740, 520),
            (1600, 1300),
        ] {
            let f = rear_field(w as u32, h as u32, light, 1.0);
            let a = |x: usize, y: usize| f[(y * w + x) * 4 + 3];
            for x in 0..w {
                assert_eq!(
                    (a(x, 0), a(x, h - 1)),
                    (0, 0),
                    "{w}x{h}: top/bottom border at x = {x}"
                );
            }
            for y in 0..h {
                assert_eq!(
                    (a(0, y), a(w - 1, y)),
                    (0, 0),
                    "{w}x{h}: left/right border at y = {y}"
                );
            }
            assert!(
                a(w * 3 / 10, h * 7 / 10) > 60,
                "{w}x{h}: the field is visible inside"
            );
            if w.min(h) < 150 {
                continue; // a box this small cannot fade within 8 levels per pixel
            }
            let mut max_step = 0;
            for y in 0..h {
                for x in 0..w {
                    if x > 0 {
                        max_step = max_step.max(a(x, y).abs_diff(a(x - 1, y)));
                    }
                    if y > 0 {
                        max_step = max_step.max(a(x, y).abs_diff(a(x, y - 1)));
                    }
                }
            }
            assert!(max_step <= 8, "{w}x{h}: largest step {max_step}");
        }
        // below the minimum the field is not drawn at all, rather than drawn with an edge
        assert!(rear_field(31, 200, light, 1.0).iter().all(|&b| b == 0));
    }

    /// G3 continuity: every term scales with strength, so a strength just above 0 is still
    /// (within one 8-bit step) the neutral baseline. This catches a term that ignores
    /// strength, which the strength-0 test alone cannot see behind its early return.
    #[test]
    fn a_tiny_strength_stays_within_one_step_of_neutral() {
        let (w, h) = (240u32, 140u32);
        let g = Glass {
            strength: 1e-3,
            ..Glass::DEFAULT
        };
        let spec = g.render_specular(w, h);
        assert!(
            spec.as_chunks::<4>().0.iter().all(|p| p[3] <= 1),
            "rim ignores strength"
        );
        let rear = rear_field(
            w + 200,
            h + 200,
            RearLight {
                hue: 352.0,
                sat: 0.75,
                halo: 1.0,
            },
            1e-3,
        );
        assert!(
            rear.as_chunks::<4>().0.iter().all(|p| p[3] <= 1),
            "rear ignores strength"
        );
        let p = g.filter_params(g.cross_section().max);
        assert!(
            p.displacement_scale < 0.1 && p.wide_sigma < 0.05 && p.tint_alpha < 0.001,
            "{p:?}"
        );
        // and the same check fails at full strength
        let full = Glass::DEFAULT.render_specular(w, h);
        assert!(full.as_chunks::<4>().0.iter().any(|p| p[3] > 1));
    }

    /// Non-finite inputs never reach the maps: they take the default's value, and a NaN
    /// strength is no effect.
    #[test]
    fn non_finite_values_never_reach_the_maps() {
        let bad = Glass {
            thickness: f32::NAN,
            ior: f32::INFINITY,
            ..Glass::DEFAULT
        };
        assert_eq!(bad.render(80, 60), Glass::DEFAULT.render(80, 60));
        let nan_strength = Glass {
            strength: f32::NAN,
            ..Glass::DEFAULT
        };
        assert_eq!(
            nan_strength.render(80, 60),
            Glass {
                strength: 0.0,
                ..Glass::DEFAULT
            }
            .render(80, 60)
        );
        let light = RearLight {
            hue: f32::NAN,
            sat: f32::NAN,
            halo: f32::NAN,
        };
        assert!(rear_field(100, 100, light, 1.0).iter().all(|&b| b == 0));
    }

    #[test]
    fn hsl_matches_known_colours() {
        assert_eq!(hsl(0.0, 1.0, 0.5), [255, 0, 0]);
        assert_eq!(hsl(120.0, 1.0, 0.5), [0, 255, 0]);
        assert_eq!(hsl(240.0, 1.0, 0.5), [0, 0, 255]);
        assert_eq!(hsl(0.0, 0.0, 1.0), [255, 255, 255]);
    }

    #[cfg(feature = "optics-png")]
    #[test]
    fn maps_encode_as_png() {
        let m = Glass::DEFAULT.render(64, 40);
        let png = png_rgba(64, 40, &m.displacement).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }
}
