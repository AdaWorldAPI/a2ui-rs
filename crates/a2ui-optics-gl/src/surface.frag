#version 300 es
// One optical surface, per pixel: the specification of a2ui_paint::optics::surface
// (footprint SDF, shoulder + dome relief, refraction per channel, direct / fine / wide
// transmission, tint, Fresnel rim, environment reflection, key-light highlight, strength). Every constant comes from Rust
// (SurfaceUniforms); nothing here is tuned by hand.
precision highp float;

uniform sampler2D u_bg;     // the page, device pixels, top-left origin, mipmapped
uniform vec2 u_size;        // page size, device pixels
uniform vec2 u_view;        // size of the drawn canvas, device pixels
uniform vec2 u_origin;      // canvas top-left on the page, device pixels
uniform vec2 u_center;      // footprint centre on the page
uniform vec2 u_halfsize;    // footprint half extents
uniform float u_corner;     // corner radius, px
uniform float u_inradius;   // half the shorter side
uniform vec4 u_relief;      // height H, face curvature c, shoulder width W, profile p
uniform float u_base;       // slab under the relief, px of extra travel
uniform vec3 u_ior;         // refractive index for R, G, B
uniform float u_refraction; // strength, 0 when refraction is switched off (ablation)
uniform vec3 u_weights;     // direct, fine, wide, normalized
uniform vec2 u_sigma;       // fine, wide kernel sigma, px
uniform vec4 u_tint;        // body colour, alpha
uniform float u_f0;         // normal-incidence reflectance of the green index
uniform float u_rim;        // Fresnel rim opacity
uniform vec3 u_hv;          // Blinn-Phong half vector
uniform float u_shininess;
uniform float u_spec;       // highlight intensity (strength applied)
uniform float u_reflection; // environment reflection (strength applied)
uniform int u_mode;         // 0 render; 1 debug: (q_r, q_g); 2 debug: (q_b, coverage, height)

out vec4 o;

// a2ui_paint::optics::refract_offset, line for line.
float refract_offset(float slope, float travel, float ior) {
    float nl = sqrt(slope * slope + 1.0);
    float nx = slope / nl;
    float nz = 1.0 / nl;
    float eta = 1.0 / max(ior, 1e-3);
    float cosi = nz;
    float k = 1.0 - eta * eta * (1.0 - cosi * cosi);
    if (k < 0.0) return 0.0;
    float m = eta * cosi - sqrt(k);
    float tx = m * nx;
    float tz = -eta + m * nz;
    return tx / abs(tz) * travel;
}

// surface::footprint_distance
float footprint_distance(vec2 p, out vec2 n) {
    vec2 q = abs(p) - (u_halfsize - u_corner);
    vec2 ov = max(q, 0.0);
    float l = length(ov);
    float sd = l + min(max(q.x, q.y), 0.0) - u_corner;
    if (q.x > 0.0 && q.y > 0.0) n = ov / max(l, 1e-30);
    else if (q.x > q.y) n = vec2(1.0, 0.0);
    else n = vec2(0.0, 1.0);
    n *= vec2(p.x < 0.0 ? -1.0 : 1.0, p.y < 0.0 ? -1.0 : 1.0);
    return sd;
}

// Surface::relief_at: (height, slope) at interior distance d
vec2 relief_at(float d) {
    float H = max(u_relief.x, 0.0);
    float c = clamp(u_relief.y, 0.0, 1.0);
    float W = u_relief.z;
    float p = clamp(u_relief.w, 1.0, 8.0);
    float s = H;
    float ds = 0.0;
    if (W > 0.0) {
        float t = clamp(d / W, 1e-4, 1.0);
        if (t < 1.0) {
            float u = 1.0 - pow(1.0 - t, p);
            s = H * pow(u, 1.0 / p);
            ds = H * pow(u, 1.0 / p - 1.0) * pow(1.0 - t, p - 1.0) / W;
        }
    }
    float rin = max(u_inradius, 1e-3);
    float hd = clamp(H, 1e-3, rin);
    float rs = (rin * rin + hd * hd) / (2.0 * hd);
    float r = clamp(rin - d, 0.0, rin);
    float root = sqrt(max(rs * rs - r * r, 1e-6));
    float f = root - (rs - hd);
    float df = r / root;
    return vec2((1.0 - c) * s + c * f, (1.0 - c) * ds + c * df);
}

vec4 page(vec2 q, float lod) {
    return textureLod(u_bg, q / u_size, lod);
}

// scattered transmission: nine taps on a ring of radius sigma at a matching mip level
vec3 scattered(vec2 q, float sigma) {
    if (sigma < 0.5) return page(q, 0.0).rgb;
    float lod = max(log2(sigma) - 1.0, 0.0);
    vec3 acc = page(q, lod).rgb;
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.7853982;
        acc += page(q + sigma * vec2(cos(a), sin(a)), lod).rgb;
    }
    return acc / 9.0;
}

void main() {
    // gl_FragCoord is bottom-up; the page and every uniform are top-left
    vec2 pt = u_origin + vec2(gl_FragCoord.x, u_view.y - gl_FragCoord.y);
    vec2 p = pt - u_center;
    vec2 n;
    float sd = footprint_distance(p, n);
    float a = clamp(0.5 - sd, 0.0, 1.0);
    if (a <= 0.0) {
        o = vec4(0.0);
        return;
    }
    vec2 hs = relief_at(max(-sd, 0.0));
    float travel = hs.x + max(u_base, 0.0);
    vec3 off = vec3(
        refract_offset(hs.y, travel, u_ior.r),
        refract_offset(hs.y, travel, u_ior.g),
        refract_offset(hs.y, travel, u_ior.b)) * u_refraction;
    vec2 qr = pt + n * off.r;
    vec2 qg = pt + n * off.g;
    vec2 qb = pt + n * off.b;
    if (u_mode == 1) { o = vec4(qr, qg); return; }
    if (u_mode == 2) { o = vec4(qb, a, hs.x); return; }
    vec3 col = u_weights.x * vec3(page(qr, 0.0).r, page(qg, 0.0).g, page(qb, 0.0).b);
    if (u_weights.y > 0.0) col += u_weights.y * scattered(qg, u_sigma.x);
    if (u_weights.z > 0.0) col += u_weights.z * scattered(qg, u_sigma.y);
    col = mix(col, u_tint.rgb, u_tint.a);
    // Fresnel rim and the key-light highlight
    float nl = sqrt(hs.y * hs.y + 1.0);
    vec3 nrm = vec3(n * hs.y / nl, 1.0 / nl);
    float fres = u_f0 + (1.0 - u_f0) * pow(1.0 - nrm.z, 5.0);
    col *= 1.0 - clamp(fres * u_rim, 0.0, 1.0);
    float refl = clamp(fres * u_reflection, 0.0, 1.0);
    col = col * (1.0 - refl) + refl;
    float nh = max(dot(nrm, u_hv), 0.0);
    float spec = clamp(pow(nh, u_shininess) * min(0.25 + 0.75 * fres / max(u_f0, 1e-3), 4.0) * u_spec, 0.0, 1.0);
    col = col * (1.0 - spec) + spec;
    o = vec4(col * a, a); // premultiplied
}
