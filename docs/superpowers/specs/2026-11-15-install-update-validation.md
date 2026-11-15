# Validierung: Installation und Updates

Stand 2026-11-15, Prüfer-Agent (Stufe 4). Geprüft: `docs/superpowers/specs/2026-11-15-install-update-design.md` (Commit 586d873) gegen `PRD.md` (inkl. 4.7), `AGENTS.md`, `CLAUDE.md`, den betroffenen Code und die Vorlage `Shellherd/install.sh`. Ausgangslage: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` grün (76 Tests, 9 ignoriert). `shellcheck` ist lokal **nicht** installiert. Der Redirect von `/releases/latest` wurde einmal geprüft: `302` nach `https://github.com/pelmenipower/HerdCtrl/releases/tag/v0.1.0`; die Asset-Namen von v0.1.0 (inkl. `herdctrl@herdctrl.local.shell-extension.zip`) bleiben auf GitHub unverändert.

**Urteil: ready after corrections.** 16 Blocker, 14 Korrekturen, 6 Minor. **Dieser Anhang ist bindend und geht Spec und Plan vor.** Wo die Spec etwas anderes sagt, gilt dieser Text.

---

## 1. Blocker (müssen so umgesetzt werden)

**B1 – Welle 1 wird rot: ungenutzte API in `src/core/update.rs`.** Im Binär-Crate erzeugt jedes nicht benutzte `pub`-Element eines privaten Moduls `dead_code`; mit `-D warnings` scheitert Clippy nach Welle 1, weil Tray und Editor die API erst in Welle 2 nutzen.
Fix: T2 setzt als erste Zeile nach dem Modul-Doc-Kommentar `#![allow(dead_code)] // Removed after wave 2, once tray.rs and dashboard.rs use this API.` Der Orchestrator löscht genau diese Zeile nach Welle 2 vor den Prüfungen; was dann als ungenutzt gemeldet wird, wird entfernt (nicht wieder erlaubt).

**B2 – `HERDCTRL_NO_UPDATE_CHECK` verhindert den Netzaufruf nicht.** Die Spec prüft die Variable nur in `available()`; der Worker würde trotzdem `curl` starten. CI, `check-x11.py`, `measure-rss.sh` und isolierte Instanzen gingen ins Netz.
Fix: `update::disabled_by_env()` wird im Tray **vor** jeder Planung geprüft; ist sie wahr, gibt es keinen Prüf-Thread, keine Schreibzugriffe auf `update.json`, und der Haken ist ausgegraut (Abschnitt 3.4). Wert: gesetzt, nicht leer und nicht `0` → abgeschaltet.

**B3 – Zwei Schreiber im Tray und Überschreiben einer ungültigen Datei.** Die Spec lässt den Worker `State` speichern und die Schleife `notified`/`enabled` speichern: verlorene Updates (Worker schreibt alten `enabled`-Wert zurück, nachdem der Benutzer abgeschaltet hat). Zudem lädt `load()` „tolerant“: Eine beschädigte Datei mit `"enabled": false` würde zu `enabled: true`, der Tray ginge gegen den Willen des Benutzers ins Netz und überschriebe die Datei (verstößt gegen „kein Überschreiben ungültiger Konfiguration“ und den Datenschutz-Abschnitt).
Fix: Nur die Tray-Hauptschleife liest und schreibt `State`. Der Prüf-Thread ist ein Einmal-Thread, der nur `fetch_latest()` ausführt und `Message::Checked(Result<Version, String>)` schickt. `State::load` liefert `Ok(State::default())` nur bei `NotFound`, bei Lese- oder JSON-Fehler `Err`. Bei `Err` im Tray: `eprintln!("update.json: {e}")`, keine Prüfung, kein Schreiben, Haken unchecked; erst ein Klick auf den Haken schreibt eine neue Datei (bewusste Benutzeraktion). Der Editor behandelt `Err` als „kein Update“.

**B4 – Blockierender Aufruf im UI-Thread.** `launch_updater()` ruft `applications::discover()` (Dateisystem-Scan) und `default_terminal()` (startet `timeout 2s xdg-terminal-exec --print-id` und wartet) auf. Die Spec ruft es direkt aus dem Popup des Editors.
Fix: `launch_updater` ist dokumentiert blockierend („call from a worker thread“). Der Editor startet es in `std::thread::spawn` und bekommt das Ergebnis über einen Kanal (`update_rx: Receiver<Result<(), String>>`), den `ui()` je Frame mit `try_recv` leert. Der Tray startet es ebenfalls in einem Thread (Spec) und meldet Fehler über `Message::UpdaterFailed(String)` an die Schleife, die dann benachrichtigt (der `Notifier` lebt nur dort).

**B5 – Rauchtest nach dem Einspielen, ELF-Prüfung gegen Test-Fixture, Bibliotheken.** Schritt 9 prüft `--version` erst, nachdem die alte Datei schon ersetzt ist: ein Fehlschlag hinterlässt eine kaputte Installation. Schritt 5 verlangt ELF-Magic, der Installer-Test liefert aber ein Shell-Skript als `herdctrl` (Widerspruch, Test kann nie grün werden). Ohne installierte Laufzeitpakete kann die Datei ggf. nicht laden.
Fix: Reihenfolge Download → Prüfung → Entpacken → Pakete (6) → `ldd`-Hinweis → **Rauchtest auf der entpackten Datei** (`timeout 10 "$stage/herdctrl" --version </dev/null`, Ausgabe muss exakt `herdctrl $version` sein) → erst dann Tray stoppen (7) und einspielen (8). Die ELF-Magic-Prüfung entfällt (Prüfsumme plus Rauchtest decken sie ab). Schlägt der Rauchtest fehl: `HerdCtrl: the downloaded program does not start (expected "herdctrl $version"). Missing libraries? Run: ldd <path>` und Exit 1, nichts installiert, kein Tray gestoppt.

