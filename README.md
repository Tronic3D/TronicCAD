# TronicCAD

Elektronik-CAD für Schaltplan und Leiterplatte. Rust-Kern, Tauri-Desktop-App, React-Oberfläche.

> **Freie Software unter der [Apache-Lizenz 2.0](LICENSE).** Alle Funktionen sind für alle frei nutzbar, ohne Aktivierung und ohne Konto. **Noch nicht veröffentlicht.** Vollständig geprüft unter Windows 11, unter Ubuntu in WSL getestet und gestartet, unter macOS nur die Rust-Tests. **Die Fertigungsdaten hat noch kein Fertiger geprüft.** Stand je Kriterium: [docs/RELEASE_READINESS.md](docs/RELEASE_READINESS.md).

## Was heute funktioniert

- Projekte anlegen, speichern, speichern unter, öffnen und schließen (`.tronic`, Format in [docs/specs/project-file-format.md](docs/specs/project-file-format.md))
- Atomares Speichern, Sicherungskopie der vorherigen Fassung, Prüfsumme beim Öffnen
- Erkennung externer Dateiänderungen statt stillem Überschreiben
- Schutz ungespeicherter Änderungen beim Schließen
- Automatische Sicherung alle 30 Sekunden, Wiederherstellung nach einem Absturz
- Liste zuletzt verwendeter Projekte
- Sprachen de-DE, en-US, zh-CN und fr-FR, jederzeit umschaltbar und dauerhaft gespeichert
- Schaltplan: Bauteile, Leitungen, Busse, Netzlabel, mehrere Seiten, hierarchische Blöcke, Rückgängig, Kopieren, Spiegeln, Drehen, Referenznummerierung
- Eigene Bauteilbibliothek mit 18 Kategorien, eigene Bauteile, Bibliotheksdateien
- ERC mit Markierung in der Grafik, Netzliste mit Export
- Leiterplatte: 1 bis 8 Lagen, Abgleich mit dem Schaltplan, Platzieren, Leiterbahnen, Vias, Kupferflächen, Luftlinien, Längenanzeige
- Autorouter mit Vorschau und Rückgängig, dessen Ergebnisse den DRC bestehen
- DRC mit Markierung in der Grafik
- Fertigungsdaten: Gerber, Excellon, Stückliste, Bestückungsliste
- Simulation: Arbeitspunkt, DC-Durchlauf, AC und Transient mit eigener Engine, Diagramme, CSV, SPICE-Netzliste zur Kontrolle. Vereinfachte Modelle, ICs ohne Modell werden gemeldet ([Grenzen](docs/specs/simulation.md))
- KI-Assistent: Regelanalyse ohne Modell und Netz, eigener lokaler Server oder Cloud-Anbieter nach Wahl, Zustimmung vor jedem Versand, Schlüssel im Schlüsselspeicher des Betriebssystems, Änderungen nur mit Vorschau und Freigabe. **Kein Modell enthalten, mit einem echten Modell noch nicht geprüft** ([Grenzen](docs/specs/ai.md))
- Synchronisierung und Sicherung: Ordner oder NAS, WebDAV, S3 und eigener Clouddienst, Versionen, Vergleich, Wiederherstellung, Konflikte ohne stillen Datenverlust, offline nutzbar. **Ein laufender TronicCAD-Clouddienst existiert nicht** ([Grenzen](docs/specs/cloud.md))
- Installer für Windows, ohne Administratorrechte und vollständig deinstallierbar. Debian-Paket für Linux. Beide nicht signiert. Handbuch in vier Sprachen in der Anwendung.
- Updates: signiertes Verfahren mit Kanälen, das manipulierte und ältere Updates ablehnt, Sicherung vor größeren Schritten und automatische Rücknahme, wenn eine neue Fassung nicht startet. **Installation nur unter Windows, kein laufender Update-Server.**
- Erweiterungen: eigene Werkzeuge in jeder Sprache ([Anleitung](docs/EXTENSIONS.md)), nur nach Zustimmung und nur in der geprüften Fassung.
- Fremdkomponenten: Alle verwendeten Programme und Quelltexte mit ihren Lizenzen stehen in der Anwendung unter Hilfe.
- Teamarbeit: Teams mit fünf Rollen, Einladungen, Bereiche und Sperren, die der Server am Inhalt prüft, automatisches Zusammenführen getrennter Bereiche, Kommentare, Audit-Protokoll, Teamserver zum Selbstbetrieb ([Grenzen](docs/specs/team.md), [Anleitung](docs/SELF_HOSTING.md))
- 3D-Ansicht: Platte mit Kupfer und Bauteilen, Import von STL, OBJ und WRL, Kollisions-, Abstands- und Höhenprüfung. STEP wird nicht gelesen ([Grenzen](docs/specs/view3d.md))

