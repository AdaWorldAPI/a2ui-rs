//! Optional installable-app (PWA) shell for this demo — `--features pwa`.
//!
//! This is slice **P2** of `docs/MORPHEUS-PWA-APPLICATION-DESKTOP.md` (#56),
//! scoped to what this binary owns: its OWN origin. With the feature on, the
//! server additionally answers
//!
//! - `/manifest.webmanifest` — a manifest for THIS site only,
//! - `/sw.js` — a service worker that caches the shell (`/`, the manifest and
//!   the icons) so an installed app still opens offline,
//! - `/icons/192.png`, `/icons/512.png`, `/icons/icon.svg` — the icons the
//!   manifest names,
//!
//! and the index page links the manifest and registers the worker. Without the
//! feature none of these routes exist and the page is byte-for-byte the page
//! the default image serves, which is what keeps this optional and
//! non-breaking.
//!
//! What it deliberately does not do: cache `/hit`, `/hit.bin`, `/delta.bin` or
//! `/html`. Those are the live addressed surface, and a stale copy of a frame
//! is worse than a failed fetch — an offline click must fail visibly, not
//! answer with an old ordinal. The worker serves the cached shell only when the
//! network is unreachable, and only for the paths listed above.
//!
//! The worker and the registration snippet are browser-host code in the page
//! shell. They are not part of the A2UI surface, which still carries no
//! handler (charter T2).

use std::sync::OnceLock;

use axum::{
    Router,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};

/// Background and theme colour, matching the page's own `#0f1115`.
const THEME: &str = "#0f1115";

/// The shell paths the worker pre-caches and may serve offline. Nothing else.
const SHELL: [&str; 5] = [
    "/",
    "/manifest.webmanifest",
    "/icons/icon.svg",
    "/icons/192.png",
    "/icons/512.png",
];

/// The tags inserted into the index page's `<head>`: the manifest link, theme
/// colour, and the worker registration. A browser without service workers skips
/// the registration and the page works as a normal tab.
pub(crate) const HEAD: &str = concat!(
    r#"<link rel="manifest" href="/manifest.webmanifest">"#,
    r##"<meta name="theme-color" content="#0f1115">"##,
    r#"<link rel="icon" href="/icons/icon.svg" type="image/svg+xml">"#,
    r#"<script>if('serviceWorker' in navigator){navigator.serviceWorker.register('/sw.js').catch(function(){});}</script>"#,
);

/// The routes the feature adds.
pub(crate) fn routes(app: Router) -> Router {
    app.route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
        .route("/icons/icon.svg", get(icon_svg))
        .route("/icons/192.png", get(icon_192))
        .route("/icons/512.png", get(icon_512))
}

pub(crate) fn manifest_json() -> String {
    format!(
        r#"{{"id":"/","name":"a2ui paint tier","short_name":"a2ui","start_url":"/","scope":"/","display":"standalone","background_color":"{THEME}","theme_color":"{THEME}","icons":[{{"src":"/icons/192.png","sizes":"192x192","type":"image/png"}},{{"src":"/icons/512.png","sizes":"512x512","type":"image/png"}},{{"src":"/icons/icon.svg","sizes":"any","type":"image/svg+xml"}}],"launch_handler":{{"client_mode":"focus-existing"}}}}"#
    )
}

async fn manifest() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/manifest+json")],
        manifest_json(),
    )
}

/// The worker source.
///
/// Every successful online fetch of a shell path replaces that path's cached
/// copy, so the offline shell is always the last one the browser saw online.
/// The worker script itself can stay byte-identical across a deploy (nothing
/// in it changes when only the page or an icon does), so a version-only cache
/// name would never refresh; the crate version in the cache name only clears
/// caches left by older worker logic.
pub(crate) fn service_worker_js() -> String {
    let shell = SHELL
        .iter()
        .map(|p| format!("'{p}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"const CACHE='a2ui-shell-{version}';
const SHELL=[{shell}];
self.addEventListener('install',e=>{{e.waitUntil(caches.open(CACHE).then(c=>c.addAll(SHELL)));self.skipWaiting();}});
self.addEventListener('activate',e=>{{e.waitUntil(caches.keys().then(ks=>Promise.all(ks.filter(k=>k!==CACHE).map(k=>caches.delete(k)))));self.clients.claim();}});
self.addEventListener('fetch',e=>{{
  const u=new URL(e.request.url);
  if(e.request.method!=='GET'||u.origin!==self.location.origin||!SHELL.includes(u.pathname)){{return;}}
  e.respondWith(fetch(e.request).then(r=>{{
    if(r.ok){{const copy=r.clone();e.waitUntil(caches.open(CACHE).then(c=>c.put(u.pathname,copy)));}}
    return r;
  }}).catch(()=>caches.match(u.pathname)));
}});
"#,
        version = env!("CARGO_PKG_VERSION"),
    )
}

async fn service_worker() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/javascript"),
            // A worker script must be re-checked on navigation, or a fixed
            // shell can never be replaced.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        service_worker_js(),
    )
}

