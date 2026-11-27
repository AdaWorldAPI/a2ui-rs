//! **Containers**: one addressed content surface, shown as a page, in a pane or in a
//! window.
//!
//! A route renders its content once. The request says which container it will be shown
//! in. The server either wraps the content in its application shell (a page) or returns
//! the content alone (a fragment) for a parent that already has the shell open.
//!
//! # Presentation only
//!
//! The container is a presentation choice, like [`crate::LayoutMode`]. It is **never**
//! an input to authorization, customer scope, object identity or the set of actions. A
//! route reads the same object, checks the same rights and offers the same actions for
//! every container; only the wrapping differs. A content surface that answers
//! differently in a window than on its page is a bug, not a variant.
//!
//! # The negotiation
//!
//! - **Request:** the header [`CONTAINER_HEADER`] (`A2-Container: pane|window`). No header
//!   or any other value means [`Container::Page`]: a full page is always a valid answer.
//! - **Response:** the same canonical URL, `text/html`, with `Vary: A2-Container` on
//!   every answer (page and fragment alike, so no cache hands a fragment to a page
//!   request) and the served container echoed in [`CONTAINER_HEADER`]
//!   ([`Container::response_headers`]).
//! - **Fragment:** one root element carrying [`CONTENT_ATTR`] (and [`TITLE_ATTR`] for the
//!   container's title), no document or shell elements and no script
//!   ([`check_fragment`]). Behaviour stays with the parent; the content carries its
//!   addressed information and its authorized actions as links and forms.
//! - **Sign-in:** a fragment request that needs a sign-in gets `401` with
//!   [`HX_REDIRECT`], so the parent leaves the container and signs in as a page
//!   ([`Container::sign_in`]). The parent also refuses any answer whose echo does not
//!   match the container it asked for, so a sign-in page reached by a plain redirect is
//!   never shown inside a window.

/// The request and response header that names the container.
pub const CONTAINER_HEADER: &str = "A2-Container";

/// The `Vary` value every negotiated response carries.
pub const VARY: &str = CONTAINER_HEADER;

/// The attribute on a fragment's root element.
pub const CONTENT_ATTR: &str = "data-a2-content";

/// The attribute on a fragment's root element that holds the container title.
pub const TITLE_ATTR: &str = "data-a2-title";

/// The response header that makes the parent leave the container and load a page
/// (htmx's own, which the shared container script relies on).
pub const HX_REDIRECT: &str = "HX-Redirect";

/// Where one addressed content surface is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Container {
    /// A standalone page with the application shell. The default and the fallback.
    #[default]
    Page,
    /// The second plane of a two-pane workspace (`.a2-duo`).
    Pane,
    /// A movable window on a desktop.
    Window,
}

impl Container {
    /// The container a request asks for, from the value of [`CONTAINER_HEADER`].
    ///
    /// Only the exact values `pane` and `window` select a fragment. Anything else, and a
    /// missing header, is [`Container::Page`]: asking for a fragment is an opt-in, and a
    /// page is always a correct answer.
    #[must_use]
    pub fn from_header(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("pane") => Container::Pane,
            Some("window") => Container::Window,
            _ => Container::Page,
        }
    }

    /// The header value of this container.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Container::Page => "page",
            Container::Pane => "pane",
            Container::Window => "window",
        }
    }

    /// Whether the response is the content alone, without the application shell.
    #[must_use]
    pub const fn is_fragment(self) -> bool {
        !matches!(self, Container::Page)
    }

    /// The headers every negotiated response carries, page and fragment alike: the
    /// content type, `Vary` and the echo of the container that was served.
    #[must_use]
    pub const fn response_headers(self) -> [(&'static str, &'static str); 3] {
        [
            ("Content-Type", "text/html; charset=utf-8"),
            ("Vary", VARY),
            (CONTAINER_HEADER, self.as_str()),
        ]
    }

    /// What a request for this container gets when it needs a sign-in first.
    #[must_use]
    pub fn sign_in(self, location: &str) -> SignIn<'_> {
        if self.is_fragment() {
            SignIn::LeaveContainer { location }
        } else {
            SignIn::Redirect { location }
        }
    }
}

/// The answer to a request that needs a sign-in, per container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignIn<'a> {
    /// A page: `303 See Other` to `location`.
    Redirect {
        /// The sign-in address.
        location: &'a str,
    },
    /// A fragment: `401` with [`HX_REDIRECT`] set to `location`. A sign-in page never
    /// renders inside a pane or a window.
    LeaveContainer {
        /// The sign-in address.
        location: &'a str,
    },
}

/// Why a fragment was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FragmentError {
    /// The fragment does not start with an element carrying [`CONTENT_ATTR`].
    NoContentRoot,
    /// The fragment carries a document or shell element (`<!doctype>`, `<html>`,
    /// `<head>`, `<body>`) or a `<script>`. Named by its lower-case tag.
    Forbidden(&'static str),
}

