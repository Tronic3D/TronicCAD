# Roadmap

Eine Phase gilt erst als abgeschlossen, wenn alle Abnahmekriterien erfüllt und belegt sind. Alle Funktionen außerhalb des Abschnitts „Stand“ sind **geplant**, nicht vorhanden. Die Editionen sind Produktziele.

## Stand

| Phase | Inhalt | Status |
|---|---|---|
| 0 | Vorbereitung | Technisch erledigt, drei Abnahmepunkte offen |
| 1 | CAD-Grundsystem | In Arbeit |
| 2 | Schaltplan | Technisch erreicht, manuelle Prüfungen offen |
| 3 | PCB | Grundfunktion und Autorouter erreicht, Fertigerprüfung und Teile von J3 offen |
| 4 | 3D und Simulation | Geplant, als Nächstes |
| 5 | KI | Geplant |
| 6 | Cloud | Geplant |
| 7 | Team | Geplant |
| 8 | Kommerzielles System | Geplant |
| 9 | Updates und Sicherheit | Geplant |
| 10 | Veröffentlichung | Geplant |

## Phase 0: Vorbereitung

- [x] Anforderungen analysiert, Architektur und Technologiestack festgelegt ([ARCHITECTURE.md](ARCHITECTURE.md))
- [x] Technologien und Lizenzen technisch geprüft ([IP_COMPLIANCE.md](IP_COMPLIANCE.md))
- [x] Risikobewertung ([docs/RISKS.md](docs/RISKS.md))
- [x] Projektstruktur, Testkonzept ([TESTPLAN.md](TESTPLAN.md)), Sicherheitskonzept ([SECURITY.md](SECURITY.md))
- [x] CI/CD-Konzept und Workflow-Datei
- [ ] CI läuft nachweislich grün. Es fehlt ein Remote-Repository.
- [ ] Fachjuristische Prüfung von Lizenzstrategie und Marke. Das kann nur extern erfolgen.
- [ ] Rechtsträger und Urheber festgelegt ([COPYRIGHT.md](COPYRIGHT.md))

## Phase 1: CAD-Grundsystem

- [x] Tauri-, Rust- und React-Grundgerüst startet (Windows)
- [x] Projekte anlegen, speichern, öffnen, inklusive Integritätsprüfung und Konflikterkennung
- [x] Vier Sprachen, zur Laufzeit umschaltbar, Vollständigkeit automatisch geprüft
- [x] Professionelle Grundoberfläche ohne Layoutüberlauf in allen vier Sprachen bei Standardgröße
- [x] Automatische Tests auf Rust-, UI- und Anwendungsebene
- [x] Automatische Sicherung alle 30 Sekunden und Wiederherstellung nach Absturz ([docs/specs/autosave.md](docs/specs/autosave.md))
- [ ] Projektarchivierung
- [ ] Getestet auf macOS und Linux
- [ ] Übersetzungen von Muttersprachlern geprüft (zh-CN, fr-FR)
- [ ] Barrierefreiheit geprüft

## Phase 2: Schaltplan

Editor, Bauteile, Leitungen, Bibliothek, Netze, ERC. Spezifikation: [docs/specs/schematic.md](docs/specs/schematic.md).

- [x] Datenmodell mit Formatversion 2 und Migration von Version 1 (Test mit echter Format-1-Datei)
- [x] Platzieren, Verbinden, Drehen, Spiegeln, Kopieren, Rückgängig, Raster, Zoom, mehrere Seiten, Hierarchie, Busse, Netznamen, Referenznummerierung, Querverweise. Bis auf Busse und Blockverbindungen, die das Testskript über das Backend anlegt, im End-to-End-Test mit Maus und Tastatur bedient
- [x] Eigene Grundbibliothek mit Herkunft jedes Symbols ([docs/LIBRARY_PROVENANCE.md](docs/LIBRARY_PROVENANCE.md)), alle 16 geforderten Kategorien
- [x] Eigene Bauteile, Bibliotheksdatei für Import und Export
- [x] Netzliste aus definierten Testschaltungen entspricht der Erwartung, auch über Seiten und Blöcke
- [x] ERC erkennt jede Regel in einem eigenen Testfall, drei saubere Referenzschaltungen ohne Fehlalarm, Fehler in der Grafik markiert
- [x] Rendering entschieden ([docs/adr/0001-schematic-rendering.md](docs/adr/0001-schematic-rendering.md)), Messwerte für 10 000 Objekte in [reports/PERFORMANCE.md](reports/PERFORMANCE.md)
- [ ] Pinbelegung der Bibliothek von einer Person gegen Datenblätter geprüft. Das kann ich nicht ersetzen
- [ ] Praxistest durch Elektronikentwickler mit einer echten Schaltung
- [ ] Getestet auf macOS und Linux

