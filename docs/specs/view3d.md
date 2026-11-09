# Spezifikation: 3D-Ansicht und mechanische Prüfung

Stand 04.11.2026, Phase 4. Code: `crates/tronic-3d`, `apps/desktop/src/view3d`. Technikentscheidung in [ADR 0002](../adr/0002-3d-rendering.md).

## Ansicht

- Plattenkörper aus dem Umriss, Oberseite und Unterseite zeigen Lötstopplack, Kupfer unter dem Lack, Pads, Vias, Bestückungsdruck und Bohrungen der jeweiligen Seite.
- Bauteile aus einfachen Körpern oder aus einem importierten Modell, auf beiden Seiten und in allen Drehungen.
- Drehen, Verschieben und Zoomen mit Maus und Tastatur. Ansichten oben, unten, vorne, Seite, isometrisch, Einpassen. Perspektive und Parallelprojektion, Bauteile ausblendbar.
- Kollidierende Bauteile sind rot, das gewählte Bauteil ist weiß.

Die Ansicht ist nicht fotorealistisch: keine Schatten, keine Spiegelungen, keine Lötstellen.

## Bauteilkörper

Die eingebauten Footprints haben Körper mit typischen Maßen, keine Herstellerkonturen. Bedrahtete Bauteile haben Anschlüsse, die 3 mm unter die Montagefläche reichen (1,6 mm Platte und 1,4 mm gekürzter Draht). Bei anderer Plattendicke ändert sich dieser Wert nicht.

## Modellimport

| Format | Umfang |
|---|---|
| STL | Binär und Text |
| OBJ | Flächen beliebiger Eckenzahl, alle Indexformen, ohne Materialien |
| WRL (VRML 2) | `IndexedFaceSet`. **Transformationen und andere Geometrieknoten werden nicht ausgewertet**, Modelle aus mehreren versetzten Teilen erscheinen daher falsch |
| STEP, STP | **Nicht unterstützt**, klare Meldung, siehe ADR 0002 |

Ein Modell gilt für alle Bauteile mit demselben Footprint und liegt in der Projektdatei. Grenzen: 200 000 Dreiecke, Datei höchstens 256 MB. Maßstab und Höhe gibt der Benutzer an, die Lage in der Platte folgt dem Ursprung des Modells. Farben und Texturen werden nicht übernommen. Die Rechte an importierten Modellen liegen beim Benutzer, die Oberfläche weist darauf hin.

Beschädigte Dateien, falsche Zahlen, Indizes außerhalb des Bereichs, NaN und eine falsche Dreieckszahl im STL-Kopf werden abgelehnt, ohne dass Speicher nach Angabe aus der Datei reserviert wird.

## Mechanische Prüfung

Regeln je Platine (Reiter „Mechanik“): Plattendicke, Mindestabstand zwischen Bauteilen (Voreinstellung 0,25 mm), Höhengrenzen je Seite (optional).

| Code | Bedeutung |
|---|---|
| `collision` | Zwei Bauteile durchdringen oder berühren sich |
| `too_close` | Abstand kleiner als gefordert, mit Ist- und Sollwert |
| `too_high` | Höher als die Grenze der Seite |
| `no_model` | Kein 3D-Körper, ein Block von 1 mm über dem Courtyard ersetzt ihn |

**Verglichen werden achsparallele Quader.** Für eingebaute Körper in 90-Grad-Schritten ist das exakt, nur Zylinder zählen mit ihrem umschließenden Quader. Importierte Modelle zählen mit dem umschließenden Quader, was eine Kollision melden kann, die es nicht gibt. Echte Kollisionen werden dadurch nicht übersehen, solange das Modell korrekt skaliert ist.

Nicht geprüft werden Gehäuse, Abstände zum Plattenrand in 3D, Bohrungen gegen Körper, Stecker im gesteckten Zustand und Montageschrauben.

## Dateiformat

Formatversion 4: `board.mechanics`, `body` und `model` je Footprint. Ältere Dateien laden mit Voreinstellungen, siehe [project-file-format.md](project-file-format.md).

## Abnahme und Nachweise

- Kollisionsfälle: `crates/tronic-3d/tests/mechanics.rs` (getrennt, überlappend, Gegenseite, unter Überhang, Abstand genau an der Grenze, Drehung, Höhen, Modell, Anschlüsse durch die Platte)
- Import: Tests in `crates/tronic-3d/src/import.rs`, auch für beschädigte Dateien
- Kamera: `apps/desktop/src/view3d/camera.test.ts`
- Echte Pixel und Bedienung: `apps/desktop/e2e/sim3d.mjs`