const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" fill="#0f1115"/><rect x="96" y="96" width="320" height="320" rx="40" fill="none" stroke="#7aa2f7" stroke-width="32"/><rect x="176" y="176" width="160" height="160" fill="#7aa2f7"/></svg>"##;

async fn icon_svg() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        ICON_SVG,
    )
}

async fn icon_192() -> impl IntoResponse {
    static PNG: OnceLock<Vec<u8>> = OnceLock::new();
    png_response(PNG.get_or_init(|| icon_png(192)))
}

async fn icon_512() -> impl IntoResponse {
    static PNG: OnceLock<Vec<u8>> = OnceLock::new();
    png_response(PNG.get_or_init(|| icon_png(512)))
}

fn png_response(bytes: &'static [u8]) -> impl IntoResponse {
    (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], bytes)
}

/// The same mark as [`ICON_SVG`], drawn at `size` pixels: a dark square, a
/// light frame, a light centre. Generated rather than committed so the icon has
/// one source and no binary sits in the repository.
pub(crate) fn icon_png(size: u32) -> Vec<u8> {
    const BG: [u8; 3] = [0x0f, 0x11, 0x15];
    const FG: [u8; 3] = [0x7a, 0xa2, 0xf7];
    // Fractions of the 512 grid in ICON_SVG.
    let at = |v: u32| v * size / 512;
    let (frame_lo, frame_hi, stroke) = (at(96), at(416), at(32).max(1));
    let (core_lo, core_hi) = (at(176), at(336));
    let mut raw = Vec::with_capacity((size as usize * 3 + 1) * size as usize);
    for y in 0..size {
        raw.push(0); // filter: none
        for x in 0..size {
            let inside = |lo: u32, hi: u32| x >= lo && x < hi && y >= lo && y < hi;
            let in_frame =
                inside(frame_lo, frame_hi) && !inside(frame_lo + stroke, frame_hi - stroke);
            let px = if in_frame || inside(core_lo, core_hi) {
                FG
            } else {
                BG
            };
            raw.extend_from_slice(&px);
        }
    }
    encode_png_rgb(size, size, &raw)
}

/// A minimal PNG encoder: 8-bit RGB, one IDAT holding a zlib stream of stored
/// (uncompressed) deflate blocks. Correct for any viewer; size is not a
/// concern for two icons generated once per process.
fn encode_png_rgb(width: u32, height: u32, filtered_rows: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // depth 8, RGB, deflate, no filter set, no interlace
    chunk(&mut out, b"IHDR", &ihdr);

    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = filtered_rows.chunks(0xFFFF).collect();
    for (i, block) in blocks.iter().enumerate() {
        let last = u8::from(i + 1 == blocks.len());
        let len = block.len() as u16;
        zlib.push(last);
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(filtered_rows).to_be_bytes());
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known answers, so a broken checksum cannot pass by agreeing with itself.
    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn icons_are_png_of_the_declared_size() {
        for size in [192u32, 512] {
            let png = icon_png(size);
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(&png[12..16], b"IHDR");
            assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), size);
            assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), size);
            assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]), "IEND crc");
        }
    }

    /// The icon is not one flat colour: both colours of the mark are present.
    #[test]
    fn the_icon_draws_the_mark() {
        let size = 192u32;
        let png = icon_png(size);
        // Raw pixel bytes follow the 2-byte zlib header and a 5-byte stored
        // block header; the first row starts with its filter byte.
        let idat = png.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
        let first_block = &png[idat + 2 + 5..];
        let row = (size * 3 + 1) as usize;
        let mid = &first_block[row * (size as usize / 2)..row * (size as usize / 2 + 1)];
        assert!(mid[1..].chunks(3).any(|p| p == [0x0f, 0x11, 0x15]));
        assert!(mid[1..].chunks(3).any(|p| p == [0x7a, 0xa2, 0xf7]));
    }

    #[test]
    fn the_manifest_is_for_this_origin_only() {
        let m = manifest_json();
        assert!(m.contains(r#""scope":"/""#));
        assert!(m.contains(r#""display":"standalone""#));
        assert!(m.contains("/icons/192.png") && m.contains("/icons/512.png"));
        assert!(!m.contains("http"), "no foreign origin in the manifest");
    }

    /// The worker caches the shell and nothing on the live surface.
    #[test]
    fn the_worker_never_caches_the_live_surface() {
        let js = service_worker_js();
        for path in SHELL {
            assert!(js.contains(&format!("'{path}'")), "{path} pre-cached");
        }
        for live in ["/hit", "/delta.bin", "/html", "/health"] {
            assert!(
                !js.contains(&format!("'{live}'")),
                "{live} must not be cached"
            );
        }
        assert!(
            js.contains("!SHELL.includes(u.pathname)"),
            "non-shell paths pass through"
        );
        // An online fetch refreshes the cached copy, so a changed shell reaches
        // the offline cache without a worker or version change.
        assert!(
            js.contains("c.put(u.pathname,copy)"),
            "online fetch refreshes the shell cache"
        );
    }
}
