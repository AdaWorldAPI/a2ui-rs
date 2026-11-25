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
    /// Opacity of the Fresnel reflection of a bright environment: white, alpha =
    /// reflectance (F0 ≈ 4 % on the flat face, rising on the bevel). Independent of the key
    /// light, unlike the rim.
    pub reflection: f32,
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
        reflection: 1.0,
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
            reflection: f(self.reflection, d.reflection),
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
        refract_offset(d, h * self.thickness + self.depth, self.ior)
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

    /// The Fresnel reflection alone: white, alpha = Schlick reflectance of the tilted surface
    /// × `reflection` × strength. Over the whole shape (the flat face reflects F0), not only the
    /// bevel. Depends on the shape, the profile, the index and the strength; not on the key
    /// light or colour.
    #[must_use]
    pub fn render_fresnel(&self, width: u32, height: u32) -> Vec<u8> {
        let g = self.sanitized();
        let (w, h) = (width as usize, height as usize);
        let mut map = vec![0u8; w * h * 4];
        let s = g.strength();
        if s == 0.0 || w == 0 || h == 0 {
            return map;
        }
        let f0 = g.f0();
        let face = ((f0 * g.reflection.max(0.0) * s).clamp(0.0, 1.0) * 255.0).round() as u8;
        let (wf, hf) = (width as f32, height as f32);
        let r = g.radius.min(wf / 2.0).min(hf / 2.0).max(0.0);
        for y in 0..h {
            for x in 0..w {
                let cx = x as f32 + 0.5 - wf / 2.0;
                let cy = y as f32 + 0.5 - hf / 2.0;
                if rounded_rect(cx, cy, wf, hf, r).is_some() {
                    map[(y * w + x) * 4..][..4].copy_from_slice(&[255, 255, 255, face]);
                }
            }
        }
        let slope_scale = g.thickness / g.bezel.max(1e-3);
        g.for_each_bevel_pixel(width, height, |i, _, xb| {
            let tilt = (g.profile.slope(xb) * slope_scale).atan();
            let fres = f0 + (1.0 - f0) * (1.0 - tilt.cos()).powi(5);
            map[i + 3] = ((fres * g.reflection.max(0.0) * s).clamp(0.0, 1.0) * 255.0).round() as u8;
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

/// The shared refraction kernel. A ray perpendicular to the page meets a surface whose
/// height rises inwards with `slope` (px of height per px inwards), refracts into a medium of
/// index `ior` (Snell, vector form, n1 = 1) and travels `travel` px down to the background.
/// Returns its lateral travel along the outward direction: negative on a convex surface (the
/// ray bends inwards, which is what magnifies), zero on a flat surface. Every geometry in this
/// module (the glass card's bevel, the droplet's cap) goes through this one function.
#[must_use]
pub fn refract_offset(slope: f32, travel: f32, ior: f32) -> f32 {
    // surface normal in the (outward, z) plane
    let nl = (slope * slope + 1.0).sqrt();
    let (nx, nz) = (slope / nl, 1.0 / nl);
    // incident ray (0, −1); eta = 1 / IOR, cos i = nz
    let eta = 1.0 / ior.max(1e-3);
    let cosi = nz;
    let k = 1.0 - eta * eta * (1.0 - cosi * cosi);
    if k < 0.0 {
        return 0.0;
    }
    let m = eta * cosi - k.sqrt();
    let tx = m * nx;
    let tz = -eta + m * nz;
    tx / tz.abs() * travel
}

/// A transparent medium: its refractive index at the d line (587.6 nm) and its Abbe number,
/// which together fix the index at every wavelength (Cauchy, `n(λ) = A + B/λ²`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Medium {
    /// Index at 587.6 nm.
    pub ior: f32,
    /// Abbe number `V = (n_d − 1)/(n_F − n_C)`; `None` is a medium without dispersion.
    pub abbe: Option<f32>,
}

impl Medium {
    /// Water at 20 °C: n_d 1.333, V 55.7 (literature values, not measured by us).
    pub const WATER: Self = Self {
        ior: 1.333,
        abbe: Some(55.7),
    };
    /// Crown glass (BK7): n_d 1.5168, V 64.17 (manufacturer data sheet values).
    pub const CROWN_GLASS: Self = Self {
        ior: 1.5168,
        abbe: Some(64.17),
    };
    /// Diamond: n_d 2.417, V 55.3 (literature values, not measured by us).
    pub const DIAMOND: Self = Self {
        ior: 2.417,
        abbe: Some(55.3),
    };

    /// The indices for red, green and blue, taken at the lines the Abbe number is defined on
    /// (C 656.3 nm, d 587.6 nm, F 486.1 nm), so green is exactly `ior` and
    /// `n_B − n_R = (ior − 1)/V`. `exaggeration` multiplies the spread; 1 is physical.
    /// Without an Abbe number (or with a non-finite or non-positive one) all three equal `ior`.
    #[must_use]
    pub fn channel_iors(&self, exaggeration: f32) -> [f32; 3] {
        let n = self.ior;
        let Some(v) = self.abbe.filter(|v| v.is_finite() && *v > 0.0) else {
            return [n; 3];
        };
        let e = if exaggeration.is_finite() {
            exaggeration.max(0.0)
        } else {
            1.0
        };
        let (c, d, f) = (0.6563f32, 0.5876f32, 0.4861f32); // µm
        let dn = (n - 1.0) / v * e;
        let b = dn / (1.0 / (f * f) - 1.0 / (c * c));
        let a = n - b / (d * d);
        [a + b / (c * c), n, a + b / (f * f)]
    }
}

/// A droplet resting on the page: a spherical cap with circular footprint `radius` (px) and
/// apex `height` (px). Rays refract at the curved top and travel through the drop to the
/// page, which is what magnifies the text under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Droplet {
    /// Footprint radius in px.
    pub radius: f32,
    /// Apex height in px, `(0, radius]`; `radius` is a hemisphere.
    pub height: f32,
}

impl Droplet {
    /// The radius of the sphere the cap is cut from.
    #[must_use]
    pub fn sphere_radius(&self) -> f32 {
        let (a, h) = (
            self.radius.max(1e-3),
            self.height.clamp(1e-3, self.radius.max(1e-3)),
        );
        (a * a + h * h) / (2.0 * h)
    }

    /// Height of the surface over the page and its slope (rising inwards) at distance `r`
    /// from the centre; `None` outside the footprint.
    #[must_use]
    pub fn surface(&self, r: f32) -> Option<(f32, f32)> {
        let a = self.radius.max(1e-3);
        if !(0.0..a).contains(&r) {
            return None;
        }
        let rs = self.sphere_radius();
        let h = self.height.clamp(1e-3, a);
        let root = (rs * rs - r * r).max(1e-6).sqrt();
        Some((root - (rs - h), r / root))
    }

    /// The magnification at the centre: the sampling point is pulled in by a fixed fraction
    /// near the axis, `r' = r (1 − k)`, so the text appears `1 / (1 − k)` times larger.
    #[must_use]
    pub fn centre_magnification(&self, ior: f32) -> f32 {
        let r1 = self.radius.max(1e-3) / CROSS_SECTION_SAMPLES as f32;
        let s1 = self.cross_section(ior).samples[1];
        1.0 / (1.0 + s1 / r1)
    }

    /// The radial cross-section of the displacement for one medium index, `r = i/N · radius`.
    #[must_use]
    pub fn cross_section(&self, ior: f32) -> CrossSection {
        let mut samples = [0.0f32; CROSS_SECTION_SAMPLES + 1];
        let mut max = 0.0f32;
        for (i, s) in samples.iter_mut().enumerate() {
            let r = i as f32 / CROSS_SECTION_SAMPLES as f32 * self.radius;
            *s = self
                .surface(r)
                .map_or(0.0, |(h, slope)| refract_offset(slope, h, ior));
            max = max.max(s.abs());
        }
        CrossSection { samples, max }
    }

    /// The droplet's maps in a square of side `2·radius + 2` px, centred: the displacement
    /// map (green-channel index, normalized like the glass map), a specular map (Blinn-Phong
    /// highlight of the key light, Schlick-weighted) and a rim map (black, alpha = Fresnel
    /// reflectance: the edge transmits less of the page). Per-channel filter scales come
    /// from [`Medium::channel_iors`].
    #[must_use]
    pub fn render(&self, medium: Medium, light: KeyLight, dispersion: f32) -> DropletMaps {
        let side = (2.0 * self.radius.max(1.0)).ceil() as u32 + 2;
        let n = side as usize;
        let [nr, ng, nb] = medium.channel_iors(dispersion);
        let cs = self.cross_section(ng);
        let maxes = [
            self.cross_section(nr).max,
            cs.max,
            self.cross_section(nb).max,
        ];
        let mut displacement = NEUTRAL_DISPLACEMENT.repeat(n * n);
        let mut specular = vec![0u8; n * n * 4];
        let mut rim = vec![0u8; n * n * 4];
        let c = side as f32 / 2.0;
        let f0 = ((ng - 1.0) / (ng + 1.0)).powi(2);
        let (la, el) = (
            light.angle_deg.to_radians(),
            light.elevation_deg.to_radians(),
        );
        let l = [la.cos() * el.cos(), la.sin() * el.cos(), el.sin()];
        let hv = {
            let v = [l[0], l[1], l[2] + 1.0];
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
            [v[0] / len, v[1] / len, v[2] / len]
        };
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
                let r = (dx * dx + dy * dy).sqrt();
                let Some((_, slope)) = self.surface(r) else {
                    continue;
                };
                let (ox, oy) = if r > 0.0 {
                    (dx / r, dy / r)
                } else {
                    (0.0, 0.0)
                };
                let i = (y * n + x) * 4;
                let idx = ((r / self.radius) * CROSS_SECTION_SAMPLES as f32).round() as usize;
                let m = if cs.max > 0.0 {
                    cs.samples[idx.min(CROSS_SECTION_SAMPLES)] / cs.max
                } else {
                    0.0
                };
                displacement[i] = (128.0 + 127.0 * m * ox).round() as u8;
                displacement[i + 1] = (128.0 + 127.0 * m * oy).round() as u8;
                // 3-D surface normal: tilted outwards by the slope
                let nl = (slope * slope + 1.0).sqrt();
                let nrm = [ox * slope / nl, oy * slope / nl, 1.0 / nl];
                let cos_v = nrm[2];
                let fres = f0 + (1.0 - f0) * (1.0 - cos_v).powi(5);
                let nh = (nrm[0] * hv[0] + nrm[1] * hv[1] + nrm[2] * hv[2]).max(0.0);
                let spec = (nh.powf(light.shininess)
                    * (0.25 + 0.75 * fres / f0.max(1e-3)).min(4.0)
                    * light.intensity)
                    .clamp(0.0, 1.0);
                specular[i..i + 4].copy_from_slice(&[255, 255, 255, (spec * 255.0).round() as u8]);
                rim[i + 3] = (fres.clamp(0.0, 1.0) * light.rim * 255.0).round() as u8;
            }
        }
        let scales = maxes.map(|mx| 2.0 * mx);
        DropletMaps {
            side,
            displacement,
            specular,
            rim,
            channel_scales: scales,
        }
    }
}

