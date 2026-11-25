//! **a2ui-optics-gl** — optical surfaces on the GPU, raw WebGL2, no wgpu.
//!
//! A WebGL2 fragment shader ([`FRAGMENT_SHADER`]) evaluates the one surface specification of
//! [`a2ui_paint::optics::surface`] per pixel: footprint distance, shoulder and dome relief,
//! per-channel refraction through the shared kernel (ported line for line), direct / fine /
//! wide transmission, body tint, Fresnel rim and key-light highlight. Rust computes every
//! constant the shader uses ([`SurfaceUniforms`]) from a [`Surface`], so droplet, capsule and
//! card are presets of one model, not shaders.
//!
//! The browser tier (`web`, wasm32 only) drives raw WebGL2 through `web-sys`. wgpu is not
//! used: its adapter and device warm-up shows as a first-frame flicker. Frames are drawn on
//! change only, into a canvas the size of the surface's box.
//!
//! The reference for the parity check is [`Surface::sample_points`] and
//! [`Surface::coverage`]; the shader is two implementations' worth of risk against one
//! specification, which is what the browser check (`SurfaceRenderer::parity`) measures.
//!
//! Experimental, Material Laboratory only (plan `.claude/plans/a2ui-material-appearance-v1.md`).

pub use a2ui_paint::optics::surface::Surface;
use a2ui_paint::optics::surface::half_vector;

/// The fragment shader (GLSL ES 3.00).
pub const FRAGMENT_SHADER: &str = include_str!("surface.frag");

/// A full-screen triangle from `gl_VertexID`; no vertex buffers.
pub const VERTEX_SHADER: &str = "#version 300 es
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Margin around the surface's box in the GPU canvas, device px (room for the edge ramp).
pub const CANVAS_MARGIN: u32 = 2;

/// Everything the shader needs, in device pixels, computed from one [`Surface`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceUniforms {
    pub halfsize: [f32; 2],
    pub corner: f32,
    pub inradius: f32,
    pub relief: [f32; 4],
    pub base: f32,
    pub ior: [f32; 3],
    pub refraction: f32,
    pub weights: [f32; 3],
    pub sigma: [f32; 2],
    pub tint: [f32; 4],
    pub f0: f32,
    pub rim: f32,
    pub hv: [f32; 3],
    pub shininess: f32,
    pub spec: f32,
    pub reflection: f32,
}

impl SurfaceUniforms {
    /// The uniforms of `s` (already in device pixels). `terms` switches refraction, rim and
    /// highlight (ablation).
    #[must_use]
    pub fn new(s: &Surface, terms: [bool; 3]) -> Self {
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        let s = &s.sanitized();
        let eff = s.effective();
        let fp = s.footprint;
        let rl = s.relief;
        Self {
            halfsize: [fp.width / 2.0, fp.height / 2.0],
            corner: fp.corner_px(),
            inradius: fp.inradius(),
            relief: [rl.height, rl.face, rl.shoulder, rl.profile],
            base: rl.base,
            ior: s.medium.channel_iors(s.dispersion),
            refraction: on(terms[0]) * s.strength(),
            weights: s.scatter.normalized(),
            sigma: [eff.fine_sigma, eff.wide_sigma],
            tint: {
                let [r, g, b] = s.tint.rgb.map(|c| f32::from(c) / 255.0);
                [r, g, b, eff.tint_alpha]
            },
            f0: s.f0(),
            rim: eff.light.rim * on(terms[1]),
            hv: half_vector(&s.light),
            shininess: s.light.shininess,
            spec: eff.light.intensity * on(terms[2]),
            reflection: eff.reflection * on(terms[2]),
        }
    }
}

/// The GPU canvas for `s` (device pixels): its box plus [`CANVAS_MARGIN`] each side.
#[must_use]
pub fn canvas_size(s: &Surface) -> (u32, u32) {
    let (w, h) = s.map_box();
    (w + 2 * CANVAS_MARGIN, h + 2 * CANVAS_MARGIN)
}

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::SurfaceRenderer;

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_paint::optics::Medium;

    #[test]
    fn uniforms_come_from_the_shared_model() {
        let s = Surface::preset("aqua").unwrap();
        let u = SurfaceUniforms::new(&s, [true; 3]);
        assert_eq!(u.ior, s.medium.channel_iors(s.dispersion));
        assert_eq!(u.weights, s.scatter.normalized());
        assert_eq!(u.halfsize, [160.0, 100.0]);
        assert_eq!(u.corner, s.footprint.corner_px());
        assert_eq!(u.relief, [16.0, 0.08, 17.0, 4.0]);
        assert!((u.tint[3] - 0.35).abs() < 1e-6);
        let n = u.hv.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-5);
        let off = SurfaceUniforms::new(&s, [false; 3]);
        assert_eq!(
            (off.refraction, off.rim, off.spec, off.reflection),
            (0.0, 0.0, 0.0, 0.0)
        );
        let zero = SurfaceUniforms::new(&Surface { strength: 0.0, ..s }, [true; 3]);
        assert_eq!(
            (
                zero.refraction,
                zero.spec,
                zero.reflection,
                zero.tint[3],
                zero.sigma
            ),
            (0.0, 0.0, 0.0, 0.0, [0.0, 0.0]),
            "strength 0 is plain glass on the GPU too"
        );
        let water = Surface {
            medium: Medium::WATER,
            ..s
        };
        assert!((SurfaceUniforms::new(&water, [true; 3]).f0 - 0.0201).abs() < 1e-3);
    }

    #[test]
    fn the_canvas_is_the_box_plus_the_margin() {
        let s = Surface::preset("capsule").unwrap();
        assert_eq!(canvas_size(&s), (272 + 4, 92 + 4));
        assert_eq!(canvas_size(&s.scaled(3.0)), (812 + 4, 272 + 4));
    }

    /// The GLSL port carries the same kernel and relief law: their lines appear verbatim.
    #[test]
    fn the_shader_ports_the_specification() {
        for line in [
            "float nl = sqrt(slope * slope + 1.0);",
            "float eta = 1.0 / max(ior, 1e-3);",
            "float k = 1.0 - eta * eta * (1.0 - cosi * cosi);",
            "float m = eta * cosi - sqrt(k);",
            "return tx / abs(tz) * travel;",
            "float t = clamp(d / W, 1e-4, 1.0);",
            "float u = 1.0 - pow(1.0 - t, p);",
            "float rs = (rin * rin + hd * hd) / (2.0 * hd);",
            "float a = clamp(0.5 - sd, 0.0, 1.0);",
            "col = mix(col, u_tint.rgb, u_tint.a);",
            "col = col * (1.0 - refl) + refl;",
        ] {
            assert!(FRAGMENT_SHADER.contains(line), "shader lacks: {line}");
        }
    }
}
