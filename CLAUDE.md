# HerdCtrl – Regeln für Claude

Native Linux-App in Rust (`egui`/`eframe`) mit einem Tray-Prozess (`herdctrl --tray`, `ksni`, `zbus`) und einer GNOME-Shell-Erweiterung (GJS). Der Editor zeichnet Zonen auf Monitore, der Launcher startet und verschiebt Fenster unter X11 (`wmctrl`, `xrandr`) und unter GNOME 50/Wayland (Erweiterung über D-Bus). UI-Texte auf Englisch, Code-Kommentare auf Englisch, Doku auf Deutsch und Englisch.

**`AGENTS.md` und `PRD.md` sind verbindlich und gehen allem hier vor.** Lies sie vor jeder Aufgabe.

## Arbeitsweise (verbindlich)

**Jedes neue Feature und jede größere Änderung läuft über den Skill `feature-pipeline`.** Fable konzipiert, Opus validiert die Spec, Sonnet setzt parallel in Wellen um, Opus prüft. Die Agenten mit festen Modellen liegen in `.claude/agents/` (`konzept`, `pruefer`, `umsetzer`). Zwischen den Stufen gibt es keine Rückfragen; Entscheidungen werden als Rulings festgehalten und am Ende genannt.

Ausnahme sind kleine, klar umrissene Fehlerbehebungen (eine Ursache, wenige Zeilen). Sie gehen direkt und test-first, ohne Pipeline. Erst die Ursache belegen, dann fixen.

## Git

- Nie auf `main` oder `staging` arbeiten, committen oder pushen. Eigener Branch (`feature/…`, `fix/…`), PR gegen `staging`. Merges macht der Benutzer.
- Vor jedem Commit und Push den Branch prüfen. Keine Versionsnummern, Tags oder Releases ohne ausdrücklichen Auftrag.

## Der Desktop des Benutzers ist kein Testfeld

- Keine View mit echten Anwendungen starten: Das verschiebt echte Fenster.
- `~/.config/herdctrl` und `~/.config/autostart` nicht anfassen. Zum Ausprobieren eine eigene `XDG_CONFIG_HOME` verwenden, dann sind Layouts, Autostart und die Sperren für Editor und Tray getrennt.
- Nur Prozesse beenden, die du selbst gestartet hast.
- Erweiterung neu laden heißt ab- und anmelden; das macht der Benutzer.

Wie die App sicher läuft und wie Screenshots entstehen, steht im Skill `run-app` (`.claude/skills/run-app/SKILL.md`).

## Code

- UI und Launch-Engine sind getrennt. Im UI-Thread gibt es keine blockierenden Prozess-, D-Bus- oder Sleep-Aufrufe; Worker melden über Kanäle.
- Fenster werden nie über ihren Titel zugeordnet. Vor jedem Verschieben wird die PID geprüft.
- Farben, Schrift und Maße kommen nur aus `src/ui/theme.rs`. Die Regeln dazu stehen in `docs/design-system.md`. Fehlt ein Wert, kommt er zuerst als Rolle in `Palette` (dunkel und hell).
- Wayland-Fenster nie über `wmctrl` steuern. Andere Compositoren als GNOME 50 nicht als unterstützt behandeln.

## Prüfen

- Vor jedem Commit: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` und `cargo test --locked`. Vor dem Push zusätzlich `cargo build --release --locked`.
- Sichtbare Änderungen mit `cargo test --locked render_ui_snapshots -- --ignored` rendern und die Bilder ansehen. Betrifft eine Änderung die Galerie, kommt `render_docs_screenshots` dazu.
- Laufzeitziele (Größe, Startzeit, Speicher) nur gemessen nennen, mit Methode und Datum in `VALIDATION.md`.

## Doku

Benutzer-Doku: `README.md` (englischer Kurzüberblick mit Galerie), `docs/manual.md` (Englisch) und `docs/handbuch.md` (Deutsch) mit gleichem Inhalt, `CHANGELOG.md`/`CHANGELOG.en.md` (Einträge unter *Unveröffentlicht*). Eine Funktion ist erst fertig, wenn beide Handbücher sie beschreiben.
