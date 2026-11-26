//! **a2ui-theme** — the themes every app shares, kept as permanent assets.
//!
//! Each app used to carry its own copy of the palette, and the copies drifted. This crate
//! is now the one home: the stylesheet of every theme ([`THEMES_CSS`]), the theme list with
//! its menu labels ([`THEMES`]), the corporate palette as values for code that cannot read
//! CSS ([`brand`]), and the files a theme links ([`asset`]: the Holo Glass material, the
//! rail pictures, the fonts).
//!
//! Materials (how surfaces meet light) and the viewer's atmosphere (their own light) are
//! separate choices in [`material`]; no theme implies a material or a colour of light.
//!
//! The operator's name and logo are not part of any theme. They are not fit for a public
//! repository, so they come from the environment at runtime ([`Identity`]); every app
//! keeps its own variable names for them.

/// The stylesheet of every theme: one `:root[data-theme="<key>"]` rule set per entry of
/// [`THEMES`], with custom properties (`--bg`, `--fg`, `--accent`, …) and the few
/// component rules a theme adds.
pub const THEMES_CSS: &str = include_str!("../assets/themes.css");

pub mod material;
pub use material::{
    AMBIENTS, APPEARANCE_STOPS, Ambient, AppearanceStop, AquaOff, Atmosphere, DEFAULT_MATERIAL,
    MATERIALS, MaterialInfo, Rendering, appearance_stop, material,
};

/// The Holo Glass material, linked only while the `holo-glass` theme is in force. It
/// refers to its pictures relative to itself (`../assets/…`), so serve it under
/// `<prefix>/holo-glass/css/holo-glass.css` and the pictures through [`asset`] under the
/// same prefix.
pub const HOLO_GLASS_CSS: &str = include_str!("../assets/holo-glass/css/holo-glass.css");

/// The `@font-face` rules for the bundled fonts, relative to `<prefix>/fonts/`.
pub const FONTS_CSS: &str = include_str!("../assets/fonts/fonts.css");

/// One theme: the key used in `data-theme` and in saved settings, and its menu label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeInfo {
    pub key: &'static str,
    /// `None` for the corporate theme: its label is the operator's name ([`Identity`]).
    pub label: Option<&'static str>,
}

/// The key of the corporate theme.
pub const BRAND_KEY: &str = "brand";

const fn t(key: &'static str, label: &'static str) -> ThemeInfo {
    ThemeInfo {
        key,
        label: Some(label),
    }
}

/// Every theme, in menu order: the three primary looks first.
pub const THEMES: [ThemeInfo; 23] = [
    t("hub", "Service Hub"),
    ThemeInfo {
        key: BRAND_KEY,
        label: None,
    },
    t("fengshui", "Teal"),
    t("mint", "Mint"),
    t("citrus", "Citrus"),
    t("hub-blue", "Hub Dunkelblau"),
    t("breathe", "Light"),
    t("work", "Work (Präzision)"),
    t("command", "Command (Cockpit)"),
    t("office", "Office"),
    t("unity", "Unity"),
    t("moca", "Moca"),
    // Displayed as "Cinnamon"; the key stays `vanilla` so saved links and cookies resolve.
    t("vanilla", "Cinnamon"),
    t("aqua", "Aqua"),
    t("dark", "Dark"),
    t("marble", "Marble"),
    t("led", "LED"),
    t("breathing", "Slate"),
    t("holo", "Holo"),
    t("holo-tactile", "Holo Tactile"),
    t("holo-apothecary", "Holo Apothecary"),
    t("future", "Future"),
    t("holo-glass", "Holo Glass"),
];

/// The theme with this key (trimmed, case-insensitive).
pub fn theme(key: &str) -> Option<ThemeInfo> {
    let key = key.trim();
    THEMES
        .iter()
        .copied()
        .find(|t| t.key.eq_ignore_ascii_case(key))
}

/// The custom-property rule of one theme, `:root[data-theme="<key>"]{…}`, without the
/// component rules some themes add (buttons, tabs). For an app that brings its own
/// components and wants only the palette.
pub fn token_rule(key: &str) -> Option<&'static str> {
    let t = theme(key)?;
    let head = format!(r#":root[data-theme="{}"]{{"#, t.key);
    let start = THEMES_CSS.find(&head)?;
    let len = THEMES_CSS[start..].find('}')? + 1;
    Some(&THEMES_CSS[start..start + len])
}