**Bewusste Einschränkungen in dieser Phase**, siehe [reports/KNOWN_ISSUES.md](reports/KNOWN_ISSUES.md): keine Mehrfachverwendung eines Blocks, keine mehrteiligen Bauteile, kein grafischer Symboleditor, kein Footprint oder 3D-Modell an den Bauteilen.

## Phase 3: PCB

PCB-Datenmodell, Platzierung, Leiterbahnen, Kupferlagen, DRC. Spezifikation: [docs/specs/pcb.md](docs/specs/pcb.md).

- [x] Rückführung der Schaltplan-Netzliste ohne Abweichung, mit Vergleichsfunktion und Tests
- [x] 1, 2, 4, 6 und 8 Lagen
- [x] Umriss, Platzieren, Leiterbahnen, Vias, Kupferflächen, Bestückungsdruck, Lötstopp, Lotpaste, Bohrungen, Längenanzeige je Netz
- [x] Eigene Footprints mit Herkunftsangabe ([docs/LIBRARY_PROVENANCE.md](docs/LIBRARY_PROVENANCE.md))
- [x] DRC mit einem Testfall je Regel, saubere Platinen ohne Fehlalarm, Fehler in der Grafik markiert
- [x] Gerber und Excellon von einem unabhängigen Parser gelesen und bei allen fünf Lagenzahlen mit der bekannten Platine verglichen
- [ ] **Fertigungsdaten von mindestens einem Fertiger geprüft und eine Testplatine gefertigt.** Kann nur extern erfolgen
- [ ] Maße der Footprints von einer Person gegen Herstellerzeichnungen geprüft
- [ ] Praxistest mit einer echten Platine
- [ ] Getestet auf macOS und Linux

- [x] Automatisches Routing mit Vorschau ([autorouter.md](docs/specs/autorouter.md))

**Aus J3 noch nicht gebaut**, nur spezifiziert: differenzielle Leitungen und Impedanzkontrolle ([high-speed.md](docs/specs/high-speed.md)). Weitere Einschränkungen in [reports/KNOWN_ISSUES.md](reports/KNOWN_ISSUES.md).

## Phase 4: 3D und Simulation

3D-Engine, 3D-Bibliothek, Kollisionsprüfung, Simulation.

Abnahme: DC, AC, Transient und Arbeitspunkt stimmen an Referenzschaltungen mit berechneten Werten überein, Kollisionsprüfung an definierten Testfällen.

**Stand 04.11.2026:** Umgesetzt und getestet. Zwei Abweichungen vom ursprünglichen Plan, begründet in [ADR 0002](docs/adr/0002-3d-rendering.md): Eine eigene Engine ersetzt ngspice, womit die ngspice-Lizenzfreigabe als Kriterium entfällt. **STEP ist nicht umgesetzt**, weil keine Bibliothek lizenzrechtlich freigegeben ist. Der Auftrag fordert STEP nur soweit lizenzrechtlich möglich.

## Phase 5: KI

Lokale KI, Cloud-KI, Provider-Schnittstellen, Schaltungsanalyse, Assistent.

Abnahme: Betrieb ohne Netz nachgewiesen, keine Projektdaten ohne Zustimmung an externe Anbieter (per Test belegt), API-Schlüssel im Schlüsselspeicher des Betriebssystems, jede KI-Änderung mit Vorschau und Freigabe, die KI kann ERC-, DRC- und Simulationsergebnisse nicht setzen.

**Stand 04.11.2026:** Umgesetzt, Nachweise in [docs/specs/ai.md](docs/specs/ai.md). **Nicht vollständig abgenommen:** Der Betrieb ohne Netz ist für die Regelanalyse und den lokalen Weg gezeigt, aber nicht mit getrenntem Netzwerk geprüft. Ein kleines echtes Modell lief lokal über Ollama, hat aber keinen brauchbaren Änderungsvorschlag geliefert. Ein Cloud-Anbieter wurde nie angesprochen. TronicCAD liefert kein Modell aus, die lokale KI braucht einen Server des Benutzers.

