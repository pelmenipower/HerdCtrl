# Design: Installation und Updates

Stand 2026-11-15. Grundlage: Recherche `.superpowers/sdd/install-update-research.md` (Konzept-Agent), übernommen als Spec mit den Rulings unten. Betroffene PRD-Abschnitte: 2 (single executable bleibt), 4.4 (Tray), 4.6 (GNOME-Erweiterung, Grabs), 5 (Dateistruktur), 6 (Größe, RAM, Startzeit), neu 4.7.

Umfang: erste Stufe vollständig (Abschnitt 10). Extras nicht.

## 1. Entscheidung in Kürze

- **Ein Installer, `install.sh` in der Repo-Wurzel und als Release-Asset.** Er installiert *und* aktualisiert per Benutzer (`~/.local/bin`, `~/.local/share/applications`, `~/.local/share/icons`), prüft jeden Download gegen `SHA256SUMS`, installiert fehlende Laufzeitpakete per `sudo apt-get` (einziger Schritt mit Root), die GNOME-Erweiterung unter GNOME/Wayland, stoppt einen laufenden Tray sauber und startet ihn mit der neuen Version neu, biegt einen alten Autostart-Eintrag auf die installierte Programmdatei um und installiert sich selbst als `~/.local/bin/herdctrl-update`.
- **`update.sh` ist ein dünner Wrapper** (Repo-Wurzel und Release-Asset): ruft das installierte `herdctrl-update` auf; fehlt es, lädt es das `install.sh` des neuesten Releases, prüft dessen Prüfsumme gegen `SHA256SUMS` und führt es aus. Damit gibt es die vom Benutzer gewünschte `update.sh`, ohne zwei Implementierungen zu pflegen.
- **Update-Prüfung sitzt im Tray** (läuft dauerhaft, hat Benachrichtigungen, ist kein UI-Thread): einmal pro Tag `curl` auf `https://github.com/pelmenipower/HerdCtrl/releases/latest` und die Version aus dem Redirect-Ziel lesen – keine GitHub-API, kein JSON, kein Rate-Limit, keine neue Rust-Abhängigkeit. Ergebnis in `update.json` neben `layouts.json`; eine Benachrichtigung je neuer Version; Menüpunkt **Update to 0.2.0…** öffnet `herdctrl-update` in einem Terminal. Der Editor liest nur die Datei und zeigt eine Kopfzeilen-Pille **Update available · 0.2.0** mit **Update now**. Abschaltbar über den Tray-Haken **Check for updates daily** und `HERDCTRL_NO_UPDATE_CHECK=1`.
- **Rust-Vorarbeit, ohne die das nicht sicher geht:** `herdctrl --version` (heute startet jedes unbekannte Argument den Editor, `src/main.rs:7-21`), sauberes Beenden des Trays bei SIGTERM (heute keine Grab-Freigabe, `src/tray.rs:168-173` läuft nur bei *Quit*), stabiler Lock-Dateiname statt `DefaultHasher` (`src/tray.rs:39-41`, über Toolchain-Wechsel nicht garantiert gleich).
- **PRD-Abweichung:** Das PRD kennt weder Distribution noch Updates. Neuer Abschnitt 4.7 als *user-approved scope extension* (Formulierung unter Abschnitt 4). `install.ps1` kommt nicht: Es gibt keine Windows-Version (Ruling im Ledger).


## 4. PRD-Abweichung und Vorschlag für Abschnitt 4.7

Das PRD beschreibt Produkt und Laufzeit (Abschnitte 2–6), nicht Distribution, Installation oder Updates. Abschnitt 2.1 verlangt „single executable“ – das bleibt; der Installer legt nur Desktop-Dateien dazu. AGENTS.md verbietet beiläufiges Umschreiben, erlaubt Produktänderungen auf ausdrücklichen Auftrag; der Auftrag liegt wörtlich vor und ist im Ledger als Ruling festgehalten. Vorschlag (Englisch, wie das PRD; einfügen nach 4.6):

