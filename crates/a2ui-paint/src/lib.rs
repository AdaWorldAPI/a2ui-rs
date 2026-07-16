//! `a2ui-paint` — the **paint tier** (charter 3.i): a consumer-agnostic renderer
//! of the addressed fieldview surface.
//!
//! *askama-HTML and wgpu-pixels are two renderers of ONE resolved surface.* This
//! crate takes the SAME `&[FieldView]` / `&[ActionRef]` the askama render
//! consumes (resolved by `a2ui-server`'s `project_node` or the `a2ui-wasm`
//! client's `resolved_fields`/`resolved_actions`) and:
//!
//! 1. **derives a 2-D layout** from each field's `position` (its mask-index
//!    address) and each action's `ordinal` — ADAPTIVELY, per [`Viewport`]
//!    (mobile stacks; desktop uses label/value columns). Layout is a RENDERER
//!    concern (charter T1: a renderer of the shared surface, never a new widget
//!    vocabulary); `position` is a 1-D address, and turning it into pixels is
//!    this crate's job, not stored data.
//! 2. **hit-tests a click** to a [`Hit`] — a field `position` or an action
//!    `ordinal`. A click on an action builds an [`ActionInvoke`](a2ui_core::ActionInvoke)
//!    up-frame carrying only the ordinal ADDRESS (charter T2), never a handler.
//!
//! # SoC — consumer-agnostic by construction
//!
//! The only inputs are OGAR-owned surface types + a viewport. This crate never
//! sees a `ClassView`, a codebook, a `CompiledClass`, or any consumer corpus —
//! ANY resolved surface (synthetic or harvested) paints identically.
//!
//! # The GPU rasterizer (`wgpu` feature)
//!
//! The layout + hit-test core here is pure and testable with nothing running.
//! The `wgpu` feature adds the actual surface draw (WebGPU + WebGL2 via one
//! backend). See [`gpu`]. Per N2 the full pipeline is a follow-up; the paint-
//! DATA path (this module) is what ships first.

#![forbid(unsafe_code)]

use ogar_render_askama::{ActionRef, FieldView};

/// A rectangle in surface pixels: top-left `(x, y)`, size `(w, h)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Does the point `(px, py)` fall inside this rect (half-open on the far
    /// edges, so adjacent rects never both claim a pixel)?
    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// The device class — drives the adaptive layout (mobile stacks label-over-value
/// in one column; desktop uses side-by-side label/value columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    /// Narrow, touch — single column, stacked, larger touch targets.
    Mobile,
    /// Wide, pointer — label/value columns, denser rows.
    Desktop,
}

impl DeviceClass {
    /// Classify by width: below `MOBILE_MAX_WIDTH` is [`Mobile`](DeviceClass::Mobile).
    /// The one relative-sizing knob the operator asked for ("mobile vs PC").
    #[must_use]
    pub fn from_width(width: f32) -> Self {
        if width < Viewport::MOBILE_MAX_WIDTH {
            Self::Mobile
        } else {
            Self::Desktop
        }
    }
}

/// The render viewport — physical size + device class. Relative window sizing +
/// adaptive screen-size detection live here (the layout reads it; nothing is
/// stored on the surface).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Surface width in pixels.
    pub width: f32,
    /// Surface height in pixels.
    pub height: f32,
    /// The device class (drives column vs stacked layout).
    pub device: DeviceClass,
}

impl Viewport {
    /// Widths below this classify as [`DeviceClass::Mobile`].
    pub const MOBILE_MAX_WIDTH: f32 = 640.0;

    /// A viewport of `(width, height)`, device class inferred from width.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            device: DeviceClass::from_width(width),
        }
    }
}

/// The interaction SKIN — one object-projection editor, many skins (the
/// projectional-knowledge-editor thesis). Each skin is a different RENDERER of
/// the SAME resolved `&[FieldView]`/`&[ActionRef]` surface (charter T1: a skin is
/// a render style, never a new widget vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skin {
    /// The form skin — label/value rows (desktop columns; mobile stacked). The
    /// default desktop/detail projection.
    #[default]
    Form,
    /// The flow skin — document-style inline "label: value" runs that wrap at
    /// the viewport width (the Word/prose projection). The first step toward the
    /// projectional document editor.
    Flow,
    // Future skins (each a renderer of the same surface): Grid (spreadsheet),
    // Spatial (CAD), Graph (native). See projectional-knowledge-editor-v1.md.
}

