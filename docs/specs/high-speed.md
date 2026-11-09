# Spezifikation: Differenzielle Leitungen und Impedanzkontrolle

Status: **nicht implementiert.** Vorhanden ist nur die Anzeige der Leiterbahnlänge je Netz.

## Differenzielle Leitungen

- Netzpaare über Namensregel (`_P`/`_N`, `+`/`-`) oder explizite Zuordnung.
- Gekoppeltes Verlegen mit festem Abstand und fester Breite.
- DRC-Regeln: Längendifferenz innerhalb des Paares, Kopplungsabstand.

## Impedanzkontrolle

- Lagenaufbau mit Dielektrikum, Dicke und Kupferstärke.
- Rechner für Microstrip und Stripline. **Näherungsformeln nach IPC-2141 weichen je nach Geometrie erheblich ab.** Sie dienen der Orientierung, verbindlich ist der Lagenaufbau des Fertigers oder ein Feldlöser.
- Netzklassen mit Zielimpedanz, daraus Bahnbreite und Abstand.

## Abnahme

Berechnete Werte werden mit veröffentlichten Referenzwerten und einem Fertigerbericht verglichen, die Abweichung wird dokumentiert. Ohne diesen Vergleich gilt die Funktion nicht als fertig.
