//! **a2ui-layout** — die geteilte Zonen- und Budget-Schicht.
//!
//! # Warum das ein Crate ist und kein Stylesheet
//!
//! Operator, 2026-08-14: *„das sollte global in a2ui-rs sein, reusable for
//! all future apps including style and onboarding guide"* — und davor die
//! Rechnung, die diese Schicht trägt:
//!
//! ```text
//! 1920 = 210 + 2×820 + 70
//! 2440 = 210 + 2×1080 + 70
//! ```
//!
//! Solche Zahlen in einem Dokument veralten still: jemand ändert eine
//! Spaltenbreite im CSS, und die Prosa daneben ist ab dann falsch, ohne dass
//! irgendetwas rot wird. Hier sind sie **eine Funktion mit Tests** — wer die
//! Breite ändert, ändert eine Konstante, und die Tests sagen sofort, welche
//! Behauptung damit gebrochen ist.
//!
//! Das Crate hat **keine Abhängigkeiten**: es ist Arithmetik plus ein Enum.
//! Damit können ein Server-Crate, ein wasm-Client und ein Build-Skript sich
//! auf dieselben Zahlen einigen, ohne dass eines davon das andere zieht.
//!
//! # Das Moca-Prinzip: die Adresse richtet die Fläche
//!
//! Der Modus ist eine Eigenschaft der **Route**, nie des Templates. Ein
//! Template kann Zonen füllen; es kann die Anordnung nicht wählen. Das ist
//! der Unterschied zwischen „jede Seite verhält sich gleich" und „jede Seite
//! erfindet sich neu" — und es ist der Grund, warum eine App mit dieser
//! Schicht mit einer Handvoll Templates auskommt statt mit einem Template pro
//! Kombination: die Kombinatorik wandert aus den Templates in die Routen.
//!
//! # Was hier NICHT hingehört
//!
//! Farben, Schriften, Komponenten-Aussehen. Diese Schicht kennt **Geometrie
//! und Vokabular**, nicht Gestaltung. Ein App-Theme setzt Tokens; die Zonen
//! bleiben dieselben.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Die sechs benannten Zonen. **Adressen, keine Style-Hooks.**
///
/// Ein Template spricht eine Zone an; es gestaltet sie nicht. Deshalb sind
/// die CSS-Klassen im Stylesheet ausdrücklich leer (*structural marker; do
/// not style*): wer sie als Hook missbraucht, koppelt Inhalt an Anordnung und
/// nimmt der Shell genau die Freiheit, die sie haben muss, um Modi zu
/// schalten.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Region {
    /// Kopfleiste über allem: App-Identität, globale Aktionen, Status-Badge.
    TopBar,
    /// Das Kontext-Band unter der Kopfleiste: was gerade in Bearbeitung ist.
    /// Leer, wenn nichts in Bearbeitung ist — nie tote Beschriftung.
    ContextBand,
    /// Linkes Menü: globale Navigation ODER der Selektor der Liste, die die
    /// Mitte füllt. Der Selektor gehört neben das Selektierte.
    LeftNav,
    /// Die Mitte. Die Arbeit.
    Center,
    /// Rechte Rail: Lesestreifen, Karten. In [`LayoutMode::MainWide`] ein
    /// schmales Schnellmenü statt einer Kartenspalte.
    RightPanel,
    /// Fußleiste: Status, nie Navigation.
    BottomBar,
}

impl Region {
    /// Der Struktur-Marker im Markup (`region-*`).
    #[must_use]
    pub const fn marker(self) -> &'static str {
        match self {
            Region::TopBar => "region-top_bar",
            Region::ContextBand => "region-patient_context",
            Region::LeftNav => "region-left_nav",
            Region::Center => "region-center",
            Region::RightPanel => "region-right_panel",
            Region::BottomBar => "region-bottom_bar",
        }
    }

    /// Alle sechs, in Lese-Reihenfolge.
    #[must_use]
    pub const fn all() -> [Region; 6] {
        [
            Region::TopBar,
            Region::ContextBand,
            Region::LeftNav,
            Region::Center,
            Region::RightPanel,
            Region::BottomBar,
        ]
    }
}

/// Breite des linken Menüs, in CSS-Pixeln. Parity-locked.
pub const LEFT_NAV_PX: u32 = 210;
/// Breite der rechten Rail im Standard-Modus.
pub const RIGHT_PANEL_PX: u32 = 270;
/// Breite der rechten Rail in [`LayoutMode::MainWide`] — das Schnellmenü.
///
/// 70px ist Operator-Vorgabe und keine Ableitung: sie ist genau das, was
/// übrig bleibt, wenn zwei glatte Panes (820 bzw. 1080) aus der Restbreite
/// geschnitten sind. Die Tests halten beide Auflösungen fest.
pub const RIGHT_SLIM_PX: u32 = 70;

