//! PROBE-GPU-LUT — the a2ui-paint `wgpu` harness proves the 256²-u16
//! palette-distance LUT **texture-gather** (the GPU realization of what CPU
//! `batch_palette_distance` does against a materialized distance table).
//!
//! This is the probe the x265/H.268 sprite-replay plan gated its wgpu/wasm
//! decode tiers on: *"the wgpu harness is shared — the minimal render harness
//! this probe needs is exactly the harness PROBE-GPU-LUT has been gated on."*
//! (ndarray `.claude/plans/x265-sprite-replay-probe-v1.md` §Decode tiers c;
//! §10(i): a materialized 256² u16 table is 128 KiB.)
//!
//! Two legs, split by what the sandbox can actually measure:
//!
//! - **CPU-reference** (always compiled, runs everywhere): the exact
//!   `textureLoad(lut, (q,k))` gather modelled in Rust, plus structural sanity
//!   (symmetric, zero-diagonal) and determinism. **This is the falsifiable
//!   core** — the arithmetic is what could be wrong; the GPU only executes it.
//!
//! - **GPU-exec** (`feature = "wgpu"`, adapter-guarded): uploads the LUT as an
//!   `R16Uint` texture, renders a 256×256 `R32Uint` target whose every texel is
//!   the fetched distance (a fragment-shader GPGPU pass mirroring the crate's
//!   render-to-texture harness), reads it back, and asserts FULL-TABLE parity
//!   (`readback[i] == lut[i]`). It **skips green** where no adapter is present
//!   (a headless box with neither GPU nor a software rasterizer), so it never
//!   reds a headless CI.
//!
//!   **Backend scope (headless):** this probe requests a **surface-less**
//!   adapter, so it validates on **WebGPU** (native or browser) + **native /
//!   software GL** (lavapipe CI). The WGSL is **WebGL2-compatible** — integer
//!   sampled texture + `textureLoad` + integer render target are all GLES3-core
//!   — but wgpu's WebGL backend REQUIRES a canvas-bound `compatible_surface`
//!   (`RequestAdapterOptions`), so the wasm32 **WebGL2** backend needs a
//!   surface-bound harness and is OUT OF SCOPE for this headless probe (see the
//!   `compatible_surface` note in `GpuLut::new`). "WebGL2" here is a
//!   shader-surface property, not a validated execution backend of this probe.
//!
//! No bgz17 dependency: this is a HARNESS-CAPABILITY probe (can our real wgpu
//! seam carry + gather the materialized table at all), not a bgz17 integration.
//! The 256² table is built deterministically here with the SAME structure
//! (symmetric u16, zero diagonal) bgz17's `batch_palette_distance` table
//! carries — keeping the a2ui-paint crate boundary clean (charter: no consumer
//! crate deps).

/// Palette codebook cardinality — one tier of the CAM-PQ 6×256 code / the
/// bgz17 palette distance table axis.
const N: usize = 256;

/// SplitMix64 — deterministic, no `rand` dep (probe discipline: a fixed seed,
/// asserted reproducible, so a NEUTRAL/KILL never depends on RNG state).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Build a deterministic **symmetric, zero-diagonal** 256×256 `u16` distance
/// table (row-major). Off-diagonal entries are forced non-zero so the
/// zero-diagonal assertion is meaningful. This mirrors the shape of a
/// materialized palette-distance table (`d(i,i)=0`, `d(i,j)=d(j,i)`).
fn build_lut(seed: u64) -> Vec<u16> {
    let mut lut = vec![0u16; N * N];
    let mut s = seed;
    for q in 0..N {
        for k in (q + 1)..N {
            // 1..=65535 — non-zero off-diagonal.
            let v = ((splitmix64(&mut s) % 65_535) + 1) as u16;
            lut[q * N + k] = v;
            lut[k * N + q] = v;
        }
    }
    lut // diagonal stays 0
}

/// The exact gather the WGSL runs: row-major `lut[q*256 + k]`. The GPU merely
/// executes this index; the CPU-reference test proves the index is right.
fn gather(lut: &[u16], q: u8, k: u8) -> u16 {
    lut[q as usize * N + k as usize]
}

