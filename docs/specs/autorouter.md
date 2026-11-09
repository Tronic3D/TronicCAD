# Spezifikation: Automatisches Routing

Umsetzung: `crates/tronic-router`, Oberfläche in `apps/desktop/src/pcb`. Status: **implementiert und getestet, nur unter Windows.** Messwerte in [reports/PERFORMANCE.md](../../reports/PERFORMANCE.md).

## Zusagen

1. **Ein Vorschlag verschlechtert die Platine nie.** Zum Schluss prüft der echte DRC das Ergebnis. Jede neue Verbindung, die einen Abstand, die Randregel, die Bohrabstände oder eine andere Regel verletzt, wird wieder entfernt.
2. **Ehrlicher Bericht.** Die offenen Verbindungen im Ergebnis werden auf der fertigen Platine neu gemessen, nicht aus der Wegsuche übernommen.
3. **Vorhandenes Kupfer bleibt unverändert.** Der Router legt nur neue Bahnen und Vias an und verschiebt keine Bauteile.
4. **Vorschau und Freigabe.** Erst „Übernehmen“ ändert die Platine, in einem Rückgängig-Schritt. Wurde die Platine zwischenzeitlich geändert, wird der Vorschlag abgelehnt.
5. **Abbrechbar.** Die Oberfläche bleibt bedienbar. Abbruch oder Zeitlimit (120 Sekunden) liefern das bisher Erreichte als gültigen Vorschlag.
6. **Deterministisch.** Dieselbe Platine ergibt denselben Vorschlag.

## Verfahren

- **Raster:** Jeder Knoten vermerkt, wer ihn nutzen darf: jeder, nur ein bestimmtes Netz oder niemand. Sperrzonen sind exakt berechnet aus Kupfer, Mindestabstand und halber Breite der neuen Bahn. Rastermaß 0,25 mm oder feiner, auf sehr großen Platinen gröber, bis die Sperrzonen keinen Schritt mehr abdecken würden. Dann meldet der Router, die Platine sei zu groß.
- **Wegsuche:** A* über Knoten, Lage und Richtung, also mit Kosten für Abbiegen. Spitzere Winkel als 90 Grad sind ausgeschlossen. Vias kosten so viel wie 30 Rasterschritte, jede Lage hat eine Vorzugsrichtung.
- **Reihenfolge:** Kurze Verbindungen zuerst.
- **Umlegen:** Findet eine Verbindung keinen Weg, sucht der Router einen Weg ohne die bereits verlegten Bahnen, entfernt die im Weg liegenden und legt sie danach neu. Jede Verbindung hat höchstens vier Versuche, insgesamt gibt es höchstens 400 Umlegungen.
- **Anschluss:** Kurze Stichleitungen verbinden Rasterknoten mit den genauen Pad-Mitten. Sie dürfen jeden Winkel haben.

## Kupferflächen

Fremde Flächen sind keine Hindernisse, weil ihre Füllung neuen Bahnen ausweicht. Eine neue Bahn kann eine Fläche so zerschneiden, dass ein Teil der Fläche keine Verbindung mehr hat. Das erscheint dann als offene Verbindung im Bericht.

## Abnahme

| Kriterium | Stand |
|---|---|
| Referenzplatinen: LED, 555-Blinker, Bus mit 8 ICs vollständig verlegt | Erfüllt, als Test abgesichert |
| Jedes Ergebnis ohne neue DRC-Befunde | Erfüllt, auf allen Testplatinen und Lagenzahlen abgesichert |
| Nicht verlegte Netze werden gemeldet | Erfüllt, abgesichert |
| Laufzeit, Abbruch, bedienbare Oberfläche | Erfüllt, abgesichert |
| Vergleich mit einem kommerziellen Router | **Nicht gemacht** |
| Praxistest auf einer echten Platine | **Offen** |

## Grenzen

Enge Platinen mit zwei Lagen werden nicht immer vollständig verlegt, siehe Messwerte. Das Umlegen ist einfach gehalten, und ein schlechtes Platzieren löst der Router nicht auf. Ergebnisse sind elektrisch regelkonform, aber nicht optimiert für Signalqualität, EMV, Stromtragfähigkeit oder Impedanz. **Versorgungsleitungen werden genauso schmal verlegt wie Signale.** Alle Bahnen haben die eingestellte Breite. Bahnen werden nicht nachträglich geglättet, und Vias werden nicht eingespart, wenn ein anderer Weg nur etwas länger wäre.
