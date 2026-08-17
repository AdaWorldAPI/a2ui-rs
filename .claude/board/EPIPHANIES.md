# EPIPHANIES — a2ui-rs

> Dated, **PREPENDED** findings/corrections/architectural discoveries.
> Append-only — never edit or delete a past entry's body; a later session may
> add a trailing `Status:`/`Confidence:` line to an existing entry, but a
> genuine correction gets its OWN new entry, dated, pointing back at what it
> corrects. Pattern mirrors `lance-graph`'s `.claude/board/EPIPHANIES.md`.

---

## 2026-07-16 — a written queue can be stale the moment it's read; verify before propagating

**Source:**
`.claude/handovers/2026-07-16-a2ui-arc-current-state.md` §4 ("Corrections to
the July-15 reconciliation").

That handover fact-checked its own predecessor
(`2026-07-15-council-ratified-nesting-and-projectional-vision.md`) against
`origin/main`, GitHub PR/issue state, and source at file:line, and found
concrete drift — not vague staleness, but specific wrong claims that had
already started propagating:

- The RBAC "permit-all mask identity" was reported resolved; it was (and, as
  of this board's creation, still is per `TECH_DEBT.md`) **open**.
- A prior handover asserted a "Stockfish DecisionEpisodeV1" work item tying UI
  interactions and chess decisions into one addressed-episode substrate. On
  verification: **no such type, plan, or board entry exists anywhere** — not
  in a2ui-rs, not in lance-graph, not in stockfish-rs; the word "episode"
  doesn't even appear in stockfish-rs. It was aspirational narrative that had
  started being cited as if it were already-authored direction.
- `A2UI-SCREEN-ADDRESSING-PROPOSAL.md` (the OGAR charter this repo builds
  against) had merged (OGAR #204/#205) but was still self-labeled "PROPOSAL
  (council pending)" and graded `[S]` (speculative) in OGAR's own
  `DISCOVERY-MAP.md` — a merged doc is not automatically a ratified one.

**Why this belongs in the ledger, not just the handover:** the general lesson
generalizes past this one incident — a status document (a handover, a plan's
status table, even this board's own `LATEST_STATE.md`) can be individually
well-reasoned and still wrong the moment something ships after it was written,
and the failure mode is a *second* session citing the *first* session's
unverified claim as if it were checked. The corrective habit the 2026-07-16
handover modeled: re-verify against `git log`/PR state/source file:line before
extending a queue, not just before starting new work. `LATEST_STATE.md`'s own
"Active work" section explicitly flags itself as being in the same position
relative to `CLAUDE.md`'s more-current status table — this board tries to
practice what this entry preaches, but a future session should still verify
rather than trust that either file stayed current.

**Status:** the specific claims above are corrected in the handover itself and
carried forward accurately in `LATEST_STATE.md` as of 2026-08-17. The general
lesson (verify before propagating a queue) has no expiry.
