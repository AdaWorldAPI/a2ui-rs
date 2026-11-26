//! Materials and the personal atmosphere, kept apart from themes.
//!
//! An interface is described by four independent choices, and none of them implies another:
//!
//! | dimension | what it decides | where it lives |
//! |---|---|---|
//! | identity | the operator's name and logo | [`crate::Identity`], from the environment |
//! | theme | palette and typography | [`crate::THEMES`] |
//! | material | how surfaces interact with light | [`MATERIALS`] (this module) |
//! | atmosphere | the viewer's own light: colour and intensity | [`Atmosphere`] (this module) |
//!
//! A material never names a colour and a theme never names an optical effect, so any theme
//! can be combined with any material and any atmosphere. Every material's stylesheet and script
//! are assets of this crate, so every app links the same files; this module names and
//! validates the choices. The materials are distinct designs: they share no rendering
//! mechanism and none is derived from another. The optical engine (`a2ui_paint::optics`, GPU
//! research) is independent of them.
//!
//! Zero dependencies, like the rest of the crate.

/// How a material is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rendering {
    /// No material: flat surfaces, nothing linked.
    None,
    /// Rules inside the always-present [`crate::THEMES_CSS`]; nothing extra to link.
    Shared,
    /// A stylesheet among this crate's assets ([`crate::asset`]), linked when in force.
    Stylesheet(&'static str),
    /// Computed by the optical engine (`a2ui_paint::optics`, the GPU research path). No
    /// shipped material uses it: Aqua Glass in production is native DOM ([`Stylesheet`]).
    ///
    /// [`Stylesheet`]: Rendering::Stylesheet
    Optical,
}

/// One material: the key used in `data-material` and in saved settings, its menu label,
/// how it is drawn, the script it needs (asset path), and whether the viewer's
/// [`Atmosphere`] applies to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub rendering: Rendering,
    pub script: Option<&'static str>,
    pub uses_atmosphere: bool,
}

/// The key of the "no material" entry.
pub const NONE_KEY: &str = "none";

/// The material in force when the viewer has no preference: Holo Glass V2.
pub const DEFAULT_MATERIAL: &str = "holo-v2";

/// Which of Aqua Glass's three mechanisms are switched off: the neutral glass, the rear
/// light and the prismatic light. The accepted look has all three on. Switching one off
/// changes no rule of the frozen stylesheet; it sets `--aqua-l-rear` / `--aqua-l-beam` to 0
/// or `data-aqua-glass="off"`, the same switches as the review parameter `?aqua-off=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AquaOff {
    pub glass: bool,
    pub rear: bool,
    pub beam: bool,
}

impl AquaOff {
    /// Everything on: the accepted Aqua Glass.
    pub const NONE: AquaOff = AquaOff {
        glass: false,
        rear: false,
        beam: false,
    };

    /// Parse tokens `glass`, `rear`, `beam` separated by `,` or `.` (any order, repeats
    /// allowed). Empty or `none` is [`AquaOff::NONE`]; any other token refuses the whole
    /// value (`None`), so a caller never applies half a setting.
    pub fn parse(s: &str) -> Option<AquaOff> {
        let s = s.trim();
        let mut off = AquaOff::NONE;
        if s.is_empty() || s.eq_ignore_ascii_case("none") {
            return Some(off);
        }
        for t in s.split([',', '.']) {
            match t.trim() {
                "glass" => off.glass = true,
                "rear" => off.rear = true,
                "beam" => off.beam = true,
                _ => return None,
            }
        }
        Some(off)
    }

    /// The canonical token list, `.`-separated in the order glass, rear, beam (cookie-safe);
    /// empty for [`AquaOff::NONE`].
    pub fn key(self) -> String {
        [
            ("glass", self.glass),
            ("rear", self.rear),
            ("beam", self.beam),
        ]
        .iter()
        .filter(|(_, on)| *on)
        .map(|(t, _)| *t)
        .collect::<Vec<_>>()
        .join(".")
    }

    /// The CSS custom properties that switch the lights off, for `<html style>` (numbers only).
    pub fn css_vars(self) -> String {
        let mut v = String::new();
        if self.rear {
            v.push_str("--aqua-l-rear:0;");
        }
        if self.beam {
            v.push_str("--aqua-l-beam:0;");
        }
        v
    }
}