/// The key light for surface highlights: direction in the page plane (screen space, y down),
/// elevation above the page, Blinn-Phong shininess, highlight intensity and rim strength.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyLight {
    /// Azimuth in degrees, screen space.
    pub angle_deg: f32,
    /// Elevation above the page in degrees.
    pub elevation_deg: f32,
    /// Blinn-Phong exponent.
    pub shininess: f32,
    /// Peak highlight opacity.
    pub intensity: f32,
    /// Opacity of the Fresnel rim at grazing angles.
    pub rim: f32,
}

impl KeyLight {
    /// Light from the upper left, 55° above the page (authored).
    pub const DEFAULT: Self = Self {
        angle_deg: -135.0,
        elevation_deg: 55.0,
        shininess: 120.0,
        intensity: 0.9,
        rim: 0.35,
    };
}

/// The rendered maps of a droplet, RGBA8, a `side × side` square centred on the drop.
#[derive(Debug, Clone, PartialEq)]
pub struct DropletMaps {
    /// Edge of the square in px.
    pub side: u32,
    /// Displacement for the green index, normalized like the glass map.
    pub displacement: Vec<u8>,
    /// Specular highlight, white with the intensity in alpha.
    pub specular: Vec<u8>,
    /// Fresnel rim, black with the reflectance in alpha.
    pub rim: Vec<u8>,
    /// `feDisplacementMap` scale per channel (R, G, B): the same map scaled per wavelength.
    pub channel_scales: [f32; 3],
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

