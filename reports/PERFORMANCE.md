# Messwerte

Gemessen am 04.11.2026 mit `apps/desktop/e2e/schematic.mjs` in der echten Anwendung. Windows 11, WebView2 156, ein Rechner mit 16 logischen Kernen, Fenster 1280 × 800. **Einzelne Läufe, keine Statistik, nur diese Plattform.**

Testfall: eine Seite mit 10 002 Objekten (5 001 Bauteile, 5 000 Leitungen, 1 Markierung).

| Vorgang | Release-Build | Debug-Build |
|---|---|---|
| Einfache Änderung einschließlich Rückgabe der Ansicht | 23 ms | 110 ms |
| Einfügen von 10 000 Objekten | 217 ms | 1 693 ms |
| ERC | 22 ms | 97 ms |
| Netzliste | 47 ms | 176 ms |
| Speichern | 24 ms | 183 ms |
| Bildzeit beim Zoomen, Median | 16,7 ms | 17,8 ms |
| Bildzeit beim Zoomen, 95. Perzentil | 16,9 ms | 18,5 ms |

16,7 ms entsprechen dem 60-Hz-Takt des Bildschirms. Datenmenge der Ansicht pro Änderung: rund 1,07 MB JSON. Die Zeit zum Einlesen dieser Antwort in der Oberfläche ist in der ersten Zeile enthalten, das anschließende Neuzeichnen nicht.

**Nicht gemessen:** Speicherverbrauch, Langzeitbetrieb, Pläne mit vielen Seiten, schwächere Rechner, macOS und Linux.

## Leiterplatte

Testfall: 8 004 Leiterbahnsegmente in 80 Netzen auf zwei Lagen, zwei Bauteile, eine Kupferfläche. Mit `apps/desktop/e2e/pcb.mjs`, gleiche Plattform.

| Vorgang | Release-Build | Debug-Build |
|---|---|---|
| Einfache Änderung einschließlich Analyse und Rückgabe der Ansicht | 66 ms | 360 ms |
| DRC | 95 ms | 457 ms |
| Speichern | 33 ms | 181 ms |
| Bildzeit beim Zoomen, Median | 16,7 ms | 16,7 ms |
| Bildzeit beim Zoomen, 95. Perzentil | 16,9 ms | 16,9 ms |

**Nicht gemessen:** Platinen mit vielen Kupferflächen und tausenden Pads. Die Flächenberechnung läuft bei jeder Änderung neu und dürfte dort der Engpass sein.

## Autorouter

`cargo run --release -p tronic-router --example route_bench`, einzelne Läufe, gleiche Plattform, Zeitlimit 120 Sekunden, Regeln: Bahn 0,25 mm, Abstand 0,2 mm. Die Zufallsplatinen verbinden Pins von SOIC-8 zufällig und sind damit ungünstiger als echte, sinnvoll platzierte Schaltungen. Jedes Ergebnis besteht den DRC ohne neue Befunde, das sichern die Tests ab.

