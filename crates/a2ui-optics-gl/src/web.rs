//! Raw WebGL2 driver for the raindrop (wasm32 only).

use a2ui_paint::optics::{Droplet, KeyLight, Medium};
use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

use crate::{DropUniforms, FRAGMENT_SHADER, VERTEX_SHADER, cpu_sample_points};

fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

fn err(msg: impl Into<String>) -> JsValue {
    JsValue::from_str(&msg.into())
}

struct Locations {
    bg: Option<WebGlUniformLocation>,
    size: Option<WebGlUniformLocation>,
    view: Option<WebGlUniformLocation>,
    origin: Option<WebGlUniformLocation>,
    center: Option<WebGlUniformLocation>,
    radius: Option<WebGlUniformLocation>,
    sphere: Option<WebGlUniformLocation>,
    height: Option<WebGlUniformLocation>,
    ior: Option<WebGlUniformLocation>,
    refraction: Option<WebGlUniformLocation>,
    f0: Option<WebGlUniformLocation>,
    rim: Option<WebGlUniformLocation>,
    half: Option<WebGlUniformLocation>,
    shininess: Option<WebGlUniformLocation>,
    spec: Option<WebGlUniformLocation>,
}

/// The raindrop renderer: one program, one background texture, one draw per change.
#[wasm_bindgen]
pub struct DropRenderer {
    gl: Gl,
    program: WebGlProgram,
    vao: WebGlVertexArrayObject,
    loc: Locations,
    bg: WebGlTexture,
    bg_size: [f32; 2],
    center: [f32; 2],
    droplet: Droplet,
    medium: Medium,
    dispersion: f32,
    light: KeyLight,
    terms: [bool; 3],
    init_ms: f64,
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

fn texture(gl: &Gl, filter: u32) -> Result<WebGlTexture, JsValue> {
    let t = gl.create_texture().ok_or_else(|| err("create_texture"))?;
    gl.bind_texture(Gl::TEXTURE_2D, Some(&t));
    for (p, v) in [
        (Gl::TEXTURE_MIN_FILTER, filter),
        (Gl::TEXTURE_MAG_FILTER, filter),
        (Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE),
        (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
    ] {
        gl.tex_parameteri(Gl::TEXTURE_2D, p, v as i32);
    }
    Ok(t)
}

#[wasm_bindgen]
impl DropRenderer {
    /// Creates the WebGL2 context on `canvas` and compiles the program. No frame is drawn.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement) -> Result<DropRenderer, JsValue> {
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
        let u = |n: &str| gl.get_uniform_location(&program, n);
        let loc = Locations {
            bg: u("u_bg"),
            size: u("u_size"),
            view: u("u_view"),
            origin: u("u_origin"),
            center: u("u_center"),
            radius: u("u_radius"),
            sphere: u("u_sphere"),
            height: u("u_height"),
            ior: u("u_ior"),
            refraction: u("u_refraction"),
            f0: u("u_f0"),
            rim: u("u_rim"),
            half: u("u_half"),
            shininess: u("u_shininess"),
            spec: u("u_spec"),
        };
        let vao = gl
            .create_vertex_array()
            .ok_or_else(|| err("create_vertex_array"))?;
        let bg = texture(&gl, Gl::LINEAR)?;
        let droplet = Droplet {
            radius: 90.0,
            height: 76.5,
        };
        Ok(Self {
            gl,
            program,
            vao,
            loc,
            bg,
            bg_size: [1.0, 1.0],
            center: [0.0, 0.0],
            droplet,
            medium: Medium::WATER,
            dispersion: 0.0,
            light: KeyLight::DEFAULT,
            terms: [true; 3],
            init_ms: now() - t0,
        })
    }

    /// Milliseconds spent creating the context and compiling the program.
    #[wasm_bindgen(getter)]
    pub fn init_ms(&self) -> f64 {
        self.init_ms
    }

    /// Uploads the page (a 2-D canvas the caller drew the text into, device pixels).
    /// Returns the upload time in milliseconds (including a `finish`, so the GPU copy counts).
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
        gl.finish();
        self.bg_size = [page.width() as f32, page.height() as f32];
        Ok(now() - t0)
    }

    /// Drop centre in device pixels, top-left origin of the page.
    pub fn set_drop(&mut self, cx: f32, cy: f32) {
        self.center = [cx, cy];
    }

    /// Where the canvas's top-left sits on the page (device pixels): `centre − buffer/2`.
    /// The caller positions the canvas element there, so the two agree by construction.
    pub fn origin_x(&self) -> f32 {
        self.center[0] - self.gl.drawing_buffer_width() as f32 / 2.0
    }

