# Local typography assets (RPT-LOCAL-TYPE-1)

**Status: bundled.** The three WOFF2 files, `fonts.css` and the licence files are embedded in the `a2ui-theme` crate (`ASSETS`, read with `asset("fonts/…")`). Bundling does not activate them: an app serves them under its own `/static/fonts/` route and links `fonts.css` only for a viewer who chose a font (report-rs does this through `data-font`). Validate the app's route and template before enabling it.

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

## Wiring in an app

1. Serve `asset("fonts/<name>")` under a narrow `/static/fonts/*path` route with the asset's media type. Unknown paths return 404; `asset` matches exact paths only, so no filesystem access is involved.
2. Link `/static/fonts/fonts.css` after the inline stylesheet, and only for a whitelisted `data-font` preference (`inter` or `google-flex`). Never emit `data-font` from a raw query string.
3. If CSP is enforced, allow `font-src 'self'`. Add no remote font host and no remote `@import`.
4. Keep the `"Segoe UI",system-ui,-apple-system,sans-serif` fallback and the `--font-base` sizing.

Release gate: do not claim “local fonts active” until all files are shipped in the image, the route and template tests pass, and browser computed-font and network evidence are attached to the implementing PR.

Full contract: RPT-LOCAL-TYPE-1, `docs/plans/rpt-local-typography-1.md` in report-rs.
