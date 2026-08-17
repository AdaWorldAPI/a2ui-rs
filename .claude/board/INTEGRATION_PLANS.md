# INTEGRATION_PLANS — a2ui-rs

> **APPEND-ONLY** index of versioned integration/design plans. Each row points
> at the fuller doc under `.claude/plans/<name>-v<N>.md`. Pattern mirrors
> `lance-graph`'s `.claude/board/INTEGRATION_PLANS.md` — consult this file
> before proposing a new plan; a superseding version gets a NEW row (prepend),
> the old row stays with a `superseded by` note, never deleted or edited away.

## Active plan index (as of 2026-08-17, `.claude/plans/` contents)

| file | status (self-labeled where stated) | one-sentence summary |
|---|---|---|
| `a2ui-screen-addressing-v1.md` | Active — sequencing the OGAR charter's waves (W1–W5) | The consumer-side wave plan for the whole arc: the charter itself (`AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`, OGAR #204/#205) is the source of truth, this file sequences it into waves and tracks open decisions/gates for the a2ui-rs repo. **Known stale per the 2026-07-16 handover:** its per-wave status table is out of date (marks shipped things as open/remaining) — read it for the boundary model and traps, not for current status. |
| `a2ui-reusable-client-and-209-lowering-v3.md` | RATIFIED (Phase-4 output of a `/5plus3` council) | The ratified design for the reusable-client separation-of-concerns split plus the OGAR `#209` Klickwege lowering (`lower_action_fire`, `lower_screen_jump`) — resolves a prior draft's council review (2 BLOCKs + convergent FIXes), cross-checked against the real OGAR #209. |
| `projectional-knowledge-editor-v1.md` | **DIRECTION — not yet scheduled work** (self-labeled) | The forward vision: Word/Excel/CAD as ClassView projections of one living OGAR object graph — "document, spreadsheet, desktop, and CAD are positional projections of the same graph." Explicitly an extension of the ratified reusable-client design, not a pivot. Do not read as a queue. |

## Notes for future entries

- When a plan is superseded by a new version, add a new row above with the new
  filename and a one-line "supersedes `<old-file>`" note; leave the old row in
  place with `superseded — see <new-file>` appended to its status column.
- The OGAR charter itself (`AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`)
  is NOT indexed here — it lives in a different repo and is the authoritative
  charter, not an a2ui-rs-local plan. `CLAUDE.md` names it directly as a
  mandatory read.