/// A field placed in the layout — its address (`position`) + where its label and
/// value draw. The paint backend draws `label` at `label_rect` and `value` at
/// `value_rect`; a click inside either resolves to this field's `position`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedField {
    /// The field's mask-index address (charter: the layout address).
    pub position: u8,
    /// The display label.
    pub label: String,
    /// The display value.
    pub value: String,
    /// Where the label draws.
    pub label_rect: Rect,
    /// Where the value draws.
    pub value_rect: Rect,
}

impl PlacedField {
    /// The field's whole clickable area (label ∪ value bounding box).
    #[must_use]
    pub fn hit_rect(&self) -> Rect {
        let x = self.label_rect.x.min(self.value_rect.x);
        let y = self.label_rect.y.min(self.value_rect.y);
        let right =
            (self.label_rect.x + self.label_rect.w).max(self.value_rect.x + self.value_rect.w);
        let bottom =
            (self.label_rect.y + self.label_rect.h).max(self.value_rect.y + self.value_rect.h);
        Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        }
    }
}

/// An action placed in the layout — its `ordinal` ADDRESS + button rect + label.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedAction {
    /// The action's ordinal address (the `ActionInvoke` ordinal, charter T2).
    pub ordinal: u32,
    /// The button caption.
    pub label: String,
    /// Where the button draws / is clicked.
    pub rect: Rect,
}

/// The result of hit-testing a click against a [`PaintLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A field at this mask-index position was clicked.
    Field(u8),
    /// An action at this ordinal ADDRESS was clicked — build an
    /// [`ActionInvoke`](a2ui_core::ActionInvoke) with it (charter T2).
    Action(u32),
}

/// A laid-out addressed surface — every field + action placed at pixel rects,
/// ready to draw and to hit-test. Produced by [`layout`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaintLayout {
    /// Placed fields, in surface order.
    pub fields: Vec<PlacedField>,
    /// Placed actions (the button row), in ordinal order.
    pub actions: Vec<PlacedAction>,
    /// Total content height the layout consumed (for scroll extent).
    pub content_height: f32,
}

impl PaintLayout {
    /// Hit-test a click at `(px, py)` — actions take priority (they sit on top of
    /// the field flow), then fields. `None` if the click missed everything.
    #[must_use]
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Hit> {
        for a in &self.actions {
            if a.rect.contains(px, py) {
                return Some(Hit::Action(a.ordinal));
            }
        }
        for f in &self.fields {
            if f.hit_rect().contains(px, py) {
                return Some(Hit::Field(f.position));
            }
        }
        None
    }

    /// Hit-test a click and, if it landed on an ACTION, build the ordinal-
    /// addressed [`ActionInvoke`](a2ui_core::ActionInvoke) up-frame LE bytes for
    /// node `key` (charter T2 — the click sends the address of a behavior, never
    /// a handler). Returns `None` for a miss or a field click (fields edit via a
    /// separate write path, not an action).
    #[must_use]
    pub fn click_to_action_frame(&self, key: [u8; 16], px: f32, py: f32) -> Option<Vec<u8>> {
        match self.hit_test(px, py)? {
            Hit::Action(ordinal) => Some(
                a2ui_core::Frame::ActionInvoke(a2ui_core::ActionInvoke {
                    key,
                    action_ordinal: ordinal,
                    args: Vec::new(),
                })
                .to_le_bytes(),
            ),
            Hit::Field(_) => None,
        }
    }
}

// Layout constants (surface pixels). Deliberately simple + deterministic — a
// real theme would parameterize these; the ADDRESSING (position/ordinal) is what
// matters, not the exact metrics.
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
const DESKTOP_ROW_H: f32 = 28.0;
const DESKTOP_LABEL_W: f32 = 160.0;
const MOBILE_LINE_H: f32 = 24.0;
const ACTION_H: f32 = 32.0;
const ACTION_MIN_W: f32 = 88.0;

