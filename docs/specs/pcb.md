# Spezifikation: Leiterplatte (Phase 3)

Umsetzung: `crates/tronic-pcb`, `crates/tronic-drc`, `crates/tronic-fab`, Footprints in `crates/tronic-library`, `apps/desktop/src/pcb`.

## Grundlagen

- Koordinaten sind ganze Nanometer, Y zeigt nach unten.
- Kupferlagen: 1, 2, 4, 6 oder 8. Lage 0 ist oben, die letzte unten. Bei einer Lage gibt es nur die obere.
- Bauteile lassen sich in 90-Grad-Schritten drehen und auf die Unterseite umsetzen. Leiterbahnen dürfen jeden Winkel haben, das Werkzeug legt waagerecht, senkrecht und 45 Grad.
- Vias gehen durch alle Lagen. **Blind- und Buried-Vias gibt es noch nicht.**

## Abgleich mit dem Schaltplan

Die Netzliste ist die einzige Quelle für Bauteile und Netze. „Aus Schaltplan aktualisieren“ gleicht ab:

| Fall | Verhalten |
|---|---|
| Neues Bauteil | Wird außerhalb der Platine neben dem Umriss platziert |
| Bauteil entfallen | Footprint wird entfernt, Leiterbahnen bleiben, der DRC meldet ihr Netz |
| Wert geändert | Wird übernommen, Position bleibt |
| Footprint geändert | Wird getauscht, Position bleibt |
| Netz eines Pins geändert | Pad erhält das neue Netz, der DRC zeigt betroffene Leiterbahnen |
| Kein Footprint zugewiesen oder Pin ohne Pad | Wird im Ergebnis gemeldet, das Bauteil fehlt oder der Pin bleibt ohne Netz |

Pins werden über die **Pinnummer** auf Pads abgebildet. Der Abgleich ist idempotent, ein zweiter Lauf ändert nichts. Der Vergleich `compare_with_netlist` zeigt jede Abweichung zwischen Platine und Netzliste.

## Kupfergeometrie

Tracks und runde Pads sind Kapseln (Strecke mit Radius), rechteckige Pads achsparallele Rechtecke, Flächen Polygone. Abstände werden exakt aus dieser Geometrie berechnet. Überlappung gleicher Lage gilt als Verbindung, Vias und durchkontaktierte Pads verbinden alle Lagen.

## Kupferflächen

Eine Fläche gehört zu einem Netz und einer Lage. Die Füllung ist berechnet, nicht gespeichert: Flächenumriss, geschnitten mit dem Platinenumriss abzüglich Randabstand, minus allem Kupfer anderer Netze samt Abstand. Inseln ohne Verbindung zum eigenen Netz entfallen. Flächen werden nach Erstellungsreihenfolge gefüllt, spätere weichen früheren aus. **Wärmebrücken gibt es noch nicht**, Pads sind vollflächig angebunden, was das Löten erschweren kann.

## DRC

| Code | Stufe | Anforderung aus J5 |
|---|---|---|
| `short`: Kupfer verschiedener Netze berührt sich | Fehler | Kurzschlüsse |
| `clearance`: Abstand zu klein | Fehler | Leiterbahnabstände, Kupferabstände |
| `track_width`: Breite unter Minimum | Fehler | Leiterbahnbreiten |
| `unconnected`: Netz nicht vollständig verbunden | Fehler | Nicht verbundene Netze |
| `via_drill`, `pad_drill`, `drill_too_large`, `annular_ring`, `hole_spacing` | Fehler | Bohrungen |
| `board_edge`: Kupfer oder Bohrung zu nah am Rand | Fehler | Platinenrand |
| `outline_missing`, `outline_invalid` | Fehler | Platinenrand |
| `outside_board`: Bauteil außerhalb des Umrisses | Fehler | Platinenrand |
| `component_overlap`: Bauteilflächen überlappen | Fehler | Bauteilüberlappungen |
| `invalid_layer`: Kupfer auf nicht vorhandener Lage | Fehler | Fertigungsgrenzen |
| `unknown_net`: Kupfer mit Netz, das kein Pad hat | Warnung | Nicht verbundene Netze |

Die Fertigungsgrenzen sind die Werte in den Designregeln. **Sie sind Voreinstellungen und nicht mit einem Fertiger abgestimmt.** Vor einer Bestellung sind sie mit dessen Vorgaben zu vergleichen.

## Fertigungsdaten

| Datei | Format |
|---|---|
| Kupfer je Lage, Lötstopp, Bestückungsdruck, Lotpaste, Umriss | Gerber RS-274X mit X2-Attributen, mm, 4.6 |
| Durchkontaktierte und nicht durchkontaktierte Bohrungen | Excellon, mm |
| Bestückliste und Pick-and-Place | CSV |

Lötstopp: Pads erhalten eine Öffnung mit Maskenrand, Vias bleiben abgedeckt. Flächen mit Löchern werden als dunkle Region mit klarer Region geschrieben, größte Fläche zuerst.

Ein unabhängiger Parser prüft die Dateien (`scripts/verify_gerber.py`). **Ein Fertiger hat sie noch nicht gesehen.**

## Noch nicht enthalten

Differenzielle Leitungen und Impedanzkontrolle ([docs/specs/high-speed.md](high-speed.md)) sind spezifiziert, aber nicht gebaut. Die Oberfläche bietet sie nicht an. Der Autorouter hat eine eigene Spezifikation ([docs/specs/autorouter.md](autorouter.md)).
