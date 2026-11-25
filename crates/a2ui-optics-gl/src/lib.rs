//! **a2ui-optics-gl** — the raindrop on the GPU, without wgpu.
//!
//! A WebGL2 fragment shader ([`FRAGMENT_SHADER`]) evaluates the droplet of
//! [`a2ui_paint::optics`] per pixel: the spherical cap, the per-channel refraction
//! through the same kernel ([`a2ui_paint::optics::refract_offset`], ported line for
//! line), the Fresnel rim and the key-light highlight. Rust computes every constant the
//! shader uses ([`DropUniforms`]) from the shared types, so there is one optical model
//! and the shader is only its per-pixel evaluation.
//!
//! The browser tier (`web` module, wasm32 only) drives raw WebGL2 through `web-sys`. wgpu
//! is deliberately not used here: its adapter and device warm-up shows as a first-frame
//! flicker. Frames are drawn on demand (a change of position, shape, medium or
//! background), never in a loop.
//!
//! [`cpu_sample_points`] is the CPU reference for the shader's sampling positions; the
//! browser parity check (`DropRenderer::parity`) reads the GPU result back and compares.
//!
//! Experimental, Material Laboratory only (plan `.claude/plans/a2ui-material-appearance-v1.md`).

use a2ui_paint::optics::{Droplet, KeyLight, Medium, refract_offset};

/// The fragment shader (GLSL ES 3.00).
pub const FRAGMENT_SHADER: &str = include_str!("drop.frag");

/// A full-screen triangle from `gl_VertexID`; no vertex buffers.
pub const VERTEX_SHADER: &str = "#version 300 es
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Everything the shader needs, in device pixels, computed from the shared model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropUniforms {
    pub center: [f32; 2],
    pub radius: f32,
    pub sphere: f32,
    pub height: f32,
    pub ior: [f32; 3],
    pub refraction: f32,
    pub f0: f32,
    pub rim: f32,
    pub half: [f32; 3],
    pub shininess: f32,
    pub spec: f32,
}

impl DropUniforms {
    /// The uniforms for a drop at `center` (device pixels) with the given shape, medium,
    /// dispersion exaggeration (0 off, 1 physical) and light. `terms` switches refraction,
    /// rim and highlight (ablation).
    #[must_use]
    pub fn new(
        center: [f32; 2],
        droplet: Droplet,
        medium: Medium,
        dispersion: f32,
        light: KeyLight,
        terms: [bool; 3],
    ) -> Self {
        let ior = medium.channel_iors(dispersion);
        let g = ior[1];
        let (az, el) = (
            light.angle_deg.to_radians(),
            light.elevation_deg.to_radians(),
        );
        let l = [az.cos() * el.cos(), az.sin() * el.cos(), el.sin()];
        let v = [l[0], l[1], l[2] + 1.0];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        Self {
            center,
            radius: droplet.radius,
            sphere: droplet.sphere_radius(),
            height: droplet.height.clamp(1e-3, droplet.radius.max(1e-3)),
            ior,
            refraction: on(terms[0]),
            f0: ((g - 1.0) / (g + 1.0)).powi(2),
            rim: light.rim * on(terms[1]),
            half: [v[0] / len, v[1] / len, v[2] / len],
            shininess: light.shininess,
            spec: light.intensity * on(terms[2]),
        }
    }
}

/// Where the shader samples the page for the pixel whose centre is `pt` (device pixels,
/// top-left origin), per channel: the CPU reference for the GPU parity check. Outside the
/// drop every channel samples `pt` itself.
#[must_use]
pub fn cpu_sample_points(u: &DropUniforms, droplet: Droplet, pt: [f32; 2]) -> [[f32; 2]; 3] {
    let d = [pt[0] - u.center[0], pt[1] - u.center[1]];
    let r = (d[0] * d[0] + d[1] * d[1]).sqrt();
    let Some((h, slope)) = droplet.surface(r) else {
        return [pt; 3];
    };
    let dir = if r > 0.0 {
        [d[0] / r, d[1] / r]
    } else {
        [0.0, 0.0]
    };
    u.ior.map(|n| {
        let off = refract_offset(slope, h, n) * u.refraction;
        [pt[0] + dir[0] * off, pt[1] + dir[1] * off]
    })
}

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::DropRenderer;

#[cfg(test)]
mod tests {
    use super::*;

    const DROP: Droplet = Droplet {
        radius: 100.0,
        height: 85.0,
    };

    #[test]
    fn uniforms_come_from_the_shared_model() {
        let u = DropUniforms::new(
            [128.0, 128.0],
            DROP,
            Medium::WATER,
            1.0,
            KeyLight::DEFAULT,
            [true; 3],
        );
        assert_eq!(u.ior, Medium::WATER.channel_iors(1.0));
        assert_eq!(u.sphere, DROP.sphere_radius());
        assert!((u.f0 - 0.0201).abs() < 1e-3, "{}", u.f0);
        let n = (u.half[0].powi(2) + u.half[1].powi(2) + u.half[2].powi(2)).sqrt();
        assert!((n - 1.0).abs() < 1e-5);
        let off = DropUniforms::new(
            [0.0; 2],
            DROP,
            Medium::WATER,
            1.0,
            KeyLight::DEFAULT,
            [false; 3],
        );
        assert_eq!((off.refraction, off.rim, off.spec), (0.0, 0.0, 0.0));
    }

    /// The CPU reference samples inwards inside the drop, by exactly the shared kernel, and
    /// leaves everything outside untouched.
    #[test]
    fn the_cpu_reference_matches_the_kernel() {
        let u = DropUniforms::new(
            [128.0, 128.0],
            DROP,
            Medium::WATER,
            0.0,
            KeyLight::DEFAULT,
            [true; 3],
        );
        let pt = [178.5, 128.0];
        let s = cpu_sample_points(&u, DROP, pt);
        let (h, slope) = DROP.surface(50.5).unwrap();
        let want = refract_offset(slope, h, Medium::WATER.ior);
        assert!(want < -1.0);
        assert!((s[1][0] - (pt[0] + want)).abs() < 1e-4 && (s[1][1] - pt[1]).abs() < 1e-6);
        assert_eq!(cpu_sample_points(&u, DROP, [10.0, 10.0]), [[10.0, 10.0]; 3]);
        let flat = DropUniforms::new(
            [128.0, 128.0],
            DROP,
            Medium::WATER,
            0.0,
            KeyLight::DEFAULT,
            [false, true, true],
        );
        assert_eq!(
            cpu_sample_points(&flat, DROP, pt),
            [pt; 3],
            "refraction off samples in place"
        );
    }

    /// The GLSL port carries the same kernel: every line of `refract_offset` appears.
    #[test]
    fn the_shader_ports_the_kernel() {
        for line in [
            "float nl = sqrt(slope * slope + 1.0);",
            "float eta = 1.0 / max(ior, 1e-3);",
            "float k = 1.0 - eta * eta * (1.0 - cosi * cosi);",
            "float m = eta * cosi - sqrt(k);",
            "return tx / abs(tz) * travel;",
            "float root = sqrt(max(u_sphere * u_sphere - r * r, 1e-6));",
        ] {
            assert!(FRAGMENT_SHADER.contains(line), "shader lacks: {line}");
        }
    }
}
