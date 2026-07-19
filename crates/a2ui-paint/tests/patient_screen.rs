//! Structure-oracle proof for the Patient furnace turn (Session B).
//!
//! Asserts the Patient list/detail surfaces lay out as ADDRESSED screens
//! (fields at their mask positions, actions at their ordinals), that action
//! clicks fire by address while field clicks do not (charter T2), and — under
//! the `raster` feature — that the rendered PNG is non-empty and deterministic.
//! The field basis mirrors `MedcareClassView::for_patient` (concept `0x0901`)
//! with canonical English labels (PII non-negotiable). Values are synthetic.

use a2ui_paint::{Hit, Viewport, layout};
use ogar_render_askama::{ActionRef, FieldView};

fn field(position: u8, label: &str, value: &str) -> FieldView {
    FieldView {
        position,
        label: label.to_string(),
        predicate: format!("ogit.Healthcare:{position}"),
        value: value.to_string(),
    }
}

fn patient_basis() -> Vec<FieldView> {
    vec![
        field(0, "Last name", "Sample"),
        field(1, "First name", "Pat"),
        field(2, "Birth date", "1975-06-15"),
        field(3, "Postal code", "10115"),
        field(4, "City", "Anytown"),
        field(5, "Phone", "+00 000 0000"),
        field(6, "Mobile", "+00 000 0001"),
        field(7, "E-mail", "pat@example.org"),
        field(8, "Street", "1 Example Street"),
        field(9, "Title", "Dr."),
    ]
}

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

fn patient_list() -> Vec<FieldView> {
    patient_basis().into_iter().take(6).collect()
}

#[test]
fn list_projection_has_six_fields_at_positions_0_through_5() {
    let fields = patient_list();
    let vp = Viewport::new(820.0, 420.0);
    let lay = layout(&fields, &patient_actions(), &vp);
    let positions: Vec<u8> = lay.fields.iter().map(|f| f.position).collect();
    assert_eq!(positions, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn detail_projection_has_ten_fields_at_positions_0_through_9() {
    let vp = Viewport::new(820.0, 420.0);
    let lay = layout(&patient_basis(), &patient_actions(), &vp);
    let positions: Vec<u8> = lay.fields.iter().map(|f| f.position).collect();
    assert_eq!(positions, (0u8..10).collect::<Vec<_>>());
}

#[test]
fn actions_are_addressed_by_ordinal() {
    let vp = Viewport::new(820.0, 420.0);
    let lay = layout(&patient_basis(), &patient_actions(), &vp);
    let ordinals: Vec<u32> = lay.actions.iter().map(|a| a.ordinal).collect();
    assert_eq!(ordinals, vec![0, 1, 2]);
}

/// Charter T2: a click on an action resolves to its ORDINAL address and builds
/// an `ActionInvoke` up-frame; a click on a field does NOT fire an action
/// (fields edit via a separate write path). Behavior travels by address only.
#[test]
fn action_click_fires_by_address_field_click_does_not() {
    let vp = Viewport::new(820.0, 420.0);
    let lay = layout(&patient_basis(), &patient_actions(), &vp);
    let key = [0u8; 16];

    // Action "Edit" (ordinal 1) — click its rect center.
    let a = &lay.actions[1];
    let (ax, ay) = (a.rect.x + a.rect.w / 2.0, a.rect.y + a.rect.h / 2.0);
    assert_eq!(lay.hit_test(ax, ay), Some(Hit::Action(1)));
    assert!(
        lay.click_to_action_frame(key, ax, ay).is_some(),
        "an action click must produce an ActionInvoke up-frame (T2)"
    );

    // Field "Last name" (position 0) — click its rect center.
    let f = &lay.fields[0];
    let hr = f.hit_rect();
    let (fx, fy) = (hr.x + hr.w / 2.0, hr.y + hr.h / 2.0);
    assert_eq!(
        lay.click_to_action_frame(key, fx, fy),
        None,
        "a field click must NOT fire an action (behavior rides no surface, T2)"
    );
}

#[cfg(feature = "raster")]
#[test]
fn detail_renders_deterministic_nonempty_png() {
    use a2ui_paint::raster::{RasterTheme, render_png};
    let vp = Viewport::new(820.0, 420.0);
    let lay = layout(&patient_basis(), &patient_actions(), &vp);
    let theme = RasterTheme::default();
    let a = render_png(&lay, &vp, &theme);
    let b = render_png(&lay, &vp, &theme);
    assert!(!a.is_empty(), "png must be non-empty");
    assert_eq!(a, b, "render must be deterministic (byte-identical)");
    // PNG magic — proves it is a real encoded image, not stub bytes.
    assert_eq!(&a[0..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
}
