# CLAUDE.md — a2ui-rs

> Read first, every session. The repo's commits + PRs are the durable record;
> this file is the awareness that would otherwise reset with the session — what
> this is, the boundaries you don't get to renegotiate, and where the plan lives.

## What this is

**a2ui-rs is the OGAR render target for screen addressing** — a Rust reimagining
of `AdaWorldAPI/A2UI` ("agents speak UI") on the V3 substrate. The headline:
**don't push pixels — address the screen.**

- **Down** the wire (server → client): `NodeDelta { key: [u8;16], mask_words:
  Vec<u64>, values }` — a 16-byte canonical GUID, a wide changed-fields mask,
  and the changed fields' ClassView-carved LE bytes.
- **Up** the wire (client → server): `ActionInvoke { key, action_ordinal, args }`
  — behavior invoked **by address** (an ordinal into the class's `ActionDef`
  set), never an inline handler.
- The client holds the ClassView/template codebook and renders locally
  (askama, zero serialization). **The ClassView registry is the font of the
  desktop** — you don't stream glyph rasters, you reference codepoints.

The frames themselves live UPSTREAM in OGAR `ogar-a2ui-frame` (W1); this repo
consumes them and grows the service + client tiers.

## The charter (authoritative — do not re-derive)

`AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md` — merged **OGAR
#204**, security-corrected **#205**. Ledger: OGAR `docs/DISCOVERY-MAP.md`
`D-A2UI-SCREEN-ADDRESSING`. The local plan (`.claude/plans/`) sequences it;
the charter is the source of truth. If a design question isn't answered here or
in the plan, read the charter before inventing.

## Iron boundaries (the charter traps — non-negotiable)

1. **T1 — no second vocabulary.** Widget skins are ClassView *templates*
   (render side, lo-u16 world), never a new closed enum beside the doc-IR
   RegionKinds. "A new widget" is a template, never a variant.
2. **T2 — behavior never rides the surface** (the SURREAL-AST trap, UI
   edition). Actions travel by ADDRESS (`ActionDef` ordinal on the Core node).
   `onClick: <lambda>` in a component tree is the same hijack as `DEFINE EVENT`
   in DDL. Reject it.
3. **T3 — no serialization in the hot path** (the Firewall, ADR-022/023).
   `to_le_bytes`/`from_le_bytes` ARE the wire format; the wasm client ingests
   bytes zero-copy. serde/JSON/proto exist ONLY at a membrane, behind a
   feature, never server→client on the hot path.

## RBAC is real, and it is by PROJECTION