/// Lay out an addressed surface for a viewport with the default [`Skin::Form`].
/// See [`layout_with_skin`].
#[must_use]
pub fn layout(fields: &[FieldView], actions: &[ActionRef], vp: &Viewport) -> PaintLayout {
    layout_with_skin(fields, actions, vp, Skin::Form)
}

/// Lay out an addressed surface for a viewport with an explicit [`Skin`] — the
/// paint-DATA path. Turns the 1-D `position`/`ordinal` addresses into 2-D pixel
/// rects; the SKIN chooses the field render style over the SAME resolved surface
/// (Form = label/value rows, adaptive desktop/mobile; Flow = document-style
/// inline wrap). The action row is shared across skins.
///
/// Pure: same `(fields, actions, viewport, skin)` → same layout, testable with
/// nothing running.
#[must_use]
pub fn layout_with_skin(
    fields: &[FieldView],
    actions: &[ActionRef],
    vp: &Viewport,
    skin: Skin,
) -> PaintLayout {
    let (placed_fields, mut y) = match skin {
        Skin::Form => place_form(fields, vp),
        Skin::Flow => place_flow(fields, vp),
    };

    // The action button row, below the fields, laid left-to-right (wrapping to a
    // new row when it would overflow the viewport width).
    y += GAP;
    let mut placed_actions = Vec::with_capacity(actions.len());
    let mut ax = PAD;
    for a in actions {
        let w = ACTION_MIN_W.max(a.label.len() as f32 * 8.0 + 2.0 * GAP);
        if ax + w > vp.width - PAD && ax > PAD {
            ax = PAD;
            y += ACTION_H + GAP;
        }
        placed_actions.push(PlacedAction {
            ordinal: a.ordinal,
            label: a.label.clone(),
            rect: Rect {
                x: ax,
                y,
                w,
                h: ACTION_H,
            },
        });
        ax += w + GAP;
    }
    let content_height = if actions.is_empty() { y } else { y + ACTION_H } + PAD;

    PaintLayout {
        fields: placed_fields,
        actions: placed_actions,
        content_height,
    }
}

/// Form skin — label/value rows, ADAPTIVE: desktop uses side-by-side columns;
/// mobile stacks value below label. Returns the placed fields + the y at the
/// bottom of the field area.
fn place_form(fields: &[FieldView], vp: &Viewport) -> (Vec<PlacedField>, f32) {
    let mut placed = Vec::with_capacity(fields.len());
    let mut y = PAD;
    match vp.device {
        DeviceClass::Desktop => {
            let value_x = PAD + DESKTOP_LABEL_W + GAP;
            let value_w = (vp.width - value_x - PAD).max(0.0);
            for f in fields {
                placed.push(PlacedField {
                    position: f.position,
                    label: f.label.clone(),
                    value: f.value.clone(),
                    label_rect: Rect {
                        x: PAD,
                        y,
                        w: DESKTOP_LABEL_W,
                        h: DESKTOP_ROW_H,
                    },
                    value_rect: Rect {
                        x: value_x,
                        y,
                        w: value_w,
                        h: DESKTOP_ROW_H,
                    },
                });
                y += DESKTOP_ROW_H;
            }
        }
        DeviceClass::Mobile => {
            let w = (vp.width - 2.0 * PAD).max(0.0);
            for f in fields {
                placed.push(PlacedField {
                    position: f.position,
                    label: f.label.clone(),
                    value: f.value.clone(),
                    label_rect: Rect {
                        x: PAD,
                        y,
                        w,
                        h: MOBILE_LINE_H,
                    },
                    value_rect: Rect {
                        x: PAD,
                        y: y + MOBILE_LINE_H,
                        w,
                        h: MOBILE_LINE_H,
                    },
                });
                y += 2.0 * MOBILE_LINE_H;
            }
        }
    }
    (placed, y)
}

