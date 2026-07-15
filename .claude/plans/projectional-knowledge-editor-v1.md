# Projectional Knowledge Editor — Word / Excel / CAD as ClassView projections (v1 vision)

> Operator vision, 2026-07-15. Status: DIRECTION (not yet scheduled work). This
> is the strategic frame the a2ui-rs screen-addressing arc is building toward.
> It is an EXTENSION of the ratified reusable-client design
> (`a2ui-reusable-client-and-209-lowering-v3.md`), not a pivot — every piece maps
> onto the existing OGAR/a2ui substrate (mapping made explicit below).

## The thesis

**A projectional knowledge editor: the OGAR object graph stays canonical, and
users edit familiar *projections* of it.** The user sees Word / Excel / CAD; OGAR
sees edits to graph objects. The three surfaces are not three data models — they
are three **ClassView projections** (interaction skins) of ONE living object
graph. This is the "document = screen" ruling extended once more:

> **Document, spreadsheet, desktop, and CAD are positional projections of the
> same graph.**

```
                    OGAR object graph  (canonical truth)
                            │
                 ClassView × WideFieldMask
                            │
   ┌──────────┬─────────────┼──────────────┬─────────────┐
   ▼          ▼             ▼              ▼             ▼
 flow skin  grid skin    form skin    spatial skin   graph skin
 (Word)     (Excel)      (form)       (CAD)          (native)
   │          │             │              │             │
 field     edge/formula  field         feature/       node/edge
 edits     ops           edits         constraint ops  ops
   └──────────┴─────────────┼──────────────┴─────────────┘
                            ▼
               addressed graph operations
        (SetField / edge-op / feature-op — NEVER a blob mutation)
```

## The load-bearing pattern — compile every rendered region to an addressable field view

**Do NOT bake values permanently into a template** (`Patient: <%= patient.name %>`).
Instead each rendered region compiles to an addressable field view:

```
text span → object 123 → field Patient.name → renderer text-inline → editor text-inline
```

When the user types, the editor emits a graph operation, not a text-range edit:

```
SetField { object: Patient:123, field: name, value: "Anna Meier" }   // NOT "replace chars 18-23"
```

That is genuine WYSIWYG graph editing. **This is exactly today's a2ui addressing
discipline applied to the WRITE side:** today `ActionInvoke { key, ordinal, args }`
carries a behavior invocation up by ADDRESS (trap T2); a field edit is the same
shape — a `SetField { key, field_position, value }` up-frame carrying a WRITE by
address, LE bytes (T3), no handler, no character range. The read side is already
`NodeDelta` (addressed field values down); the write side is the mirror.

## The three surfaces (reuse paths differ)

### Word-like → ProseMirror / Tiptap concepts (the cleanest route)
ProseMirror uses a schema-governed structured document *tree*, not an opaque
string; Tiptap adds a friendlier extension layer with custom node views. A
paragraph / table / medical finding / requirement can look like ordinary editable
document content while keeping a stable graph identity underneath.

```
visible:  Patient: Anna Meyer   Diagnosis: Asthma   Medication: Salbutamol
under:    Node<Patient:123> —has_diagnosis→ Node<Diagnosis:456> …
```

A custom node carries: `class_id, object_id, field_id, ClassView, FieldMask,
template_id`. **Better than embedding OnlyOffice/Collabora** — those edit office
*documents*, forcing constant reverse-mapping of doc mutations into graph meaning;
ProseMirror starts from a semantic, schema-controlled model much nearer our
substrate. **a2ui mapping:** the custom-node carrier IS a `key` (canonical GUID) +
its `ClassView` template; the "document" is an L1 screen whose fields resolve to
child node keys (see the nesting note in the handover) — the flow skin is a
ClassView template whose `template_id` selects inline text-flow layout.

### Excel-like → Univer as a viewport shell, but own the data model
A spreadsheet projection maps cleanly onto OGAR:

| spreadsheet | OGAR |
|---|---|
| row | object |
| column | field or relation |
| cell | FieldView |
| formula | graph operation / derived property |
| sheet | ClassView |
| filter | graph projection |

```
A2 → Patient:123.name
B2 → Patient:123.has_diagnosis → Diagnosis:456.label
D2 → invoice_sum(Patient:123)
```

Editing B2 replaces/creates a graph EDGE, not a cell blob. Formulas become an
approachable surface for graph ops: `=RELATED("has_invoice").SUM("amount")`,
`=COUNT(RELATED("has_diagnosis"))`, `=LOOKUP("treated_by","name")` — compiling
into OGAR operations, not Excel formulas. **Recommendation:** build a thin grid
over the wgpu/ClassView layer (the ratified `a2ui-paint` tier, grid skin) before
adopting a full spreadsheet engine — Excel clones bring a calc/file-compat
universe that becomes ballast. Univer is worth mining for interaction patterns
(selection, fill handles, frozen panes, formula bar). **a2ui mapping:** the grid
skin is a ClassView template rendering a collection of node keys as rows × the
class's field positions as columns; each cell is a `FieldView` at
`(row_key, field_position)`; a cell edit is a `SetField`/edge-op up-frame.

