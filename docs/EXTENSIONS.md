# Erweiterungen schreiben

TronicCAD lässt sich mit eigenen Werkzeugen erweitern, in jeder Programmiersprache. Ein Beispiel liegt in [extensions/examples/part-list](../extensions/examples/part-list).

Stand: Schnittstellenversion 1. Geprüft nur unter Windows 11. Eine Erweiterung kann lesen und Meldungen und Dateien erzeugen, aber das Projekt nicht ändern und keine eigenen Fenster in die Anwendung bringen.

## Aufbau

Jede Erweiterung ist ein Ordner im Erweiterungsordner, den die Anwendung unter „Erweiterungen“ nennt:

```text
extensions/
  meine-erweiterung/
    extension.json
    main.py
```

`extension.json`:

| Feld | Bedeutung |
|---|---|
| `api` | Schnittstellenversion, derzeit `1` |
| `id` | Kleinbuchstaben, Ziffern, Bindestriche, eindeutig |
| `name`, `version`, `description`, `author` | Für die Anzeige |
| `license` | SPDX-Angabe. Erweiterungen behalten ihre eigene Lizenz |
| `command` | Programm und Argumente, gestartet im Ordner der Erweiterung. `./tool` meint eine Datei der Erweiterung, alles andere wird wie üblich gesucht |
| `timeout_seconds` | Optional, 1 bis 3600, sonst 60 |

## Ablauf

Die Anwendung startet das Programm und schreibt auf die Standardeingabe:

```json
{ "api": 1, "project": { "name": "...", "description": "..." }, "schematic": {}, "board": {}, "netlist": {} }
```

`schematic` und `board` sind die Daten der Projektdatei, `netlist` die berechnete Netzliste. **Ihre Struktur kann sich mit neuen Fassungen ändern.** Nur `api` zeigt verbindlich an, was gilt.

Die Antwort kommt als JSON auf die Standardausgabe, mit höchstens 64 MB:

```json
{
  "messages": [{ "level": "info", "text": "12 Bauteile" }],
  "files": { "stueckliste.csv": "Text der Datei" }
}
```

`level` ist `info`, `warning` oder `error`. Dateien sind Text mit einfachem Namen ohne Pfad. Sie landen neben dem Projekt in `<Projekt>-extensions/<id>/` und ersetzen dort frühere Ergebnisse dieser Erweiterung.

Ein Fehler wird über einen Rückgabewert ungleich 0 gemeldet, der Text auf der Fehlerausgabe erscheint in der Anwendung. Weitergegeben werden nur wenige Umgebungsvariablen wie `PATH`, damit Zugangsdaten aus der Umgebung nicht bei einer Erweiterung landen.

## Sicherheit

**Eine Erweiterung ist normaler Programmcode mit den Rechten der angemeldeten Person.** Sie ist nicht abgeschottet und kann auf alle Dateien zugreifen und das Netz nutzen.

Deshalb läuft sie erst, wenn die Person zugestimmt hat, und nur in genau der geprüften Fassung: Die Zustimmung gilt für eine Prüfsumme über alle Dateien des Ordners. Ändert sich eine Datei, ist die Zustimmung hinfällig. Das erkennt Änderungen, garantiert aber nicht, dass eine Erweiterung harmlos ist. Signaturen und einen Katalog gibt es nicht.