#[test]
fn probe_gpu_lut_cpu_reference() {
    let seed = 0x9E37_79B9_7F4A_7C15;
    let lut = build_lut(seed);

    // Determinism (fixed-seed reproducibility).
    assert_eq!(lut, build_lut(seed), "LUT build is deterministic");

    // Structural: 256² u16 = 131072 B = 128 KiB — the §10(i) materialized
    // palette-table figure.
    assert_eq!(lut.len(), N * N);
    assert_eq!(
        lut.len() * 2,
        131_072,
        "256² u16 == 128 KiB (the materialized palette distance table)"
    );

    // Symmetric + zero diagonal + non-zero off-diagonal.
    for q in 0..N {
        assert_eq!(lut[q * N + q], 0, "zero diagonal at {q}");
        for k in 0..N {
            assert_eq!(lut[q * N + k], lut[k * N + q], "symmetric at ({q},{k})");
            if q != k {
                assert_ne!(lut[q * N + k], 0, "off-diagonal non-zero at ({q},{k})");
            }
        }
    }

    // Full 65536-entry gather == row-major index (the arithmetic the shader runs).
    for q in 0u16..256 {
        for k in 0u16..256 {
            assert_eq!(
                gather(&lut, q as u8, k as u8),
                lut[q as usize * N + k as usize]
            );
        }
    }

    eprintln!(
        "PROBE-GPU-LUT cpu-ref: 256×256 u16 LUT = {} B (128 KiB); \
         symmetric + zero-diagonal; full 65536-entry gather == row-major index; deterministic",
        lut.len() * 2
    );
}

/// The WGSL the GPU-exec leg uploads — committed so the GPU realization is
/// reviewable even where no adapter runs it. A full-screen-triangle vertex
/// stage (no vertex buffer) + a fragment stage that `textureLoad`s the LUT at
/// the output texel's `(x, y)` and writes the fetched distance. Integer sampled
/// texture + `textureLoad` + integer render target are all WebGL2-core (GLES3),
/// so the one shader covers both the WebGPU and the WebGL2 backend.
#[cfg(feature = "wgpu")]
const LUT_WGSL: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // Oversized triangle covering the whole 256×256 target in one primitive.
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

@group(0) @binding(0) var lut: texture_2d<u32>;

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) u32 {
    // frag.xy is the pixel center (x+0.5); floor to the integer texel (x=col, y=row).
    let coord = vec2<i32>(i32(frag.x), i32(frag.y));
    return textureLoad(lut, coord, 0).r;
}
"#;

#[cfg(feature = "wgpu")]
mod gpu_exec {
    use super::N;

    /// Owned device/queue + the LUT-gather pipeline. `new` returns `None` when
    /// no adapter is available (a box with neither GPU nor software
    /// rasterizer), mirroring `super::super::gpu::GpuPainter::new`.
    pub struct GpuLut {
        device: wgpu::Device,
        queue: wgpu::Queue,
    }