/// The theme an app opens with when the viewer has chosen none: the key in `var`
/// (`REPORT_DEFAULT_THEME`, say), read through `get`; the corporate theme when unset or
/// blank. An unknown key is refused, so a typo fails at start instead of silently
/// falling back.
pub fn default_theme(
    get: impl Fn(&str) -> Option<String>,
    var: &str,
) -> Result<ThemeInfo, UnknownTheme> {
    match get(var).map(|v| v.trim().to_string()) {
        None => Ok(BRAND),
        Some(v) if v.is_empty() => Ok(BRAND),
        Some(v) => theme(&v).ok_or(UnknownTheme {
            var: var.to_string(),
            key: v,
        }),
    }
}

/// The corporate theme.
pub const BRAND: ThemeInfo = ThemeInfo {
    key: BRAND_KEY,
    label: None,
};

/// A default-theme variable names a key that is not in [`THEMES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTheme {
    pub var: String,
    pub key: String,
}

impl std::fmt::Display for UnknownTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} names an unknown theme: {:?}", self.var, self.key)
    }
}

impl std::error::Error for UnknownTheme {}

/// The corporate palette as plain values, for code that cannot read the stylesheet:
/// spreadsheet styles, PDF output, a server that renders its own top bar. They are the
/// values of the `brand` rule in [`THEMES_CSS`]; a test keeps the two equal.
pub mod brand {
    pub const BG: &str = "#f5f5f5";
    pub const FG: &str = "#1a1a1a";
    pub const H1: &str = "#1a1a1a";
    /// Secondary text and table headings.
    pub const MUTED: &str = "#4a4a4a";
    pub const ACCENT: &str = "#c8102e";
    /// The accent's pressed and border shade.
    pub const ACCENT_DARK: &str = "#a00d24";
    pub const ON_ACCENT: &str = "#ffffff";
    pub const BORDER: &str = "#e0e6ec";
    pub const CARD_BG: &str = "#ffffff";
    pub const SUBTLE_BG: &str = "#eef1f4";
    pub const SIDE_BG: &str = "#0d3a64";
    pub const SIDE_FG: &str = "#ffffff";
    pub const SIDE_HOVER: &str = "rgba(255,255,255,.08)";
    pub const RADIUS: &str = "14px";
    pub const RADIUS_SM: &str = "10px";
    pub const ELEVATION: &str = "0 4px 16px rgba(0,0,0,.08)";
    pub const FONT_FAMILY: &str =
        "-apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, Helvetica, Arial, sans-serif";

    /// Spreadsheet colours as `0xRRGGBB`.
    pub mod sheet {
        pub const RED: u32 = 0xC8102E;
        pub const LIGHT_RED: u32 = 0xF5E6E9;
        pub const GREY: u32 = 0x4A4A4A;
        pub const BAND: u32 = 0xF2F2F2;
        pub const LINE: u32 = 0xD9D9D9;
        pub const FONT: &str = "Aptos";
        pub const FONT_SIZE: f32 = 10.0;
    }
}

