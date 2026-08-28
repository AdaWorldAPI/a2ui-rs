# Addendum to Wave E — the two status fields (REVISED)

> **⚠ THIS DOCUMENT WAS REWRITTEN.** Its first version was a standalone
> integration plan for wiring document fields into a2ui rendering. That was
> wrong: an authoritative plan already exists and is further along —
> `AdaWorldAPI/tesseract-rs` `.claude/plans/paperless-archive-integration-v1.md`
> ("Paperless substrate — one receipt lane, N borrowers, addressed by DocIr,
> **rendered by a2ui**"), whose **Wave E** is exactly this problem, solved
> better. This file is now an **addendum subordinate to Wave E**, carrying
> only the one finding Wave E does not have, plus an honest record of what
> the first draft got wrong.
>
> **Wave E is the authority. Read it first.** Nothing here overrides it.
>
> The failure that produced the first draft is the one this workspace's own
> rule names: *grep the existing files before writing a new one.* I designed
> against the crates instead of against the plans, and the plans already had
> the answer.

## §1 What the first draft got wrong (all four are Wave E / §2c findings)

**1. It assumed the wire can carry text. It cannot.** a2ui's value lane is
**12 bytes — one `u8` per field position, 0..11** (`FACET_LEN = 12`,
`render_stream.rs:58`); the client renders `state.facet[i].to_string()` — a
string produced from ONE BYTE. The draft said `quality` would "render
through the current `Text` fold with its status *string*." There is no
string. Rendering `doc.v1` through a2ui as it stands "would show twelve
numbers per node" (§2c).

**2. It missed the binding ceiling.** The draft carefully worked out
`FieldMask` (64) vs `WideFieldMask` (unbounded) — and that analysis is
correct as far as it goes, but it is not the constraint that binds. §2c
records **four** ceilings, inconsistent with each other: `FieldMask` at 64,
`FieldView.position`/`WideFieldMask` u8-bounded at 256, the wire's own
`mask_words` u32-native, and **the 12-byte value lane**. Twelve is the one
that decides what can render. Getting the mask width right while missing
the value width is precisely the kind of locally-correct, globally-wrong
analysis the draft's own G1 gate was supposed to protect against.

**3. It proposed building what already exists.** `ogar_doc_ir::project::masked_values`
is **already built and already RBAC-aware via `WideFieldMask`**, projecting
`DocIr` regions into the same `(position, value)` shape a2ui's `project_node`
expects (Wave E). And `Skin::Tile` *already* reads a `BBoxRail`'s two
const-asserted `u8:u8` rails for placement — so page/region navigation "is a
rendering wave, not a wire wave." The draft proposed hand-building a field
basis in an example; the projection path was there the whole time.

**4. It filed a known prerequisite as a surprise.** The draft's §A.3 raised
the red `ogar-class-view` test (`typed_field 0x080a` in `class_ids::ALL` but
missing from the registry) as an unexpected precondition. It is **Wave F**,
already named: *"Mint `typed_field 0x080A` in `ogar-vocab` — a small, scoped
OGAR PR. Blocks: any field-node addressing beyond `ir.fields`' current flat
projection."* The red test is that gap showing up in CI, not a new find.

**And one thing the draft understated.** USABILITY §5's correction path is
worse off than "an `ActionDef` ordinal." §2c: there is **no field-write/edit
frame anywhere** — `FrameKind` is a closed 2-variant vocabulary, so
"correcting an OCR'd value has no representation." An `ActionInvoke` can
*fire* an action; it cannot carry the corrected value. Correction needs a
wire extension, which is Wave F territory, not a Wave E deliverable.

## §2 The one additive finding: a status is a BYTE, which is what the lane carries

This is the whole reason this addendum exists rather than being deleted.

USABILITY.md argued from the MedCare-rs `LabValue::classify` → `LabFlag`
lesson that a document's confidence should be surfaced as a **reading**
(*clean* / *check* / *suspect*), never as a raw `mean_conf`. That was argued
on usability grounds: a person reads "normal", not "13.2", and the raw mean
is measured uncorrelated with correctness on the failures that matter
(`mean_conf 99.47` at `CER 0.6154`).

§2c's blocker turns that from a preference into the **only shape that
fits**:

| candidate surface | size | fits the 12-byte lane? |
|---|---|---|
| document TEXT | KB | **no** — Wave E correctly routes it via `Projection{delta,html}` |
| raw `mean_conf` (f32/f64) | 4-8 B | **no** — not a `u8` |
| **a status enum discriminant** | **1 B** | **yes** — natively |
| a dropped-content count | 1 B if capped | **yes** — saturating at 255 |

So "a number becomes a status" is simultaneously the usability answer and
the *wire* answer. A status byte is addressable through the existing lane
with no wire extension, no value-slab, no Wave F dependency. That is a
genuine convergence and it is the only part of the first draft that
survives contact with §2c.

**Consequent correction to the draft's own proposal:** `dropped: u16` does
not fit a one-byte position. It becomes `u8`, **saturating at 255** — and
saturation is honest here, since the signal is "content was discarded, look"
and not an exact inventory. A page that dropped 300 things and one that
dropped 255 both mean *look at this*.

## §3 The revised proposal, scoped to what Wave E leaves open

Wave E lists what ships now: structural addressing via `masked_values`,
`Skin::Tile` region navigation from `BBoxRail`, actions by ordinal, and text
deliberately NOT on the wire. It does **not** carry a document-level quality
reading or a loss signal — those are new fields, and they are the addendum.

Appended to `ogar-vocab::document()` (`lib.rs:4288-4307`, currently 7
attributes at positions 0..6; append-only so nothing moves):

```
pos 7  quality   DocQuality   1-byte enum — the reading, never a raw score
pos 8  dropped   u8           knowingly-discarded content, saturating at 255
```

Both are source-agnostic (a DOM retina reports `dropped = 0`), both fit the
facet lane natively, and both are `Badge`-shaped for the day the upstream
`enum FieldView{Text,Badge,Table,ObjectSlot,Geometry,…}` widening lands —
consumed then, not pre-empted now (**T1**).

**Sequencing:** this is strictly *after* Wave E, and it inherits Wave E's
falsifiers rather than restating them. Its own gate is narrow and real:

- **G1 (fits the lane).** `DocQuality`'s discriminant range and `dropped`
  both round-trip through a single facet byte unchanged. *Can fail* if a
  variant is added past 255 or the field is widened — which is the exact
  regression this addendum exists to prevent.
- **G2 (append-only).** Positions 0..6 resolve to the same seven field names
  after the change as before.

## §4 What this addendum does NOT do

- **Does not restate or revise Wave E.** Structural addressing, Tile
  navigation, the HTML text path, and actions-by-ordinal are its
  deliverables, with its falsifiers.
- **Does not touch the correction path.** No field-write frame exists;
  that is Wave F.
- **Does not define the quality thresholds.** What separates *clean* /
  *check* / *suspect* in numbers is unmeasured — flagged in USABILITY.md §6
  and still open. This wires the field; the derivation needs its own fixture
  under the measure-then-pin rule.
- **Does not adopt Wave F.** The `typed_field 0x080A` mint and the
  value-slab wire extension are named prerequisites owned elsewhere.
