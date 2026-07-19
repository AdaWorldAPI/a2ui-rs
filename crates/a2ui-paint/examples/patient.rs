//! Patient structure-render — the Session B "structure oracle" half of the
//! Patient furnace turn (see MedCare-rs `.claude/CODEGEN-PARITY-SESSION-SPLIT.md`).
//!
//! Renders the Patient concept's **list** and **detail** screens (desktop +
//! mobile) to deterministic PNGs through the paint tier — the "same screens?"
//! structure check that pairs with Session A's value oracle. The field basis
//! mirrors the real `MedcareClassView::for_patient` shape (concept `0x0901`,
//! `PATIENT_LIST` = positions 0..6, `PATIENT_DETAIL` = 0..10) but the labels
//! are the **canonical English** leaf-renames of the German medcare columns —
//! the PII non-negotiable (never emit German PII labels) is honored at this
//! boundary, exactly as the medcare adapter guarantees on the value side.
//! Values are clearly-synthetic samples, never real patient data.
//!
//! Run:
//! ```sh
//! cargo +1.95.0 run -p a2ui-paint --features raster --example patient -- <out_dir>
//! ```
//! Writes `patient_{list,detail}_{desktop,mobile}.png` into `<out_dir>` (cwd
//! by default).

use a2ui_paint::raster::{RasterTheme, render_png};
use a2ui_paint::{Viewport, layout};
use ogar_render_askama::{ActionRef, FieldView};

/// One Patient field at its mask-index `position` (the layout ADDRESS), with
/// its canonical (English) label + predicate IRI + a synthetic display value.
fn field(position: u8, label: &str, predicate: &str, value: &str) -> FieldView {
    FieldView {
        position,
        label: label.to_string(),
        predicate: predicate.to_string(),
        value: value.to_string(),
    }
}

/// The full Patient field basis, in `MedcareClassView` mask-position order,
/// leaf-renamed to canonical English labels. Positions 0..6 are the list
/// projection; 0..10 the detail projection.
fn patient_basis() -> Vec<FieldView> {
    vec![
        field(0, "Last name", "ogit.Healthcare:lastname", "Sample"),
        field(1, "First name", "ogit.Healthcare:firstname", "Pat"),
        field(2, "Birth date", "ogit.Healthcare:birth", "1975-06-15"),
        field(3, "Postal code", "ogit.Healthcare:zip", "10115"),
        field(4, "City", "ogit.Healthcare:place", "Anytown"),
        field(5, "Phone", "ogit:otherPhone", "+00 000 0000"),
        field(6, "Mobile", "ogit.Healthcare:mobil", "+00 000 0001"),
        field(7, "E-mail", "ogit.Healthcare:mail", "pat@example.org"),
        field(8, "Street", "ogit.Healthcare:street", "1 Example Street"),
        field(9, "Title", "ogit.Healthcare:title1", "Dr."),
    ]
}

/// The Patient action set — ordinals into the class's `ActionDef` set (the
/// invocation ADDRESSES the up-frame carries, charter T2). Never handlers.
fn patient_actions() -> Vec<ActionRef> {
    vec![
        ActionRef {
            ordinal: 0,
            label: "Open".into(),
        },
        ActionRef {
            ordinal: 1,
            label: "Edit".into(),
        },
        ActionRef {
            ordinal: 2,
            label: "New".into(),
        },
    ]
}

/// The list screen surface — the first 6 fields (`PATIENT_LIST`).
fn patient_list() -> (Vec<FieldView>, Vec<ActionRef>) {
    let fields: Vec<FieldView> = patient_basis().into_iter().take(6).collect();
    (fields, patient_actions())
}

/// The detail screen surface — all 10 fields (`PATIENT_DETAIL`).
fn patient_detail() -> (Vec<FieldView>, Vec<ActionRef>) {
    (patient_basis(), patient_actions())
}

fn render_to(dir: &str, name: &str, fields: &[FieldView], actions: &[ActionRef], vp: &Viewport) {
    let lay = layout(fields, actions, vp);
    let theme = RasterTheme {
        show_addresses: true,
        ..RasterTheme::default()
    };
    let png = render_png(&lay, vp, &theme);
    let path = if dir.is_empty() {
        format!("{name}.png")
    } else {
        format!("{}/{name}.png", dir.trim_end_matches('/'))
    };
    std::fs::write(&path, &png).expect("write png");
    eprintln!(
        "wrote {path} — {}x{} ({} bytes), {} fields, {} actions",
        vp.width as u32,
        vp.height.max(lay.content_height) as u32,
        png.len(),
        lay.fields.len(),
        lay.actions.len(),
    );
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_default();

    let desktop = Viewport::new(820.0, 420.0);
    let mobile = Viewport::new(390.0, 780.0);

    let (lf, la) = patient_list();
    let (df, da) = patient_detail();

    render_to(&dir, "patient_list_desktop", &lf, &la, &desktop);
    render_to(&dir, "patient_detail_desktop", &df, &da, &desktop);
    render_to(&dir, "patient_list_mobile", &lf, &la, &mobile);
    render_to(&dir, "patient_detail_mobile", &df, &da, &mobile);
}
