# AGENT_LOG — a2ui-rs

> One entry per completed agent-run session: what was touched (D-ids/PRs
> touched, commit, tests run, outcome). Pattern mirrors `lance-graph`'s
> `.claude/board/AGENT_LOG.md`, including its **ONE-WRITER rule** (that
> repo's `CLAUDE.md`, "ONE-WRITER CORRECTION", 2026-07-22): a shared
> append-log with concurrent writers is a lost-write race and re-creates the
> exact shared-mutable-sink pattern the runtime substrate this stack builds
> toward is designed to eliminate (one-writer-per-mailbox / no singleton
> shared sink). Concretely:
>
> - **Sub-agents do NOT append to this file.** If a sub-agent is spawned
>   during a session and needs to leave a record, it writes its own
>   agent-tag file (e.g. under a per-agent scratch location) — never this
>   shared log.
> - **The orchestrating main thread of a session is the SOLE writer** of this
>   file. It consolidates any sub-agent tag-files into one entry here after
>   the work is committed, not before.
> - **PREPEND**, newest entry first. Never edit a past entry's body; a
>   correction gets its own new entry.
> - The consolidation write itself is not a separate "agent run" and does not
>   get its own log entry beyond the entry it is writing (no infinite
>   regress — see `lance-graph` `CLAUDE.md`'s "Termination clause" for the
>   general form of this rule, applied there to a different file).

---

## 2026-08-17 — board scaffold created (`.claude/board/`)

**What:** created the `.claude/board/` triple-ledger (six files:
`LATEST_STATE.md`, `PR_ARC_INVENTORY.md`, `INTEGRATION_PLANS.md`,
`TECH_DEBT.md`, `EPIPHANIES.md`, this file) — a2ui-rs previously had
`.claude/handovers/` and `.claude/plans/` but no `.claude/board/` at all.
Pattern is mirrored from `lance-graph`'s `.claude/board/` (see that repo's
`CLAUDE.md` § "Mandatory Board-Hygiene Rule" and "Session Start — MANDATORY
READS" for the canonical description each file here follows), which
`MedCare-rs` also already implements in full as a worked example of the
concrete format.

**Drew from:**
- `CLAUDE.md` (full read) — repo identity, charter pointers, crate status
  table, iron boundaries T1/T2/T3.
- Root `Cargo.toml` — the 9-crate workspace member list.
- Each crate's own `Cargo.toml` `description` + `lib.rs` module doc — the
  `LATEST_STATE.md` crate-inventory table.
- `.claude/handovers/2026-07-15-council-ratified-nesting-and-projectional-vision.md`
  and `.claude/handovers/2026-07-16-a2ui-arc-current-state.md` (both read in
  full) — the 2026-07-16 file's own self-correcting §4/§5 became this board's
  first `EPIPHANIES.md` entry; both are cited in `LATEST_STATE.md`'s "Active
  work" section as stale-but-informative.
- `.claude/plans/*.md` (all three, headers read for status labels) — indexed
  verbatim into `INTEGRATION_PLANS.md`.
- `git log --oneline` (unshallowed first — the checkout arrived shallow at
  depth 1) — seeded the `PR_ARC_INVENTORY.md` baseline table from the most
  recent 20 commits.
- `grep -rn "TODO\|FIXME\|unimplemented!"` across `crates/**/*.rs` — came back
  empty, which is why `TECH_DEBT.md` has exactly one seeded entry (the
  `WideFieldMask` permit-all identity, which `CLAUDE.md` itself flags as an
  open decision) rather than several invented ones.

**Commit / branch:** `claude/board-hygiene-scaffold` (see PR for the actual
commit hash once opened).

**Tests:** none run — this is a docs-only change (six new Markdown files
under `.claude/board/`, no source touched). `CLAUDE.md`'s build/test commands
were not invoked because nothing they cover changed.

**Outcome:** board scaffold created and committed; PR opened against `main`.
No source-code, crate-inventory, or design claim in this repo was changed —
only its documented understanding of its own state.
