//! Serve ONE MGRA v3 stream and the client that draws it.
//!
//! The field renderer (`a2ui-graph`, wgpu → WebGPU with a WebGL2 fallback) has
//! existed for a while with nothing in this repo actually serving it a graph.
//! `medcare-rs` does — from its own substrate wave — but that is a consumer's
//! endpoint inside a private clinical app, so any other producer of the wire
//! had no way to see its own bytes drawn. This binary is that way.
//!
//! # The stream comes from a PATH, never from this repo
//!
//! `a2ui-rs` is public, and the streams people want to look at are frequently
//! derived from material that must not be redistributed — the 6502 graph this
//! was built for is lifted from a commercial game image, and its ore already
//! had to be moved out of a public repo once for exactly that reason. So there
//! are no graph bytes here and none are ever committed: `FIELD_ABI` names a
//! file, the operator points it wherever their own stream lives.
//!
//! A consequence worth stating: with `FIELD_ABI` unset this server has nothing
//! to draw, and it says so in as many words. It does not ship a synthetic
//! stand-in that would render a convincing field of nothing — "it drew
//! something" is precisely the wrong signal when the question is whether YOUR
//! bytes are right.
//!
//! # Validated once, at startup
//!
//! The stream is parsed with the consumer's own reader before it is served,
//! and the page reports the node and edge counts it found. A stream that is
//! not v3, or truncated, is refused with the reader's own reason rather than
//! reaching the browser to fail there — where, an ES module having no global
//! catch, the failure is a canvas that stays empty and a status line that
//! never changes. (That exact silence cost a debugging session on the MedCare
//! field; it is not a hypothetical.)
//!
//! ```sh
//! # 1. build the client (once; needs the wasm32 target + wasm-bindgen CLI)
//! ./scripts/build-graph-wasm.sh
//!
//! # 2. point it at a stream and run
//! FIELD_ABI=/path/to/locode.abi cargo run -p a2ui-field-web
//! ```
//!
//! Port: `$PORT` from the environment (Railway injects it); 8080 only as a
//! local fallback.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::get,
};

use a2ui_graph::GraphAbi;

/// Where the stream came from, or why there isn't one.
///
/// Every non-ready variant carries enough to act on: the path that was tried,
/// and the reason it did not work. A variant that only said "no stream" would
/// leave the operator guessing between a typo, a permission, and a stream the
/// reader rejected — three different fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stream {
    /// Parsed, counted, and held in memory for the `.abi` route.
    Ready {
        bytes: Vec<u8>,
        nodes: usize,
        edges: usize,
        path: String,
    },
    /// `FIELD_ABI` is not set.
    Unset,
    /// The path is set but could not be read.
    Unreadable { path: String, why: String },
    /// The bytes were read but the consumer's own reader refused them.
    Refused { path: String, why: String },
}

/// Read and validate the configured stream.
///
/// Separate from `main` because this is the whole decision, and a decision
/// made inside an async entry point is a decision no test can reach.
fn load_stream(path: Option<&str>) -> Stream {
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        return Stream::Unset;
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return Stream::Unreadable {
                path: path.to_string(),
                why: e.to_string(),
            };
        }
    };
    // The consumer's reader, not a second one written here: if it will not
    // parse in the browser it must not be served from here either.
    match GraphAbi::parse(&bytes) {
        Ok(abi) => Stream::Ready {
            nodes: abi.node_count(),
            edges: abi.edge_count(),
            bytes,
            path: path.to_string(),
        },
        Err(e) => Stream::Refused {
            path: path.to_string(),
            why: e.to_string(),
        },
    }
}

/// The only three files the client needs, and the only three this serves.
///
/// A WHITELIST, not a sanitiser. `pkg` is an operator-supplied directory and
/// the request names a file inside it; joining a request path onto a base
/// directory is the classic traversal, and every "strip the `..`" scheme is a
/// guess about encodings. Three known names cannot traverse anywhere.
fn pkg_file(dir: &Path, name: &str) -> Option<(PathBuf, &'static str)> {
    let mime = match name {
        "a2ui_graph.js" => "text/javascript; charset=utf-8",
        "a2ui_graph_bg.wasm" => "application/wasm",
        "a2ui_graph.d.ts" => "text/plain; charset=utf-8",
        _ => return None,
    };
    Some((dir.join(name), mime))
}

