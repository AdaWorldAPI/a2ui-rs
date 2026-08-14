// Field shaders — WebGL2-safe by construction: no storage buffers, no
// compute, no bindings beyond one uniform block.

struct Camera {
    center:   vec2<f32>,
    viewport: vec2<f32>,
    scale:    f32,
    fade:     f32,
    pad:      vec2<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

// World -> clip. One place, so nodes, edges and arrows can never disagree
// about where a point is.
fn project(world: vec2<f32>) -> vec4<f32> {
    let px = (world - cam.center) / cam.scale;
    let ndc = vec2<f32>(px.x / (cam.viewport.x * 0.5), -px.y / (cam.viewport.y * 0.5));
    return vec4<f32>(ndc, 0.0, 1.0);
}

// The palette is a small fixed table, indexed by the OPAQUE domain byte. The
// shader does not know what a domain means either — it looks one up.
fn palette(i: u32) -> vec3<f32> {
    switch (i % 8u) {
        case 0u:  { return vec3<f32>(0.55, 0.60, 0.70); } // unknown / grey
        case 1u:  { return vec3<f32>(0.91, 0.35, 0.42); } // red
        case 2u:  { return vec3<f32>(0.36, 0.78, 0.85); } // cyan
        case 3u:  { return vec3<f32>(0.95, 0.75, 0.30); } // amber
        case 4u:  { return vec3<f32>(0.45, 0.85, 0.55); } // green
        case 5u:  { return vec3<f32>(0.72, 0.55, 0.92); } // violet
        case 6u:  { return vec3<f32>(0.95, 0.60, 0.35); } // orange
        default:  { return vec3<f32>(0.85, 0.87, 0.92); } // near-white
    }
}

// ── nodes: one instanced quad per node, ring drawn by SDF ──────────────────

struct NodeIn {
    @location(0) corner:  vec2<f32>,  // per-vertex, the unit quad
    @location(1) pos:     vec2<f32>,  // per-instance
    @location(2) radius:  f32,
    @location(3) alpha:   f32,
    @location(4) palette: u32,
    @location(5) state:   u32,
};

struct NodeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv:    vec2<f32>,
    @location(1) rgba:  vec4<f32>,
    @location(2) state: f32,
    @location(3) px_r: f32,
};

@vertex
fn vs_node(in: NodeIn) -> NodeOut {
    var out: NodeOut;
    // The quad is grown by a constant so the ring's outer stroke and its
    // antialiasing have room; without the margin the stroke is clipped by
    // its own geometry at high zoom.
    let r = in.radius * 1.35;
    out.clip = project(in.pos + in.corner * r);
    out.uv = in.corner;
    out.rgba = vec4<f32>(palette(in.palette), in.alpha * cam.fade);
    out.state = f32(in.state);
    // Radius in PIXELS: the fragment shader needs it to keep the stroke one
    // pixel wide no matter how far the camera is zoomed.
    out.px_r = r / cam.scale;
    return out;
}

@fragment
fn fs_node(in: NodeOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);                 // 0 at centre, 1 at the quad edge
    let ring = 0.74;                        // where the stroke sits
    // Stroke width and softness in UV units, derived from the on-screen size
    // so the ring stays crisp at every zoom instead of blurring out.
    let w = clamp(1.6 / max(in.px_r, 1.0), 0.02, 0.30);
    let aa = clamp(1.0 / max(in.px_r, 1.0), 0.004, 0.20);
    var a = (1.0 - smoothstep(ring + w, ring + w + aa, d))
          * smoothstep(ring - w - aa, ring - w, d);
    // A held node fills, so "I am dragging this" is unmistakable.
    if (in.state > 1.5) {
        a = max(a, (1.0 - smoothstep(ring - w - aa, ring - w, d)) * 0.55);
    }
    if (a <= 0.0) { discard; }
    // A lit node gets a hotter core than the palette, without a second pass.
    let boost = select(1.0, 1.25, in.state > 0.5);
    return vec4<f32>(in.rgba.rgb * boost, in.rgba.a * a);
}

// ── edges: LineList indexed into the node positions ────────────────────────

struct EdgeIn {
    @location(0) pos:   vec2<f32>,
    @location(1) alpha: f32,
};

struct EdgeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) a: f32,
};

@vertex
fn vs_edge(in: EdgeIn) -> EdgeOut {
    var out: EdgeOut;
    out.clip = project(in.pos);
    out.a = in.alpha;
    return out;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    // Edges sit UNDER the nodes tonally: an edge as loud as a node turns a
    // dense field into a grey mat.
    return vec4<f32>(0.42, 0.47, 0.58, in.a * 0.55 * cam.fade);
}

// ── arrowheads: one instanced triangle per edge, at the midpoint ───────────

struct ArrowIn {
    @location(0) corner: vec2<f32>,  // per-vertex unit triangle
    // NOT `from`/`to`: both are RESERVED WGSL keywords, and naga rejects the
    // module at create_shader_module. Caught by the offscreen paint test the
    // moment a real adapter existed — a shader that never compiles is
    // invisible to every CPU-side test.
    @location(1) tail:   vec2<f32>,  // per-instance: the edge's source end
    @location(2) head:   vec2<f32>,  // per-instance: the edge's target end
    @location(3) alpha:  f32,
};

struct ArrowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) a: f32,
};

@vertex
fn vs_arrow(in: ArrowIn) -> ArrowOut {
    var out: ArrowOut;
    let d = in.head - in.tail;
    let len = max(length(d), 1e-4);
    let dir = d / len;
    let nrm = vec2<f32>(-dir.y, dir.x);
    // Size in WORLD units derived from the camera, so the head stays the same
    // on-screen size at every zoom — the "dynamic arrows" the field needs.
    let s = 7.0 * cam.scale;
    let mid = in.tail + d * 0.5;
    let p = mid + dir * in.corner.x * s + nrm * in.corner.y * s;
    out.clip = project(p);
    out.a = in.alpha;
    return out;
}

@fragment
fn fs_arrow(in: ArrowOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.58, 0.63, 0.74, in.a * 0.75 * cam.fade);
}
