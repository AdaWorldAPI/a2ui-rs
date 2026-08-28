# Document fields → a2ui render — FOLDED INTO Wave E, kept as a pointer

> **Status: SUPERSEDED. Do not design from this file.**
>
> **The plan is `AdaWorldAPI/tesseract-rs`
> `.claude/plans/paperless-archive-integration-v1.md` — Wave E**
> ("a2ui addresses the structure; text stays on the render path until the
> wire is widened"), with its §2c reality check on what the wire can carry
> and its Wave F prerequisite list. Everything this file once proposed now
> lives there, corrected.
>
> This stub remains only so a session that finds the filename does not
> re-derive the same wrong plan a third time.

## Why this file exists at all

It began (commit `75012f8`) as a standalone integration plan for wiring the
archive's document fields into a2ui rendering. That was a mistake of method,
not of detail: **it was designed against the crates instead of against the
plans**, and an authoritative plan already covered the problem and was
further along. This workspace's own rule names the failure — grep the
existing plans before writing a new one.

## What it got wrong (all answered by that plan's §2c)

1. **It assumed the wire could carry a status string.** The a2ui value lane
   is **12 bytes — one `u8` per field position** (`FACET_LEN = 12`,
   `render_stream.rs:58`); the client renders `state.facet[i].to_string()`,
   a string produced from one byte.
2. **It missed the binding ceiling.** The `FieldMask`(64) vs
   `WideFieldMask`(unbounded) analysis was correct but not decisive; §2c
   records four inconsistent ceilings and the **12-byte value lane** is the
   one that decides what can render.
3. **It proposed building what exists.**
   `ogar_doc_ir::project::masked_values` is already built and already
   RBAC-aware, and `Skin::Tile` already reads `BBoxRail`'s `u8:u8` rails for
   placement.
4. **It filed a known prerequisite as a surprise** — the red
   `ogar-class-view` test is Wave F's `typed_field 0x080A` mint surfacing
   in CI.

It also understated the correction path: there is **no field-write frame**
at all (`FrameKind` is a closed 2-variant vocabulary), so an `ActionInvoke`
can fire an action but cannot carry a corrected value.

## The one finding that survived, now in Wave E

**A status is one byte, which is exactly what the facet lane carries.** So
the usability argument for surfacing confidence as a *reading* rather than a
raw `mean_conf` (`paperless-rs` `docs/USABILITY.md`, from MedCare-rs's
`LabValue::classify` → `LabFlag`) is simultaneously the *wire* answer: text
cannot cross, an `f32` cannot cross, a status discriminant crosses natively
with no value-slab extension. `quality: DocQuality` + `dropped: u8`
(saturating at 255) are folded into Wave E's design list, with the
`ogar-vocab::document()` append listed as a Wave F prerequisite.
