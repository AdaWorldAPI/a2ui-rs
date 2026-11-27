//! One addressed content surface in three containers, under the a2ui container contract
//! (`a2ui_layout::container`). Built only with `--features pwa`.
//!
//! - `/surface` is the content's canonical URL. Without the `A2-Container` header it is a
//!   page; with `A2-Container: pane` or `window` it is the content alone, for a parent
//!   that already has its shell open.
//! - `/workspace` is a two-pane page. Its second plane shows the same content, either
//!   composed by the server (`/workspace?open=surface`, no script needed) or loaded into
//!   the pane by the shared container script.
//! - The desktop opens `/surface` natively in a window (see `desktop.rs`).
//!
//! The content is built by one function, [`content`], for all three. Nothing about the
//! container reaches what the content holds: the same resolved fields and the same
//! ordinal actions in every case.

use a2ui_layout::container::{CONTAINER_HEADER, Container, check_fragment};
use axum::{
    Router,
    extract::RawQuery,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};

/// The title of the demo surface, in every container.
const TITLE: &str = "Adressierte Fläche";

/// The content: the root element of the contract around the resolved surface. The askama
/// fieldview is the one resolution the demo made at start-up; nothing is re-derived.
pub(crate) fn content() -> String {
    format!(
        r#"<section class="a2-surface" data-a2-content data-a2-title="{TITLE}"><h2>{TITLE}</h2><p class="lead">Ein Datensatz, adressiert: dieselben Felder und dieselben Aktionen als Seite, im zweiten Bereich oder als Fenster.</p>{}</section>"#,
        crate::demo().html
    )
}

/// The shared head of this server's pages that open containers.
const HEAD: &str = r#"<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="stylesheet" href="/static/a2ui/containers/containers.css"><script src="/static/a2ui/vendor/htmx-2.0.4.min.js" defer></script><script src="/static/a2ui/containers/containers.js" defer></script>
<style>*{box-sizing:border-box}body{margin:0;font:14px/1.45 ui-sans-serif,system-ui,"Segoe UI",sans-serif;color:#1b2430;background:#eef1f5}header{padding:10px 18px;background:#1f2a3a;color:#e9eef6}header a{color:inherit}main{padding:18px}.a2-surface{padding:4px 2px}.a2-surface h2{margin:0 0 6px;font-size:17px}.lead{color:#4c5869;margin:0 0 12px}.a2-duo{gap:16px}.a2-duo>section{background:#fff;border:1px solid #d6dce5;border-radius:10px;padding:14px;min-height:240px}ul{padding-left:18px}</style>"#;

/// The content as a page: the application shell around it.
fn page(body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="de"><head>{HEAD}<title>{TITLE}</title></head><body><header><a href="/desktop">Morpheus Desktop</a> · <a href="/workspace">Arbeitsbereich</a></header><main class="region-center">{body}</main></body></html>"#
    )
}

/// The negotiated answer: the headers of the contract on every answer, a page or the
/// content alone.
fn negotiated(container: Container, body: String) -> Response {
    let mut r = (StatusCode::OK, body).into_response();
    for (k, v) in container.response_headers() {
        let name = HeaderName::from_bytes(k.as_bytes()).expect("a valid header name");
        r.headers_mut().insert(name, HeaderValue::from_static(v));
    }
    r
}

/// `GET /surface`.
async fn surface(headers: HeaderMap) -> Response {
    let container =
        Container::from_header(headers.get(CONTAINER_HEADER).and_then(|v| v.to_str().ok()));
    let c = content();
    debug_assert_eq!(check_fragment(&c), Ok(()));
    if container.is_fragment() {
        negotiated(container, c)
    } else {
        negotiated(container, page(&c))
    }
}

