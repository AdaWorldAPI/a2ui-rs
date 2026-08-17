# PR_ARC_INVENTORY — a2ui-rs

> **APPEND-ONLY.** One row per merged PR, reverse chronological (newest at
> top — PREPEND, never insert mid-list). Pattern mirrors `lance-graph`'s
> `.claude/board/PR_ARC_INVENTORY.md` (see that repo's `CLAUDE.md` §
> "Mandatory Board-Hygiene Rule" for the canonical description). Never edit a
> past row except its own `Confidence:` line; a correction gets its own new
> row, dated, pointing back at what it corrects.
>
> **Going forward**, every merged PR gets a full row:
>
> ```
> ## PR #<n> — <title> (merged <date>, <merge-commit-short-hash>)
> - **Added:** what shipped
> - **Locked:** what invariant/contract this now holds fixed
> - **Deferred:** what was explicitly left for later
> - **Docs:** which doc(s) this PR touched or should have touched
> - **Confidence:** how sure the authoring session was, and why
> ```
>
> Until that discipline starts, the table below is a **seed baseline** — the
> last 20 commits on `main` as of board creation (2026-08-17), turned into a
> reverse-chronological list. It does not carry Added/Locked/Deferred/Docs
> detail (that would require re-deriving intent this session didn't have) — it
> exists so the ledger has a starting point instead of a gap. New rows above
> this baseline follow the full format.

---

## Seed baseline (git log, `origin/main`, 2026-08-17, most recent 20 commits)

| commit | description |
|---|---|
| `9869e23` | Merge PR #40 — a2ui-graph: declare `wasm-bindgen-futures` (FieldHandle's async impl needs it) |
| `f085ca8` | a2ui-graph: declare wasm-bindgen-futures dependency |
| `7ec7ce5` | Merge PR #39 — docs: wake a cold field after every mutation (wgpu frame-wake doc) |
| `77d414c` | docs: wake a cold field after every mutation |
| `7f79989` | Merge PR #38 — a2ui-graph: expose explicit browser backend routing receipts (wgpu diagnostics routing) |
| `d3278d9` | a2ui-graph: expose explicit browser backend routing receipts |
| `eed9431` | Merge PR #37 — docker: unlock the Railway build |
| `107f115` | docker: stop copying Cargo.lock too — floating is the point |
| `5352d01` | docker: drop `--locked` from both Railway builds — it cannot hold here |
| `2899856` | Merge PR #35 — a2ui-layout: shared UX zone/budget substrate |
| `a5f1099` | a2ui-layout: Zonen und Budgets als geteilte Schicht, nicht als Prosa |
| `6e76e6f` | Merge PR #33 — WebGL2 fallback, made to actually fire |
| `4a71a68` | Der WebGL2-Fallback muss GEFAHREN werden — er passiert nicht von selbst |
| `5f59c23` | Merge PR #34 — q2/OSM map re-encoding follow-up |
| `53a04dc` | web: defer the reconfigure past present, stop burning warm on skipped frames |
| `a305f13` | Merge PR #32 — q2/OSM map re-encoding |
| `c67a57f` | wgpu: follow the fork's trunk to 0b87b29 (+138 upstream commits) |
| `ce85757` | wgpu: name the fork once, with no version pin anywhere |
| `5ca6f06` | wgpu: consume the AdaWorldAPI fork, migrate 22 → 30 to do it |
| `fa34888` | Merge PR #31 — wasm as the default target, wgpu as default feature |

Earlier PRs (visible via `git log --oneline` further back, not reproduced
here to keep this seed bounded) include, per `CLAUDE.md`'s own status
narrative: `a2ui-solid`/`a2ui-solid-web` (CAD POC, PR #24/#25 range),
`a2ui-graph`'s GPU field renderer + wasm target + JS surface (PR #26–#29
range), and the original W1–W3+#209-lowering + `a2ui-paint` reusable-client
build (PRs referenced in
`.claude/handovers/2026-07-16-a2ui-arc-current-state.md` as #9, #10, #11).
A future session reconstructing full detail for those should read the actual
PR diffs on GitHub rather than re-deriving from commit subjects alone.

---

*(New full-format rows go above this line, newest first.)*