    /// The Fresnel reflection covers the whole shape at F0 and rises on the bevel; it does
    /// not move with the key light (unlike the rim) and is empty at strength 0.
    #[test]
    fn fresnel_reflection_is_f0_on_the_face_and_rises_on_the_bevel() {
        let (w, h) = (200usize, 120usize);
        let m = Glass::DEFAULT.render_fresnel(w as u32, h as u32);
        let a = |x: usize, y: usize| m[(y * w + x) * 4 + 3];
        assert_eq!(
            a(100, 60),
            (0.04f32 * 255.0).round() as u8,
            "flat face reflects F0"
        );
        assert!(
            a(100, 1) > a(100, 60),
            "bevel {} vs face {}",
            a(100, 1),
            a(100, 60)
        );
        // not directional: the top, bottom, left and right edges reflect alike
        assert_eq!(a(100, 1), a(100, h - 2));
        assert_eq!(a(1, 60), a(w - 2, 60));
        // a steeper bevel reflects more at the edge
        let steep = Glass {
            thickness: 60.0,
            ..Glass::DEFAULT
        }
        .render_fresnel(w as u32, h as u32);
        assert!(
            steep[(w + 100) * 4 + 3] > a(100, 1),
            "steep bevel must reflect more"
        );
        assert_eq!(a(0, 0), 0, "outside the shape");
        let turned = Glass {
            light_angle_deg: 70.0,
            ..Glass::DEFAULT
        }
        .render_fresnel(w as u32, h as u32);
        assert_eq!(m, turned, "the key light does not move the Fresnel term");
        assert!(
            Glass {
                strength: 0.0,
                ..Glass::DEFAULT
            }
            .render_fresnel(80, 60)
            .iter()
            .all(|&b| b == 0)
        );
    }

