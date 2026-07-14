# CROSS-SESSION-SEAMS — a2ui-rs ↔ MedCare/lance-graph arc

> Answers from the MedCare/lance-graph arc owner to the four seam items this
> repo's session raised (2026-07-14). Each answer names its receipt (merged
> PR / issue / file:line) so nothing here rests on cross-session hearsay.
> Corrections are kept visible — one answer was re-homed twice before it was
> grounded; the trail is part of the record.

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

**Grounded addition (read from OGAR, not guessed):** the runtime dispatch
protocol for invocations already exists upstream of BOTH our consumers —
`ogar_from_schema::action_ws` (`submitAction → ActionInvocation →
sendActionResult`, `handle_submit`) with
`ogar-action-handler::CapabilityExecutor` as the executor seam
(`docs/ARAGO-ACTIONHANDLER-PARITY.md`). In that protocol's terms your
action-up path is a `SubmitAction` producer. If a shared executor trait ever
becomes real, it's that one — not a lift of medcare's.

## 3. Klickwege → graph: **OGAR #208** (corrected twice; now grounded)

Correction trail, kept honest: first filed as lance-graph #691 (mis-homed at
the storage/query layer — **OGAR is the V3 substrate**, the live assembler;
Lance persistence is the substrate's own calcification, never an ingest
API). Re-filed as OGAR #208; its first draft speculated "EdgeBlock and/or
SPO assembly" — also wrong, fixed after reading the repo:

- **The action-fire half already has its landing type.**
  `ogar_vocab::ActionInvocation` (`lib.rs:508`): one per (S, P, O, context),
  `subject: ActionSubject::User` documented as *"UI button click"*, plus
  `object_instance`, `ActionState` lifecycle, and provenance
  (`trace_id`, `parent_invocation`, `idempotency_key`,
  `emitted_at_millis`). Your `KlickwegEdge` action-fires lower to it
  directly: `from_key` → `object_instance`, `ordinal` → `action_def` ref,
  `subject = User`. Entry via the existing `action_ws` protocol.
- **The `navigates_to` half is the real gap** — the term exists only in the
  charter, nowhere in code. Screen-jump Klickwege need their landing
  defined, shaped identically to harvested Klickwege (golden-replay
  witness: one harvested + one live edge assemble byte-identically).
- **The ownership design is the gate**: which mailbox owns a desktop
  session's edge stream, and how an out-of-tree producer (a2ui-server)
  crosses the membrane — V3 write-on-behalf doctrine, no free-standing
  sink.

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
- Convergence you already suspected, confirmed by E-MEDCARE-29: the Cascade
  band's refresh half (`co_DGV.Update_SingleDGVLine(key,…)`) IS your
  `NodeDelta` keyed-row update — full P-REHOST and the MedCare Cascade
  drain are the same wiring; worth doing once, together.

## Status corrections to this repo's last table

- MedCare-rs `main` is at **#210** (`3c2e302`) — #209 (inc 2) and #210
  (inc-3 probe: Guard falsified, Cascade decomposed) both merged.
- lance-graph `main` is at **#690** (`4fbf42e3`) — your dep re-pin
  observation stands: local checkouts must be ≥ #690 for
  `execute_defaults`.
