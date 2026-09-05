//! The wgpu draw — WebGPU natively, **WebGL2** in the browser.
//!
//! # WebGL2 is a constraint, not a target flag
//!
//! Everything here is deliberately GLES3-core so ONE pipeline serves both
//! backends. Concretely that forbids: storage buffers, compute passes,
//! `textureLoad` on a storage texture, instance step modes beyond the basic
//! two, and non-zero `first_instance`. What it leaves is enough — instanced
//! quads with an SDF in the fragment shader, and an indexed `LineList`.
//!
//! # Why an SDF ring rather than a circle mesh
//!
//! A tessellated circle costs vertices per node and looks polygonal when the
//! camera zooms in — and zoom is the whole point of the field. A quad with a
//! signed-distance ring in the fragment shader is 4 vertices per node at ANY
//! zoom, and the ring's edge stays exactly one pixel soft because the shader
//! derives softness from the fragment's own derivative.
//!
//! # Edges as an index buffer, and what that buys
//!
//! An edge is `LineList` over the node position buffer: its two endpoints ARE
//! two nodes' positions, read by index. So moving a node moves every edge
//! touching it with zero edge work — no CPU geometry rebuild per frame, which
//! is the cost that makes a retained-mode graph view collapse under motion.
//!
//! Arrowheads ride a second instanced pass over the same index pairs, placed
//! at the midpoint and rotated by the edge's own direction, so direction is
//! visible without a per-edge mesh either.

use wgpu::util::DeviceExt as _;

use crate::Scene;

/// The camera + global draw state, mirrored into a uniform buffer.
///
/// `#[repr(C)]` with an explicit tail pad: WebGL2 requires uniform blocks to
/// be 16-byte aligned, and a silently mis-sized struct is the classic
/// "renders on native, blank in the browser" bug.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Camera {
    /// World-space centre the viewport looks at.
    pub center: [f32; 2],
    /// Viewport size in physical pixels.
    pub viewport: [f32; 2],
    /// Layout units per pixel; larger means zoomed out.
    pub scale: f32,
    /// Global alpha multiplier — a fade that costs one uniform write.
    pub fade: f32,
    /// Explicit tail padding to a 16-byte boundary. Named, not implicit.
    pub _pad: [f32; 2],
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            center: [0.0, 0.0],
            viewport: [1280.0, 720.0],
            scale: 1.0,
            fade: 1.0,
            _pad: [0.0, 0.0],
        }
    }
}

impl Camera {
    /// Frame a bounding box with a margin — the fit-to-view a first frame does.
    /// A degenerate box cannot divide by zero: both extents get a floor.
    #[must_use]
    pub fn fit(bounds: (f32, f32, f32, f32), viewport: [f32; 2], margin: f32) -> Self {
        let (x0, y0, x1, y1) = bounds;
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let scale = (w / viewport[0].max(1.0)).max(h / viewport[1].max(1.0)) * margin;
        Camera {
            center: [(x0 + x1) * 0.5, (y0 + y1) * 0.5],
            viewport,
            scale: scale.max(1e-4),
            ..Camera::default()
        }
    }

    /// Screen point → world point. The inverse of what the shader does, and
    /// the function a click needs before it can hit-test.
    #[must_use]
    pub fn to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            (sx - self.viewport[0] * 0.5) * self.scale + self.center[0],
            (sy - self.viewport[1] * 0.5) * self.scale + self.center[1],
        )
    }

    /// Zoom about a screen anchor, keeping the world point under it fixed —
    /// the behaviour a wheel-zoom must have to feel attached to the content.
    pub fn zoom_at(&mut self, sx: f32, sy: f32, factor: f32) {
        let before = self.to_world(sx, sy);
        self.scale = (self.scale / factor).clamp(1e-4, 1e4);
        let after = self.to_world(sx, sy);
        self.center[0] += before.0 - after.0;
        self.center[1] += before.1 - after.1;
    }
}

/// The unit quad every node instance is stamped from — two triangles as a
/// strip, so a node costs 4 vertices and no index buffer.
const QUAD: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]];
/// The unit arrowhead: tip forward, two barbs back.
const ARROW: [[f32; 2]; 3] = [[1.0, 0.0], [-0.6, 0.55], [-0.6, -0.55]];

/// The per-vertex position+alpha lane the edge pass reads. Derived from the
/// scene each frame — the ONE place the CPU touches edge geometry, and it is
/// a copy of two floats per node, not per edge.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EdgeVertex {
    pos: [f32; 2],
    alpha: f32,
    _pad: f32,
}