    const DROP: Droplet = Droplet {
        radius: 60.0,
        height: 18.0,
    };

    /// The droplet magnifies: the map samples inwards, by an amount that is zero at the
    /// centre and at the contact line and largest in between.
    #[test]
    fn a_droplet_refracts_inwards_and_vanishes_at_centre_and_rim() {
        let cs = DROP.cross_section(Medium::WATER.ior);
        assert_eq!(cs.samples[0], 0.0, "no offset at the centre");
        assert!(
            cs.samples[CROSS_SECTION_SAMPLES].abs() < 1e-3,
            "outside the footprint"
        );
        let inner = &cs.samples[1..CROSS_SECTION_SAMPLES];
        assert!(inner.iter().all(|&v| v < 0.0), "every sample bends inwards");
        let peak = inner.iter().cloned().fold(0.0f32, f32::min);
        assert!(
            peak < -1.0 && (inner[0] > peak) && (inner[inner.len() - 1] > peak),
            "peak {peak}"
        );
    }

    /// Near the centre the sampling point is pulled in proportionally, which is a
    /// magnification `1 / (1 − k)` > 1; a taller drop magnifies more, a flat one not at all.
    #[test]
    fn a_taller_drop_magnifies_more() {
        let mag = |d: Droplet| d.centre_magnification(Medium::WATER.ior);
        let flat = mag(Droplet {
            radius: 60.0,
            height: 0.5,
        });
        let low = mag(Droplet {
            radius: 60.0,
            height: 12.0,
        });
        let high = mag(Droplet {
            radius: 60.0,
            height: 30.0,
        });
        assert!((flat - 1.0).abs() < 0.01, "flat {flat}");
        assert!(1.0 < low && low < high, "low {low} high {high}");
    }

