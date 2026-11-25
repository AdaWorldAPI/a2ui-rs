//! Raw WebGL2 driver for an optical surface (wasm32 only).

use a2ui_paint::optics::surface::{ErrorStats, Surface};
use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlVertexArrayObject,
};

use crate::{FRAGMENT_SHADER, SurfaceUniforms, VERTEX_SHADER, canvas_size};

fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

fn err(msg: impl Into<String>) -> JsValue {
    JsValue::from_str(&msg.into())
}

fn compile(gl: &Gl, kind: u32, src: &str) -> Result<WebGlShader, JsValue> {
    let s = gl.create_shader(kind).ok_or_else(|| err("create_shader"))?;
    gl.shader_source(&s, src);
    gl.compile_shader(&s);
    if gl.get_shader_parameter(&s, Gl::COMPILE_STATUS).as_bool() == Some(true) {
        Ok(s)
    } else {
        Err(err(gl.get_shader_info_log(&s).unwrap_or_default()))
    }
}

fn texture(gl: &Gl, min: u32, mag: u32) -> Result<WebGlTexture, JsValue> {
    let t = gl.create_texture().ok_or_else(|| err("create_texture"))?;
    gl.bind_texture(Gl::TEXTURE_2D, Some(&t));
    for (p, v) in [
        (Gl::TEXTURE_MIN_FILTER, min),
        (Gl::TEXTURE_MAG_FILTER, mag),
        (Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE),
        (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
    ] {
        gl.tex_parameteri(Gl::TEXTURE_2D, p, v as i32);
    }
    Ok(t)
}

fn stats_json(s: &ErrorStats) -> String {
    format!(
        "{{\"n\":{},\"p50\":{:.6},\"p95\":{:.6},\"p99\":{:.6},\"max\":{:.6}}}",
        s.n, s.p50, s.p95, s.p99, s.max
    )
}

/// One optical surface on its own canvas: one program, one page texture, one draw per change.
#[wasm_bindgen]
pub struct SurfaceRenderer {
    gl: Gl,
    program: WebGlProgram,
    vao: WebGlVertexArrayObject,
    bg: WebGlTexture,
    bg_size: [f32; 2],
    surface: Surface,
    center: [f32; 2],
    terms: [bool; 3],
    frames: u32,
    init_ms: f64,
}

#[wasm_bindgen]
impl SurfaceRenderer {
    /// Creates the WebGL2 context on `canvas` and compiles the program. No frame is drawn.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement) -> Result<SurfaceRenderer, JsValue> {
        let t0 = now();
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| err("WebGL2 is not available"))?
            .dyn_into()?;
        let vs = compile(&gl, Gl::VERTEX_SHADER, VERTEX_SHADER)?;
        let fs = compile(&gl, Gl::FRAGMENT_SHADER, FRAGMENT_SHADER)?;
        let program = gl.create_program().ok_or_else(|| err("create_program"))?;
        gl.attach_shader(&program, &vs);
        gl.attach_shader(&program, &fs);
        gl.link_program(&program);
        if gl
            .get_program_parameter(&program, Gl::LINK_STATUS)
            .as_bool()
            != Some(true)
        {
            return Err(err(gl.get_program_info_log(&program).unwrap_or_default()));
        }
        let vao = gl
            .create_vertex_array()
            .ok_or_else(|| err("create_vertex_array"))?;
        let bg = texture(&gl, Gl::LINEAR_MIPMAP_LINEAR, Gl::LINEAR)?;
        let surface = Surface::from_query("");
        Ok(Self {
            gl,
            program,
            vao,
            bg,
            bg_size: [1.0, 1.0],
            surface,
            center: [0.0, 0.0],
            terms: [true; 3],
            frames: 0,
            init_ms: now() - t0,
        })
    }

    /// Milliseconds spent creating the context and compiling the program.
    #[wasm_bindgen(getter)]
    pub fn init_ms(&self) -> f64 {
        self.init_ms
    }

    /// Frames drawn so far (idle check: must not grow without a change).
    #[wasm_bindgen(getter)]
    pub fn frames(&self) -> u32 {
        self.frames
    }

    /// The GPU's own name for itself (unmasked where the browser allows), so a measurement
    /// says whether it ran on a GPU or on software WebGL.
    pub fn backend(&self) -> String {
        const UNMASKED_RENDERER: u32 = 0x9246;
        let gl = &self.gl;
        let unmasked = gl
            .get_extension("WEBGL_debug_renderer_info")
            .ok()
            .flatten()
            .and_then(|_| gl.get_parameter(UNMASKED_RENDERER).ok())
            .and_then(|v| v.as_string());
        unmasked
            .or_else(|| {
                gl.get_parameter(Gl::RENDERER)
                    .ok()
                    .and_then(|v| v.as_string())
            })
            .unwrap_or_default()
    }

    /// Uploads the page (a 2-D canvas, device pixels) and builds its mip chain (the scattered
    /// terms read it). Returns the call time in ms; the copy may finish later.
    pub fn set_background(&mut self, page: &HtmlCanvasElement) -> Result<f64, JsValue> {
        let t0 = now();
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.bg));
        gl.tex_image_2d_with_u32_and_u32_and_html_canvas_element(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA as i32,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            page,
        )?;
        gl.generate_mipmap(Gl::TEXTURE_2D);
        self.bg_size = [page.width() as f32, page.height() as f32];
        Ok(now() - t0)
    }

    /// The surface, from the shared query-string contract (`Surface::from_query`), at device
    /// pixel ratio `dpr`.
    pub fn set_params(&mut self, query: &str, dpr: f32) {
        self.surface = Surface::from_query(query).scaled(dpr);
    }

    /// Canvas width the caller must give the GPU canvas, device px.
    pub fn canvas_width(&self) -> u32 {
        canvas_size(&self.surface).0
    }

    /// Canvas height, device px.
    pub fn canvas_height(&self) -> u32 {
        canvas_size(&self.surface).1
    }

    /// Footprint centre on the page, device px.
    pub fn set_center(&mut self, cx: f32, cy: f32) {
        self.center = [cx, cy];
    }

    /// Canvas top-left on the page (device px, whole pixels); the caller places the canvas
    /// element there, so the shader's page coordinates and the screen agree.
    pub fn origin_x(&self) -> f32 {
        (self.center[0] - self.canvas_width() as f32 / 2.0).floor()
    }

    /// See [`Self::origin_x`].
    pub fn origin_y(&self) -> f32 {
        (self.center[1] - self.canvas_height() as f32 / 2.0).floor()
    }

    /// Ablation switches: refraction, Fresnel rim, specular.
    pub fn set_terms(&mut self, refraction: bool, rim: bool, spec: bool) {
        self.terms = [refraction, rim, spec];
    }

    /// Centre magnification for the green index: `1 / (1 + off'(0))`, the derivative of the
    /// inward offset at the centre (1 on a flat face).
    pub fn magnification(&self) -> f32 {
        let s = &self.surface;
        let e = s.footprint.inradius().max(2.0) * 0.01;
        let a = s.offsets([e, 0.0])[1].min(s.offsets([0.0, e])[1]);
        1.0 / (1.0 + a / e)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        u: &SurfaceUniforms,
        page: [f32; 2],
        view: [u32; 2],
        origin: [f32; 2],
        center: [f32; 2],
        mode: i32,
    ) {
        let gl = &self.gl;
        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        gl.active_texture(Gl::TEXTURE0);
        let l = |n: &str| gl.get_uniform_location(&self.program, n);
        gl.uniform1i(l("u_bg").as_ref(), 0);
        gl.uniform1i(l("u_mode").as_ref(), mode);
        gl.uniform2f(l("u_size").as_ref(), page[0], page[1]);
        gl.uniform2f(l("u_view").as_ref(), view[0] as f32, view[1] as f32);
        gl.uniform2f(l("u_origin").as_ref(), origin[0], origin[1]);
        gl.uniform2f(l("u_center").as_ref(), center[0], center[1]);
        gl.uniform2f(l("u_halfsize").as_ref(), u.halfsize[0], u.halfsize[1]);
        gl.uniform1f(l("u_corner").as_ref(), u.corner);
        gl.uniform1f(l("u_inradius").as_ref(), u.inradius);
        gl.uniform4f(
            l("u_relief").as_ref(),
            u.relief[0],
            u.relief[1],
            u.relief[2],
            u.relief[3],
        );
        gl.uniform1f(l("u_base").as_ref(), u.base);
        gl.uniform3f(l("u_ior").as_ref(), u.ior[0], u.ior[1], u.ior[2]);
        gl.uniform1f(l("u_refraction").as_ref(), u.refraction);
        gl.uniform3f(
            l("u_weights").as_ref(),
            u.weights[0],
            u.weights[1],
            u.weights[2],
        );
        gl.uniform2f(l("u_sigma").as_ref(), u.sigma[0], u.sigma[1]);
        gl.uniform4f(
            l("u_tint").as_ref(),
            u.tint[0],
            u.tint[1],
            u.tint[2],
            u.tint[3],
        );
        gl.uniform1f(l("u_f0").as_ref(), u.f0);
        gl.uniform1f(l("u_rim").as_ref(), u.rim);
        gl.uniform3f(l("u_hv").as_ref(), u.hv[0], u.hv[1], u.hv[2]);
        gl.uniform1f(l("u_shininess").as_ref(), u.shininess);
        gl.uniform1f(l("u_spec").as_ref(), u.spec);
        gl.viewport(0, 0, view[0] as i32, view[1] as i32);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(Gl::COLOR_BUFFER_BIT);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
    }

    /// Draws one frame to the canvas. Call on change only. Returns the elapsed ms; with
    /// `sync` a one-pixel readback waits for the GPU (`gl.finish()` alone is not a fence in
    /// browsers), so the number includes GPU time.
    pub fn render(&mut self, sync: bool) -> f64 {
        let t0 = now();
        let gl = &self.gl;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.bg));
        let u = SurfaceUniforms::new(&self.surface, self.terms);
        let view = [self.canvas_width(), self.canvas_height()];
        self.draw(
            &u,
            self.bg_size,
            view,
            [self.origin_x(), self.origin_y()],
            self.center,
            0,
        );
        if sync {
            let mut px = [0u8; 4];
            let _ = gl.read_pixels_with_opt_u8_array(
                0,
                0,
                1,
                1,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                Some(&mut px),
            );
        }
        self.frames += 1;
        now() - t0
    }

    /// GPU-vs-CPU parity of the current surface. Two checks, returned as JSON text:
    ///
    /// Each part reports `gl_error` (must be 0: a refused draw reads back zeros) and the
    /// float part `nonzero`, so a silent failure cannot pass as agreement.
    ///
    /// * `float`: the shader's own sample positions and coverage (debug modes, RGBA32F
    ///   target) against `Surface::sample_points` / `Surface::coverage`, at full size.
    ///   Per channel, error percentiles over the interior (coverage 1) and the anti-aliased
    ///   edge, and the coverage error over the edge.
    /// * `texel`: the real texture path (mode 0) over a 256² coordinate texture with nearest
    ///   filtering, the surface scaled to fit: exact-texel agreement per channel, direct
    ///   transmission only, no tint, rim or highlight. `off_by_one` counts one-texel
    ///   disagreements, `mismatch` larger ones; `control` the disagreements with a CPU
    ///   reference without dispersion (non-zero when dispersion is on, or the check cannot see
    ///   per-channel errors).
    pub fn parity(&mut self) -> Result<String, JsValue> {
        let float = self.parity_float()?;
        let texel = self.parity_texel()?;
        let gl = &self.gl;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        Ok(format!("{{\"float\":{float},\"texel\":{texel}}}"))
    }

    fn target(
        &self,
        internal: u32,
        w: i32,
        h: i32,
    ) -> Result<(web_sys::WebGlFramebuffer, WebGlTexture), JsValue> {
        let gl = &self.gl;
        let t = texture(gl, Gl::NEAREST, Gl::NEAREST)?;
        gl.tex_storage_2d(Gl::TEXTURE_2D, 1, internal, w, h);
        let fb = gl
            .create_framebuffer()
            .ok_or_else(|| err("create_framebuffer"))?;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(&fb));
        gl.framebuffer_texture_2d(
            Gl::FRAMEBUFFER,
            Gl::COLOR_ATTACHMENT0,
            Gl::TEXTURE_2D,
            Some(&t),
            0,
        );
        if gl.check_framebuffer_status(Gl::FRAMEBUFFER) != Gl::FRAMEBUFFER_COMPLETE {
            return Err(err("framebuffer incomplete"));
        }
        Ok((fb, t))
    }

    fn parity_float(&self) -> Result<String, JsValue> {
        let gl = &self.gl;
        if gl
            .get_extension("EXT_color_buffer_float")
            .ok()
            .flatten()
            .is_none()
        {
            return Ok("{\"error\":\"EXT_color_buffer_float unavailable\"}".into());
        }
        let s = self.surface;
        let (w, h) = canvas_size(&s);
        let (wi, hi) = (w as usize, h as usize);
        let center = [w as f32 / 2.0, h as f32 / 2.0];
        let (fb, t) = self.target(Gl::RGBA32F, w as i32, h as i32)?;
        // the target must not also be the sampled texture (a feedback loop: WebGL refuses
        // the draw and the readback is zeros)
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.bg));
        let u = SurfaceUniforms::new(&s, self.terms);
        let read = |mode: i32| -> Result<Vec<f32>, JsValue> {
            self.draw(&u, [w as f32, h as f32], [w, h], [0.0, 0.0], center, mode);
            let buf = js_sys::Float32Array::new_with_length((wi * hi * 4) as u32);
            gl.read_pixels_with_opt_array_buffer_view(
                0,
                0,
                w as i32,
                h as i32,
                Gl::RGBA,
                Gl::FLOAT,
                Some(&buf),
            )?;
            Ok(buf.to_vec())
        };
        let a = read(1)?;
        let gl_error = gl.get_error();
        let b = read(2)?;
        let nonzero = a.iter().filter(|v| **v != 0.0).count();
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        gl.delete_framebuffer(Some(&fb));
        gl.delete_texture(Some(&t));

        let mut interior: [Vec<f32>; 3] = Default::default();
        let mut edge_pos = Vec::new();
        let mut edge_cov = Vec::new();
        let mut leak = 0usize;
        for row in 0..hi {
            let y = hi - 1 - row; // readback rows are bottom-up
            for x in 0..wi {
                let i = (row * wi + x) * 4;
                let pt = [x as f32 + 0.5, y as f32 + 0.5];
                let p = [pt[0] - center[0], pt[1] - center[1]];
                let cov = s.coverage(p);
                let gcov = b[i + 2];
                if cov == 0.0 {
                    if gcov != 0.0 {
                        leak += 1;
                    }
                    continue;
                }
                let want = s.sample_points(p);
                let got = [[a[i], a[i + 1]], [a[i + 2], a[i + 3]], [b[i], b[i + 1]]];
                let e: Vec<f32> = (0..3)
                    .map(|c| {
                        let off = if u.refraction == 0.0 { [p; 3] } else { want };
                        (got[c][0] - (off[c][0] + center[0]))
                            .hypot(got[c][1] - (off[c][1] + center[1]))
                    })
                    .collect();
                if cov >= 1.0 {
                    for c in 0..3 {
                        interior[c].push(e[c]);
                    }
                } else {
                    edge_pos.push(e[0].max(e[1]).max(e[2]));
                    edge_cov.push((gcov - cov).abs());
                }
            }
        }
        let [r, g, bl] = interior.map(|mut v| stats_json(&ErrorStats::of(&mut v)));
        Ok(format!(
            "{{\"gl_error\":{gl_error},\"nonzero\":{nonzero},\"interior\":{{\"r\":{r},\"g\":{g},\"b\":{bl}}},\"edge_position\":{},\"edge_coverage\":{},\"leak\":{leak}}}",
            stats_json(&ErrorStats::of(&mut edge_pos)),
            stats_json(&ErrorStats::of(&mut edge_cov))
        ))
    }

    fn parity_texel(&self) -> Result<String, JsValue> {
        const N: usize = 256;
        let gl = &self.gl;
        let (cw, ch) = canvas_size(&self.surface);
        let k = (250.0 / cw.max(ch) as f32).min(1.0);
        let s = self.surface.scaled(k);
        let mut coords = vec![0u8; N * N * 4];
        for y in 0..N {
            for x in 0..N {
                let i = (y * N + x) * 4;
                coords[i..i + 4].copy_from_slice(&[x as u8, y as u8, x as u8, 255]);
            }
        }
        let src = texture(gl, Gl::NEAREST, Gl::NEAREST)?;
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA8 as i32,
            N as i32,
            N as i32,
            0,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            Some(&coords),
        )?;
        let (fb, dst) = self.target(Gl::RGBA8, N as i32, N as i32)?;
        let mut u = SurfaceUniforms::new(&s, [true, false, false]);
        u.weights = [1.0, 0.0, 0.0];
        u.tint[3] = 0.0;
        let center = [128.0, 128.0];
        gl.bind_texture(Gl::TEXTURE_2D, Some(&src));
        self.draw(
            &u,
            [N as f32, N as f32],
            [N as u32, N as u32],
            [0.0, 0.0],
            center,
            0,
        );
        let mut out = vec![0u8; N * N * 4];
        let gl_error = gl.get_error();
        gl.read_pixels_with_opt_u8_array(
            0,
            0,
            N as i32,
            N as i32,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            Some(&mut out),
        )?;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        gl.delete_framebuffer(Some(&fb));
        gl.delete_texture(Some(&src));
        gl.delete_texture(Some(&dst));

        let control = Surface {
            dispersion: 0.0,
            ..s
        };
        let (mut inside, mut mismatch, mut off_by_one, mut control_diff) =
            (0usize, 0usize, 0usize, 0usize);
        for row in 0..N {
            let y = N - 1 - row;
            for x in 0..N {
                let i = (row * N + x) * 4;
                if out[i + 3] != 255 {
                    continue; // outside or on the anti-aliased edge: coverage, not position
                }
                inside += 1;
                let p = [x as f32 + 0.5 - center[0], y as f32 + 0.5 - center[1]];
                let texel = |q: [f32; 2]| {
                    [
                        (q[0] + center[0]).floor().clamp(0.0, 255.0),
                        (q[1] + center[1]).floor().clamp(0.0, 255.0),
                    ]
                };
                let error = |c: [[f32; 2]; 3]| {
                    let (r, g, b) = (texel(c[0]), texel(c[1]), texel(c[2]));
                    (f32::from(out[i]) - r[0])
                        .abs()
                        .max((f32::from(out[i + 1]) - g[1]).abs())
                        .max((f32::from(out[i + 2]) - b[0]).abs())
                };
                let e = error(s.sample_points(p));
                if e > 1.0 {
                    mismatch += 1;
                } else if e > 0.0 {
                    off_by_one += 1;
                }
                if error(control.sample_points(p)) > 0.0 {
                    control_diff += 1;
                }
            }
        }
        Ok(format!(
            "{{\"gl_error\":{gl_error},\"scale\":{k:.3},\"inside\":{inside},\"mismatch\":{mismatch},\"off_by_one\":{off_by_one},\"control\":{control_diff}}}"
        ))
    }
}
