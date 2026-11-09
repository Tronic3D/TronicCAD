# Testplan

Ergebnisse stehen nur im generierten [reports/QUALITY_REPORT.md](reports/QUALITY_REPORT.md), nicht hier.

## Ebenen

| Ebene | Werkzeug | Ort | Stand |
|---|---|---|---|
| Unit | `cargo test` | `crates/*/src`, `src-tauri/src` | Eingerichtet |
| Integration, echtes Dateisystem | `cargo test` | `crates/tronic-project/tests` | Eingerichtet |
| UI, Backend gestubbt | vitest, Testing Library | `apps/desktop/src/*.test.tsx` | Eingerichtet |
| Übersetzung | vitest | `apps/desktop/src/i18n/locales.test.ts` | Eingerichtet |
| End-to-End, echte Anwendung | playwright-core über CDP | `apps/desktop/e2e/smoke.mjs`, `schematic.mjs`, `recovery.mjs`, `pcb.mjs`, `router.mjs`, `sim3d.mjs`, `ai.mjs`, `cloud.mjs`, `team.mjs` | Eingerichtet, nur Windows |
| Regression | Jeder behobene Fehler bekommt einen Test | überall | Eingerichtet |
| Performance | Messlauf mit 10 000 Objekten | `apps/desktop/e2e/schematic.mjs`, [reports/PERFORMANCE.md](reports/PERFORMANCE.md) | Messung vorhanden, ohne Grenzwert in der CI |
| Speicher | geplant: Langlauf mit Speicherprofil | - | **Nicht eingerichtet** |
| Fuzzing des Dateiformats | geplant: cargo-fuzz | - | **Vor Phase 10** |
| Fertigungsdaten, fremder Parser | gerbonara | `scripts/verify_gerber.py`, `scripts/parse_gerber.py` | Eingerichtet, nur Entwicklungswerkzeug |
| Simulation, fremder Simulator | ngspice 47 | `crates/tronic-sim/examples/sim_decks.rs`, `scripts/verify_sim.py` | Eingerichtet, nur Entwicklungswerkzeug, läuft mit `TRONICCAD_NGSPICE` |
| Sicherheit | cargo audit, npm audit | CI, `scripts/quality.mjs` | Eingerichtet |
| Lizenzen | cargo deny, `scripts/licenses.mjs` | CI, `scripts/quality.mjs` | Eingerichtet |

## Was der End-to-End-Test abdeckt

Er startet die gebaute Anwendung und bedient sie über WebView2. Geprüft wird unter anderem: Projekt auf Pfad mit Umlauten und chinesischen Zeichen anlegen, Inhalt auf der Platte, alle vier Sprachen ohne Layoutüberlauf, Speichern mit Sicherungskopie, externe Änderung, manipulierte Datei, Neustart mit erhaltener Sprache und Liste, Fensterschließen mit ungespeicherten Änderungen, keine Konsolen- oder CSP-Fehler.

**Grenzen:**

- Das Auswählen im nativen Dateidialog ist ersetzt, weil es sich aus der Seite nicht bedienen lässt. Geprüft wird nur, dass der Dialog erlaubt ist und sich öffnet. Der tatsächliche Dateiauswahl-Ablauf braucht einen manuellen Test.
- Die Tests laufen mit eigenem Datenordner (`TRONICCAD_DATA_DIR`) und berühren die Einstellungen des Benutzers nicht. Nur der WebView2-Datenordner ist gemeinsam.
- Der Überlauf-Test läuft nur bei Standardfenstergröße.

Der Schaltplan-Test `schematic.mjs` zeichnet mit echten Mausklicks und Tastendrücken und prüft das Ergebnis über das Backend und die Datei. **Direkt über das Backend, nicht über die Oberfläche**, legt er an: das zweite globale Label, den Inhalt des Blocks und seine Anschlussleitung, den Bus. Das Mausrad wird nur im Messlauf benutzt.

Der Absturz-Test `recovery.mjs` beendet die echte Anwendung hart per `taskkill /F` und startet sie neu, einmal wartet er auf den echten 30-Sekunden-Zeitgeber. Das Verhalten bei zwei gleichzeitig laufenden Anwendungen ist nur auf Rust-Ebene getestet. Ein echter Stromausfall wurde nicht geprüft.

Der Platinen-Test `pcb.mjs` legt den Schaltplan über das Backend an, weil das schon `schematic.mjs` abdeckt. Umriss, Platzieren, Drehen, Routing über Vias, Kupferfläche, Regeln und Export bedient er mit Maus und Tastatur. Für den Messlauf erzeugt er die große Platine über das Backend. Das Parsen der exportierten Dateien läuft nur, wenn `TRONICCAD_VERIFY_PYTHON` gesetzt ist.

Der Router-Test `router.mjs` baut einen echten 555-Blinker vom Schaltplan an. Die Platzierung setzt er über das Backend, den Router bedient er über die Oberfläche. Der Router-Benchmark (`cargo run --release -p tronic-router --example route_bench`) läuft nicht in der CI, weil er mehrere Minuten dauert.

Der Test `sim3d.mjs` vergleicht Simulationsergebnisse mit von Hand gerechneten Werten und liest in der 3D-Ansicht echte Pixel aus dem Grafikspeicher. Damit prüft er, dass sich das Bild mit Ansicht, Kupfer und Modell ändert. Ob es richtig aussieht, zeigen nur die Screenshots in `e2e-artifacts`, die bei einer Abnahme anzusehen sind.

