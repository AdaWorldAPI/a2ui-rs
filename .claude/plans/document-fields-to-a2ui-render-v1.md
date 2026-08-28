# DRAFT v1 — document fields → a2ui render (the USABILITY §5 wiring)

> Wires the archive's document concept into the a2ui render path so the
> usability signals a person needs are **addressed fields**, not template
> conditionals. Motivating study: `AdaWorldAPI/paperless-rs`
> `docs/USABILITY.md` (merged PR #4) — the product-surface findings; this
> plan is the render-side half of its §5.
>
> Ledger (board hygiene, per this repo's CLAUDE.md): OGAR
> `docs/DISCOVERY-MAP.md` `D-A2UI-SCREEN-ADDRESSING` — append the wave
> status there when a wave lands; a2ui-rs commits + CLAUDE.md + this plan
> are the consumer-side record.

## PART A — HONEST REGISTER

### A.1 VERIFIED AGAINST CODE (receipts — file:line, this session)

- `ogar-vocab::document()` (`crates/ogar-vocab/src/lib.rs:4288-4307`) exists
  and declares **7 attributes**: `version`, `source`, `geometry`,
  `content_sha256`, `mime`, `pages`, `fields` → mask positions **0..6**.
- It is registered in the ClassView registry:
  `("document", document())` (`ogar-class-view/src/lib.rs:228`).
- Field-basis contract: `FieldMask` bit `n` = the `n`-th `FieldRef` in
  `ObjectView::fields`; order is **attributes first (declaration order),
  then associations**; "once instances persist, bit positions are
  append-only" (`ogar-class-view/src/lib.rs:34-48`).
- `FieldMask(pub u64)`, `MAX_FIELDS = 64`
  (`lance-graph-contract/src/class_view.rs:70,76`). `from_positions`
  **ignores** positions ≥ 64 — explicitly NOT folded, because `& 63` would
  alias position 64 onto bit 0 and "silently corrupt the presence contract"
  (Codex P2 on #441, `:78-92`).
- `WideFieldMask(WideRepr)` (`:221`) is `Small(u64)` until a position ≥ 64
  is set, then promotes once to `Wide(Box<[u64]>)` (`:231-239`).
- **N3 across the pair** (`:202-206`): position `n` denotes the same logical
  field in both types; `0..63` read bit-for-bit identically; widening only
  adds ≥ 64, never moves the low bits.
- Why two types and not one widened type: `FieldMask: Copy` is load-bearing
  (`ClassProjection::next` reads `self.mask.has(..)` out of `&mut self`); a
  `Box`-bearing repr cannot be `Copy` — "the exact footgun Ruling (c)
  forbids" (`:190-199`).
- RBAC is by projection over **`WideFieldMask`** on both sides — the
  wide-native mask "#205 correction mandated"; `project_surface` is
  fail-closed and `WideFieldMask::full_for` is a **render** sentinel,
  FORBIDDEN as an RBAC fallback (charter C1.4(c))
  (`a2ui-server/src/project.rs:1-20,54-60`).
- Wire frames: `NodeDelta { key: [u8;16], mask_words: Vec<u64>, values }` +
  `ActionInvoke`, re-exported from `ogar-a2ui-frame`
  (`a2ui-core/src/lib.rs:21-24`) — the wire mask is **wide-native**.
- The worked render pattern exists: `a2ui-paint/examples/patient.rs` builds
  a basis of `FieldView { position, label, predicate, value }` in
  mask-position order + `ActionRef { ordinal, label }`, and slices surfaces
  from it (`patient_basis()`, list = `take(6)`, detail = `0..10`).
- **`FieldView` is a fold being widened upstream**: OGAR is widening it to a
  renderer-neutral `enum FieldView { Text, Badge, Table, ObjectSlot,
  Geometry, … }` (the struct becomes `FieldRow`, the `Text` payload) —
  `E-ONE-MASK-THREE-PORTS`. Named as an **OGAR-side gap, not code**
  (this repo's `CLAUDE.md` § DocIr composition grounding).

### A.2 DESIGN PROPERTIES (sound by construction — NOT yet code)

Everything in PART C/D below is design intent. The gates in PART E are the
receipts, at implementation time — not facts verified today.

### A.3 A PRE-EXISTING RED TEST ON THE W1 CRATE (not caused by this plan)

`ogar-class-view` is currently red on OGAR `main`:
`every_codebook_id_appears_in_class_ids_all` fails with *"typed_field
(0x080a) in class_ids::ALL but missing from OgarClassView registry"*, and
`known_class_ids_iterates_in_stable_codebook_order` fails on the same
missing `2058` (OGAR Actions run 32879569898, observed 2026-08-25).
`typed_field()` exists as a class fn (`ogar-vocab/src/lib.rs:4272-4279`) but
is absent from the registry list. **W1 touches this crate**, so the plan must
not be mistaken for its cause, and W1's gate cannot be "the crate is green"
until this is fixed or explicitly excluded. Filed as a **precondition**, not
adopted as scope.

## PART B — WHAT IS BEING WIRED, AND WHY THESE TWO FIELDS

`USABILITY.md` §5 ranks five gaps. Two of them are **missing fields**; the
other three are already addressable from the existing basis or are actions:

| §5 item | shape | status against today's basis |
|---|---|---|
| 1. show the page image | field | **already addressable** — `content_sha256` (pos 3) is the address of the raw bytes; the renderer resolves it |
| 2. confidence → **status** | field | **MISSING** — no quality field on the class |
| 3. highlight uncertain words | field | reachable via `pages` (pos 5); per-word `conf`+bbox are inside `doc.v1` |
| 4. show what was **dropped** | field | **MISSING** — no loss signal on the class |
| 5. correction | action | `ActionDef` ordinal (T2), not a field |

So W1 adds exactly **two** attributes. The discipline is deliberate:

- **`quality` is a reading, not a score.** The MedCare-rs lesson
  (`LabValue::classify` → `LabFlag`): a clinician reads *"normal"*, not
  *"13.2"*. Emitting a raw `mean_conf` is what the current UI does and it is
  measured to be uncorrelated with correctness on the failures that matter
  (`mean_conf 99.47` at `CER 0.6154`). The class carries the **status**; the
  raw number stays out of the canonical field basis.
- **`dropped` makes loss loud.** `tesseract-ocr`'s `Document.drop_caps`
  exists precisely to "make the loss LOUD" and no screen reads it. A count
  of knowingly-discarded content is source-agnostic: a DOM retina reports 0.

Both are **source-agnostic**, which is the bar for the canonical class —
`DocIr` already carries per-source confidence interpreted through
`Provenance`, so a document-level reading is consistent with the existing
vocabulary rather than novel.

## PART C — THE FIELD BASIS, AND WHICH MASK AT WHICH LAYER

**Append-only.** New attributes take positions **7-8**; `0..6` do not move
(N3 + the class-view append-only rule).

```
pos 0  version          String        (existing)
pos 1  source           Provenance    (existing)
pos 2  geometry         Geometry      (existing)
pos 3  content_sha256   [u8; 32]      (existing)  ← the image address
pos 4  mime             String        (existing)
pos 5  pages            Vec<DocPage>  (existing)  ← per-word conf + bbox live here
pos 6  fields           Vec<TypedField> (existing)
pos 7  quality          DocQuality    (NEW — the reading, not a score)
pos 8  dropped          u16           (NEW — knowingly-discarded content count)
```

**Mask choice, stated because it is the easiest thing here to get wrong:**

- **Class side → `FieldMask`.** 9 positions is far under 64, and the
  contract's own guidance is explicit: *"reach for `FieldMask` for classes
  with <= 64 fields"* (`class_view.rs:213-215`).
- **a2ui boundary → `WideFieldMask`.** `project_surface` and
  `NodeDelta.mask_words` are wide-native by the #205 mandate. Not optional,
  not a preference.
- **The bridge is free and safe by N3**: document's mask stays `Small(u64)`,
  never allocates, and no position renumbers.
- **Never hand-build a `FieldMask` from positions that could cross 64** —
  the silent-drop at `from_positions` is fail-closed for RBAC (a lost bit
  only shrinks `surface ∩ role`) but it is *silent missing content* for
  rendering, i.e. exactly the failure class this whole arc is about.

**Ride the `FieldView` widening; do not pre-empt it.** `quality` is a
**`Badge`** in the coming enum — a status chip is precisely that variant.
Until the widening lands upstream, `quality` renders through the current
`Text` fold with its status *string*, and W2 must not invent a parallel
badge vocabulary to get there early (**T1**: variants are render skins,
never a second closed vocabulary). Same for item 3: word-level highlight is
a **`Geometry`** candidate, deferred to the widening rather than hand-rolled.

## PART D — WAVES

**W1 — OGAR: extend the class (small, append-only).**
Append `quality` + `dropped` to `document()` at positions 7-8. No reorder,
no classid mint (the hard rule: concepts are minted in `ogar-vocab`'s
codebook; `DOCUMENT = 0x080B` already exists at `lib.rs:1960`, and this adds
attributes to an existing class, not a concept).

**W2 — a2ui-rs: the render example (the actual wiring).**
`crates/a2ui-paint/examples/document.rs`, mirroring `patient.rs` exactly: a
`document_basis()` in mask-position order matching W1's class, a
`document_actions()` of `ActionRef` ordinals, list + detail surfaces, and
deterministic PNG output. No new types, no new vocabulary.

**W3 — consumer (NOT in this plan's scope).**
`tesseract-paperless-web` populating `quality`/`dropped` and rendering
through the projection rather than its hand-rolled Askama. Named so the arc
is legible; deliberately out of scope until W1+W2 are green.

## PART E — PRE-REGISTERED GATES

- **G1 (N3 bridge, the property this plan leans on).** Build
  `FieldMask::from_positions(&[0..=8])` and a `WideFieldMask` over the same
  positions; assert `has(n)` agrees for every `n in 0..9`, and that the wide
  value is still `Small` (never promoted). *Can fail* if the N3 claim is
  wrong — it is not a restatement of a type signature.
- **G2 (append-only).** Positions `0..6` resolve to the same seven field
  names after W1 as before. A reorder or insert fails it.
- **G3 (basis fits).** `document()`'s field count `< FieldMask::MAX_FIELDS`,
  asserted rather than assumed — the guard that would catch a future basis
  growing into the silent-drop zone.
- **G4 (T2 — behavior by address).** The example's actions are `ActionRef`
  ordinals only; grep the W2 diff for zero inline handlers/callbacks.
- **G5 (T1 — no second vocabulary).** W2 introduces no new field-kind enum;
  `quality` renders through the existing `Text` fold pending the upstream
  widening.
- **G6 (central gates).** `cargo +1.97.1 test`, `clippy --all-targets -D
  warnings`, `fmt --check` per this repo's CLAUDE.md § Build.
- **G7 (W1 crate honesty).** A.3's pre-existing red must be green or
  explicitly excluded before W1's own result is called green — a passing W1
  in a red crate is not evidence.

## PART F — WHAT THIS PLAN DOES NOT DO

- **Does not define the quality thresholds.** What separates *clean* /
  *check* / *suspect* in numbers is unmeasured, and `USABILITY.md` §6 already
  flags it. This plan wires the field; the derivation is its own work with
  its own fixture, per this workspace's measure-then-pin rule.
- **Does not touch `ogar-doc-ir`.** The IR is an IR. A prior attempt to put
  UX findings there was closed as the wrong layer (OGAR #283). The one
  IR-shaped observation from the study — `DocIr::Region` carries no
  confidence where `TableCell`/`TypedField` do, so a renderer cannot shade an
  uncertain *line* — stays recorded and unfiled until it has its own evidence.
- **Does not build the client.** W2 is the paint-tier example, the same
  footing as `patient.rs`; no server/wasm change.
- **Does not claim the FieldView widening.** That is OGAR-side and named as a
  gap in this repo's CLAUDE.md; this plan consumes it when it lands.
