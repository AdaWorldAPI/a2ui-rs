//! Headless CPU rasterizer (feature `raster`) — the **third renderer** of the
//! one resolved surface: `PaintLayout` → an RGBA canvas (rect fills + borders +
//! real glyphs) → PNG.
//!
//! askama gives HTML, [`gpu`](crate::gpu) gives a GPU texture (needs an adapter),
//! and this gives a **PNG with no display, no GPU, deterministically** — the
//! path for *checking what a screen looks like* in CI / a headless box. It is a
//! renderer of the SAME `PaintLayout` the other two consume (charter T1: a
//! render style, never a new vocabulary); it reads the layout, writes pixels,
//! serialises nothing on the hot path (T3 — PNG is an inspection artifact, not a
//! wire format).
//!
//! Text is real: an embedded DejaVu Sans (`assets/DejaVuSans.ttf`) is rasterised
//! per glyph via `fontdue` and alpha-blended. No `unsafe` here (the crate keeps
//! `#![forbid(unsafe_code)]`); the font/PNG deps carry their own.

use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

use crate::{PaintLayout, Rect, Viewport};

/// The embedded UI font (DejaVu Sans — Bitstream-Vera-derived, redistributable).
const FONT_TTF: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// An RGBA colour, 0–255 per channel.
pub type Rgba = [u8; 4];

/// The paint theme — colours + text sizes for the CPU raster. Deliberately a
/// plain light "desktop form" look; a real consumer swaps the colours.
#[derive(Debug, Clone, Copy)]
pub struct RasterTheme {
    /// Page background.
    pub background: Rgba,
    /// Value-cell fill.
    pub value_fill: Rgba,
    /// Value-cell + field border.
    pub border: Rgba,
    /// Field label text.
    pub label: Rgba,
    /// Field value text.
    pub value: Rgba,
    /// Action button fill.
    pub action_fill: Rgba,
    /// Action button caption.
    pub action_text: Rgba,
    /// Field text size (px).
    pub field_px: f32,
    /// Action caption size (px).
    pub action_px: f32,
    /// Draw the field `position` / action `ordinal` as a faint address tag —
    /// useful for *verifying* a screen is addressed where expected.
    pub show_addresses: bool,
}

impl Default for RasterTheme {
    fn default() -> Self {
        Self {
            background: [0xFA, 0xFA, 0xFB, 0xFF],
            value_fill: [0xFF, 0xFF, 0xFF, 0xFF],
            border: [0xDD, 0xDD, 0xE2, 0xFF],
            label: [0x44, 0x4A, 0x54, 0xFF],
            value: [0x11, 0x14, 0x18, 0xFF],
            action_fill: [0x2E, 0x6F, 0xE0, 0xFF],
            action_text: [0xFF, 0xFF, 0xFF, 0xFF],
            field_px: 15.0,
            action_px: 14.0,
            show_addresses: false,
        }
    }
}

/// A simple RGBA canvas with the pixel ops the layout needs.
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>, // w*h*4, opaque
}

impl Canvas {
    fn new(w: u32, h: u32, bg: Rgba) -> Self {
        let mut px = vec![0u8; (w as usize) * (h as usize) * 4];
        for chunk in px.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg);
        }
        Self { w, h, px }
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, c: Rgba, cov: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || cov == 0 {
            return;
        }
        let i = ((y as usize) * (self.w as usize) + (x as usize)) * 4;
        let a = (cov as u32) * (c[3] as u32) / 255; // src alpha 0..255
        for (k, &src) in c.iter().take(3).enumerate() {
            let dst = self.px[i + k] as u32;
            self.px[i + k] = ((src as u32 * a + dst * (255 - a)) / 255) as u8;
        }
        // canvas stays opaque
        self.px[i + 3] = 0xFF;
    }

    fn fill(&mut self, r: Rect, c: Rgba) {
        let x0 = r.x.max(0.0) as i32;
        let y0 = r.y.max(0.0) as i32;
        let x1 = ((r.x + r.w).min(self.w as f32)) as i32;
        let y1 = ((r.y + r.h).min(self.h as f32)) as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, c, 255);
            }
        }
    }

    fn stroke(&mut self, r: Rect, c: Rgba) {
        let x0 = r.x as i32;
        let y0 = r.y as i32;
        let x1 = (r.x + r.w) as i32;
        let y1 = (r.y + r.h) as i32;
        for x in x0..x1 {
            self.blend(x, y0, c, 255);
            self.blend(x, y1 - 1, c, 255);
        }
        for y in y0..y1 {
            self.blend(x0, y, c, 255);
            self.blend(x1 - 1, y, c, 255);
        }
    }
}

/// A glyph cache keyed by `(char, px_bits)` — one rasterisation per glyph/size.
struct TextRenderer<'a> {
    font: &'a Font,
    cache: HashMap<(char, u32), (Metrics, Vec<u8>)>,
}

impl<'a> TextRenderer<'a> {
    fn new(font: &'a Font) -> Self {
        Self {
            font,
            cache: HashMap::new(),
        }
    }

