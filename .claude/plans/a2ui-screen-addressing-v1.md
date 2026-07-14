# Plan — a2ui-rs screen addressing (v1)

> The consumer-side sequencing of the charter. The **charter is the source of
> truth** (`AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md`, merged
> OGAR #204, security-corrected #205); this file sequences its waves for the
> a2ui-rs repo and records the open decisions + gates. Ledger:
> OGAR `docs/DISCOVERY-MAP.md` `D-A2UI-SCREEN-ADDRESSING`.

## The thesis (one line)

**Don't push pixels — address the screen.** Down the wire go `NodeDelta`
frames (16-byte GUID key + wide changed-fields mask + ClassView-carved LE
values); up go `ActionInvoke` frames (behavior by ordinal address). The client
holds the ClassView/template codebook and renders locally (askama → wasm, zero
serialization). The ClassView registry is the font of the desktop: reference
codepoints, don't stream glyph rasters.

## Where the seam falls (repo boundary)

| lives in | what |
|---|---|
| **OGAR (upstream)** | the canon-free frame types (`ogar-a2ui-frame`, W1), ClassView carving, RBAC `WideFieldMask`, the Core change + its ledger |
| **a2ui-rs (this repo)** | the render target: consumes the frames, grows the service + wasm client tiers, hosts the P-REHOST proof |

The frames are upstream because they are canon-adjacent (they carry the GUID
key + the ClassView-carved value bytes); the service and client are here
because they are the *reimagining of A2UI*, not OGAR core.

## Waves (charter C5, sequenced for this repo)

- **W0 — repo + regrade** — *DONE.* `AdaWorldAPI/a2ui-rs` minted; A2UI-fork
  `hamming.proto` regraded (shape kept, payload marked pre-V3, charter C2).
  Council verified the charter spec (#204/#205).
- **W1 — surface contract (canon-free, in OGAR)** — *CODED, PR OGAR #206 open.*
  `ogar-a2ui-frame`: `FRAME_VERSION`, `FrameKind{NodeDelta,ActionInvoke}` (closed),
  `NodeDelta{key:[u8;16], mask_words:Vec<u64>, values}`, `ActionInvoke{key,
  action_ordinal, args}`, `to_le_bytes`/`from_le_bytes`, `mask_positions`,
  `FrameError` refusals. `#![forbid(unsafe_code)]`, zero hot-path deps, serde
  membrane-only. **a2ui-core re-exports these** (seed shipped; git-dep floats on
  the W1 branch until #206 merges, then flips to `main`).
- **W2 — a2ui-server transcode** — *DONE (crate `a2ui-server`).* The graph
  desktop projection tier over the W1 frames: `project.rs` (RBAC
  `WideFieldMask ∩ role`, **fail-closed** — the sentinel ban enforced;
  `mask_to_words` bridges the wide mask to the wire), `render_stream.rs`
  (`project_node`: node → `NodeDelta` down + the askama **fieldview** surface,
  RBAC-projected BEFORE framing), `action_stream.rs` (`ActionInvoke` up →
  `concept_of_key` + ordinal-address `ActionDef` resolution, trap T2),
  `session.rs` (rdp-2graph capability: Argon2 session KDF + class-range +
  role-mask gates). 22 unit tests. The render half is the upstream OGAR brick
  `ogar-render-askama::field_view` (`render_field_view`, added same arc —
  `data-field-pos` = mask address, `data-action-ordinal` = ActionDef address,
  `escape="html"` no `|safe`, on OGAR `main` via #207). Membrane adapters
  (JSON/proto) only at the edge, behind a feature (T3) — deferred; the hot path
  is LE + AEAD. All OGAR deps flipped to `main` (float-then-flip complete).
- **W3 — a2ui-wasm fieldview client** — *CORE DONE (crate `a2ui-wasm`).* The
  `FieldviewClient` holds the ClassView/template codebook (the font of the
  desktop) + per-node facet state, ingests `NodeDelta` LE bytes (W1 load gate,
  no serde), resolves `key → ClassView → template` (`concept_of_key`, zero
  value decode), accumulates deltas, and renders the addressed fieldview
  locally via the SAME `ogar-render-askama::render_field_view` the server uses.
  Actions go up by ordinal address (`invoke_action` → `ActionInvoke`).
  **`cargo check --target wasm32-unknown-unknown` GREEN — core AND the
  `wasm-bindgen` wrapper (`wasm::FieldviewClientWasm`, feature `wasm`)** — the
  charter C1.3 "the browser IS the thin client / same Rust" claim proven
  mechanically. 7 native tests (delta apply + accumulate, unknown-class
  fail-loud, value-underrun refusal, action round-trip, up-frame-rejected-as-
  down). *Remaining:* the canvas/webgpu paint (turn the rendered surface into
  pixels on the client's own silicon) + a browser e2e harness — the last-mile
  UI, not new render logic.
- **W4 — P-REHOST** — *THE GATE — GREEN (lite).* See below.
  `crates/a2ui-server/tests/p_rehost.rs` proves the whole mechanism end-to-end
  (harvested Class × ActionDef → codegen struct-of-methods via
  `render_class_with_methods_wide`; RBAC-project; `NodeDelta` + fieldview down;
  sealed transport round-trip; `ActionInvoke` up resolved by ordinal address).
  *-lite* only because a2ui-rs does not vendor the MedCare harvest — the probe
  uses a harvested-SHAPE stand-in class; the full corpus re-host is the
  remaining step before scale-out.
- **W5 — rdp-2graph session** — *DONE (`a2ui-server::desktop::DesktopSession`).*
  The C2 service shape as one object — the "Citrix without pixels" loop:
  `render_node` (RenderStream: RBAC-project → seal `NodeDelta` down + fieldview),
  `receive_action` (ActionStream: open sealed `ActionInvoke` → resolve by
  ordinal address → **record a Klickweg edge**, C1.6), `sync_codebook`
  (SyncCodebook: versioned template-codebook amortization). Built on
  `a2ui-server::session` (`ogar-encryption` Argon2id KDF, once per session;
  fresh-salt invariant enforced via `establish_random_salt`) +
  `SealedTransport` (XChaCha20-Poly1305 per-frame AEAD, counter-nonce +
  direction separation + strict-monotonic replay/reorder rejection) + role-mask
  projection (C1.4). The Klickwege telemetry (`KlickwegEdge` — a click IS a
  `navigates_to`/`ActionInvocation` edge, zero new vocabulary) accumulates
  per-session and drains via `take_klickwege` for the graph. Salt/nonce-reuse
  hardening landed (review on #4). Remaining: wire the drained Klickwege edges
  into the live AriGraph SPO store (an OGAR-side hop).

## The killer probe — P-REHOST (charter C4)

**Harvest the app → re-render the app, no WinForms.** Re-render ONE harvested
MedCare screen from its `CompiledClass` × ClassView × askama, and fire one
harvested `ActionDef` round-trip (`ActionInvoke` up → state delta → `NodeDelta`
down). Everything it needs already exists: Klickwege golden v2, 162
`CompiledClass` / 2,748 `ActionDef`s, `ogar-render-askama`. If it renders and
the action round-trips, the thesis is proven end-to-end on a real app; if it
can't, the arc stops here and we learn why. **This is the falsifier, not a demo.**

## Open decisions

1. **W1 — permit-all RBAC identity.** `ClassRbac::field_mask` is retyped
   `FieldMask → WideFieldMask` in place (charter C1.4, codex-P2 #205) so
   surfaces past 64 fields are covered. The one open call: the permit-all
   identity — `WideFieldMask::ALL` (explicit "all fields") vs the default
   `full_for(field_count)` (all fields *of this class*). **Fail-closed
   constraint:** whichever is chosen, a missing/narrow role mask must NOT fall
   back to "emit everything" — `full_for` is a *render* convenience, never an
   RBAC fallback. Resolve in the lance-graph-contract PR that lands the retype.
2. **W2 — membrane placement.** JSON/proto adapters live behind a feature at
   the service edge only; the audit sink (if any) is inner, never on the
   client-facing membrane (medcare-rs iron rule #7 — the witness is examined in
   place, it does not egress).

## Iron boundaries (the charter traps — restated so the plan can't drift)

- **T1** — widget skins are ClassView templates (render side, lo-u16), never a
  second closed vocabulary beside the doc-IR RegionKinds. "A new widget" is a
  template.
- **T2** — behavior never rides the surface (the SURREAL-AST trap, UI edition):
  actions by `ActionDef` ordinal address, never `onClick: <lambda>` inline.
- **T3** — no serialization in the hot path (the Firewall, ADR-022/023):
  `to_le_bytes`/`from_le_bytes` ARE the wire; serde only at a membrane, behind
  a feature.

## Gate order (nothing skips)

```
W0 done → W1 (#206 merge → flip dep to main) → W2 server → W3 wasm → W4 P-REHOST GREEN → W5 session
                                                                        │
                                              no scale-out crosses this line
```

## Board hygiene

Durable ledger for the arc = OGAR `docs/DISCOVERY-MAP.md`
`D-A2UI-SCREEN-ADDRESSING` (the Core change lives there, append-only). This
plan + `CLAUDE.md` + the a2ui-rs commit history are the consumer-side record.
When a wave lands, append its status to the ledger entry in the same PR arc.
