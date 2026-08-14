//! `a2ui-paint` — the **paint tier** (charter 3.i): a consumer-agnostic renderer
//! of the addressed fieldview surface.
//!
//! *askama-HTML and wgpu-pixels are two renderers of ONE resolved surface.* This
//! crate takes the SAME `&[FieldView]` / `&[ActionRef]` the askama render
//! consumes (resolved by `a2ui-server`'s `project_node` or the `a2ui-wasm`
//! client's `resolved_fields`/`resolved_actions`) and:
//!
//! 1. **derives a 2-D layout** from each field's `position` (its mask-index
//!    address) and each action's `ordinal` — ADAPTIVELY, per [`Viewport`]
//!    (mobile stacks; desktop uses label/value columns). Layout is a RENDERER
//!    concern (charter T1: a renderer of the shared surface, never a new widget
//!    vocabulary); `position` is a 1-D address, and turning it into pixels is
//!    this crate's job, not stored data.
//! 2. **hit-tests a click** to a [`Hit`] — a field `position` or an action
//!    `ordinal`. A click on an action builds an [`ActionInvoke`](a2ui_core::ActionInvoke)
//!    up-frame carrying only the ordinal ADDRESS (charter T2), never a handler.
//!
//! # SoC — consumer-agnostic by construction
//!
//! The only inputs are OGAR-owned surface types + a viewport. This crate never
//! sees a `ClassView`, a codebook, a `CompiledClass`, or any consumer corpus —
//! ANY resolved surface (synthetic or harvested) paints identically.
//!
//! # The GPU rasterizer (`wgpu` feature)
//!
//! The layout + hit-test core here is pure and testable with nothing running.
//! The `wgpu` feature adds the actual surface draw (WebGPU + WebGL2 via one
//! backend). See [`gpu`]. Per N2 the full pipeline is a follow-up; the paint-
//! DATA path (this module) is what ships first.

#![forbid(unsafe_code)]

use ogar_render_askama::{ActionRef, FieldView};

/// Headless CPU rasterizer: `PaintLayout` → PNG (rects + real glyphs). The third
/// renderer of the one surface — for checking screens with no GPU/display.
#[cfg(feature = "raster")]
pub mod raster;

/// A rectangle in surface pixels: top-left `(x, y)`, size `(w, h)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Does the point `(px, py)` fall inside this rect (half-open on the far
    /// edges, so adjacent rects never both claim a pixel)?
    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// The device class — drives the adaptive layout (mobile stacks label-over-value
/// in one column; desktop uses side-by-side label/value columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    /// Narrow, touch — single column, stacked, larger touch targets.
    Mobile,
    /// Wide, pointer — label/value columns, denser rows.
    Desktop,
}

impl DeviceClass {
    /// Classify by width: below `MOBILE_MAX_WIDTH` is [`Mobile`](DeviceClass::Mobile).
    /// The one relative-sizing knob the operator asked for ("mobile vs PC").
    #[must_use]
    pub fn from_width(width: f32) -> Self {
        if width < Viewport::MOBILE_MAX_WIDTH {
            Self::Mobile
        } else {
            Self::Desktop
        }
    }
}

/// The render viewport — physical size + device class. Relative window sizing +
/// adaptive screen-size detection live here (the layout reads it; nothing is
/// stored on the surface).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Surface width in pixels.
    pub width: f32,
    /// Surface height in pixels.
    pub height: f32,
    /// The device class (drives column vs stacked layout).
    pub device: DeviceClass,
}

impl Viewport {
    /// Widths below this classify as [`DeviceClass::Mobile`].
    pub const MOBILE_MAX_WIDTH: f32 = 640.0;

    /// A viewport of `(width, height)`, device class inferred from width.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            device: DeviceClass::from_width(width),
        }
    }
}

/// The interaction SKIN — one object-projection editor, many skins (the
/// projectional-knowledge-editor thesis). Each skin is a different RENDERER of
/// the SAME resolved `&[FieldView]`/`&[ActionRef]` surface (charter T1: a skin is
/// a render style, never a new widget vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skin {
    /// The form skin — label/value rows (desktop columns; mobile stacked). The
    /// default desktop/detail projection.
    #[default]
    Form,
    /// The flow skin — document-style inline "label: value" runs that wrap at
    /// the viewport width (the Word/prose projection). The first step toward the
    /// projectional document editor.
    Flow,
    /// The grid skin — `position` read as a **row-major cell address** over
    /// `cols` columns.
    ///
    /// This is the first skin where `position` is genuinely a coordinate.
    /// [`Skin::Form`] and [`Skin::Flow`] place by ITERATION ORDER and copy
    /// `position` through only so hit-test can round-trip it; a gap in the
    /// position sequence moves nothing. Here a gap leaves a cell empty, which
    /// is the whole difference and is what the falsifier tests.
    ///
    /// `cols` lives on the variant rather than on [`Viewport`] because the
    /// viewport is device geometry while the column count is a projection
    /// choice — keeping it here preserves the purity contract of
    /// [`layout_with_skin`] (same inputs → same layout).
    Grid {
        /// Columns per row. Clamped to at least 1 — a zero would otherwise be
        /// a division by zero rather than a sensible degenerate case.
        cols: u8,
    },
    /// The tile skin — the surface placed at a **geographic** coordinate read
    /// out of its own fields, rather than at an iteration- or mask-derived
    /// slot.
    ///
    /// This is the map topcoat, and it is still charter-T1 clean: no new
    /// widget vocabulary, no new surface type, no `Skin`-specific field kind.
    /// It reads the SAME `&[FieldView]` the form and flow skins read; it just
    /// takes two of those fields to be a coordinate.
    ///
    /// # Which two fields, and why that is not a private convention
    ///
    /// `rail` names a `(u8:u8)` pair of the V3 content-blind facet register
    /// (`lance-graph` `le-contract.md` §3): the two mask positions
    /// `rail*2` and `rail*2 + 1`. For a spatially-bound domain those two
    /// bytes are a `256×256` centroid tile's **x** and **y** — the workspace
    /// canon binds the axes per domain and names OSM explicitly ("OSM:
    /// literal x/y"). `ogar_osm::GEO_V3_FACET` is the table that says so for
    /// the Geo domain: rails 0–3 are the HHTL cascade tiers heel/hip/twig/leaf,
    /// coarse to fine, so `rail` is also the **zoom** choice.
    ///
    /// A semantic domain binds the same rail to a PQ subspace pair instead —
    /// which is exactly why this skin takes the rail as a parameter and does
    /// not hardcode "geo".
    ///
    /// # One surface is one marker
    ///
    /// A row is a feature, so a `Tile` layout places ONE marker. A trace or a
    /// viewport of features is N surfaces → N layouts, merged by the consumer.
    /// That composes with no new API here because
    /// [`PaintLayout::click_to_action_frame`] takes the key as an ARGUMENT
    /// rather than storing it: a consumer holds `Vec<([u8; 16], PaintLayout)>`
    /// and hit-tests each, so the up-frame is addressed to the right row.
    ///
    /// # Degenerate input falls back rather than piling up at the origin
    ///
    /// A surface whose rail fields are missing, or whose values do not read as
    /// a byte, has no coordinate. Placing it at `(0, 0)` would silently stack
    /// every such surface in one corner and look like a rendering bug; this
    /// skin falls back to [`Skin::Form`] for that surface instead, which is
    /// visibly "no position known".
    Tile {
        /// Which `(u8:u8)` facet rail carries the coordinate. For the Geo
        /// domain: 0 = heel (coarsest) … 3 = leaf (finest).
        rail: u8,
    },
    // Future skins (each a renderer of the same surface): Spatial (CAD),
    // Graph (native). See projectional-knowledge-editor-v1.md.
}

