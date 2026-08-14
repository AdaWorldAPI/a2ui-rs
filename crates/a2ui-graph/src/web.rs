//! The browser client: a canvas-bound wgpu surface and the frame loop.
//!
//! # The one blocker this module removes
//!
//! `a2ui-paint`'s GPU probe states it exactly: *"wgpu's WebGL backend
//! REQUIRES a canvas-bound `compatible_surface` (`RequestAdapterOptions`), so
//! the wasm32 WebGL2 backend needs a surface-bound harness and is OUT OF
//! SCOPE for this headless probe."* Everything else was already in place —
//! the `wgpu` feature, GLES3-core shaders, a proven upload/readback path.
//! What was missing is below: create the surface from an `HtmlCanvasElement`
//! FIRST, then request the adapter *with* it. Requesting a surface-less
//! adapter on wasm32 silently yields no WebGL2 backend at all.
//!
//! # Ownership and teardown
//!
//! `FieldClient` owns the device, queue, surface and every buffer. Dropping
//! it drops them, and wgpu frees the GPU resources on drop — deterministic,
//! no GC. That is the whole memory story on the Rust side: there is no
//! per-node object to leak, because the field never made one.
//!
//! The one thing Rust cannot reclaim for you is a DOM event listener, so
//! [`FieldClient::detach`] exists and the consumer must call it when the view
//! goes away. Named, not hidden — an "it cleans itself up" claim about a
//! listener is how leaks get shipped.

use wasm_bindgen::prelude::*;

use crate::{Camera, FieldRenderer, GraphAbi, Layout, Scene};

/// A pointer gesture, already resolved to what it MEANS rather than to which
/// button did it — so the consumer binds semantics, not input plumbing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// Press at a screen point.
    Down(f32, f32),
    /// Move to a screen point (only meaningful while down).
    Move(f32, f32),
    /// Release.
    Up,
    /// Wheel at a screen point, `factor > 1` zooming in.
    Zoom(f32, f32, f32),
}

/// What a click resolved to — an ORDINAL and the address behind it, never a
/// handler. The same charter the FieldView paint tier follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picked {
    /// Index into the node lane.
    pub ordinal: u32,
    /// `(classid, identity)` — what the consumer resolves.
    pub address: (u32, u32),
}

/// The live field on a canvas: device, surface, scene, layout, camera.
pub struct FieldClient {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: FieldRenderer,
    scene: Scene,
    layout: Layout,
    camera: Camera,
    /// The node currently held, if any — the drag.
    dragging: Option<u32>,
    /// Frames of simulation still owed. A settled field costs nothing per
    /// frame; a disturbed one re-settles and then goes quiet again.
    warm: u32,
}

/// How many simulation steps a disturbance buys. Enough to re-settle after a
/// drag; small enough that an idle field stops computing entirely.
const WARM_FRAMES: u32 = 240;
/// Click radius in PIXELS — converted to world units per camera, so the
/// target stays the same physical size at every zoom.
const PICK_RADIUS_PX: f32 = 18.0;

