# Spezifikation: Automatisches Sichern und Wiederherstellung

Umsetzung: `crates/tronic-project/src/autosave.rs`, Zeitgeber in `apps/desktop/src-tauri/src/lib.rs`. Status: **implementiert und getestet**, nur unter Windows.

## Verhalten

- Alle 30 Sekunden wird ein Sicherungsstand geschrieben, wenn es neue ungespeicherte Änderungen gibt. Die Projektdatei bleibt unberührt.
- Der Stand liegt im Unterordner `autosave` des Datenordners, je Projektpfad eine Datei (`<SHA-256 des Pfads>.autosave`), atomar ersetzt.
- Die Statusleiste zeigt die Zeit der letzten Sicherung oder eine Warnung bei einem Fehler.

| Ereignis | Sicherungsstand |
|---|---|
| Speichern, Speichern unter | Wird gelöscht |
| Schließen oder Wechseln ohne Speichern, nach ausdrücklicher Entscheidung | Wird gelöscht |
| Absturz, Stromausfall, Abbruch des Prozesses | Bleibt, wird beim nächsten Start angeboten |

## Wiederherstellung

Beim Start und beim Öffnen einer betroffenen Datei erscheint ein Dialog mit **Wiederherstellen**, **Verwerfen** und **Später entscheiden**.

- Wiederhergestellte Arbeit gilt als ungespeichert. Gespeichert wird erst auf Wunsch.
- Der gespeicherte Hash der Ausgangsdatei bleibt erhalten. Wurde die Datei inzwischen geändert, zeigt der Dialog das an, und das Speichern fragt vor dem Überschreiben. Eine gelöschte Datei wird neu angelegt.
- Der Stand geht sofort auf die neue Anwendung über, ein weiterer Absturz verliert also nichts.

## Laufende Anwendungen unterscheiden

Jede laufende Anwendung hält eine Betriebssystem-Sperre auf `<kennung>.lock`, und jeder Stand nennt seine Kennung. Ist die Sperre frei, läuft diese Anwendung nicht mehr. Im Zweifel gilt sie als laufend, und ihr Stand wird nicht angefasst. Verwaiste Sperrdateien räumt der nächste Start weg.

## Dateiaufbau

JSON mit `format` (`tronicautosave`), `format_version`, `owner`, `original_path`, `base_hash`, `saved_at` und `document`. `document` enthält die vollständige Projektdatei im normalen Format samt Prüfsumme. Beschädigte Stände werden übersprungen und nicht gelöscht.

## Datenordner

Die Umgebungsvariable `TRONICCAD_DATA_DIR` ersetzt den Datenordner des Betriebssystems, für den Betrieb von einem Wechseldatenträger und für isolierte Tests.
