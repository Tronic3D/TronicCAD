# Spezifikation: Projektdatei `.tronic`, Format 2

Implementierung: `crates/tronic-project/src/format.rs`. Status: **implementiert und getestet.**

## Aufbau

UTF-8-JSON. Die Prüfsumme gilt für die exakten Bytes des Werts `project`, wie sie in der Datei stehen.

```json
{
  "format": "tronicproject",
  "format_version": 4,
  "checksum": { "algorithm": "sha-256", "value": "<64 Hex-Zeichen>" },
  "project": {
    "id": "<UUID v4>",
    "name": "Netzteil",
    "description": "",
    "created_at": "2026-11-04T10:00:00Z",
    "modified_at": "2026-11-04T10:00:00Z",
    "schematic": { }
  }
}
```

| Feld | Regel |
|---|---|
| `name` | Nach Trimmen 1 bis 120 Unicode-Zeichen, keine Steuerzeichen |
| `description` | Höchstens 4000 Unicode-Zeichen |
| Zeitstempel | RFC 3339 |

## Laden

1. Größe höchstens 512 MiB, sonst `TooLarge`.
2. JSON und `format` prüfen, sonst `NotAProject`.
3. `format_version` höher als unterstützt: `UnsupportedVersion`. Datei wird nicht verändert.
4. Prüfsumme vergleichen, sonst `ChecksumMismatch`.
5. Inhalt validieren.

## Speichern

1. Bytes in temporäre Datei im Zielordner schreiben und auf die Platte zwingen.
2. Vorhandene Datei nach `<name>.bak` kopieren. Es gibt genau eine Generation.
3. Temporäre Datei über das Ziel umbenennen.

Absturzfolge: Es bleibt immer eine vollständige Fassung erhalten.

## Konflikterkennung

Die Sitzung merkt sich den SHA-256 der gesamten Datei. Vor dem Speichern wird er mit der Platte verglichen. Bei Abweichung gibt es `Conflict`, bis der Benutzer ausdrücklich überschreibt. Das ist optimistisch: Zwischen Vergleich und Umbenennen bleibt ein sehr kleines Zeitfenster. Für echte gleichzeitige Bearbeitung sind Sperren und Bearbeitungsbereiche in Phase 7 vorgesehen.

## Weiterentwicklung

Jede inhaltliche Änderung erhöht `format_version` und bekommt eine Migration, ältere Dateien müssen lesbar bleiben. Format 2 ergänzt `schematic` ([docs/specs/schematic.md](schematic.md)). Dateien in Format 1 werden gelesen, gespeichert wird in Format 2, die alte Fassung bleibt als `.bak` erhalten. Format 3 ergänzt `board` ([pcb.md](pcb.md)). Format 4 ergänzt `board.mechanics` sowie `body` und `model` je Footprint ([view3d.md](view3d.md)). Fehlende Angaben laden mit Voreinstellungen, gespeichert wird immer im neuesten Format.

**Offene Entscheidung vor Phase 6:** „Speichern unter“ behält heute die Projekt-ID, zwei Kopien sind also nicht unterscheidbar. Für die Synchronisierung muss eine Kopie eine neue Identität erhalten.