    /// See [`Self::origin_x`].
    pub fn origin_y(&self) -> f32 {
        self.center[1] - self.gl.drawing_buffer_height() as f32 / 2.0
    }

    /// Footprint radius (device pixels) and curvature (apex height / radius, 0..1].
    pub fn set_shape(&mut self, radius: f32, curvature: f32) {
        let radius = radius.max(1.0);
        self.droplet = Droplet {
            radius,
            height: radius * curvature.clamp(0.01, 1.0),
        };
    }

    /// `water`, `glass` or `diamond`; dispersion 0 = off, 1 = physical.
    pub fn set_medium(&mut self, name: &str, dispersion: f32) {
        self.medium = match name {
            "glass" => Medium::CROWN_GLASS,
            "diamond" => Medium::DIAMOND,
            _ => Medium::WATER,
        };
        self.dispersion = dispersion.max(0.0);
    }

    /// Key-light azimuth in degrees.
    pub fn set_light(&mut self, angle_deg: f32) {
        self.light.angle_deg = angle_deg;
    }

    /// Ablation switches: refraction, Fresnel rim, specular.
    pub fn set_terms(&mut self, refraction: bool, rim: bool, spec: bool) {
        self.terms = [refraction, rim, spec];
    }

    /// Centre magnification of the current drop for the green index.
    pub fn magnification(&self) -> f32 {
        self.droplet
            .centre_magnification(self.medium.channel_iors(self.dispersion)[1])
    }

    fn uniforms(&self) -> DropUniforms {
        DropUniforms::new(
            self.center,
            self.droplet,
            self.medium,
            self.dispersion,
            self.light,
            self.terms,
        )
    }

    /// Draws the drop into a `view`-sized target whose top-left sits at `origin` on a page of
    /// `page` size (all device pixels).
    fn draw(&self, u: &DropUniforms, page: [f32; 2], view: [f32; 2], origin: [f32; 2]) {
        let gl = &self.gl;
        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        gl.active_texture(Gl::TEXTURE0);
        let l = &self.loc;
        gl.uniform1i(l.bg.as_ref(), 0);
        gl.uniform2f(l.size.as_ref(), page[0], page[1]);
        gl.uniform2f(l.view.as_ref(), view[0], view[1]);
        gl.uniform2f(l.origin.as_ref(), origin[0], origin[1]);
        gl.uniform2f(l.center.as_ref(), u.center[0], u.center[1]);
        gl.uniform1f(l.radius.as_ref(), u.radius);
        gl.uniform1f(l.sphere.as_ref(), u.sphere);
        gl.uniform1f(l.height.as_ref(), u.height);
        gl.uniform3f(l.ior.as_ref(), u.ior[0], u.ior[1], u.ior[2]);
        gl.uniform1f(l.refraction.as_ref(), u.refraction);
        gl.uniform1f(l.f0.as_ref(), u.f0);
        gl.uniform1f(l.rim.as_ref(), u.rim);
        gl.uniform3f(l.half.as_ref(), u.half[0], u.half[1], u.half[2]);
        gl.uniform1f(l.shininess.as_ref(), u.shininess);
        gl.uniform1f(l.spec.as_ref(), u.spec);
        gl.viewport(0, 0, view[0] as i32, view[1] as i32);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(Gl::COLOR_BUFFER_BIT);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
    }