/// Flow skin — document-style inline "label value" runs flowing left-to-right,
/// wrapping at the viewport width (the Word/prose projection). Each field's
/// `label_rect` is the label run and `value_rect` the value run just after it,
/// so a click on either still resolves to the field's `position`. The first
/// concrete projectional skin over the shared surface.
fn place_flow(fields: &[FieldView], vp: &Viewport) -> (Vec<PlacedField>, f32) {
    const CH_W: f32 = 8.0; // deterministic per-char advance (a real theme measures glyphs)
    let line_h = DESKTOP_ROW_H;
    let max_x = (vp.width - PAD).max(PAD);
    let mut placed = Vec::with_capacity(fields.len());
    let mut x = PAD;
    let mut y = PAD;
    for f in fields {
        let label_w = (f.label.chars().count() as f32 + 1.0) * CH_W; // "label "
        let value_w = (f.value.chars().count().max(1) as f32) * CH_W;
        let run_w = label_w + value_w + GAP;
        if x + run_w > max_x && x > PAD {
            x = PAD;
            y += line_h;
        }
        placed.push(PlacedField {
            position: f.position,
            label: f.label.clone(),
            value: f.value.clone(),
            label_rect: Rect {
                x,
                y,
                w: label_w,
                h: line_h,
            },
            value_rect: Rect {
                x: x + label_w,
                y,
                w: value_w,
                h: line_h,
            },
        });
        x += run_w + GAP;
    }
    let after = if placed.is_empty() { PAD } else { y + line_h };
    (placed, after)
}

/// The GPU rasterization backend (WebGPU / WebGL2 via wgpu). OFF by default —
/// the layout + hit-test core above is backend-agnostic and ships first; this
/// wires the actual draw. Headless render-to-texture today (enough to prove the
/// pipeline: buffer → shader → render pass → texture); windowed surface
/// presentation (the only place surface `unsafe` would live — kept OUT of this
/// `#![forbid(unsafe_code)]` crate) is a later addition.
///
/// The layout is already fully positioned by [`layout`]/[`layout_with_skin`];
/// this backend only rasterizes its rects. Glyph/text raster is a follow-up (a
/// textured quad over the same rects); the ADDRESSING (position/ordinal → rect)
/// is what the pipeline proves.
#[cfg(feature = "wgpu")]
pub mod gpu {
    use wgpu::util::DeviceExt;

    use super::{PaintLayout, Rect, Viewport};

