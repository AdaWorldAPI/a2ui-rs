//! The shared container script speaks the contract `a2ui_layout::container` defines.
//! JavaScript cannot read the Rust constants, so the names appear twice; this test is the
//! coupling. Change a name on one side only and it fails here, not in a browser.

use a2ui_layout::container::{CONTAINER_HEADER, CONTENT_ATTR, TITLE_ATTR};

fn text(path: &str) -> &'static str {
    let a = a2ui_theme::asset(path).unwrap_or_else(|| panic!("{path} is not an asset"));
    std::str::from_utf8(a.bytes).unwrap()
}

#[test]
fn the_script_uses_the_contract_names() {
    let js = text("containers/containers.js");
    assert!(js.contains(&format!("'{CONTAINER_HEADER}'")), "header name");
    assert!(
        js.contains(&format!("[{CONTENT_ATTR}]")),
        "content root attribute"
    );
    assert!(js.contains(&format!("'{TITLE_ATTR}'")), "title attribute");
    // Both fragment kinds are requested by their exact header value.
    for v in ["pane", "window"] {
        assert!(js.contains(&format!("'{v}'")), "{v}");
    }
}

/// The parent refuses any answer whose echo does not match, and leaves the container
/// for a sign-in instead of drawing it inside. CAN FIRE: removing the echo check from
/// the script drops the `getResponseHeader` comparison this test looks for.
#[test]
fn the_script_checks_the_echo_before_it_swaps() {
    let js = text("containers/containers.js");
    assert!(js.contains("htmx:beforeSwap"));
    assert!(js.contains("xhr.getResponseHeader(HEADER) === want"));
    assert!(js.contains("e.detail.shouldSwap = false"));
}

/// Loading is htmx's: the script must not grow its own fetch-and-swap.
#[test]
fn the_script_does_not_fetch_on_its_own() {
    let js = text("containers/containers.js");
    for own in ["fetch(", "XMLHttpRequest", "innerHTML ="] {
        assert!(
            !js.contains(own),
            "containers.js must leave loading to htmx: {own}"
        );
    }
    assert!(js.contains("htmx.ajax("));
}

/// The vendored htmx is the release the script is written against, with its licence.
#[test]
fn htmx_is_vendored_with_its_licence() {
    let h = text("vendor/htmx-2.0.4.min.js");
    assert!(h.starts_with("var htmx=function()"));
    assert!(h.contains("version:\"2.0.4\""));
    assert!(text("vendor/htmx-LICENSE.txt").starts_with("Zero-Clause BSD"));
}