/// A field placed in the layout — its address (`position`) + where its label and
/// value draw. The paint backend draws `label` at `label_rect` and `value` at
/// `value_rect`; a click inside either resolves to this field's `position`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedField {
    /// The field's mask-index address (charter: the layout address).
    pub position: u8,
    /// The display label.
    pub label: String,
    /// The display value.
    pub value: String,
    /// Where the label draws.
    pub label_rect: Rect,
    /// Where the value draws.
    pub value_rect: Rect,
}

impl PlacedField {
    /// The field's whole clickable area (label ∪ value bounding box).
    #[must_use]
    pub fn hit_rect(&self) -> Rect {
        let x = self.label_rect.x.min(self.value_rect.x);
        let y = self.label_rect.y.min(self.value_rect.y);
        let right =
            (self.label_rect.x + self.label_rect.w).max(self.value_rect.x + self.value_rect.w);
        let bottom =
            (self.label_rect.y + self.label_rect.h).max(self.value_rect.y + self.value_rect.h);
        Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        }
    }
}

/// An action placed in the layout — its `ordinal` ADDRESS + button rect + label.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedAction {
    /// The action's ordinal address (the `ActionInvoke` ordinal, charter T2).
    pub ordinal: u32,
    /// The button caption.
    pub label: String,
    /// Where the button draws / is clicked.
    pub rect: Rect,
}

/// The result of hit-testing a click against a [`PaintLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A field at this mask-index position was clicked.
    Field(u8),
    /// An action at this ordinal ADDRESS was clicked — build an
    /// [`ActionInvoke`](a2ui_core::ActionInvoke) with it (charter T2).
    Action(u32),
}

/// A laid-out addressed surface — every field + action placed at pixel rects,
/// ready to draw and to hit-test. Produced by [`layout`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaintLayout {
    /// Placed fields, in surface order.
    pub fields: Vec<PlacedField>,
    /// Placed actions (the button row), in ordinal order.
    pub actions: Vec<PlacedAction>,
    /// Total content height the layout consumed (for scroll extent).
    pub content_height: f32,
}

impl PaintLayout {
    /// Hit-test a click at `(px, py)` — actions take priority (they sit on top of
    /// the field flow), then fields. `None` if the click missed everything.
    #[must_use]
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Hit> {
        for a in &self.actions {
            if a.rect.contains(px, py) {
                return Some(Hit::Action(a.ordinal));
            }
        }
        for f in &self.fields {
            if f.hit_rect().contains(px, py) {
                return Some(Hit::Field(f.position));
            }
        }
        None
    }

    /// Hit-test a click and, if it landed on an ACTION, build the ordinal-
    /// addressed [`ActionInvoke`](a2ui_core::ActionInvoke) up-frame LE bytes for
    /// node `key` (charter T2 — the click sends the address of a behavior, never
    /// a handler). Returns `None` for a miss or a field click (fields edit via a
    /// separate write path, not an action).
    #[must_use]
    pub fn click_to_action_frame(&self, key: [u8; 16], px: f32, py: f32) -> Option<Vec<u8>> {
        match self.hit_test(px, py)? {
            Hit::Action(ordinal) => Some(
                a2ui_core::Frame::ActionInvoke(a2ui_core::ActionInvoke {
                    key,
                    action_ordinal: ordinal,
                    args: Vec::new(),
                })
                .to_le_bytes(),
            ),
            Hit::Field(_) => None,
        }
    }
}

// Layout constants (surface pixels). Deliberately simple + deterministic — a
// real theme would parameterize these; the ADDRESSING (position/ordinal) is what
// matters, not the exact metrics.
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
const DESKTOP_ROW_H: f32 = 28.0;
const DESKTOP_LABEL_W: f32 = 160.0;
const MOBILE_LINE_H: f32 = 24.0;
const ACTION_H: f32 = 32.0;
const ACTION_MIN_W: f32 = 88.0;

