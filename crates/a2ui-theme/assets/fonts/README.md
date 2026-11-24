# Local typography assets (RPT-LOCAL-TYPE-1)

**Status: CSS prototype / NOT ENABLED.** `fonts.css` is checked in but NOT routed, linked or loaded by the production application. The WOFF2 binaries are intentionally **not** included in this planning PR. This directory does not currently constitute a working font package.

## Expected self-hosted bundle

| Filename | Font source | Face | Weight |
| --- | --- | --- | --- |
| `inter-roman-variable.woff2` | official Inter distribution | roman | 100–900 |
| `inter-italic-variable.woff2` | official Inter distribution | italic | 100–900 |
| `google-sans-flex-variable.woff2` | official Google Sans Flex build | roman | 100–900 |

Use vetted WOFF2 binaries rather than renaming TTFs; WOFF2 decoding must be valid. Verify German (including umlauts/ß), Latin-extended, symbols used by the cockpit and tabular digits. Font subsetting is a separate, reviewed operation. Do not assume an italic Google Sans Flex face exists.

## Sources and licensing

- Inter: https://github.com/rsms/inter (SIL OFL 1.1; preserve `LICENSE.txt` and copyright notices).
- Google Sans Flex: https://github.com/googlefonts/googlesans-flex (SIL OFL 1.1; preserve `OFL.txt`, copyright notices and honor `TRADEMARKS.txt`).
- Do not imply affiliation with or endorsement by Google. Do not treat the Google Sans Flex font name as an app brand.
- Before importing each binary, capture exact upstream release/commit, source URL, SHA-256, byte length, supported glyph coverage, license copy, and any allowed subsetting steps in a manifest committed in the same PR.

## Required wiring in a separate implementation slice

1. Add the three approved binary files and licensing files. The binaries MUST be present before any `include_bytes!` is added to Rust.
2. In `src/app.rs`, add a narrow allowlisted `/static/fonts/*path` handler (in the style of `holo_glass`) for `fonts.css` and exactly the three WOFF2 names. Send `text/css; charset=utf-8` and `font/woff2` respectively. Unknown paths must 404 without filesystem access.
3. Only then link `/static/fonts/fonts.css` from `templates/base.html` *after* the inline stylesheet. Use a controlled `data-font="inter"` default or a separately whitelisted `google-flex` preference. Preserve existing theme/material/density/focus preferences. Do not emit `data-font` from raw query strings.
4. If CSP is enforced, allow `font-src 'self'`; do not add `fonts.googleapis.com`, `fonts.gstatic.com` or any other runtime font host. No remote `@import`.
5. Verify same-origin requests in the browser Network panel and Rust route tests, including missing file paths and correct MIME/cache behavior.
6. Preserve the current `"Segoe UI",system-ui,-apple-system,sans-serif` fallback and original `--font-base` numeric sizing. Verify cockpit, service hub, dark and brand pages under default/Holo Glass material.

Release gate: do not claim “local fonts active” until all files are shipped in the image, the route and template tests pass, and browser computed-font and network evidence are attached to the implementing PR.

Full contract: [RPT-LOCAL-TYPE-1](../../docs/plans/rpt-local-typography-1.md).