/// One stop of the appearance sidecar's slider: a material and, for Aqua Glass, which of its
/// mechanisms are off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceStop {
    /// Stable key of the stop.
    pub key: &'static str,
    /// The material key (`data-material`).
    pub material: &'static str,
    /// For Aqua Glass: the mechanisms switched off, in [`AquaOff::parse`] tokens.
    pub aqua_off: &'static str,
    /// Menu label (English; an app may show its own).
    pub label: &'static str,
}

/// The appearance sidecar's slider, left to right: Holo Glass V2 (the default), the accepted
/// Aqua Glass (rear and prismatic light), then the four review states of the same Aqua Glass
/// (rear light only, prismatic light only, neutral glass, light without the glass). The
/// other materials stay choosable where an app lists [`MATERIALS`].
pub const APPEARANCE_STOPS: [AppearanceStop; 6] = [
    AppearanceStop {
        key: DEFAULT_MATERIAL,
        material: DEFAULT_MATERIAL,
        aqua_off: "",
        label: "Holo Glass V2",
    },
    AppearanceStop {
        key: "aqua-glass",
        material: "aqua-glass",
        aqua_off: "",
        label: "Aqua Glass",
    },
    AppearanceStop {
        key: "aqua-rear",
        material: "aqua-glass",
        aqua_off: "beam",
        label: "Aqua Glass, rear light only",
    },
    AppearanceStop {
        key: "aqua-prism",
        material: "aqua-glass",
        aqua_off: "rear",
        label: "Aqua Glass, prismatic light only",
    },
    AppearanceStop {
        key: "aqua-neutral",
        material: "aqua-glass",
        aqua_off: "rear.beam",
        label: "Aqua Glass, neutral glass",
    },
    AppearanceStop {
        key: "aqua-light",
        material: "aqua-glass",
        aqua_off: "glass",
        label: "Aqua light without the glass",
    },
];

/// The slider stop for a material and Aqua setting: the exact stop when there is one, else
/// `None` (a material that is not on the slider).
pub fn appearance_stop(material_key: &str, off: AquaOff) -> Option<usize> {
    APPEARANCE_STOPS.iter().position(|s| {
        s.material == material_key
            && (s.material != "aqua-glass" || AquaOff::parse(s.aqua_off) == Some(off))
    })
}

/// Every material the shared tier provides, in menu order. Each is its own design with its
/// own rules; none is a parameter set of another.
///
/// Holo Glass is listed both as a theme and as a material: the theme carries the palette,
/// the material carries the stylesheet, so the glass can be chosen under any theme. Holo V2
/// shares the Holo Glass stylesheet but has its own rules (`data-material="holo-v2"`).
pub const MATERIALS: [MaterialInfo; 8] = [
    MaterialInfo {
        key: NONE_KEY,
        label: "No material",
        rendering: Rendering::None,
        script: None,
        uses_atmosphere: false,
    },
    MaterialInfo {
        key: "holo-glass",
        label: "Holo Glass",
        rendering: Rendering::Stylesheet("holo-glass/css/holo-glass.css"),
        script: None,
        uses_atmosphere: false,
    },
    MaterialInfo {
        key: "elevation",
        label: "Elevation",
        rendering: Rendering::Shared,
        script: None,
        uses_atmosphere: false,
    },
    MaterialInfo {
        key: "frosted",
        label: "Frosted Glass",
        rendering: Rendering::Shared,
        script: None,
        uses_atmosphere: false,
    },
    MaterialInfo {
        key: DEFAULT_MATERIAL,
        label: "Holo Glass V2",
        rendering: Rendering::Stylesheet("holo-glass/css/holo-glass.css"),
        script: None,
        uses_atmosphere: false,
    },
    MaterialInfo {
        key: "holo-v3",
        label: "Holo Glass V3",
        rendering: Rendering::Stylesheet("holo-v3/holo-v3.css"),
        script: None,
        uses_atmosphere: false,
    },
    MaterialInfo {
        key: "aqua-glass",
        label: "Aqua Glass",
        rendering: Rendering::Stylesheet("aqua-glass/aqua-glass.css"),
        script: Some("aqua-glass/aqua-sidecar.js"),
        uses_atmosphere: true,
    },
    // The ray-traced surface of `a2ui_paint::optics::surface` as an SVG backdrop filter
    // (Chromium). The script loads the engine's maps from the app's map endpoint; without
    // them every surface is a readable frosted pane.
    MaterialInfo {
        key: "physical-glass",
        label: "Physikalisches Glas",
        rendering: Rendering::Stylesheet("physical-glass/physical-glass.css"),
        script: Some("physical-glass/physical-glass.js"),
        uses_atmosphere: false,
    },
];