/// Welche Zonen-Anordnung eine Route verlangt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutMode {
    /// `LeftNav | Center | RightPanel` — die Standard-Anordnung.
    #[default]
    Standard,
    /// `LeftNav | Center | schmale Rail` — die Mitte trägt zwei halbe
    /// Budgets ([`Budget::pane`]), die Rail wird zum Schnellmenü.
    MainWide,
}

impl LayoutMode {
    /// Die Klasse, die die Shell auf ihren Raster-Container schreibt. Leer
    /// für den Standard, damit bestehende Seiten unverändert bleiben.
    #[must_use]
    pub const fn class(self) -> &'static str {
        match self {
            LayoutMode::Standard => "",
            LayoutMode::MainWide => "mode-main-wide",
        }
    }

    /// Breite der rechten Zone in diesem Modus.
    #[must_use]
    pub const fn right_px(self) -> u32 {
        match self {
            LayoutMode::Standard => RIGHT_PANEL_PX,
            LayoutMode::MainWide => RIGHT_SLIM_PX,
        }
    }
}

/// Die ausgerechneten Breiten für einen Viewport und einen Modus.
///
/// **Vor Padding und Gap.** Was eine Zone am Ende innen frei hat, hängt vom
/// Theme ab; was ihr die Shell zuteilt, steht hier. Die Trennung ist Absicht:
/// ein Theme darf Innenabstände ändern, ohne dass die Zuteilung wandert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Viewport-Breite, aus der gerechnet wurde.
    pub viewport: u32,
    /// Linkes Menü.
    pub left: u32,
    /// Mitte.
    pub center: u32,
    /// Rechte Zone (Rail oder Schnellmenü).
    pub right: u32,
}

impl Budget {
    /// Rechne die Zuteilung für einen Viewport aus.
    ///
    /// **Sättigend, nicht panisch:** ein Viewport, der schmaler ist als das
    /// Chrome, liefert eine Mitte von `0` statt eines Unterlaufs. Ein Layout,
    /// das bei 320px paniert, ist schlechter als eines, das ehrlich sagt „für
    /// die Mitte bleibt nichts" — die Entscheidung, was dann passiert (ein
    /// Umbruch-Breakpoint), gehört ins Theme, nicht in die Arithmetik.
    #[must_use]
    pub const fn for_viewport(viewport: u32, mode: LayoutMode) -> Self {
        let right = mode.right_px();
        let chrome = LEFT_NAV_PX + right;
        Budget {
            viewport,
            left: LEFT_NAV_PX,
            center: viewport.saturating_sub(chrome),
            right,
        }
    }

    /// Die Breite EINES Panes, wenn die Mitte zwei gleiche trägt.
    ///
    /// Zwei gleiche Hälften sind der Punkt, nicht ein Detail: ein frei
    /// proportionales Paar wäre wieder eine Fläche, die sich ihre Geometrie
    /// selbst ausdenkt — genau das, was feste Budgets verhindern.
    ///
    /// Ungerade Mitten runden ab; ein Rest-Pixel bleibt beim Gap, nicht in
    /// einem der Panes. Sonst wären die Hälften nicht gleich, und die
    /// Zusicherung im Namen wäre unwahr.
    #[must_use]
    pub const fn pane(self) -> u32 {
        self.center / 2
    }
}

/// Die zwei Referenz-Auflösungen, an denen die Budgets festgenagelt sind.
///
/// Sie stehen hier als Daten, damit ein Test, ein Doc-Generator und ein
/// Browser-Probe dieselbe Liste lesen — und nicht drei Stellen dieselben
/// Zahlen abschreiben.
pub const REFERENCE_VIEWPORTS: [u32; 2] = [1920, 2440];

