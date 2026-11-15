---
name: feature-pipeline
description: Pflicht-Ablauf für jedes neue Feature und jede größere Änderung in HerdCtrl. Fable konzipiert, Opus validiert, Sonnet baut parallel in Wellen, Opus prüft. Laden, sobald etwas gebaut, erweitert oder umgebaut werden soll (nicht für Ein-Zeilen-Fixes).
---

# Feature-Pipeline für HerdCtrl

Grundlage sind `AGENTS.md` und `PRD.md`; beide gehen diesem Ablauf vor. Zwischen den Stufen fragst du nicht nach Freigabe. Entscheidungen hältst du als `Ruling: <was> — <warum> — <Kosten, falls falsch>` im Ledger fest und nennst sie am Ende. Nur vier Dinge halten an:

- unumkehrbare oder destruktive Schritte
- Sicherheitsfragen
- Seiteneffekte auf dem echten Desktop des Benutzers (Fenster bewegen, Hotkeys belegen, Erweiterung neu laden)
- ein Plan, bei dem jeder Weg nur geraten wäre

Weicht ein Feature vom PRD ab, frag nach, bevor du es baust (`AGENTS.md`).

## Agenten (feste Modelle)

| Agent | Modell | Aufgabe |
|---|---|---|
| `konzept` | Fable | Analyse und Empfehlung |
| `pruefer` | Opus | Spec validieren, Review, Nachprüfung |
| `umsetzer` | Sonnet | eine Task auf zugewiesenen Dateien |

Beim Agent-Aufruf immer `subagent_type` auf einen dieser drei setzen. Ein anderes Modell ist nur erlaubt, wenn der Benutzer es ausdrücklich verlangt.

## Ablauf

1. **Branch:** `feature/<thema>` oder `fix/<thema>` vom aktuellen Stand von `staging`, bei gestapelter Arbeit vom vorigen offenen Branch. Für jedes Feature legst du ein Ledger an: `.superpowers/sdd/<datum>-<thema>/progress.md`.
2. **Konzept** (`konzept`): Recherche und Empfehlung nach `.superpowers/sdd/<thema>-research.md`.
3. **Spec und Plan** (du):
   - Lege den Umfang fest (erste Stufe gegenüber Extras) und schreibe `docs/superpowers/specs/<datum>-<thema>-design.md`.
   - Ist das Feature größer, kommt `docs/superpowers/plans/<datum>-<thema>.md` mit Wellen dazu.
   - Nenne die betroffenen PRD-Abschnitte. Commit: `Design <thema>`.
4. **Validierung** (`pruefer`): erzeugt `…-validation.md` mit Vertrag (Rust-Signaturen und Typen), Blockern, Korrekturen und Wellen. Übernimm sie bindend und committe sie.
5. **Umsetzung** (`umsetzer`, so viele parallel wie möglich):
   - Je Welle getrennte Dateien. Jeder Auftrag nennt die Spec-Pflichtpunkte, die eigenen Dateien, „kein Commit“ und eine Report-Datei.
   - **Prüfung nach jeder Welle:** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` und `cargo test --locked` müssen grün sein. Danach committest du pro Task.
   - Bedenken der Umsetzer kommen als Minor ins Ledger.
6. **Doku** im selben Branch:
   - Abschnitt in `docs/handbuch.md` **und** `docs/manual.md`
   - Eintrag unter *Unveröffentlicht* in `CHANGELOG.md` und `CHANGELOG.en.md`
   - Feature-Liste der englischen `README.md` von Hand anpassen, ein Punkt je Feature
   - Ändert sich eine Fläche aus der Galerie, die Bilder mit `cargo test --locked render_docs_screenshots -- --ignored` neu erzeugen, jedes ansehen und mitcommitten (Skill `run-app`)
   - Messwerte und Abnahmen mit Datum in `VALIDATION.md`
   - **Keine Versionsnummer erhöhen**, außer auf ausdrücklichen Auftrag
7. **Review** (`pruefer`) des ganzen Branches.
8. **Fix-Welle** (`umsetzer`, ein Agent oder zwei auf getrennten Dateien). Danach eine **Nachprüfung** (`pruefer`). Es gibt keine zweite Fix-Welle; Restbefunde gehen an den Benutzer.
9. **Abschluss:**
   - `cargo build --release --locked`, dann Branch prüfen und pushen.
   - PR mit `gh pr create --base staging` öffnen. Ist der vorige Feature-Branch noch offen, ist er die Basis (gestapelt).
   - Nie auf `main` oder `staging` pushen und nie selbst mergen. Merges übernimmt der Benutzer.
   - Zum Schluss eine Zusammenfassung für den Benutzer: alle Rulings, die betroffenen PRD-Anforderungen mit ihrer Prüfung und die offenen Punkte für den manuellen Test auf dem Desktop.

## Regeln aus Erfahrung

- **Keine blockierenden Aufrufe im UI-Thread.** Prozesse, D-Bus und Polling laufen auf Workern. Der Editor fragt nur Kanäle ab.
- **Nie den echten Desktop als Testfeld nutzen.** Kein Start einer View mit echten Anwendungen, keine Prozesse des Benutzers beenden, kein `~/.config/herdctrl` anfassen. Isoliert mit `XDG_CONFIG_HOME` (Skill `run-app`).
- **Erst die Ursache belegen, dann fixen.** Eine Vermutung ohne Messung führte bei der X11-CI zu zwei falschen Fixes, die wieder zurückgenommen werden mussten. Ein Test, der noch nie lief, kann selbst falsch messen.
- **Messen statt behaupten.** RAM, Startzeit und Größe nur mit Methode und Datum nennen.
- **Agenten löschen nichts außerhalb ihrer Task,** auch keine unversionierten Dateien.
- **Commit-Format** wie im Repository: kurzer Satz im Imperativ (`Add …`, `Fix …`), darunter Was und Warum.
