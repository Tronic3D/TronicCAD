# Sicherheit

## Status

TronicCAD ist in früher Entwicklung und wurde nicht extern geprüft. Es gibt noch keine veröffentlichte Version, keine Netzwerkfunktionen und keine Kontenverwaltung.

## Sicherheitslücken melden

**Offen, vor der ersten externen Weitergabe zu klären:** Es existiert noch keine öffentliche Meldeadresse. Vorgesehen sind `security@`-Adresse, `security.txt` nach RFC 9116 und Bestätigung innerhalb von 3 Werktagen.

## Behandlung

1. Eingang bestätigen und einstufen (CVSS).
2. Ursache untersuchen, Korrektur mit Regressionstest.
3. Kritische Fehler haben Vorrang vor allen Funktionsarbeiten.
4. Veröffentlichung als Sicherheitsupdate, koordinierte Offenlegung.
5. Prüfen, ob eine Meldepflicht besteht (siehe CRA).

## Aktuell umgesetzte Maßnahmen

| Bereich | Maßnahme |
|---|---|
| Speichersicherheit | `unsafe_code = "forbid"` in allen eigenen Crates |
| Web-Oberfläche | Strikte CSP ohne Inline-Skripte und Inline-Styles; im End-to-End-Test ohne Verstöße |
| Rechte der Oberfläche | Tauri-Capabilities auf die genutzten Befehle begrenzt; keine Dateisystem- oder Shell-Rechte für die Seite |
| Dateien | Prüfsumme, Größenlimit und Validierung beim Öffnen, beschädigte Dateien werden abgelehnt |
| Datenintegrität | Atomares Schreiben, Sicherungskopie, Erkennung externer Änderungen |
| Abhängigkeiten | `cargo audit`, `npm audit`, `cargo deny` in CI und Qualitätsbericht; nur crates.io als Quelle |
| Geheimnisse | `.gitignore` sperrt Schlüsseldateien |
| Datenschutz | Keine Telemetrie, keine Netzwerkverbindungen im aktuellen Stand. Automatische Sicherungen liegen unverschlüsselt im lokalen Benutzerprofil und werden nach dem Speichern oder Verwerfen gelöscht |

**Grenzen:** Die Prüfsumme erkennt versehentliche Beschädigung und einfache Änderungen, sie ist keine Signatur. Wer die Datei gezielt ändert, kann sie neu berechnen. Projektdateien sind unverschlüsselt.

## Verbindliche Anforderungen für spätere Phasen

| Bereich | Anforderung | Phase |
|---|---|---|
| API-Schlüssel, Tokens | Nur im Schlüsselspeicher des Betriebssystems, nie im Klartext. Für KI-Anbieter umgesetzt, unter Windows per Test belegt | 5, 6 |
| KI | Projektdaten erst nach ausdrücklicher Zustimmung an externe Anbieter, im Backend erzwungen und per Test belegt. Nur https nach außen, keine Weiterleitungen. Prompt-Injection ist nicht verhindert, nur im Schaden begrenzt ([Spezifikation](docs/specs/ai.md)) | 5 |
| Kommunikation | Klartext nur zu diesem Rechner, nach außen nur https, keine Weiterleitungen. **Der Clouddienst selbst spricht kein TLS**, dafür ist ein Proxy nötig. Anmeldung mit Passwort, OAuth, OIDC und Passkeys fehlen | 6 |
| Rechte | Serverseitig durchgesetzt, deny by default, Audit-Protokoll | 7 |
| Zahlungen | Nur über Zahlungsdienstleister, keine Kartendaten in eigenen Systemen | 8 |
| Updates | Signatur, Integrität, Version und Plattform prüfen, Schutz vor Downgrade | 9, umgesetzt, [Spezifikation](docs/specs/updates.md) |
| Signaturschlüssel | Getrennt vom Quellcode, möglichst Hardware-Schlüssel, Rotationsplan | 9, [dokumentiert](docs/RELEASE.md). Kein Hardware-Schlüssel |
| Telemetrie | Nur Opt-in, minimal, dokumentiert | Bei Einführung |

### Hinweis zum Tauri-Updater

