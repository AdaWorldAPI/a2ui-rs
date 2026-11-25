# Chromatic dispersion for the a2ui glass material — design note

All numbers below come from a Python port of `Glass::displacement` (squircle profile, bezel 17, thickness 16,
depth 8, 128 samples). Scripts: `.claude/plans/a2ui-optics-dispersion/disp.py` and `v.py`; run with `python3 -I disp.py` and `python3 -I v.py`.

## 1. Physics and the size of the effect

Cauchy: `n(λ) = A + B/λ²`. Abbe number `V_d = (n_d − 1)/(n_F − n_C)`, with Fraunhofer lines
F = 486.1 nm, d = 587.6 nm, C = 656.3 nm. Given `(n_d, V)`: `Δn = n_F − n_C = (n_d − 1)/V`,
`B = Δn / (1/λ_F² − 1/λ_C²)`, `A = n_d − B/λ_d²`. 

Wavelengths per channel: **R = C (656.3), G = d (587.6), B = F (486.1)**. Reasons: these are the lines V is defined
on, so `n_B − n_R = (n_d − 1)/V` exactly; and anchoring G on d makes `n_G == ior`, which is what lets "no dispersion"
reproduce today's map bit for bit (§2). sRGB dominant wavelengths (612/549/465 nm) move every result by < 3 %.

| glass (n_d, V) | channels | n_R | n_G | n_B | max \|D_G\| | max \|D_B − D_R\| |
|---|---|---|---|---|---|---|
| engine default (1.5, 64) | C/d/F | 1.49765 | 1.50000 | 1.50546 | 14.10 px | **0.131 px** |
| BK7 crown (1.5168, 64.17) | C/d/F | 1.51438 | 1.51680 | 1.52243 | 14.38 px | 0.133 px |
| F2 flint (1.6200, 36.37) | C/d/F | 1.61491 | 1.62004 | 1.63196 | 16.03 px | 0.264 px |
| SF10 dense flint (1.7283, 28.41) | C/d/F | 1.72054 | 1.72825 | 1.74617 | 17.66 px | 0.375 px |

The separation peaks one sample in from the rim (x = 0.008, where D_G peaks) and falls with D: at ior 1.5 / V 64 it is
0.131 → 0.074 (x 0.125) → 0.040 (x 0.25) → 0.010 (x 0.5) → 0.000 (flat face). It scales as 1/V at fixed n_d
(V 32 / V 64 = 1.998). Separation by V at ior 1.5: V 64 → 0.13, 32 → 0.26, 16 → 0.52, 10 → 0.83, **8 → 1.04 px**, 5 → 1.65.

**Verdict: with real glass at this geometry the effect is sub-pixel.** Crown glass gives 0.13 px, about one LSB of the
8-bit map (LSB = scale/255 = 28.19/255 = 0.111 px). Even dense flint stays at 0.4 px. A full pixel of R/B fringe needs
V ≈ 8, and no optical glass gets that low (the most dispersive optical glasses and polymers have V ≈ 17–20). A fringe
grows linearly with displacement magnitude, so it becomes visible only with a much thicker lens, an exaggerated
`refraction` multiplier, or an unphysical V; anything stronger than ~0.4 px is exaggeration and must be labelled so.

## 2. Engine mechanism

Dispersion is a per-wavelength IOR. One material constant drives it, so the API carries one optional field:

```
Glass { ior: f32 /* = n_d */, abbe: Option<f32>, .. }
```

- `Glass::channel_ior(&self) -> [f32; 3]`: Cauchy from `(ior, abbe)` at C/d/F. `None`, non-finite or ≤ 0 → `[ior; 3]`
  (no panic, no NaN). G is `ior` by construction.
- The cross-section is computed per channel through the same `displacement` routine with the channel's n. The G
  cross-section uses `n == ior` through identical code, so it is bit-identical to today.
- `FilterParams` keeps `displacement_scale` (= G, unchanged) and gains `channel_scales: Option<[f32; 3]>`,
  `None` when `abbe` is `None`. Strength and `refraction` multiply all three, so strength 0 stays the identity (gate G3).
- The map does not change (§3): it stays the G shape normalised by max_G. The R and B scales are `2·max_c·refraction·s`.

## 3. Browser composition

`feDisplacementMap` has one `scale` and reads two channels (x, y) of `in2`. The direction is the same for all
wavelengths, and only the magnitude differs. So **one map with three scales** is the natural form.