**B6 – Skript ersetzt sich während der Ausführung; Trap-Fallen.**
- `herdctrl-update` *ist* das laufende Skript. Bash liest Skriptdateien stückweise; ein Überschreiben am Ort (`install`, `cp`) lässt Bash nach `main` Datenmüll lesen. Fix: Jede eingespielte Datei (`herdctrl`, `herdctrl-update`, Desktop-Eintrag, Autostart, Stamp) über Temp-Datei im Zielverzeichnis plus `mv -f` (neuer Inode). Letzte Zeile des Skripts exakt `main "$@"; exit`.
- `trap 'rm -rf "$work"' EXIT` mit lokalem `work` in `main`: Beim Skriptende ist `work` nicht mehr gesetzt, `set -u` bricht im Trap ab, nichts wird aufgeräumt. Fix: globale Variable `work=''` am Skriptanfang, `cleanup() { if [[ -n $work ]]; then rm -rf -- "$work"; fi; }`, `trap cleanup EXIT`.
- `update.sh`: `exec bash "$work/install.sh"` läuft am EXIT-Trap vorbei (Temp-Verzeichnis bleibt liegen); ohne `exec` löschte ein Trap das Skript nicht vor dem Lesen, aber der Exit-Code (10 bei `--check`) muss durchgereicht werden. Fix: `status=0; bash "$work/install.sh" "$@" || status=$?; exit "$status"` (Trap räumt danach auf). `exec "$BIN/herdctrl-update" "$@"` ohne Temp-Verzeichnis bleibt erlaubt.

**B7 – Prozess-Erkennung über `/proc` ist so nicht korrekt.**
- „Gleicher `XDG_CONFIG_HOME`-Wert (gesetzt oder nicht gesetzt)“ ist falsch: `Config::path()` (`src/core/config.rs:69-76`) nimmt `XDG_CONFIG_HOME` nur, wenn er absolut ist, sonst `$HOME/.config`. Ein Tray mit `XDG_CONFIG_HOME=$HOME/.config` und ein Installer ohne Variable meinen dieselbe Konfiguration; ein relativer Wert ist „nicht gesetzt“. Fix: Für Installer und Zielprozess dieselbe Funktion `effective_config <xdg> <home>` → `xdg` wenn es mit `/` beginnt, sonst `home/.config`; verglichen wird das Ergebnis. `HOME` und `XDG_CONFIG_HOME` des Zielprozesses aus dessen `environ`.
- Nur eigene Prozesse: `[[ -O /proc/$pid ]]`, sonst überspringen.
- Exakte Muster: Tray = `argc == 2`, Basisname von `argv[0]` ist `herdctrl`, `argv[1] == --tray`. Editor = `argc == 1`, Basisname `herdctrl`. Alles andere (`--identify-monitors`, `--version`, `cargo`) wird ignoriert. Eigene PID `$$`/`$BASHPID` überspringen.
- NUL-sicher und `set -e`-fest lesen: `mapfile -d '' -t argv < "/proc/$pid/cmdline" 2>/dev/null || continue`; `environ` mit `while IFS= read -r -d '' entry; do case $entry in HOME=*|XDG_CONFIG_HOME=*) …; esac; done < "/proc/$pid/environ" 2>/dev/null || continue`. Kein `grep` in Pipelines (kein Treffer = Exit 1 → `pipefail`/`set -e` beendet den Installer).
- Warten auf Ende: alle 0,1 s bis 5 s; beendet heißt `/proc/$pid` fehlt **oder** Zustand `Z` (drittes Feld in `/proc/$pid/stat` nach `") "`). `kill -0` allein zählt Zombies als lebend. Nach 5 s `kill -KILL`, weitere 2 s warten; dieser Tray gilt dann als „ohne Grab-Freigabe“ (B8). Lebt er danach noch: `HerdCtrl: the tray (PID $pid) does not stop. Quit it from its menu and run the installer again.` Exit 1, nichts eingespielt.

**B8 – Tray-Neustart: Sitzung, Umgebung, Zeitpunkt.**
- Ohne grafische Sitzung (z. B. per SSH) startete der Installer einen Tray ohne `DISPLAY`/`WAYLAND_DISPLAY`: Hotkeys aus, Lock belegt. Fix: Stoppen und Neustarten nur, wenn `${WAYLAND_DISPLAY:-}${DISPLAY:-}` nicht leer ist. Sonst Tray laufen lassen und ausgeben: `The tray keeps running the previous version until you quit and restart it.` (`--uninstall` stoppt immer.)
- Der neue Tray erbt die Umgebung des Installers. Startete der Tray-Menüpunkt den Installer, enthält sie ggf. `HERDCTRL_TERMINAL_COMMAND` (gesetzt von `InstalledApp::terminal_command`, `src/core/applications.rs:55`), das die Fensterzuordnung in `src/core/launcher/matching.rs:41` liest; Programme des neuen Trays mit Roh-Befehl erbten es. Fix: `(cd "$HOME" && exec env -u HERDCTRL_TERMINAL_COMMAND setsid -f "$BIN/herdctrl" --tray </dev/null >/dev/null 2>&1 9>&-)`.
- Grab-Wartezeit unter GNOME/Wayland (16 s, Heartbeat 15 s in `controller.mjs:19-22`, zusätzlich räumt `extension.js` alle 5 s ab, ein `bindings`-Aufruf räumt sofort ab): Sie gilt, wenn vorher kein Stamp existierte **oder** der Tray mit SIGKILL beendet wurde, und läuft ab dem festgestellten Prozessende, nicht ab dem Signal. Text: `Waiting for GNOME to release the previous tray's shortcuts…`.
- Danach bis 3 s prüfen, ob ein passender Tray-Prozess existiert (Muster aus B7); sonst `The tray did not start. Start HerdCtrl from the app menu.` (Warnung, Exit-Code bleibt 0). Ein vom offenen Editor zwischenzeitlich gestarteter Tray zählt als Erfolg; der zweite Start scheitert harmlos am Lock.

**B9 – Ausführung als root.** `curl … | sudo bash` installierte nach `/root/.local` und durchsuchte root-Prozesse. Fix: `if ((EUID == 0)); then fail 'Run the installer as your normal user, not with sudo.'; fi` vor allem anderen außer `--help`.

**B10 – Zwei Installer gleichzeitig** (Tray-Menü und Editor-Knopf, oder Doppelklick) stoppen sich gegenseitig den Tray und tauschen Dateien parallel. Fix: `exec 9>"${XDG_RUNTIME_DIR:-/tmp}/herdctrl-install-$(id -u).lock"; flock -n 9 || fail 'Another HerdCtrl installer is running.'` (gilt auch für `--uninstall`, nicht für `--check`/`--help`). Der Deskriptor 9 darf **nicht** an den neuen Tray vererbt werden (`9>&-`, B8), sonst hält der Tray die Installer-Sperre dauerhaft. `flock` (util-linux) kommt in die Liste der Pflichtprogramme.

