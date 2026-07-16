# HANDOVER — a2ui-rs screen-addressing arc: current state (2026-07-16)

> **Supersedes the *operational queue* of `2026-07-15-council-ratified-nesting-and-projectional-vision.md`.**
> That handover's "Next actions 1–5" are now **archival** — all five shipped
> (PR #9, real wgpu in PR #11). Keep the July-15 doc as the **decision /
> council record** (its 5+3 reasoning and the ratified design are still valid);
> do **not** use its action list as the queue. This doc is the current queue.
>
> **Verified**, not narrated: every fact below was checked against ground truth
> (git `origin/main`, GitHub PR/issue state, source at file:line) on 2026-07-16.
> Where the prior reconciliation drifted, this doc corrects it (§4).

## 1. Verified state (the numbers a new session needs)

- **a2ui-rs `main` tip:** `96cab313e4b1af1768c0855fb3450b61ece621b8` — the merge
  commit of **PR #11**, merged 2026-07-16T00:45:09Z.
- **Merged since the July-15 handover** (all on main):
  - **PR #9** (merge `d57f7a1`) — reusable-client build: the `a2ui-paint` crate,
    resolved-surface accessor, `#209` lowering, L1/L2 nesting, Form/Flow skins.
  - **PR #10** (merge `9a4b5e5`) — dedup `hex16` into one `pub(crate)` helper in
    `a2ui-server` (PR #9 had introduced a same-crate copy).
  - **PR #11** (merge `96cab31`) — replaced the `gpu` **N2 placeholder** with a
    real headless wgpu backend.
- **Working tree (the thing GitHub cannot prove):** on 2026-07-16 the local
  `/home/user/a2ui-rs` tree is **CLEAN** — `git status --short` returns nothing,
  zero uncommitted/untracked files. (The July-15 "tree clean" claim was for a
  *different, earlier* session; this is a fresh local confirmation.)
- **Toolchain:** `cargo +1.95.0` (edition 2024). Test attrs on main:
  a2ui-server **38**, a2ui-wasm **10**, a2ui-paint **6**.

## 2. What actually ships on main (source-verified)

| crate | shipped surface | evidence |
|---|---|---|
| `a2ui-core` | W1 frame re-export (`NodeDelta`/`ActionInvoke`, LE wire) | — |
| `a2ui-server` | RBAC-project → `NodeDelta` + askama fieldview; `ActionInvoke` up by ordinal; `ogar-encryption` sealed `DesktopSession` + Klickwege; `json` membrane feature-gated; **`lowering`**: `lower_action_fire(&KlickwegEdge,u16) -> ogar_vocab::ActionInvocation` (`subject = ActionSubject::User`), `lower_screen_jump -> NavWitness` (pure value `{from_concept,predicate,seq}`, no SPO stamp, no const-mint) | `lowering.rs:50/67/79`, goldens `:108/:147` |
| `a2ui-wasm` | fieldview client; **`resolved_fields`/`resolved_actions`** accessor (one resolution, two renderers); **`resolve_nested`** (L1/L2 drill-down, `NestedSurface`, cycle-safe `max_depth`); wasm32 green | `lib.rs:340/350/377`, `NestedSurface :165` |
| `a2ui-paint` | consumer-agnostic paint tier; adaptive `DeviceClass` (mobile/desktop) layout from `position`/`ordinal`; hit-test → ordinal → `ActionInvoke` (T2); **`Skin::{Form, Flow}`**; **real wgpu `GpuPainter`** (`to_ndc_vertices` rect→2-triangle geometry → vertex buffer → WGSL pipeline → offscreen `Rgba8UnormSrgb` texture); `#![forbid(unsafe_code)]` intact | `lib.rs:498 GpuPainter`, `:587 render_to_texture`, `:460 to_ndc_vertices`, `Skin :114-124` |

**RBAC is real and fail-closed:** `project_surface(&WideFieldMask, &WideFieldMask)
-> Result<WideFieldMask, RbacError>` — empty role → `NoRoleGrant`; disjoint
role∩surface → `EmptyProjection`; exercised past position 64 (test asserts a
field at position 133); runs **before** framing (`render_stream::project_node`).
`full_for` is documented as a *render* convenience, never an RBAC fallback.
(`project.rs:70-80`, test `wide_projection_covers_positions_past_64 :146`.)

## 3. Traps still hold by construction

No JSON on the hot path (T3 — `to_le_bytes`/`from_le_bytes` are the wire; serde
only behind the `json` membrane feature). No widget vocabulary (T1 — render is
askama + ClassView + `WideFieldMask`; a "skin" is a *renderer*, not an enum). No
behavior on the surface (T2 — actions travel as ordinal addresses; a click
hit-test builds `ActionInvoke`, never a handler).

## 4. Corrections to the July-15 reconciliation (do not propagate the errors)

1. **The permit-all mask identity is STILL OPEN — not resolved.** Only the
   fail-closed *constraint* (empty role + disjoint both refused) is coded. The
   *identity* choice — `WideFieldMask::ALL` (explicit all-fields) vs default
   `full_for(field_count)` — remains undecided, deferred to the
   `lance-graph-contract` `WideFieldMask` retype PR. "The projection takes two
   masks and refuses empty grants" is true but does **not** close this question.
2. **There is NO "Stockfish DecisionEpisodeV1" work.** Verified absent: no such
   type, plan, or board entry in a2ui-rs / lance-graph / stockfish-rs; the word
   "episode" does not appear in stockfish-rs at all. The "UI interactions AND
   chess decisions as addressed episodes in one Lance/AriGraph layer" dovetail is
   **aspirational** — it would have to be *authored*. The nearest planned
   substrate is `EpisodicWitness64` (episode-as-SoA-tenant), which is gated
   offline on cognitive-shader-driver's `MailboxSoA<N>` and is **not yet a code
   symbol**. Do not cite a Stockfish episode handover in a read order.
3. **`A2UI-SCREEN-ADDRESSING-PROPOSAL.md` is a MERGED PROPOSAL, not a ratified
   charter.** #204/#205 merged and the file is on OGAR main, but it is
   self-labeled "PROPOSAL (council pending)" and graded `[S]` in DISCOVERY-MAP
   `D-A2UI-SCREEN-ADDRESSING`. Council verification + P-REHOST (C4) are not done.
   Treat it as authoritative *direction*, not final. (Aside: PR #205's original
   additive `field_mask_wide` seam was **superseded** by operator correction
   `8cf0900` — retype `field_mask` to `WideFieldMask` in place, **no** parallel
   method.)
4. **Klickwege→AriGraph is an OGAR-side hop, not "Lance episode persistence."**
   The a2ui plan's wording is "wire the drained Klickwege edges into the live
   AriGraph SPO store (an OGAR-side hop)" — no "episode", and the landing is
   OGAR's responsibility (assembler-vs-storage doctrine, #691→#208). Before
   treating it as "a lance-graph hop away," verify an OGAR-side landing API/issue
   (successor to #208) actually exists.

## 5. Stale docs — what NOT to trust for status

- **`.claude/plans/a2ui-screen-addressing-v1.md`** — status drift: W1 still
  labels OGAR #206 "open" (it merged; deps flipped to OGAR main `df7331e`); W3
  still lists the "canvas/webgpu paint" as *Remaining* (shipped — a2ui-paint +
  real wgpu); it never records the `a2ui-paint` crate nor PRs #9/#10/#11. (Its
  "permit-all identity OPEN" line is **correct** — see §4.1.) *The wave plan is
  usable for the boundary model + traps, but its per-wave status is stale.*
- **`.claude/handovers/2026-07-15-council-ratified-nesting-and-projectional-vision.md`**
  — "Next actions 1–5" all shipped; status table omits `a2ui-paint` and gives
  stale test counts. Keep for the council reasoning; ignore its queue.
- **`CLAUDE.md`** — `a2ui-paint` row says "5 tests" + tags GPU raster "N2"; PR
  #11 closed the N2 stub and the wgpu test set is now 6. One step stale (PR #11
  was a 1-file diff that didn't touch CLAUDE.md).
- **`.claude/plans/projectional-knowledge-editor-v1.md`** — **valid**; self-labels
  `Status: DIRECTION (not scheduled work)`. Form + Flow begun; Grid/Spatial/Graph
  future. Do not read it as a queue.

## 6. The current queue (grounded, ordered)

1. **Full-corpus P-REHOST** (plan W4 / proposal C4) — re-host one **real
   harvested MedCare screen** (162 CompiledClass / 2,748 ActionDefs vendored in
   MedCare-rs), not the harvested-*shape* lite stand-in that is green today. This
   is the gate before any scale-out, and the thing that promotes the proposal
   from `[S]`. **Corpus-free on the a2ui side by design** — the corpus-touching
   test is MedCare-side (its fixture, its repo).
2. **Klickwege → live AriGraph SPO store** (plan W5) — the OGAR-side hop (§4.4);
   confirm the OGAR landing API/issue exists first.
3. **a2ui-paint last mile** — text/glyph raster (textured quad over the addressed
   rects) + windowed surface/present (the `unsafe` surface boundary, deliberately
   kept OUT of the forbid-unsafe crate); plus an a2ui-wasm browser e2e / canvas
   harness (plan W3). The GPU *rect* raster is done; typography and glass are not.
4. **Write-side addressed operations** — begin with `SetField{key,field,value}`
   as the write mirror of `ActionInvoke` (projectional-editor thesis). Open
   sub-questions per `projectional-knowledge-editor-v1.md`: relation/edge edits,
   validation ownership, collaborative editing, template/skin registry.
5. **OGAR #210** — canonical `nav_witnessed` vocabulary term (OGAR-owned, F2:
   assembler owns vocabulary). Still OPEN; unimplemented — no `NAV_WITNESSED`
   const in ogar-vocab/ogar-emitter yet. Does **not** block the shipped a2ui
   lowering (which correctly emits a `NavWitness` value and stops).
6. **OGAR PR #211 (region grammar → nested ClassViews)** — currently OPEN; when
   it merges it adds only a handover doc. Its live seam for this arc: harvested
   Odoo layout facts (`docked_at` / `tab_order` / `opens_popup` /
   `odoo_regions.conf`, region → position-in-parent) feed the nested-ClassView
   projection, **WIDE masks only**. ⚠ The authoritative Odoo forward-plan doc is
   **NOT on odoo-rs main** — it sits on branch
   `claude/odoo-rs-v3-ogar-transpile-nwriny` (`593b9c2`) pending odoo-rs PR #37.
   Read the branch, not main.
7. **Permit-all mask identity decision** — lands with the `lance-graph-contract`
   `WideFieldMask` retype (§4.1).
8. **Future skins** — Grid (spreadsheet), Spatial (CAD), Graph (native) — are
   `projectional-knowledge-editor-v1.md` DIRECTION, not scheduled. *If* the arc
   genuinely wants a chess/UI "addressed episodes" unification, that concept has
   no artifact anywhere and must be authored first (§4.2).

## 7. Read order for a new session (corrected)

1. `a2ui-rs/CLAUDE.md` — current shipped status (modulo the stale `a2ui-paint`
   test-count/N2 line, §5).
2. **This handover** — the current queue + the corrections.
3. OGAR `docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md` — the merged proposal /
   direction (not a ratified charter, §4.3).
4. `.claude/plans/projectional-knowledge-editor-v1.md` — forward editor
   architecture (DIRECTION).
5. PRs **#9** and **#11** — what actually landed after the July-15 handover.
6. OGAR issue **#210** and PR **#211** — the two live OGAR seams (§6.5, §6.6).
7. *(No Stockfish "DecisionEpisodeV1" — it does not exist, §4.2.)*

## 8. Caveats (do not relearn)

- **Branch is post-merge:** PRs #9/#10/#11 are merged; follow-up work is a fresh
  branch off latest `main` (`git checkout -B <branch> origin/main`) — merged PRs
  cannot track new commits.
- **Token-safe push:** never `git remote set-url` a token URL; push to a one-shot
  `https://x-access-token:$GHT@github.com/…` argument, then fetch to sync
  `origin/*`. Strip literal quotes from `$GH_TOKEN`.
- **git identity:** `git config user.email noreply@anthropic.com &&
  git config user.name Claude` before committing.
- **a2ui-rs deps allow-list:** OGAR-shared crates + `lance-graph-contract` +
  std/getrandom/serde_json/wasm-bindgen/wgpu. Any `medcare-*` dep is a fail.
  (`a2ui-paint` direct deps: `ogar-render-askama` + `a2ui-core` + optional
  `wgpu`; `ogar-vocab` appears only *transitively* via ogar-render-askama.)
- **Model policy:** main-thread Opus; Sonnet subagents for mechanical work; never
  Haiku.

---

*Provenance: reconciled + adversarially fact-checked against `origin/main`
(`96cab31`), GitHub PR/issue state, and source at file:line on 2026-07-16. Two
claims from the prior reconciliation were corrected here (permit-all identity
still open; no Stockfish DecisionEpisodeV1) and one context note added (proposal,
not ratified charter). This file is append-only — supersede it with a new dated
handover; do not edit past entries except status lines.*