    /// Draw `text` left-aligned, vertically centred in `rect`, clipped to the
    /// rect's right edge. Returns nothing — best-effort screen text.
    fn draw(&mut self, canvas: &mut Canvas, rect: Rect, text: &str, px: f32, color: Rgba) {
        let lm = match self.font.horizontal_line_metrics(px) {
            Some(m) => m,
            None => return,
        };
        // Vertically centre the (ascent..descent) box inside the rect.
        let baseline = rect.y + (rect.h - (lm.ascent - lm.descent)) / 2.0 + lm.ascent;
        let mut pen = rect.x + 3.0; // small left pad
        let right = rect.x + rect.w - 2.0;
        let key_px = px.to_bits();
        for ch in text.chars() {
            let (m, bmp) = self
                .cache
                .entry((ch, key_px))
                .or_insert_with(|| self.font.rasterize(ch, px));
            if pen + m.advance_width > right && ch != ' ' {
                break; // clip overflow (no ellipsis — inspection artifact)
            }
            let gx0 = (pen + m.xmin as f32) as i32;
            // screen y-down: glyph top = baseline - ymin - height
            let gy0 = (baseline - m.ymin as f32 - m.height as f32) as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let cov = bmp[row * m.width + col];
                    canvas.blend(gx0 + col as i32, gy0 + row as i32, color, cov);
                }
            }
            pen += m.advance_width;
        }
    }
}

/// Render a laid-out surface to an RGBA8 buffer. Canvas is `viewport.width` wide
/// and tall enough to hold the whole layout (`max(height, content_height)`), so
/// nothing is clipped vertically. Returns `(width, height, rgba)`.
#[must_use]
pub fn render_rgba(
    layout: &PaintLayout,
    viewport: &Viewport,
    theme: &RasterTheme,
) -> (u32, u32, Vec<u8>) {
    let w = viewport.width.max(1.0).ceil() as u32;
    let h = viewport.height.max(layout.content_height).max(1.0).ceil() as u32;
    let mut canvas = Canvas::new(w, h, theme.background);
    let font = Font::from_bytes(FONT_TTF, FontSettings::default())
        .expect("embedded DejaVuSans.ttf parses");
    let mut text = TextRenderer::new(&font);

    for f in &layout.fields {
        // value cell: filled + bordered so the addressed grid is visible
        canvas.fill(f.value_rect, theme.value_fill);
        canvas.stroke(f.value_rect, theme.border);
        text.draw(
            &mut canvas,
            f.label_rect,
            &f.label,
            theme.field_px,
            theme.label,
        );
        text.draw(
            &mut canvas,
            f.value_rect,
            &f.value,
            theme.field_px,
            theme.value,
        );
        if theme.show_addresses {
            let tag = Rect {
                x: f.label_rect.x,
                y: f.label_rect.y - 1.0,
                w: 22.0,
                h: 12.0,
            };
            text.draw(&mut canvas, tag, &f.position.to_string(), 9.0, theme.border);
        }
    }

    for a in &layout.actions {
        canvas.fill(a.rect, theme.action_fill);
        text.draw(
            &mut canvas,
            a.rect,
            &a.label,
            theme.action_px,
            theme.action_text,
        );
        if theme.show_addresses {
            let tag = Rect {
                x: a.rect.x + 2.0,
                y: a.rect.y - 1.0,
                w: 16.0,
                h: 11.0,
            };
            text.draw(
                &mut canvas,
                tag,
                &a.ordinal.to_string(),
                9.0,
                theme.action_fill,
            );
        }
    }

    (w, h, canvas.px)
}

/// Render a laid-out surface to PNG bytes (RGBA8). This is the "check a screen"
/// artifact — write it to a file and look at it.
#[must_use]
pub fn render_png(layout: &PaintLayout, viewport: &Viewport, theme: &RasterTheme) -> Vec<u8> {
    let (w, h, rgba) = render_rgba(layout, viewport, theme);
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&rgba).expect("png data");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Viewport, layout};
    use ogar_render_askama::{ActionRef, FieldView};

    fn fv(position: u8, label: &str, value: &str) -> FieldView {
        FieldView {
            position,
            label: label.to_string(),
            predicate: format!("iri:{label}"),
            value: value.to_string(),
        }
    }

    #[test]
    fn renders_a_screen_to_a_nonempty_png_with_pixels() {
        let fields = [fv(0, "Total", "1240.50"), fv(2, "Partner", "Acme GmbH")];
        let actions = [ActionRef {
            ordinal: 1,
            label: "Post".into(),
        }];
        let vp = Viewport::new(700.0, 300.0);
        let lay = layout(&fields, &actions, &vp);

        let (w, h, rgba) = render_rgba(&lay, &vp, &RasterTheme::default());
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
        // Some pixels differ from the background → text/rects were drawn.
        let bg = RasterTheme::default().background;
        let non_bg = rgba.chunks_exact(4).filter(|p| *p != bg).count();
        assert!(
            non_bg > 500,
            "expected drawn content, got {non_bg} non-bg px"
        );

        let png = render_png(&lay, &vp, &RasterTheme::default());
        assert_eq!(
            &png[..8],
            &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']
        );
    }
}
