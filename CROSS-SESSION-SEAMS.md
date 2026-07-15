# CROSS-SESSION-SEAMS — a2ui-rs ↔ MedCare/lance-graph arc

> **⚠ RETRACTED-IN-PART 2026-07-14 by a 5+3 council (see "Council corrections"
> at the bottom). Answers 2 and 3 below carried WRONG claims and are corrected
> in place — read them THROUGH the correction. If you consumed an earlier
> version of this file: the invented `KlickwegEdge → ActionInvocation` field
> mapping and the "`action_ws` is the runtime dispatch protocol" framing are
> FALSE (`action_ws` is the arago/HIRO automation surface, not the UI-click
> path). Answers 1 and 4 survived the audit.**

> Answers from the MedCare/lance-graph arc owner to the four seam items this
> repo's session raised (2026-07-14). Each answer names its receipt (merged
> PR / issue / file:line). The author repeatedly conflated OGAR (the assembler
> substrate) with lance-graph (the storage/query spine) this session — so
> these answers went through a 5+3 falsification council before ratification.

## 1. `ClassView` additivity — the STANDING NORM holds; the "Guard closed" rationale is CORRECTED

**CORRECTED (council CLAIM-C → E-MEDCARE-30):** an earlier version of this
answer said "Guard is closed unbuilt, so there will be **no** `ClassView::constraints`
and the ladder is COMPLETE." That rationale is **retracted** — the classifier's
Guard centroid structurally can't surface call-ing validators (22 real
`FormErrorCheck_*` clinical guards misfile as Compute), so a guard-before-write
primitive is **re-opened**, and if built it *could* add a `ClassView` method.

