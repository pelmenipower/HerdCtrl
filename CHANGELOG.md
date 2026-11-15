# Änderungen

Alle nennenswerten Änderungen an HerdCtrl. English version: [CHANGELOG.en.md](CHANGELOG.en.md). Die Versionsnummer folgt [Semantic Versioning](https://semver.org/lang/de/). Versionen und Releases legt nur der Benutzer fest; neue Einträge kommen bis dahin unter **Unveröffentlicht**.

## [Unveröffentlicht] – 0.1.0

Erste Fassung, entwickelt auf `feature/initial-app` vom 2026-10-31 bis 2026-11-10.

### Editor

- Neu: Views mit Zonen auf einer maßstabsgetreuen Vorschau aller Monitore zeichnen, verschieben, in der Größe ändern und auf andere Monitore ziehen. Zonen rasten an Rändern, Hälften, Dritteln, Vierteln und Nachbarzonen ein, ohne sich zu überlappen. **Align edges** räumt auf, Überlappungen werden markiert statt umgeschrieben.
- Neu: Anwendungsauswahl aus den installierten Desktop-Anwendungen mit Suche und Tastaturbedienung. Bereits genutzte Anwendungen stehen oben, Terminals sind markiert, **Custom command…** bleibt erreichbar.
- Neu: Programme im Terminal (zum Beispiel `herdr` in Ptyxis), eigene Befehle mit **Run in terminal** und ein brauchbares Standard-Terminal über `xdg-terminal-exec` oder bekannte Terminals.
- Neu: Hotkeys durch Drücken der ganzen Kombination aufnehmen, Super inklusive. Kombinationen ohne Modifikator werden abgelehnt.
- Neu: Oberfläche „Werkbank“ mit dunklem, hellem und System-Thema, eingebetteten Schriften Inter und JetBrains Mono, Statusanzeigen im Kopf, Zonenliste in Startreihenfolge, Eigenschaften-Panel, Checkliste beim ersten Start, Speichern mit Strg+S und Nachfrage beim Schließen.
- Neu: **Identify monitors** zeigt die Monitornummern auf den Bildschirmen. **Duplicate view** kopiert eine View ohne Hotkey, und **Delete view** fragt vorher nach.

### Starten und Fenster

- Neu: Offene Fenster werden wiederverwendet statt doppelt gestartet. Mehrere Zonen derselben Anwendung bekommen je ein eigenes Fenster, nie nach Fenstertitel geraten. Neue Fenster laufen über die Neues-Fenster-Aktion des Desktop-Eintrags.
- Neu: Fortschritt je Zone (*opening…*, *placed*, *failed*), **Retry zone** für fehlgeschlagene Zonen und ein Log. Fehlermeldungen nennen Anwendung und Timeout.
- Neu: Neu geöffnete Fenster werden nach 0,25, 0,75 und 1,75 Sekunden erneut platziert, weil Programme wie Ptyxis oder Zen ihre gemerkte Größe nachträglich setzen.
- Neu: Unter GNOME füllen Zonen den Arbeitsbereich, obere Leiste und Dock bleiben frei.
- Behoben: Programme, die sich selbst aktualisiert haben (Programmdatei *(deleted)*), werden wieder erkannt.
- Behoben: Starts derselben Anwendung laufen nacheinander, damit sich Zonen nicht gegenseitig Fenster wegnehmen.

### Speicher

- Neu: Der Editor zeichnet mit Vulkan und öffnet nur den Treiber der verbauten GPU. Er braucht damit 28,8 bis 30,5 statt 77,3 MB und ist nach 30 statt 59 ms sichtbar (Intel-GPU, GNOME 50). Die eingebauten egui-Schriften entfallen; drei Symbole sind dafür ersetzt oder gezeichnet. Auf Laptops mit zwei GPUs bleibt die dedizierte im Ruhezustand. Ohne nutzbares Vulkan fällt er von selbst auf OpenGL zurück (dann etwa 85 MB); `HERDCTRL_RENDERER=glow` erzwingt das. Gestartete Programme erben die Treiberauswahl des Editors nicht. Die Programmdatei wächst auf 9,2 MB.

### Tray und Hotkeys

- Neu: `herdctrl --tray` hält die globalen Hotkeys mit etwa 2 MB eigenem Speicher, auch wenn der Editor geschlossen ist. Das Menü startet Views und öffnet den Editor, außerdem schaltet es **Start at login** und beendet HerdCtrl. Konfigurationsänderungen übernimmt der Tray nach etwa einer Sekunde, Fehler meldet er als Desktop-Benachrichtigung.
- Behoben: Ohne Tray-Host (zum Beispiel bei gesperrtem Bildschirm) wuchs der Speicher des Trays mit jedem Neuversuch, das Symbol anzumelden (beobachtet: von 1,8 auf 19 MB eigenen Speicher während einer Stunde Bildschirmsperre). Der Tray wartet jetzt auf einen Host, statt das Symbol immer neu anzumelden.

### GNOME 50 / Wayland

- Neu: Mitgelieferte GNOME-Shell-Erweiterung mit versionierter D-Bus-Schnittstelle für Monitore, Arbeitsbereich, Fenster und globale Kürzel. HerdCtrl spricht sie direkt über D-Bus an, ohne `gdbus` zu starten.

### Doku und Prüfung

- Neu: Release-Workflow für Tags `v*`. Er baut die Linux-Dateien, signiert `SHA256SUMS` schlüssellos mit Sigstore, legt eine GitHub-Build-Bescheinigung an und erstellt nur einen Entwurf, den der Benutzer veröffentlicht. Die Prüfung beschreiben beide Handbücher.
- Doku: README nach Shellherd-Vorbild mit Galerie, ausführliches Handbuch auf Deutsch und Englisch, Designsystem, Anleitung für die Screenshots, `CLAUDE.md` sowie die Skills `run-app` und `feature-pipeline`.
- Tests: Bedienabläufe mit `egui_kittest` ohne GPU. Dazu kommen Screenshots auf Abruf, eine X11-Integration unter Xvfb und Openbox in der CI (jetzt mit `xwininfo` gemessen), ein Rust↔GJS-Protokolltest und Live-Tests für GNOME.
- Messung: siehe Abschnitt *Speicher*; der Tray braucht 1,8 MB eigenen Speicher. Details in [VALIDATION.md](VALIDATION.md).