/// The material with this key (trimmed, case-insensitive). Unknown keys are `None`, never a
/// fallback, so a caller decides what "no preference" means.
pub fn material(key: &str) -> Option<MaterialInfo> {
    let key = key.trim();
    MATERIALS
        .iter()
        .copied()
        .find(|m| m.key.eq_ignore_ascii_case(key))
}

/// A named light colour for the atmosphere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ambient {
    pub key: &'static str,
    pub label: &'static str,
    /// Degrees, `0..=360`.
    pub hue: u16,
    /// Percent; 0 is clear (no colour).
    pub sat: u8,
}

/// The ambient-light presets, in menu order.
pub const AMBIENTS: [Ambient; 6] = [
    Ambient {
        key: "ruby",
        label: "Ruby",
        hue: 352,
        sat: 75,
    },
    Ambient {
        key: "ocean",
        label: "Ocean",
        hue: 212,
        sat: 80,
    },
    Ambient {
        key: "mint",
        label: "Mint",
        hue: 158,
        sat: 55,
    },
    Ambient {
        key: "vanilla",
        label: "Vanilla",
        hue: 45,
        sat: 70,
    },
    Ambient {
        key: "titanium",
        label: "Titanium",
        hue: 215,
        sat: 15,
    },
    Ambient {
        key: "crystal",
        label: "Crystal",
        hue: 0,
        sat: 0,
    },
];

/// The viewer's own light: colour and intensity, independent of theme and material.
///
/// `strength` is the master: every optical effect is weighted by it, so 0 is a plain neutral
/// interface. Whole numbers; `hue` is degrees, everything else percent. A value always holds
/// clamped numbers: build it with [`Atmosphere::new`], [`Atmosphere::parse`] or by clamping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atmosphere {
    pub hue: u16,
    /// Colour of the light; 0 is clear.
    pub sat: u8,
    pub strength: u8,
    pub transmission: u8,
    /// Restrained to [`Atmosphere::GLARE_MIN`]..=[`Atmosphere::GLARE_MAX`].
    pub glare: u8,
    /// Intensity of the light behind active surfaces.
    pub halo: u8,
    pub depth: u8,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Atmosphere::DEFAULT
    }
}

impl Atmosphere {
    /// Ruby at 53 % strength: the approved reference.
    pub const DEFAULT: Atmosphere = Atmosphere {
        hue: 352,
        sat: 75,
        strength: 53,
        transmission: 85,
        glare: 53,
        halo: 45,
        depth: 55,
    };
    /// Glare stays restrained: never below this.
    pub const GLARE_MIN: u8 = 43;
    /// Glare stays restrained: never above this.
    pub const GLARE_MAX: u8 = 63;

    /// Build from raw numbers, clamping each into its range.
    pub fn new(
        hue: u16,
        sat: u8,
        strength: u8,
        transmission: u8,
        glare: u8,
        halo: u8,
        depth: u8,
    ) -> Self {
        Atmosphere {
            hue,
            sat,
            strength,
            transmission,
            glare,
            halo,
            depth,
        }
        .clamped()
    }

    /// Every value in its range: hue `0..=360`, glare restrained, the rest `0..=100`.
    #[must_use]
    pub fn clamped(self) -> Atmosphere {
        Atmosphere {
            hue: self.hue.min(360),
            sat: self.sat.min(100),
            strength: self.strength.min(100),
            transmission: self.transmission.min(100),
            glare: self.glare.clamp(Self::GLARE_MIN, Self::GLARE_MAX),
            halo: self.halo.min(100),
            depth: self.depth.min(100),
        }
    }