Is a per-channel `scale` exact? The ratios D_R/D_G and D_B/D_G are *not* constant across the bevel: R/G is 0.99718 at
the rim and 0.99686 on the face, and B/G is 1.00653 → 1.00726. They drift because the Snell term is non-linear in n
times the slope. The error of "G shape × per-channel scale" against the exact per-channel cross-section is at most
**0.0015 px (R) / 0.0033 px (B)** at ior 1.5 / V 64, which is 3.6 % of the channel offset. For SF10 it is 0.0055 / 0.0135 px
(≈5 %). Every value is ≤ 0.12 of a map LSB. **Approximate in principle, exact at 8-bit resolution.** Separate per-channel
maps would add two `feImage` decodes for nothing.

Chain (Chromium; `in` = backdrop):

```
feImage → map
feDisplacementMap in=SourceGraphic in2=map scale=s_R → dR
feDisplacementMap … scale=s_G (today's) → dG
feDisplacementMap … scale=s_B → dB
feColorMatrix on dR keeping R+A only (rows: 1 0 0 0 0 / 0… / 0… / 0 0 0 1 0) → r   (likewise g, b)
feBlend r g mode=screen → rg;  feBlend rg b mode=screen → ref
→ existing fine/wide blurs + feComposite mix (unchanged)
```

`screen` is exact on channel-disjoint inputs (a + 0 − a·0 = a) and keeps alpha 1; `feComposite arithmetic` sums alpha
and works on premultiplied values, a hazard on translucent edges.

Cost: 1 → 3 displacement passes plus 3 colour matrices and 2 blends, i.e. 1 → 8 primitives ahead of the blurs.
The blurs dominate today (σ 10), so the added cost is probably moderate, but it is unmeasured: profile before claiming it.
When `channel_scales` is `None` the lab must emit today's single-pass chain, not three equal passes, so the off state is
literally the old filter.

## 4. Other implementations (read, no code copied)

- kube.io, "Liquid Glass in CSS (and SVG)" (https://kube.io/blog/liquid-glass-css-svg/): Snell with n = 1.5,
  perpendicular rays, a single feDisplacementMap with R=x / G=y and `scale` = max displacement, and feBlend for the
  specular. **No dispersion.** No licence; the author says the code is not released.
- naughtyduk/liquidGL (https://github.com/naughtyduk/liquidGL, MIT © NaughtyDuk): WebGL; `aberration` option
  "disperses red and blue either side of the refraction vector, blue further than red". It scales with the refraction
  offset, so it vanishes on the flat face, and 0 skips the extra samples. **No per-wavelength IOR**: an authored
  strength, not a material constant.
- liquid-glass-js (https://cdn.jsdelivr.net/npm/liquid-glass-js@0.1.0/README.md, MIT © Eamon Liu): height-field
  model and feDisplacementMap. The README claims chromatic aberration; not verified how.
- zettersten skills page (https://zettersten.github.io/skills/): describes "3 feDisplacementMap passes for R/G/B with
  different scale values", the same chain shape as §3. Licence not stated.
- "liquid-glass-lab" by TheTechBot: **not found** by two searches. Its claims (MIT, R/G/B split) remain unverified.

None of these derives the split from an Abbe number; §2–3 is the physical form of the same three-pass chain.

## 5. Falsifiable tests

1. `abbe: None` → `render()` output, `cross_section()` and `filter_params()` are byte-identical to today, and
   `channel_scales` is `None`. Also `abbe: Some(f32::INFINITY | 0.0 | NaN | -1)` → identical to `None`, with no panic.
2. Order: with `abbe: Some(64)`, n_R < n_G = ior < n_B, and |D_R(x)| < |D_G(x)| < |D_B(x)| at every sample with
   |D_G| > 1e-3 (red displaces less than blue).
3. Abbe identity: `n_B − n_R == (ior − 1)/V` within 1e-6. This is the constant the API promises, not an authored fringe.
4. Shape: separation |D_B − D_R| is ≥ 0.12 px near the rim and < 0.002 px for x ≥ 0.75. It is exactly 0 on the
   flat face (x = 1) and where `strength = 0`.
5. 1/V law at fixed ior: sep(V/2) / sep(V) ∈ [1.95, 2.05] (measured 1.998). Pair it with V → ∞ making sep → 0.
6. Scale-only error bound: |D_c − (max_c/max_G)·D_G| ≤ 0.02 px for every channel and sample at V ≥ 28, the bound that
   licenses one map (§3). The test must also show that the exact ratio is not constant (R/G at the rim ≠ on the face),
   so the bound is checked against a real gap rather than holding trivially.
7. Disable runs: removing the Cauchy term (all channels = ior) must turn tests 2–5 red; reverting the G anchor to the
   sRGB 549 nm must turn test 1 red.
