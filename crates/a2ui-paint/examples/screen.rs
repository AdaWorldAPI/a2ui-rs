//! `screen` — render one demo addressed surface to `screen.png` so a screen can
//! be *looked at*. Run with the raster feature:
//!
//! ```sh
//! cargo +1.95.0 run -p a2ui-paint --features raster --example screen -- screen.png
//! ```
//!
//! It builds a resolved `(FieldView, ActionRef)` surface (the SAME shape the
//! server projects and the wasm client resolves), lays it out with the Form
//! skin, and rasterises it to a PNG. Pass an output path (default `screen.png`)
//! and optionally `mobile` to see the adaptive stacked layout.

use a2ui_paint::raster::{RasterTheme, render_png};
use a2ui_paint::{Viewport, layout};
use ogar_render_askama::{ActionRef, FieldView};

fn field(position: u8, label: &str, value: &str) -> FieldView {
    FieldView {
        position,
        label: label.to_string(),
        predicate: format!("ogit:{label}"),
        value: value.to_string(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .iter()
        .skip(1)
        .find(|a| a.ends_with(".png"))
        .cloned()
        .unwrap_or_else(|| "screen.png".to_string());
    let mobile = args.iter().any(|a| a == "mobile");

    // A harvested-shape invoice screen: sparse positions (1 projected out).
    let fields = [
        field(0, "Total", "1240.50 EUR"),
        field(2, "Partner", "Acme GmbH"),
        field(3, "Status", "posted"),
        field(5, "Due date", "2026-08-01"),
        field(6, "Reference", "INV-2026-0417"),
    ];
    let actions = [
        ActionRef {
            ordinal: 0,
            label: "View".into(),
        },
        ActionRef {
            ordinal: 1,
            label: "Post".into(),
        },
        ActionRef {
            ordinal: 2,
            label: "Send reminder".into(),
        },
    ];

    let vp = if mobile {
        Viewport::new(390.0, 780.0)
    } else {
        Viewport::new(820.0, 420.0)
    };
    let lay = layout(&fields, &actions, &vp);

    let theme = RasterTheme {
        show_addresses: true,
        ..RasterTheme::default()
    };
    let png = render_png(&lay, &vp, &theme);
    std::fs::write(&out, &png).expect("write png");
    eprintln!(
        "wrote {out} — {}x{} ({} bytes), {} fields, {} actions, {}",
        vp.width as u32,
        vp.height.max(lay.content_height) as u32,
        png.len(),
        lay.fields.len(),
        lay.actions.len(),
        if mobile { "mobile" } else { "desktop" }
    );
}