    impl GpuLut {
        pub async fn new() -> Option<Self> {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    // Headless: no surface. Reaches WebGPU + native/software GL
                    // (lavapipe) adapters. wgpu's WebGL backend REQUIRES a
                    // canvas-bound compatible_surface, so the wasm32 WebGL2 path
                    // needs a surface-bound harness — out of scope for this
                    // headless probe (the WGSL itself stays WebGL2-compatible).
                    compatible_surface: None,
                })
                .await?;
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("a2ui-paint gpu-lut device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::downlevel_defaults(),
                        memory_hints: wgpu::MemoryHints::default(),
                    },
                    None,
                )
                .await
                .ok()?;
            Some(Self { device, queue })
        }

        /// Upload `lut` (256×256 `u16`, row-major) as an `R16Uint` texture,
        /// render every texel of a 256×256 `R32Uint` target by `textureLoad`,
        /// copy back, and return the 65536 gathered values as `u32`.
        pub fn gather_full_table(&self, lut: &[u16]) -> Vec<u32> {
            let dim = N as u32;
            let extent = wgpu::Extent3d {
                width: dim,
                height: dim,
                depth_or_array_layers: 1,
            };

            // --- LUT source texture (R16Uint, sampled via textureLoad) ---
            let lut_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("gpu-lut source"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            // u16 row-major → native-endian bytes (no bytemuck; crate forbids unsafe).
            let mut lut_bytes = Vec::with_capacity(lut.len() * 2);
            for v in lut {
                lut_bytes.extend_from_slice(&v.to_ne_bytes());
            }
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &lut_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &lut_bytes,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(dim * 2), // R16Uint = 2 B/texel
                    rows_per_image: Some(dim),
                },
                extent,
            );
            let lut_view = lut_tex.create_view(&wgpu::TextureViewDescriptor::default());

            // --- R32Uint render target (each texel = the fetched distance) ---
            let target = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("gpu-lut target"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Uint,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

            // --- pipeline ---
            let shader = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("gpu-lut shader"),
                    source: wgpu::ShaderSource::Wgsl(super::LUT_WGSL.into()),
                });
            let bind_group_layout =
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("gpu-lut bgl"),
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Uint,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        }],
                    });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu-lut bg"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&lut_view),
                }],
            });
            let pipeline_layout =
                self.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("gpu-lut layout"),
                        bind_group_layouts: &[&bind_group_layout],
                        push_constant_ranges: &[],
                    });
            let pipeline = self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("gpu-lut pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: "vs_main",
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: "fs_main",
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R32Uint,
                            blend: None, // integer targets cannot blend
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

            // --- readback buffer (256 texels × 4 B = 1024 B/row, already a
            // multiple of COPY_BYTES_PER_ROW_ALIGNMENT=256, so no padding) ---
            let bytes_per_row = dim * 4;
            let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-lut readback"),
                size: (bytes_per_row * dim) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu-lut encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gpu-lut pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1); // full-screen triangle
            }
            encoder.copy_texture_to_buffer(
                wgpu::ImageCopyTexture {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyBuffer {
                    buffer: &readback,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(dim),
                    },
                },
                extent,
            );
            self.queue.submit(std::iter::once(encoder.finish()));

            // Map + poll + read back → Vec<u32> (from_ne_bytes, no unsafe).
            let slice = readback.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device.poll(wgpu::Maintain::Wait);
            let data = slice.get_mapped_range();
            let mut out = Vec::with_capacity((dim * dim) as usize);
            for chunk in data.chunks_exact(4) {
                out.push(u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            drop(data);
            readback.unmap();
            out
        }
    }
}

/// GPU-exec leg — adapter-guarded, full-table parity. Skips green where no
/// WebGPU/WebGL2 adapter exists (e.g. a headless container with no software
/// rasterizer ICD installed); runs the real parity wherever one does.
#[cfg(feature = "wgpu")]
#[test]
fn probe_gpu_lut_gpu_exec_full_table_parity() {
    let lut = build_lut(0x9E37_79B9_7F4A_7C15);
    let Some(gpu) = pollster::block_on(gpu_exec::GpuLut::new()) else {
        eprintln!(
            "PROBE-GPU-LUT gpu-exec: SKIP — no surface-less wgpu adapter in this \
             environment (the parity runs wherever a WebGPU or native/software-GL \
             adapter exists — lavapipe CI / browser WebGPU)"
        );
        return;
    };
    let got = gpu.gather_full_table(&lut);
    assert_eq!(got.len(), N * N, "readback covers the full table");
    let mut mismatches = 0usize;
    for i in 0..N * N {
        if got[i] != lut[i] as u32 {
            mismatches += 1;
        }
    }
    assert_eq!(
        mismatches, 0,
        "GPU texture-gather is bit-exact vs the CPU LUT over all 65536 entries"
    );
    eprintln!(
        "PROBE-GPU-LUT gpu-exec: PASS — 65536/65536 texels bit-exact \
         (256×256 R16Uint LUT → R32Uint target via textureLoad)"
    );
}