| Platine | Verbindungen | Verlegt | Anteil | Vias | Umlegungen | Zeit |
|---|---|---|---|---|---|---|
| LED, 2 Lagen | 3 | 3 | 100 % | 0 | 0 | 0,0 s |
| 555-Blinker, 2 Lagen | 15 | 15 | 100 % | 9 | 0 | 0,2 s |
| Bus mit 8 ICs, 2 Lagen | 36 | 36 | 100 % | 9 | 0 | 0,7 s |
| Zufall 9 ICs, 60 x 60 mm, 2 Lagen | 40 | 40 | 100 % | 44 | 0 | 2,1 s |
| Zufall 16 ICs, 80 x 80 mm, 2 Lagen | 68 | 68 | 100 % | 152 | 19 | 11,8 s |
| Zufall 16 ICs, 80 x 80 mm, 4 Lagen | 68 | 68 | 100 % | 97 | 0 | 16,8 s |
| Zufall 25 ICs, 90 x 90 mm, 2 Lagen | 110 | 110 | 100 % | 286 | 0 | 16,7 s |
| Zufall 25 ICs, 90 x 90 mm, 4 Lagen | 110 | 110 | 100 % | 187 | 0 | 32,7 s |
| Zufall 36 ICs, 100 x 100 mm, 4 Lagen | 160 | 160 | 100 % | 294 | 21 | 54,5 s |
| Eng: 16 ICs, 45 x 45 mm, 2 Lagen | 71 | 70 | 98,6 % | 138 | 54 | 2,7 s |
| Eng: 16 ICs, 45 x 45 mm, 4 Lagen | 71 | 71 | 100 % | 95 | 0 | 4,7 s |
| **Eng: 25 ICs, 55 x 55 mm, 2 Lagen** | 113 | 89 | **78,8 %** | 244 | 405 | 25,4 s |
| Eng: 25 ICs, 55 x 55 mm, 4 Lagen | 113 | 113 | 100 % | 171 | 0 | 10,6 s |
| **Kreuzung 48 Leitungen, 80 x 40 mm, 2 Lagen** | 48 | 39 | **81,2 %** | 110 | 406 | 43,7 s |
| Kreuzung 48 Leitungen, 80 x 40 mm, 4 Lagen | 48 | 48 | 100 % | 67 | 0 | 12,3 s |

Unbekannt ist, ob die beiden unvollständigen Platinen mit zwei Lagen überhaupt lösbar sind. Der Grenzwert von 400 Umlegungen wurde bei beiden erreicht. **Ein Vergleich mit einem kommerziellen Router fehlt.** Die Vielzahl an Vias zeigt, dass die Ergebnisse regelkonform, aber nicht optimiert sind.

## Simulation und 3D

Gemessen mit `apps/desktop/e2e/sim3d.mjs` in der Entwicklungsversion (debug), ein Lauf, derselbe Rechner.

| Vorgang | Dauer |
|---|---|
| Modell an der Grenze importieren (200 000 Dreiecke, drei Bauteile mit diesem Footprint, also 600 000 gezeichnet) | 4,0 s |
| Zwei Bilder beim Drehen mit diesem Modell, Median aus 20 | 33 ms, also etwa 60 Bilder je Sekunde |
| Projekt mit diesem Modell speichern | 2,5 s |
| Transient, 100 000 Schritte, RC-Schaltung mit vier Bauteilen | 1,5 s |

Die Bildrate ist auf 60 je Sekunde begrenzt, die Messung zeigt also nur, dass diese erreicht wird. Grenzwerte sind nicht festgelegt. Große Schaltungen wurden nicht gemessen: Die Engine rechnet mit dichten Matrizen, der Aufwand steigt mit der dritten Potenz der Knotenzahl.

## KI mit echtem lokalem Modell

Ollama 0.40.14, `qwen2.5:1.5b` (986 MB), komplett im Grafikspeicher einer NVIDIA RTX 4060 laut Server. Wenige Läufe, ein Rechner. Die Zeit hängt stark vom Modell ab.

| Vorgang | Wert |
|---|---|
| Erste Anfrage nach dem Herunterladen, einschließlich Laden des Modells | 119 s |
| Weitere kurze Anfragen | 0,5 bis 4 s |
| Geschwindigkeit | 110 bis 140 Tokens je Sekunde |
| Anfrage mit Schaltplan und Symbolbibliothek | 1866 Tokens |

## Clouddienst

`cargo run --release -p tronic-cloud --example load`. 32 Arbeiter, je 40 Runden aus Schreiben, Lesen und Auflisten mit 16 KB, Objektspeicher im lokalen Ordner, PostgreSQL 18 mit Voreinstellungen, alles auf demselben Rechner. Ein Lauf.

| Instanzen | Anfragen je Sekunde | Fehlgeschlagene Runden |
|---|---|---|
| 1 | 1849 | 0 |
| 2 | 1648 | 0 |
| 4 | 1651 | 0 |