What leaves the server = `surface ∩ role` — an unauthorized field is **absent
from the wire**, not hidden client-side (pixels can't promise this). The seam:
`ClassRbac::field_mask` is being **retyped to `WideFieldMask`** (charter C1.4,
codex-P2-corrected #205) so surfaces past 64 fields are covered; the permit-all
identity (`WideFieldMask::ALL` vs default `full_for(field_count)`) is the one
open W1 decision. **Fail-closed:** a missing/narrow role mask never falls back
to "emit everything" (`full_for` is a *render* convenience, never an RBAC
fallback). RBAC happens BEFORE framing — the frame is dumb transport.

## DocIr composition grounding (OGAR #217 / #218)

The DocIr composition ruling (`AdaWorldAPI/OGAR docs/DOCIR-COMPOSITION-LAYER.md`
#217, grounded in `docs/DOCIR-COMPOSITION-GROUNDING.md` #218) is **this arc,
generalized — not a competing surface.** a2ui-rs is already the code proof:
`render_stream.rs` says *"one template projection; serve it as a living screen
or re-issue it as a document."* Ground rules that follow, so a future session
absorbs the OGAR-side widenings correctly (they are **expansions, not
rewrites**):

- **a2ui is `DocRenderer`'s fourth adapter.** A3: *"a screen and a document are
  the SAME positional projection; `DocRenderer` gains its fourth adapter."* The
  live surface (this repo) sits beside askama / Typst / the paged renderers on
  one `DocRenderer` trait — do NOT mint a parallel renderer trait. When OGAR
  renames the doctrine's `ProjectionRenderer` to `DocRenderer`, this repo
  consumes that trait.
- **The `FieldView` we consume is a fold, being widened.** `render_field_view`'s
  `FieldView` (the `ogar-render-askama` struct: `position/label/predicate/value`)
  is the **`Text`-leaf reading** of the `E-ONE-MASK-THREE-PORTS` fieldview fold.
  OGAR is widening it to a renderer-neutral `enum FieldView {Text, Badge, Table,
  ObjectSlot, Geometry, …}` (the struct becomes `FieldRow`, the `Text` payload).
  When that lands upstream, `a2ui-wasm` / `a2ui-server` / `a2ui-paint` consume
  the **enum's fold** — the widget-per-field-type dispatch the paint tier
  already does by hand (`Skin{Form,Flow}`) becomes the enum's variant match. **T1
  holds:** the enum variants are render *skins* (the fold), never a second
  closed doc-IR vocabulary beside `RegionKind`.
- **`ObjectSlot` = our nested-ClassView addressing, made explicit.** The
  `resolve_nested` L1/L2 drill-down (`a2ui-wasm`) is the ObjectSlot portal:
  `desktop → window → region → widget` is `ObjectSlot{target, class_view,
  wide_field_mask}` recursion (the A3 Klickwege brick + an `ObjectRef` +
  `ResolutionMode`). Behavior still travels by address (T2); the portal carries
  a projection, never an inline handler.
- **No code change here yet** — the composition types are OGAR-side named gaps.
  This section is the seam record so the widening is a rename-follow, not a
  re-derivation.

## Layout + status

| crate | role | status |
|---|---|---|
| `a2ui-core` | re-exports `ogar-a2ui-frame` (W1 frames) | seed (shipped) |
| `a2ui-server` | the graph desktop projection + the live session loop: RBAC-project (`WideFieldMask ∩ role`, fail-closed) → `NodeDelta` + askama fieldview down; `ActionInvoke` up by ordinal address; `ogar-encryption` sealed session transport (fresh-salt invariant); `DesktopSession` + Klickwege edges; **`lowering` (#209): `lower_action_fire`→`ActionInvocation`, `lower_screen_jump`→`NavWitness` — pure compile-time fns** | **W2 + W5 + #209 lowering shipped** (34 tests, warden COMPILE-TIME-CLEAN) |
| `a2ui-wasm` | the fieldview client — codebook + per-node facet state; ingest `NodeDelta` LE zero-copy; resolve `key → ClassView → template`; render via `ogar-render-askama::render_field_view`; actions up by ordinal; **`resolved_fields`/`resolved_actions` accessor (one surface, two renderers); `resolve_nested` (L1/L2 drill-down by address)** | **W3 + accessor + nesting** (10 tests; `wasm32` green) |
| `a2ui-paint` | the **paint tier** — consumer-agnostic renderer of the resolved surface: adaptive 2-D layout (`DeviceClass` mobile/desktop) from `position`/`ordinal` addresses; hit-test→ordinal→`ActionInvoke` (T2); **`Skin{Form,Flow,Grid,Tile}` — many skins, one surface** (projectional editor); GPU raster behind optional `wgpu` (WebGPU+WebGL2, N2) | **shipped** (14 tests; `wgpu` feature clippy-green) |

### `Skin::Tile` — the map topcoat (2026-08-06)

The spatial skin, and the one where **placement stops coming from `position`**.
`Form`/`Flow` place by iteration order; `Grid` reads `position` as a row-major
cell; `Tile` reads a *geographic* coordinate **out of the surface's own
fields** — the two mask positions `rail*2` / `rail*2 + 1`.

That is not a private convention. The 12-byte V3 facet register is carved
`6×(u8:u8)` (`le-contract.md` §3), each rail is a `256×256` centroid tile, and
the canon binds the axes per domain naming OSM explicitly — *"OSM: literal
x/y"*. `ogar_osm::GEO_V3_FACET` (OGAR #249, merged) is the table that says so:
rails 0–3 are the HHTL tiers heel→leaf, so **`rail` is also the zoom choice**.
A semantic domain binds the same rail to a PQ subspace pair, which is why the
skin takes `rail` as a parameter rather than hardcoding geo.

Three things a future session should not re-derive:

- **The y flip is load-bearing.** TMS y increases *north*; screen y increases
  *down*. Omitting `1.0 - fy` mirrors the map about its horizontal axis — which
  still looks like a map, so it ships. Pinned two-sided and mutation-verified.
- **One surface is one marker.** A trace or a viewport is N surfaces → N
  layouts, merged by the consumer. This needs **no new API**: 
  `click_to_action_frame` takes the key as an *argument*, so a consumer holds
  `Vec<([u8;16], PaintLayout)>` and each up-frame is addressed to its own row.
- **No coordinate ⇒ fall back to `Form`,** never `(0, 0)`. Placing unplaceable
  surfaces at the origin silently stacks them in one corner and reads as a
  rendering bug.

T1 holds throughout: no new surface type, no widget vocabulary, no geo-specific
field kind — the same `&[FieldView]` every other skin consumes.

Upstream deps are **all on `branch = "main"`** now (float-then-flip complete):
`a2ui-core`'s `ogar-a2ui-frame` (W1, OGAR #206) and `a2ui-server`'s
`ogar-render-askama` (the fieldview brick, OGAR #207) + `ogar-encryption` — one
OGAR source. The render half is the upstream OGAR brick
`ogar-render-askama::field_view` (`render_field_view`).

## The killer probe — P-REHOST (the arc's proof)

Re-render ONE harvested MedCare screen from its `CompiledClass` × ClassView ×
askama and fire one harvested `ActionDef` round-trip: **harvest the app →
re-render the app, no WinForms.** Everything it needs exists (Klickwege golden
v2, 162 CompiledClass / 2,748 ActionDefs, `ogar-render-askama`). No wave
scales out before P-REHOST is green.

## Session start — mandatory reads

1. This file.
2. `.claude/plans/a2ui-screen-addressing-v1.md` — the wave plan + gates.
3. The charter (OGAR `docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`) if touching
   design, not just wiring.

## Model policy

- **Main thread: Opus** — architecture, the transcode judgment, review.
- **Sonnet subagents:** mechanical scaffolding (crate stubs, grep-rewrite,
  bookkeeping). **Never Haiku for synthesis.**

## Build

```sh
cargo test                    # workspace = edition 2024 / rust 1.98.1 (matches OGAR)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The pin lives in `rust-toolchain.toml` — a bare `cargo` in this checkout
resolves to it, so these invocations deliberately carry no `+<version>`
prefix: that prefix would be a second source of truth for the toolchain
version, exactly what `rust-toolchain.toml` exists to make unnecessary.

## Git — token-safe push (hard-won lesson, do not relearn)

Outbound is a policy proxy. The `local_proxy@127.0.0.1:<port>` remote allows
FETCH but DENIES push. **Never `git remote set-url` a token URL** — it persists
the PAT in `.git/config` (exposed via `git remote -v`, logs, copied worktrees;
codex P2 on OGAR #200). Push to a ONE-SHOT token URL argument, leaving the
tracked remote token-free:

```sh
GHT="${GH_TOKEN%\"}"; GHT="${GHT#\"}"   # strip the env var's literal quotes
git push "https://x-access-token:$GHT@github.com/AdaWorldAPI/a2ui-rs.git" HEAD:refs/heads/<branch>
# then sync the local tracking ref (a one-shot push does not update origin/*):
git fetch "https://x-access-token:$GHT@github.com/AdaWorldAPI/a2ui-rs.git" \
  "refs/heads/<branch>:refs/remotes/origin/<branch>"
git config branch.<branch>.remote origin
git config branch.<branch>.merge refs/heads/<branch>
```

PR creation: MCP `mcp__github__create_pull_request` (a2ui-rs is in scope), or
direct REST via `bash` curl if not. **All work goes through PRs** (the seed's
initial commit was the one exception — an empty repo has no PR base).

## Board hygiene

The durable ledger for this arc is OGAR `docs/DISCOVERY-MAP.md`
`D-A2UI-SCREEN-ADDRESSING` (the Core change lives there). a2ui-rs commits +
this file + the plan are the consumer-side record. When a wave lands, append
its status to the ledger entry (append-only) in the same PR arc.
