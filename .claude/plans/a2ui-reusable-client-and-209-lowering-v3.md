# DRAFT v3 (RATIFIED) — a2ui-rs reusable-client SoC + the #209 Klickwege lowering

> Phase-4 output of the /5plus3 council. Resolves the Phase-3 panel: 2 BLOCKs
> (dilution-collapse-sentinel on A1 + A2) + convergent FIXes (overclaim,
> firewall). Stricter verdict won on every split. Cross-checked against the
> REAL OGAR #209 (read this session). FROZEN F1-F5 untouched.
>
> Panel tally: overclaim=FIX, dilution-collapse=BLOCK, firewall=FIX.
> Both BLOCKs resolved by ADOPTING the reviewers' own recommendation.

## PART A — HONEST REGISTER (the overclaim fix)

Draft v2 mixed two registers in one "CONFIRMED" section. v3 keeps them apart.

### A.1 VERIFIED AGAINST CODE (receipts — file:line, this session)
- SoC clean: zero `medcare|CompiledClass|Recipe|corpus` in any a2ui-rs public
  signature; only `tests/p_rehost.rs` matches. (grep, all 3 reviewers PASS.)
- §2 inventory CODED verbatim: `KlickwegEdge` (desktop.rs:56-67), `ActionInvocation`
  (ogar-vocab lib.rs:508-540), `render_field_view` sig (field_view.rs:109-116),
  both `FACET_LEN=12` (render_stream.rs:57, wasm lib.rs:48).
- `ActionInvocation::new(identity, action_def, object_instance)` + `..Default::default()`
  exists (ogar-vocab lib.rs:704-718); `ActionState::#[default] Pending` (669-672).
- `ActionSubject::User` exists, doc = "A human user (UI button click...)" (lib.rs:548-549);
  `System` is the default (551).
- `ogar-emitter::emit_action_invocation(&ActionInvocation) -> Vec<Triple>` (lib.rs:774)
  — SPO emission is OGAR-SIDE, a separate phase.
- `nav_witnessed` in OGAR is ONLY `emit_do_adapters(classes, nav_witnessed: &BTreeSet<String>)`
  (do_adapter.rs:46) — a codegen-time concept-name gate; the string `"nav_witnessed"`
  is never a literal/predicate/SPO value anywhere in OGAR.
- Dep reality: ogar-vocab is a2ui-server **dev-dep only** (Cargo.toml); a2ui-rs
  Cargo.lock has zero `medcare-*` / `lance-graph-engine` / `lance-graph-planner`;
  ogar-vocab's own deps = only optional serde. wgpu absent everywhere today.
- `FieldView`/`ActionRef` are OGAR types (ogar-render-askama field_view.rs:57,74),
  imported (never re-exported) by BOTH a2ui-server (render_stream.rs:50) and
  a2ui-wasm (lib.rs:45), with no cross-dep between those two crates. They derive
  only `Debug,Clone,PartialEq,Eq` — no serde.

### A.2 DESIGN PROPERTIES (sound by construction; the GATES are the receipts — NOT yet code)
The paint tier and the lowering are UNBUILT. Their F1/T1/T3/G5 compliance is a
property of the design, proven at implementation by the pre-registered gates
G1-G7 — not a fact verified today. v3 states them as design intent + gate, never
as present-tense receipts (overclaim fix).

## PART B — #209 SCOPE (read from the real issue, not paraphrased)

