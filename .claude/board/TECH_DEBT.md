# TECH_DEBT — a2ui-rs

> Known, dated debt entries. Only concretely-evidenced debt goes here — a real
> TODO/FIXME, an explicit "not yet implemented"/"open decision" note in a
> handover or plan, or a gap a session actually hit. Do not pad this file with
> invented or speculative debt; an empty section for a given date is the
> correct, honest state if nothing concrete was found. Pattern mirrors
> `lance-graph`'s `.claude/board/TECH_DEBT.md`.

## 2026-08-17 — seeded at board creation

- **The `WideFieldMask` permit-all identity is undecided.** `CLAUDE.md`
  (§ "RBAC is real, and it is by PROJECTION") states explicitly: *"the
  permit-all identity (`WideFieldMask::ALL` vs default
  `full_for(field_count)`) is the one open W1 decision."* The fail-closed
  *constraint* is implemented and tested (empty role → `NoRoleGrant`, disjoint
  role∩surface → `EmptyProjection`, per the 2026-07-16 handover's evidence at
  `project.rs:70-80`), but which value counts as "grant everything" is not yet
  chosen. Blocked on an upstream `lance-graph-contract` `WideFieldMask` retype
  PR (per `CLAUDE.md` and repeated in
  `.claude/handovers/2026-07-16-a2ui-arc-current-state.md` §4.1 and §6.7).
  **Do not** assume `full_for` is safe to treat as the RBAC default in new
  code — `CLAUDE.md` is explicit that it is a *render* convenience, never an
  RBAC fallback.

No other concretely-evidenced debt was found in this pass: `crates/**/*.rs`
has zero `TODO`/`FIXME`/`unimplemented!()` markers as of `9869e23`. Genuine
gaps that exist are tracked as *deferred scope* rather than debt — see
`LATEST_STATE.md`'s "Deferred / not yet scheduled" section (typography/glass
raster in `a2ui-paint`, write-side `SetField` operations, OGAR-side #210/#211
dependencies) — those are declared future work, not defects, so they don't
belong in this ledger unless a session finds them causing an actual problem.
