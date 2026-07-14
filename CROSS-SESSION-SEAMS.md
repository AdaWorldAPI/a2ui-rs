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

## 1. `ClassView` additivity — CONFIRMED, stronger than asked

**Guard is closed unbuilt** (E-MEDCARE-29, MedCare-rs #210 merged): the inc-3
probe enumerated all 36 Guard-classified methods — 33 die with the
`lettre`-replaced mail-collection classes, 3 are WinForms plumbing that is
structural in Rust (`Option`/`Result`). There will be **no** validation
phase, no `ClassView::constraints`, no `GuardRefused` variant. The upstream
recipe ladder is COMPLETE at Compute (`execute_compute_dag`, inc 1) +
Default (`execute_defaults`, lance-graph #690).

Standing norm you can rely on: every recent `ClassView` addition
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
reaches OGAR's invocation surface is an OPEN design item (OGAR #208), not a
protocol that already exists.

## 3. Klickwege → graph: **OGAR #208** (corrected twice; now grounded)

Correction trail, kept honest: first filed as lance-graph #691 (mis-homed at
the storage/query layer — **OGAR is the V3 substrate**, the live assembler;
Lance persistence is the substrate's own calcification, never an ingest
API). Re-filed as OGAR #208; its first draft speculated "EdgeBlock and/or
SPO assembly" — also wrong, fixed after reading the repo:

- **The action-fire landing TYPE exists** — `ogar_vocab::ActionInvocation`
  (`lib.rs:508`): `object_instance` is the target/landing field, with
  `ActionState` lifecycle + provenance, and live SPO emission
  (`ogar-emitter::emit_action_invocation`, `lib.rs:774`). **CORRECTED (council
  CLAIM-A/B):** the specific field mapping I gave earlier
  (`from_key → object_instance`, `ordinal → action_def`, `subject = User`) was
  **invented** — `ordinal` is a `u32` array index while `action_def` is a
  String identity (a type mismatch), and the only real constructor sets
  `subject = System`, not `User`. The *type* to land on is settled; the
  *lowering* from a `KlickwegEdge` is NOT designed — that is the real work.
- **`navigates_to`: CORRECTED (council CLAIM-B).** I claimed it "has no landing,
  charter-only." False — the action-fire landing (`ActionInvocation.object_instance`
  + SPO emit) already exists, and a nav-adjacent `nav_witnessed` Klickwege plane
  exists in `ogar-emitter/src/do_adapter.rs:38`. The literal token is absent from
  `.rs` (true but trivial); the substantive gap is narrower than I framed — it is
  the *relationship* between a live screen-jump edge and those existing mechanisms,
  not a wholly unhomed concept.
- **The ownership design is the gate** *(open item, not audited-as-settled)*:
  which mailbox owns a desktop session's edge stream, and how an out-of-tree
  producer (a2ui-server) crosses the membrane — V3 write-on-behalf doctrine, no
  free-standing sink. (This was asserted as settled in an earlier version; it is
  an OPEN design item, tracked in OGAR #208.)

Your side stays mechanical once #208 lands: `take_klickwege()` drains into
the landing. Tracked owner: the MedCare/lance-graph arc.

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
| 1 — `ClassView` additivity | **holds** (LOW risk) | unchanged |
| 2 — action seam / `action_ws` | **WRONG** | the "`action_ws` is the runtime dispatch protocol / `ResolvedAction` is a `SubmitAction` producer" claim retracted — `action_ws` is the arago/HIRO automation surface; consult OGAR `ACTIONDEF-VALUE-DISPATCH-PROPOSAL.md` |
| 3 — Klickwege → graph | **WRONG in parts** | the invented `KlickwegEdge→ActionInvocation` field mapping retracted (type mismatch; `subject=System` not `User`); the landing *type* exists, the *lowering* + ownership are OPEN |
| 4 — corpus pointer | **holds** | unchanged (E-MEDCARE-29 caveat noted) |

Receipts and the full findings: MedCare-rs E-MEDCARE-30 + `AGENT_LOG.md`
(council run), OGAR #208 (corrected), OGAR issue for the `contract::action`
shape-parity gap. The lesson is the point: a claim entering a cross-session
doc is a claim entering canon — it goes through the council BEFORE it lands,
not after.
