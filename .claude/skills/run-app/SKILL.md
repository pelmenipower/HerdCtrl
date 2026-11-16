---
name: run-app
description: HerdCtrl (Rust/egui) bauen, starten, bedienen und Screenshots machen – ohne die echten Layouts, Fenster oder Hotkeys des Benutzers anzufassen. Laden, wenn die App laufen soll, eine Änderung in der echten Oberfläche geprüft, Speicher/Startzeit gemessen oder Screenshots für README, Handbuch oder Review erzeugt werden sollen.
---

# HerdCtrl starten und bedienen

HerdCtrl verschiebt echte Fenster und hält globale Hotkeys. Deshalb gilt: **erst simulieren, dann isoliert, und auf dem echten Desktop nur mit Rückfrage.**

## Toolchain

Ist `cargo` nicht im `PATH`, liegt die Toolchain dieses Projekts oft unter `/tmp`:

```sh
export RUSTUP_HOME=/tmp/herdctrl-rustup CARGO_HOME=/tmp/herdctrl-cargo \
  PATH=/tmp/herdctrl-rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH
```

Immer mit `--locked`. Ändert sich `Cargo.lock`, vorher `cargo fetch`.

## Stufe 1: Oberfläche ohne Fenster (Standard)

Für fast jede sichtbare Änderung reicht `egui_kittest`. Es zeichnet `Dashboard::ui` mit festen Beispieldaten (vier Monitore, zwei Views, sechs Anwendungen) und ohne Hintergrund-Worker. Es öffnet kein Fenster und bewegt kein Fenster.

```sh
# Alle Hauptzustände, dunkel und hell -> target/ui-snapshots/*.png
cargo test --locked render_ui_snapshots -- --ignored
# README-/Handbuch-Galerie -> docs/images/screenshot-*.png (überschreibt!)
cargo test --locked render_docs_screenshots -- --ignored
```

- Bilder **immer mit dem Read-Tool ansehen**, bevor du etwas behauptest oder committest. Die Tests vergleichen nichts.
- Zustände und Beispieldaten stehen in `src/ui/dashboard/snapshots.rs` (`dashboard()`, `launching`, `failed`, `picker`, `recording`, `first_run`). Ein neuer Zustand ist eine neue Funktion und eine `render(...)`-Zeile.
- Beispieldaten dürfen nichts vom echten Rechner lesen. Werte wie `autostart`, `tray_running` und Monitore setzt die Testfunktion fest.
- Bedienabläufe mit simulierter Maus und Tastatur stehen in `src/ui/dashboard/interactions.rs` (Harness aus `snapshots::harness`, Knöpfe über `harness.get_by_label(...)`). Sie laufen bei `cargo test` mit und brauchen keine GPU. Speichern in Tests startet keinen Tray (`cfg!(test)`).

## Stufe 2: Echtes Fenster, isoliert

Alle Dateien von HerdCtrl hängen an `XDG_CONFIG_HOME`: `layouts.json`, `ui.json`, der Autostart-Eintrag und auch die Sperren für Editor und Tray (Hash des Layout-Pfads). Mit einem eigenen Verzeichnis bleibt alles vom Benutzer getrennt:

```sh
cargo build --release --locked
export RUN=$(mktemp -d)
XDG_CONFIG_HOME=$RUN ./target/release/herdctrl &
```

Wichtig:

- Isolierte Instanzen immer mit `HERDCTRL_NO_UPDATE_CHECK=1` starten (`XDG_CONFIG_HOME=$RUN HERDCTRL_NO_UPDATE_CHECK=1 ./target/release/herdctrl &`). Sonst fragt der Tray von sich aus bei github.com nach Updates und zeigt womöglich eine Update-Pille oder Benachrichtigung.
- Der Editor startet beim Öffnen **einen eigenen Tray** (`herdctrl --tray`) für diese Konfiguration. Beende beim Aufräumen genau die Prozesse, die du gestartet hast. Den Tray findest du, indem du bei jedem `herdctrl --tray` in `/proc/<pid>/environ` nach `XDG_CONFIG_HOME=$RUN` suchst. Fremde HerdCtrl-Prozesse des Benutzers nie beenden.
- Unter GNOME besitzt immer nur eine Instanz die Kürzel. Der echte Tray des Benutzers behält sie, die Erweiterung lehnt einen zweiten Besitzer ab. Der isolierte Tray kann dann eine Konflikt-Benachrichtigung zeigen; das ist erwartet.
- **Keine View starten, die echte Anwendungen betrifft.** Ein Start verschiebt vorhandene Fenster des Benutzers (Browser, Terminals). Zum Testen nur Zonen mit eigenen Befehlen für Wegwerf-Fenster verwenden, zum Beispiel `xterm -T herdctrl-test -e sleep 30`, und nur nach Rückfrage auf dem echten Desktop.
- Screenshots vom echten Desktop zeigen private Fenster. Unter Wayland gehen sie nur über das Screenshot-Portal mit Bestätigung. Für Doku-Bilder immer Stufe 1 nehmen.

### Ohne echten Desktop: Xvfb und Openbox

So läuft die CI (`.github/workflows/ci.yml`). Lokal brauchst du `xvfb`, `openbox`, `xterm`, `xdotool` und `x11-utils`:

```sh
Xvfb :99 -screen 0 1920x1080x24 & export DISPLAY=:99; openbox &
cargo test --locked x11_ -- --ignored --test-threads=1 --nocapture
python3 scripts/check-x11.py --runs 3 --output target/x11-acceptance.json
```

`check-x11.py` nutzt eine eigene Konfiguration, startet und beendet seine Instanzen selbst und drückt `Ctrl+Alt+Shift+F10`. Für Fenstergeometrie unter Openbox `xwininfo` verwenden, nicht `wmctrl -lG`: Letzteres zählt die Titelleiste doppelt.

## Stufe 3: Echter GNOME-Desktop (nur mit Freigabe)

Die Live-Tests öffnen eigene, rahmenlose GTK-Testfenster auf jedem Monitor und räumen sie wieder auf. Gespeicherte Layouts bleiben unverändert, aber Fenster erscheinen sichtbar auf dem Desktop des Benutzers. Vorher fragen:

```sh
HERDCTRL_LIVE_GNOME=1 cargo test --locked gnome_live_launch_and_resize_on_each_monitor -- --ignored --nocapture --test-threads=1
cargo build --release --locked
HERDCTRL_LIVE_GNOME=1 cargo test --locked gnome_live_identification_positions_and_cleans_up_labels -- --ignored --nocapture --test-threads=1
python3 scripts/check-gnome.py   # nur lesen
```

Änderungen an der Shell-Erweiterung wirken erst nach `bash scripts/install-gnome-extension.sh` **und** Ab- und Anmelden. Das kann nur der Benutzer selbst tun. Sag es ihm, statt es zu umgehen.

## Messen

```sh
cargo build --release --locked
scripts/measure-rss.sh 5            # Editor: Zeit bis sichtbar, VmRSS, RssAnon (Median)
grep -E 'VmRSS|RssAnon' /proc/<tray-pid>/status
stat -c %s target/release/herdctrl
```

`measure-rss.sh` nutzt je Start ein Wegwerf-`XDG_CONFIG_HOME` und wartet auf das Fenster mit der eigenen `_NET_WM_PID`. Es misst die Sichtbarkeit des Fensters, nicht den ersten gezeichneten Frame. Ergebnisse mit Datum, Rechner und Methode in `VALIDATION.md` eintragen. Ungemessenes gilt nicht als erfüllt (siehe `AGENTS.md`).
