# Freigabestand: Die 16 Kriterien des Produktauftrags

Stand 05.11.2026. Abschnitt Q des Auftrags nennt 16 Bedingungen, unter denen TronicCAD als fertig gilt. Seit [ADR 0004](adr/0004-open-source.md) ist TronicCAD freie Software, die Punkte zu Lizenzen und Verkauf sind angepasst.

## Ergebnis

**TronicCAD ist noch nicht veröffentlicht.** Es gibt keine öffentliche Fassung und keinen Git-Tag für ein Release.

| Stand | Kriterien |
|---|---|
| Belegt | 4, 5, 7 |
| Belegt mit wesentlichen Einschränkungen | 1, 2, 3, 6, 8, 10 |
| Entfallen | 11 |
| Teilweise | 9, 12, 15 |
| Nicht erfüllt | 13, 14, 16 |

„Belegt“ heißt: Es gibt einen Test oder Nachweis, der hier lief. Es heißt nicht, dass ein Kunde oder eine externe Stelle es bestätigt hat. Alle Prüfungen stammen von derselben Stelle, die den Code geschrieben hat.

## Die Kriterien im Einzelnen

| Nr. | Kriterium | Stand | Nachweis | Was fehlt |
|---|---|---|---|---|
| 1 | Kernfunktionen tatsächlich implementiert | Belegt, eingeschränkt | Module in [ARCHITECTURE.md](../ARCHITECTURE.md), End-to-End-Tests je Modul | STEP-Import, mehrteilige Bauteile, Modelle für integrierte Schaltungen, Echtzeit-Zusammenarbeit, erweiterte Prüfungen je Edition. Vollständig: [KNOWN_ISSUES](../reports/KNOWN_ISSUES.md) |
| 2 | Schaltplan- und PCB-Daten konsistent | Belegt, eingeschränkt | `tronic_pcb::compare_with_netlist`, Abgleich im Test, Sperren in `services/cloud/tests/team.rs` | Abweichungen werden gemeldet, nicht verhindert |
| 3 | ERC und DRC zuverlässig | Belegt, eingeschränkt | `crates/tronic-erc`, `tronic-drc`, Gerber gegen einen fremden Parser | **Kein Fertiger hat je Daten dieser Software geprüft.** Keine echte Platine gefertigt. Pinbelegungen nicht gegen Datenblätter geprüft |
| 4 | 3D-Darstellung funktioniert | Belegt | `e2e/sim3d.mjs`, Bildschirmfotos angesehen | Nur unter Windows bedient |
| 5 | Projektdateien zuverlässig gespeichert | Belegt | `tronic-project`, `e2e/recovery.mjs` mit Absturz | – |
| 6 | Cloud-Synchronisierung getestet | Belegt, eingeschränkt | Zufallstest mit Fehlern, `e2e/cloud.mjs`, [cloud.md](specs/cloud.md) | Nur gegen rclone und den eigenen Dienst. Horizontale Skalierung nicht nachgewiesen. Kein laufender Dienst |
| 7 | Teamfunktionen und Rechte getestet | Belegt | `services/cloud/tests/team.rs` mit jeder Rolle gegen jede Aktion, `e2e/team.mjs` | Keine externe Prüfung, siehe 14 |
| 8 | Vier Sprachen vollständig integriert | Belegt, eingeschränkt | Tests auf fehlende Schlüssel, Überlauf je Modul, Handbuch und Website | **Kein Muttersprachler hat die Übersetzungen geprüft.** Sie stammen aus derselben Hand |
| 9 | Lokale und Cloud-KI funktionieren | **Teilweise** | `e2e/ai.mjs`, `e2e/ai-real.mjs` mit Ollama und einem sehr kleinen Modell | **Cloud-KI lief nie gegen einen echten Anbieter.** Das erprobte Modell lieferte keinen brauchbaren Vorschlag, ein stärkeres ist nicht erprobt |
| 10 | Update-System arbeitet sicher | Belegt, eingeschränkt | `e2e/update.mjs`, `e2e/installer.mjs`, [updates.md](specs/updates.md) | Installation nur unter Windows, kein Server, keine zwei echten Fassungen, keine externe Prüfung |
| 11 | Lizenzverwaltung funktioniert | **Entfallen** | Freie Software, ADR 0004 | – |
| 12 | Windows, macOS und Linux erfolgreich getestet | **Teilweise** | Windows 11: alle Prüfungen. Ubuntu 26.04 in WSL: `scripts/verify_linux.py`, alle Rust- und Oberflächentests, Debian-Paket, Installation, Start mit Bildschirmfoto, Entfernen | **macOS wurde nie gebaut oder gestartet.** Unter Linux wurde die Oberfläche angezeigt, aber nicht bedient, und der Lauf war nicht auf echter Hardware. Die CI lief nie, es fehlt ein Remote-Repository |
| 13 | Abhängigkeiten rechtlich geprüft | **Nicht erfüllt** | Automatisch: `cargo deny`, `scripts/licenses.mjs`, 0 Verstöße. [IP_COMPLIANCE.md](../IP_COMPLIANCE.md) | Die Vorgabe verlangt eine nicht nur automatische Prüfung. 43 Komponenten ohne Lizenzdatei sind offen, ebenso der Lizenzweg des Windows-Installers |
| 14 | Sicherheitsprüfungen abgeschlossen | **Nicht erfüllt** | `cargo audit`, `npm audit`, Geheimnis-Suche, Angriffsfälle in Tests | Externer Penetrationstest ([Auftrag](PENTEST_SCOPE.md)), Fuzz-Tests, fachliche CRA-Prüfung ([Vorarbeit](CRA_ASSESSMENT.md)) |
| 15 | Dokumentation und Support vorhanden | **Teilweise** | Handbuch in vier Sprachen in der Anwendung, [SUPPORT.md](../SUPPORT.md), Selbsthosting-Anleitung | **Niemand ist erreichbar, es gibt keine Kontaktadresse.** Das Handbuch ist knapp |
| 16 | Veröffentlichung freigegeben | **Nicht erfüllt** | – | Entscheidung des Auftraggebers, siehe [Prüfliste](LEGAL_REVIEW.md) |

