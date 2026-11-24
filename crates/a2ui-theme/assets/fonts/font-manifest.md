# Font manifest (RPT-LOCAL-TYPE-1)

Self-hosted, served same-origin from the binary (`include_bytes!`). No font CDN.

| File | Source | Revision | Transformation | Bytes | SHA-256 |
|---|---|---|---|---|---|
| `inter-roman-variable.woff2` | github.com/rsms/inter release `Inter-4.1.zip`, `web/InterVariable.woff2` | v4.1 | none (copied) | 352240 | `693b77d4f32ee9b8bfc995589b5fad5e99adf2832738661f5402f9978429a8e3` |
| `inter-italic-variable.woff2` | same release, `web/InterVariable-Italic.woff2` | v4.1 | none (copied) | 387976 | `e564f652916db6c139570fefb9524a77c4d48f30c92928de9db19b6b5c7a262a` |
| `google-sans-flex-variable.woff2` | github.com/google/fonts `ofl/googlesansflex/GoogleSansFlex[ROND,opsz,slnt,wdth,wght].ttf` (TTF SHA-256 `a9ac770798857d5e8ac593ced42b22e40a7c08cec98319f92b0eccc2783a77ba`) | google/fonts `cc39c4c2c304bc368c9b7c311ea2bcc6013299fe` (2026-11-14) | TTF → WOFF2 with fontTools (`TTFont.flavor = "woff2"`), no subsetting | 1452172 | `3bca8dae7508a501f805b6e1c943e26b771dffeaf37a7df2eb6620257ec590d6` |

Licenses: Inter, SIL OFL 1.1 (`LICENSE-Inter.txt`). Google Sans Flex, SIL OFL 1.1 (`OFL-Google-Sans-Flex.txt`); trademark guidance in `TRADEMARKS-Google-Sans-Flex.md`. No affiliation with or endorsement by Google is implied.

Google Sans Flex axes (from `fvar`): opsz 1–144, wdth 25–151, wght 1–1000, ROND 0–100, slnt −10–0. Glyph check: `ß` and `ü` present. There is no separate italic file; italics use `slnt`.
