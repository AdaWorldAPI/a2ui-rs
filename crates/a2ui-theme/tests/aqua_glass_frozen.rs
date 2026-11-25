//! G1b (2026-11-25): the accepted Aqua Glass is frozen. Any change to the stylesheet fails
//! here; a deliberate change replaces `tests/visual/aqua-glass-accepted.css` and the
//! reference images in the same commit, after a new visual acceptance.

#[test]
fn the_accepted_aqua_glass_is_unchanged() {
    let shipped = a2ui_theme::asset("aqua-glass/aqua-glass.css")
        .expect("the Aqua Glass stylesheet is an asset")
        .bytes;
    let accepted = include_bytes!("visual/aqua-glass-accepted.css");
    assert!(
        shipped == accepted.as_slice(),
        "aqua-glass.css differs from the accepted stylesheet"
    );
    for png in [
        include_bytes!("visual/aqua-glass-dashboard.png").as_slice(),
        include_bytes!("visual/aqua-glass-start-1440x900.png"),
        include_bytes!("visual/aqua-glass-ablation-default-rear-prism.png"),
        include_bytes!("visual/aqua-glass-ablation-rear-only.png"),
        include_bytes!("visual/aqua-glass-ablation-prism-only.png"),
        include_bytes!("visual/aqua-glass-ablation-neutral-both-off.png"),
        include_bytes!("visual/aqua-glass-ablation-glass-off.png"),
    ] {
        assert!(png.starts_with(b"\x89PNG"));
    }
}

/// The ablation switches stay in the shipped sidecar: `?aqua-off=glass,rear,beam`.
#[test]
fn the_ablation_switches_ship_with_the_material() {
    let js = std::str::from_utf8(
        a2ui_theme::asset("aqua-glass/aqua-sidecar.js")
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert!(js.contains("get('aqua-off')"));
    assert!(js.contains("'--aqua-l-' + t"));
    assert!(js.contains("'data-aqua-glass', 'off'"));
}