/// Das Zonen-Stylesheet, mitgeliefert statt nachgebaut.
///
/// Eine App bindet es ein (`include_str!` über diese Konstante oder eine
/// Kopie beim Build) und bekommt exakt das Raster, das die Budgets oben
/// beschreiben. Ohne das müsste jede App die Spaltenbreiten abschreiben —
/// und die dritte Abschrift ist die, die abweicht.
pub const ZONES_CSS: &str = include_str!("../assets/zones.css");

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Operator-Rechnung, exakt: `1920 − 210 = 1710 = 2×820 + 70` und
    /// `2440 − 210 = 2230 = 2×1080 + 70`.
    ///
    /// CAN FIRE: jede Änderung an `LEFT_NAV_PX` oder `RIGHT_SLIM_PX` bricht
    /// genau diesen Test — was der Zweck ist. Die Zahlen sind eine Zusage,
    /// keine Beobachtung.
    #[test]
    fn the_wide_mode_budget_matches_the_pinned_arithmetic() {
        let b1920 = Budget::for_viewport(1920, LayoutMode::MainWide);
        assert_eq!(b1920.center, 1640, "1920 − 210 − 70");
        assert_eq!(b1920.pane(), 820, "zwei glatte Panes bei 1920");

        let b2440 = Budget::for_viewport(2440, LayoutMode::MainWide);
        assert_eq!(b2440.center, 2160, "2440 − 210 − 70");
        assert_eq!(b2440.pane(), 1080, "zwei glatte Panes bei 2440");

        // Und die Zerlegung geht ohne Rest auf — das ist die eigentliche
        // Behauptung hinter „glatt": der Viewport ist VOLLSTAENDIG verteilt,
        // es bleibt kein Pixel unbenannt.
        for b in [b1920, b2440] {
            assert_eq!(b.pane() * 2, b.center, "ein Rest-Pixel waere ungleich");
            assert_eq!(
                b.left + b.center + b.right,
                b.viewport,
                "die Zonen decken den Viewport nicht exakt"
            );
        }
    }

    /// Der Standard-Modus behält seine parity-locked Mitte.
    ///
    /// CAN STAY SILENT: der Gegentest zum obigen. Ohne ihn könnte
    /// `right_px()` konstant 70 liefern und der Wide-Test bliebe grün,
    /// während jede bestehende Seite ihre Rail verlöre.
    #[test]
    fn the_standard_mode_keeps_the_parity_locked_grid() {
        let b = Budget::for_viewport(1920, LayoutMode::Standard);
        assert_eq!((b.left, b.center, b.right), (210, 1440, 270));
        assert_eq!(
            Budget::for_viewport(2440, LayoutMode::Standard).center,
            1960
        );
    }

    /// Ein zu schmaler Viewport liefert 0, nicht einen Unterlauf.
    #[test]
    fn a_viewport_narrower_than_the_chrome_saturates_instead_of_wrapping() {
        let b = Budget::for_viewport(320, LayoutMode::Standard);
        assert_eq!(b.center, 0);
        assert_eq!(b.pane(), 0);
    }

    /// Jede Zone hat genau einen Marker, und keiner kollidiert.
    ///
    /// CAN FIRE: zwei Varianten denselben String geben lassen — dann zeigen
    /// zwei Zonen auf dieselbe Adresse, und ein Template kann nicht mehr
    /// sagen, welche es meint.
    #[test]
    fn every_region_has_its_own_marker() {
        let all = Region::all();
        for (i, a) in all.iter().enumerate() {
            assert!(a.marker().starts_with("region-"));
            for b in &all[i + 1..] {
                assert_ne!(a.marker(), b.marker(), "Marker-Kollision");
            }
        }
    }

    /// Das mitgelieferte CSS trägt DIESELBEN Zahlen wie die Konstanten.
    ///
    /// CSS liest keine Rust-Konstanten, also stehen die Breiten zwangsläufig
    /// zweimal. Dieser Test ist der Ersatz für die fehlende Kopplung: wer
    /// eine Spalte im Stylesheet ändert und die Konstante vergisst (oder
    /// umgekehrt), sieht es hier — statt später an einem Layout, das um
    /// 60 Pixel danebenliegt.
    ///
    /// CAN FIRE: eine der vier Zahlen in `assets/zones.css` verstellen.
    #[test]
    fn the_shipped_css_carries_the_same_numbers_as_the_constants() {
        let css = ZONES_CSS;
        assert!(
            css.contains(&format!(
                "{LEFT_NAV_PX}px minmax(0, 1fr) {RIGHT_PANEL_PX}px"
            )),
            "das Standard-Raster im CSS weicht von den Konstanten ab"
        );
        assert!(
            css.contains(&format!("{LEFT_NAV_PX}px minmax(0, 1fr) {RIGHT_SLIM_PX}px")),
            "das main_wide-Raster im CSS weicht von den Konstanten ab"
        );
        assert!(
            css.contains(LayoutMode::MainWide.class()),
            "das CSS kennt die Modus-Klasse nicht, die lib.rs ausgibt"
        );
        for r in Region::all() {
            assert!(
                css.contains(r.marker()),
                "Marker {} fehlt im CSS",
                r.marker()
            );
        }
    }

    /// Der Standard-Modus schreibt KEINE Klasse — bestehende Seiten dürfen
    /// von dieser Schicht nichts merken.
    #[test]
    fn the_standard_mode_writes_no_class_at_all() {
        assert_eq!(LayoutMode::default(), LayoutMode::Standard);
        assert!(LayoutMode::Standard.class().is_empty());
        assert!(!LayoutMode::MainWide.class().is_empty());
    }
}