    /// The same values with the light colour of a preset.
    #[must_use]
    pub fn with_ambient(self, ambient: &Ambient) -> Atmosphere {
        Atmosphere {
            hue: ambient.hue,
            sat: ambient.sat,
            ..self
        }
        .clamped()
    }

    /// `hue.sat.strength.transmission.glare.halo.depth`: safe in a cookie and a URL.
    pub fn key(self) -> String {
        format!(
            "{}.{}.{}.{}.{}.{}.{}",
            self.hue, self.sat, self.strength, self.transmission, self.glare, self.halo, self.depth
        )
    }

    /// Exactly seven whole numbers; out-of-range values are clamped, anything else is `None`.
    pub fn parse(s: &str) -> Option<Atmosphere> {
        let n: Vec<u16> = s
            .trim()
            .split('.')
            .map(|p| p.parse::<u16>().ok())
            .collect::<Option<Vec<_>>>()?;
        if n.len() != 7 {
            return None;
        }
        let pct = |v: u16| v.min(100) as u8;
        Some(Atmosphere::new(
            n[0],
            pct(n[1]),
            pct(n[2]),
            pct(n[3]),
            pct(n[4]),
            pct(n[5]),
            pct(n[6]),
        ))
    }

    /// The preset whose light colour these values carry, if any.
    pub fn ambient(self) -> Option<&'static Ambient> {
        AMBIENTS
            .iter()
            .find(|a| (a.hue, a.sat) == (self.hue, self.sat))
    }

    /// The custom properties for the root element, `--atmo-hue` in degrees and every other
    /// value as a fraction `0.00..=1.00`. Numbers only, so no escaping is needed.
    pub fn css_vars(self) -> String {
        let f = |v: u8| format!("{:.2}", f32::from(v) / 100.0);
        format!(
            "--atmo-hue:{};--atmo-sat:{};--atmo-strength:{};--atmo-transmission:{};--atmo-glare:{};--atmo-halo:{};--atmo-depth:{}",
            self.hue,
            f(self.sat),
            f(self.strength),
            f(self.transmission),
            f(self.glare),
            f(self.halo),
            f(self.depth)
        )
    }

    /// True when only the light colour differs: what changes if the viewer picks another
    /// ambient preset. A renderer can keep every geometry-derived resource for such a change.
    pub fn same_except_colour(self, other: Atmosphere) -> bool {
        Atmosphere {
            hue: 0,
            sat: 0,
            ..self
        } == Atmosphere {
            hue: 0,
            sat: 0,
            ..other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_are_looked_up_by_key_and_unknown_keys_are_refused() {
        assert_eq!(material(" Aqua-Glass ").map(|m| m.key), Some("aqua-glass"));
        assert_eq!(
            material(NONE_KEY).map(|m| m.rendering),
            Some(Rendering::None)
        );
        assert_eq!(material("holo-v2").map(|m| m.key), Some(DEFAULT_MATERIAL));
        assert_eq!(material("holo-v4"), None);
        assert_eq!(material(""), None);
        let mut keys: Vec<_> = MATERIALS.iter().map(|m| m.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), MATERIALS.len(), "keys are unique");
    }

    #[test]
    fn every_material_stylesheet_is_one_of_the_crates_assets() {
        let mut sheets = 0;
        for m in MATERIALS {
            if let Rendering::Stylesheet(path) = m.rendering {
                let a = crate::asset(path)
                    .unwrap_or_else(|| panic!("{} links a missing {path}", m.key));
                assert!(a.media_type.starts_with("text/css"));
                sheets += 1;
            }
        }
        assert!(sheets > 0, "the check must cover at least one stylesheet");
    }

    /// Each material carries its own rules: its key appears as `data-material="<key>"` in the
    /// sheet it links (or in the shared theme sheet), so a material that silently lost its
    /// rules, or was folded into another, fails here.
    #[test]
    fn every_material_has_its_own_rules_and_assets() {
        let text = |p: &str| std::str::from_utf8(crate::asset(p).unwrap().bytes).unwrap();
        for m in MATERIALS {
            let selector = format!("data-material=\"{}\"", m.key);
            match m.rendering {
                Rendering::None => assert_eq!(m.key, NONE_KEY),
                Rendering::Shared => {
                    assert!(crate::THEMES_CSS.contains(&selector), "{}", m.key)
                }
                Rendering::Stylesheet(p) => assert!(text(p).contains(&selector), "{}", m.key),
                Rendering::Optical => panic!("{} ships no GPU material", m.key),
            }
            if let Some(js) = m.script {
                let a = crate::asset(js).unwrap_or_else(|| panic!("{} needs {js}", m.key));
                assert!(a.media_type.starts_with("text/javascript"));
            }
        }
    }

    #[test]
    fn the_slider_starts_at_the_default_then_the_accepted_aqua_and_its_four_states() {
        assert_eq!(DEFAULT_MATERIAL, "holo-v2");
        assert_eq!(APPEARANCE_STOPS[0].material, DEFAULT_MATERIAL);
        assert_eq!(APPEARANCE_STOPS[1].material, "aqua-glass");
        assert_eq!(
            AquaOff::parse(APPEARANCE_STOPS[1].aqua_off),
            Some(AquaOff::NONE)
        );
        let mut keys: Vec<_> = APPEARANCE_STOPS.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), APPEARANCE_STOPS.len(), "unique keys");
        let mut offs = Vec::new();
        for (i, s) in APPEARANCE_STOPS.iter().enumerate() {
            assert!(material(s.material).is_some(), "{} is registered", s.key);
            let off = AquaOff::parse(s.aqua_off).unwrap_or_else(|| panic!("{} tokens", s.key));
            if s.material == "aqua-glass" {
                assert!(
                    !offs.contains(&off),
                    "{}: each Aqua stop is a distinct state",
                    s.key
                );
                offs.push(off);
            }
            assert_eq!(
                appearance_stop(s.material, off),
                Some(i),
                "{} round-trips",
                s.key
            );
        }
        assert_eq!(offs.len(), 5, "the accepted Aqua and four review states");
        assert_eq!(appearance_stop("elevation", AquaOff::NONE), None);
    }

    /// Physikalisches Glas draws the engine's maps and computes no optics itself; its
    /// fallback stays readable, and it never touches the frozen Aqua stylesheet.
    #[test]
    fn physical_glass_renders_the_engine_not_a_copy() {
        let m = material("physical-glass").expect("physical-glass");
        assert_eq!(m.label, "Physikalisches Glas");
        let js = std::str::from_utf8(
            crate::asset("physical-glass/physical-glass.js")
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert!(js.contains("/optics/surface/map") && js.contains("x-a2ui-box"));
        for forbidden in [
            "Math.sqrt",
            "Math.hypot",
            "Math.sin",
            "createImageData",
            "putImageData",
        ] {
            assert!(!js.contains(forbidden), "{forbidden}");
        }
        let css = std::str::from_utf8(
            crate::asset("physical-glass/physical-glass.css")
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert!(
            css.contains("var(--card-bg) 88%"),
            "frosted fallback before the maps arrive"
        );
        assert!(!css.contains("aqua"), "independent of Aqua Glass");
        // every background the selector offers has its rule, every wallpaper is an asset
        for bg in ["brushed", "aurora", "morning", "rose", "lagoon"] {
            assert!(js.contains(&format!("['{bg}', ")), "{bg} offered");
            assert!(
                css.contains(&format!("[data-pg-bg=\"{bg}\"] body")),
                "{bg} drawn"
            );
        }
        for w in ["aurora", "morning", "rose", "lagoon"] {
            let a = crate::asset(&format!("physical-glass/wallpapers/{w}.webp")).expect(w);
            assert_eq!(&a.bytes[8..12], b"WEBP", "{w}");
            assert!(css.contains(&format!("url(\"physical-glass/wallpapers/{w}.webp\")")));
        }
    }

    #[test]
    fn aqua_off_parses_canonically_and_refuses_unknown_tokens() {
        assert_eq!(AquaOff::parse(""), Some(AquaOff::NONE));
        assert_eq!(AquaOff::parse("none"), Some(AquaOff::NONE));
        let both = AquaOff::parse("beam,rear").unwrap();
        assert_eq!(both, AquaOff::parse("rear.beam").unwrap());
        assert_eq!(both.key(), "rear.beam");
        assert_eq!(both.css_vars(), "--aqua-l-rear:0;--aqua-l-beam:0;");
        let glass = AquaOff::parse("glass").unwrap();
        assert!(glass.glass && glass.css_vars().is_empty());
        for bad in ["prism", "rear,", "rear;beam", "glass,x"] {
            assert_eq!(AquaOff::parse(bad), None, "{bad}");
        }
        for s in APPEARANCE_STOPS {
            let off = AquaOff::parse(s.aqua_off).unwrap();
            assert_eq!(AquaOff::parse(&off.key()), Some(off));
        }
    }

    /// Orthogonality: the atmosphere names no theme and no material, and a material names no
    /// colour. Picking one never changes another.
    #[test]
    fn materials_and_themes_are_independent_choices() {
        // only the optical material takes the atmosphere
        assert_eq!(
            MATERIALS
                .iter()
                .filter(|m| m.uses_atmosphere)
                .map(|m| m.key)
                .collect::<Vec<_>>(),
            ["aqua-glass"]
        );
        // every theme can be combined with every material: the registry has no theme field,
        // and holo-glass is both a theme and a material, looked up independently
        assert!(crate::theme("holo-glass").is_some() && material("holo-glass").is_some());
        assert!(crate::theme("aqua-glass").is_none());
        let vars = Atmosphere::DEFAULT.css_vars();
        assert!(vars.split(';').all(|v| v.starts_with("--atmo-")), "{vars}");
    }

    #[test]
    fn the_atmosphere_round_trips_through_its_key() {
        let a = Atmosphere::parse("212.80.20.30.50.40.10").unwrap();
        assert_eq!(a, Atmosphere::new(212, 80, 20, 30, 50, 40, 10));
        assert_eq!(Atmosphere::parse(&a.key()), Some(a));
        assert_eq!(
            Atmosphere::parse(&Atmosphere::DEFAULT.key()),
            Some(Atmosphere::DEFAULT)
        );
    }

    #[test]
    fn out_of_range_values_are_clamped_and_glare_stays_restrained() {
        let c = Atmosphere::parse("999.200.150.100.99.100.100").unwrap();
        assert_eq!(
            (c.hue, c.sat, c.strength, c.glare),
            (360, 100, 100, Atmosphere::GLARE_MAX)
        );
        assert_eq!(
            Atmosphere::parse("1.2.3.4.0.6.7").unwrap().glare,
            Atmosphere::GLARE_MIN
        );
        assert_eq!(
            Atmosphere::new(0, 0, 0, 0, 0, 0, 0).glare,
            Atmosphere::GLARE_MIN
        );
        // a value already in range is untouched
        assert_eq!(Atmosphere::DEFAULT.clamped(), Atmosphere::DEFAULT);
    }

    #[test]
    fn anything_but_seven_whole_numbers_is_refused() {
        for bad in [
            "",
            "1.2.3.4.5.6",
            "1.2.3.4.5.6.7.8",
            "a.2.3.4.5.6.7",
            "1..3.4.5.6.7",
            "-1.2.3.4.5.6.7",
            "1.2.3.4.5.6.70000",
        ] {
            assert_eq!(Atmosphere::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn presets_set_only_the_light_colour() {
        let base = Atmosphere::DEFAULT;
        for a in &AMBIENTS {
            let x = base.with_ambient(a);
            assert_eq!(x.ambient().map(|p| p.key), Some(a.key));
            assert!(
                x.same_except_colour(base),
                "{} changed more than the colour",
                a.key
            );
        }
        assert_eq!(Atmosphere::DEFAULT.ambient().map(|p| p.key), Some("ruby"));
        assert_eq!(Atmosphere::new(100, 50, 53, 85, 53, 45, 55).ambient(), None);
        // and a change of strength is not a colour-only change
        assert!(!base.same_except_colour(Atmosphere {
            strength: 0,
            ..base
        }));
    }

    #[test]
    fn css_vars_are_numbers_only() {
        let v = Atmosphere::DEFAULT.css_vars();
        assert!(
            v.contains("--atmo-hue:352") && v.contains("--atmo-strength:0.53"),
            "{v}"
        );
        assert!(
            v.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-:;.".contains(c)),
            "{v}"
        );
        assert_eq!(v.split(';').count(), 7);
    }
}