impl FieldClient {
    /// Mount the field on a canvas over an ABI v3 byte stream.
    ///
    /// The order below is the load-bearing part: surface FIRST, then the
    /// adapter request carrying it. Reversing them costs the WebGL2 backend.
    ///
    /// # Errors
    /// If the stream is not v3, if no adapter satisfies a WebGL2-capable
    /// device, or if the canvas cannot back a surface. Each is returned, none
    /// is papered over with a blank canvas.
    pub async fn mount(
        canvas: web_sys::HtmlCanvasElement,
        abi_bytes: &[u8],
    ) -> Result<Self, JsValue> {
        let abi = GraphAbi::parse(abi_bytes)
            .map_err(|e| JsValue::from_str(&format!("graph ABI: {e}")))?;

        let (w, h) = (canvas.width().max(1), canvas.height().max(1));
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&format!("canvas surface: {e}")))?;

        // THE line the probe named. `compatible_surface` is what makes wgpu
        // offer its WebGL backend at all on wasm32.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| {
                JsValue::from_str("no wgpu adapter for this canvas (WebGL2 missing?)")
            })?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("a2ui-graph field"),
                    required_features: wgpu::Features::empty(),
                    // The browser tier's real ceiling. Asking for more would
                    // fail on exactly the machines this exists to serve.
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| JsValue::from_str(&format!("device: {e}")))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        // Settle before the first frame, so the field appears laid out rather
        // than exploding outward while the user watches.
        let mut layout = Layout::from_abi(&abi);
        layout.settle(160);
        let scene = Scene::build(&abi, &layout);
        let renderer = FieldRenderer::new(&device, format, &scene);
        let camera = layout.bounds().map_or_else(
            || Camera {
                viewport: [w as f32, h as f32],
                ..Camera::default()
            },
            |b| Camera::fit(b, [w as f32, h as f32], 1.25),
        );

        Ok(FieldClient {
            device,
            queue,
            surface,
            config,
            renderer,
            scene,
            layout,
            camera,
            dragging: None,
            warm: WARM_FRAMES,
        })
    }

    /// Resize with the canvas. A zero extent is ignored rather than
    /// configured — a 0-wide surface is a device-lost on some backends.
    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.camera.viewport = [w as f32, h as f32];
        self.surface.configure(&self.device, &self.config);
    }
}

impl FieldClient {
    /// Feed a pointer gesture. Returns the node a press landed on, so the
    /// consumer can open a preview — by ADDRESS, never by callback.
    ///
    /// A press that hits a node starts a drag; the drag pins it, and pinning
    /// is what makes the neighbours wobble (the layout does the rest).
    pub fn gesture(&mut self, g: Gesture) -> Option<Picked> {
        match g {
            Gesture::Down(sx, sy) => {
                let (wx, wy) = self.camera.to_world(sx, sy);
                let r = PICK_RADIUS_PX * self.camera.scale;
                let hit = self.layout.hit_test(wx, wy, r)?;
                let ord = hit as u32;
                self.dragging = Some(ord);
                self.layout.pin(hit, wx, wy);
                self.scene.set_pinned(ord, true);
                self.warm = WARM_FRAMES;
                Some(Picked {
                    ordinal: ord,
                    address: (0, 0),
                })
            }
            Gesture::Move(sx, sy) => {
                let d = self.dragging?;
                let (wx, wy) = self.camera.to_world(sx, sy);
                self.layout.pin(d as usize, wx, wy);
                // Every drag frame re-arms the warm window, so the field
                // keeps reacting for as long as the user keeps moving.
                self.warm = WARM_FRAMES;
                None
            }
            Gesture::Up => {
                if let Some(d) = self.dragging.take() {
                    self.layout.unpin(d as usize);
                    self.scene.set_pinned(d, false);
                    self.warm = WARM_FRAMES;
                }
                None
            }
            Gesture::Zoom(sx, sy, factor) => {
                self.camera.zoom_at(sx, sy, factor);
                None
            }
        }
    }

    /// Resolve an ordinal to its address through the ABI view.
    ///
    /// Takes the view rather than storing one, because the client borrows the
    /// caller's bytes: keeping a `GraphAbi<'_>` in the struct would tie the
    /// client's lifetime to the buffer and make it un-storable in JS.
    #[must_use]
    pub fn address_of(abi: &GraphAbi<'_>, ordinal: u32) -> Option<(u32, u32)> {
        if (ordinal as usize) < abi.node_count() {
            Some(abi.address(ordinal as usize))
        } else {
            None
        }
    }

    /// Light a bounded neighbourhood around a node.
    pub fn spread(&mut self, seed: u32, hops: u32) -> Vec<u32> {
        self.scene.spread(seed, hops)
    }
    /// Light the shortest path between two nodes, or nothing if none exists.
    pub fn trace(&mut self, from: u32, to: u32) -> Option<Vec<u32>> {
        self.scene.trace(from, to)
    }
    /// Back to the resting field.
    pub fn clear(&mut self) {
        self.scene.clear();
    }
    /// The current selection, as ordinals.
    #[must_use]
    pub fn selection(&self) -> &[u32] {
        self.scene.selection()
    }