## Phase 6: Cloud

Benutzerkonten, Cloudspeicherung, Synchronisierung, externe Anbieter, Backup.

Abnahme: Synchronisierung übersteht Abbruch und Offlinebetrieb, gleichzeitige Änderungen führen nie zu stillem Datenverlust (Tests mit Fehlerinjektion), S3- und WebDAV-Adapter gegen echte Testserver, Lastnachweis für horizontale Skalierung, Sicherheitsprüfung des Backends.

**Stand 04.11.2026:** Umgesetzt, Nachweise in [docs/specs/cloud.md](docs/specs/cloud.md). **Nicht vollständig abgenommen:** Der Lastnachweis für horizontale Skalierung fehlt, auf einem Rechner stieg der Durchsatz mit mehr Instanzen nicht. Die Sicherheitsprüfung ist nur eine Eigenprüfung. Ein laufender Clouddienst existiert nicht, das Betriebsmodell ist offen.

## Phase 7: Team

Teamverwaltung, Rechte, gemeinsame Projekte, Konfliktmanagement, Selbsthosting.

Abnahme: Alle fünf Rollen durch Berechtigungstests abgedeckt, Bearbeitungsbereiche und Sperren halten elektrische und mechanische Daten konsistent, Selbsthosting nach Anleitung auf sauberem System installierbar, Audit-Protokoll vollständig.

**Stand 04.11.2026:** Umgesetzt, Nachweise in [docs/specs/team.md](docs/specs/team.md) und [docs/SELF_HOSTING.md](docs/SELF_HOSTING.md). Zu den Grenzen: „Konsistent“ heißt, Abweichungen zwischen Schaltplan und Leiterplatte werden gemeldet, nicht behoben. Das saubere System ist ein frisches Ubuntu in WSL, kein eigener Server. Echtzeit-Zusammenarbeit im selben Bereich gibt es nicht. Die unabhängige Sicherheitsprüfung aus Phase 6 steht weiter aus.

## Phase 8: Kommerzielles System

**Entfallen.** TronicCAD ist freie Software unter Apache-2.0, ohne Editionen, Aktivierung und Bezahlsystem, siehe [ADR 0004](docs/adr/0004-open-source.md). Die frühere Umsetzung liegt in der Git-Geschichte.

## Phase 9: Updates und Sicherheit

Signierte Updates, Sicherheitsprüfungen, Releasekanäle, Wiederherstellung, Compliance.

Abnahme: Manipulierte, falsch signierte und ältere Updates werden abgelehnt (Tests, siehe Advisory in [SECURITY.md](SECURITY.md)), Backup vor größeren Updates, Schlüsselverwaltung dokumentiert, externer Penetrationstest, CRA-Anwendbarkeit fachlich geprüft.

**Stand 05.11.2026:** Technisch umgesetzt, Nachweise in [docs/specs/updates.md](docs/specs/updates.md) und [docs/RELEASE.md](docs/RELEASE.md). **Zwei Abnahmekriterien sind nicht erfüllt und können es nur extern werden:** der Penetrationstest ([Auftragsbeschreibung](docs/PENTEST_SCOPE.md)) und die fachliche Prüfung des CRA ([Vorarbeit](docs/CRA_ASSESSMENT.md)). **Phase 9 ist deshalb nicht abgenommen.**

## Phase 10: Veröffentlichung

Installer, Dokumentation, Support, Website, Testversion, kommerzielle Freigabe.

Abnahme: Die 16 Kriterien aus Abschnitt Q des Produktauftrags sind einzeln belegt, inklusive signierter Installer auf allen Plattformen und fachlich-rechtlicher Freigabe.

**Stand 05.11.2026: Nicht abgenommen, und die kommerzielle Freigabe ist nicht erteilt.** Die Kriterien sind einzeln bewertet in [docs/RELEASE_READINESS.md](docs/RELEASE_READINESS.md): 3 belegt, 7 belegt mit wesentlichen Einschränkungen, 3 teilweise, 3 nicht erfüllt. Gebaut sind Windows-Installer, Debian-Paket, Handbuch, Support-Ablauf und Website. Es fehlen macOS, jede Signatur, Support mit Kontaktadresse, externe Prüfungen und die Rechtsprüfung. Ein großer Teil davon kann nur außerhalb der Entwicklung entschieden werden.
