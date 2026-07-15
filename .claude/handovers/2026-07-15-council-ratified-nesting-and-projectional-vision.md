# Handover — 2026-07-15 — council ratified, nesting + projectional-editor vision

> Token-wall handover for the next session. This file is the awareness that
> would otherwise reset. Read it, then `.claude/plans/a2ui-screen-addressing-v1.md`
> (the wave plan) and `.claude/plans/projectional-knowledge-editor-v1.md` (the
> forward vision), then continue at **§ Next actions**.

## Where the arc stands (all merged, main)

| crate | role | status |
|---|---|---|
| `a2ui-core` | re-exports `ogar-a2ui-frame` W1 frames | shipped |
| `a2ui-server` | RBAC-project (`WideFieldMask ∩ role`, fail-closed) → `NodeDelta` + askama fieldview down; `ActionInvoke` up by ordinal; `ogar-encryption` sealed transport; `DesktopSession` + Klickwege edges; `json` membrane (feature-gated, edge-only) | W2+W5 shipped (29 tests) |
| `a2ui-wasm` | fieldview client: codebook + per-node facet state, ingest `NodeDelta` LE zero-copy, resolve `key→ClassView→template`, render via `ogar_render_askama::render_field_view`, act by ordinal | W3 core (7 tests, wasm32 green) |

**JSON was never the protocol** (there was no JSON A2UI component protocol to
rip out) — the render is askama + ClassView + WideFieldMask by construction; the
only JSON is the T3-sanctioned `json` membrane (feature off by default). **No
widget vocabulary** exists (trap T1) — only fieldview template addressing.

## The 5+3 council result (this session) — RATIFIED