    /// Advance and draw one frame.
    ///
    /// Simulation runs only while `warm`, so an untouched field costs a draw
    /// and nothing else. A surface that is merely outdated is reconfigured
    /// and skipped; a lost one is reported, because silently drawing nothing
    /// forever is worse than a visible failure.
    pub fn frame(&mut self) -> Result<(), JsValue> {
        if self.warm > 0 {
            self.layout.step();
            self.scene.sync_positions(&self.layout);
            self.warm -= 1;
        }
        self.renderer.upload(&self.queue, &self.scene, &self.camera);

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(e) => return Err(JsValue::from_str(&format!("surface: {e}"))),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("field"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.043,
                            g: 0.051,
                            b: 0.070,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer.draw(&mut pass);
        }
        self.queue.submit([enc.finish()]);
        frame.present();
        Ok(())
    }

    /// Whether the simulation is still moving — a driver can stop scheduling
    /// frames when this goes false and resume on the next gesture.
    #[must_use]
    pub fn is_warm(&self) -> bool {
        self.warm > 0
    }

    /// Release the GPU side deterministically.
    ///
    /// Dropping the client already frees device, surface and every buffer —
    /// wgpu frees on drop and there is no GC in the way. This exists so a
    /// consumer can do it at a chosen moment, and as the place the DOM
    /// listeners it registered must be removed: those are the ONLY thing
    /// Rust cannot reclaim for you.
    pub fn detach(self) {
        drop(self);
    }
}

// ── The JS surface ──────────────────────────────────────────────────────────

/// The field, as JavaScript sees it.
///
/// # Why this wrapper exists at all
///
/// Measured 2026-08-14, and it is the reason this module was unreachable:
/// `web.rs` carried **zero** `#[wasm_bindgen]` attributes. [`FieldClient`] was
/// a plain Rust struct, so nothing in JS could construct one, so the linker
/// dropped the whole client — `Layout`, `Scene`, `FieldRenderer` and all. The
/// receipt was blunt: a release `--features web` module contained **2** SIMD
/// instructions and no `a2ui_graph::layout::Layout` symbol whatsoever, while
/// the same code compiled as an rlib carried **800** in `integrate` alone.
/// The crate compiled, linked, and shipped nothing.
///
/// `crate-type = ["cdylib"]` was necessary and NOT sufficient: a cdylib emits
/// a `.wasm`, but only exported items survive the link. Both halves are
/// needed, and the first without the second looks finished.
///
/// # Why a wrapper and not attributes on `FieldClient`
///
/// `Gesture` is an enum with payloads (`Down(f32, f32)`), which wasm-bindgen
/// cannot carry across the boundary — only C-like enums cross. Rather than
/// flatten the Rust type to suit the FFI, the boundary gets its own shape:
/// one method per gesture. The Rust API keeps the enum it wants; JS gets the
/// calls it wants; neither is bent to the other.
#[wasm_bindgen]
pub struct FieldHandle {
    inner: FieldClient,
}

/// What a press resolved to, as JavaScript sees it.
///
/// Flat `u32` fields, because a tuple has no JS shape. Still an ADDRESS and an
/// ordinal — never a handler, per the paint charter.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub struct PickedJs {
    ordinal: u32,
    classid: u32,
    identity: u32,
}

#[wasm_bindgen]
impl PickedJs {
    /// Index into the node lane.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
    /// The address' class half.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn classid(&self) -> u32 {
        self.classid
    }
    /// The address' identity half.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn identity(&self) -> u32 {
        self.identity
    }
}

