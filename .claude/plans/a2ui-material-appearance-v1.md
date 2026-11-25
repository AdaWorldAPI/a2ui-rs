# a2ui material and appearance architecture — extraction plan (v1)

Status: **PROPOSAL** — research and architecture only. No crate is created and nothing is
integrated by this plan. Every step after the plan is gated (see "Gates").

North star: **one optical material implementation, consumed by every application** — not one
CSS imitation per application. Applications describe *what an element is* (role, state); the
shared tier decides how light interacts with it.

## 1. Source references

| Ref | What it is | Where |
|---|---|---|
| report-rs #208 (merged 2026-11-25 10:02, head `151a111`) | The original design contract and reference implementation, which was not wired in | `design/aqua-glass/material.css` (`.aqua-scene`, `.aqua-tile`, `--aqua-*`), `design/aqua-glass/material-studio.js` (`mountMaterialStudio`: hue + strength default, Expert disclosure, reset, `localStorage` key `service-hub.aqua.v1`) |
| report-rs #209 (merged 2026-11-25 10:04, head `a1cfebb`, merge `0c49444`) | The integrated implementation plus the optical experiments | see the inventory in §2 |
| report-rs `docs/plans/aqua-glass-optics-research-v1.md` (in #209) | Research on the optical implementations: mechanisms, parameter provenance, licences | report-rs main |
| report-rs `docs/design/holo-glass-v2/` | The earlier Holo Glass V2 kit (tokens template, material lab, QA gates) | report-rs main |

Neither PR is the canonical owner going forward; they are the historical design and
implementation references. a2ui-rs becomes the owner.

## 2. Inventory of #209 (report-rs main, `0c49444`)

| Symbol / file | Kind | Generic or application-specific |
|---|---|---|
| `src/prefs.rs:468` `enum Material` (`Plain`, `HoloGlass`, `Elevation`, `Frosted`, `HoloV2`, `HoloV3`, `AquaGlass`), `key()`/`label()`/`parse()` | material registry | **generic** (registry) |
| `src/prefs.rs:553` `struct Aqua` (hue, sat, strength, transmission, glare, halo, depth), `clamped`, `key`/`parse` (`h.s.st.t.g.ha.d`), `ambient`, `css_vars` (`:649`) | atmosphere value + validation + CSS projection | **generic** |
| `src/prefs.rs:572` `AQUA_AMBIENTS` (Ruby, Ocean, Mint, Vanilla, Titanium, Crystal) | presets | **generic** |
| `src/prefs.rs:664` `Aqua::from_request`, `:893` `COOKIE_AQUA = "report_aqua"`, `COOKIE_MATERIAL` | persistence (query > cookie) | **application** (cookie names, persistence policy) |
| `src/frame.rs:839` `appearance_sidecar` (eye entry + `role="dialog"` sidecar: hue + strength default, Expert with transmission/glare/halo/depth + presets, reset) | appearance control markup | **generic** (markup), menu placement is the app's |
| `src/frame.rs:1075` `effective_material`, `:1085` `material_attr`, `:374` `BaseTemplate.aqua_style` | applies material + atmosphere to `<html>` | generic rule, app-specific template |
| `static/aqua-sidecar.js` | sidecar behaviour: hover/pin/Escape, Tab order, Space, viewport clamping, bottom sheet, live apply, cookie write | **generic** except the two cookie names |
| `static/aqua-glass.css` | material rules (light model) + studio/sidecar styles | material CSS: **generic, not accepted visually**; sidecar styles: generic |
| `static/aqua-studio.js`, `src/app.rs:6416` `aqua_studio_html`, `:6240` `MaterialLabQuery::aqua`, `:6293` `material_lab_body`, `:6555` `view_material_lab_frame`, `:6646` `view_material_lab` | Material-Labor studio and preview frame | **generic** (laboratory) |
| `static/optics-lab.html`, `static/optics-lab.js`, `src/app.rs:3094` `view_optics_lab` | optics prototype: SDF, convex bevel, Snell, displacement map via `feDisplacementMap` in `backdrop-filter`, dual-kernel scattering, Fresnel rim | **generic** (the seed of the optical engine) |
| `static/aqua-optics.js` | normal-field 9-slice texture generator (served, not loaded) | superseded experiment; do not upstream |
| `templates/shell.css` `.prefs-panel` narrow-screen rule | menu clipping fix | app (report-rs shell) |
| `tests/p75_aqua_glass.rs` (8 tests), `tests/p6_global_nav.rs` script allowlist | gates | generic parts move with the code |

## 3. What a2ui-rs already has (verified in this checkout, `d0fbb77`)

| Crate | Deps | Relevant today |
|---|---|---|
| `a2ui-theme` | **none** | `THEMES_CSS`, `THEMES` (23), `token_rule`, `HOLO_GLASS_CSS`, `asset()` registry, `Identity` (operator name/logo from env), `brand` palette. **Holo Glass is a theme here (`data-theme="holo-glass"`) that also carries a material** — theme and material are not yet separate. |
| `a2ui-layout` | **none** | zones, budgets, `Region`, layout modes; explicitly no colours/fonts ("Geometrie und Vokabular, nicht Gestaltung"). |
| `a2ui-paint` | `ogar-render-askama`, `a2ui-core` (non-optional); `wgpu`, `fontdue`/`png` optional | layout + hit-test of the resolved surface, `Skin`, GPU raster behind `wgpu`. |
| `a2ui-paint-web` | axum, paint, wasm | demo server for the paint tier. |
| `a2ui-graph` | bytemuck, wgpu | SDF rings in WGSL for the graph field. |
| `a2ui-solid` | **none** | 3-D CSG signed-distance evaluation (`sdf.rs`) for printable parts. |
| `a2ui-wasm` | core, lance-graph-contract, ogar-render-askama | fieldview client compiled to wasm. |
| `a2ui-field-web` | a2ui-graph | MGRA stream demo. |

report-rs consumes only `a2ui-theme` and `a2ui-layout` (both zero-dependency).

## 4. Five independent dimensions

| Dimension | Examples | Owner | Carried as |
|---|---|---|---|
| Identity | operator name, logo | `a2ui-theme::Identity` (exists) | env at runtime |
| Theme + typography | brand, dark, office, cockpit; fonts | `a2ui-theme` (exists) | `data-theme`, tokens |
| Material | plain, frosted, holo, optical glass | **`a2ui-theme` material registry (new module)** | `data-material` |
| Atmosphere (personal) | hue, saturation, strength, Expert values, presets | **`a2ui-theme` atmosphere (new module)** | custom properties on `<html>` |
| Projection | hub, matrix, cockpit, form, grid | `a2ui-layout` modes / `a2ui-paint` skins (exist) | route / skin |

Composition rule: each dimension changes only its own attribute or property set. A material
change must not change the theme's tokens, the layout's geometry, the projection, or any
semantic content, action address or permission.

First structural step: **separate material from theme in `a2ui-theme`**. `holo-glass` stays a
theme for compatibility, and its material becomes a registry entry, so any theme can carry any
material (report-rs already does this with `data-material`).

## 5. Ownership matrix (proposed)

| Concern | Proposed owner | Reason | Open question |
|---|---|---|---|
| Material registry, material CSS, presets, atmosphere value type with validation and CSS projection (`Aqua` → `Atmosphere`) | `a2ui-theme` | zero-dependency, already the shared asset home, already consumed by report-rs | none |
| Appearance sidecar markup, styles and script; Material Studio; Material-Labor page assets | `a2ui-theme` assets (`asset()`), markup builder as a function | same consumers, no dependency added | whether the markup builder belongs in `a2ui-theme` or a small `a2ui-appearance` module there |
| Sidecar placement arithmetic (anchor, viewport collision, sheet breakpoint) | `a2ui-layout` | pure geometry, zero-dependency, already the shared budget/zone home | the JS mirrors the arithmetic, or a wasm build calls the Rust (see §6) |
| Optical mathematics: rounded-rectangle SDF, bevel height profile, normals, Snell displacement, Fresnel, scattering parameters, displacement-map encoding | **decision needed** — see below | must be renderer-independent and must not pull OGAR or GPU deps into HTML consumers | crate placement |
| Browser composition: SVG filter, `backdrop-filter`, per-size map generation, fallback detection | `a2ui-theme` assets (JS) for HTML consumers; `a2ui-paint-web` for the demo | HTML consumers need no wasm/GPU | wasm vs JS for map generation |
| GPU optical rendering | `a2ui-paint` behind `wgpu` | GPU stays optional | later, after acceptance |
| Persistence (cookie names, tenant preferences), authentication, permissions, business logic | consumer (report-rs) | application-specific | none |

**The one placement decision for the optical mathematics.** It cannot go into `a2ui-paint` as
it is today: `a2ui-paint` depends unconditionally on `ogar-render-askama` and `a2ui-core`, so an
HTML consumer (report-rs) would pull the OGAR render stack just to show glass. Options:

1. **A zero-dependency `optics` module in `a2ui-theme`** — simplest, but makes the theme crate
   carry numeric code.
2. **A zero-dependency `optics` module in `a2ui-layout`** — geometry-adjacent (SDF is geometry),
   but the crate's own charter excludes appearance.
3. **Make `a2ui-paint`'s OGAR dependencies a default feature** and put `optics` there with
   no dependencies — matches "askama-HTML and wgpu-pixels are two renderers of ONE resolved
   surface", but changes an existing crate's feature surface.
4. **A new zero-dependency crate** (e.g. `a2ui-optics`) — clean, but the brief asks to establish
   reuse before creating crates.

Recommendation: **option 3** if the paint maintainers accept the feature split (it puts the
math where the renderers already live and keeps HTML consumers dependency-free); **option 4**
otherwise. The 2-D rounded-rectangle SDF is four lines; it should not be shared with
`a2ui-solid`'s 3-D CSG evaluator (different domain, different metric caveats).

## 6. Optical engine (from the research in report-rs `docs/plans/aqua-glass-optics-research-v1.md`)

Mechanism (renderer-independent):

1. Signed distance d to the rounded rectangle; outward normal from its gradient.
2. Height over the bevel: x = distance from edge / bevel width; convex squircle
   `(1 − (1 − x)⁴)^¼` or circle `√(1 − (1 − x)²)`.
3. Surface normal from the derivative of the height.
4. Snell refraction (n1 = 1, n2 = IOR) of a perpendicular ray, travel to the back face →
   lateral displacement; one cross-section (128 samples) reused around the shape.
5. Displacement encoded in R/G around 128, normalized by the maximum (filter scale = 2 × max).
6. Scattering after refraction: fine and wide Gaussian mixed by the fine share.
7. Specular rim from normal tilt and key-light facing, Schlick Fresnel weight.
8. Rear illumination as a separate field behind the surface (no silhouette, no mask).

Parameters and provenance (third-party, not verified by us): IOR 1.5 (authored); bevel
0.6 × corner radius, scattering σ 0.875 / 10 with fine share 0.336, regular tint grey 241 at
56.5 %, rim exponent 6 (measured, shayann07); clear tint 4 % white, light direction
(−0.35, −0.94), clear depth 16 (authored, shayann07); F0 = ((n − 1)/(n + 1))² (derived).

Licences: shayann07/liquidglass Apache-2.0 (NOTICE), LiquidLens MIT, liquid-glass-lab MIT,
rookasrudzianskas port MIT + Glass HQ notice, kube.io none stated (no code reuse). The #209
prototype copies no third-party code; parameter values are cited with attribution. Apple
publishes behaviour (WWDC25 session 219), no formulas or constants.

Backends, one material definition:

| Backend | Path | Dependency |
|---|---|---|
| HTML, Chromium | precomputed displacement map → `feDisplacementMap` in `backdrop-filter` | none (CSS/SVG + small JS or server-generated map) |
| HTML, other browsers | frosted fallback (`backdrop-filter: blur()`), no refraction | none |
| GPU | `a2ui-paint` `wgpu` feature, same math in WGSL | optional `wgpu` |
| CPU raster | `a2ui-paint` `raster` feature | optional |

Map generation for HTML: per element size and parameter set, cached. Either in the browser
(JS, or the same Rust compiled to wasm) or on the server (Rust, served as an image asset keyed
by size and parameters). Decide in the laboratory by measuring generation cost and visual
equality between the two.

## 7. Material Laboratory in a2ui-rs

A reference page served by `a2ui-paint-web` (the existing demo server), built from the
`a2ui-theme` assets:

- calibration backgrounds: checkerboard (displacement), lines (distortion), black/white
  (transmission), gradient, RGB boundaries (dispersion), a page of text;
- the Neutral Glass Baseline (one material, no coloured light) and the same material under rear
  illumination;
- every term switchable on its own (ambient, transmission, caustics, rear, body, shadow,
  scattering, rim) for ablation;
- hue and strength, Expert controls, presets — the same sidecar consumers get;
- component sizes from a 56 px control to a 900 px panel; desktop and mobile;
- the reference renderings from report-rs #208/#209 as acceptance references;
- fixed viewports for screenshots at strength 0 %, 53 %, 100 %, and Ruby / Ocean / Mint /
  Vanilla at equal strength.

## 8. Gates (falsifiable)

G1 — **Visual acceptance in the laboratory** by the operator, against the approved reference,
before any production integration. Unit tests do not satisfy G1.

G2 — **Material invariance across colours**: at equal strength, Ruby / Ocean / Mint / Vanilla
renders differ only in the rear-light colour; the neutral highlights and the glass are
identical (compare the material layer with rear light switched off: pixel-identical).

G3 — **Strength 0 is neutral**: at strength 0 every optical term is off (no refraction, no
caustic, no rear light); test by rendering with each term's switch on and off at strength 0 and
asserting equality.

G4 — **Non-interference**: switching the material changes no text, no action address, no
`data-*` used for behaviour, and no layout box (compare DOM text, `href`s and element rects
with material on and off).

G5 — **No silhouette**: the rear-light layer has no mask, no clip and fades to transparent
inside its own box (static check on the stylesheet plus a pixel check at the box edge).

G6 — **Dependencies**: an HTML consumer that enables the material pulls no `wgpu`, no WebGL
requirement and no OGAR render crate (check `cargo tree` of a consumer fixture).

G7 — **Fallbacks**: non-Chromium gets the frosted fallback; `prefers-reduced-transparency`,
`prefers-contrast: more` and `forced-colors` get solid surfaces.

G8 — **No duplicate renderer**: after migration report-rs contains no optical code of its own
(no SDF, displacement or material CSS; check by search over its `static/` and `src/`).

## 9. Migration sequence

| Step | Repo | Scope | Gate |
|---|---|---|---|
| A | a2ui-rs | Material registry and atmosphere contracts in `a2ui-theme` (separate material from theme; `Atmosphere` value type with validation, `key`/`parse`, CSS projection, presets). Port the generic #209 tests. | unit tests; G3 (static part) |
| B | a2ui-rs | Optical mathematics module (placement per §5 decision) and the Material Laboratory in `a2ui-paint-web`, from the #209 optics prototype. | **G1**, G2, G3, G5 |
| C | a2ui-rs | Browser composition assets (SVG filter, map generation, fallback) and the shared appearance sidecar (markup builder, styles, script; placement arithmetic in `a2ui-layout`). | G4, G6, G7 |
| D | report-rs | Replace the local implementation with the a2ui-rs one; keep cookie names, persistence, menu placement. Remove `static/aqua-*`, `static/optics-lab.*`, the material CSS and the lab routes. | G8, report-rs P75 equivalents |
| E | other consumers | Adopt the same tier. | per consumer |

No step after A starts before the previous step's gate is green; B's G1 is the operator's.

## 10. Non-goals

- No production integration before G1.
- No mandatory GPU or WebGL for HTML consumers.
- No upstreaming of the rejected CSS light model as the material; its interaction architecture
  and preference semantics move, its rendering is replaced by the optical engine.
- No application persistence, authentication or permissions in a2ui-rs.