## Zusätzlich aus der Abnahme von Phase 10

| Punkt | Stand |
|---|---|
| Windows-Installer | **Gebaut und geprüft** (`e2e/installer.mjs`), nicht signiert. Windows warnt also |
| macOS-Installer | **Nicht gebaut.** Braucht einen Mac oder die CI, und ein Apple-Entwicklerkonto zum Signieren |
| Linux-Paket | **Debian-Paket gebaut und geprüft,** nicht signiert. AppImage baut, darf aber nicht ausgeliefert werden: Es enthält WebKitGTK unter LGPL, und die Pflichten dazu sind nicht geklärt |
| Website | **Gebaut und geprüft** (`website/check.mjs`), nicht veröffentlicht, und die Seiten halten Suchmaschinen fern. Kein Impressum, keine Datenschutzerklärung, keine Domain |

## Beta-Test

Eine kleine Beta ist **keine kommerzielle Freigabe** und braucht weniger als diese. Das Paket entsteht mit `scripts/build_beta.mjs`, ist nicht signiert und wird von niemandem automatisch verteilt: Anleitung in [BETA_TESTERS.md](BETA_TESTERS.md). Sie braucht keinen Server.

Vor der Weitergabe: CI grün, der Punkt „Beta-Test“ der [Prüfliste](LEGAL_REVIEW.md), und ein Ablageort, auf den nur die Testpersonen Zugriff haben.

## Was bis zur Veröffentlichung zu tun ist

1. Rechteinhaber festlegen und die [Prüfliste](LEGAL_REVIEW.md) abarbeiten.
2. Repository öffentlich stellen, nach ausdrücklicher Entscheidung.
3. CI auf allen Systemen grün, macOS bauen und prüfen, Linux auf echter Hardware bedienen.
4. Eine Platine von einem echten Fertiger herstellen lassen. Pinbelegungen prüfen oder die Warnung beibehalten.
5. Unabhängige Sicherheitsprüfung, soweit gewünscht.
6. Signaturen für die Pakete, Update-Server und Support klären, soweit gewünscht.
