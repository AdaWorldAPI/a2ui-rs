# UX-Zonen und Budgets — der Style- und Onboarding-Guide

> **Für wen:** jede App, die auf a2ui-rs baut, und jede Session, die in einer
> davon ein Template anfasst. Einmal lesen, dann anwenden.
>
> **Warum es das gibt** (Operator, 2026-08-14): *feste UX-Anordnung und
> Budgets, damit sich jedes Template gleich verhält, weil die Templates nur
> noch die Zone ansprechen und sich an Style-Guides halten, die ggf. von
> CodeRabbit enforced werden* — plus das **Moca-Prinzip**: die Adresse
> richtet die Fläche, nie das Template.
>
> **Der Code dazu ist `a2ui-layout`.** Die Zahlen unten stehen nicht nur
> hier: sie sind Konstanten mit Tests, und ein Test hält das mitgelieferte
> Stylesheet dagegen. Eine Zahl in diesem Dokument, die nicht in
> `a2ui_layout` steht, ist ein Fehler in diesem Dokument.

## 1. Die sechs Zonen

`a2ui_layout::Region` — Adressen, keine Style-Hooks:

| Zone | Marker | was hineingehört |
|---|---|---|
| `TopBar` | `region-top_bar` | App-Identität, globale Aktionen, Status |
| `ContextBand` | `region-patient_context` | was gerade in Bearbeitung ist; leer, wenn nichts |
| `LeftNav` | `region-left_nav` | globale Navigation **oder** der Selektor der Liste, die die Mitte füllt |
| `Center` | `region-center` | die Arbeit |
| `RightPanel` | `region-right_panel` | Lesestreifen; im Wide-Modus ein Schnellmenü |
| `BottomBar` | `region-bottom_bar` | Status, nie Navigation |

Die CSS-Klassen sind **absichtlich leer**. Wer sie gestaltet, koppelt Inhalt
an Anordnung und nimmt der Shell die Freiheit, Modi zu schalten.

## 2. Die zwei Modi

`a2ui_layout::LayoutMode` ist ein Feld am Layout-Frame der **Route**. Ein
Template kann Zonen füllen; es kann die Anordnung nicht wählen.

| Modus | Raster | rechte Zone |
|---|---|---|
| `Standard` (Default) | `210 \| 1fr \| 270` | Karten-Rail |
| `MainWide` | `210 \| 1fr \| 70` | Schnellmenü (Chips) |

Was ein Modus nicht zeigt, wird **nicht gerendert** — server-seitig abwesend,
nicht client-seitig versteckt. Was nie im Markup steht, kann nicht aufblitzen
und verschiebt keine Budgets.

## 3. Die Budgets

`a2ui_layout::Budget::for_viewport(vw, mode)` — vor Padding und Gap:

| Viewport | Modus | links | Mitte | rechts | je Pane |
|---|---|---|---|---|---|
| 1920 | `Standard` | 210 | **1440** | 270 | — |
| 1920 | `MainWide` | 210 | **1640** | 70 | **820** |
| 2440 | `Standard` | 210 | **1960** | 270 | — |
| 2440 | `MainWide` | 210 | **2160** | 70 | **1080** |

Die Wide-Zeilen gehen **restlos** auf: `210 + 2×820 + 70 = 1920`,
`210 + 2×1080 + 70 = 2440`. Ein Test hält beides fest — inklusive der
Bedingung, dass die Zonen den Viewport exakt decken, kein Pixel unbenannt.

Zwei **gleiche** Hälften sind der Punkt, nicht ein Detail: ein frei
proportionales Paar wäre wieder eine Fläche, die sich ihre Geometrie selbst
ausdenkt.

## 4. Die Regeln

Nummeriert, damit ein Review-Gate sie halten kann statt sie zu meinen.

1. **Ein Template erweitert die Shell.** Standalone-Vollseiten-Dokumente sind
   die Ausnahme und tragen ihre Begründung als Kommentar in den ersten
   20 Zeilen.
2. **Kein `position: fixed` in Templates.** Overlays über einer Canvas sind
   erlaubt, aber innerhalb der Zonen-Box (`absolute` im `relative`-Container),
   nie am Viewport.
3. **Keine Farbe außerhalb der Tokens.** Ein Hex-Literal im `<style>`-Block
   eines Templates ist greppbar ein Verstoß.
4. **Keine Geometrie-Erfindung.** Eine Canvas oder ein Pane **misst** seine
   Zelle (`getBoundingClientRect` + `ResizeObserver`) und nimmt nie den
   Viewport an.
5. **Der Modus kommt aus der Route.** Ein Template, das die Modus-Klasse
   selbst setzt oder eine Zone per CSS versteckt, ist ein Verstoß.
6. **Ein WebGL-Kontext pro schwerem Renderer und Sitzung.** Browser halten
   etwa sechzehn; ein Mesh oder Feld, das pro Panel neu gemountet wird, zahlt
   Decode und Kontext mehrfach. Solche Renderer leben einmal im Elternbaum und
   werden in die Zone **adoptiert**.
7. **Server-seitig abwesend schlägt client-seitig versteckt.**

## 5. Onboarding — eine neue App in vier Schritten

1. **`a2ui-layout` einbinden** und `ZONES_CSS` ausliefern (oder in das
   App-Stylesheet kopieren; der Drift-Test schützt nur die Crate-Kopie).
2. **Shell-Template schreiben**, das die sechs Marker setzt und pro Zone
   einen Block anbietet. Genau *ein* Shell-Template pro App.
3. **Modus am Frame führen.** Die Route entscheidet; das Template liest.
4. **Seiten füllen Blöcke.** Keine Seite setzt Spalten, Farben oder feste
   Positionen.

Ab da ist jede neue Kombination eine **Route**, kein Template. Das ist der
Grund, warum die Template-Zahl klein bleibt statt mit den Kombinationen zu
wachsen — eine Seite, die drei Inhalte in eigener Geometrie stapelt, wird zu
einer Route mit `MainWide` und zwei Zonen-Inhalten, die es schon gibt.

## 6. Was hier NICHT hingehört

Farben, Schriften, Komponenten-Aussehen. Diese Schicht kennt Geometrie und
Vokabular. Ein App-Theme setzt Tokens; die Zonen bleiben dieselben.
