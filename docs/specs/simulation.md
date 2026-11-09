# Spezifikation: Simulation

Stand 04.11.2026, Phase 4. Code: `crates/tronic-sim`, `apps/desktop/src/simulation`.

## Zweck

Einfache analoge Schaltungen aus dem Schaltplan rechnen und das Ergebnis nachvollziehbar machen. Kein Ersatz für einen voll ausgebauten SPICE-Simulator (Begründung in [ADR 0002](../adr/0002-3d-rendering.md)).

## Analysen

| Analyse | Eingaben | Ausgabe |
|---|---|---|
| Arbeitspunkt | keine | Spannungen und Ströme als Tabelle |
| DC-Durchlauf | Quelle, Start, Ende, Schritt | Kurven über der Quellgröße |
| AC, Frequenzgang | Start, Ende, Punkte je Dekade | Betrag (wahlweise in dB) und Phase über der Frequenz, logarithmisch |
| Transient | Dauer, Zeitschritt | Kurven über der Zeit |

Werte können mit Vorsatz eingegeben werden: `10k`, `4k7`, `1meg`, `100n`, `3V3`.

## Verfahren

Modifizierte Knotenanalyse mit dichter LU-Zerlegung. Newton-Verfahren mit Spannungsbegrenzung an Halbleitern, bei Bedarf Gmin- und Quellenschrittweite. Transient mit fester Schrittweite und Trapezregel. Beim Start und an Pulsecken wird mit feiner unterteiltem Rückwärts-Euler gedämpft, weil die Trapezregel sonst schwingt. Temperatur fest 27 °C.

**Der Zeitschritt ist fest.** Er muss kürzer sein als die kürzeste Zeitkonstante. Es gibt keine Schrittweitensteuerung. Ein zu großer Schritt liefert ein glattes, aber falsches Ergebnis ohne Warnung. Die Oberfläche weist darauf hin.

## Quellen

- `sim:vsource` und `sim:isource` aus der Bibliothekskategorie „Simulation“. Ihr Wert lautet zum Beispiel `5`, `DC 5`, `AC 1`, `SIN(0 1 1k)`, `PULSE(0 5 0 1n 1n 1m 2m)` oder eine Kombination.
- Versorgungssymbole: Die Spannung kommt aus dem Netznamen (`+5V`, `3V3`, `-12V`) oder aus einem Eingabefeld.

Wie bei SPICE gilt: Arbeitspunkt, DC-Durchlauf und die Arbeitspunktbestimmung der AC-Analyse verwenden den angegebenen DC-Wert, sonst den Wert der Wellenform bei null. Eine Transient-Analyse startet immer mit dem Wert der Wellenform bei null.

## Modelle

| Bauteil | Modell | Vereinfachung |
|---|---|---|
| Widerstand, Kondensator, Spule | Ideal | Keine parasitären Größen |
| Potentiometer | Zwei Widerstände | Schleifer fest auf 50 % |
| Diode, LED, Zener, Schottky | Shockley, Zener mit Durchbruch bei 1 mA | Typische Werte je Art, keine Sperrschichtkapazität, keine Herstellerdaten |
| NPN, PNP | Ebers-Moll | Kein Early-Effekt, keine Kapazitäten |
| N- und P-MOSFET | Level 1 | Keine Kapazitäten |
| Operationsverstärker | Ideal, Verstärkung 1e5 | Keine Versorgungsgrenze, kein Eingangsstrom, keine Bandbreitengrenze |
| Steckverbinder | Offen | |
| Alles andere (555, Mikrocontroller, Regler, Logik) | **Kein Modell** | Wird namentlich gemeldet und fehlt im Ergebnis |

Das Bauteil im Schaltplan bestimmt das Modell. Ein Wert wie „BC547“ ändert die Parameter nicht: Alle NPN rechnen mit denselben typischen Werten.

## Verhalten bei Problemen

| Lage | Verhalten |
|---|---|
| Bauteil ohne Modell | Deutlicher Hinweis mit Referenz, Lauf bleibt möglich |
| Unlesbarer Wert, unbekannte Versorgungsspannung | Lauf gesperrt, Oberfläche und Backend (`sim.incomplete`) |
| Keine Masse, keine Bauteile, nicht lösbar, keine Konvergenz | Übersetzte Fehlermeldung statt Ergebnis |
| Zu viele Punkte | `sim.invalidSettings` (höchstens 20 000 Punkte bei DC und AC, 2 Mio. Zeitschritte) |

## Nachvollziehbarkeit

Zu jedem Lauf zeigt die Oberfläche die verwendeten Modelle und die SPICE-Netzliste. Beides und die Ergebnisse (CSV mit Punkt als Dezimalzeichen) lassen sich speichern.

## Abnahme und Nachweise

- 28 Referenztests gegen von Hand gerechnete Werte: `crates/tronic-sim/tests/reference.rs`
- 21 Schaltungen im Vergleich mit ngspice 47: `crates/tronic-sim/examples/sim_decks.rs`, `scripts/verify_sim.py`. Toleranzen: Arbeitspunkt 10 µV + 0,1 %, DC 1 mV + 0,2 %, AC 1 µ + 0,5 %, Transient 50 mV + 1 %. Die größte Abweichung im Zeitbereich liegt mit 62 mV an der steilen Flanke des MOSFET-Schalters, außerhalb von Flanken bei etwa 1 mV.
- End-to-End gegen die echte Anwendung mit gerechneten Werten: `apps/desktop/e2e/sim3d.mjs`

Der Vergleich läuft nur, wenn `TRONICCAD_NGSPICE` gesetzt ist. ngspice ist nur ein Entwicklungswerkzeug und wird nicht ausgeliefert.
