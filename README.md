# a2ui-rs — the OGAR render target for screen addressing

**Don't push pixels — address the screen.**

The remote desktop as addressed surface: down the wire go
`(GUID key 16 B, wide mask delta, changed LE values)` frames, up the wire go
`ActionInvocation`s; the client holds the ClassView/template codebook and
renders locally from borrowed memory (askama, zero serialization). *The
ClassView registry is the font of the desktop* — you don't stream glyph
rasters, you reference codepoints.

## Charter

`AdaWorldAPI/OGAR docs/A2UI-SCREEN-ADDRESSING-PROPOSAL.md` (merged OGAR
#204, security-corrected #205). The A2UI pattern ("agents speak UI",
`AdaWorldAPI/A2UI`) reimagined on the V3 substrate: its `hamming.proto`
service shape survives (RenderStream / ActionStream / codebook sync); its
payload is replaced by the 16-byte facet key + mask + values.

## Layout

| crate | role | status |
|---|---|---|
| `a2ui-core` | re-exports the W1 frames (`ogar-a2ui-frame`); grows the W2 service shape | seed |
| (planned) `a2ui-server` | RenderStream/ActionStream over the frames; RBAC-projected (`surface ∩ role`, fail-closed) before framing | W2 |
| (planned) `a2ui-wasm` | the fieldview client — ClassView resolve + askama render compiled to wasm; LE ingest zero-copy | W3 |

## Iron boundaries (charter traps)

- **T1** — widget skins are ClassView templates, never a second closed
  vocabulary.
- **T2** — the surface never carries behavior; actions travel by ADDRESS
  (`ActionDef` ordinal on the Core node).
- **T3** — no serialization in the hot path; LE bytes end-to-end; membrane
  adapters (JSON/proto) only at the edge.

## Killer probe

**P-REHOST** — re-render one harvested MedCare screen from its
`CompiledClass` × ClassView × askama and fire one harvested `ActionDef`
round-trip: harvest the app → re-render the app, no WinForms.