/// One file a theme links: its path below the theme prefix, its media type and its bytes.
#[derive(Debug, Clone, Copy)]
pub struct Asset {
    pub path: &'static str,
    pub media_type: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! assets {
    ($($path:literal => $mt:literal),* $(,)?) => {
        /// Every file the themes link, by path below the theme prefix.
        pub const ASSETS: &[Asset] = &[
            $(Asset { path: $path, media_type: $mt, bytes: include_bytes!(concat!("../assets/", $path)) }),*
        ];
    };
}

assets! {
    "themes.css" => "text/css; charset=utf-8",
    "holo-glass/css/holo-glass.css" => "text/css; charset=utf-8",
    "holo-v3/holo-v3.css" => "text/css; charset=utf-8",
    "aqua-glass/aqua-glass.css" => "text/css; charset=utf-8",
    "aqua-glass/aqua-sidecar.js" => "text/javascript; charset=utf-8",
    "aqua-glass/aqua-studio.js" => "text/javascript; charset=utf-8",
    "physical-glass/physical-glass.css" => "text/css; charset=utf-8",
    "physical-glass/physical-glass.js" => "text/javascript; charset=utf-8",
    "physical-glass/wallpapers/aurora.webp" => "image/webp",
    "physical-glass/wallpapers/night.webp" => "image/webp",
    "physical-glass/wallpapers/morning.webp" => "image/webp",
    "physical-glass/wallpapers/rose.webp" => "image/webp",
    "physical-glass/wallpapers/lagoon.webp" => "image/webp",
    "physical-glass/wallpapers/README.md" => "text/markdown; charset=utf-8",
    "holo-glass/assets/effects/ambient-glow.png" => "image/png",
    "holo-glass/assets/effects/ambient-ribbons.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__active@1x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__active@2x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__active@3x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__disabled@1x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__hover@1x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__hover@2x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__hover@3x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__idle@1x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__idle@2x.png" => "image/png",
    "holo-glass/assets/rail/rail_shell__idle@3x.png" => "image/png",
    "rail/rail_shell__active.png" => "image/png",
    "rail/rail_shell__hover.png" => "image/png",
    "rail/rail_shell__idle.png" => "image/png",
    "fonts/fonts.css" => "text/css; charset=utf-8",
    "fonts/google-sans-flex-variable.woff2" => "font/woff2",
    "fonts/inter-italic-variable.woff2" => "font/woff2",
    "fonts/inter-roman-variable.woff2" => "font/woff2",
    "fonts/LICENSE-Inter.txt" => "text/plain; charset=utf-8",
    "fonts/OFL-Google-Sans-Flex.txt" => "text/plain; charset=utf-8",
    "fonts/TRADEMARKS-Google-Sans-Flex.md" => "text/markdown; charset=utf-8",
    "fonts/README.md" => "text/markdown; charset=utf-8",
    "fonts/font-manifest.md" => "text/markdown; charset=utf-8",
}

/// The asset at this path below the theme prefix. Exact match only, so a request path
/// can be passed straight in without a traversal check.
pub fn asset(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}

/// The operator's name and logo, read from the environment at runtime. They stay out of
/// every repository; each app names its own two variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// Trimmed; empty when unset.
    pub name: String,
    /// An `https://` address, or `None` when unset.
    pub logo_url: Option<String>,
}

/// Why an identity setting was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// The logo variable is set but is not a plain `https://` address.
    LogoNotHttps { var: String },
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::LogoNotHttps { var } => write!(f, "{var} must be an https address"),
        }
    }
}

impl std::error::Error for IdentityError {}

impl Identity {
    /// Reads the name from `name_var` and the logo from `logo_var` through `get`
    /// (`|k| std::env::var(k).ok()` in an app). Blank values count as unset. The value
    /// of a refused logo is never echoed.
    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        name_var: &str,
        logo_var: &str,
    ) -> Result<Identity, IdentityError> {
        let name = get(name_var)
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        let logo_url = match get(logo_var).map(|v| v.trim().to_string()) {
            None => None,
            Some(v) if v.is_empty() => None,
            Some(v) if https_url_ok(&v) => Some(v),
            Some(_) => {
                return Err(IdentityError::LogoNotHttps {
                    var: logo_var.to_string(),
                });
            }
        };
        Ok(Identity { name, logo_url })
    }

    /// The menu label of a theme: the operator's name for the corporate theme, `fallback`
    /// when no name is set.
    pub fn label<'a>(&'a self, theme: &ThemeInfo, fallback: &'a str) -> &'a str {
        match theme.label {
            Some(l) => l,
            None if self.name.is_empty() => fallback,
            None => &self.name,
        }
    }

    /// `<img>` for the logo with the name as its alternative text, empty without a logo.
    pub fn logo_html(&self, class: &str) -> String {
        match &self.logo_url {
            None => String::new(),
            Some(url) => {
                let alt = if self.name.is_empty() {
                    "Logo"
                } else {
                    &self.name
                };
                format!(
                    r#"<img class="{}" src="{}" alt="{}">"#,
                    escape_html(class),
                    escape_html(url),
                    escape_html(alt)
                )
            }
        }
    }
}

/// A plain `https://host/...` address: no credentials, no whitespace, no characters that
/// could leave an HTML attribute or a CSS `url()`.
pub fn https_url_ok(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    host_ok(authority)
        && raw.chars().all(|c| {
            c.is_ascii_graphic() && !matches!(c, '"' | '\'' | '<' | '>' | '\\' | '(' | ')' | '`')
        })
}