#209 = a compile-time, type-level seam. Two lowerings + a golden. No runtime/
storage machinery. Home is flexible ("next to the consumer's frame types OR
beside ogar-vocab/ogar-emitter"). Open points are TYPE-LEVEL: ordinal→action_def,
and subject for a click.

## PART C — RATIFIED DESIGN

### 3.i — the wgpu paint tier  (A1 REVERTED per BLOCK-1)
- **SEPARATE crate `a2ui-paint`** (workspace member), consumer-agnostic. Deps:
  `ogar-render-askama` (for the `FieldView`/`ActionRef` TYPES), `wgpu`,
  `a2ui-core` (frames if it ingests), `lance-graph-contract`. **NO dep on
  a2ui-wasm; NO ogar-vocab; NO consumer dep.** (BLOCK-1: FieldView/ActionRef are
  OGAR-owned, so a separate crate takes them the same way a2ui-wasm does; the v2
  "must dep back on a2ui-wasm" premise was factually false. A separate crate ALSO
  keeps a2ui-wasm's crate-wide `#![forbid(unsafe_code)]` intact — wgpu surface/
  window glue that needs `unsafe` lives in a2ui-paint, which sets its own crate
  attrs, and preserves a boundary for a future native/non-wasm paint consumer.)
- **API:** `pub fn paint(fields: &[FieldView], actions: &[ActionRef], …) -> …` —
  plain borrowed slices, exactly how `render_field_view` is invoked. It never
  sees a codebook, a CompiledClass, or a corpus.
- **Layout is paint-internal (A1b FIX):** `FieldView.position` is a 1-D u8 mask
  index. askama hands it to CSS; wgpu derives its own 2-D grid coords from it.
  Layout math is a RENDERER concern (T1-fine: a renderer of the shared surface,
  not a new vocabulary), NOT a shared/stored field. v3 drops the v2 "X:Y rail
  drives the grid" implication that layout is shared data.
- **Hot path (T3):** the wasm client ingests `NodeDelta` LE bytes zero-copy,
  resolves via codebook, and hands the resolved slices to `paint`. No serde.

### 3.i-b — expose the resolved surface  (A1b, reframed — NOT scaffolding)
- `FieldviewClient::apply_node_delta` today computes `Vec<FieldView>`+`Vec<ActionRef>`
  as locals then folds into HTML (lib.rs:238-271). v3 refactors: compute the
  resolved surface once, STORE it on the node's state (or return it), and:
  `pub fn resolved_fields(&self, key:&[u8;16]) -> Option<&[FieldView]>` +
  `pub fn resolved_actions(&self, key:&[u8;16]) -> Option<&[ActionRef]>`.
- **Why it is real, not scaffolding:** a2ui-paint is a SEPARATE crate taking plain
  slices; a wasm client that wants pixels must GET its resolved surface out of its
  own FieldviewClient to hand to `paint`. The accessor exposes what the client
  already computes — a one-time, consumer-agnostic addition to layer (b). Both
  `render_field_view` (HTML) and `paint` (pixels) read the identical stored slice.
- **T3-creep guard (firewall note):** these types now become long-lived public
  client state. If FieldView/ActionRef ever gain serde, it MUST be feature-gated
  exactly like `Frame`/`NodeDelta` (`cfg_attr(feature="serde",…)`), never on by
  default. Documented at the accessor.

### 3.ii — the #209 Klickwege lowering  (A2 REVISED per BLOCK-2 + #209 real scope)
- **Location:** `a2ui_server::lowering` (a2ui-server, ogar-vocab dev→normal
  promotion — 3.ii PASS on home from all reviewers). Plain compile-time fns.
- **Two lowerings (#209):**
  1. `pub fn lower_action_fire(edge: &KlickwegEdge, app_prefix: u16) -> ogar_vocab::ActionInvocation`
     — the action-fire case. Body: `ActionInvocation::new(identity, action_def,
     object_instance)` (A3, reuse constructor) then override `subject =
     ActionSubject::User` (a human click — #209 design point, documented against
     the `System` default). `object_instance` = `from_key` hex GUID; `action_def`
     = canonical String from `(class_id, ordinal, edge.predicate)` — resolvable
     FROM THE EDGE (predicate already carried), no external lookup (door-knock
     clean); `identity` = deterministic from `(class_id, from_key, seq)`.
  2. `pub fn lower_screen_jump(edge: &KlickwegEdge) -> NavWitness` — the screen-jump
     case. `NavWitness` = a plain a2ui-server value (newtype over the witnessed
     target **concept name** String derived from the edge). It is a VALUE, not an
     SPO triple and not a predicate-string constant.
- **nav_witnessed reconciliation (A2 BLOCK resolved):** a2ui-rs does NOT mint a
  `NAV_WITNESSED` predicate const and does NOT stamp an SPO triple. Reasons: (a)
  OGAR's `nav_witnessed` is a codegen-time `BTreeSet<String>` gate — a DIFFERENT
  shape/phase, only an English-name overlap, NOT "the same fact" (v2's claim was
  the phase-mixing error the doctrine warns against); (b) per F2 the assembler
  owns vocabulary — a canonical nav predicate/const belongs in OGAR; (c) SPO
  emission from an invocation is already `ogar-emitter::emit_action_invocation`
  (OGAR-side, a separate runtime/codegen phase). So the a2ui-rs lowering PRODUCES
  VALUES (`ActionInvocation` / `NavWitness`) and stops. Any SPO recording is the
  downstream OGAR-side hop the plan already fences (out of #209 scope).
- **No runtime verbs.** v3 scrubs "stamps / lands / sends" from the lowering
  description (firewall 3.ii FIX). The fn's sole output is its return value.
- **OGAR follow-up (non-blocking, filed):** define a canonical `nav_witnessed`
  vocabulary term (predicate/const) ONCE in OGAR, reconciled with
  `ogar-emitter`'s existing gate, so any future SPO framing on either side
  references one source. a2ui-rs consumes it if/when it lands; it does not
  pre-mint it.
- **Golden test (SoC-safe):** a2ui-rs's own golden = SYNTHETIC `KlickwegEdge`s →
  deterministic `ActionInvocation`/`NavWitness` digest, dev-only, corpus-free
  (F3(c)). #209's "harvested ≡ live" parity against the REAL MedCare golden
  (`klickwege-digest-v2.txt`) is exercised MEDCARE-SIDE (its fixture, its repo) —
  a2ui-rs never reads it. Both halves together satisfy #209 acceptance without a
  piggyback.

## PART D — PRE-REGISTERED GATES (v3)
- G1. `cargo +1.95.0 test` green across a2ui-rs; the lowering golden passes.
- G2. `cargo +1.95.0 clippy --all-targets -- -D warnings` clean: native, `wgpu`
      feature (a2ui-paint), wasm target.
- G3. No-piggyback re-run (AUGMENTED, firewall): zero `medcare-*` in any a2ui-rs
      Cargo.toml/lock; no consumer name in any PUBLIC (non-test) surface; AND once
      wgpu lands, `cargo tree -p a2ui-paint` shows no forbidden crate
      (`medcare-*` / `lance-graph-engine` / `lance-graph-planner`).
- G4. `lower_action_fire`/`lower_screen_jump` unit-testable with nothing running
      (F1); no `use` of any runtime/engine/SoA/mailbox symbol in the module.
- G5. (VERBATIM STRICT — A5 overclaim fix, no "per consumer" reinterpretation) A
      SECOND synthetic (non-MedCare) `ClassView` + codebook renders + paints +
      lowers through the identical client with zero client-side code change — a
      TEST, not a claim. (The one-time resolved-surface accessor is layer-(b)
      construction shared by all consumers; the test is the falsifier that proves
      no consumer forces a change. Run it; do not assert it.)
- G6. Data-identity (not layout): a test asserts `render_field_view` and `paint`
      consume the identical `&[FieldView]`/`&[ActionRef]` slice — one resolution,
      two renderers. Layout divergence is expected (renderer-internal), not tested.
- G7. `a2ui-paint` deps: no a2ui-wasm, no ogar-vocab, no consumer crate (a Cargo
      manifest assertion / the no-piggyback grep extended to the new crate).

## PART E — MANDATORY vs FOLLOW-UP (savant 5b, corrected for v3)
MANDATORY (this arc): new `a2ui-paint` crate + workspace row; the resolved-surface
accessor on FieldviewClient; ogar-vocab dev→normal in a2ui-server; the `lowering`
module + re-export; the synthetic golden; plan "Remaining" lines + CLAUDE.md status.
FOLLOW-UP (separate, none touch a2ui-rs client code): OGAR canonical `nav_witnessed`
vocabulary term; MedCare-side harvested≡live parity test; full corpus P-REHOST;
GPU-raster/browser e2e (behind the wgpu feature).