### CAD-like → FreeCAD *semantics*, not its GUI; a Rust/OCCT kernel
FreeCAD is parametric, feature-based: objects expose properties, geometry is
produced from parameters, assemblies connect parts via constraints; its file
structure separates parametric object definitions from GUI representation. A
description like *"object A shaped box x,y,z; feature B attaches at m, way g,
radius f, opacity k"* maps to a feature graph:

```
Object A { shape: Box, dimensions: [x,y,z], material, opacity: k
           Feature B { attachment_position: m, attachment_mode: g, radius: f } }
```

→ semantic object graph → constraint/feature graph → geometry kernel → render mesh
→ wgpu. **Use FreeCAD two ways:** (1) reference architecture — harvest its
object/property/feature/constraint concepts into OGAR; (2) external editing
adapter — FreeCAD edits geometry, OGAR imports/exports property+feature changes.
**Do NOT** build on its Qt/Coin3D GUI — the semantics are valuable, the render
stack is not aligned with wgpu/WASM. **OGAR-native kernel candidates:** Open
CASCADE (precise B-rep, booleans, fillets, STEP/IGES), CadQuery (compact
parametric vocabulary — `box(x,y,z).faces(">Z").workplane().hole(radius)`, close
to the sentence structure), **Truck (Rust-native kernel)**, SolveSpace's
constraint solver, OpenSCAD-style CSG. Store an OGAR feature graph as truth,
render it through a CadQuery/OCCT adapter — not CadQuery code as truth.

## The strongest version — one object-projection editor, many skins

The CAD metaphor may be the universal one. A Word paragraph is an object with flow
constraints; a spreadsheet cell an object attached to a row/column coordinate; a
window an object attached to a layout region; a CAD feature an object attached to
a geometric coordinate. **They all reduce to:**

```
object + properties + attachment + constraints + projection + actions
```

So rather than "Word mode / Excel mode / CAD mode" as separate editors, build ONE
**object projection editor** with interaction skins: **flow / grid / form /
spatial / graph**. A patient appears as a paragraph in one view and a row in
another; **both write through the same field addresses.**

### Template / Content / Behavior separation (the OGAR canon, restated)
- **Content:** objects, values, relations, constraints (the graph).
- **Template:** placement, presentation, editor type, formatting (the ClassView —
  lo-u16 render world; a skin is a `template_id`).
- **Behavior:** ActionDef, validation, navigation, transformations (the Core
  node's `ActionDef`/`KausalSpec` — invoked by address, T2).

Switching template must NOT transform the knowledge itself — "data-as-config."

## How this sits on the ratified a2ui substrate (why it is an extension, not a pivot)

| vision piece | existing a2ui/OGAR mechanism |
|---|---|
| projection skin (flow/grid/form/spatial) | a ClassView **template** (T1: template, not a new vocabulary) |
| addressed field edit `SetField{obj,field,value}` | a WRITE up-frame mirroring `ActionInvoke` (by address, T2; LE bytes, T3) |
| nesting (document/screen with sub-regions) | L1/L2 ClassView slots resolving to child node keys — pure codebook-layer composition, flat wire |
| render skins → pixels | the ratified `a2ui-paint` tier over the resolved `&[FieldView]`/`&[ActionRef]` surface, layout derived per skin |
| formula / feature / constraint ops | `ActionDef` invocations (behavior by address) → OGAR ops; SPO emission is OGAR-side (`emit_action_invocation`) |
| canonical truth | the OGAR object graph; a2ui-rs never owns a second data model |

## Practical first choices (operator)
1. Tiptap/ProseMirror **concepts** for the first document (flow) editor.
2. A custom wgpu grid (grid skin over `a2ui-paint`) for the spreadsheet projection.
3. FreeCAD/CadQuery **semantics** + a Rust (Truck) or OCCT geometry kernel for CAD.
4. OGAR stays canonical; every edit emitted as an addressed graph operation.

## Open design points for a future council (not decided here)
- The WRITE up-frame shape (`SetField` / edge-op / feature-op) — a new frame kind
  in `ogar-a2ui-frame`, or an `ActionInvoke` specialization? (T2/T3 must hold.)
- Where field-edit validation runs (Behavior arm = `ActionDef`/`KausalSpec`,
  OGAR-side; the consumer only addresses it).
- Collaborative editing / CRDT vs the graph-of-active-record as the merge
  authority (the graph is already the canonical merge point).
- The skin↔template_id registry and how a user switches skin without touching
  content.