**B11 – Quoting im Desktop-Eintrag.** Die Spec verweist auf `src/tray.rs:494-503`; das ist nur die Exec-Ebene. Laut Desktop-Entry-Spezifikation werden zuerst die String-Escapes (`\\`, `\s`, `\n`) aufgelöst, dann das Exec-Quoting; `$` in Anführungszeichen bräuchte `\\$` in der Datei, `%` muss `%%` sein. Fix (einfach und sicher): Der Installer lehnt ein `$BIN` ab, das eines der Zeichen `"` `` ` `` `$` `\` `%` oder einen Zeilenumbruch enthält: `HerdCtrl: the install directory $BIN contains characters a desktop entry cannot hold. Set XDG_BIN_HOME to a simpler path.` Exit 1. Sonst `Exec="$BIN/herdctrl"` (Leerzeichen sind in Anführungszeichen erlaubt) und `TryExec=$BIN/herdctrl` **ohne** Anführungszeichen (TryExec ist kein Exec-Wert). Gleiches für den Autostart-Eintrag: `Exec="$BIN/herdctrl" --tray`.

**B12 – `--archive` widerspricht sich selbst.**
- Orchestrator-Ruling: Bis 0.2.0 wird mit einem lokal gepackten Tarball getestet; der hat Version 0.1.0 (`Cargo.toml`), `MIN_VERSION=0.2.0` würde ihn ablehnen.
- „Version aus dem Verzeichnisnamen im Archiv“ lädt zu `tar -tzf … | head -1` ein: `head` schließt die Pipe, `tar` stirbt an SIGPIPE, `pipefail` + `set -e` beenden den Installer zufällig.
- Das Archiv enthält kein Erweiterungs-ZIP.
Fix: `MIN_VERSION` gilt nur für heruntergeladene Versionen (`--version`, neueste). Bei `--archive` ist das Merkmal „Installer-Ära“ die Pflichtdatei `install.sh` im Archiv (ein 0.1.0-Release-Tarball hat sie nicht und wird abgelehnt, **bevor** irgendetwas ausgeführt wird: `HerdCtrl: this archive predates the installer (no install.sh inside).`). Version: Archiv nach `$work/stage` entpacken (ohne `--strip-components`), genau ein Verzeichnis `herdctrl-v<X.Y.Z>-linux-x86_64` verlangen und die Version daraus per Bash-Regex lesen; danach Rauchtest (B5). Erweiterung wird bei `--archive` übersprungen mit `The GNOME extension is not part of the archive; install it with scripts/install-gnome-extension.sh.` Downloads nutzen dasselbe Entpacken (dann ist der Verzeichnisname zusätzlich gegen die Zielversion zu prüfen).

**B13 – „Bereits installiert“ braucht für die Erweiterung einen Download.** Schritt 3 will die Erweiterung prüfen, das geht nur mit dem ZIP; zugleich soll der Schritt ohne Download auskommen. Fix: Bei gleicher Version ohne `--reinstall` laufen nur Schritt 6 (Pakete, ohne Download) und Schritt 10 (Autostart umbiegen). Fehlt unter GNOME/Wayland das Verzeichnis `$DATA/gnome-shell/extensions/herdctrl@herdctrl.local`, Hinweis `The GNOME extension is missing. Run: herdctrl-update --reinstall`. Kein Tray-Stopp. Dann `HerdCtrl $version is already installed. Nothing to do.` Exit 0.

**B14 – Signal-Handling im Tray präzisieren** (sonst `signal()` ohne `SA_RESTART`, `eprintln!` im Handler o. Ä.).
- `sigaction` (nicht `signal`) für `SIGTERM`, `SIGINT`, `SIGHUP`, `sa_flags = SA_RESTART`, leere Maske. Ohne `SA_RESTART` bekommen blockierende Aufrufe in fremden Threads (zbus, global-hotkey, ksni) `EINTR`.
- Der Handler tut ausschließlich `STOP.store(true, Ordering::Relaxed)` auf ein `static AtomicBool` (lock-frei, async-signal-sicher). Keine Ausgabe, keine Allokation, kein Mutex, kein Kanal.
- Cast in zwei Schritten (`request_stop as extern "C" fn(libc::c_int) as libc::sighandler_t`), damit kein Lint zu Funktions-zu-Integer-Casts anschlägt; `// SAFETY:`-Kommentar wie in `src/ui/hotkey_input.rs:190`.
- Installation in `run()` direkt nach erfolgreichem `lock(TRAY_LOCK)`, vor `Hotkeys::new`. Fehler → `eprintln!` und weiterlaufen.
- Die Schleife prüft `STOP` am Anfang jedes Durchlaufs **und** direkt nach `recv_timeout` und läuft dann in den bestehenden Shutdown (`drop(hotkeys)`, `handle.shutdown().wait()`). Prüf- und Updater-Threads werden nie gejoint (ein laufendes `curl` darf das Beenden nicht bis zu 20 s aufhalten).
- Der Lock (`File` mit `O_CLOEXEC` aus `std`) fällt mit dem Prozessende; das genügt.

**B15 – Updater-Terminal erbt die falsche Konfiguration.** Terminals mit Server-Prozess (Ptyxis, GNOME Terminal) starten die Shell mit der Umgebung des Servers, nicht mit der des Aufrufers. Ein Tray mit eigener `XDG_CONFIG_HOME` (run-app-Skill) würde den Installer gegen die **echte** Konfiguration laufen lassen und den echten Tray des Benutzers stoppen. Fix: `launch_updater` schreibt die Konfiguration in das Skript: Ist `XDG_CONFIG_HOME` im eigenen Prozess gesetzt und absolut, beginnt das Skript mit `export XDG_CONFIG_HOME=<shlex::try_quote>; `. Den Pfad zu `herdctrl-update` ebenfalls absolut und gequotet (das Terminal hat `~/.local/bin` nicht sicher im `PATH`). Nach `terminal_command(…)` wird `HERDCTRL_TERMINAL_COMMAND` per `env_remove` wieder entfernt. Vorlage siehe 3.3 `updater_script`.

**B16 – `curl | bash` und stdin.** Unter `curl | bash` ist stdin das Skript. Fix: Jeder Kindprozess außer `sudo` bekommt `</dev/null`; `sudo` fragt über `/dev/tty`. APT: `sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y -- <pakete> </dev/null` (sudo verwirft sonst die Variable). Kein `read` im Installer. Schlägt `sudo` fehl (kein TTY, falsches Passwort): `Could not install packages: <liste>. Install them with: sudo apt-get install <liste>` und weiter (der Rauchtest aus B5 fängt fehlende Bibliotheken ab).

---

## 2. Korrekturen (bindend)