/// Lay out an addressed surface for a viewport with the default [`Skin::Form`].
/// See [`layout_with_skin`].
#[must_use]
pub fn layout(fields: &[FieldView], actions: &[ActionRef], vp: &Viewport) -> PaintLayout {
    layout_with_skin(fields, actions, vp, Skin::Form)
}

/// Lay out an addressed surface for a viewport with an explicit [`Skin`] — the
/// paint-DATA path. Turns the 1-D `position`/`ordinal` addresses into 2-D pixel
/// rects; the SKIN chooses the field render style over the SAME resolved surface
/// (Form = label/value rows, adaptive desktop/mobile; Flow = document-style
/// inline wrap). The action row is shared across skins.
///
/// Pure: same `(fields, actions, viewport, skin)` → same layout, testable with
/// nothing running.
#[must_use]
pub fn layout_with_skin(
    fields: &[FieldView],
    actions: &[ActionRef],
    vp: &Viewport,
    skin: Skin,
) -> PaintLayout {
    let (placed_fields, mut y) = match skin {
        Skin::Form => place_form(fields, vp),
        Skin::Flow => place_flow(fields, vp),
        Skin::Grid { cols } => place_grid(fields, vp, cols),
        Skin::Tile { rail } => place_tile(fields, vp, rail),
    };

    // The action button row, below the fields, laid left-to-right (wrapping to a
    // new row when it would overflow the viewport width).
    y += GAP;
    let mut placed_actions = Vec::with_capacity(actions.len());
    let mut ax = PAD;
    for a in actions {
        let w = ACTION_MIN_W.max(a.label.len() as f32 * 8.0 + 2.0 * GAP);
        if ax + w > vp.width - PAD && ax > PAD {
            ax = PAD;
            y += ACTION_H + GAP;
        }
        placed_actions.push(PlacedAction {
            ordinal: a.ordinal,
            label: a.label.clone(),
            rect: Rect {
                x: ax,
                y,
                w,
                h: ACTION_H,
            },
        });
        ax += w + GAP;
    }
    let content_height = if actions.is_empty() { y } else { y + ACTION_H } + PAD;

    PaintLayout {
        fields: placed_fields,
        actions: placed_actions,
        content_height,
    }
}

/// Form skin — label/value rows, ADAPTIVE: desktop uses side-by-side columns;
/// mobile stacks value below label. Returns the placed fields + the y at the
/// bottom of the field area.
fn place_form(fields: &[FieldView], vp: &Viewport) -> (Vec<PlacedField>, f32) {
    let mut placed = Vec::with_capacity(fields.len());
    let mut y = PAD;
    match vp.device {
        DeviceClass::Desktop => {
            let value_x = PAD + DESKTOP_LABEL_W + GAP;
            let value_w = (vp.width - value_x - PAD).max(0.0);
            for f in fields {
                placed.push(PlacedField {
                    position: f.position,
                    label: f.label.clone(),
                    value: f.value.clone(),
                    label_rect: Rect {
                        x: PAD,
                        y,
                        w: DESKTOP_LABEL_W,
                        h: DESKTOP_ROW_H,
                    },
                    value_rect: Rect {
                        x: value_x,
                        y,
                        w: value_w,
                        h: DESKTOP_ROW_H,
                    },
                });
                y += DESKTOP_ROW_H;
            }
        }
        DeviceClass::Mobile => {
            let w = (vp.width - 2.0 * PAD).max(0.0);
            for f in fields {
                placed.push(PlacedField {
                    position: f.position,
                    label: f.label.clone(),
                    value: f.value.clone(),
                    label_rect: Rect {
                        x: PAD,
                        y,
                        w,
                        h: MOBILE_LINE_H,
                    },
                    value_rect: Rect {
                        x: PAD,
                        y: y + MOBILE_LINE_H,
                        w,
                        h: MOBILE_LINE_H,
                    },
                });
                y += 2.0 * MOBILE_LINE_H;
            }
        }
    }
    (placed, y)
}

/// Marker footprint — the clickable dot the tile skin places at the coordinate.
const MARKER: f32 = 16.0;

/// Read one facet byte out of the resolved surface: the field at `position`,
/// whose `value` reads as a `u8`. `None` when the field is absent or its value
/// is not a byte — the two ways a surface can fail to carry a coordinate.
fn facet_byte(fields: &[FieldView], position: u8) -> Option<u8> {
    fields
        .iter()
        .find(|f| f.position == position)
        .and_then(|f| f.value.trim().parse::<u8>().ok())
}