/// The authority of an `https` address: a non-empty host (a name, or a bracketed IPv6
/// address) and an optional numeric port; no credentials.
fn host_ok(authority: &str) -> bool {
    if authority.contains('@') {
        return false;
    }
    let (host, port) = match authority.strip_prefix('[') {
        Some(v6) => match v6.split_once(']') {
            Some((addr, rest)) if !addr.is_empty() => ("[v6]", rest),
            _ => return false,
        },
        None => match authority.split_once(':') {
            Some((h, p)) => (h, &authority[h.len()..][..1 + p.len()]),
            None => (authority, ""),
        },
    };
    let port_ok = match port.strip_prefix(':') {
        None => port.is_empty(),
        Some(p) => !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()),
    };
    !host.is_empty() && port_ok
}

/// Escapes text for HTML text and double-quoted attributes.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(key: &str) -> &'static str {
        let head = format!(r#":root[data-theme="{key}"]{{"#);
        let start = THEMES_CSS
            .find(&head)
            .unwrap_or_else(|| panic!("no rule for {key}"));
        let body = &THEMES_CSS[start + head.len()..];
        &body[..body.find('}').expect("rule closes")]
    }

    #[test]
    fn every_theme_has_a_rule_and_every_rule_a_theme() {
        for t in THEMES {
            assert!(rule(t.key).contains("--accent:"), "{} has no accent", t.key);
        }
        let mut keys: Vec<&str> = THEMES_CSS
            .match_indices(r#":root[data-theme=""#)
            .map(|(i, m)| {
                let rest = &THEMES_CSS[i + m.len()..];
                &rest[..rest.find('"').unwrap()]
            })
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), THEMES.len());
        assert!(keys.iter().all(|k| theme(k).is_some()));
    }

    #[test]
    fn a_token_rule_is_one_themes_palette_and_nothing_else() {
        for t in THEMES {
            let r = token_rule(t.key).unwrap();
            assert!(r.starts_with(&format!(r#":root[data-theme="{}"]{{"#, t.key)));
            assert!(r.ends_with('}') && r.matches('{').count() == 1);
            assert!(r.contains("--accent:"));
        }
        assert_eq!(token_rule(" BRAND "), token_rule(BRAND_KEY));
        assert!(!token_rule(BRAND_KEY).unwrap().contains("button"));
        assert_eq!(token_rule("nope"), None);
    }

    /// FNV-1a 64 of a file's bytes.
    fn fnv(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// The theme stylesheets are the exact bytes report-rs shipped before they moved
    /// here. A change to how a theme looks is the owner's decision, made on purpose by
    /// updating these two numbers, never as a side effect.
    #[test]
    fn the_theme_stylesheets_are_the_approved_bytes() {
        assert_eq!(
            (THEMES_CSS.len(), fnv(THEMES_CSS.as_bytes())),
            (11981, 0x6f912e122759d3c9)
        );
        assert_eq!(
            (HOLO_GLASS_CSS.len(), fnv(HOLO_GLASS_CSS.as_bytes())),
            (21634, 0x68fdceb72513782f)
        );
    }

    #[test]
    fn the_brand_values_equal_the_brand_rule() {
        let r = rule(BRAND_KEY);
        for (prop, v) in [
            ("--bg", brand::BG),
            ("--fg", brand::FG),
            ("--h1", brand::H1),
            ("--accent", brand::ACCENT),
            ("--border", brand::BORDER),
            ("--card-bg", brand::CARD_BG),
            ("--side-bg", brand::SIDE_BG),
            ("--side-fg", brand::SIDE_FG),
            ("--side-hover", brand::SIDE_HOVER),
            ("--radius", brand::RADIUS),
            ("--radius-sm", brand::RADIUS_SM),
        ] {
            assert!(
                r.contains(&format!("{prop}:{v};")) || r.ends_with(&format!("{prop}:{v}")),
                "{prop} != {v}"
            );
        }
        assert!(r.contains(&format!("--elev-1:{}", brand::ELEVATION)));
        assert_eq!(format!("#{:06x}", brand::sheet::RED), brand::ACCENT);
        assert!(THEMES_CSS.contains(brand::ACCENT_DARK));
    }

    #[test]
    fn the_default_theme_is_a_variable_and_falls_back_to_the_corporate_theme() {
        let set = |v: &'static str| move |k: &str| (k == "D").then(|| v.to_string());
        assert_eq!(default_theme(|_| None, "D"), Ok(BRAND));
        assert_eq!(default_theme(set("  "), "D"), Ok(BRAND));
        assert_eq!(
            default_theme(set(" Holo-Glass "), "D").map(|t| t.key),
            Ok("holo-glass")
        );
        let err = default_theme(set("hollo"), "D").unwrap_err();
        assert_eq!(err.to_string(), r#"D names an unknown theme: "hollo""#);
        assert!(THEMES.contains(&BRAND));
    }

    #[test]
    fn theme_lookup_is_trimmed_and_case_insensitive_and_refuses_unknown_keys() {
        assert_eq!(theme(" Holo-Glass ").map(|t| t.key), Some("holo-glass"));
        assert_eq!(theme("nope"), None);
    }

    #[test]
    fn every_asset_file_is_listed_and_every_listed_file_exists() {
        fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, base, out);
                } else {
                    out.push(
                        p.strip_prefix(base)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let mut on_disk = Vec::new();
        walk(&base, &base, &mut on_disk);
        on_disk.sort();
        let mut listed: Vec<String> = ASSETS.iter().map(|a| a.path.to_string()).collect();
        listed.sort();
        assert_eq!(listed, on_disk);
        assert!(ASSETS.iter().all(|a| !a.bytes.is_empty()));
        assert!(asset("../Cargo.toml").is_none());
        assert_eq!(
            asset("rail/rail_shell__idle.png").map(|a| a.media_type),
            Some("image/png")
        );
    }

    #[test]
    fn the_operators_name_is_in_no_asset() {
        // The name a theme used to carry, assembled so that it is not spelled here either.
        let name = ["data", "group"].concat();
        for a in ASSETS {
            let lower = a.bytes.to_ascii_lowercase();
            assert!(
                !lower.windows(name.len()).any(|w| w == name.as_bytes()),
                "{} contains the name",
                a.path
            );
        }
    }

    #[test]
    fn the_identity_comes_from_the_two_variables() {
        let env = |k: &str| match k {
            "N" => Some("  Acme  ".to_string()),
            "L" => Some("https://cdn.example.org/logo.png".to_string()),
            _ => None,
        };
        let id = Identity::from_lookup(env, "N", "L").unwrap();
        assert_eq!(id.name, "Acme");
        assert_eq!(id.label(&theme(BRAND_KEY).unwrap(), "Brand"), "Acme");
        assert_eq!(id.label(&theme("dark").unwrap(), "Brand"), "Dark");
        assert_eq!(
            id.logo_html("logo"),
            r#"<img class="logo" src="https://cdn.example.org/logo.png" alt="Acme">"#
        );
        let none = Identity::from_lookup(|_| None, "N", "L").unwrap();
        assert_eq!(none, Identity::default());
        assert_eq!(none.label(&theme(BRAND_KEY).unwrap(), "Brand"), "Brand");
        assert_eq!(none.logo_html("logo"), "");
    }

    #[test]
    fn a_logo_that_is_not_plain_https_is_refused_without_echoing_it() {
        for bad in [
            "http://x.example/l.png",
            "https://",
            "https://u@x.example/",
            "https://x.example/a\"b",
            "https://:443/logo.png",
            "https://x.example:/l.png",
            "https://[]/l.png",
            "javascript:x",
        ] {
            let err = Identity::from_lookup(|k| (k == "L").then(|| bad.to_string()), "N", "L")
                .unwrap_err();
            assert!(!err.to_string().contains(bad), "{bad} echoed");
        }
        assert!(
            Identity::from_lookup(|k| (k == "L").then(|| "  ".into()), "N", "L")
                .unwrap()
                .logo_url
                .is_none()
        );
    }

    #[test]
    fn hosts_with_ports_and_ipv6_are_accepted() {
        for ok in [
            "https://x.example/l.png",
            "https://x.example:8443/l.png",
            "https://[::1]:443/l.png",
            "https://[::1]/l.png",
        ] {
            assert!(https_url_ok(ok), "{ok}");
        }
    }

    #[test]
    fn the_name_is_escaped_in_the_logo() {
        let id = Identity {
            name: "A<b>\"".into(),
            logo_url: Some("https://x.example/l.png".into()),
        };
        assert!(id.logo_html("c").contains(r#"alt="A&lt;b&gt;&quot;""#));
    }
}