Vollständig geprüft nur unter Windows 11. Unter Ubuntu 26.04 in WSL laufen alle Rust- und Oberflächentests, und die Anwendung installiert und startet (`scripts/verify_linux.py`), wurde dort aber nicht bedient. macOS ist ungetestet.

## Entwicklung

Voraussetzungen: Rust (stable), Node.js LTS, unter Windows MSVC-Build-Tools und WebView2, unter Linux die [Tauri-Systembibliotheken](.github/workflows/ci.yml).

```bash
cd apps/desktop && npm ci
```

```bash
cd apps/desktop && npx tauri dev
```

Alle Prüfungen ausführen und `reports/QUALITY_REPORT.md` neu erzeugen:

```bash
node scripts/quality.mjs
```

## Struktur

| Pfad | Inhalt |
|---|---|
| `crates/tronic-core` | Domänenmodell, ohne I/O und UI |
| `crates/tronic-project` | Dateiformat, atomares Speichern, Sitzung |
| `crates/tronic-appdata` | Einstellungen und Projektliste in SQLite |
| `crates/tronic-schematic` | Schaltplan-Modell, Befehle, Verbindungen, Netzliste |
| `crates/tronic-library` | Eigene Bauteilbibliothek |
| `crates/tronic-erc` | Elektrische Regelprüfung |
| `crates/tronic-pcb` | Platinenmodell, Befehle, Kupferanalyse, Flächen |
| `crates/tronic-drc` | Designregelprüfung |
| `crates/tronic-router` | Autorouter |
| `crates/tronic-fab` | Gerber, Excellon, Listen |
| `crates/tronic-sim` | Eigene Simulations-Engine |
| `crates/tronic-3d` | 3D-Szene, Modellimport, mechanische Prüfung |
| `crates/tronic-storage` | Speicheranbieter, Synchronisierung, Sicherung |
| `services/cloud` | Clouddienst: Konten, Speicher, Ereignisse |
| `crates/tronic-ai` | KI-Anbieter, Zustimmung, Änderungsvorschläge, Regelanalyse |
| `apps/desktop/src-tauri` | Dünne Tauri-Befehlsschicht |
| `apps/desktop/src` | React-Oberfläche und Übersetzungen |
| `apps/desktop/e2e` | End-to-End-Test gegen die echte Anwendung |
| `scripts` | Lizenz-, SBOM- und Qualitätsberichte |

## Dokumentation

[ROADMAP](ROADMAP.md) · [ARCHITECTURE](ARCHITECTURE.md) · [TESTPLAN](TESTPLAN.md) · [SECURITY](SECURITY.md) · [IP_COMPLIANCE](IP_COMPLIANCE.md) · [COPYRIGHT](COPYRIGHT.md) · [TRADEMARKS](TRADEMARKS.md) · [CONTRIBUTING](CONTRIBUTING.md) · [Risiken](docs/RISKS.md) · [Entwicklungsbericht](reports/DEVELOPMENT_REPORT.md) · [Qualitätsbericht](reports/QUALITY_REPORT.md) · [Messwerte](reports/PERFORMANCE.md) · [Bibliotheksherkunft](docs/LIBRARY_PROVENANCE.md)

## Lizenz

[Apache-2.0](LICENSE), siehe auch [NOTICE](NOTICE). Fremdkomponenten stehen unter ihren eigenen Lizenzen: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