/// Tile skin — the surface placed at the geographic coordinate carried by
/// facet rail `rail` (positions `rail*2` = x, `rail*2 + 1` = y).
///
/// # The y flip is load-bearing, not cosmetic
///
/// TMS tile y increases **north**; screen y increases **down**. Placing a
/// coordinate without the flip mirrors the map about its horizontal axis —
/// which still looks like a map, so it is the kind of bug that ships. The
/// flip is `1.0 - fy`, pinned two-sided by
/// `tile_skin_flips_y_because_tms_y_increases_north`.
fn place_tile(fields: &[FieldView], vp: &Viewport, rail: u8) -> (Vec<PlacedField>, f32) {
    let (Some(x), Some(y)) = (
        facet_byte(fields, rail * 2),
        facet_byte(fields, rail * 2 + 1),
    ) else {
        // No coordinate in this surface — see `Skin::Tile`'s doc: fall back
        // rather than stack every unplaceable surface at the origin.
        return place_form(fields, vp);
    };

    // Byte index -> unit fraction of the tile, then to the drawable box. 255
    // (not 256) because both endpoints must be reachable: a feature at the
    // tile's far edge belongs at the far edge, not one step short of it.
    let fx = f32::from(x) / 255.0;
    let fy = f32::from(y) / 255.0;
    let span_w = (vp.width - 2.0 * PAD - MARKER).max(0.0);
    let span_h = (vp.height - 2.0 * PAD - MARKER).max(0.0);
    let mx = PAD + fx * span_w;
    let my = PAD + (1.0 - fy) * span_h; // <- the TMS -> screen flip

    let marker = Rect {
        x: mx,
        y: my,
        w: MARKER,
        h: MARKER,
    };

    // The two coordinate fields ARE the marker: clicking the dot resolves to
    // the address that placed it. Every other field draws as a callout row
    // beside it — the info card in the shipped Fahrtenbuch shape — so the
    // whole surface stays addressable, not just the coordinate.
    let mut placed = Vec::with_capacity(fields.len());
    let callout_x = mx + MARKER + GAP;
    let callout_w = (vp.width - callout_x - PAD).max(0.0);
    let mut cy = my;
    for f in fields {
        if f.position == rail * 2 || f.position == rail * 2 + 1 {
            placed.push(PlacedField {
                position: f.position,
                label: f.label.clone(),
                value: f.value.clone(),
                label_rect: marker,
                value_rect: marker,
            });
            continue;
        }
        placed.push(PlacedField {
            position: f.position,
            label: f.label.clone(),
            value: f.value.clone(),
            label_rect: Rect {
                x: callout_x,
                y: cy,
                w: callout_w / 2.0,
                h: MOBILE_LINE_H,
            },
            value_rect: Rect {
                x: callout_x + callout_w / 2.0,
                y: cy,
                w: callout_w / 2.0,
                h: MOBILE_LINE_H,
            },
        });
        cy += MOBILE_LINE_H;
    }
    (placed, cy.max(my + MARKER))
}

/// Flow skin — document-style inline "label value" runs flowing left-to-right,
/// wrapping at the viewport width (the Word/prose projection). Each field's
/// `label_rect` is the label run and `value_rect` the value run just after it,
/// so a click on either still resolves to the field's `position`. The first
/// concrete projectional skin over the shared surface.
fn place_flow(fields: &[FieldView], vp: &Viewport) -> (Vec<PlacedField>, f32) {
    const CH_W: f32 = 8.0; // deterministic per-char advance (a real theme measures glyphs)
    let line_h = DESKTOP_ROW_H;
    let max_x = (vp.width - PAD).max(PAD);
    let mut placed = Vec::with_capacity(fields.len());
    let mut x = PAD;
    let mut y = PAD;
    for f in fields {
        let label_w = (f.label.chars().count() as f32 + 1.0) * CH_W; // "label "
        let value_w = (f.value.chars().count().max(1) as f32) * CH_W;
        let run_w = label_w + value_w + GAP;
        if x + run_w > max_x && x > PAD {
            x = PAD;
            y += line_h;
        }
        placed.push(PlacedField {
            position: f.position,
            label: f.label.clone(),
            value: f.value.clone(),
            label_rect: Rect {
                x,
                y,
                w: label_w,
                h: line_h,
            },
            value_rect: Rect {
                x: x + label_w,
                y,
                w: value_w,
                h: line_h,
            },
        });
        x += run_w + GAP;
    }
    let after = if placed.is_empty() { PAD } else { y + line_h };
    (placed, after)
}

/// The cell a position addresses, row-major over `cols` columns.
///
/// `cols == 0` is clamped to 1 rather than panicking: `Skin` is public and
/// `Copy`, so a zero is reachable from any caller, and a one-column layout is
/// the sensible degenerate reading of "no columns fit".
const fn cell_of(position: u8, cols: u8) -> (u16, u8) {
    let c = if cols == 0 { 1 } else { cols };
    ((position / c) as u16, position % c)
}

/// Place fields by ADDRESS: `position` → `(row, col)` → rect.
///
/// No iteration counter appears anywhere below — that is what distinguishes
/// this from [`place_form`] / [`place_flow`], and a gap in the position
/// sequence therefore leaves a cell empty instead of closing up.
///
/// Two fields sharing a `position` cannot arrive from `apply_node_delta`
/// (mask positions are strictly ascending), but this function is public and
/// takes an arbitrary slice. Duplicates are placed at the same rect and BOTH
/// are kept; `hit_test`'s first-match-wins then resolves to the first. That is
/// documented rather than "fixed", because rejecting duplicates would refuse
/// input the other two skins accept.
fn place_grid(fields: &[FieldView], vp: &Viewport, cols: u8) -> (Vec<PlacedField>, f32) {
    let c = if cols == 0 { 1 } else { cols };
    let cf = f32::from(c);
    let cell_w = ((vp.width - 2.0 * PAD - (cf - 1.0) * GAP) / cf).max(1.0);
    let cell_h = DESKTOP_ROW_H;
    let mut placed = Vec::with_capacity(fields.len());
    let mut max_row: u16 = 0;
    for f in fields {
        let (row, col) = cell_of(f.position, c);
        max_row = max_row.max(row);
        let x = PAD + f32::from(col) * (cell_w + GAP);
        let y = PAD + f32::from(row) * (cell_h + GAP);
        // Label above value inside the cell, so a cell is one addressable tile
        // rather than two side-by-side columns (which is `Form`'s reading).
        let half = cell_h / 2.0;
        placed.push(PlacedField {
            position: f.position,
            label: f.label.clone(),
            value: f.value.clone(),
            label_rect: Rect {
                x,
                y,
                w: cell_w,
                h: half,
            },
            value_rect: Rect {
                x,
                y: y + half,
                w: cell_w,
                h: half,
            },
        });
    }
    let after = if placed.is_empty() {
        PAD
    } else {
        PAD + f32::from(max_row + 1) * (cell_h + GAP)
    };
    (placed, after)
}