**K1 – Veraltete Installer-Logik (Übergabe).** Läuft `herdctrl-update` (das Installer-Skript des *installierten* Releases) für eine neuere Zielversion, fehlen ihm neue Schritte dieser Version (z. B. neue Pakete) bis zum nächsten Lauf. Fix: Nach Bestimmen der Zielversion und Laden von `SHA256SUMS` (und cosign-Prüfung) wird bei Install/Update zusätzlich `install.sh` des Ziel-Tags geladen und geprüft. Läuft das Skript aus einer Datei (`[[ -f ${BASH_SOURCE[0]:-} ]]`), ist `HERDCTRL_INSTALL_HANDOFF` nicht gesetzt und weicht die SHA-256 der eigenen Datei von der geprüften ab: `Handing over to the installer of HerdCtrl $version…`, Sperre freigeben (`exec 9>&-`), dann `status=0; HERDCTRL_INSTALL_HANDOFF=1 bash "$work/install.sh" "$@" || status=$?; exit "$status"`. Nicht bei `--check`, `--uninstall`, `--archive`. Unter `curl | bash` keine Übergabe.

**K2 – Versionsabfrage ohne `--location`.** Die Regel „Download immer … `--location`“ gilt nicht für Schritt 1: Mit `-L` ist `%{redirect_url}` leer. Exakt: `curl --fail --silent --show-error --proto '=https' --max-time 30 -o /dev/null -w '%{redirect_url}' https://github.com/pelmenipower/HerdCtrl/releases/latest </dev/null`. Akzeptiert nur `^https://github\.com/pelmenipower/HerdCtrl/releases/tag/v([0-9]+\.[0-9]+\.[0-9]+)$` (Groß-/Kleinschreibung exakt). Sonst `HerdCtrl: could not determine the latest release (GitHub answered: <url or "no redirect">).` Exit 1. Downloads: `curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 600 -o <datei> <url> </dev/null`.

**K3 – Versionsvergleich ohne `sort -V`.** Reiner Bash-Vergleich `version_gt a b` (je Komponente `IFS=. read -r`, dann `((10#$x > 10#$y))`); `sort -V` und `setsid` bleiben nicht Pflicht für den Vergleich. Pflichtprogramme: `curl tar sha256sum mktemp flock setsid timeout uname id`. `--check` braucht nur `curl`.

**K4 – Signatur fehlerhaft = Abbruch.** Ist `cosign` vorhanden und schlägt `verify-blob` fehl: `HerdCtrl: the signature of SHA256SUMS does not verify. Nothing was installed.` Exit 1. Prüfsumme: wie Shellherd (`awk -v asset=… '$2 == asset {print $1; n++} END {exit n != 1}'`, Hex-Regex, `sha256sum --check --status`). Fehler: `HerdCtrl: checksum mismatch for <asset>. Nothing was installed.` Exit 1.

**K5 – `--check` mit unbekannter Installation.** Ohne Stamp: `Installed: unknown (installed by hand or before 0.2.0) · Latest: X → Run the installer to update.` Exit 10. Ist die installierte Version ≥ der neuesten: `HerdCtrl X is up to date` Exit 0. Mit Update: `Installed: A · Latest: B → Update available. Run: herdctrl-update` Exit 10.

**K6 – Argumentregeln (Exit 2).** Unbekannte Option, fehlender Wert, `--version` + `--archive`, `--check` + (`--archive`|`--reinstall`|`--uninstall`), `--purge` ohne `--uninstall`, `--uninstall` mit anderen Optionen als `--purge`, `--extension` + `--no-extension`, `--version` nicht `X.Y.Z` (ein führendes `v` wird entfernt). Text `HerdCtrl: <grund>` plus `Run with --help for usage.` auf stderr. `--version` < `MIN_VERSION`: `HerdCtrl X predates the installer; install 0.2.0 or newer.` Exit 1 (gültige Eingabe, nicht unterstützt).

**K7 – Autostart nur umbiegen und nur löschen, wenn es unserer ist.** Umbiegen: Die Datei hat genau eine Zeile `Exec=…`, deren Programm (erstes Wort, ggf. ohne Anführungszeichen) den Basisnamen `herdctrl` hat und die auf ` --tray` endet; nur diese Zeile wird ersetzt, Rest unverändert, Temp + `mv`. Ausgabe `Start at login now uses $BIN/herdctrl.` Nie anlegen. `--uninstall` löscht den Autostart-Eintrag nur, wenn sein `Exec` auf `"$BIN/herdctrl" --tray` zeigt; sonst Hinweis `Left $CONFIG/autostart/herdctrl.desktop alone: it starts another HerdCtrl.`

**K8 – Pfade mit derselben Regel wie Rust.** `BIN`, `DATA`, `CONFIG`: XDG-Variable nur, wenn sie mit `/` beginnt, sonst Standard (`$HOME/.local/bin`, `$HOME/.local/share`, `$HOME/.config`); so behandelt auch `Config::path()` und `applications::discover()` relative Werte. `--purge` löscht `$CONFIG/herdctrl` nur, wenn `CONFIG` absolut und nicht `/` ist.

**K9 – Stamp und eingespielte Dateien.** Stamp `$DATA/herdctrl/installed-version`, Inhalt `X.Y.Z\n`, Temp + `mv`, zuletzt geschrieben (nach Rauchtest und Einspielen). Gelesen: erste Zeile, muss `^[0-9]+\.[0-9]+\.[0-9]+$` erfüllen, sonst „unknown“.

**K10 – Update-Plan im Tray (statt Langzeit-Worker).** Die Tray-Schleife plant selbst im bestehenden 1-s-Takt (`next_check`): Prüfung starten, wenn Prüfung erlaubt (nicht `disabled_by_env`, `State` geladen, `enabled`), kein Thread läuft, `Instant::now() >= not_before` und (`force` oder `due(&state, now_unix())`). `not_before` = Start + 60 s. Erfolg: `state.checked_at = now`, `state.latest = Some(v.to_string())`, speichern, Menü aktualisieren, ggf. benachrichtigen. Fehler: nur `eprintln!("Update check failed: {e}")`, `not_before = now + 1 h`, nichts gespeichert. Einschalten per Haken: `enabled = true`, speichern, `force = true`, `not_before = now`. Ausschalten: `enabled = false`, speichern, Menüpunkt *Update to …* verschwindet.

**K11 – `update.json` atomar ohne hängende Temp-Datei.** `Config::save` nutzt `create_new` für die Temp-Datei; bleibt sie nach einem Absturz liegen, scheitert jedes weitere Speichern. `State::store` öffnet `update.json.tmp` mit `write + create + truncate`, Modus `0o600`, `write_all`, `sync_all`, `rename`; bei Fehler Temp löschen. Verzeichnis anlegen wie `Config::save`.

