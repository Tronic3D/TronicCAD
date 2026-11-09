# Spezifikation: Updates

Stand 05.11.2026, Phase 9. Code: `crates/tronic-update`, `apps/desktop/src-tauri/src/update.rs`, `apps/desktop/src/update`. Ablauf einer Veröffentlichung: [RELEASE.md](../RELEASE.md).

**Es gibt keinen Update-Server und keine veröffentlichte Fassung.** Eine Fassung ohne eingebaute Adresse und ohne Schlüssel sucht nicht nach Updates und sagt das. Der ganze Weg lief nur gegen einen lokalen Testserver.

**Direkt installieren geht nur unter Windows, und nur dort ist es geprüft.** Auf macOS und Linux kann die Anwendung Updates finden und anzeigen, installiert aber nichts und sagt das auch.

## Warum ein eigenes Verfahren

[CVE-2026-95625](https://osv.dev/vulnerability/CVE-2026-95625) beschreibt, dass die Signatur des Tauri-Updaters nur die Binärdaten abdeckt, nicht die Versionsnummer. Das erlaubt ein erzwungenes Downgrade. Hier deckt die Signatur alles ab, was entscheidet.

## Manifest

Je Kanal eine Datei `<url>/stable.json`, `beta.json` oder `developer.json`: die exakten signierten Bytes und eine Ed25519-Signatur. Inhalt: Format, Kanal, laufende Nummer, Ausstellungs- und Ablaufzeit, Version, Art (`security` oder `feature`), Änderungsprotokoll je Sprache, und je Plattform Adresse, Größe und SHA-256. Unbekannte Felder werden abgelehnt.

## Was geprüft wird

| Prüfung | Abgewehrt wird | Fehlercode |
|---|---|---|
| Signatur gegen die eingebauten Schlüssel, bevor etwas gelesen wird | Manipulation, falscher Schlüssel | `update.badSignature` |
| Version größer als die laufende (Semantic Versioning) | Downgrade mit gültiger Signatur | wird nicht angeboten |
| Kanal | Manifest eines anderen Kanals | `update.wrongChannel` |
| Laufende Nummer nicht kleiner als die zuletzt gesehene | Einspielen eines alten, gültigen Manifests | `update.replayed` |
| Ablaufzeit | Zurückhalten neuer Updates durch ein altes Manifest | `update.expired` |
| Plattform | Paket für ein anderes System | `update.wrongPlatform` |
| Adresse: https, http nur zu diesem Rechner | Ungesicherter Download | `update.badAddress` |
| Größe und SHA-256 des Downloads, vor der Installation noch einmal | Verändertes, gekürztes, verlängertes Paket | `update.wrongContent` |

Die Größe begrenzt das Lesen: Mehr als signiert wird nicht geladen. Weiterleitungen werden nicht verfolgt. Ein fehlgeschlagener Check verwirft ein vorheriges Angebot.

## Installation

1. **Unsaved Arbeit** verhindert die Installation (`update.saveFirst`).
2. **Sicherung** vor einem größeren Schritt, also bei neuer Haupt- oder Nebenversion: Kopie der Einstellungen und aller bekannten Projektdateien, höchstens 2 GB. Schlägt sie fehl, wird nichts installiert. Die letzten drei bleiben erhalten.
3. **Austausch:** Das laufende Programm wird umbenannt in `troniccad.previous`, das neue tritt an seine Stelle.
4. **Probe:** Das alte Programm startet das neue und wartet bis zu 60 Sekunden, ob dessen Oberfläche sich meldet. Es beendet sich erst danach.
5. **Rücknahme:** Beendet sich das neue vorher oder meldet sich nicht, stellt das alte Programm sich selbst wieder her und merkt sich die fehlgeschlagene Version.

**Zurück auf Wunsch:** Die vorherige Fassung bleibt auf dem Rechner, und die Anwendung kann zu ihr wechseln. Das ist der einzige Weg zurück, und er braucht keinen Server.

## Einstellungen und Datenschutz

Die automatische Suche ist **eingeschaltet und abschaltbar**. Sie ruft einmal täglich eine öffentliche Datei ab und sendet nichts über Nutzer oder Projekte. Der Server sieht wie bei jedem Abruf die IP-Adresse. **Installiert wird nur nach einem Klick**, weil ein Neustart mitten in der Arbeit stören würde. Ob das den CRA erfüllt, ist offen, siehe [CRA_ASSESSMENT.md](../CRA_ASSESSMENT.md). Telemetrie gibt es nicht.

## Grenzen

- Die Probe erkennt, dass die Oberfläche startet, nicht ob die Fassung sonst fehlerfrei ist.
- Fällt der Strom nach dem Austausch und vor der Probe aus, gibt es keine automatische Rücknahme. Die alte Fassung liegt dann als `.previous` neben dem Programm.
- Die Rücknahme stellt das Programm wieder her, nicht die Daten. Die Sicherung liegt im Ordner und wird von Hand zurückgeholt. **Gezielt nicht geprüft** ist, wie eine ältere Fassung Einstellungen und Projektdateien einer neueren behandelt, weil es erst eine Fassung gibt.
- Der Installer legt das Programm für den angemeldeten Benutzer ab, dort geht der Austausch ohne Administratorrechte (`e2e/installer.mjs`). Wer es von Hand in einen geschützten Ordner wie `C:\Program Files` legt, bekommt `update.notWritable`.
- Es gibt keine Windows-Codesignatur, das System warnt deshalb.
- Wer auf dem Rechner mit denselben Rechten läuft, kann das Programm ohnehin ersetzen. Dagegen hilft das Verfahren nicht.
- Ein Angreifer mit dem Release-Schlüssel kann alles signieren, siehe [RELEASE.md](../RELEASE.md).
- Sicherheitsupdates sind gekennzeichnet, aber ein Kanal nennt nur die neueste Version. Ein Sicherheitsupdate getrennt von neuen Funktionen verlangt also einen eigenen Release-Zweig.

## Abnahme und Nachweise

| Kriterium | Stand | Nachweis |
|---|---|---|
| Manipulierte Updates werden abgelehnt | Erfüllt | `crates/tronic-update`, `apps/desktop/e2e/update.mjs` |
| Falsch signierte Updates werden abgelehnt | Erfüllt | ebenda |
| Ältere Updates werden abgelehnt | Erfüllt, auch mit gültiger Signatur und beim erneuten Einspielen | ebenda |
| Sicherung vor größeren Updates | Erfüllt | `e2e/update.mjs` vergleicht die Kopie Byte für Byte |
| Schlüsselverwaltung dokumentiert | Erfüllt als Dokument | [RELEASE.md](../RELEASE.md). Kein Hardware-Schlüssel eingerichtet |
| Externer Penetrationstest | **Nicht erfüllt** | Nur extern möglich. Vorbereitet: [PENTEST_SCOPE.md](../PENTEST_SCOPE.md) |
| CRA-Anwendbarkeit fachlich geprüft | **Nicht erfüllt** | Nur extern möglich. Vorarbeit: [CRA_ASSESSMENT.md](../CRA_ASSESSMENT.md) |

Im End-to-End-Test ist die „neue Version“ dieselbe Programmdatei mit angehängten Bytes und einer per Entwicklungsschalter gesetzten Versionsnummer. Der Austausch und der Neustart sind echt, zwei wirklich verschiedene Fassungen sind es nicht.