Council target: the reusable-client SoC before building (i) the wgpu paint tier
and (ii) the OGAR #209 Klickwege lowering. Full ratified design:
**`.claude/plans/a2ui-reusable-client-and-209-lowering-v3.md`** (copied in-repo
from the council's draft v3). Summary of what the council changed vs the initial
SPEC:

- **Phase 1 (5 savants):** SoC clean (zero consumer types in any public a2ui
  signature); all inventory CODED. Surfaced 2 load-bearing gaps.
- **Phase 3 (3 brutal reviewers): 2 BLOCKs** (stricter verdict wins), both
  resolved by adopting the reviewers' own recommendation:
  - **BLOCK-1 (A1): paint is a SEPARATE crate `a2ui-paint`, NOT a module in
    a2ui-wasm.** The "fold into a2ui-wasm" idea rested on a false premise —
    `FieldView`/`ActionRef` are OGAR-owned (`ogar-render-askama`), so a standalone
    crate takes them the same way (plain `&[FieldView]`/`&[ActionRef]` args),
    depending only on `ogar-render-askama` + `wgpu`. Also: a2ui-wasm's crate-wide
    `#![forbid(unsafe_code)]` would make wgpu surface/window glue uncompilable.
  - **BLOCK-2 (A2): `lower_klickweg` is PURE `KlickwegEdge → ActionInvocation`,
    no SPO-stamp, no local `nav_witnessed` const.** OGAR's `nav_witnessed` is a
    codegen-time `BTreeSet<String>` gate (`ogar-emitter/do_adapter.rs:46`) — a
    different shape/phase, only a name overlap; calling them "one fact" erased the
    compile/runtime split. SPO emission is already OGAR's `emit_action_invocation`
    (`ogar-emitter/lib.rs:774`). The nav vocabulary is an OGAR follow-up (F2:
    assembler owns vocabulary).
- **Cross-checked against the REAL OGAR #209** (read this session): two lowerings
  (`KlickwegEdge → ActionInvocation` action-fire; `KlickwegEdge → NavWitness`
  screen-jump), `subject: ActionSubject::User` for a click (documented, vs the
  `System` default), golden parity harvested≡live. #209 body confirms it is "a
  compile-time codegen seam, nothing else."

**layer-boundary-warden (Task #15) was INTERRUPTED** mid-run (model switch), not
completed. It is a belt-and-suspenders re-check: §3.ii compile-time-cleanliness is
already confirmed by Savant 4 (door-knock test not tripped, right home), all 3
Phase-3 reviewers (PASS on 3.ii), and #209's own framing. **Quick re-run item**:
`Agent(layer-boundary-warden)` on the v3 §3.ii lowering — expect COMPILE-TIME-CLEAN.

## Ratified implementation (the next build) — 3 deliverables + 7 gates

1. **`a2ui-paint`** (new workspace crate, consumer-agnostic). Deps:
   `ogar-render-askama` (types), `wgpu`, `a2ui-core`, `lance-graph-contract`. NO
   a2ui-wasm, NO ogar-vocab, NO consumer dep. API: `paint(fields: &[FieldView],
   actions: &[ActionRef], …)`. Layout (1-D `position` → 2-D coords) is
   paint-INTERNAL (renderer concern, T1-fine). wgpu backends WebGPU + WebGL2, so
   one crate covers both; wasm+ndarray stays the compute lane.
2. **Resolved-surface accessor** on `FieldviewClient` (a2ui-wasm): refactor
   `apply_node_delta` to compute-then-store the resolved `Vec<FieldView>` +
   `Vec<ActionRef>`, expose `resolved_fields(&key)` / `resolved_actions(&key)`.
   Both `render_field_view` (HTML) and `paint` (pixels) read the identical stored
   slice. (T3-creep guard: if these types ever gain serde, feature-gate it like
   `Frame`/`NodeDelta`.)
3. **`a2ui_server::lowering`** (ogar-vocab dev→normal): `lower_action_fire(&edge,
   app_prefix) -> ActionInvocation` (via `ActionInvocation::new` + `subject=User`),
   `lower_screen_jump(&edge) -> NavWitness` (a plain value = witnessed concept
   name; no SPO, no const-mint). SYNTHETIC golden (corpus-free); the real
   harvested≡live parity is a MedCare-side test.

Gates: G1 `cargo +1.95.0 test`; G2 clippy native+wgpu+wasm; G3 no-piggyback
(+`cargo tree -p a2ui-paint` no forbidden crate once wgpu lands); G4 lowering
unit-testable with nothing running; G5 second-synthetic-consumer test (verbatim
strict); G6 data-identity (render & paint read the same slice); G7 a2ui-paint deps
assertion.

## The nesting direction (operator, this session) — the next wave after paint

Today the client renders ONE flat surface per node; Klickwege navigates BETWEEN
screens. **Nesting (L1 screen / L2++ drill-down / leaf = interactive preset) lives
in the ClassView/codebook layer, NEVER on the wire.** A "screen" is a class whose
fields resolve to CHILD NODE KEYS instead of scalars; the client composes them by
address (recursive `key→ClassView→template` walk); each drill IS a Klickweg edge
(existing closed vocabulary). The wire stays a flat stream of addressed
`NodeDelta`s (T3 untouched); composition is pure template resolution. The ratified
resolved-surface accessor is per-key, so nesting layers on top as a tree walk with
zero change to it. **T1-clean by construction** — no new frame kind, no new
vocabulary. Adaptive sizing / mobile-vs-PC / wgpu-vs-WebGL are paint-INTERNAL
(layout derived per renderer from the same positions), per the ratified A1b ruling.

## The projectional-knowledge-editor vision (operator, this session)

Captured in full at **`.claude/plans/projectional-knowledge-editor-v1.md`**. One
line: **Word, Excel, and CAD become three ClassView projections (interaction
skins) of the same canonical OGAR object graph; every edit is emitted as an
addressed graph operation (`SetField`/edge-op/feature-op), never a
character/cell/blob mutation.** This is "document = screen" extended: document,
spreadsheet, desktop, CAD are positional projections of one graph. It maps
faithfully onto the a2ui substrate (details in that plan) — it is an EXTENSION of
the ratified design, not a pivot.

## Git / operational caveats (do not relearn)

- **Token-safe push** (a2ui-rs CLAUDE.md): NEVER `git remote set-url` a token URL
  (persists the PAT). Push to a one-shot token URL argument; then sync the local
  tracking ref. `GHT="${GH_TOKEN%\"}"; GHT="${GHT#\"}"` to strip literal quotes.
- **git identity**: `git config user.email noreply@anthropic.com && git config
  user.name Claude` before committing (the stop-hook checks this).
- `df7331e` in **OGAR** (committer `noreply@github.com`) is the already-merged
  PR #207 GitHub merge commit on origin/main — immutable, NOT ours to re-author.
  The stop-hook flags it but it must be left alone.
- **No model identifier** in any committed artifact (chat only). **No German PII.**
- a2ui-rs deps allow-list: only OGAR-shared crates (`ogar-a2ui-frame`,
  `ogar-render-askama`, `ogar-vocab`, `ogar-auth`, `ogar-encryption`) +
  `lance-graph-contract` + std/getrandom/serde_json/wasm-bindgen/wgpu. Any
  `medcare-*` dep is a fail.

## Next actions (in order)

1. (30 s) Re-run `Agent(layer-boundary-warden)` on v3 §3.ii — expect
   COMPILE-TIME-CLEAN; then the council is fully ratified.
2. Build the 3 ratified deliverables (paint crate, resolved-surface accessor,
   lowering + golden). Run gates G1-G7.
3. File the OGAR follow-up: canonical `nav_witnessed` vocabulary term (F2).
4. Nesting wave (L1/L2 slot-bearing ClassView templates + recursive client
   resolve). Independent of paint — can be pulled forward if screen composition
   is the more urgent proof.
5. Then adaptive layout / device detection inside paint; and begin the
   projectional-editor projections (start with the document/flow skin).
