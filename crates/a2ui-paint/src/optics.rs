//! The optical engine: shared physics for every material surface.
//!
//! - [`refract_offset`]: the one refraction kernel (Snell, vector form). Every geometry goes
//!   through it.
//! - [`Medium`]: refractive index and Abbe number, per-channel indices for dispersion.
//! - [`KeyLight`]: the key light for highlights and the Fresnel rim.
//! - [`surface`]: the one optical surface specification (footprint, relief, material,
//!   transmission, tint, light, strength). Droplet, capsule and card are presets of it; the
//!   SVG maps and the WebGL2 shader (`a2ui-optics-gl`) are its backends. The earlier bevel
//!   model of the card lab (`Glass`) and the separate `Droplet` were retired onto it.
//! - [`rear_field`]: the coloured light behind an active surface, the only place the
//!   atmosphere's colour enters (plan gate G2), fading to zero inside its own box (G5).
//!
//! Pure arithmetic, no dependencies. The `optics-png` feature adds PNG encoding of the maps
//! for consumers that serve them as image assets.

pub mod surface;

/// The R/G/B/A value of "no displacement" (`feDisplacementMap` reads 128 as zero).
pub const NEUTRAL_DISPLACEMENT: [u8; 4] = [128, 128, 128, 255];

/// The shared refraction kernel. A ray perpendicular to the page meets a surface whose
/// height rises inwards with `slope` (px of height per px inwards), refracts into a medium of
/// index `ior` (Snell, vector form, n1 = 1) and travels `travel` px down to the background.
/// Returns its lateral travel along the outward direction: negative on a convex surface (the
/// ray bends inwards, which is what magnifies), zero on a flat surface. Every surface goes through
/// this one function.
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

    /// G2: the colour presets differ only in the rear light's colour; its alpha does not
    /// depend on colour (and the surfaces have no atmosphere-colour input at all).
    #[test]
    fn colour_presets_change_only_the_rear_colour() {
        let (w, h) = (240u32, 140u32);
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
    }

    #[test]
    fn the_rear_field_is_empty_at_strength_zero_and_for_non_finite_input() {
        let light = RearLight {
            hue: 352.0,
            sat: 0.75,
            halo: 0.45,
        };
        assert!(rear_field(160, 90, light, 0.0).iter().all(|&b| b == 0));
        assert!(rear_field(160, 90, light, 1.0).iter().any(|&b| b != 0));
        let bad = RearLight {
            hue: f32::NAN,
            sat: f32::NAN,
            halo: f32::NAN,
        };
        assert!(rear_field(100, 100, bad, 1.0).iter().all(|&b| b == 0));
        let tiny = rear_field(440, 340, RearLight { halo: 1.0, ..light }, 1e-3);
        assert!(
            tiny.as_chunks::<4>().0.iter().all(|p| p[3] <= 1),
            "rear ignores strength"
        );
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
        let m = surface::Surface::preset("clear-card")
            .unwrap()
            .render_maps();
        let png = png_rgba(m.width, m.height, &m.displacement).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }
}