/// One arrowhead instance: the edge's two endpoints, so the shader can derive
/// direction without the CPU computing an angle per edge per frame.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ArrowInstance {
    /// The edge's source end. Named `tail`/`head` rather than `from`/`to`
    /// because those are reserved words in WGSL — the Rust side follows the
    /// shader's vocabulary so the two never drift apart.
    tail: [f32; 2],
    head: [f32; 2],
    alpha: f32,
    _pad: [f32; 3],
}

/// The GPU-side field: pipelines, buffers, and the one uniform.
pub struct FieldRenderer {
    node_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    arrow_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera_buf: wgpu::Buffer,
    quad_buf: wgpu::Buffer,
    arrow_buf: wgpu::Buffer,
    node_buf: wgpu::Buffer,
    edge_vert_buf: wgpu::Buffer,
    edge_idx_buf: wgpu::Buffer,
    arrow_inst_buf: wgpu::Buffer,
    node_count: u32,
    edge_count: u32,
    node_cap: usize,
    edge_cap: usize,
}

fn vb<'a>(
    stride: u64,
    step: wgpu::VertexStepMode,
    attrs: &'a [wgpu::VertexAttribute],
) -> Option<wgpu::VertexBufferLayout<'a>> {
    // `VertexState::buffers` is a slice of `Option` now — a `None` slot leaves
    // that vertex-buffer index unbound. Every call here binds a real layout, so
    // the wrapping lives in this ONE place instead of at each call site.
    Some(wgpu::VertexBufferLayout {
        array_stride: stride,
        step_mode: step,
        attributes: attrs,
    })
}