**K12 – Rückfall ohne `herdctrl-update`.** Das Skript führt keinen unbestätigten `curl | bash` in `sh` aus (ohne `pipefail` liefe bei Netzfehler stumm `bash` mit leerer Eingabe). Vorlage in 3.3 (`updater_script`): erklärender Text, Bestätigung per Enter (stdin ist hier das Terminal), Download in eine Temp-Datei, dann `bash`. Für Quellbauten heißt das: Das Update installiert das Release nach `~/.local/bin` und biegt den Autostart um; der Text sagt das.

**K13 – Editor: keine echten Dateien in Tests, kein Polling dort.** `Dashboard::with_parts` setzt `update: None`, `update_checked: None` und liest `update.json` nicht; nur `Dashboard::new` lädt und setzt `update_checked = Some(Instant::now())`. `ui()` lädt nur neu, wenn `update_checked` `Some` und älter als 60 s ist. So hängen Snapshots, Galerie und Interaktionstests nicht von der `update.json` des Rechners ab; die Galerie bleibt unverändert.

**K14 – Prüfungen pro Welle und CI-Umfang.** `shellcheck` ist lokal nicht installiert: Nach Welle 1 lokal `bash -n install.sh update.sh` und `python3 scripts/installer.test.py`; `shellcheck -S warning install.sh update.sh` nur, wenn vorhanden, sonst übernimmt die CI. In CI wird `scripts/install-gnome-extension.sh` **nicht** neu unter shellcheck gestellt (ungeprüfter Altbestand könnte die CI rot machen; Minor M3). Die CI-Job-`env` `HERDCTRL_NO_UPDATE_CHECK: '1'` steht auf Job-Ebene (`jobs.linux.env`), damit auch der vom Editor gestartete Tray in `check-x11.py` sie erbt.

---

## 3. Vertrag

### 3.1 `src/cli.rs` (neu, T1)

```rust
//! Command-line modes. Parsed before any lock, renderer setup or thread exists.

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Editor,
    Tray,
    Identify(String),
    Version,
    Help,
    /// The message for stderr, without the usage text.
    Unknown(String),
}

/// `args` excludes argv[0].
pub fn parse(args: impl IntoIterator<Item = String>) -> Mode;

/// "herdctrl 0.1.0" (from CARGO_PKG_VERSION).
pub fn version_line() -> String;

pub const USAGE: &str = "Usage: herdctrl [--tray | --version | --help]\n\n  (no option)  Open the layout editor\n  --tray       Run in the background with the global hotkeys and a tray icon\n  --version    Print the version\n  --help       Print this help\n";
```

Regeln für `parse` (Tests für jede Zeile):

| Argumente | Ergebnis |
|---|---|
| keine | `Editor` |
| `--tray` | `Tray` |
| `--identify-monitors <data>` | `Identify(data)` |
| `--identify-monitors` | `Unknown("Missing monitor identification data")` |
| `--version` oder `-V` | `Version` |
| `--help` oder `-h` | `Help` |
| erstes Argument sonst | `Unknown(format!("Unknown option: {arg}"))` |
| zusätzliches Argument nach einem gültigen Modus | `Unknown(format!("Unexpected argument: {arg}"))` |

`main.rs`: `mod cli;`, dann als Erstes `match cli::parse(std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()))`. `Version` → `println!("{}", cli::version_line())`, `Ok(())`. `Help` → `print!("{}", cli::USAGE)`, `Ok(())`. `Unknown(m)` → `eprintln!("herdctrl: {m}")`, `eprint!("{}", cli::USAGE)`, `std::process::exit(2)`. `Identify(raw)` → `ui::identify::run_helper(&raw)`. `Tray` → `tray::run()`. `Editor` → bestehender Pfad unverändert ab `ui::renderer::prepare()`. `--version`/`--help` laufen ohne Lock, ohne Renderer, ohne D-Bus.

### 3.2 `src/tray.rs`, Welle 1 (T3)

```rust
static STOP: std::sync::atomic::AtomicBool = AtomicBool::new(false);
extern "C" fn request_stop(_signal: libc::c_int);
/// SIGTERM, SIGINT and SIGHUP end the run loop through the normal shutdown.
fn handle_stop_signals() -> Result<(), String>;
fn stop_requested() -> bool;

/// FNV-1a, 64 bit: stable across Rust toolchains, unlike DefaultHasher.
fn fnv1a64(bytes: &[u8]) -> u64;
/// "herdctrl-{kind}-{:016x}.lock" over the bytes of the layouts path.
fn lock_name(kind: &str, config: &std::path::Path) -> String;
```

`lock()` nutzt `lock_name(kind, &Config::path()?)` mit `std::os::unix::ffi::OsStrExt::as_bytes`. Testvektoren: `fnv1a64(b"") == 0xcbf29ce484222325`, `fnv1a64(b"a") == 0xaf63dc4c8601ec8c`, `lock_name("tray", Path::new("/home/u/.config/herdctrl/layouts.json")) == "herdctrl-tray-b7fc927f20e81cf5.lock"`. Signal-Test: `handle_stop_signals()`, `libc::raise(libc::SIGHUP)`, `assert!(stop_requested())` (nur der Flag, kein Shutdown; der Test setzt `STOP` danach zurück). `Cargo.toml`: `libc = "0.2"`; `Cargo.lock` einmal mit `cargo check --offline` aktualisieren, Diff darf nur `"libc",` in der Abhängigkeitsliste von `herdctrl` hinzufügen.

### 3.3 `src/core/update.rs` (neu, T2) – öffentliche API