const FORBIDDEN: [&str; 5] = ["!doctype", "html", "head", "body", "script"];

/// Check that `html` is a content fragment: one root element carrying [`CONTENT_ATTR`]
/// first, and no document, shell or script element anywhere.
///
/// Tags are matched by name, so `<header>` or `<bodytext>` are not mistaken for `<head>`
/// or `<body>`. An `<iframe>` is allowed: genuine embedding stays possible inside content.
///
/// # Errors
///
/// [`FragmentError`] naming the first problem found.
pub fn check_fragment(html: &str) -> Result<(), FragmentError> {
    let body = html.trim_start();
    let first_tag_end = body.find('>').unwrap_or(body.len());
    if !body.starts_with('<') || !body[..first_tag_end].contains(CONTENT_ATTR) {
        return Err(FragmentError::NoContentRoot);
    }
    let bytes = body.as_bytes();
    let mut i = 0;
    while let Some(off) = body[i..].find('<') {
        let start = i + off + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'!') {
            end += 1;
        }
        let name = body[start..end].to_ascii_lowercase();
        if let Some(f) = FORBIDDEN.iter().find(|f| **f == name) {
            return Err(FragmentError::Forbidden(f));
        }
        i = start;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CAN FIRE: `pane` and `window` select fragments. CAN STAY SILENT: no header, an empty
    /// value, an unknown value and a differently cased value all fall back to the page.
    #[test]
    fn only_the_exact_values_select_a_fragment() {
        assert_eq!(Container::from_header(Some("pane")), Container::Pane);
        assert_eq!(Container::from_header(Some(" window ")), Container::Window);
        for v in [None, Some(""), Some("page"), Some("dialog"), Some("Window")] {
            assert_eq!(Container::from_header(v), Container::Page, "{v:?}");
        }
        assert!(!Container::Page.is_fragment());
        assert!(Container::Pane.is_fragment() && Container::Window.is_fragment());
    }

    /// Every answer varies on the header and echoes what it served, the page included:
    /// a cache that kept a fragment for a page request would break every page.
    #[test]
    fn every_answer_varies_and_echoes() {
        for c in [Container::Page, Container::Pane, Container::Window] {
            let h = c.response_headers();
            assert!(h.contains(&("Vary", "A2-Container")), "{c:?}");
            assert!(h.contains(&(CONTAINER_HEADER, c.as_str())), "{c:?}");
            assert!(h.contains(&("Content-Type", "text/html; charset=utf-8")));
        }
        assert_eq!(
            Container::from_header(Some(Container::Window.as_str())),
            Container::Window,
            "the echo must parse back to the container it names"
        );
    }

    /// A page redirects to the sign-in; a fragment leaves its container instead.
    #[test]
    fn a_fragment_never_receives_a_sign_in_page() {
        assert_eq!(
            Container::Page.sign_in("/login"),
            SignIn::Redirect { location: "/login" }
        );
        for c in [Container::Pane, Container::Window] {
            assert_eq!(
                c.sign_in("/login"),
                SignIn::LeaveContainer { location: "/login" }
            );
        }
    }

    /// CAN FIRE: a document, a shell element, a script, or a fragment without a content
    /// root is refused. CAN STAY SILENT: a real fragment with a header, a table, a form
    /// and an embedded viewer passes.
    #[test]
    fn a_fragment_is_content_and_nothing_else() {
        let good = r#"<section data-a2-content data-a2-title="Rechnung 4711"><header><h2>Rechnung</h2></header><table><tr><td>1</td></tr></table><form method="post" action="/x"><button>OK</button></form><iframe src="/viewer/1" title="PDF"></iframe><p>bodytext</p></section>"#;
        assert_eq!(check_fragment(good), Ok(()));
        assert_eq!(check_fragment(&format!("\n  {good}")), Ok(()));

        let cases = [
            (
                "<!DOCTYPE html><section data-a2-content></section>",
                FragmentError::NoContentRoot,
            ),
            ("<section>no marker</section>", FragmentError::NoContentRoot),
            ("plain text", FragmentError::NoContentRoot),
            (
                "<section data-a2-content><html></html></section>",
                FragmentError::Forbidden("html"),
            ),
            (
                "<section data-a2-content><HEAD></HEAD></section>",
                FragmentError::Forbidden("head"),
            ),
            (
                "<section data-a2-content><body></body></section>",
                FragmentError::Forbidden("body"),
            ),
            (
                "<section data-a2-content><script>alert(1)</script></section>",
                FragmentError::Forbidden("script"),
            ),
            (
                "<section data-a2-content><!doctype html></section>",
                FragmentError::Forbidden("!doctype"),
            ),
        ];
        for (html, want) in cases {
            assert_eq!(check_fragment(html), Err(want), "{html}");
        }
    }
}
