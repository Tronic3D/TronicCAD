# Spezifikation: Schaltplan (Phase 2)

Umsetzung: `crates/tronic-schematic`, `crates/tronic-library`, `crates/tronic-erc`, `apps/desktop/src/schematic`.

## Einheiten und Geometrie

- Koordinaten sind ganze Nanometer, Y zeigt nach unten. Raster: 1,27 mm.
- Transformation eines Symbols: erst spiegeln (X), dann drehen (90° im Uhrzeigersinn: `(x, y) → (-y, x)`), dann verschieben.
- Leitungen und Busse sind waagerecht oder senkrecht. Das Werkzeug zerlegt schräge Wege in zwei Segmente, das Modell lehnt andere ab.

## Verbindungsregeln

Zwei Elemente sind auf einer Seite verbunden, wenn:

1. Pins, Label oder Ports am selben Punkt liegen,
2. ein Pin, Label, Port oder ein Leitungsende auf einer Leitung liegt, auch mitten auf ihr,
3. eine explizite Verbindungsstelle auf beiden Leitungen liegt.

Zwei Leitungen, die sich nur kreuzen, sind **nicht** verbunden. Leitungen und Busse sind elektrisch getrennt.

Über Namen verbunden werden:

| Element | Geltungsbereich |
|---|---|
| Lokales Label | Gleiche Seite, gleicher Name |
| Hierarchisches Label | Wie lokal, zusätzlich zum gleichnamigen Port der übergeordneten Blockinstanz |
| Globales Label | Alle Seiten |
| Versorgungssymbol | Alle Seiten, Netzname ist der Wert des Symbols |

## Netznamen

Vorrang: Versorgung, global, lokal oder hierarchisch auf höchster Ebene (`/Seite/Name`), sonst automatisch aus dem kleinsten Pin (`Net-R1-1`). Reihenfolge und Namen sind deterministisch.

## Seiten und Hierarchie

- Oberste Seiten sind die Seiten des Plans.
- Ein Block auf einer Seite verweist auf genau eine Unterseite. **Einschränkung:** Eine Seite kann nur einmal instanziiert werden, die Mehrfachverwendung eines Blocks ist noch nicht möglich. Referenznummern bleiben dadurch eindeutig ohne Pfadzuordnung.
- Ports eines Blocks folgen den hierarchischen Labels der Unterseite. Bestehende Ports behalten ihre Position, damit Leitungen nicht abreißen.

## Busse

Ein Bus ist eine Gruppe verbundener Bussegmente mit einem Label wie `D[0..7]`. Die Einzelsignale `D0` bis `D7` verbinden sich über ihre gleichnamigen Labels. Der ERC prüft den Busnamen und ob an den Bus geführte Leitungen ein passendes Label tragen.

## Bearbeitung

Jede Änderung ist ein Befehl im Rust-Backend und atomar: Schlägt sie fehl, bleibt das Dokument unverändert. Rückgängig und Wiederholen arbeiten mit Zuständen (höchstens 200 Schritte). Metadaten des Projekts sind davon ausgenommen. Beim Verschieben, Drehen und Spiegeln bleiben Verbindungen zu nicht ausgewählten Elementen durch neue Leitungen erhalten. Benachbarte Leitungsstücke auf einer Linie werden zusammengefasst, wenn sich dadurch die Verbindungen nicht ändern.

## ERC

| Code | Stufe | Anforderung aus J5 |
|---|---|---|
| `supply_short`: verschiedene Versorgungen am selben Netz | Fehler | Kurzschlüsse |
| `component_shorted`: alle Pins eines Bauteils am selben Netz | Warnung | Kurzschlüsse |
| `pin_not_connected` | Fehler | Nicht verbundene Pins |
| `no_connect_on_net`, `no_connect_unattached` | Warnung | Nicht verbundene Pins |
| `not_connected_pin_wired` | Fehler | Falsche Pin-Typen |
| `output_on_supply`: Ausgang an Versorgungssymbol | Fehler | Falsche Pin-Typen |
| `missing_supply`: Versorgungseingang ohne Quelle | Fehler | Fehlende Versorgung |
| `input_not_driven` | Warnung | Falsche Verbindungen |
| `conflicting_net_names` | Warnung | Falsche Verbindungen |
| `bus_label_missing`, `bus_label_invalid`, `bus_entry_not_member` | Warnung / Fehler | Falsche Verbindungen |
| `label_not_connected`, `wire_dangling`, `supply_port_unused` | Warnung | Falsche Verbindungen |
| `hierarchical_label_without_block`, `port_not_connected` | Fehler / Warnung | Falsche Verbindungen |
| `multiple_outputs`, `multiple_power_outputs`, `output_conflict` | Fehler | Mehrfach belegte Ausgänge |
| `duplicate_reference` | Fehler | Doppelte Referenznummern |
| `reference_not_annotated` | Warnung | Doppelte Referenznummern |

Die Prüfung ist reine Berechnung ohne KI. Ein Versorgungseingang gilt nur als versorgt, wenn sein Netz ein Versorgungssymbol oder einen Versorgungsausgang enthält; hinter einem Widerstand liegende Eingänge brauchen also ein eigenes Symbol.

## Bibliothek

Jedes Symbol trägt Pindefinitionen, Teiledaten (Footprint, 3D-Modell, Hersteller, Artikelnummer, Datenblatt, Grenzwerte, jeweils optional) und einen Herkunftsdatensatz. Ein Projekt enthält Kopien aller benutzten Symbole und bleibt so ohne Bibliothek lesbar.

**Footprint und 3D-Modell sind in Phase 2 leer**, sie folgen in Phase 3 und 4. Die Pinbelegung realer Typen ist **nicht gegen Datenblätter geprüft** und entsprechend markiert (`reviewed: false`).

## Dateiformat 2

Format 1 erweitert um das Feld `schematic` im geprüften Inhalt. Dateien in Format 1 werden gelesen und erhalten einen leeren Schaltplan, gespeichert wird immer in Format 2.