```rust
//! Release check state, version parsing and the updater launch. No HTTP in-process:
//! `fetch_latest` runs curl and must only be called from a worker thread.
#![allow(dead_code)] // Removed after wave 2, once tray.rs and dashboard.rs use this API.

pub const LATEST_URL: &str = "https://github.com/pelmenipower/HerdCtrl/releases/latest";
pub const TAG_URL_PREFIX: &str = "https://github.com/pelmenipower/HerdCtrl/releases/tag/v";
pub const UPDATE_SCRIPT_URL: &str =
    "https://github.com/pelmenipower/HerdCtrl/releases/latest/download/update.sh";
pub const NO_CHECK_ENV: &str = "HERDCTRL_NO_UPDATE_CHECK";
pub const CHECK_INTERVAL_SECS: u64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Exactly "X.Y.Z" with decimal digits; no "v", no whitespace, no suffix.
    pub fn parse(text: &str) -> Option<Self>;
    /// This build's version (CARGO_PKG_VERSION).
    pub fn current() -> Self;
}
impl std::fmt::Display for Version {} // "0.2.0"

/// Trims ASCII whitespace, requires TAG_URL_PREFIX exactly, then Version::parse on the rest.
pub fn version_from_redirect(url: &str) -> Option<Version>;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)] // no deny_unknown_fields: newer builds may add fields
pub struct State {
    pub enabled: bool,          // default true
    pub checked_at: u64,        // Unix seconds of the last successful check, 0 = never
    pub latest: Option<String>, // "X.Y.Z" from the last successful check
    pub notified: Option<String>,
}
impl Default for State {}

impl State {
    /// `<dir of Config::path()>/update.json`.
    pub fn path() -> Result<std::path::PathBuf, String>;
    /// NotFound → Ok(State::default()); unreadable or invalid JSON → Err.
    pub fn load(path: &std::path::Path) -> Result<State, String>;
    /// Temp file (write+create+truncate, 0600), sync, rename.
    pub fn store(&self, path: &std::path::Path) -> Result<(), String>;
    pub fn latest_version(&self) -> Option<Version>;
}

pub fn now() -> u64; // Unix seconds, 0 on clock error
/// Set to a non-empty value other than "0".
pub fn disabled_by_env() -> bool;
/// `now < checked_at` (clock moved back) or `now - checked_at >= CHECK_INTERVAL_SECS`.
pub fn due(state: &State, now: u64) -> bool;
/// `enabled` and latest > current. Pure; for tests.
pub fn newer(state: &State, current: Version) -> Option<Version>;
/// `None` if disabled_by_env(), else newer(state, Version::current()).
pub fn available(state: &State) -> Option<Version>;

/// Blocking, worker only. Runs
/// `timeout 20s curl --fail --silent --show-error --proto =https --max-time 15
///  -o /dev/null -w %{redirect_url} <LATEST_URL>` with stdin null, no shell.
/// Errors: "curl is not installed", "curl: <stderr>", "Unexpected redirect: <url>".
pub fn fetch_latest() -> Result<Version, String>;

/// "https://github.com/pelmenipower/HerdCtrl/releases/tag/v0.2.0"
pub fn release_url(version: Version) -> String;
/// `xdg-open <release_url>` detached (spawn + reaper thread, stdio null). Non-blocking.
pub fn open_release_notes(version: Version) -> Result<(), String>;

/// Blocking (application discovery, terminal detection): call from a worker thread.
/// Opens a terminal that runs `updater_script(...)`.
pub fn launch_updater() -> Result<(), String>;

/// `updater`: absolute path of herdctrl-update if found next to the running executable
/// (strip " (deleted)" like tray::executable) or on PATH (applications::executable_path),
/// else None. `config_home`: XDG_CONFIG_HOME if set and absolute.
pub fn updater_script(updater: Option<&std::path::Path>, config_home: Option<&std::path::Path>) -> String;
```

`updater_script` (Werte mit `shlex::try_quote`; schlägt Quoting fehl, `launch_updater` → `Err`):

- Präfix nur mit `config_home`: `export XDG_CONFIG_HOME=<q>; `
- Mit `updater`: `<q-updater>; printf '\nPress Enter to close '; read -r _`
- Ohne `updater`:
  `printf 'herdctrl-update is not installed. This downloads the HerdCtrl installer and installs the latest release to ~/.local/bin.\nPress Enter to continue, or close this window. '; read -r _; t=$(mktemp) && curl -fsSL --proto '=https' <UPDATE_SCRIPT_URL> -o "$t" && bash "$t"; rm -f "$t"; printf '\nPress Enter to close '; read -r _`

Terminal: `applications::executable_path("xdg-terminal-exec")` gefunden → `Command::new(path).args(["sh", "-c", &script])`; sonst `applications::default_terminal(&applications::discover())?.terminal_command(&script)?` und danach `env_remove("HERDCTRL_TERMINAL_COMMAND")`. Immer `crate::os::child_env::clean`, `stdin/stdout/stderr` null, Reaper-Thread `thread::spawn(move || child.wait())`. Fehlertext ohne Terminal: `No terminal found. Run herdctrl-update in a terminal.`

Tests (kein Netz, kein Prozessstart): `Version::parse` (gültig, `v0.2.0`, `0.2`, `0.2.0.1`, `0.2.0-rc1`, Leerzeichen), Ordnung (`0.10.0 > 0.9.9`), `Display`, `version_from_redirect` (gültig mit `\n`, `/releases` ohne Tag, fremdes Repo, `http://`), `due` (0, 23 h 59 min, 24 h, Zukunft), `newer` (disabled, gleich, älter, neuer, kaputter String), `State` Roundtrip und `load` (fehlt → Default, kaputt → `Err`, unbekanntes Feld → ok) in einem Temp-Verzeichnis, `updater_script` mit/ohne Updater, mit Leerzeichen und `'` im Pfad.

### 3.4 Tray, Welle 2 (T6)

```rust
enum Message {
    Launch(usize), Edit, Autostart(bool), Quit,          // bestehend
    Checked(Result<update::Version, String>),            // Einmal-Thread fetch_latest
    UpdateCheck(bool),                                    // Haken
    Update,                                               // Menüpunkt "Update to …"
    UpdaterFailed(String),                                // Thread launch_updater
}
struct Tray { /* bestehend */ update: Option<update::Version>, update_check: UpdateCheck }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UpdateCheck { On, Off, Unavailable } // Unavailable: disabled_by_env()
fn update_label(version: update::Version) -> String; // "Update to 0.2.0…"
```

Menü (Reihenfolge): Views, Separator, bei `update.is_some()` **`Update to 0.2.0…`**, **`Edit layouts…`**, **`Start at login`**, `CheckmarkItem` **`Check for updates daily`** (`checked = update_check == On`, `enabled = update_check != Unavailable`, Aktivierung sendet `UpdateCheck(!checked)`), Separator, **`Quit HerdCtrl`**.

Benachrichtigung bei `available(&state) == Some(v)` und `state.notified.as_deref() != Some(&v.to_string())`: Titel `HerdCtrl 0.2.0 is available`, Text `Update from the tray menu, or run herdctrl-update in a terminal.`; danach `notified` speichern. Fehler: `Updater could not start` / `<fehler>`; `Update check setting not saved` / `<fehler>`. Tests: `update_label`, Menütexte über eine reine Funktion (z. B. Labels der Menüeinträge für `update = Some(…)` und `Unavailable`).

### 3.5 Editor (T7)