> ### 4.7 Installation and updates (user-approved scope extension)
>
> * Linux releases install per user with one command: `install.sh` (a Windows `install.ps1` follows once a Windows build exists). It downloads the signed release, verifies every file against the release's `SHA256SUMS` before installing anything (and the Sigstore bundle when `cosign` is available), places the executable in `~/.local/bin`, adds the desktop entry and icons, installs the GNOME Shell extension on GNOME/Wayland, and installs missing runtime packages through APT, the only step that needs sudo.
> * Running the installer again updates. It stops a running tray, installs, starts the tray with the new version, and points an existing autostart entry at the installed executable. `--check` reports the installed and latest version without installing; `--uninstall` removes the installation and keeps the configuration unless asked otherwise. The installer never executes a previously installed executable to learn its version.
> * The tray checks for a newer release at most once a day with a single HTTPS request to the GitHub releases page (no account, no telemetry beyond the IP address and curl's user agent), notifies once per new version, and offers the update in its menu; the editor shows it in the header. The check can be turned off. The application never downloads or replaces itself: the installer, started in a terminal on request, is the only update path.
> * `herdctrl --version` prints the version and unknown arguments are rejected instead of starting the editor. The tray releases its hotkey grabs and lock on SIGTERM, so an updater can restart it without waiting for the extension's heartbeat expiry.

Außerdem in Abschnitt 5 (Dateistruktur) `install.sh`, `update.sh`, `src/cli.rs` und `src/core/update.rs` ergänzen – das ist Pflege, keine Produktänderung.


## 5. `install.sh` im Detail

**Aufruf**

```sh
curl -fsSL https://github.com/pelmenipower/HerdCtrl/releases/latest/download/install.sh | bash
curl -fsSL https://github.com/pelmenipower/HerdCtrl/releases/latest/download/install.sh | bash -s -- --version 0.2.0
bash install.sh [--version X.Y.Z | --archive ./herdctrl-vX.Y.Z-linux-x86_64.tar.gz]
                [--check] [--reinstall] [--no-packages] [--no-extension | --extension]
                [--uninstall [--purge]] [-h | --help]
```

**Regeln**

- `set -euo pipefail`, ganzes Skript in `main "$@"` am Dateiende (ein halb geladenes Skript tut nichts). Ausgaben Englisch, Fehler als `HerdCtrl: …` auf stderr, Exit 1; Nutzungsfehler Exit 2; `--check` mit verfügbarem Update Exit 10. Keine Eingaben von stdin (unter `curl | bash` ist stdin die Pipe); `sudo` fragt selbst über `/dev/tty`.
- Nur Linux x86_64; benötigt `curl`, `tar`, `sha256sum`, `mktemp`, `sort -V`, `setsid`. Kein `python3`, kein `unzip`.
- `MIN_VERSION=0.2.0`: Releases davor haben kein `--version` und kein `install.sh` im Tarball; `--version 0.1.0` wird abgelehnt („HerdCtrl 0.1.0 predates the installer“).
- Pfade: `BIN=${XDG_BIN_HOME:-$HOME/.local/bin}`, `DATA=${XDG_DATA_HOME:-$HOME/.local/share}`, `CONFIG=${XDG_CONFIG_HOME:-$HOME/.config}`. Stamp `$DATA/herdctrl/installed-version` (Inhalt `0.2.0`). Die installierte Version wird **nur** aus dem Stamp gelesen; ohne Stamp gilt „unknown (installed by hand or before 0.2.0)“.
- Download immer `curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --max-time 120`.

**Ablauf (Install/Update)**

1. Zielversion: `--version` oder Redirect-Ziel von `/releases/latest` (`curl -fsS --proto =https -o /dev/null -w '%{redirect_url}'`, Regex `/releases/tag/v([0-9]+\.[0-9]+\.[0-9]+)$`). Installierte Version aus dem Stamp.
2. `--check`: `Installed: 0.1.0 · Latest: 0.2.0 → Update available. Run: herdctrl-update` (Exit 10) oder `HerdCtrl 0.2.0 is up to date` (Exit 0). Ohne Download.
3. Gleich installiert und kein `--reinstall`/`--archive`: Pakete und Erweiterung trotzdem prüfen (idempotent), dann `HerdCtrl 0.2.0 is already installed. Nothing to do.` Exit 0.
4. Download in `mktemp -d` (Trap räumt auf): Tarball, `SHA256SUMS`, `SHA256SUMS.sigstore.json`, unter GNOME/Wayland das Erweiterungs-ZIP. Ist `cosign` vorhanden: `cosign verify-blob --bundle … --certificate-identity https://github.com/pelmenipower/HerdCtrl/.github/workflows/release.yml@refs/tags/v$version --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS` → `Signature verified (Sigstore)`; sonst eine Zeile `cosign not found: checksum only (see docs/manual.md#verify-a-download)`. Danach Prüfsumme jedes geladenen Assets wie bei Shellherd (`awk` auf genau einen Treffer, `sha256sum --check --status`). Mit `--archive` entfällt die Prüfung (lokale Datei ist vom Benutzer, wie bei Shellherd); Version aus dem Verzeichnisnamen im Archiv.
5. Entpacken mit `--strip-components=1`; Pflichtdateien `herdctrl` (ELF-Magic `\x7fELF`), `desktop/herdctrl.desktop`, `desktop/hicolor/`, `install.sh`.
6. Laufzeitpakete (nur mit `apt-get` und `dpkg-query`, sonst Liste ausgeben und weiter): `libxkbcommon-x11-0 libxcb-xkb1 libgl1 libvulkan1 mesa-vulkan-drivers wmctrl x11-xserver-utils` (aus `docs/manual.md:52-56` ohne `-dev` und Compiler). Fehlende per `sudo apt-get install -y …`; `--no-packages` überspringt; schlägt `sudo` fehl, Warnung und weiter. Anschließend `ldd herdctrl`: Zeilen mit `not found` → Fehler mit Paketliste; schlägt `ldd` selbst fehl (z. B. im Test ein Skript statt ELF), nur Hinweis.
7. Laufende Prozesse finden (nur eigene, über `/proc`): `cmdline` mit `argv[0]`-Basisname `herdctrl` und Argument `--tray`, deren `environ` denselben `XDG_CONFIG_HOME`-Wert hat (gesetzt oder nicht gesetzt). So bleiben isolierte Test-Trays (`run-app`-Skill) und Trays anderer Konfigurationen unberührt. Tray mit SIGTERM beenden, bis 5 s auf Ende warten. Läuft ein Editor (gleiches Muster ohne `--tray`/`--identify-monitors`), nicht beenden, nur merken: `The editor is open; close and reopen it to use 0.2.0.`
8. Dateien atomar einspielen: `install -D -m 755 herdctrl "$BIN/.herdctrl.new" && mv -f … "$BIN/herdctrl"`; `install.sh` → `$BIN/herdctrl-update` (755); Icons `desktop/hicolor/<size>/apps/herdctrl.png` → `$DATA/icons/hicolor/<size>/apps/`; Desktop-Eintrag aus dem Tarball nach `$DATA/applications/herdctrl.desktop` mit `Exec="$BIN/herdctrl"` und `TryExec=$BIN/herdctrl` (Quoting wie `src/tray.rs:494-503`: `"`, `` ` ``, `$`, `\` escapen; GNOME startet Desktop-Einträge nicht zwingend mit `~/.local/bin` im `PATH`). Dann `update-desktop-database -q` und `gtk-update-icon-cache -q -t -f`, falls vorhanden, Fehler ignorieren. Stamp schreiben.
9. Rauchtest: `"$BIN/herdctrl" --version` muss `herdctrl 0.2.0` liefern (sicher, weil nur ≥ 0.2.0 installiert wird); sonst Fehler mit `ldd`-Hinweis.
10. Autostart: existiert `$CONFIG/autostart/herdctrl.desktop` mit `Exec=… --tray`, aber anderem Pfad (z. B. `target/release`), `Exec` auf `"$BIN/herdctrl" --tray` umschreiben, Rest unverändert. Nie anlegen (das ist *Start at login* des Benutzers).
11. Erweiterung (nur wenn `XDG_CURRENT_DESKTOP` `GNOME` enthält, `XDG_SESSION_TYPE=wayland` und `gnome-extensions` vorhanden; `--extension` erzwingt, `--no-extension` überspringt): Hash aller Dateien in `$DATA/gnome-shell/extensions/herdctrl@herdctrl.local` vor und nach `gnome-extensions install --force <zip>` vergleichen. Unverändert → `Extension is up to date.`; geändert oder neu → `gnome-extensions enable` versuchen und `Log out and back in to load the new extension.` Shell-Version wie in `scripts/install-gnome-extension.sh:12-16` prüfen, bei Abweichung nur Hinweis, nicht abbrechen (das Programm läuft auch ohne).
12. Tray neu starten, wenn Schritt 7 einen gestoppt hat: `setsid -f "$BIN/herdctrl" --tray >/dev/null 2>&1 </dev/null`. War der gestoppte Tray **vor 0.2.0** (kein Stamp) und die Sitzung GNOME/Wayland, vorher 16 s warten mit Hinweis `Waiting for GNOME to release the previous tray's shortcuts…` (Heartbeat-Ablauf 15 s, `controller.mjs:19-22`); sonst sofort. War keiner aktiv: `Start HerdCtrl from the app menu.`
13. Zusammenfassung: Version, Pfad, App-Menü, `herdctrl-update` / `herdctrl-update --check`, Hinweis `~/.local/bin is not in your PATH yet; open a new terminal or log out and in` wenn nötig.

**Deinstallation** (`--uninstall`): Tray wie oben stoppen; `herdctrl`, `herdctrl-update`, Desktop-Eintrag, Icons, Stamp, Autostart-Eintrag und (falls installiert) `gnome-extensions uninstall herdctrl@herdctrl.local` entfernen; `$CONFIG/herdctrl` bleibt (Hinweis), mit `--purge` auch weg. Deinstallation ja, weil sie die Rückabwicklung eines Fehlschlags ist und 40 Zeilen kostet.

**`update.sh`** (≈30 Zeilen): `exec "$BIN/herdctrl-update" "$@"` wenn vorhanden; sonst Zielversion per Redirect, `install.sh` und `SHA256SUMS` des Tags laden, Prüfsumme von `install.sh` verifizieren, `exec bash "$work/install.sh" "$@"`. Dokumentierter Update-Einzeiler: `curl -fsSL https://github.com/pelmenipower/HerdCtrl/releases/latest/download/update.sh | bash`, lokal `herdctrl-update`, Prüfung `herdctrl-update --check`.


## 6. Rust-Änderungen

**`src/cli.rs` (neu) + `src/main.rs`:** `enum Mode { Editor, Tray, Identify(String), Version, Help, Unknown(String) }`, `fn parse(args: impl Iterator<Item = String>) -> Mode` mit Tests. `--version` → `herdctrl 0.2.0` (`env!("CARGO_PKG_VERSION")`), `--help` → Nutzung, `Unknown` → Nutzung auf stderr, Exit 2. `main.rs` behält die Reihenfolge (Renderer-Vorbereitung vor Threads).

**`src/core/update.rs` (neu, keine neue Abhängigkeit):**
- `Version(u32,u32,u32)`: `parse`, `Ord`, `Display`; `CURRENT` aus `CARGO_PKG_VERSION`; `version_from_redirect(&str) -> Option<Version>`.
- `State { enabled: bool (default true), checked_at: u64, latest: Option<String>, notified: Option<String> }` in `<config-dir>/update.json` (Pfad über `Config::path().parent()`, damit `XDG_CONFIG_HOME` isoliert), `load()` tolerant, `store()` atomar (`.tmp` + `rename`).
- `due(&State, now) -> bool` (24 h, Zukunft = fällig). `available(&State) -> Option<Version>` (latest > CURRENT, enabled, nicht `HERDCTRL_NO_UPDATE_CHECK`).
- `fetch_latest() -> Result<Version, String>` (blockierend, nur Worker): `timeout 20s curl -fsS --proto =https --max-time 15 -o /dev/null -w %{redirect_url} <URL>`; fehlt `curl`, Fehler mit Text, der nur ins Log geht.
- `release_url(&Version) -> String` und `open_release_notes()` (`xdg-open`, detached).
- `launch_updater() -> Result<(), String>`: Skript `herdctrl-update; printf '\nPress Enter to close '; read -r _`; fehlt `herdctrl-update` neben der laufenden Datei und auf `PATH` (Quellbau), stattdessen `curl -fsSL …/latest/download/update.sh | bash` mit vorangestelltem `echo` des Befehls. Terminal: `xdg-terminal-exec` falls vorhanden, sonst `applications::default_terminal(&applications::discover())?.terminal_command(script)`; Spawn über `child_env::clean`, stdio null, Reaper-Thread wie `spawn_self` (`src/tray.rs:73-82`).

**`src/tray.rs` + `Cargo.toml` (`libc = "0.2"`, bereits im Lock):**
- SIGTERM/SIGINT/SIGHUP setzen ein `AtomicBool`; die Schleife (`:112-167`) prüft es je Durchlauf (100 ms) und läuft in den bestehenden Shutdown (`drop(hotkeys)`, `handle.shutdown().wait()`). Lock wird mit dem Prozess freigegeben.
- Lock-Name: FNV-1a 64 über die Bytes des Config-Pfads statt `DefaultHasher` (Kommentar: stabil über Toolchains). Der erste Wechsel erkennt einen 0.1.0-Tray nicht über den Lock – deshalb sucht der Installer über `/proc`, nicht über Locks.
- Update-Worker (Thread): 60 s nach Start, dann alle 15 min prüfen, ob fällig; bei fällig `fetch_latest`, `State` speichern, `Message::Update(Option<Version>)` an die Schleife. Dort: Menü aktualisieren; ist `available` und `notified != latest` → `Notifier.show("HerdCtrl 0.2.0 is available", "Update from the tray menu, or run herdctrl-update in a terminal.")` und `notified` speichern.
- Menü: vor *Edit layouts…* bei Verfügbarkeit `Update to 0.2.0…` (→ `launch_updater` im Thread, Fehler als Benachrichtigung); nach *Start at login* `CheckmarkItem "Check for updates daily"` (schaltet `State.enabled`; Einschalten löst sofort eine Prüfung aus). Nur der Tray schreibt `update.json`.

**`src/ui/theme.rs` + `src/ui/dashboard.rs` + `src/ui/dashboard/snapshots.rs`:**
- `Tone::Accent` → (`accent_soft`, `accent`), Regel aus `docs/design-system.md` (Rolle in beiden Paletten vorhanden).
- Dashboard: Feld `update: Option<Version>`, beim Start und alle 60 s `update::State::load()` → `available()`. Pille `Update available · 0.2.0` (Accent) links der Tray-Pille, Hover `Click for options`; Klick öffnet ein kleines Popup mit **Update now** (`launch_updater`, Ergebnis ins Log: `Updater opened in a terminal` / Fehler) und **What's new** (`open_release_notes`). Keine Netzaufrufe im Editor.
- Snapshot-Zustand `update_available` in `snapshots.rs` (setzt `update = Some(…)`), Bild mit `render_ui_snapshots` ansehen. Galerie-Bilder unverändert.


## 7. Release-Workflow, CI, Tests

- `release.yml` Paketieren (`:36-47`): `cp install.sh "$DIST/$name/"` in den Tarball, `cp install.sh update.sh "$DIST/"`, `sha256sum -- *.tar.gz *.zip install.sh update.sh > SHA256SUMS`. Attestation und Signatur decken die Datei damit automatisch. Release-Notes-Text um die Einzeiler ergänzen. Workflow-Dry-Run auf dem PR läuft, weil die Datei sich ändert (`:10-12`).
- `ci.yml`: Schritt `bash -n install.sh update.sh && shellcheck -S warning install.sh update.sh scripts/install-gnome-extension.sh` (shellcheck ist auf `ubuntu-latest` vorinstalliert) und `python3 scripts/installer.test.py`. Für die Integrationsschritte `HERDCTRL_NO_UPDATE_CHECK=1` als Job-`env`; dasselbe in `scripts/check-x11.py:97` und `scripts/measure-rss.sh:71` (Wegwerf-Instanzen dürfen nicht ins Netz).
- `scripts/installer.test.py` (unittest, kein Netz, kein Desktop): Temp-`HOME`/`XDG_*`, Stub-Verzeichnis vorn im `PATH` mit `curl` (liefert Fixtures je URL; `-w %{redirect_url}` gibt den Fake-Tag), `sudo`/`apt-get` (Recorder), `dpkg-query` (ein Paket fehlt), `gnome-extensions` (Recorder, legt das Verzeichnis an), `update-desktop-database`, `gtk-update-icon-cache`, `setsid` (Recorder). Fixture-Tarball mit einem Shell-Skript `herdctrl`, das auf `--version` `herdctrl 0.2.0` ausgibt. Fälle: Frischinstallation (Dateien, `Exec`-Quoting mit Leerzeichen im `HOME`, Stamp, `herdctrl-update`, nur fehlende Pakete an `apt-get`), `--check` (Exit 10 und 0), gleiche Version erneut (Nichts zu tun), Update 0.2.0 → 0.3.0 inklusive Autostart-Umschreibung von einem fremden Pfad, `--archive`, ungültige `--version`, falsche Prüfsumme (nichts installiert), Tray-Stopp (Prozess `exec -a herdctrl sleep 60 --tray` wird beendet, einer mit anderem `XDG_CONFIG_HOME` nicht), `--uninstall` behält Konfiguration, `--purge` entfernt sie.
- Rust-Tests: `cli::parse`, `Version`, `version_from_redirect`, `due`, `available`, `State`-Roundtrip in Temp-Verzeichnis, Menübeschriftung. Kein Test ruft `curl`.
- `VALIDATION.md`: Tests, `stat -c %s target/release/herdctrl` (Größe nach `libc`/`update.rs`), Tray-RSS unverändert belegen; manuelle Abnahme als offene Punkte: Frischinstallation in einer sauberen Ubuntu-24.04-VM, Update 0.1.0 → 0.2.0 auf dem Rechner des Benutzers (Tray-Neustart, Hotkeys danach, Autostart-Umschreibung, Erweiterung), Benachrichtigung und Menüpunkt.


## 8. Regeln und Risiken (was ausdrücklich nicht getan wird)

- **Kein HTTP im Prozess, kein Selbst-Download:** keine neue Rust-HTTP-Abhängigkeit (Größe, RAM); die App lädt nie Dateien und ersetzt sich nie selbst. Einziger Update-Weg ist das Skript. `curl` nur im Tray-Worker mit `timeout`; der Editor liest nur `update.json`. Kein Netz im UI-Thread.
- **Alte Programmdatei nie zur Versionsabfrage starten:** 0.1.0 öffnet dann den Editor (`src/main.rs:16-21`). Nur Stamp lesen.
- **Nur eigene Prozesse mit passendem `XDG_CONFIG_HOME` stoppen**, nie den Editor, nie fremde Trays. Keine `pkill`-Muster.
- **Keine Grab-Verluste:** 0.2.0-Trays geben bei SIGTERM frei; ein 0.1.0-Tray kann das nicht, daher die einmalige 16-s-Wartezeit unter GNOME/Wayland. Unter X11 löst der X-Server die Grabs mit dem Prozess.
- **Erweiterung lädt nur nach Ab-/Anmelden** – das Skript sagt es, umgeht es nicht. Kein `gnome-extensions` außerhalb von GNOME/Wayland; andere Wayland-Compositoren bleiben unsupported.
- **`curl | bash` kann sich nicht selbst verifizieren;** wie bei Shellherd dokumentieren, Prüfsumme von `install.sh` liegt in `SHA256SUMS`, cosign optional.
- **Ein Schreiber für `update.json` (Tray).** Deshalb kein *Skip this version* im Editor in Stufe 1.
- **Datenschutz:** eine HTTPS-Anfrage pro Tag an github.com (IP-Adresse, curl-User-Agent), nichts aus der Konfiguration; Default an wie bei Shellherd, in beiden Handbüchern beschrieben, abschaltbar per Haken und Umgebungsvariable.
- **Lock-Namen ändern sich** mit dem FNV-Wechsel: Für genau einen Übergang erkennen alter und neuer Build einander nicht; der Installer stoppt den alten über `/proc`. Ohne diesen Wechsel bleibt das Risiko dauerhaft (CI-Build vs. lokaler Build).
- **Keine Versionsnummer erhöhen**; `MIN_VERSION=0.2.0` im Skript ist eine Konstante, die der Benutzer beim nächsten Release bestätigt (0.2.0 wäre das erste Release mit Installer).
- **Shellherd-Abweichungen bewusst:** Englisch statt Deutsch, Redirect statt API, Tarball statt DEB, `--archive` statt `--package`, zusätzlich `--check`/`--uninstall`.


## 9. Zerlegung in Tasks (getrennte Dateien) und Wellen

**Welle 1 (fünf parallele Tasks, keine Abhängigkeiten untereinander)**

| Task | Dateien | Inhalt |
|---|---|---|
| T1 CLI | `src/cli.rs` (neu), `src/main.rs` | `Mode`, `parse`, `--version`/`--help`, Unbekanntes ablehnen; Tests |
| T2 Update-Kern | `src/core/update.rs` (neu), `src/core/mod.rs` (eine Zeile) | `Version`, `State`, `due`, `available`, `fetch_latest`, `launch_updater`, `open_release_notes`; Tests ohne Netz |
| T3 Tray-Lebenszyklus | `src/tray.rs`, `Cargo.toml`, `Cargo.lock` | Signal-Handler, Shutdown-Pfad, FNV-Lock-Name; `cargo build` aktualisiert das Lock nur um den `libc`-Eintrag |
| T4 Skripte | `install.sh` (neu), `update.sh` (neu), `scripts/installer.test.py` (neu) | Abschnitt 5 komplett; Vertrag: `herdctrl --version` → `herdctrl X.Y.Z`, Exit 0 |
| T5 Workflows | `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `scripts/check-x11.py`, `scripts/measure-rss.sh` | Assets und Prüfsummen, shellcheck, Installer-Test, `HERDCTRL_NO_UPDATE_CHECK=1` |

**Welle 2 (zwei parallele Tasks, nach Welle 1)**

| Task | Dateien | Abhängig von |
|---|---|---|
| T6 Tray-Update | `src/tray.rs` | T2, T3: Worker, Benachrichtigung, Menüpunkte |
| T7 Editor-Pille | `src/ui/theme.rs`, `src/ui/dashboard.rs`, `src/ui/dashboard/snapshots.rs` | T2: `Tone::Accent`, Pille, Popup, Snapshot-Zustand |

**Welle 3 (Doku, zwei parallele Tasks; PRD und VALIDATION durch den Orchestrator)**

| Task | Dateien |
|---|---|
| T8 Englisch | `docs/manual.md` (Installation → *Install*, *Update*, *Uninstall*, *Update check and privacy*, Troubleshooting-Zeilen), `README.md` (Abschnitt *Install* mit Einzeiler vor *Install from source*, Feature-Punkt), `CHANGELOG.en.md` |
| T9 Deutsch | `docs/handbuch.md` (gleiche Abschnitte), `CHANGELOG.md`, Hinweis in `.claude/skills/run-app/SKILL.md` auf `HERDCTRL_NO_UPDATE_CHECK=1` für isolierte Instanzen |

Nach jeder Welle `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`; nach Welle 1 zusätzlich `python3 scripts/installer.test.py` und `shellcheck`. Nach Welle 2 `cargo test --locked render_ui_snapshots -- --ignored` und das neue Bild ansehen.


## 10. Erste Stufe gegenüber Extras

**Erste Stufe (komfortabel, nicht minimal):** T1–T9 vollständig: Einzeiler für Install und Update, `herdctrl-update` mit `--check`, Prüfsummen (cosign optional), Pakete, Erweiterung, Tray-Stopp/-Neustart ohne Hotkey-Verlust, Autostart-Umschreibung, Deinstallation, tägliche Prüfung im Tray mit einer Benachrichtigung je Version und Menüpunkt, Pille mit **Update now** im Editor, Assets und Prüfsummen im Release, CI-Tests ohne Netz, Doku in beiden Sprachen, PRD 4.7.

**Extras (nicht jetzt):** `install.ps1`; *Skip this version*; Buttons in der Benachrichtigung; Tray-Selbstneustart bei ersetzter Datei; Editor-Hinweis zum Neustart; Erweiterungs-Version und „Extension outdated“; Prerelease-Kanal; systemweite Installation; DEB/APT-Repo; arm64.


## 11. Rulings

- Ruling: Installer-Ausgaben Englisch — HerdCtrl-UI und Skripte sind Englisch, Doku zweisprachig — Kosten: Textzeilen.
- Ruling: Version über Redirect von `/releases/latest` statt GitHub-API — kein JSON, kein `python3`, kein Rate-Limit, Drafts/Prereleases ausgeschlossen — Kosten: hängt am Redirect-Verhalten von GitHub (seit Jahren stabil).
- Ruling: `update.json` nur vom Tray geschrieben, Editor liest — kein Schreibkonflikt — Kosten: *Skip this version* später.
- Ruling: `MIN_VERSION=0.2.0`, alte Programmdateien werden nie ausgeführt — 0.1.0 startet bei `--version` den Editor — Kosten: kein Rollback auf 0.1.0 per Skript.
- Ruling: Lock-Name per FNV-1a statt `DefaultHasher` — Stabilität über Toolchains — Kosten: ein Übergang, den der Installer über `/proc` abdeckt.
- Ruling: `libc` als direkte Abhängigkeit für Signale — bereits transitiv im Lock, keine Größenänderung — Kosten: keine.


## Ergänzende Rulings des Orchestrators

- Ruling: Der Installer läuft nur für Releases ab 0.2.0. Die Versionsnummer erhöht erst der Benutzer beim nächsten Release. Bis dahin lässt sich `--archive` mit einem lokal gepackten Tarball testen. — Kosten falls falsch: Ein Nutzer von 0.1.0 braucht einmal den Einzeiler statt `herdctrl-update`.
- Ruling: Die tägliche Prüfung ist standardmäßig an (wie Shellherd), abschaltbar per Haken und Umgebungsvariable, Datenschutz in beiden Handbüchern beschrieben. — Kosten: eine Anfrage pro Tag an github.com.
- Ruling: Pipeline-Stufen wie im Skill; die PRD-Ergänzung 4.7 schreibt der Orchestrator wörtlich nach Abschnitt 4 oben.
