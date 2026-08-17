# LATEST_STATE — a2ui-rs

> Mutable "what exists now." Cold-start map for a new session. Update this file
> in the SAME PR as any change that alters the inventory or the active-work
> list — see `CLAUDE.md`'s repo-wide conventions this board mirrors (pattern
> borrowed from `lance-graph`'s `.claude/board/` triple-ledger, see
> `PR_ARC_INVENTORY.md` header for the pointer). This file is **mutable** —
> unlike `PR_ARC_INVENTORY.md` / `EPIPHANIES.md`, it is edited in place, not
> prepended.
>
> Last reconciled: 2026-08-17, against `origin/main` @ `9869e23` (PR #40 merge)
> and `CLAUDE.md` as it stood that day. If this file and `CLAUDE.md`'s own
> status table disagree, treat `CLAUDE.md` as more likely current (it is read
> every session and gets touched more often) and fix this file to match, not
> the reverse.

## What this repo is (one line)

**a2ui-rs is the OGAR render target for screen addressing** — a Rust
reimplementation of `AdaWorldAPI/A2UI`. Thesis: *don't push pixels — address
the screen.* Down the wire: `NodeDelta` (16-byte GUID key + wide field mask +
ClassView-carved LE values). Up the wire: `ActionInvoke` (behavior by ordinal
address, never an inline handler). Full charter + boundaries: `CLAUDE.md`
(read that file in full before touching design — this board does not restate
its iron rules T1/T2/T3).

## Workspace crate inventory

Nine members in the root `Cargo.toml`. Role column condensed from each crate's
own `Cargo.toml` `description` / `lib.rs` module doc — read the crate itself
for the full story, this is a locator, not a spec.

| crate | role (one line) |
|---|---|
| `a2ui-core` | Re-exports `ogar-a2ui-frame` (W1 LE wire types: `NodeDelta`/`ActionInvoke`). The frame types themselves live upstream in OGAR; this crate is the consuming seed. |
| `a2ui-server` | The W2/W5 service tier: RBAC-projects a class surface (`WideFieldMask ∩ role`, fail-closed) *before* framing, emits `NodeDelta` + askama fieldview, resolves `ActionInvoke` up by ordinal, carries it over an `ogar-encryption` sealed session (`DesktopSession` + Klickwege edges). Also owns the `#209` lowering (`lower_action_fire`, `lower_screen_jump`, pure compile-time fns). |
| `a2ui-wasm` | The W3 fieldview client: the browser as thin client. Ingests `NodeDelta` LE bytes zero-copy, resolves `key → ClassView → template`, renders via `ogar-render-askama::render_field_view`, sends actions up by ordinal. Has `resolved_fields`/`resolved_actions` (one resolution, two renderers) and `resolve_nested` (L1/L2 drill-down by address). |
| `a2ui-paint` | The consumer-agnostic **paint tier**: renders the same resolved `&[FieldView]`/`&[ActionRef]` surface as askama does, but as an adaptive 2-D layout (mobile/desktop `DeviceClass`) with hit-test → ordinal → `ActionInvoke`. Multiple `Skin`s over one surface: `Form`, `Flow`, `Grid`, `Tile` (the map/geo skin, reads `position` as a `rail*2`/`rail*2+1` coordinate pair). GPU raster behind an optional `wgpu` feature. |
| `a2ui-paint-web` | Railway-deployable demo binary of the paint tier: one resolved surface, many skins, clicks answered by ordinal address. No library surface of its own — a demo/deployment shell. |
| `a2ui-layout` | Shared, zero-dependency UX zone + pixel-budget vocabulary (fixed arrangement, computed budgets, address-driven modes — "the Moca principle: the address arranges the surface, never the template"). Meant to be agreed on by server, wasm client, and build scripts alike. Style-guide doc: `docs/UX-ZONES-AND-BUDGETS.md`. |
| `a2ui-graph` | The GPU renderer of the node/edge **field** (distinct from the FieldView node-preview renderer): instanced SDF rings + indexed line-list edges over `wgpu` (WebGPU/WebGL2), fed zero-copy from graph-ABI v3 LE lanes. Division of labor operator-ruled 2026-08-14 — see its own module doc for why it's a separate crate from `a2ui-paint`. Browser build path documented in `docs/WASM-INTEGRATION.md`. |
| `a2ui-solid` | Parametric solids as addressed objects: a closed CSG vocabulary whose parameters are `u8:u8` rails on the 12-byte content-blind V3 facet, evaluated as a signed distance field, meshed watertight for printing. "Don't push meshes — address the solid." |
| `a2ui-solid-web` | Railway-deployable demo of the solid tier: a parametric part addressed by six `u8:u8` rails, projected to SVG server-side, exported as STL, edited by `NodeDelta`. The wire carries 12 bytes; the mesh never crosses it. Demo shell, no library surface. |

## Active work (from the most recent handovers/plans)

**Caveat, honestly stated:** the two files in `.claude/handovers/` are both
dated **2026-07-15/16** — over a month stale relative to `CLAUDE.md`'s own
status table (which documents work through **2026-08-14**: `a2ui-layout`,
`a2ui-graph`, `Skin::Tile`, the wgpu fork migration, `a2ui-solid*`). None of
that later work has its own handover doc. So:

- The **2026-07-16 handover**
  (`2026-07-16-a2ui-arc-current-state.md`) is the last *written-down* queue,
  and it is stale — its own §6 "current queue" predates `a2ui-paint`'s
  `Skin::Tile`, the whole `a2ui-layout`/`a2ui-graph`/`a2ui-solid*` crates, and
  the wgpu-fork migration (PRs in the #24–#40 range). Do not treat its queue
  as current without re-verifying against `git log` and `CLAUDE.md` first —
  this is the exact mistake that handover itself was written to correct about
  its *own* predecessor (see its §4/§5 "corrections" pattern).
- `CLAUDE.md`'s own status table (crate table + "Layout + status" section) is
  the best available cold-start source for **what has shipped**, because it is
  the file every session reads first and the one most recently touched
  (2026-08-14 entries for `Skin::Tile`, `a2ui-layout`, the wgpu fork).
- **What appears to be in flight right now** (inferred from `CLAUDE.md`'s "The
  killer probe — P-REHOST" section, still phrased as the gate nothing scales
  past): full-corpus **P-REHOST** — re-render one *real* harvested MedCare
  screen end-to-end (162 `CompiledClass` / 2,748 `ActionDef`s) and fire one
  harvested `ActionDef` round-trip, with zero WinForms. `CLAUDE.md` states "No
  wave scales out before P-REHOST is green" and does not report it green.
  **Unclear — needs a session to establish** whether P-REHOST has since gone
  green; no board entry or handover confirms either way.
- **Open, explicitly flagged in `CLAUDE.md`:** the `WideFieldMask` permit-all
  identity choice (`WideFieldMask::ALL` vs `full_for(field_count)` as the RBAC
  default) — gated on an upstream `lance-graph-contract` retype PR. Also
  seeded in `TECH_DEBT.md`.
- **Recent commit-log shape** (last ~20 commits before this board's creation,
  PR #24 through #40): `a2ui-solid`/`a2ui-solid-web` (CAD POC), `a2ui-graph`
  (GPU field renderer + WebGL2 canvas surface + wasm target + JS surface +
  browser diagnostics + frame-wake doc), `a2ui-layout` (zones/budgets
  substrate), the `wgpu` fork migration (workspace-wide, no version pins), and
  a Railway build-unlock fix (dropped `--locked` + stopped shipping
  `Cargo.lock` in the two Dockerfiles). **Unclear** whether any of this has a
  successor in flight beyond what's visible in the commit log — no plan file
  sequences the `a2ui-graph`/`a2ui-layout`/`a2ui-solid*` work the way
  `a2ui-screen-addressing-v1.md` sequences W1–W5; **needs a session to
  establish** whether such a plan exists elsewhere or should be written.

## Deferred / not yet scheduled (explicitly marked upstream as such)

- `.claude/plans/projectional-knowledge-editor-v1.md` self-labels **"Status:
  DIRECTION (not yet scheduled work)"** — Word/Excel/CAD-as-ClassView-
  projections vision. Grid/Spatial/Graph skins are named there as future;
  `Grid`/`Tile` have since shipped in `a2ui-paint` per `CLAUDE.md`, narrowing
  the gap but not closing the doc's remaining scope (collaborative editing,
  validation ownership, write-side `SetField` operations are still open
  questions per that plan).
- OGAR-side dependencies named as open in the 2026-07-16 handover (OGAR issue
  #210 canonical `nav_witnessed` vocab term; OGAR PR #211 region-grammar →
  nested ClassViews) — **unclear** whether either has since landed; this board
  does not have OGAR-repo access baked in, so treat both as open until a
  session checks OGAR directly.

## Read order for a new session

Unchanged from `CLAUDE.md`'s own "Session start — mandatory reads": this file
(`CLAUDE.md`), then `.claude/plans/a2ui-screen-addressing-v1.md`, then the OGAR
charter if touching design. This board (`LATEST_STATE.md` +
`PR_ARC_INVENTORY.md` + `INTEGRATION_PLANS.md` + `TECH_DEBT.md` +
`EPIPHANIES.md` + `AGENT_LOG.md`) is a supplementary cold-start layer, read
after those three, not instead of them.