Der Test `ai.mjs` spricht mit einem aufzeichnenden Server auf diesem Rechner, nicht mit einem Modell. Er belegt, was gesendet wird und was nicht, sagt aber nichts über die Qualität von Antworten. Den Zustimmungstest führt er gegen eine Netzadresse dieses Rechners, die als extern gilt. Ohne Netzadresse entfallen diese Schritte und das Ergebnis vermerkt es. Ein echter lokaler Server lässt sich mit `cargo run -p tronic-ai --example local_check -- <Adresse> <Modell>` prüfen, die ganze Anwendung damit über `TRONICCAD_REAL_MODEL=<Modell> node e2e/ai-real.mjs`. Dieser Lauf gehört nicht zur Qualitätsprüfung, seinen Bericht in `e2e-artifacts/ai-real-report.md` liest ein Mensch.

Der Test `cloud.mjs` startet zwei Instanzen der Anwendung wie zwei Rechner, mit eigenen Einstellungen und Projektdateien. WebDAV und S3 laufen gegen rclone (`TRONICCAD_RCLONE`), der Clouddienst gegen PostgreSQL (`TRONICCAD_TEST_DATABASE_URL`). Fehlt eine dieser Angaben, entfallen die Teile und der Qualitätsbericht vermerkt es. Rclone und PostgreSQL sind nur Entwicklungswerkzeuge. Die Lastmessung läuft mit `cargo run --release -p tronic-cloud --example load`.

Der Test `team.mjs` startet zwei Instanzen mit eigenen Konten gegen den echten Dienst. Die Rollen prüft `services/cloud/tests/team.rs` vollständig, jede Rolle gegen jede Aktion. `scripts/verify_selfhost.py` führt die Befehle der Anleitung wörtlich auf einer frischen Ubuntu-Kopie aus. Das braucht WSL und mehrere Minuten und gehört daher nicht zur Qualitätsprüfung, sondern zu jeder Änderung an Anleitung oder Dienst.

**Der fremde Parser beweist nicht, dass ein Fertiger die Daten akzeptiert oder die Platine funktioniert.** Er zeigt, dass eine unabhängige Implementierung die Dateien so liest, wie sie gemeint sind.

## Manuelle Prüfung vor jeder Phasenabnahme

1. Neues Projekt über den echten Dialog anlegen, schließen, wieder öffnen.
2. Auf einem Netzlaufwerk speichern und öffnen.
3. Bei minimaler Fenstergröße (960 × 600) alle Sprachen durchsehen.
4. Bildschirmskalierung 150 %.

## Regeln

- Tests beschreiben Verhalten, nicht Implementierung.
- Ein gemeldeter Fehler bekommt zuerst einen fehlschlagenden Test, dann die Korrektur.
- Testergebnisse werden nur aus echten Läufen berichtet. Nicht Ausgeführtes wird als nicht ausgeführt benannt.
- Automatische Quellcodeänderungen laufen nur über einen Branch mit nachvollziehbaren Tests.

## Fehlerablauf

Erkennen → dokumentieren in `reports/KNOWN_ISSUES.md` → Ursache untersuchen → Lösung vorschlagen → umsetzen → Tests erneut ausführen → Ergebnis dokumentieren.

## Updates

`crates/tronic-update` prüft alle Regeln ohne Netz. `apps/desktop/e2e/update.mjs` startet eine Kopie der echten Anwendung gegen einen lokalen Server, der sich falsch verhält, mit dem echten Signierwerkzeug: geänderte Version, fremder Schlüssel, ältere und gleiche Version mit gültiger Signatur, anderer Kanal und andere Plattform, abgelaufenes und wiederholtes Manifest, unverschlüsselte Adresse, verändertes, gekürztes und verlängertes Paket. Danach tatsächlicher Austausch der Programmdatei mit Neustart, Rückkehr auf Wunsch und automatische Rücknahme bei Programmen, die nicht starten.

`scripts/secrets_check.mjs` läuft in der Qualitätsprüfung. `scripts/check_release_binary.mjs` gehört zum Veröffentlichen, weil es eine Release-Fassung braucht.

## Veröffentlichung

`apps/desktop/e2e/installer.mjs` installiert den echten Windows-Installer für diesen Benutzer in einen Testordner, startet das installierte Programm, aktualisiert es dort und deinstalliert es. Ist TronicCAD wirklich installiert, läuft dieser Test nicht, um nichts zu verändern. `website/check.mjs` lädt alle Seiten in einem echten Browser, auf Desktop- und Telefonbreite.

`scripts/verify_linux.py` läuft von Hand, weil es rund 15 Minuten braucht und ein sauberes WSL-Abbild verlangt: auf frischem Ubuntu alle Rust- und Oberflächentests, Debian-Paket und AppImage bauen, installieren, auf einem virtuellen Bildschirm starten, Bildschirmfoto, entfernen. Ein einfarbiges Bild gilt als Fehler, nachdem der erste Lauf mit laufendem Prozess nur Schwarz zeigte.

## Erweiterungen und Fremdkomponenten

`crates/tronic-extensions` prüft Beschreibungen, Antworten, Zeitlimit und Dateinamen. `apps/desktop/e2e/extensions.mjs` führt das mitgelieferte Beispiel aus, prüft die nötige Zustimmung und die erneute Zustimmung nach einer Änderung, und ob die Anwendung alle Fremdkomponenten mit Lizenztexten nennt.