impl FieldRenderer {
    /// Build the pipelines and size the buffers for `scene`.
    ///
    /// `format` is the surface's texture format — passed in rather than
    /// assumed, because a canvas-bound WebGL2 surface and a native offscreen
    /// target routinely disagree about it.
    #[must_use]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, scene: &Scene) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("a2ui-graph field"),
            source: wgpu::ShaderSource::Wgsl(include_str!("field.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("field camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("field camera"),
            contents: bytemuck::bytes_of(&Camera::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("field camera"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("field"),
            bind_group_layouts: &[Some(&bgl)],
            // Push constants became "immediates", declared as a size rather than
            // as ranges. This pipeline uses none.
            immediate_size: 0,
        });

        // Straight alpha over the target — the field is drawn back to front
        // (edges, arrows, then nodes) rather than depth-sorted, because a 2-D
        // field has no depth to sort by and a depth buffer would only cost.
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let target = [Some(wgpu::ColorTargetState {
            format,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let quad_attrs = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }];
        let node_attrs = wgpu::vertex_attr_array![1 => Float32x2, 2 => Float32, 3 => Float32, 4 => Uint32, 5 => Uint32];
        let edge_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32];
        let arrow_attrs = wgpu::vertex_attr_array![1 => Float32x2, 2 => Float32x2, 3 => Float32];

        let node_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("field nodes"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_node"),
                compilation_options: Default::default(),
                buffers: &[
                    vb(8, wgpu::VertexStepMode::Vertex, &quad_attrs),
                    vb(
                        std::mem::size_of::<crate::NodeInstance>() as u64,
                        wgpu::VertexStepMode::Instance,
                        &node_attrs,
                    ),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_node"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let edge_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("field edges"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_edge"),
                compilation_options: Default::default(),
                buffers: &[vb(
                    std::mem::size_of::<EdgeVertex>() as u64,
                    wgpu::VertexStepMode::Vertex,
                    &edge_attrs,
                )],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_edge"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let arrow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("field arrows"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_arrow"),
                compilation_options: Default::default(),
                buffers: &[
                    vb(8, wgpu::VertexStepMode::Vertex, &quad_attrs),
                    vb(
                        std::mem::size_of::<ArrowInstance>() as u64,
                        wgpu::VertexStepMode::Instance,
                        &arrow_attrs,
                    ),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_arrow"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let quad_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("unit quad"),
            contents: bytemuck::cast_slice(&QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let arrow_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("unit arrow"),
            contents: bytemuck::cast_slice(&ARROW),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let node_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("node instances"),
            contents: scene.node_bytes(),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let edge_idx_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("edge indices"),
            contents: scene.edge_bytes(),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        let edge_verts = Self::edge_vertices(scene);
        let edge_vert_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("edge vertices"),
            contents: bytemuck::cast_slice(&edge_verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let arrows = Self::arrow_instances(scene);
        let arrow_inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("arrow instances"),
            contents: bytemuck::cast_slice(&arrows),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        FieldRenderer {
            node_pipeline,
            edge_pipeline,
            arrow_pipeline,
            bind_group,
            camera_buf,
            quad_buf,
            arrow_buf,
            node_buf,
            edge_vert_buf,
            edge_idx_buf,
            arrow_inst_buf,
            node_count: scene.nodes.len() as u32,
            edge_count: scene.edges.len() as u32,
            node_cap: scene.nodes.len(),
            edge_cap: scene.edges.len(),
        }
    }
}

impl FieldRenderer {
    /// Node positions + alpha as the edge pass's vertex lane.
    ///
    /// An edge inherits the DIMMER of its two endpoints, so dimming a
    /// selection dims the edges leaving it without the scene tracking edge
    /// alpha at all — n writes, not e writes, and e is the larger number.
    fn edge_vertices(scene: &Scene) -> Vec<EdgeVertex> {
        scene
            .nodes
            .iter()
            .map(|n| EdgeVertex {
                pos: n.pos,
                alpha: n.alpha,
                _pad: 0.0,
            })
            .collect()
    }

    /// One arrowhead per edge, carrying both endpoints so the shader derives
    /// the angle. The CPU never computes a rotation per edge per frame.
    fn arrow_instances(scene: &Scene) -> Vec<ArrowInstance> {
        scene
            .edges
            .iter()
            .map(|e| {
                let (a, b) = (scene.nodes[e.from as usize], scene.nodes[e.to as usize]);
                ArrowInstance {
                    tail: a.pos,
                    head: b.pos,
                    alpha: a.alpha.min(b.alpha),
                    _pad: [0.0; 3],
                }
            })
            .collect()
    }

    /// Push the frame's state to the GPU.
    ///
    /// Three `write_buffer` calls, all sized by the NODE count (the arrow
    /// lane is per edge but written from node positions). No allocation of a
    /// scene, no geometry rebuild — this is the whole per-frame CPU cost.
    ///
    /// # Panics
    /// If the scene grew past the capacity this renderer was built for. A
    /// silent truncation would draw a subset of the graph and look correct.
    pub fn upload(&mut self, queue: &wgpu::Queue, scene: &Scene, camera: &Camera) {
        assert!(
            scene.nodes.len() <= self.node_cap && scene.edges.len() <= self.edge_cap,
            "scene grew past the renderer's buffers ({} nodes / {} edges > {} / {}) — rebuild it",
            scene.nodes.len(),
            scene.edges.len(),
            self.node_cap,
            self.edge_cap
        );
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(camera));
        queue.write_buffer(&self.node_buf, 0, scene.node_bytes());
        queue.write_buffer(
            &self.edge_vert_buf,
            0,
            bytemuck::cast_slice(&Self::edge_vertices(scene)),
        );
        queue.write_buffer(
            &self.arrow_inst_buf,
            0,
            bytemuck::cast_slice(&Self::arrow_instances(scene)),
        );
        self.node_count = scene.nodes.len() as u32;
        self.edge_count = scene.edges.len() as u32;
    }

    /// Record the field into a render pass.
    ///
    /// Order is edges → arrows → nodes and that is deliberate: the rings must
    /// cover the line ends, or every node looks like it has whiskers.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.bind_group, &[]);

        if self.edge_count > 0 {
            pass.set_pipeline(&self.edge_pipeline);
            pass.set_vertex_buffer(0, self.edge_vert_buf.slice(..));
            pass.set_index_buffer(self.edge_idx_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.edge_count * 2, 0, 0..1);

            pass.set_pipeline(&self.arrow_pipeline);
            pass.set_vertex_buffer(0, self.arrow_buf.slice(..));
            pass.set_vertex_buffer(1, self.arrow_inst_buf.slice(..));
            pass.draw(0..3, 0..self.edge_count);
        }

        if self.node_count > 0 {
            pass.set_pipeline(&self.node_pipeline);
            pass.set_vertex_buffer(0, self.quad_buf.slice(..));
            pass.set_vertex_buffer(1, self.node_buf.slice(..));
            pass.draw(0..4, 0..self.node_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_frames_a_box_and_survives_a_degenerate_one() {
        let c = Camera::fit((-100.0, -50.0, 100.0, 50.0), [800.0, 400.0], 1.1);
        assert_eq!(c.center, [0.0, 0.0]);
        assert!(c.scale > 0.0 && c.scale.is_finite());
        // CAN FIRE: a zero-extent box divides by zero without the floor, and
        // an infinite scale renders a blank canvas that looks like "no data".
        let d = Camera::fit((5.0, 5.0, 5.0, 5.0), [800.0, 400.0], 1.1);
        assert!(
            d.scale.is_finite() && d.scale > 0.0,
            "degenerate box broke the camera"
        );
    }

    /// Zoom must keep the world point under the cursor fixed — that is what
    /// makes a wheel feel attached to the content rather than to the viewport.
    #[test]
    fn zoom_holds_the_point_under_the_cursor() {
        let mut c = Camera {
            center: [10.0, -4.0],
            viewport: [800.0, 600.0],
            scale: 2.0,
            ..Camera::default()
        };
        let (sx, sy) = (600.0, 150.0);
        let before = c.to_world(sx, sy);
        c.zoom_at(sx, sy, 1.5);
        let after = c.to_world(sx, sy);
        assert!(
            (before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3,
            "the anchored point drifted: {before:?} -> {after:?}"
        );
        assert!(c.scale < 2.0, "zooming in must reduce units-per-pixel");
    }

    /// An edge takes the DIMMER of its endpoints — so dimming n nodes dims
    /// the e edges for free. CAN FIRE: an implementation that ignored alpha
    /// would leave a dimmed selection's edges at full strength, which is the
    /// grey-mat failure the whole tonal split exists to avoid.
    #[test]
    fn edge_alpha_follows_the_dimmer_endpoint() {
        let buf = crate::scene::tests_support::fixture();
        let abi = crate::GraphAbi::parse(&buf).expect("parses");
        let l = crate::Layout::from_abi(&abi);
        let mut s = Scene::build(&abi, &l);
        s.light(&[0]);
        let verts = FieldRenderer::edge_vertices(&s);
        assert_eq!(verts[0].alpha, crate::scene::LIT_ALPHA);
        assert_eq!(verts[1].alpha, crate::scene::DIM_ALPHA);
        let arrows = FieldRenderer::arrow_instances(&s);
        assert_eq!(
            arrows[0].alpha,
            crate::scene::DIM_ALPHA,
            "edge 0-1 must take node 1's dim, not node 0's lit"
        );
    }

    /// The real thing: build a device, draw the field offscreen, read the
    /// pixels back, and assert something was actually painted.
    ///
    /// Skips GREEN with no adapter (a headless box with neither GPU nor a
    /// software rasterizer), so it never reds CI — the same discipline
    /// `gpu_lut_probe` uses. When it DOES run it is a real falsifier: a
    /// pipeline that compiles but draws nothing produces an untouched clear
    /// colour, and the assertion below fails on exactly that.
    #[test]
    fn the_field_actually_paints_pixels() {
        let instance = wgpu::Instance::default();
        // `request_adapter` returns a `Result` now — "no adapter" carries a
        // reason. The skip stays a skip; only the shape changed.
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("no wgpu adapter — GPU leg skipped (CPU legs above still ran)");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("field test"),
            required_features: wgpu::Features::empty(),
            // The WebGL2 tier, so a pass here means the browser backend
            // is satisfiable too — testing against defaults would prove
            // nothing about the target that actually matters.
            required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            // The old trailing `None` argument, now a field.
            trace: wgpu::Trace::Off,
        }))
        .expect("device");

        let buf = crate::scene::tests_support::fixture();
        let abi = crate::GraphAbi::parse(&buf).expect("parses");
        let mut l = crate::Layout::from_abi(&abi);
        l.settle(80);
        let scene = Scene::build(&abi, &l);

        const W: u32 = 256;
        const H: u32 = 256;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("field target"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());

        let mut r = FieldRenderer::new(&device, format, &scene);
        let cam = Camera::fit(l.bounds().expect("bounds"), [W as f32, H as f32], 1.3);
        r.upload(&queue, &scene, &cam);

        // 256 * 4 = 1024, already a multiple of COPY_BYTES_PER_ROW_ALIGNMENT
        // (256) — chosen so the readback needs no padding arithmetic.
        let bpr = W * 4;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(bpr * H),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("field"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    // A 2-D target has no depth slice to select.
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Pure black clear, so ANY non-black pixel is paint.
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            r.draw(&mut pass);
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(H),
                },
            },
            wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        // `poll` is `PollType` now and REPORTS whether the wait actually
        // completed; `get_mapped_range` is fallible for the same reason. Both
        // are unwrapped rather than ignored — a readback that silently returned
        // an unmapped buffer would make the pixel count below read 0 and blame
        // the pipeline for a mapping failure.
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll");
        let data = slice.get_mapped_range().expect("readback mapped");
        let painted = data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] | p[1] | p[2] != 0)
            .count();

        assert!(
            painted > 200,
            "the field drew {painted} non-black pixels — a pipeline that compiles but paints \
             nothing leaves the clear colour untouched, which is exactly this number being ~0"
        );
        // …and it must not have painted EVERYTHING either: a shader that
        // returns a constant would fill all 65 536 pixels and also "pass" a
        // naive >0 check.
        assert!(
            painted < (W * H) as usize / 2,
            "the field painted {painted} of {} pixels — that is a fill, not a graph",
            W * H
        );
    }
}
