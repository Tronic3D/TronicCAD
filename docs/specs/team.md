# Spezifikation: Teamarbeit

Stand 04.11.2026, Phase 7. Code: `services/cloud/src/team.rs`, `permissions.rs`, `audit.rs`, `crates/tronic-project/src/areas.rs`, `apps/desktop/src/team`. Betrieb: [SELF_HOSTING.md](../SELF_HOSTING.md).

## Rollen

Eine einzige Tabelle entscheidet (`permissions.rs`). Eine Projektrolle kann die Teamrolle für ein Projekt ersetzen, Team-Mitgliedschaft bleibt Voraussetzung.

| Aktion | Administrator | Projektleiter | Entwickler | Mitarbeiter | Betrachter |
|---|---|---|---|---|---|
| Lesen: Versionen, Kommentare, Sperren | ja | ja | ja | ja | ja |
| Team umbenennen, löschen | ja | | | | |
| Mitglieder verwalten, einladen | ja | nur Entwickler und darunter | | | |
| Projekte hinzufügen, Bereiche zuweisen, fremde Sperren aufheben | ja | ja | | | |
| Audit-Protokoll lesen | ja | ja | | | |
| Kommentieren | ja | ja | ja | ja | |
| Jeden Bereich ändern und sperren | ja | ja | ja | | |
| Zugewiesene Bereiche ändern und sperren | ja | ja | ja | ja | |

Niemand kann die eigenen Rechte erhöhen. Ein Team behält immer mindestens einen Administrator, auch beim Löschen eines Kontos. Die Projektdateien von Teamprojekten überleben die Kontolöschung ihres Erstellers.

## Bereiche

| Bereich | Inhalt |
|---|---|
| Schaltplan | Bauteile, Netze, Werte, also die elektrischen Daten |
| Leiterplatte | Platzierung, Kupfer, Designregeln |
| Mechanik | Plattendicke, Höhengrenzen, importierte 3D-Modelle |
| Projektdaten | Name, Beschreibung |

Der Zeitstempel in der Datei gehört zu keinem Bereich. Speichern ohne Änderung ändert also nichts.

## Sperren

Eine Sperre gilt für einen Bereich eines Projekts, läuft nach 4 Stunden ab und lässt sich vom Inhaber erneuern. Inhaber, Administratoren und Projektleiter können sie aufheben, wobei Letzteres den Inhaber benachrichtigt und im Protokoll steht. Ein entferntes Mitglied verliert seine Sperren.

Ohne Sperre darf jeder mit passender Rolle ändern. Sperren schützen also vor Überschneidungen, wo man sie erwartet. Sie sind keine Pflicht.

**Durchsetzung auf dem Server:** Trifft eine neue Version in einem Teamprojekt ein, vergleicht der Server die gespeicherten Dateien der Vorgänger mit der neuen Datei und ermittelt die geänderten Bereiche selbst. Für jeden prüft er Rolle, Zuweisung und Sperre. Angaben des Clients zählen nicht: Ein veränderter Client kommt nicht an Sperren vorbei, und eine Version ohne Vorgänger zählt als Änderung aller Bereiche. Keine gültige Projektdatei wird abgelehnt.

Die Desktop-App sperrt die Editoren nicht. Wer in einem gesperrten Bereich arbeitet, erfährt das beim Synchronisieren: Die Version wird abgelehnt, die eigene Datei bleibt unverändert.

## Konsistenz und Zusammenführen

Haben zwei Stände verschiedene Bereiche geändert, führt die Synchronisierung sie zusammen und meldet, was woher kam. Derselbe Bereich auf beiden Seiten bleibt ein Konflikt für einen Menschen. Innerhalb eines Bereichs wird nie zusammengeführt.

Elektrische und mechanische Daten bleiben konsistent, soweit das heißt:

- Jedes Ergebnis ist eine vollständig geprüfte Projektdatei.
- Abweichungen zwischen Schaltplan und Leiterplatte werden **berechnet und gemeldet**, wie nach jeder Änderung am Schaltplan. Sie werden nicht automatisch behoben und verhindern die Zusammenführung nicht. Der Abgleich bleibt eine Entscheidung des Benutzers.

Echtzeit-Zusammenarbeit im selben Bereich gibt es nicht. Der Auftrag sieht sie erst später vor.

## Audit-Protokoll

Jede Änderung schreibt im selben Datenbankvorgang einen Eintrag: Konten, An- und Abmeldung, auch fehlgeschlagene, Teams, Mitglieder, Einladungen, Projekte, Rollen, Bereiche, Sperren, Kommentare, Versionen, Freigaben. Lesezugriffe stehen nicht darin.

Die Datenbank weist Änderungen und Löschungen per Trigger ab, und die Einträge bilden eine Hash-Kette. **Wer die Datenbank voll kontrolliert, kann Trigger abschalten und die Kette neu berechnen.** Eine Änderung einzelner Einträge fällt auf, eine vollständige Neuberechnung nicht. Dagegen hilft nur eine Kopie außerhalb, die der Dienst nicht anbietet.

Einträge überleben die Kontolöschung und enthalten E-Mail-Adressen. Das ist mit dem Datenschutz abzuwägen.

## Weitere Funktionen

| Funktion | Umsetzung | Grenze |
|---|---|---|
| Einladungen | Code für genau eine Adresse, 14 Tage, einmal | Kein Mailversand, der Code wird von Hand weitergegeben |
| Kommentare | Je Projekt, mit Bereich und Bauteil oder Stelle | Keine Markierung in der Grafik |
| Benachrichtigungen | Im Team-Modul | Nur beim Öffnen sichtbar, kein Hinweis außerhalb |
| Live-Status | Wer arbeitet woran | Nur solange das Team-Modul offen ist, Bereich von Hand gewählt |
| Versionsvergleich | Aus Phase 6 | |
| Änderungsprotokoll | Audit-Protokoll | |

Über WebSocket meldet der Server neue Versionen, die App nutzt das noch nicht. Sie fragt jede Minute nach.

## Nachweise

- `services/cloud/tests/team.rs`: alle Rollen gegen alle Aktionen über den echten Dienst, Rechteausweitung, Sperren mit echten Projektdateien, gefälschte Versionen, drei Personen in drei Bereichen, Vollständigkeit des Protokolls, Manipulation.
- `crates/tronic-project/src/areas.rs`: Zuordnung zu Bereichen und Zusammenführen.
- `apps/desktop/e2e/team.mjs`: zwei Instanzen der Anwendung mit eigenen Konten.
- `scripts/verify_selfhost.py`: Anleitung wörtlich auf frischem Ubuntu.
