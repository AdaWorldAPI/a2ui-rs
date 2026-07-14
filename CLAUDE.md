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

## Layout + status

| crate | role | status |
|---|---|---|
| `a2ui-core` | re-exports `ogar-a2ui-frame` (W1 frames) | seed (shipped) |
| `a2ui-server` | the graph desktop projection: RBAC-project (`WideFieldMask ∩ role`, fail-closed) → `NodeDelta` + askama fieldview down; `ActionInvoke` up by ordinal address; `ogar-encryption` sealed session transport | **W2 shipped** (22 unit tests + P-REHOST-lite green) |
| `a2ui-wasm` (planned) | the fieldview client — ClassView resolve + askama → wasm; LE ingest zero-copy | W3 |

Upstream dep split (two OGAR refs, one package each — no conflict):
`a2ui-core`'s `ogar-a2ui-frame` is on **`branch = "main"`** (W1 flipped once OGAR
#206 merged — the float-then-flip pattern). `a2ui-server`'s render + crypto deps
(`ogar-render-askama` = the fieldview brick, `ogar-encryption`) still **float on
`claude/ogar-a2ui-transcoding-b7xzrn`** until the OGAR W2/W3 render PR merges,
then flip to `main`. The render half is the upstream OGAR brick
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
cargo +1.95.0 test            # workspace = edition 2024 / rust 1.95 (matches OGAR)
cargo +1.95.0 clippy --all-targets -- -D warnings
cargo +1.95.0 fmt --check
```

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