/// `GET /workspace`: a list beside a pane. `?open=surface` composes the pane on the
/// server, from the same [`content`] the fragment carries.
async fn workspace(RawQuery(raw): RawQuery) -> Response {
    let open = raw
        .as_deref()
        .unwrap_or("")
        .split('&')
        .any(|kv| kv == "open=surface");
    let pane = if open {
        content()
    } else {
        String::from(r#"<p class="lead">Links einen Eintrag wählen.</p>"#)
    };
    let body = format!(
        r#"<div class="a2-duo"><section><h2>Einträge</h2><ul><li><a href="/workspace?open=surface" data-a2-open="pane" data-a2-pane="side" data-a2-src="/surface">{TITLE}</a></li><li><a href="/surface">{TITLE} als Seite</a></li></ul></section><section data-a2-pane="side">{pane}</section></div>"#
    );
    negotiated(Container::Page, page(&body))
}

/// The shared container assets: the script, its stylesheet and the vendored htmx. Exact
/// matches against the theme crate's asset table, under these two prefixes only.
async fn a2ui_asset(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    let allowed = path.starts_with("containers/") || path == "vendor/htmx-2.0.4.min.js";
    match allowed.then(|| a2ui_theme::asset(&path)).flatten() {
        Some(a) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, a.media_type)],
            a.bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The routes of the three containers.
pub(crate) fn routes(app: Router) -> Router {
    app.route("/surface", get(surface))
        .route("/workspace", get(workspace))
        .route("/static/a2ui/{*path}", get(a2ui_asset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn get_surface(header: Option<&'static str>) -> (Response, String) {
        let mut h = HeaderMap::new();
        if let Some(v) = header {
            h.insert(CONTAINER_HEADER, HeaderValue::from_static(v));
        }
        let r = surface(h).await;
        let (parts, body) = r.into_parts();
        let text = String::from_utf8(to_bytes(body, usize::MAX).await.unwrap().to_vec()).unwrap();
        (Response::from_parts(parts, axum::body::Body::empty()), text)
    }

    /// The same content in all three containers: the window and the pane carry exactly
    /// the content, and the page carries it inside its shell. CAN FIRE: a container that
    /// rendered anything of its own (another heading, fewer actions) breaks equality.
    #[tokio::test]
    async fn one_content_three_containers() {
        let c = content();
        let (_, window) = get_surface(Some("window")).await;
        let (_, pane) = get_surface(Some("pane")).await;
        let (_, page) = get_surface(None).await;
        assert_eq!(window, c);
        assert_eq!(pane, c);
        assert!(page.contains(&c));
        assert!(page.starts_with("<!doctype html>"));
        // The resolved surface itself, with its ordinal actions, is in the content.
        assert!(c.contains(&crate::demo().html));
        assert!(c.contains("data-action-ordinal=\"0\""));
    }

    /// A fragment is content and nothing else; the page is not a fragment. Two-sided, so
    /// the check cannot pass by accepting everything.
    #[tokio::test]
    async fn fragments_pass_the_contract_and_pages_do_not() {
        for v in ["window", "pane"] {
            let (_, body) = get_surface(Some(v)).await;
            assert_eq!(check_fragment(&body), Ok(()), "{v}");
        }
        let (_, page) = get_surface(None).await;
        assert!(check_fragment(&page).is_err());
    }

    /// Every answer varies on the header and echoes what it served. An unknown request
    /// gets the page, echoed as `page`.
    #[tokio::test]
    async fn every_answer_varies_and_echoes() {
        for (asked, served) in [
            (Some("window"), "window"),
            (Some("pane"), "pane"),
            (None, "page"),
            (Some("dialog"), "page"),
        ] {
            let (r, _) = get_surface(asked).await;
            assert_eq!(r.headers()["vary"], "A2-Container", "{asked:?}");
            assert_eq!(r.headers()[CONTAINER_HEADER], served, "{asked:?}");
            assert_eq!(r.headers()["content-type"], "text/html; charset=utf-8");
        }
    }

    /// The server-composed pane holds exactly the content the pane fragment carries.
    /// Silence: without `?open=surface` the pane holds no content.
    #[tokio::test]
    async fn the_server_composed_pane_is_the_same_content() {
        let body = |r: Response| async {
            String::from_utf8(to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
        };
        let open = body(workspace(RawQuery(Some("open=surface".into()))).await).await;
        assert!(open.contains(&format!(
            r#"<section data-a2-pane="side">{}</section>"#,
            content()
        )));
        let closed = body(workspace(RawQuery(None)).await).await;
        assert!(!closed.contains("data-a2-content"));
        assert!(
            closed.contains(r#"data-a2-open="pane" data-a2-pane="side" data-a2-src="/surface""#)
        );
    }

    /// Only the container assets are served from the theme crate here.
    #[tokio::test]
    async fn only_the_container_assets_are_served() {
        for ok in [
            "containers/containers.js",
            "containers/containers.css",
            "vendor/htmx-2.0.4.min.js",
        ] {
            let r = a2ui_asset(axum::extract::Path(ok.to_string())).await;
            assert_eq!(r.status(), StatusCode::OK, "{ok}");
        }
        for bad in [
            "themes.css",
            "vendor/htmx-LICENSE.txt",
            "../containers.js",
            "containers/nope.js",
        ] {
            let r = a2ui_asset(axum::extract::Path(bad.to_string())).await;
            assert_eq!(r.status(), StatusCode::NOT_FOUND, "{bad}");
        }
    }
}