- `theme.rs`: `Tone::Accent`; `color` → `p.accent`; `pill`: `Accent` → Fläche `accent_soft`, Rand `accent`, Text `p.text` (wie Warn/Danger; Kontrast `text` auf `accent_soft` ≈ 11:1 dunkel, ≈ 15:1 hell). `docs/design-system.md`: Zeile in der Bausteine-/Farbregel ergänzen („`Tone::Accent` – Hinweis mit Aktion, z. B. Update“).
- `dashboard.rs`: Felder `update: Option<update::Version>`, `update_checked: Option<Instant>`, `update_tx: Sender<Result<(), String>>`, `update_rx: Receiver<Result<(), String>>`. Pille direkt nach dem Tray-Pillen-Block (rechts-nach-links-Layout ⇒ links der Tray-Pille), Text **`Update available · 0.2.0`**, `Tone::Accent`, `.interact(Sense::click()).on_hover_text("Click for options")`, Klick toggelt `egui::popup_below_widget(ui, Id::new("update-popup"), &response, PopupCloseBehavior::CloseOnClickOutside, …)` mit `theme::button(ui, "Update now")` und `theme::ghost(ui, "What's new")`; beide schließen das Popup. Log-Texte: `Updater opened in a terminal`, `Updater could not start: {e}`, `Release notes could not open: {e}`.
- `snapshots.rs`: `fn update_available(d: &mut Dashboard) { d.update = Some(Version(0, 2, 0)); }` und `render(&format!("update-{suffix}"), theme, update_available);` in `render_ui_snapshots`. `render_docs_screenshots` unverändert.

### 3.6 `update.json`

`$XDG_CONFIG_HOME/herdctrl/update.json` (Standard `~/.config/herdctrl/update.json`), Modus 0600, nur der Tray schreibt:

```json
{
  "enabled": true,
  "checked_at": 1763215200,
  "latest": "0.2.0",
  "notified": "0.2.0"
}
```

### 3.7 Installer: Schnittstelle

- Aufruf wie Spec 5; Optionen `--version X.Y.Z`, `--archive PATH`, `--check`, `--reinstall`, `--no-packages`, `--extension`, `--no-extension`, `--uninstall`, `--purge`, `-h`/`--help`.
- Exit-Codes: `0` Erfolg/aktuell/nichts zu tun; `1` Fehler; `2` Nutzungsfehler; `10` nur bei `--check`: Update verfügbar oder installierte Version unbekannt. `update.sh` reicht den Code unverändert durch.
- Stamp: `$DATA/herdctrl/installed-version`.
- Umgebung: `XDG_BIN_HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME` (nur absolut, K8), `XDG_RUNTIME_DIR` (Sperre), `XDG_CURRENT_DESKTOP`/`XDG_SESSION_TYPE` (Erweiterung), `DISPLAY`/`WAYLAND_DISPLAY` (Neustart, B8), intern `HERDCTRL_INSTALL_HANDOFF` (K1). **Keine** Variable zum Umleiten der Download-URL (wäre ein Umgehungspfad der Prüfsummen); Tests ersetzen `curl` per `PATH`.
- Feste Texte (stdout, Tests prüfen sie): `Installed: A · Latest: B → Update available. Run: herdctrl-update`, `HerdCtrl X is up to date`, `HerdCtrl X is already installed. Nothing to do.`, `Signature verified (Sigstore)`, `cosign not found: checksum only (see docs/manual.md#verify-a-download)`, `Extension is up to date.`, `Log out and back in to load the new extension.`, `The editor is open; close and reopen it to use X.`, `Start HerdCtrl from the app menu.`, `~/.local/bin is not in your PATH yet; open a new terminal or log out and in` (mit tatsächlichem `$BIN`). Fehler auf stderr mit Präfix `HerdCtrl: `.

### 3.8 Installer-Test (`scripts/installer.test.py`, kein Netz)

- Pflicht: `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_BIN_HOME`, `XDG_RUNTIME_DIR` auf eigene Temp-Verzeichnisse; der Test bricht ab, wenn eines davon unter dem echten `$HOME` liegt. `PATH = <stubs>:/usr/bin:/bin`. `XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`, `WAYLAND_DISPLAY` entfernt, außer im Erweiterungsfall; `DISPLAY=:0` als Dummy, damit der Tray-Neustartpfad läuft (`setsid` ist ein Recorder).
- Stubs: `curl` (Fixtures je URL, unbekannte URL → Exit 22 und Protokoll; `-w %{redirect_url}` → Fake-Tag), `cosign` (Recorder, Exit 0; ein Fall mit Exit 1 → nichts installiert), `sudo` (führt den Rest aus), `apt-get` (Recorder), `dpkg-query` (ein Paket fehlt), `gnome-extensions`, `gnome-shell` (`GNOME Shell 50.0`), `update-desktop-database`, `gtk-update-icon-cache`, `setsid` (Recorder). Fixture-`herdctrl` ist ein Shell-Skript (`--version` → `herdctrl X`); Fixture-Tarballs in Python erzeugt, Ordner `herdctrl-vX-linux-x86_64/` mit `herdctrl`, `desktop/herdctrl.desktop`, `desktop/hicolor/16x16/apps/herdctrl.png`, `install.sh` (Kopie des Repo-`install.sh`, damit keine Übergabe K1 stattfindet, außer im eigenen Übergabe-Fall).
- Zusätzliche Fälle zu Spec 7: Rauchtest schlägt fehl → nichts eingespielt, Stamp unverändert; `--archive` ohne `install.sh` → abgelehnt ohne Ausführung; `--archive` mit 0.1.0-Tarball mit `install.sh` → installiert; Ausführung als „root“ ist nicht testbar ohne root, entfällt; `BIN` mit `$` → Exit 1; `--purge` ohne `--uninstall` → Exit 2; zweiter Installer bei gehaltener Sperre → Fehler; Übergabe an abweichendes `install.sh` des Ziel-Tags (Recorder-Skript); Tray-Stopp mit `bash -c 'exec -a herdctrl sleep 60 --tray'` (gleiche effektive Konfiguration → beendet, `setsid` aufgerufen) und einem mit anderem `XDG_CONFIG_HOME` (läuft weiter, wird am Testende vom Test beendet); ein Prozess `exec -a herdctrl sleep 60` (Editor) läuft weiter und erzeugt den Editor-Hinweis. Keine 16-s-Wartezeit im Test (keine GNOME-Wayland-Umgebung beim Tray-Fall).
- Laufzeit-Ziel < 30 s; aufräumen aller selbst gestarteten Prozesse in `tearDown`.

---

## 4. Wellen und Dateien (ersetzt Spec 9)

**Welle 1** (parallel, getrennte Dateien)