    /// Draws one frame to the canvas. Call on change only. Returns the elapsed ms; with
    /// `sync` the call reads one pixel back, which waits for the GPU to finish the frame
    /// (`gl.finish()` alone is not a reliable fence in browsers), so the number is GPU time.
    pub fn render(&self, sync: bool) -> f64 {
        let t0 = now();
        let gl = &self.gl;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.bg));
        let view = [
            gl.drawing_buffer_width() as f32,
            gl.drawing_buffer_height() as f32,
        ];
        self.draw(
            &self.uniforms(),
            self.bg_size,
            view,
            [self.origin_x(), self.origin_y()],
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
        now() - t0
    }

    /// GPU-vs-CPU parity. Renders the drop over a coordinate texture (R = x, G = y, B = x,
    /// nearest filtering, 256 × 256) into an offscreen framebuffer, reads it back, and
    /// compares each channel's sampled coordinate with [`cpu_sample_points`]. Returns
    /// `{"pixels":n,"inside":n,"edge":n,"leak":n,"max_err_px":e,"mismatch":n,"control_mismatch":n}`
    /// (`leak`: pixels drawn outside the footprint or left empty inside it) as JSON
    /// text; a mismatch is a pixel whose GPU coordinate differs from the CPU one by more than
    /// one texel, `off_by_one` one that differs by exactly one (a float landing on the other
    /// side of a texel edge; a pass is 0 mismatch and a small off_by_one share — a shader that
    /// used the green index for red showed 0 mismatch and only off-by-one errors, so both
    /// numbers are needed). `control_mismatch` (any texel difference) compares the GPU against a CPU reference WITHOUT
    /// dispersion (green index for all channels): with dispersion on it must be non-zero, or
    /// the check cannot see a shader that ignores the per-channel index.
    /// Rim and highlight are switched off for the check (they change colour, not position).
    pub fn parity(&mut self) -> Result<String, JsValue> {
        const N: usize = 256;
        let gl = &self.gl;
        let mut coords = vec![0u8; N * N * 4];
        for y in 0..N {
            for x in 0..N {
                let i = (y * N + x) * 4;
                coords[i..i + 4].copy_from_slice(&[x as u8, y as u8, x as u8, 255]);
            }
        }
        let src = texture(gl, Gl::NEAREST)?;
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
        let dst = texture(gl, Gl::NEAREST)?;
        gl.tex_storage_2d(Gl::TEXTURE_2D, 1, Gl::RGBA8, N as i32, N as i32);
        let fb = gl
            .create_framebuffer()
            .ok_or_else(|| err("create_framebuffer"))?;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(&fb));
        gl.framebuffer_texture_2d(
            Gl::FRAMEBUFFER,
            Gl::COLOR_ATTACHMENT0,
            Gl::TEXTURE_2D,
            Some(&dst),
            0,
        );

        // a drop centred in the test texture, large enough to reach its steep edge
        let droplet = Droplet {
            radius: 100.0,
            height: self.droplet.height / self.droplet.radius * 100.0,
        };
        let terms = [true, false, false];
        let u = DropUniforms::new(
            [128.0, 128.0],
            droplet,
            self.medium,
            self.dispersion,
            self.light,
            terms,
        );
        gl.bind_texture(Gl::TEXTURE_2D, Some(&src));
        self.draw(&u, [N as f32, N as f32], [N as f32, N as f32], [0.0, 0.0]);
        let mut out = vec![0u8; N * N * 4];
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

        let control = DropUniforms {
            ior: [u.ior[1]; 3],
            ..u
        };
        let (mut inside, mut mismatch, mut control_mismatch, mut max_err) =
            (0usize, 0usize, 0usize, 0f32);
        let (mut edge, mut leak, mut off_by_one) = (0usize, 0usize, 0usize);
        for row in 0..N {
            // read_pixels rows are bottom-up; top-left y of this row:
            let y = N - 1 - row;
            for x in 0..N {
                let i = (row * N + x) * 4;
                let pt = [x as f32 + 0.5, y as f32 + 0.5];
                let cpu = cpu_sample_points(&u, droplet, pt);
                let dist = ((pt[0] - 128.0).powi(2) + (pt[1] - 128.0).powi(2)).sqrt();
                match out[i + 3] {
                    // outside the footprint nothing is drawn: the page shows through
                    0 => {
                        if dist < droplet.radius - 0.5 {
                            leak += 1;
                        }
                        continue;
                    }
                    255 => inside += 1,
                    // the anti-aliased contact line: coverage, not position
                    _ => {
                        edge += 1;
                        continue;
                    }
                }
                if dist > droplet.radius + 0.5 {
                    leak += 1;
                }
                // nearest sampling returns the texel containing the sample point
                let want = |p: [f32; 2]| {
                    [
                        p[0].floor().clamp(0.0, 255.0),
                        p[1].floor().clamp(0.0, 255.0),
                    ]
                };
                let error = |c: [[f32; 2]; 3]| {
                    let (r, g, b) = (want(c[0]), want(c[1]), want(c[2]));
                    (out[i] as f32 - r[0])
                        .abs()
                        .max((out[i + 1] as f32 - g[1]).abs())
                        .max((out[i + 2] as f32 - b[0]).abs())
                };
                let e = error(cpu);
                max_err = max_err.max(e);
                if e > 1.0 {
                    mismatch += 1;
                } else if e > 0.0 {
                    off_by_one += 1;
                }
                if error(cpu_sample_points(&control, droplet, pt)) > 0.0 {
                    control_mismatch += 1;
                }
            }
        }
        Ok(format!(
            "{{\"pixels\":{},\"inside\":{},\"edge\":{},\"leak\":{},\"max_err_px\":{},\"mismatch\":{},\"off_by_one\":{},\"control_mismatch\":{}}}",
            N * N,
            inside,
            edge,
            leak,
            max_err,
            mismatch,
            off_by_one,
            control_mismatch
        ))
    }
}