**Das ist kein Nachweis horizontaler Skalierung: Der Durchsatz steigt nicht.** Lastgenerator, alle Instanzen, Datenbank und Speicher teilen sich einen Rechner und eine Datenbank, die Engstelle wurde nicht ermittelt. Belegt ist nur, dass mehrere Instanzen unter Last korrekt zusammenarbeiten. Ein echter Nachweis braucht mehrere Rechner.

## Selbsthosting

`scripts/verify_selfhost.py` führt die markierten Befehle aus `docs/SELF_HOSTING.md` unverändert aus, je Weg in einer frischen Kopie von Ubuntu 26.04 unter WSL 2 auf dem Entwicklungsrechner, mit Internetzugang. Letzter Lauf am 05.11.2026:

| Weg | Schritte | Ergebnis | Pakete | Bauen und Starten | Übrige Schritte zusammen |
|---|---|---|---|---|---|
| Ohne Docker | 8 | bestanden | 69 s | 67 s (Bauen) | 3 s |
| Docker | 7 | bestanden | 32 s | 110 s (Abbilder laden, Bauen, Starten) | 33 s |

Was die Läufe prüfen:

| Prüfung | Ohne Docker | Docker |
|---|---|---|
| TLS über den Proxy | Zertifikatspeicher des Systems | Eigene Zertifizierungsstelle |
| Anmeldung | ja | ja |
| Registrierung geschlossen | nicht in diesem Lauf, Test in `services/cloud/tests/team.rs` | ja |
| Zwei Geräte, Sperre, Zusammenführen, Audit-Protokoll | ja, `check_server` | **nein**, es läuft aber dasselbe Programm wie links |
| Daten nach einem Neustart | nicht in diesem Lauf | ja, ein abgelegtes Objekt |
| Sichern und Wiederherstellen | ja, drei Versionen und sieben Audit-Einträge unverändert | ja, Konto und ein abgelegtes Objekt |

Die Zeiten hängen stark von Netz und Rechner ab und sind nur ein Anhaltspunkt. **Das ist kein Nachweis für echte Hardware**, andere Distributionen oder einen Betrieb über längere Zeit.

## Reproduzierbare Builds

`scripts/release_build.mjs`, zweimal auf demselben Rechner in verschiedenen Zielordnern, von Grund auf, am 05.11.2026, Rust 1.99.1, Windows 11:

| Versuch | Ergebnis |
|---|---|
| `cargo build --release` ohne Einstellungen | Alle Programmdateien verschieden |
| Mit `scripts/release_build.mjs`, `troniccad.exe` (19,5 MB) | **Byte für Byte gleich**, SHA-256 beginnt mit `581e7b75` |

**Das zeigt nur die Wiederholbarkeit auf einem Rechner.** Ein zweiter Rechner, andere Versionen von Node und Visual Studio sowie macOS und Linux sind nicht geprüft.

## Linux

`scripts/verify_linux.py` auf frischem Ubuntu 26.04 unter WSL 2 am 05.11.2026, mit Rust 1.99.1:

| Schritt | Ergebnis | Dauer |
|---|---|---|
| Pakete und Rust einrichten | bestanden | 129 s |
| Rust-Tests | 520 bestanden, 0 fehlgeschlagen, ohne Datenbank, also ohne die Tests gegen PostgreSQL | 236 s |
| Oberflächentests | 131 bestanden | 7 s |
| Debian-Paket (7,4 MB) | gebaut | 172 s |
| AppImage (86,6 MB) | gebaut, nicht zur Auslieferung | 133 s |
| Installieren, starten, Bild prüfen, entfernen | bestanden | 30 s |

Der Lauf brach zweimal am Prüfskript ab und wurde nach dessen Korrektur auf demselben System ab der Installation fortgesetzt: einmal wegen automatischer Systemupdates, die die Paketsperre hielten. **Bedient wurde die Oberfläche unter Linux nicht.** Die Zahl der Rust-Tests stimmt mit Windows überein, weil Tests ohne Datenbank dort als bestanden zählen, ohne etwas zu prüfen.