    /// Textbook check of the whole chain: an object at the flat face of a hemispherical lens,
    /// viewed along the axis, is magnified by exactly the refractive index.
    #[test]
    fn a_hemisphere_magnifies_by_the_refractive_index() {
        let hemi = Droplet {
            radius: 90.0,
            height: 90.0,
        };
        for m in [Medium::WATER, Medium::CROWN_GLASS, Medium::DIAMOND] {
            let mag = hemi.centre_magnification(m.ior);
            assert!((mag / m.ior - 1.0).abs() < 0.01, "n {} gives ×{mag}", m.ior);
        }
    }

    #[test]
    fn denser_media_refract_more() {
        let max = |m: Medium| DROP.cross_section(m.ior).max;
        assert!(max(Medium::WATER) < max(Medium::CROWN_GLASS));
        assert!(max(Medium::CROWN_GLASS) < max(Medium::DIAMOND));
    }

    /// Dispersion follows the Abbe number: red < green < blue, green is the d-line index,
    /// the spread is (n − 1)/V, exaggeration scales it, and no Abbe number means none.
    #[test]
    fn channel_indices_follow_the_abbe_number() {
        let [r, g, b] = Medium::CROWN_GLASS.channel_iors(1.0);
        assert_eq!(g, Medium::CROWN_GLASS.ior);
        assert!(r < g && g < b);
        assert!(((b - r) - (1.5168 - 1.0) / 64.17).abs() < 1e-5, "{}", b - r);
        let [r2, _, b2] = Medium::CROWN_GLASS.channel_iors(2.0);
        assert!(((b2 - r2) / (b - r) - 2.0).abs() < 1e-3);
        let plain = Medium {
            ior: 1.5,
            abbe: None,
        };
        assert_eq!(plain.channel_iors(5.0), [1.5; 3]);
        assert_eq!(
            Medium {
                abbe: Some(f32::NAN),
                ..plain
            }
            .channel_iors(1.0),
            [1.5; 3]
        );
        assert_eq!(Medium::WATER.channel_iors(0.0), [Medium::WATER.ior; 3]);
    }

    /// The per-channel filter scales are ordered by wavelength, and equal without dispersion.
    #[test]
    fn droplet_channel_scales_split_only_with_dispersion() {
        let m = DROP.render(Medium::WATER, KeyLight::DEFAULT, 1.0);
        let [r, g, b] = m.channel_scales;
        assert!(r < g && g < b, "{:?}", m.channel_scales);
        let none = DROP.render(Medium::WATER, KeyLight::DEFAULT, 0.0);
        assert_eq!(none.channel_scales, [none.channel_scales[1]; 3]);
        assert_eq!(
            m.displacement, none.displacement,
            "the map is the green index in both"
        );
    }

    /// The highlight sits on the side facing the key light; the Fresnel rim is strongest
    /// at the contact line.
    #[test]
    fn the_highlight_faces_the_light_and_the_rim_is_at_the_edge() {
        let m = DROP.render(Medium::WATER, KeyLight::DEFAULT, 0.0);
        let n = m.side as usize;
        let a = |buf: &[u8], x: usize, y: usize| buf[(y * n + x) * 4 + 3];
        let (mut best, mut at) = (0u8, (0, 0));
        for y in 0..n {
            for x in 0..n {
                if a(&m.specular, x, y) > best {
                    best = a(&m.specular, x, y);
                    at = (x, y);
                }
            }
        }
        assert!(best > 100, "highlight {best}");
        assert!(
            at.0 < n / 2 && at.1 < n / 2,
            "light from the upper left, highlight at {at:?}"
        );
        assert_eq!(a(&m.rim, 0, 0), 0, "outside the drop");
        // Fresnel reflectance grows with the tilt: on a steep drop the rim rises at the
        // contact line, on a flat one it stays near F0 everywhere
        let tall = Droplet {
            radius: 60.0,
            height: 55.0,
        }
        .render(Medium::WATER, KeyLight::DEFAULT, 0.0);
        let t = tall.side as usize;
        let ra = |x: usize| tall.rim[((t / 2) * t + x) * 4 + 3];
        assert!(
            ra(t / 2 + 58) > 2 * ra(t / 2 + 20),
            "rim {} vs {}",
            ra(t / 2 + 58),
            ra(t / 2 + 20)
        );
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
