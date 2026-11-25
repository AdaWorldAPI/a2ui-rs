# Visual references — Aqua Glass (accepted, frozen)

G1b, 2026-11-25: the operator accepted the Aqua Glass preview in full: neutral glass, diffuse
rear illumination (the Ruby underlight behind milk glass) and the prismatic light. It is the
canonical default. Do not tune the default further; personal variations go through the
appearance sidecar (light colour, effect strength, Expert values), never into the stylesheet.

Ownership moved here from report-rs (`static/aqua-glass.css`, report-rs #213) unchanged.

| file | what |
|---|---|
| `aqua-glass-accepted.css` | the accepted stylesheet, byte for byte; `tests/aqua_glass_frozen.rs` requires `assets/aqua-glass/aqua-glass.css` to equal it |
| `aqua-glass-dashboard.png` | the Dashboard tile as accepted (crop of the start page, 2× upscaled) |
| `aqua-glass-start-1440x900.png` | the full start page it was cut from |
| `aqua-glass-ablation-default-rear-prism.png` | default: neutral glass, rear light and prism light |
| `aqua-glass-ablation-rear-only.png` | `?aqua-off=beam` |
| `aqua-glass-ablation-prism-only.png` | `?aqua-off=rear` |
| `aqua-glass-ablation-neutral-both-off.png` | `?aqua-off=rear,beam`: the neutral glass alone |
| `aqua-glass-ablation-glass-off.png` | `?aqua-off=glass` |

How they were produced: report-rs `/view?material=aqua-glass`, start page, anonymous kiosk,
stylesheet of report-rs `6c39d5b`, default atmosphere `352.75.53.85.53.45.55`, headless
Chromium (Playwright) at 1440 × 900, device scale factor 1. The Dashboard crop is box
(150, 230)–(600, 450), resized to 900 × 440 (Lanczos); the ablation panels are 640 × 350 crops
of the start page's upper left. SHA-256: dashboard
`c933f093a58f2d22044defc598cb10138b99eb6c3337e14df80add2f1df32786`, full page
`518454baee384fd14e284c06603cdfe122220ebdd8cdca8da01a1150130a2257`.

A pixel comparison needs the same browser build and fonts; the byte-for-byte stylesheet test
is the gate that runs everywhere.