[CVE-2026-95625](https://osv.dev/vulnerability/CVE-2026-95625) beschreibt, dass die Signatur des Tauri-Updater-Plugins nur die Binärdaten abdeckt und nicht die Versionsnummer, was ein erzwungenes Downgrade ermöglichen kann. **Der Tauri-Updater wird nicht verwendet.** Das eigene Verfahren signiert Version, Kanal und Plattform mit, und ein Test spielt ältere Pakete mit gültiger Signatur ein.

## Regulatorik

> Keine Rechtsberatung. Die Anwendbarkeitsprüfung muss fachlich erfolgen.

**Cyber Resilience Act (Verordnung (EU) 2024/2847):** Software, die in der EU vertrieben wird, dürfte als Produkt mit digitalen Elementen in den Anwendungsbereich fallen. Die Meldepflichten für aktiv ausgenutzte Schwachstellen und schwere Vorfälle (24 h, 72 h, 14 Tage) gelten seit dem 11. September 2026 für Produkte auf dem Markt. Die vollständigen Anforderungen einschließlich CE-Kennzeichnung gelten ab dem 11. Dezember 2027. Die Kommission hat am 27. Juli 2026 Leitlinien veröffentlicht. Da TronicCAD voraussichtlich erst danach erscheint, sollte es von Anfang an CRA-konform geplant werden: SBOM (vorhanden), Schwachstellenprozess (oben, noch ohne Meldeadresse), Supportzeitraum, sichere Standardeinstellungen, technische Dokumentation, Konformitätsbewertung.

**DSGVO:** Relevant ab Benutzerkonten, Cloud, Telemetrie und KI-Anbietern. Für Phase 6 vorgesehen: Datenschutzfolgenabschätzung prüfen, Auftragsverarbeitung, Löschkonzept, Datenstandort, Datenschutzerklärung.

Quellen: [Jones Day zum CRA](https://www.jonesday.com/en/insights/2026/07/eu-cyber-resilience-act-24hour-reporting-duties-start-september-11-2026), [McCann FitzGerald](https://www.mccannfitzgerald.com/knowledge/data-privacy-and-cyber-risk/cyber-resilience-act-reporting-obligations-apply-from-11-september-2026), [DataGuard zur CRA-Zeitleiste](https://www.dataguard.com/cyber-resilience-act/timeline/)

## Clouddienst: Eigenprüfung

Stand 04.11.2026, Phase 6. **Das ist eine Durchsicht durch den Autor des Codes, keine unabhängige Prüfung.** Ein Penetrationstest durch Dritte steht aus und ist vor einem Betrieb nötig.

**Mit Tests belegt** (`services/cloud/tests/api.rs`): kein Zugriff auf fremde Projekte beim Lesen, Auflisten, Schreiben und Freigeben; Freigaben gewähren genau das erteilte Recht; Abmelden und Ablauf beenden Token, auch auf offenen WebSocket-Verbindungen; Passwörter nur als Argon2id, Token nur als Hash in der Datenbank; begrenzte Fehlversuche beim Anmelden; gleiche Antwort für unbekannte Konten und falsche Passwörter; Pfade und SQL im Eingabetext ohne Wirkung; gespeicherte Objekte nicht ersetzbar; große Uploads erst nach der Anmeldung; Kontingent auch bei gleichzeitigen Uploads; Kontolöschung entfernt auch die Inhalte.

Rollen und Sperren sind mit `services/cloud/tests/team.rs` geprüft: jede Rolle gegen jede Aktion, Versuche der Rechteausweitung, gefälschte Versionen. Die Eigenprüfung für Phase 7 ist genauso wenig unabhängig wie die für Phase 6.

**Bekannte Schwächen, nicht behoben:**

| Schwäche | Folge | Abhilfe |
|---|---|---|
| Keine E-Mail-Bestätigung und kein Zurücksetzen des Passworts | Fremde Adressen lassen sich belegen, ein vergessenes Passwort beendet den Zugang | Mailversand nötig |
| Registrierung und Freigabe verraten, ob eine Adresse ein Konto hat | Konten lassen sich ermitteln | Mit E-Mail-Bestätigung lösen |
| Fehlversuche zählen je Adresse | Ein Angreifer kann ein Konto für 15 Minuten aussperren. Kein Schutz über viele Adressen hinweg | Begrenzung je Absender am Proxy |
| Keine Begrenzung der Anfragen außerhalb der Anmeldung | Angemeldete Benutzer können Last erzeugen und beliebig viele leere Projekte anlegen. 16 gleichzeitige Transfers bis 600 MB können viel Speicher belegen, langsame Uploads die Plätze blockieren | Grenzen am Proxy, Streaming statt Speicher |
| Wer eine Projekt-ID zuerst benutzt, wird Eigentümer | Wer die zufällige ID kennt, kann den Eigentümer blockieren | ID-Vergabe durch den Server |
| Datenbankverbindung ohne TLS | Nur im privaten Netz vertretbar | TLS für PostgreSQL ergänzen |
| Inhalte liegen unverschlüsselt im Objektspeicher | Wer Speicher oder Dienst kontrolliert, liest alle Projekte | Ende-zu-Ende-Verschlüsselung bewerten |
| Token gelten 90 Tage, kein Passwortwechsel | Ein gestohlener Token bleibt lange gültig | Passwortwechsel beendet alle Sitzungen |
| Audit-Protokoll liegt in derselben Datenbank | Wer die Datenbank voll kontrolliert, kann die Hash-Kette neu berechnen | Kopie außerhalb, zum Beispiel an ein Protokollsystem |
| Der Server parst hochgeladene Projektdateien, um Bereiche zu vergleichen | Eine böswillige Datei trifft den Parser auf dem Server, und große Dateien belasten ihn. Nur für Mitglieder eines Teams erreichbar | Fuzzing vor Phase 10, Grenzen |
| Einem Speicher wird beim Auflisten vertraut, was die Menge angeht | Ein böswilliger Server kann die App mit endlosen Listen beschäftigen. Inhalte prüft die App trotzdem | Grenzen ergänzen |

## Stand der Prüfungen

**Automatisch in der Qualitätsprüfung:** `cargo audit`, `cargo deny`, `npm audit`, Suche nach Schlüsseln im Repository, Angriffsfälle in den Tests von Lizenzen, Updates, Rollen und Audit.

**Noch nie durchgeführt:** Eine unabhängige Prüfung. Alle Prüfungen stammen von derselben Stelle, die den Code geschrieben hat, und übersehen deshalb vermutlich dieselben Dinge. Es gibt außerdem keine Fuzz-Tests der Parser und keine statische Analyse über Clippy hinaus. Auftragsbeschreibung: [docs/PENTEST_SCOPE.md](docs/PENTEST_SCOPE.md).

**Telemetrie:** Gibt es nicht. Die Anwendung sendet keine Nutzungsdaten. Sollte das je kommen, dann nur mit ausdrücklicher Einwilligung.

## Erweiterungen

Eine Erweiterung ist Programmcode mit den Rechten der angemeldeten Person und nicht abgeschottet. Sie läuft erst nach Zustimmung und nur in der Fassung, für die zugestimmt wurde, siehe [docs/EXTENSIONS.md](docs/EXTENSIONS.md). Das Update-Verfahren bleibt signiert.

