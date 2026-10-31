# Repository-Regeln

## Verbindliche Produktgrundlage

- Vor jeder Planung, Implementierung, Änderung und Review zuerst `PRD.md` vollständig lesen. Bei einer neuen Aufgabe oder geänderter PRD erneut lesen.
- Alle Entscheidungen und Abnahmekriterien an `PRD.md` ausrichten. Im Arbeitsbericht die betroffenen Anforderungen, deren Prüfung und offene Abweichungen benennen.
- Bei Widersprüchen, fehlenden Vorgaben oder technisch unerreichbaren Anforderungen die Abweichung ausdrücklich klären; Anforderungen nicht stillschweigend ändern. Explizite Benutzeranweisungen haben Vorrang.
- `PRD.md` nicht beiläufig umschreiben, um eine Implementierung nachträglich passend zu machen. Produktänderungen nur auf ausdrücklichen Auftrag dokumentieren.
- Rust mit `egui`/`eframe` verwenden. Initiale Zielplattform: Ubuntu unter X11. Keine Electron- oder Web-Runtime einführen.
- Laufzeitziele (Binärgröße, Startzeit, Speicher) messen und Ergebnisse dokumentieren; ungemessene Ziele nicht als erfüllt ausgeben.

## Git und Veröffentlichungen

- Niemals direkt auf `main` oder `staging` arbeiten, committen oder pushen; eigenen Feature- oder Fix-Branch verwenden.
- Vor jedem Commit und Push den aktuellen Branch prüfen.
- Nur den eigenen Branch pushen. Pull Requests gegen `staging` richten.
- Merges eigener Branch → `staging` → `main` übernimmt der Benutzer.
- Keine Versionsnummern erhöhen, Tags erstellen oder Releases veröffentlichen, außer auf ausdrücklichen Auftrag.
- Bestehende Änderungen respektieren; keine fremden Änderungen verwerfen.
- Bei einem neuen Repository ohne `staging` keine Basisbranches eigenständig veröffentlichen. Feature-Branch pushen und fehlende PR-Basis melden.

## Validierung

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` und `cargo build --release` ausführen.
- Tests für Koordinaten, Konfiguration und Fensterzuordnung pflegen. X11-Integration separat prüfen und Grenzen offen dokumentieren.
- UI und Launch-Engine trennen. Keine blockierenden Prozessaufrufe oder Polling-Schleifen im UI-Thread.