fn pkg_dir() -> PathBuf {
    std::env::var("FIELD_PKG").map_or_else(
        |_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../a2ui-graph/pkg")),
        PathBuf::from,
    )
}

/// The status line the page shows, and the reason it is prose rather than a
/// code: whoever reads it is looking at an empty canvas and needs the next
/// action, not a taxonomy.
fn status_line(s: &Stream) -> String {
    match s {
        Stream::Ready {
            nodes, edges, path, ..
        } => format!("{nodes} nodes · {edges} edges · {path}"),
        Stream::Unset => {
            "FIELD_ABI is not set — nothing to draw. Point it at an MGRA v3 stream, e.g. \
             FIELD_ABI=/path/to/locode.abi. This server ships no graph bytes of its own."
                .to_string()
        }
        Stream::Unreadable { path, why } => format!("cannot read {path}: {why}"),
        Stream::Refused { path, why } => {
            format!("{path} is not a stream this client can draw: {why}")
        }
    }
}

fn index(stream: &Stream) -> Html<String> {
    let status = status_line(stream);
    let ready = matches!(stream, Stream::Ready { .. });
    // Only mount when there is something to mount. A client told to draw a
    // missing stream fails inside the wasm, where the message does not reach
    // the reader.
    let boot = if ready {
        r#"
    import init, { FieldHandle } from './pkg/a2ui_graph.js';
    const canvas = document.getElementById('stage');
    const stat = document.getElementById('stat');
    const pick = document.getElementById('pick');
    function fit() {
      const r = Math.min(window.devicePixelRatio || 1, 2);
      const b = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(b.width * r));
      canvas.height = Math.max(1, Math.round(b.height * r));
      return r;
    }
    try {
      await init();
      const dpr = fit();
      const bytes = new Uint8Array(await (await fetch('/field.abi')).arrayBuffer());
      const field = await FieldHandle.mount(canvas, bytes);
      stat.textContent = stat.textContent + ' · ' + field.backend;
      const at = e => [e.offsetX * dpr, e.offsetY * dpr];
      canvas.addEventListener('pointerdown', e => {
        const hit = field.pointerDown(...at(e));
        if (hit) {
          // classid/identity are the point: they read 0x00000000 for every
          // node until a2ui-rs#42, so a non-zero pair here is the fix visible.
          pick.textContent =
            'ordinal ' + hit.ordinal +
            ' · classid 0x' + hit.classid.toString(16).padStart(8, '0') +
            ' · identity ' + hit.identity;
        }
      });
      canvas.addEventListener('pointermove', e => field.pointerMove(...at(e)));
      canvas.addEventListener('pointerup', () => field.pointerUp());
      canvas.addEventListener('wheel', e => {
        e.preventDefault();
        field.zoom(e.offsetX * dpr, e.offsetY * dpr, e.deltaY < 0 ? 1.1 : 0.9);
      }, { passive: false });
      window.addEventListener('resize', () => {
        const r = fit();
        field.resize(canvas.width, canvas.height);
        void r;
      });
      const tick = () => { field.frame(); requestAnimationFrame(tick); };
      requestAnimationFrame(tick);
    } catch (e) {
      // An ES module has no global catch. Without this the page would sit on
      // its placeholder forever and show the reader nothing at all.
      stat.textContent = 'client failed: ' + (e && e.message ? e.message : e);
    }
"#
    } else {
        ""
    };

    Html(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>a2ui field</title>
<style>
  :root {{ color-scheme: dark; }}
  body {{ margin: 0; background: #0b0f14; color: #c8d3e0;
         font: 14px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; }}
  header {{ padding: 12px 16px; border-bottom: 1px solid #1b2532; }}
  h1 {{ margin: 0 0 4px; font-size: 15px; font-weight: 600; color: #e6edf5; }}
  #stat, #pick {{ color: #7f8fa4; word-break: break-word; }}
  #pick {{ margin-top: 4px; min-height: 1.5em; color: #9ecbff; }}
  #stage {{ display: block; width: 100vw; height: calc(100vh - 92px); }}
</style></head>
<body>
  <header>
    <h1>a2ui field</h1>
    <div id="stat">{status}</div>
    <div id="pick"></div>
  </header>
  <canvas id="stage"></canvas>
  <script type="module">{boot}</script>
</body></html>
"#
    ))
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let stream = load_stream(std::env::var("FIELD_ABI").ok().as_deref());
    let dir = pkg_dir();

    let page = index(&stream);
    // Cloned once into each handler: the stream never changes at runtime, so
    // rebuilding it per request would be work with no possible new answer.
    let bytes = match &stream {
        Stream::Ready { bytes, .. } => bytes.clone(),
        _ => Vec::new(),
    };
    let missing = status_line(&stream);
    let ready = matches!(stream, Stream::Ready { .. });

    let app = Router::new()
        .route(
            "/",
            get(move || {
                let page = page.clone();
                async move { page }
            }),
        )
        .route(
            "/field.abi",
            get(move || {
                let (bytes, missing, ready) = (bytes.clone(), missing.clone(), ready);
                async move {
                    if ready {
                        (
                            [
                                (header::CONTENT_TYPE, "application/octet-stream"),
                                (header::CACHE_CONTROL, "no-store"),
                            ],
                            bytes,
                        )
                            .into_response()
                    } else {
                        (StatusCode::NOT_FOUND, missing).into_response()
                    }
                }
            }),
        )
        .route(
            "/pkg/{file}",
            get(
                move |axum::extract::Path(file): axum::extract::Path<String>| {
                    let dir = dir.clone();
                    async move {
                        let Some((path, mime)) = pkg_file(&dir, &file) else {
                            return (StatusCode::NOT_FOUND, "no such client file").into_response();
                        };
                        match tokio::fs::read(&path).await {
                            Ok(b) => {
                                ([(header::CONTENT_TYPE, mime)], Body::from(b)).into_response()
                            }
                            Err(e) => (
                                StatusCode::NOT_FOUND,
                                format!(
                                    "{} is missing: {e} — run ./scripts/build-graph-wasm.sh",
                                    path.display()
                                ),
                            )
                                .into_response(),
                        }
                    }
                },
            ),
        )
        .route("/health", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr} failed: {e}"));

    eprintln!(
        "a2ui-field-web listening on {addr} (PORT={port_env}) — {status}",
        port_env = std::env::var("PORT").unwrap_or_else(|_| "unset→8080".into()),
        status = status_line(&stream),
    );

    axum::serve(listener, app).await.expect("server");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal, hand-built v3 stream: two nodes with DISTINCT addresses, one
    /// edge, one label lane. Built here rather than read from a file so the
    /// tests carry no corpus and cannot skip when one is absent.
    fn stream_bytes(version: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"MGRA");
        b.extend_from_slice(&version.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // flags: labels only
        b.extend_from_slice(&2u32.to_le_bytes()); // nodes
        b.extend_from_slice(&1u32.to_le_bytes()); // edges
        for (classid, identity) in [(0x0901u32, 10u32), (0x0903, 12)] {
            b.extend_from_slice(&classid.to_le_bytes());
            b.extend_from_slice(&identity.to_le_bytes());
            b.extend_from_slice(&[0, 0, 0, 0]); // vocab, role, flags, reserved
            b.extend_from_slice(&[1, 0]); // domain, evidence
            b.extend_from_slice(&0u16.to_le_bytes()); // reserved
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // edge from
        b.extend_from_slice(&1u32.to_le_bytes()); // edge to
        b.extend_from_slice(&[0, 0, 0, 0]); // kind, role, flags, predicate
        b.extend_from_slice(b"MGL1");
        for n in ["a", "b"] {
            b.extend_from_slice(&(n.len() as u16).to_le_bytes());
            b.extend_from_slice(n.as_bytes());
        }
        b
    }

    fn tmp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("a2ui-field-{}-{name}", std::process::id()));
        std::fs::write(&p, bytes).expect("write fixture");
        p
    }

    #[test]
    fn a_valid_stream_is_counted_before_it_is_served() {
        let p = tmp("ok.abi", &stream_bytes(3));
        match load_stream(p.to_str()) {
            Stream::Ready { nodes, edges, .. } => {
                assert_eq!((nodes, edges), (2, 1));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
        std::fs::remove_file(&p).ok();
    }

    /// **A stream the client cannot draw is refused HERE, with its reason.**
    ///
    /// The alternative is serving it and letting the browser fail: an ES module
    /// has no global catch, so the page would keep its placeholder and show the
    /// reader nothing. This is the same silence that hid a `SyntaxError` on the
    /// MedCare field for eleven days.
    ///
    /// CAN FIRE: drop the `GraphAbi::parse` call from `load_stream` and this
    /// v2 stream sails through to `Ready`.
    #[test]
    fn a_stream_the_client_would_reject_is_refused_with_its_reason() {
        let good = tmp("v3.abi", &stream_bytes(3));
        assert!(
            matches!(load_stream(good.to_str()), Stream::Ready { .. }),
            "the fixture is a real stream apart from its version byte"
        );

        let bad = tmp("v2.abi", &stream_bytes(2));
        match load_stream(bad.to_str()) {
            Stream::Refused { why, .. } => assert!(
                why.contains('2'),
                "the refusal names the version it actually saw: {why}"
            ),
            other => panic!("a v2 stream must not be served: {other:?}"),
        }
        std::fs::remove_file(&good).ok();
        std::fs::remove_file(&bad).ok();
    }

    #[test]
    fn an_unset_or_unreadable_path_is_distinguished_not_merged() {
        assert_eq!(load_stream(None), Stream::Unset);
        assert_eq!(load_stream(Some("")), Stream::Unset);
        // A typo and an absent setting need different fixes, so they are
        // different variants and different messages.
        match load_stream(Some("/nonexistent/nope.abi")) {
            Stream::Unreadable { path, .. } => assert_eq!(path, "/nonexistent/nope.abi"),
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    /// **The client-file route cannot be walked out of its directory.**
    ///
    /// `FIELD_PKG` is an operator directory and the request names a file in it,
    /// which is the shape every traversal exploits. The guard is a whitelist of
    /// three names; nothing is stripped, decoded, or canonicalised, because
    /// each of those is a guess about encodings.
    ///
    /// CAN FIRE: replace `pkg_file`'s match with `dir.join(name)` and the
    /// first assertion returns a path outside the directory.
    #[test]
    fn the_client_route_serves_three_names_and_refuses_everything_else() {
        let dir = Path::new("/srv/pkg");
        for escape in [
            "../../../../etc/passwd",
            "..%2f..%2fetc%2fpasswd",
            "a2ui_graph.js/../../secret",
            "",
            ".",
            "..",
        ] {
            assert!(
                pkg_file(dir, escape).is_none(),
                "must refuse {escape:?} rather than join it onto the directory"
            );
        }
        // The silence half: the three real names still resolve, inside the
        // directory, with their own content types.
        for (name, mime) in [
            ("a2ui_graph.js", "text/javascript; charset=utf-8"),
            ("a2ui_graph_bg.wasm", "application/wasm"),
        ] {
            let (path, got) = pkg_file(dir, name).expect("a real client file resolves");
            assert_eq!(got, mime);
            assert_eq!(path, dir.join(name));
            assert!(path.starts_with(dir));
        }
    }

    /// **A page with no stream explains itself instead of mounting nothing.**
    ///
    /// Mounting a client against a missing stream fails inside the wasm, where
    /// the message does not reach the reader — a blank canvas and no error.
    #[test]
    fn a_page_without_a_stream_carries_no_client_and_says_why() {
        let html = index(&Stream::Unset).0;
        assert!(
            !html.contains("FieldHandle.mount"),
            "no client is mounted when there is nothing for it to draw"
        );
        assert!(html.contains("FIELD_ABI is not set"));

        // And the ready page DOES mount, or the assertion above would pass for
        // a page that never mounts anything at all.
        let ready = index(&Stream::Ready {
            bytes: vec![],
            nodes: 2,
            edges: 1,
            path: "/tmp/x.abi".into(),
        })
        .0;
        assert!(ready.contains("FieldHandle.mount"));
        assert!(ready.contains("2 nodes · 1 edges"));
    }
}