    /// Clip-space passthrough + a solid fill — every field/action rect is drawn
    /// as filled geometry at its addressed position.
    const SHADER: &str = r#"
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.55, 0.95, 1.0);
}
"#;

    /// The offscreen target texture format.
    pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    /// The clear colour a paint surface starts from before drawing the layout.
    #[must_use]
    pub fn clear_color() -> wgpu::Color {
        wgpu::Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }
    }

    /// Convert every rect in a laid-out surface — each field's `label_rect` and
    /// `value_rect`, each action's `rect` — into a clip-space (NDC) triangle
    /// list: 6 vertices (two triangles) per rect, fields first then actions.
    /// This is the pure geometry the GPU uploads; it is unit-tested without a
    /// GPU so the rasterizer stays a thin consumer.
    ///
    /// Device pixels (top-left origin, y-down) map to NDC (`[-1, 1]`, y-up):
    /// `ndc_x = 2·x/W − 1`, `ndc_y = 1 − 2·y/H`.
    #[must_use]
    pub fn to_ndc_vertices(layout: &PaintLayout, vp: &Viewport) -> Vec<[f32; 2]> {
        let w = vp.width.max(1.0);
        let h = vp.height.max(1.0);
        let ndc = |x: f32, y: f32| [2.0 * x / w - 1.0, 1.0 - 2.0 * y / h];
        let mut out = Vec::with_capacity((layout.fields.len() * 2 + layout.actions.len()) * 6);
        let mut push = |r: &Rect| {
            let tl = ndc(r.x, r.y);
            let tr = ndc(r.x + r.w, r.y);
            let bl = ndc(r.x, r.y + r.h);
            let br = ndc(r.x + r.w, r.y + r.h);
            // Triangle 1: TL, TR, BL. Triangle 2: TR, BR, BL.
            out.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        };
        for f in &layout.fields {
            push(&f.label_rect);
            push(&f.value_rect);
        }
        for a in &layout.actions {
            push(&a.rect);
        }
        out
    }

    /// Flatten clip-space vertices to native-endian bytes for the vertex-buffer
    /// upload — no `unsafe`, so the crate's `#![forbid(unsafe_code)]` holds
    /// (bytemuck/pointer casts are avoided for one small buffer).
    fn vertex_bytes(verts: &[[f32; 2]]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(verts.len() * 8);
        for v in verts {
            bytes.extend_from_slice(&v[0].to_ne_bytes());
            bytes.extend_from_slice(&v[1].to_ne_bytes());
        }
        bytes
    }

    /// A headless GPU painter: an owned device/queue + the quad-fill pipeline.
    /// Consumes a [`PaintLayout`] (the addressed rects) and draws it into an
    /// offscreen texture — no window, no surface.
    pub struct GpuPainter {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
    }

    impl GpuPainter {
        /// Create a headless painter. Returns `None` if no adapter is available
        /// (a box with neither GPU nor software rasterizer). Accepts the
        /// fallback adapter so it works without real hardware.
        pub async fn new() -> Option<Self> {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await?;
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("a2ui-paint device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::downlevel_defaults(),
                        memory_hints: wgpu::MemoryHints::default(),
                    },
                    None,
                )
                .await
                .ok()?;

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("a2ui-paint quad shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("a2ui-paint layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("a2ui-paint pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TARGET_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

            Some(Self {
                device,
                queue,
                pipeline,
            })
        }

        /// Rasterize a laid-out surface into a fresh offscreen texture sized to
        /// the viewport, over `clear`. Returns the texture; a windowed consumer
        /// presents it, a test consumer copies it back.
        #[must_use]
        pub fn render_to_texture(
            &self,
            layout: &PaintLayout,
            vp: &Viewport,
            clear: wgpu::Color,
        ) -> wgpu::Texture {
            let verts = to_ndc_vertices(layout, vp);
            let bytes = vertex_bytes(&verts);
            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("a2ui-paint vertices"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                });

            let target = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("a2ui-paint target"),
                size: wgpu::Extent3d {
                    width: vp.width.max(1.0) as u32,
                    height: vp.height.max(1.0) as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TARGET_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("a2ui-paint encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("a2ui-paint pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..(verts.len() as u32), 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            target
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Vec<FieldView> {
        vec![
            FieldView {
                position: 0,
                label: "Total".into(),
                predicate: "amount_total".into(),
                value: "42".into(),
            },
            FieldView {
                position: 2,
                label: "Partner".into(),
                predicate: "partner_id".into(),
                value: "7".into(),
            },
        ]
    }
    fn actions() -> Vec<ActionRef> {
        vec![
            ActionRef {
                ordinal: 0,
                label: "View".into(),
            },
            ActionRef {
                ordinal: 1,
                label: "Post".into(),
            },
        ]
    }

    #[test]
    fn desktop_lays_out_label_value_columns_and_hit_tests_by_address() {
        let vp = Viewport::new(1000.0, 800.0);
        assert_eq!(vp.device, DeviceClass::Desktop);
        let lay = layout(&fields(), &actions(), &vp);

        // Two fields, each a row; the value column is to the right of the label.
        assert_eq!(lay.fields.len(), 2);
        assert_eq!(lay.fields[0].position, 0);
        assert!(lay.fields[0].value_rect.x > lay.fields[0].label_rect.x);
        // A click in field 0's value cell resolves to its position ADDRESS.
        let vr = lay.fields[0].value_rect;
        assert_eq!(
            lay.hit_test(vr.x + 2.0, vr.y + 2.0),
            Some(Hit::Field(0)),
            "value cell → field 0"
        );
        // A click on the second action button resolves to ordinal 1.
        let a1 = &lay.actions[1];
        assert_eq!(
            lay.hit_test(a1.rect.x + 1.0, a1.rect.y + 1.0),
            Some(Hit::Action(1))
        );
        // A miss returns None.
        assert_eq!(lay.hit_test(-5.0, -5.0), None);
    }

    #[test]
    fn mobile_stacks_value_below_label() {
        let vp = Viewport::new(360.0, 720.0);
        assert_eq!(vp.device, DeviceClass::Mobile);
        let lay = layout(&fields(), &actions(), &vp);
        // Value is stacked BELOW the label (same column), not to the right.
        assert_eq!(lay.fields[0].value_rect.x, lay.fields[0].label_rect.x);
        assert!(lay.fields[0].value_rect.y > lay.fields[0].label_rect.y);
    }

    #[test]
    fn action_click_builds_the_ordinal_addressed_up_frame() {
        let vp = Viewport::new(1000.0, 800.0);
        let lay = layout(&fields(), &actions(), &vp);
        let key = [9u8; 16];
        let a1 = &lay.actions[1];
        let bytes = lay
            .click_to_action_frame(key, a1.rect.x + 1.0, a1.rect.y + 1.0)
            .expect("action hit builds a frame");
        // Round-trips to an ActionInvoke carrying only the ordinal address (T2).
        match a2ui_core::Frame::from_le_bytes(&bytes).unwrap() {
            a2ui_core::Frame::ActionInvoke(ai) => {
                assert_eq!(ai.key, key);
                assert_eq!(ai.action_ordinal, 1);
                assert!(ai.args.is_empty(), "no handler on the wire");
            }
            a2ui_core::Frame::NodeDelta(_) => panic!("expected ActionInvoke"),
        }
        // A field click does NOT build an action frame (fields edit separately).
        let vr = lay.fields[0].value_rect;
        assert!(
            lay.click_to_action_frame(key, vr.x + 2.0, vr.y + 2.0)
                .is_none()
        );
    }

    #[test]
    fn flow_skin_lays_fields_inline_over_the_same_surface() {
        // The projectional thesis: one resolved surface, two skins. Form stacks
        // label/value rows; Flow flows "label value" runs inline (the document
        // projection). Same fields/actions, different render — never a new
        // vocabulary (T1).
        let vp = Viewport::new(1000.0, 800.0);
        let form = layout_with_skin(&fields(), &actions(), &vp, Skin::Form);
        let flow = layout_with_skin(&fields(), &actions(), &vp, Skin::Flow);
        assert_eq!(flow.fields.len(), 2);
        assert_eq!(flow.actions.len(), 2);
        // Flow: the two short fields sit on ONE line (same y). Form stacks them.
        assert_eq!(
            flow.fields[0].label_rect.y, flow.fields[1].label_rect.y,
            "flow is inline"
        );
        assert_ne!(
            form.fields[0].label_rect.y, form.fields[1].label_rect.y,
            "form stacks rows"
        );
        // The value run sits right after the label run (inline document flow).
        assert!(
            flow.fields[0].value_rect.x
                >= flow.fields[0].label_rect.x + flow.fields[0].label_rect.w - 0.01
        );
        // Hit-testing still resolves by ADDRESS in the flow skin.
        let vr = flow.fields[1].value_rect;
        assert_eq!(flow.hit_test(vr.x + 1.0, vr.y + 1.0), Some(Hit::Field(2)));
        // The bare `layout` is the Form skin (back-compat).
        assert_eq!(layout(&fields(), &actions(), &vp), form);
    }

    #[test]
    fn empty_surface_lays_out_without_panic() {
        let vp = Viewport::new(800.0, 600.0);
        let lay = layout(&[], &[], &vp);
        assert!(lay.fields.is_empty() && lay.actions.is_empty());
        assert_eq!(lay.hit_test(10.0, 10.0), None);
    }

    #[cfg(feature = "wgpu")]
    #[test]
    fn ndc_vertices_are_six_per_rect_in_clip_space() {
        // The GPU backend's geometry is pure (no device needed): every rect the
        // layout placed becomes two triangles = 6 clip-space vertices.
        let vp = Viewport::new(1000.0, 800.0);
        let lay = layout(&fields(), &actions(), &vp);
        let verts = gpu::to_ndc_vertices(&lay, &vp);
        // Each field contributes label_rect + value_rect; each action one rect.
        let rects = lay.fields.len() * 2 + lay.actions.len();
        assert_eq!(verts.len(), rects * 6, "6 vertices per placed rect");
        // Everything the layout placed inside the viewport maps into the clip cube.
        for v in &verts {
            assert!(
                (-1.0..=1.0).contains(&v[0]) && (-1.0..=1.0).contains(&v[1]),
                "vertex {v:?} outside NDC"
            );
        }
    }
}