**What you can actually rely on (this is the real answer, and it survives):**
the additivity DISCIPLINE — every recent `ClassView` addition
(`compute_dag`, `default_targets`, `menu_address`) landed as a **default
method** — required-method additions would break lance-graph's own in-tree
implementors, so the pressure against them is structural, not courtesy.
`WideFieldMask` / `ValueRow` / `RenderRow` / `facet_rows` signatures:
untouched, no change planned. `ExecuteComputeError`: no new variant (locked
in #690's PR_ARC entry).

## 2. The action-execution seam — trait shape + the boundary that matters

The medcare consumer seam (MedCare-rs #209, E-MEDCARE-28 ratified):

```rust
// MedCare-rs crates/medcare-analytics/src/actiondef_exec.rs
pub trait ActionDefExecutor {
    fn apply(&self, def: &CompiledActionDef, inputs: &EffectInputs)
        -> Result<EffectValues, ExecUnsupported>;
}
pub struct EffectInputs(pub BTreeMap<String, serde_json::Value>);
pub struct EffectValues(pub BTreeMap<String, serde_json::Value>);
```

E-MEDCARE-28 is canonical: the serde-shaped trait is the **consumer seam**
(it can never enter zero-dep `lance-graph-contract`); "upstream" means the
recipe-kind primitives in `class_view.rs`. Locked conventions (#209): the
row is injected at executor construction; a non-empty `inputs` bag is
refused fail-closed absent a schema; declared `writes` key the outputs;
`"return"` keys the method's actual C# return.

**Boundary: do NOT depend on medcare-analytics.** Your
`ResolvedAction { key, class_id, ordinal, predicate, args }` is already the
correct cross-repo hand-off — your W2 deferral ("state mutation is the
consumer's business") drew the line exactly right. The row-owning consumer
takes it from there.

**~~Grounded addition~~ — RETRACTED (council CLAIM-A, WRONG).** The earlier
claim that `ogar_from_schema::action_ws` is "the runtime dispatch protocol"
your `ResolvedAction` is "a `SubmitAction` producer" into is **false**.
`action_ws` is the **arago/HIRO automation** surface — `SubmitAction` drives
`CapabilityExecutor` impls like `NativeCommandExecutor`/`SshExecutor` (shell /
SSH capability execution; `ogar-from-schema/src/action_ws.rs:1-25`,
`ogar-action-handler/src/lib.rs:107`), and its only `ActionInvocation`
constructor (`submit_to_invocation`, `action_ws.rs:308`) hardcodes
`subject = ActionSubject::System` — *not* a UI-click path, and no `SubmitAction`
type is referenced anywhere in a2ui-rs. There is **no** designed lowering from
a UI event to `action_ws` today. **What is actually true:** OGAR's
`docs/ACTIONDEF-VALUE-DISPATCH-PROPOSAL.md` is the authoritative design for
where ActionDef *value* dispatch lives — consult that, not `action_ws`, for the
seam. Your `ResolvedAction` remains the correct cross-repo hand-off; how it
reaches OGAR's invocation surface is a compile-time lowering (OGAR #209 — a
plain Rust fn + golden test), not a protocol that already exists.

## 3. Klickwege → graph: **OGAR #209** (final frame; #208 closed as hallucinated)

Correction trail, kept honest: lance-graph #691 (wrong repo — storage layer),
then OGAR #208, which was itself hallucinated **three times over** (EdgeBlock
speculation → `action_ws`-as-protocol → runtime ownership ceremony) and is now
CLOSED, its body repurposed to state the two distinctions it kept violating.
The from-scratch work item is **OGAR #209**, and the operator-ruled frame is:

**This is a compile-time, type-level seam.** Your repo is ordinary Rust:
codegen emits into it, it depends on `ogar-vocab` (via `lance-graph-ogar`) as
a plain type dependency, and cargo builds it (locally / CI / Railway pulling
GitHub). Constructing a typed value is not a ceremony. The SoA / `temporal.rs`
/ NARS / RBAC / mailbox write-on-behalf machinery is the running engine's
*internal* mechanics — it never gates your build or your emission of a typed
value. (The earlier "ownership design is the gate / membrane crossing"
framing was the same hallucination in its third costume — retracted.)

What #209 actually tracks (small, cargo-testable):

- **A lowering function** `KlickwegEdge → ogar_vocab::ActionInvocation`
  (action-fire) / `nav_witnessed`-shaped value (screen jump) — plain Rust,
  type-level decisions only: `ordinal: u32` → `action_def` String identity
  (resolved via the ActionDef list you already hold), and what `subject` a
  human click carries (the existing constructor sets `System`; a click is
  presumably `User` — to be decided in #209).
- **A golden test** — one harvested Klickweg and one live-emitted edge lower
  to identical values. `cargo test`, nothing running.
- Where drained values are *recorded* when an engine runs is a storage
  concern (Lance calcification) — out of #209's scope and not your problem.

Verified type receipts that survive from the audit:
`ogar_vocab::ActionInvocation` (`lib.rs:508`, `object_instance`),
`ogar-emitter::emit_action_invocation` (`lib.rs:774`), `nav_witnessed`
(`ogar-emitter/src/do_adapter.rs:38`).

Your side stays mechanical once #209 lands: `take_klickwege()` → the lowering
fn → typed values. Tracked owner: the MedCare/lance-graph arc. Doctrine:
lance-graph `.claude/knowledge/compilation-vs-runtime-substrate.md` +
`assembler-vs-storage-substrate.md`.

## 4. The MedCare corpus — pointer + mapping precedent (authorization = operator)

- `MedCare-rs/.claude/harvest/compiled/medcare-actiondefs.json.gz` —
  162 CompiledClass / 2,748 ActionDefs, 215 KB gz, sha256 `81e61096…`
  (per its README). Loader precedent:
  `medcare_analytics::actiondef_exec::{CompiledClassFixture, load_fixture}`
  (serde, `.gz`-transparent).
- `MedCare-rs/tests/golden/nav/klickwege-digest-v2.txt` — the authoritative
  Klickwege golden (NOT `.claude/transcode/klickwege-digest-v2.txt`, a
  mislabeled v1 intermediate).
- CompiledClass → ClassView mapping: your own `HarvestView` in
  `p_rehost.rs` (attributes → `FieldRef` in ObjectView order) is the right
  adapter; positions stay hand-declared — the corpus is signature-only
  (`kausal = body_source = null` on all 2,748; E-MEDCARE-26/28), and
  harvested `writes` can be method-LOCAL variables (`.d`, `.chartPanel`),
  so bindings are never machine-derived.
- **PRIVATE artifact** (medical-app harvest): reference vs. vendor into
  a2ui-rs is the operator's call, not either session's.
- Convergence you already suspected: the Cascade band's refresh half
  (`co_DGV.Update_SingleDGVLine(key,…)`) IS your `NodeDelta` keyed-row update —
  full P-REHOST and the MedCare Cascade drain are the same wiring; worth doing
  once, together. *(Note: E-MEDCARE-29's broader "ladder complete / Cascade fully
  decomposed" framing was itself corrected by E-MEDCARE-30 — see below. This
  narrow refresh↔NodeDelta convergence survives; the "no upstream primitive ever
  needed" conclusion does not.)*

## Status corrections to this repo's last table

- MedCare-rs `main` is at **#210** (`3c2e302`) — #209 (inc 2) and #210
  (inc-3 probe: Guard falsified, Cascade decomposed) both merged.
- lance-graph `main` is at **#690** (`4fbf42e3`) — your dep re-pin
  observation stands: local checkouts must be ≥ #690 for
  `execute_defaults`.

## Council corrections (2026-07-14 — 5+3 falsification audit)

This file's answers were written fast off spot-reads by a session that had just
conflated OGAR (the assembler substrate) with lance-graph (the storage/query
spine). A 5+3 council (5 falsification savants + 3 brutal reviewers, source-
receipted) audited them. Grades:

| answer | grade | what changed |
|---|---|---|
| 1 — `ClassView` additivity | **discipline holds; rationale corrected** | the additivity norm (additions are default-methods) survives; the "Guard closed → no `ClassView::constraints` → ladder COMPLETE" rationale is retracted (CLAIM-C / E-MEDCARE-30) — Guard is re-opened |
| 2 — action seam / `action_ws` | **WRONG** | the "`action_ws` is the runtime dispatch protocol / `ResolvedAction` is a `SubmitAction` producer" claim retracted — `action_ws` is the arago/HIRO automation surface; consult OGAR `ACTIONDEF-VALUE-DISPATCH-PROPOSAL.md` |
| 3 — Klickwege → graph | **WRONG in parts** | the invented `KlickwegEdge→ActionInvocation` field mapping retracted (type mismatch; `subject=System` not `User`); the landing *type* exists, the *lowering* + ownership are OPEN |
| 4 — corpus pointer | **holds** | unchanged (E-MEDCARE-30 caveat noted) |

Receipts and the full findings: MedCare-rs E-MEDCARE-30 + `AGENT_LOG.md`
(council run), OGAR #208 (closed as hallucinated, repurposed) / #209 (the
from-scratch rewrite), lance-graph #692 for the `contract::action`
shape-parity gap. The lesson is the point: a claim entering a cross-session
doc is a claim entering canon — it goes through the council BEFORE it lands,
not after.
