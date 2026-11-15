# HerdCtrl – Designsystem „Werkbank“

Verbindliche Regeln für jede Oberfläche des Editors. Quelle der Werte ist `src/ui/theme.rs`; diese Datei beschreibt, wofür sie gedacht sind. Gewählt wurde Richtung A „Werkbank“ aus den Entwürfen vom 2026-11-09: ruhige, deckende Flächen, ein einziger Akzent und die Monitore als Werkstück in der Mitte.

## Grundsätze

1. **Die Arbeitsfläche ist der Inhalt.** Leisten und Panels treten zurück; Farbe erscheint nur dort, wo sie etwas bedeutet (Auswahl, Zustand, Hauptaktion).
2. **Ein Akzent.** Grün kennzeichnet die Hauptaktion, die Auswahl und Erfolg. Pro Bereich gibt es höchstens einen primären Knopf.
3. **Zustand immer als Text plus Farbe.** Status-Pills, Zonen-Badges und Launch-Chips tragen ein Wort (*placed*, *failed*), nie nur einen Punkt.
4. **Keine Transparenz für Flächen.** Paneele und Popups sind deckend, damit Text auf jedem Hintergrund lesbar bleibt.
5. **Keine Animationen.** egui zeichnet bei Bedarf neu; Bewegung kommt nur aus Ziehen, Einrasten und Fortschritt.

## Farben

Jede Farbe kommt aus `Palette` (`theme::palette(ctx)`), nie als `Color32::from_rgb` in einer Komponente. Fehlt eine Rolle, kommt sie zuerst in `Palette` (dunkel und hell) und erst dann in die Komponente.

| Rolle | Dunkel | Hell | Verwendung |
|---|---|---|---|
| `bg` | `#111315` | `#F3F4F6` | Fensterhintergrund hinter der Arbeitsfläche |
| `panel` | `#16191C` | `#FFFFFF` | Kopf, Seitenleiste, Eigenschaften, Popups |
| `raised` | `#1E2226` | `#F5F6F8` | Eingabefelder, Knöpfe, Karten |
| `hover` | `#252A2F` | `#EBEDF0` | Hover und gedrückte Knöpfe |
| `border` | `#2A2F35` | `#DCDFE3` | Trennlinien, Rahmen |
| `text` | `#E6E8EA` | `#16191C` | Fließtext |
| `muted` | `#9AA3AD` | `#59616A` | Beschriftungen, Nebeninfos |
| `accent` | `#3FB89A` | `#1A7F67` | Hauptaktion, Auswahl, Links |
| `accent_on` | `#0B1F1A` | `#FFFFFF` | Text auf `accent` |
| `accent_soft` | `#1C302E` | `#E6F2EF` | Fläche der gewählten Zone, Auswahl |
| `ok` | `#3FB89A` | `#1A7F67` | *placed*, verbunden |
| `warn` / `warn_soft` | `#F0A35E` / `#352C25` | `#A35A12` / `#F6EFE7` | Ungespeichert, Tray aus |
| `danger` / `danger_soft` | `#F08A6C` / `#352927` | `#B4432A` / `#F9EFEC` | *failed*, Löschen |
| `monitor` / `monitor_edge` | `#1A1E22` / `#3A4149` | `#FFFFFF` / `#B9BFC6` | Monitor-Vorschau |
| `hatch` | `#34393F` | `#CFD3D8` | Reservierte Leisten und Docks |
| `zone` / `zone_edge` | `#24282C` / `#4A525B` | `#F4F5F7` / `#A3AAB2` | Zonen auf der Arbeitsfläche |

Kontraste (nachgerechnet 2026-11-10): `text` auf `bg`, `panel` und `raised` 13:1 bis 17,6:1, `muted` 5,7:1 bis 7,3:1, `accent_on` auf `accent` 7,0:1 (dunkel) und 4,9:1 (hell). Alles erfüllt WCAG AA (4,5:1). Neue Farbpaare müssen das ebenfalls einhalten.

## Schrift

| Stil | Schrift | Größe |
|---|---|---|
| `Heading` | Inter SemiBold | 20 |
| `Body`, `Button` | Inter Regular | 13 |
| `Small` | Inter Regular | 11 |
| Abschnittsbeschriftung (`caps`) | Inter SemiBold, Großbuchstaben, +0,6 Laufweite | 11 |
| `Monospace` (Befehle, Pixel, Anschlüsse, Tasten) | JetBrains Mono | 12 |

`theme::strong()` setzt Inter SemiBold für Namen und Hauptknöpfe. Die Schriften sind eingebettet (`assets/fonts`, SIL OFL 1.1); egui-eigene Schriften bleiben als Rückfall für Symbole wie ▶.

## Maße

- Abstand zwischen Elementen 8, Knopf-Innenabstand 12 × 6.
- Höhe: Eingaben und Knöpfe 30, primäre Knöpfe 32, Ghost-Knöpfe 28.
- Radien: Widgets 6, Menüs 8, Fenster und Dialoge 10.
- Schatten nur für Fenster und Popups: Versatz 8, Unschärfe 24.

## Bausteine (`theme.rs`)

| Funktion | Zweck |
|---|---|
| `primary` / `primary_enabled` | Die eine Hauptaktion eines Bereichs (**Launch view**, **Create view**) |
| `button` | Normale Aktion mit Standardhöhe |
| `ghost` | Textaktion in Akzentfarbe (**Change**, **Show log**) |
| `caps` | Abschnittsbeschriftung |
| `kbd` | Tastenkappe für Hotkeys |
| `pill` + `Tone` | Statusanzeige im Kopf mit Punkt und Text |
| `dot` | Kleiner Statuspunkt |
| `hatch` | Schraffur für reservierte Bereiche |
| `logo`, `appearance_icon` | Gezeichnetes HerdCtrl-Zeichen und Darstellungsumschalter |

Neue Oberflächen setzen sich aus diesen Bausteinen zusammen. Ein neuer Baustein kommt nach `theme.rs`, nicht in die Komponente.

## Prüfen

Sichtbare Änderungen prüfst du mit `cargo test --locked render_ui_snapshots -- --ignored` in beiden Themen (`target/ui-snapshots/`). Ändert sich eine Fläche aus der README-Galerie, erzeugst du die Bilder mit `render_docs_screenshots` neu (siehe [images/README.md](images/README.md)).
