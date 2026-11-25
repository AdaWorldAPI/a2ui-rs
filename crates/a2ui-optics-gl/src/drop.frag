#version 300 es
// The raindrop, per pixel. The same model as a2ui_paint::optics: a spherical cap
// resting on the page, a perpendicular ray refracted at the curved top (Snell,
// vector form) and carried down to the page, which is what magnifies the text.
// Every constant comes from Rust (DropUniforms); nothing here is tuned by hand.
precision highp float;

uniform sampler2D u_bg;     // the page, device pixels, top-left origin
uniform vec2 u_size;        // page size in device pixels
uniform vec2 u_view;        // size of the drawn canvas, device pixels (just the drop's square)
uniform vec2 u_origin;      // where the canvas's top-left sits on the page, device pixels
uniform vec2 u_center;      // drop centre, device pixels, top-left origin
uniform float u_radius;     // footprint radius, device pixels
uniform float u_sphere;     // radius of the sphere the cap is cut from
uniform float u_height;     // apex height, device pixels
uniform vec3 u_ior;         // refractive index for R, G, B
uniform float u_refraction; // 1 = on, 0 = off (ablation)
uniform float u_f0;         // normal-incidence reflectance of the green index
uniform float u_rim;        // Fresnel rim opacity (0 = off)
uniform vec3 u_half;        // Blinn-Phong half vector (light + view), normalized
uniform float u_shininess;
uniform float u_spec;       // highlight intensity (0 = off)

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

vec4 page(vec2 p) {
    return texture(u_bg, p / u_size);
}

void main() {
    // gl_FragCoord is bottom-up; the page and every uniform are top-left. The canvas
    // covers only the drop; the page around it is the page itself, not a copy.
    vec2 pt = u_origin + vec2(gl_FragCoord.x, u_view.y - gl_FragCoord.y);
    vec2 d = pt - u_center;
    float r = length(d);
    // coverage of this pixel by the footprint: 1 inside, 0 outside, a one-pixel ramp at the edge
    float a = clamp(u_radius - r + 0.5, 0.0, 1.0);
    if (a <= 0.0) {
        o = vec4(0.0);
        return;
    }
    r = min(r, u_radius);
    // a2ui_paint::optics::Droplet::surface
    float root = sqrt(max(u_sphere * u_sphere - r * r, 1e-6));
    float h = root - (u_sphere - u_height);
    float slope = r / root;
    vec2 dir = r > 0.0 ? d / r : vec2(0.0);
    vec3 off = vec3(
        refract_offset(slope, h, u_ior.r),
        refract_offset(slope, h, u_ior.g),
        refract_offset(slope, h, u_ior.b)) * u_refraction;
    vec3 col = vec3(
        page(pt + dir * off.r).r,
        page(pt + dir * off.g).g,
        page(pt + dir * off.b).b);
    // Fresnel rim: the edge transmits less of the page
    float nl = sqrt(slope * slope + 1.0);
    vec3 n = vec3(dir * slope / nl, 1.0 / nl);
    float fres = u_f0 + (1.0 - u_f0) * pow(1.0 - n.z, 5.0);
    col *= 1.0 - clamp(fres * u_rim, 0.0, 1.0);
    // specular highlight of the key light, Schlick-weighted, white over
    float nh = max(dot(n, u_half), 0.0);
    float spec = clamp(pow(nh, u_shininess) * min(0.25 + 0.75 * fres / max(u_f0, 1e-3), 4.0) * u_spec, 0.0, 1.0);
    col = col * (1.0 - spec) + spec;
    o = vec4(col * a, a); // premultiplied, as the canvas composites it
}