| Task | Dateien | Inhalt | Pflichtpunkte |
|---|---|---|---|
| T1 CLI | `src/cli.rs` (neu), `src/main.rs` | 3.1 | Tests aller Tabellenzeilen |
| T2 Update-Kern | `src/core/update.rs` (neu), `src/core/mod.rs` (`pub mod update;`) | 3.3 | B1, B3 (`load`), B15, K11, K12; Tests ohne Netz |
| T3 Tray-Lebenszyklus | `src/tray.rs`, `Cargo.toml`, `Cargo.lock` | 3.2 | B14, FNV mit Testvektoren, Lock-Diff nur `libc` |
| T4 Skripte | `install.sh`, `update.sh`, `scripts/installer.test.py` (alle neu) | Spec 5 mit B5–B13, B16, K1–K9, 3.7, 3.8 | `bash -n`, Test grün |
| T5 Workflows | `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `scripts/check-x11.py`, `scripts/measure-rss.sh` | Spec 7 mit K14 | Release: `install.sh` in den Tarball und als Asset, `update.sh` als Asset, `SHA256SUMS` über `*.tar.gz *.zip install.sh update.sh`; Release-Notes mit Einzeilern. `check-x11.py`: `env = dict(os.environ, XDG_CONFIG_HOME=directory, HERDCTRL_NO_UPDATE_CHECK="1")`. `measure-rss.sh`: Variable am Startaufruf. |

Prüfung nach Welle 1: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `bash -n install.sh update.sh`, `python3 scripts/installer.test.py`, `shellcheck` falls vorhanden.

**Welle 2** (parallel)

| Task | Dateien | Abhängig von |
|---|---|---|
| T6 Tray-Update | `src/tray.rs` | T2, T3; 3.4, B2, B3, B4, K10 |
| T7 Editor-Pille | `src/ui/theme.rs`, `src/ui/dashboard.rs`, `src/ui/dashboard/snapshots.rs`, `docs/design-system.md` | T2; 3.5, B4, K13 |

Danach entfernt der Orchestrator `#![allow(dead_code)]` aus `src/core/update.rs` (B1), dann die drei Cargo-Prüfungen und `cargo test --locked render_ui_snapshots -- --ignored`; `update-dark.png` und `update-light.png` ansehen, zusätzlich `edit-dark.png` auf Unverändertheit.

**Welle 3** (Doku, parallel) wie Spec 9 (T8 Englisch: `docs/manual.md`, `README.md`, `CHANGELOG.en.md`; T9 Deutsch: `docs/handbuch.md`, `CHANGELOG.md`, `.claude/skills/run-app/SKILL.md`). Zusätzlich in beiden Handbüchern: `update.json` und Stamp in der Dateitabelle, geänderter Lock-Hash (FNV, gleiche Namensform), `herdctrl --version`/`--help`, `herdctrl-update --extension` als Weg zur Erweiterung ohne Quellbaum, `HERDCTRL_NO_UPDATE_CHECK`, Datenschutz (eine Anfrage pro Tag, IP + curl-User-Agent), `--archive` prüft keine Prüfsummen. PRD (bereits eingefügt) und `VALIDATION.md` durch den Orchestrator.

---

## 5. PRD-Abgleich

| Anforderung | Prüfung in diesem Feature | Abweichung |
|---|---|---|
| 2.1 Single executable, Zero-Bloat | Keine HTTP-Abhängigkeit; `libc` schon im Lock; Größe nach Umsetzung messen (`stat -c %s`) | keine |
| 4.4 Tray | Menü erweitert (abgedeckt durch 4.7); SIGTERM-Shutdown | keine |
| 4.6 Grabs freigeben, Timeouts, keine fremden Fenster | B14 (Freigabe bei SIGTERM), B8 (Wartezeit nur ohne Freigabe); Installer bewegt keine Fenster | keine |
| 4.7 Installation und Updates | Spec 5 + B5–B16, K1–K9; Tray-Prüfung K10; `--version` 3.1 | `--archive` (lokale Datei) prüft keine Prüfsummen, wie Shellherd `--package` – in beiden Handbüchern nennen. „verifies every file … before installing anything“ gilt für Downloads. |
| 5 Dateistruktur | `cli.rs`, `update.rs`, `install.sh`, `update.sh` (PRD bereits ergänzt) | keine |
| 6 Größe, Start, RAM | Tray-RSS mit `measure-rss.sh`, Startzeit unverändert (CLI-Parsing vor Renderer) | erst nach Messung als erfüllt nennen |
| AGENTS: keine Blockierung im UI-Thread | B4, K13 | keine |

Abnahme manuell (offene Punkte für `VALIDATION.md`): Frischinstallation in Ubuntu-24.04-VM; Update 0.1.0 → 0.2.0 auf dem Rechner des Benutzers (16-s-Wartezeit, Hotkeys danach, Autostart umgebogen, Erweiterung); Update 0.2.0 → 0.3.0 ohne Wartezeit (SIGTERM-Freigabe); Benachrichtigung, Menüpunkt, Pille, **Update now** aus Tray und Editor; Haken aus → keine Anfrage (z. B. `strace -f -e trace=execve` auf den Tray oder Log).

---

## 6. Minor (ins Ledger, nicht blockierend)

- **M1** Spec-Nummerierung springt von 1 auf 4 (Abschnitte 2 und 3 fehlen); Verweise „Abschnitt 4“ meinen den PRD-Vorschlag. Keine Wirkung auf die Umsetzung.
- **M2** Bestehender Fehler außerhalb des Umfangs: `autostart_entry` (`src/tray.rs:494-503`) schreibt `\$` statt `\\$` (zwei Escape-Ebenen, B11). Betrifft nur Pfade mit `"` `` ` `` `$` `\`; bei Gelegenheit wie B11 lösen.
- **M3** `scripts/install-gnome-extension.sh` ist nie durch shellcheck gelaufen; später einmal prüfen und dann in die CI aufnehmen.
- **M4** Ein während des Updates offener alter Editor (0.1.0, `DefaultHasher`-Lock) und ein neuer Editor erkennen einander nicht und könnten beide `layouts.json` schreiben. Der Installer-Hinweis „close and reopen“ deckt das ab; einmaliger Übergang.
- **M5** Ein offener alter Editor zeigt nach dem Update weiter die Pille (seine eigene Version ist älter); **Update now** meldet dann „Nothing to do“. Hinnehmbar.
- **M6** Die Texte „Run scripts/install-gnome-extension.sh …“ in `src/os/gnome.rs:19` und `src/ui/dashboard.rs:956` passen nicht zu installierten Releases ohne Quellbaum. Nicht in diesem Feature ändern (der Text in `dashboard.rs` steht im Galeriebild *first run*); Handbücher nennen `herdctrl-update --extension`.