/// The GPU rasterization backend (WebGPU / WebGL2 via wgpu). OFF by default —
/// the layout + hit-test core above is backend-agnostic and ships first; this
/// wires the actual draw. Headless render-to-texture today (enough to prove the
/// pipeline: buffer → shader → render pass → texture); windowed surface
/// presentation (the only place surface `unsafe` would live — kept OUT of this
/// `#![forbid(unsafe_code)]` crate) is a later addition.
///
/// The layout is already fully positioned by [`layout`]/[`layout_with_skin`];
/// this backend only rasterizes its rects. Glyph/text raster is a follow-up (a
/// textured quad over the same rects); the ADDRESSING (position/ordinal → rect)
/// is what the pipeline proves.
#[cfg(feature = "wgpu")]
pub mod gpu {
    use wgpu::util::DeviceExt;

    use super::{PaintLayout, Rect, Viewport};

    /// Clip-space passthrough + a solid fill — every field/action rect is drawn
    /// as filled geometry at its addressed position.
    const SHADER: &str = r#"
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.55, 0.95, 1.0);
}
"#;

    /// The offscreen target texture format.
    pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    /// The clear colour a paint surface starts from before drawing the layout.
    #[must_use]
    pub fn clear_color() -> wgpu::Color {
        wgpu::Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }
    }

    /// Convert every rect in a laid-out surface — each field's `label_rect` and
    /// `value_rect`, each action's `rect` — into a clip-space (NDC) triangle
    /// list: 6 vertices (two triangles) per rect, fields first then actions.
    /// This is the pure geometry the GPU uploads; it is unit-tested without a
    /// GPU so the rasterizer stays a thin consumer.
    ///
    /// Device pixels (top-left origin, y-down) map to NDC (`[-1, 1]`, y-up):
    /// `ndc_x = 2·x/W − 1`, `ndc_y = 1 − 2·y/H`.
    #[must_use]
    pub fn to_ndc_vertices(layout: &PaintLayout, vp: &Viewport) -> Vec<[f32; 2]> {
        let w = vp.width.max(1.0);
        let h = vp.height.max(1.0);
        let ndc = |x: f32, y: f32| [2.0 * x / w - 1.0, 1.0 - 2.0 * y / h];
        let mut out = Vec::with_capacity((layout.fields.len() * 2 + layout.actions.len()) * 6);
        let mut push = |r: &Rect| {
            let tl = ndc(r.x, r.y);
            let tr = ndc(r.x + r.w, r.y);
            let bl = ndc(r.x, r.y + r.h);
            let br = ndc(r.x + r.w, r.y + r.h);
            // Triangle 1: TL, TR, BL. Triangle 2: TR, BR, BL.
            out.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        };
        for f in &layout.fields {
            push(&f.label_rect);
            push(&f.value_rect);
        }
        for a in &layout.actions {
            push(&a.rect);
        }
        out
    }

    /// Flatten clip-space vertices to native-endian bytes for the vertex-buffer
    /// upload — no `unsafe`, so the crate's `#![forbid(unsafe_code)]` holds
    /// (bytemuck/pointer casts are avoided for one small buffer).
    fn vertex_bytes(verts: &[[f32; 2]]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(verts.len() * 8);
        for v in verts {
            bytes.extend_from_slice(&v[0].to_ne_bytes());
            bytes.extend_from_slice(&v[1].to_ne_bytes());
        }
        bytes
    }

    /// A headless GPU painter: an owned device/queue + the quad-fill pipeline.
    /// Consumes a [`PaintLayout`] (the addressed rects) and draws it into an
    /// offscreen texture — no window, no surface.
    pub struct GpuPainter {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
    }

    impl GpuPainter {
        /// Create a headless painter. Returns `None` if no adapter is available
        /// (a box with neither GPU nor software rasterizer). Accepts the
        /// fallback adapter so it works without real hardware.
        pub async fn new() -> Option<Self> {
            // `InstanceDescriptor` no longer implements `Default` — it carries a
            // boxed display handle. `new_without_display_handle()` is the
            // headless constructor, which is exactly what this painter is.
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                    // Limit bucketing is a FINGERPRINTING defence for hosts that
                    // expose wgpu to untrusted content. This painter is headless
                    // and in-process, so the real limits are the useful ones.
                    apply_limit_buckets: false,
                })
                // Now a `Result`, not an `Option`: "no adapter" carries a reason.
                .await
                .ok()?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("a2ui-paint device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                    memory_hints: wgpu::MemoryHints::default(),
                    // The old trailing `None` argument, now a field.
                    trace: wgpu::Trace::Off,
                })
                .await
                .ok()?;

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("a2ui-paint quad shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("a2ui-paint layout"),
                bind_group_layouts: &[],
                // Push constants were renamed to "immediates" and are declared as
                // a SIZE rather than as ranges. This pipeline uses none.
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("a2ui-paint pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TARGET_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

            Some(Self {
                device,
                queue,
                pipeline,
            })
        }

        /// Rasterize a laid-out surface into a fresh offscreen texture sized to
        /// the viewport, over `clear`. Returns the texture; a windowed consumer
        /// presents it, a test consumer copies it back.
        #[must_use]
        pub fn render_to_texture(
            &self,
            layout: &PaintLayout,
            vp: &Viewport,
            clear: wgpu::Color,
        ) -> wgpu::Texture {
            let verts = to_ndc_vertices(layout, vp);
            let bytes = vertex_bytes(&verts);
            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("a2ui-paint vertices"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                });

            let target = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("a2ui-paint target"),
                size: wgpu::Extent3d {
                    width: vp.width.max(1.0) as u32,
                    height: vp.height.max(1.0) as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TARGET_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("a2ui-paint encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("a2ui-paint pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        // 2-D target, so there is no depth slice to select.
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    multiview_mask: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..(verts.len() as u32), 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            target
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Vec<FieldView> {
        vec![
            FieldView {
                position: 0,
                label: "Total".into(),
                predicate: "amount_total".into(),
                value: "42".into(),
            },
            FieldView {
                position: 2,
                label: "Partner".into(),
                predicate: "partner_id".into(),
                value: "7".into(),
            },
        ]
    }
    /// A geo surface: rail 0 carries the coordinate (`heel.x` / `heel.y`, the
    /// `ogar_osm::GEO_V3_FACET` reading), plus one non-coordinate field so the
    /// callout path is exercised too.
    fn geo_fields(x: u8, y: u8) -> Vec<FieldView> {
        vec![
            FieldView {
                position: 0,
                label: "Heel x".into(),
                predicate: "geo:heel.x".into(),
                value: x.to_string(),
            },
            FieldView {
                position: 1,
                label: "Heel y".into(),
                predicate: "geo:heel.y".into(),
                value: y.to_string(),
            },
            FieldView {
                position: 11,
                label: "Identity".into(),
                predicate: "geo:identity".into(),
                value: "0".into(),
            },
        ]
    }

    fn marker_of(layout: &PaintLayout) -> Rect {
        layout
            .fields
            .iter()
            .find(|f| f.position == 0)
            .expect("the x field must be placed")
            .label_rect
    }

    fn map_vp() -> Viewport {
        Viewport::new(1000.0, 800.0)
    }

    #[test]
    fn tile_skin_places_the_marker_at_the_rail_coordinate() {
        let vp = map_vp();
        let west = marker_of(&layout_with_skin(
            &geo_fields(0, 128),
            &[],
            &vp,
            Skin::Tile { rail: 0 },
        ));
        let east = marker_of(&layout_with_skin(
            &geo_fields(255, 128),
            &[],
            &vp,
            Skin::Tile { rail: 0 },
        ));
        assert!(
            west.x < east.x,
            "x byte 0 must place west of x byte 255 ({} vs {})",
            west.x,
            east.x
        );
        // Both endpoints are REACHABLE — the /255 (not /256) divisor. A far-edge
        // feature belongs at the far edge, not one step short of it.
        assert!((west.x - PAD).abs() < f32::EPSILON);
        assert!((east.x + MARKER - (vp.width - PAD)).abs() < 0.01);
    }

    #[test]
    fn tile_skin_flips_y_because_tms_y_increases_north() {
        // The bug this pins mirrors the map about its horizontal axis, which
        // still LOOKS like a map — so it is the kind that ships. Two-sided:
        // north must be up AND south must be down.
        let vp = map_vp();
        let north = marker_of(&layout_with_skin(
            &geo_fields(128, 255),
            &[],
            &vp,
            Skin::Tile { rail: 0 },
        ));
        let south = marker_of(&layout_with_skin(
            &geo_fields(128, 0),
            &[],
            &vp,
            Skin::Tile { rail: 0 },
        ));
        assert!(
            north.y < south.y,
            "TMS y=255 is NORTH and must draw ABOVE y=0 ({} vs {})",
            north.y,
            south.y
        );
        assert!((north.y - PAD).abs() < f32::EPSILON);
        assert!((south.y + MARKER - (vp.height - PAD)).abs() < 0.01);
    }

    #[test]
    fn a_surface_without_a_coordinate_falls_back_instead_of_stacking_at_the_origin() {
        let vp = map_vp();
        let tile = Skin::Tile { rail: 0 };
        // No rail-0 fields at all.
        let absent = layout_with_skin(&fields(), &[], &vp, tile);
        assert_eq!(
            absent.fields,
            layout_with_skin(&fields(), &[], &vp, Skin::Form).fields
        );

        // Present but not a byte — the other way a coordinate can be missing.
        let mut junk = geo_fields(10, 20);
        junk[0].value = "n/a".into();
        let unparseable = layout_with_skin(&junk, &[], &vp, tile);
        assert_eq!(
            unparseable.fields,
            layout_with_skin(&junk, &[], &vp, Skin::Form).fields
        );

        // Anti-vacuity: a WELL-FORMED surface must NOT take the fallback, or
        // the two assertions above would pass for a skin that never places
        // anything.
        let good = layout_with_skin(&geo_fields(10, 20), &[], &vp, tile);
        assert_ne!(
            good.fields,
            layout_with_skin(&geo_fields(10, 20), &[], &vp, Skin::Form).fields
        );
    }

    #[test]
    fn clicking_the_marker_round_trips_the_coordinate_address() {
        let vp = map_vp();
        let layout = layout_with_skin(&geo_fields(64, 192), &[], &vp, Skin::Tile { rail: 0 });
        let m = marker_of(&layout);
        // The dot resolves to the address that placed it, so an up-frame from a
        // map click is addressed, never a handler (charter T2).
        assert_eq!(
            layout.hit_test(m.x + m.w / 2.0, m.y + m.h / 2.0),
            Some(Hit::Field(0))
        );
        // A non-coordinate field is still addressable from its callout row.
        let callout = layout.fields.iter().find(|f| f.position == 11).unwrap();
        let r = callout.hit_rect();
        assert_eq!(layout.hit_test(r.x + 1.0, r.y + 1.0), Some(Hit::Field(11)));
    }

    #[test]
    fn distinct_coordinates_place_at_distinct_pixels_and_the_skin_is_pure() {
        let vp = map_vp();
        let tile = Skin::Tile { rail: 0 };
        let a = marker_of(&layout_with_skin(&geo_fields(10, 20), &[], &vp, tile));
        let b = marker_of(&layout_with_skin(&geo_fields(200, 30), &[], &vp, tile));
        assert_ne!((a.x, a.y), (b.x, b.y));
        // Pure: same inputs -> same layout, so N surfaces can be laid out
        // independently and merged by the consumer.
        let again = marker_of(&layout_with_skin(&geo_fields(10, 20), &[], &vp, tile));
        assert_eq!((a.x, a.y), (again.x, again.y));
    }

    #[test]
    fn a_finer_rail_reads_a_different_pair_of_positions() {
        // `rail` is the zoom choice: rail 0 = heel (coarsest) ... rail 3 = leaf.
        // A surface carrying ONLY rail 0 must not be silently placed by rail 3.
        let vp = map_vp();
        let only_heel = geo_fields(255, 255);
        let by_leaf = layout_with_skin(&only_heel, &[], &vp, Skin::Tile { rail: 3 });
        assert_eq!(
            by_leaf.fields,
            layout_with_skin(&only_heel, &[], &vp, Skin::Form).fields,
            "rail 3 must not read rail 0's bytes"
        );
    }

    fn actions() -> Vec<ActionRef> {
        vec![
            ActionRef {
                ordinal: 0,
                label: "View".into(),
            },
            ActionRef {
                ordinal: 1,
                label: "Post".into(),
            },
        ]
    }

    #[test]
    fn desktop_lays_out_label_value_columns_and_hit_tests_by_address() {
        let vp = Viewport::new(1000.0, 800.0);
        assert_eq!(vp.device, DeviceClass::Desktop);
        let lay = layout(&fields(), &actions(), &vp);

        // Two fields, each a row; the value column is to the right of the label.
        assert_eq!(lay.fields.len(), 2);
        assert_eq!(lay.fields[0].position, 0);
        assert!(lay.fields[0].value_rect.x > lay.fields[0].label_rect.x);
        // A click in field 0's value cell resolves to its position ADDRESS.
        let vr = lay.fields[0].value_rect;
        assert_eq!(
            lay.hit_test(vr.x + 2.0, vr.y + 2.0),
            Some(Hit::Field(0)),
            "value cell → field 0"
        );
        // A click on the second action button resolves to ordinal 1.
        let a1 = &lay.actions[1];
        assert_eq!(
            lay.hit_test(a1.rect.x + 1.0, a1.rect.y + 1.0),
            Some(Hit::Action(1))
        );
        // A miss returns None.
        assert_eq!(lay.hit_test(-5.0, -5.0), None);
    }

    #[test]
    fn mobile_stacks_value_below_label() {
        let vp = Viewport::new(360.0, 720.0);
        assert_eq!(vp.device, DeviceClass::Mobile);
        let lay = layout(&fields(), &actions(), &vp);
        // Value is stacked BELOW the label (same column), not to the right.
        assert_eq!(lay.fields[0].value_rect.x, lay.fields[0].label_rect.x);
        assert!(lay.fields[0].value_rect.y > lay.fields[0].label_rect.y);
    }

    #[test]
    fn action_click_builds_the_ordinal_addressed_up_frame() {
        let vp = Viewport::new(1000.0, 800.0);
        let lay = layout(&fields(), &actions(), &vp);
        let key = [9u8; 16];
        let a1 = &lay.actions[1];
        let bytes = lay
            .click_to_action_frame(key, a1.rect.x + 1.0, a1.rect.y + 1.0)
            .expect("action hit builds a frame");
        // Round-trips to an ActionInvoke carrying only the ordinal address (T2).
        match a2ui_core::Frame::from_le_bytes(&bytes).unwrap() {
            a2ui_core::Frame::ActionInvoke(ai) => {
                assert_eq!(ai.key, key);
                assert_eq!(ai.action_ordinal, 1);
                assert!(ai.args.is_empty(), "no handler on the wire");
            }
            a2ui_core::Frame::NodeDelta(_) => panic!("expected ActionInvoke"),
        }
        // A field click does NOT build an action frame (fields edit separately).
        let vr = lay.fields[0].value_rect;
        assert!(
            lay.click_to_action_frame(key, vr.x + 2.0, vr.y + 2.0)
                .is_none()
        );
    }

    #[test]
    fn flow_skin_lays_fields_inline_over_the_same_surface() {
        // The projectional thesis: one resolved surface, two skins. Form stacks
        // label/value rows; Flow flows "label value" runs inline (the document
        // projection). Same fields/actions, different render — never a new
        // vocabulary (T1).
        let vp = Viewport::new(1000.0, 800.0);
        let form = layout_with_skin(&fields(), &actions(), &vp, Skin::Form);
        let flow = layout_with_skin(&fields(), &actions(), &vp, Skin::Flow);
        assert_eq!(flow.fields.len(), 2);
        assert_eq!(flow.actions.len(), 2);
        // Flow: the two short fields sit on ONE line (same y). Form stacks them.
        assert_eq!(
            flow.fields[0].label_rect.y, flow.fields[1].label_rect.y,
            "flow is inline"
        );
        assert_ne!(
            form.fields[0].label_rect.y, form.fields[1].label_rect.y,
            "form stacks rows"
        );
        // The value run sits right after the label run (inline document flow).
        assert!(
            flow.fields[0].value_rect.x
                >= flow.fields[0].label_rect.x + flow.fields[0].label_rect.w - 0.01
        );
        // Hit-testing still resolves by ADDRESS in the flow skin.
        let vr = flow.fields[1].value_rect;
        assert_eq!(flow.hit_test(vr.x + 1.0, vr.y + 1.0), Some(Hit::Field(2)));
        // The bare `layout` is the Form skin (back-compat).
        assert_eq!(layout(&fields(), &actions(), &vp), form);
    }

    #[test]
    fn empty_surface_lays_out_without_panic() {
        let vp = Viewport::new(800.0, 600.0);
        let lay = layout(&[], &[], &vp);
        assert!(lay.fields.is_empty() && lay.actions.is_empty());
        assert_eq!(lay.hit_test(10.0, 10.0), None);
    }

    /// Three tiles at positions **0, 1, 3** over 2 columns.
    ///
    /// The gap at position 2 is the entire point: with contiguous positions,
    /// address-driven and order-driven placement coincide and the test would
    /// pass with `place_flow` substituted. Here position 3 must land in
    /// **column 1** of row 1, because it is an address — an order-driven
    /// placer would put the third *iterated* field at column 0.
    #[test]
    fn grid_places_by_address_so_a_position_gap_leaves_its_cell_empty() {
        let tiles = vec![
            FieldView {
                position: 0,
                label: "A".into(),
                predicate: "a".into(),
                value: "1".into(),
            },
            FieldView {
                position: 1,
                label: "B".into(),
                predicate: "b".into(),
                value: "2".into(),
            },
            FieldView {
                position: 3,
                label: "D".into(),
                predicate: "d".into(),
                value: "4".into(),
            },
        ];
        let vp = Viewport::new(1000.0, 800.0);
        let lay = layout_with_skin(&tiles, &[], &vp, Skin::Grid { cols: 2 });
        let at = |p: u8| {
            lay.fields
                .iter()
                .find(|f| f.position == p)
                .unwrap_or_else(|| panic!("position {p} was not placed"))
        };

        // 0 and 1 share a row.
        assert_eq!(at(0).label_rect.y, at(1).label_rect.y);
        // 3 is on the NEXT row…
        assert!(at(3).label_rect.y > at(0).label_rect.y);
        // …and in column 1, NOT column 0. This is the assertion an
        // order-driven placer fails: it would place the third field it
        // iterated at the start of the row.
        assert_eq!(at(3).label_rect.x, at(1).label_rect.x);
        assert!(at(3).label_rect.x > at(0).label_rect.x);
    }

    #[test]
    fn grid_honours_its_column_count_and_is_safe_at_zero() {
        let tiles: Vec<FieldView> = (0..4)
            .map(|i| FieldView {
                position: i,
                label: format!("F{i}"),
                predicate: "p".into(),
                value: "v".into(),
            })
            .collect();
        let vp = Viewport::new(1000.0, 800.0);

        // One column: every tile on its own row, matching Form's one-per-row
        // reading. An implementation that ignored `cols` and hardcoded a width
        // fails here.
        let one = layout_with_skin(&tiles, &[], &vp, Skin::Grid { cols: 1 });
        let ys: Vec<f32> = one.fields.iter().map(|f| f.label_rect.y).collect();
        assert!(ys.windows(2).all(|w| w[1] > w[0]), "cols=1 must stack");
        assert!(
            one.fields
                .iter()
                .all(|f| f.label_rect.x == one.fields[0].label_rect.x)
        );

        // Four columns: all four on ONE row. Two-sided against the above — a
        // placer that always stacked would pass the first half alone.
        let four = layout_with_skin(&tiles, &[], &vp, Skin::Grid { cols: 4 });
        let y0 = four.fields[0].label_rect.y;
        assert!(
            four.fields.iter().all(|f| f.label_rect.y == y0),
            "cols=4 must be one row"
        );
        // …and the columns are genuinely distinct, not stacked at one x.
        let xs: Vec<f32> = four.fields.iter().map(|f| f.label_rect.x).collect();
        assert!(xs.windows(2).all(|w| w[1] > w[0]));

        // cols = 0 degenerates to one column rather than dividing by zero.
        let zero = layout_with_skin(&tiles, &[], &vp, Skin::Grid { cols: 0 });
        assert_eq!(zero.fields.len(), 4);
        assert!(zero.fields.iter().all(|f| f.label_rect.w.is_finite()));
        assert_eq!(
            zero.fields
                .iter()
                .map(|f| f.label_rect.y)
                .collect::<Vec<_>>(),
            ys,
            "cols=0 must read as cols=1"
        );
    }

    #[test]
    fn a_grid_click_still_resolves_to_an_ordinal_address() {
        // T2 by construction: Grid writes the real `position` into
        // `PlacedField.position` like every other placer, so `hit_test` and
        // `click_to_action_frame` are UNCHANGED and the address path cannot
        // diverge. This asserts that rather than assuming it.
        let vp = Viewport::new(1000.0, 800.0);
        let lay = layout_with_skin(&fields(), &actions(), &vp, Skin::Grid { cols: 2 });
        let a = &lay.actions[1];
        let hit = lay.hit_test(a.rect.x + 1.0, a.rect.y + 1.0);
        assert_eq!(hit, Some(Hit::Action(1)));
        // A field cell hit-tests to its ADDRESS, and position 2 is the field's
        // own position — not its index in the slice.
        let f = lay.fields.iter().find(|f| f.position == 2).unwrap();
        assert_eq!(
            lay.hit_test(f.label_rect.x + 1.0, f.label_rect.y + 1.0),
            Some(Hit::Field(2))
        );
    }

    #[cfg(feature = "wgpu")]
    #[test]
    fn ndc_vertices_are_six_per_rect_in_clip_space() {
        // The GPU backend's geometry is pure (no device needed): every rect the
        // layout placed becomes two triangles = 6 clip-space vertices.
        let vp = Viewport::new(1000.0, 800.0);
        let lay = layout(&fields(), &actions(), &vp);
        let verts = gpu::to_ndc_vertices(&lay, &vp);
        // Each field contributes label_rect + value_rect; each action one rect.
        let rects = lay.fields.len() * 2 + lay.actions.len();
        assert_eq!(verts.len(), rects * 6, "6 vertices per placed rect");
        // Everything the layout placed inside the viewport maps into the clip cube.
        for v in &verts {
            assert!(
                (-1.0..=1.0).contains(&v[0]) && (-1.0..=1.0).contains(&v[1]),
                "vertex {v:?} outside NDC"
            );
        }
    }
}