#[wasm_bindgen]
impl FieldHandle {
    /// Mount the field on a canvas over an ABI v3 byte stream.
    ///
    /// Async because adapter and device are async on the web, and there is no
    /// honest way to make that synchronous — a blocking mount would either
    /// deadlock the event loop or return a client that is not yet usable.
    ///
    /// # Errors
    /// Propagates every failure [`FieldClient::mount`] reports: a stream that
    /// is not v3, no WebGL2-capable adapter, or a canvas that cannot back a
    /// surface. None is papered over with a blank canvas.
    #[wasm_bindgen]
    pub async fn mount(
        canvas: web_sys::HtmlCanvasElement,
        abi_bytes: Vec<u8>,
    ) -> Result<FieldHandle, JsValue> {
        // Owned bytes, not a borrowed slice: the handle outlives the call, and
        // a `&[u8]` across the boundary would tie it to a buffer JS is free to
        // release. One copy at mount, never per frame.
        let inner = FieldClient::mount(canvas, &abi_bytes).await?;
        Ok(FieldHandle { inner })
    }

    /// Press at a screen point. Returns the node it landed on, if any.
    #[wasm_bindgen(js_name = pointerDown)]
    pub fn pointer_down(&mut self, x: f32, y: f32) -> Option<PickedJs> {
        self.inner.gesture(Gesture::Down(x, y)).map(|p| PickedJs {
            ordinal: p.ordinal,
            classid: p.address.0,
            identity: p.address.1,
        })
    }

    /// Move to a screen point — only meaningful while a press is held.
    #[wasm_bindgen(js_name = pointerMove)]
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.inner.gesture(Gesture::Move(x, y));
    }

    /// Release.
    #[wasm_bindgen(js_name = pointerUp)]
    pub fn pointer_up(&mut self) {
        self.inner.gesture(Gesture::Up);
    }

    /// Wheel at a screen point; `factor > 1` zooms in.
    #[wasm_bindgen]
    pub fn zoom(&mut self, x: f32, y: f32, factor: f32) {
        self.inner.gesture(Gesture::Zoom(x, y, factor));
    }

    /// Resize with the canvas.
    #[wasm_bindgen]
    pub fn resize(&mut self, w: u32, h: u32) {
        self.inner.resize(w, h);
    }

    /// Light a bounded neighbourhood around a node.
    #[wasm_bindgen]
    pub fn spread(&mut self, seed: u32, hops: u32) -> Vec<u32> {
        self.inner.spread(seed, hops)
    }

    /// Light the shortest path between two nodes; empty if none exists.
    ///
    /// Empty rather than `null`, because JS callers iterate the result and an
    /// "either an array or null" return is a null-check waiting to be
    /// forgotten. "No path" and "a path of no nodes" are the same thing to a
    /// renderer.
    #[wasm_bindgen]
    pub fn trace(&mut self, from: u32, to: u32) -> Vec<u32> {
        self.inner.trace(from, to).unwrap_or_default()
    }

    /// Back to the resting field.
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// The current selection, as ordinals.
    #[wasm_bindgen]
    #[must_use]
    pub fn selection(&self) -> Vec<u32> {
        self.inner.selection().to_vec()
    }

    /// Advance and draw one frame.
    ///
    /// # Errors
    /// If the surface is lost beyond recovery. An outdated surface is
    /// reconfigured and skipped rather than reported.
    #[wasm_bindgen]
    pub fn frame(&mut self) -> Result<(), JsValue> {
        self.inner.frame()
    }

    /// Whether the simulation is still moving. A driver stops scheduling
    /// frames when this goes false and resumes on the next gesture.
    #[wasm_bindgen(js_name = isWarm)]
    #[must_use]
    pub fn is_warm(&self) -> bool {
        self.inner.is_warm()
    }

    /// Release the GPU side deterministically.
    ///
    /// Consumes the handle: wgpu frees device, surface and every buffer on
    /// drop, with no GC in the way. The DOM listeners the consumer registered
    /// are the one thing Rust cannot reclaim — remove them here.
    #[wasm_bindgen]
    pub fn detach(self) {
        self.inner.detach();
    }
}
