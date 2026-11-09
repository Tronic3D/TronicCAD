# Entwicklungsbericht

## 09.11.2026: Umstellung auf freie Software

TronicCAD steht jetzt unter Apache-2.0, ohne Editionen, Aktivierung und Bezahlsystem, siehe [ADR 0004](../docs/adr/0004-open-source.md). Editionen, Lizenzdienst und Paddle-Anbindung sind entfernt. Neu sind Erweiterungen ([Anleitung](../docs/EXTENSIONS.md)) und die Anzeige aller Fremdkomponenten in der Anwendung. Das Repository bleibt privat, bis der Auftraggeber anders entscheidet.

## 05.11.2026: Phase 10, Veröffentlichung

Windows-Installer, Debian-Paket, Handbuch, Support-Ablauf, Website und die Bewertung der 16 Kriterien in [docs/RELEASE_READINESS.md](../docs/RELEASE_READINESS.md). **Nicht abgenommen, keine Freigabe erteilt.**

| Entscheidung | Grund |
|---|---|
| Installer für den angemeldeten Benutzer | Nur so kann das Update-System das Programm ohne Administratorrechte ersetzen |
| Handbuch in der Anwendung | Funktioniert ohne Verbindung, und die Website liest dieselbe Datei |
| Website aus denselben Dateien wie die Anwendung | Editionen und Handbuch können nicht auseinanderlaufen |
| Keine Preise, kein Download, Suchmaschinen ausgesperrt | Es gibt nichts zu kaufen, und ohne Impressum darf die Seite nicht öffentlich sein |
| Keine Vertragstexte | Ohne Verkäufer und Prüfung gäben sie falsche Sicherheit |
| AppImage aus den Standardzielen entfernt | Es enthält WebKitGTK unter LGPL, ungeklärt |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| **Datenverlust möglich:** Ein erneutes Hochladen einer schon gespeicherten Fassung konnte bei einem Abbruch die einzige gute Kopie durch eine halbe ersetzen. Trat nur selten auf, ein Zufallstest fand es in der ersten CI unter Linux | CI, danach 20 000 Durchläufe, die eine erste Korrektur als unzureichend zeigten | Eine vorhandene Kopie wird nur noch ersetzt, wenn sie mehrfach gelesen falsch ist. Danach 24 000 Durchläufe ohne Verlust |
| Das Bildschirmfoto unter Linux war schwarz, obwohl der Prozess lief und die Prüfung bestand. Das Fenster öffnete sich über Wayland statt auf dem virtuellen Bildschirm | Ansehen des Bildes | Eigener Bildschirm erzwungen, einfarbiges Bild gilt als Fehler |
| Eine aktualisierte Installation zeigte in Windows weiter die alte Version | Durchdenken des Zusammenspiels | Eintrag wird nach einem Update gesetzt, Test |
| Das Update-System hätte beim Deinstallieren Dateien hinterlassen | Durchdenken | Eigener Schritt im Installer, Test |
| Das README behauptete, 3D, KI, Cloud und mehr gebe es nicht | Beim Sammeln der Belege | Korrigiert |
| Bildschirmfotos für die Website enthielten einen lokalen Pfad mit Benutzernamen | Ansehen der Bilder | Statusleiste abgeschnitten, Prüfung der fertigen Seiten |
| Britische Schreibweise „Licences“ in der Sprache en-US | Ansehen des Bildes | Korrigiert |

## 05.11.2026: Phase 9, Updates und Sicherheit

Signierte Updates mit Kanälen, Installation mit Sicherung und Rücknahme, Veröffentlichungsablauf, Schlüsselverwaltung, Stückliste für Maschinen, Vorarbeit zum CRA. Spezifikation: [docs/specs/updates.md](../docs/specs/updates.md). **Nicht abgenommen:** Penetrationstest und fachliche CRA-Prüfung können nur extern erfolgen.

| Entscheidung | Grund |
|---|---|
| Eigenes Update-Verfahren statt Tauri-Updater | CVE-2026-95625, Vorgabe in SECURITY.md: Die Signatur muss Version und Kanal abdecken |
| Laufende Nummer und Ablaufzeit im Manifest | Gegen das Einspielen alter Manifeste und das Zurückhalten von Updates |
| Das alte Programm bleibt bis zur Bestätigung des neuen | Nur so kann eine Fassung zurückgenommen werden, die gar nicht startet |
| Automatische Suche an, Installation nur nach Klick | Annex I des CRA nennt automatische Sicherheitsupdates als Voreinstellung, wo anwendbar. Ein Neustart ohne Zutun gefährdet aber laufende Arbeit. Bleibt zur fachlichen Prüfung offen |
| Zwei eingebaute Schlüssel | Ohne vorbereiteten zweiten Schlüssel wäre ein Wechsel unmöglich |
| Keine Daten-Rücknahme | Das Zurückschreiben alter Einstellungen könnte neuere überschreiben. Die Sicherung bleibt im Ordner |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| Zwei Release-Builds ergaben verschiedene Programme, die Vorgabe eines reproduzierbaren Ablaufs war nicht erfüllt | Messung | `scripts/release_build.mjs`, festgelegte Rust-Version, danach Byte für Byte gleich |
| Die Stückliste war nur Markdown, der CRA verlangt ein maschinenlesbares Format | Lesen der Verordnung | `sbom.cdx.json` |
| Ein Team-Test schlug zeitweise fehl: Seine Dauerlast öffnete so schnell Verbindungen, dass Windows die Anschlüsse ausgingen | Qualitätsprüfung, unter Last nachgestellt | Last im Test gedrosselt. Der Dienst war nicht betroffen |
| Der Clouddienst antwortete manchmal, bevor er die Anfrage ganz gelesen hatte. Die Verbindung konnte abbrechen und eine gelungene Anfrage als Fehler erscheinen, auch bei echten Nutzern | Team-Tests unter Last, zuvor schon im Lizenzdienst | Kleine Anfragen werden immer ganz gelesen, Test mit großem Inhalt |
| Ein reiner Release-Build mit Cargo hätte die Entwicklungsadresse der Oberfläche enthalten | Durchdenken des Ablaufs | Das Skript baut über Tauri |

### Offene Entscheidungen aus dem CRA

Falls der CRA gilt, sind Sicherheitsupdates für jede verkaufte Hauptversion voraussichtlich mindestens fünf Jahre kostenlos bereitzustellen. Das betrifft ADR 0003, siehe [docs/CRA_ASSESSMENT.md](../docs/CRA_ASSESSMENT.md).

## 04.11.2026: Phase 8, Lizenzen

Editionen, Lizenzdateien, Lizenzdienst, Zahlungsschnittstelle, Verwaltung und Lizenzmodul. Spezifikation: [docs/specs/license.md](../docs/specs/license.md). **Nicht abgenommen:** Paddles Sandbox und die juristische Prüfung stehen aus, beides geht nur mit dem Auftraggeber. Geschäftsentscheidungen des Auftraggebers: [ADR 0003](../docs/adr/0003-commercial-decisions.md).

| Entscheidung | Grund |
|---|---|
| Ed25519, Prüfung ohne Verbindung | Vorgabe: Ein Serverausfall darf nichts stilllegen |
| Eine Tabelle für beide Seiten | Vorgabe: keine mehrfach programmierten Editionsgrenzen |
| Fertigungsdaten bleiben nach Ablauf möglich | ADR 0003: Projekte lassen sich immer exportieren. Die Vorgabe sieht Fertigungsexport erst ab Professional vor, Free bleibt deshalb ohne |
| Testanbieter nutzt dasselbe Lesen und Verarbeiten wie Paddle | So prüft der End-to-End-Test möglichst viel des echten Wegs. Nachgewiesen ist Paddle damit trotzdem nicht |
| Eigene Datenbank für den Dienst | Beim ersten Testlauf kollidierten Tabellen mit dem Clouddienst |
| Keine künstlichen Unterschiede bei „erweiterten“ Funktionen | Lieber fehlt ein Unterschied, als dass Funktionen zum Schein eingeschränkt werden |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| Wer die Uhr einmal zu weit vorgestellt hatte, wäre auch mit neuer Lizenzdatei nie wieder freigekommen | Durchdenken des Tests zur Uhr | Eine Antwort des Dienstes setzt die gemerkte Zeit zurück, Schritt im End-to-End-Test |
| Der Testanbieter hätte in einem falsch eingestellten Betrieb Lizenzen verschenkt | Durchsicht der Konfiguration | Start nur mit ausdrücklicher Einstellung |
| Antworten ohne den Anfrageinhalt zu lesen konnten vermutlich die Verbindung unter Windows abbrechen, etwa beim Abmelden. Die Ursache ist abgeleitet, nicht gezielt nachgestellt | Fehlschlag in der Qualitätsprüfung und in einer von sechs Wiederholungen | Inhalt wird immer gelesen. Danach 30 Läufe ohne diesen Fehler |
| Drei Tests schlugen zeitweise fehl: Suche nach Ziffern traf Transaktionsnummern, feste Adresse sammelte Fehlversuche über Läufe, gleichzeitig angelegte Lizenzen hatten keine Reihenfolge | Wiederholte Läufe | Tests korrigiert. Der Dienst war davon nicht betroffen |
| Lizenzmodul mit Cloud-Symbol, deutsche Meldung nach Sprachwechsel, Test-Kennzeichen an Free, Testzeitraum auch nach seiner Nutzung angeboten | Ansehen der Bildschirmfotos | Behoben |

### Nicht erledigt

Paddle-Sandbox, Vertragsbedingungen, laufender Dienst, Konto löschen im Lizenzdienst, einzelne Verwaltungskonten, Mailversand. Vollständig in [KNOWN_ISSUES.md](KNOWN_ISSUES.md).

## 04.11.2026: Phase 7, Team

Teams, Rollen, Bereiche und Sperren, Kommentare, Audit-Protokoll und Selbsthosting. Spezifikation: [docs/specs/team.md](../docs/specs/team.md), Anleitung: [docs/SELF_HOSTING.md](../docs/SELF_HOSTING.md). Ubuntu in WSL wurde mit Zustimmung des Auftraggebers für den Installationstest eingerichtet.

| Entscheidung | Grund |
|---|---|
| Der Server ermittelt geänderte Bereiche aus den Dateien | Nur so halten Sperren und Rollen auch gegen einen veränderten Client |
| Zusammenführen nur bereichsweise | Vorgabe H1. Innerhalb eines Bereichs würde es raten, und falsche elektrische Daten fallen spät auf |
| Abweichungen melden statt verhindern | Dieselbe Regel wie nach jeder Änderung am Schaltplan. Der Abgleich bleibt eine Entscheidung |
| Rollen in einer einzigen Tabelle | Vorgabe: keine mehrfach programmierten Rechte. Der Test schreibt die Tabelle noch einmal aus |
| Audit im selben Datenbankvorgang wie die Änderung | Weder Änderung ohne Eintrag noch Eintrag ohne Änderung |
| Anleitung wird wörtlich ausgeführt | Eine geprüfte Kopie der Befehle würde auseinanderlaufen |
| Zertifikatspeicher des Betriebssystems | Beim Durchdenken des Selbsthostings fiel auf, dass eigene Zertifizierungsstellen sonst scheitern |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| Das Löschen eines Kontos hätte Teamprojekte mitgenommen | Durchsicht des Schemas | Projekte wechseln zu einem Administrator, Test |
| Nachladen im Hintergrund löschte Erfolgsmeldungen | End-to-End-Test | Getrennter Weg ohne Löschen |
| Die Anleitung zur Wiederherstellung las die Sicherung als Benutzer `postgres` aus `/root`, was scheitert, nachdem die Datenbank schon gelöscht ist | Wörtliches Ausführen der Anleitung | Sicherung über die Standardeingabe |
| Die Prüfung nach der Wiederherstellung hätte auch mit leerem Objektspeicher bestanden, weil sie nur ein neues Projekt anlegte | Durchsicht des eigenen Prüfprogramms | `check_server --save` und `--compare` vergleichen Versionen und Audit-Protokoll. Gegenprobe mit geleertem Speicher schlägt fehl |
| Für den Docker-Weg fehlte eine Anleitung zum Sichern, und der Neustart prüfte nur die Anmeldung | Durchsicht der Anleitung | Sichern und Wiederherstellen ergänzt, beide Schritte vergleichen ein abgelegtes Objekt |
| Jeder Start ersetzte die Audit-Trigger. Laufende Anfragen anderer Instanzen konnten sich damit verklemmen, und der Start scheiterte | Einmaliger Fehlschlag in der Qualitätsprüfung, Ursache im Protokoll von PostgreSQL | Schema nur bei Änderung anwenden, bei Verklemmung neuer Versuch. Der Test scheitert nachweislich mit dem alten Verhalten |
| Das Prüfskript veränderte Befehle: `wsl.exe` schickte Argumente durch eine weitere Shell, Windows machte aus Zeilenenden CR LF | Erste Läufe | Befehle als Bytes über die Standardeingabe, Ausführung aus einer Datei |

### Nicht erledigt

Mailversand, Sperren in den Editoren, Projektrollen in der Oberfläche, Echtzeit-Zusammenarbeit, unabhängige Sicherheitsprüfung, Betrieb auf echter Hardware. Vollständig in [KNOWN_ISSUES.md](KNOWN_ISSUES.md).

### Nächster Schritt

Phase 8, Kommerzielles System, nach Entscheidung des Auftraggebers. Dafür sind Geschäftsfragen zu klären: Zahlungsdienstleister, Preise, Lizenzbedingungen.

## 04.11.2026: Phase 6, Cloud

Synchronisierung, Sicherung und ein eigener Clouddienst. Spezifikation: [docs/specs/cloud.md](../docs/specs/cloud.md). Rclone und PostgreSQL wurden mit Zustimmung des Auftraggebers als Testwerkzeuge installiert.

| Entscheidung | Grund |
|---|---|
| Unveränderliche Versionen wie bei Git statt bedingter Schreibvorgänge | Läuft auf jedem Speicher, kann nichts überschreiben, und gleichzeitiges Schreiben wird immer sichtbar |
| Kein Löschen in der Speicherschnittstelle | Ein Fehler in der Synchronisierung kann keine Version zerstören. Preis: Der Speicher wächst |
| Clouddienst ohne eigenen Zustand je Instanz | Vorgabe zur horizontalen Erweiterung |
| `tokio-postgres` statt `sqlx` | `sqlx` kollidiert im Abhängigkeitsbaum mit dem SQLite der Desktop-App |
| Keine Ende-zu-Ende-Verschlüsselung in dieser Phase | Schlüsselverlust und Freigaben wären eigene Produktfragen. Offen gekennzeichnet |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| **WebDAV: Große Uploads in fehlende Ordner brachen ab**, der Server schließt die Verbindung vor dem Fallback | Test mit 8 MB gegen echten Server | Ordner werden vorab angelegt |
| Antworten mit fremdem Namensraum-Präfix ergaben leere Einträge | Unit-Test | Schließende Tags werden erkannt |
| Der Dienst las bis zu 600 MB vor der Anmeldeprüfung | Durchsicht | Anmeldung zuerst, JSON auf 64 KB begrenzt, Test |
| Offene WebSocket-Verbindungen überlebten das Abmelden | Durchsicht | Sitzung wird je Ereignis geprüft, Test |
| Passwort-Hashing und große Transfers waren unbegrenzt parallel | Durchsicht | Begrenzt |

### Nicht erledigt

Lastnachweis auf mehreren Rechnern, unabhängige Sicherheitsprüfung, Tests gegen Amazon S3 und Nextcloud, Betriebsmodell. Vollständig in [KNOWN_ISSUES.md](KNOWN_ISSUES.md). Aus Phase 5 weiter offen: ein stärkeres Modell und ein Cloud-KI-Anbieter.

### Nächster Schritt

Phase 7, Team, nach Entscheidung des Auftraggebers.

## 04.11.2026: KI mit echtem Modell

Auf Wunsch des Auftraggebers lief der Assistent vor Phase 6 gegen ein echtes lokales Modell: Ollama, per winget installiert, mit `qwen2.5:1.5b`. Beides gehört nicht zum Produkt.

| Fund | Gefunden durch | Lösung |
|---|---|---|
| **Ollama schneidet große Anfragen ohne Meldung ab**, das Modell antwortet dann aus einem Teil der Daten | Messung nach Hinweis in den Serverdaten | Warnung in der Anwendung. Nicht verhindert, siehe KNOWN_ISSUES.md |
| Das Modell schrieb seine Änderungen in anderer Form, die Anwendung ignorierte sie stillschweigend | Echtes Modell | Mehr Schreibweisen werden gelesen, alle laufen durch dieselben Prüfungen |
| Das Modell nannte einen Wert aus dem Prompt-Beispiel als Wert des Projekts | Echtes Modell | Beispiele im Prompt sind jetzt Platzhalter, per Test belegt |
| Bei einem unlesbaren Block sah der Benutzer nicht, was das Modell schrieb | Echtes Modell | Der Text wird gezeigt |

**Ergebnis:** Technik und Schutzmechanismen halten: Streaming, Speicher- und GPU-Anzeige, unberührte Prüfergebnisse, jede falsche Änderung abgelehnt. **Dieses Modell hat in vier Versuchen keinen brauchbaren Schaltungsvorschlag geliefert, und seine Antworten sind fachlich unbrauchbar.** Die Freigabe eines echten gültigen Vorschlags ist nicht belegt. Der Prompt wurde nicht weiter auf dieses Modell abgestimmt, weil das an einem so kleinen Modell wenig über stärkere aussagt.

Ein Cloud-Anbieter wurde weiterhin nicht angesprochen.

## 04.11.2026: Phase 5, KI

Der Assistent ist ein echtes Modul. Spezifikation: [docs/specs/ai.md](../docs/specs/ai.md).

| Entscheidung | Grund |
|---|---|
| Regelanalyse ohne Modell als Grundfunktion | Funktioniert immer, ohne Netz, und ist nachprüfbar. Ausdrücklich kein Prüfergebnis |
| Kein mitgeliefertes Modell | Modelllizenzen, Größe, freie Wahl. Abweichung, in Roadmap und Architektur vermerkt |
| Lokal oder extern folgt aus der Adresse | Eine Einstellung ließe sich falsch setzen |
| Zustimmung im Backend, festgehaltene Anfrage | Was gezeigt wurde, ist genau das, was gesendet wird. Fehler in der Oberfläche können nichts senden |
| Geschlossene Änderungsliste, Probelauf auf einer Kopie | Prüfergebnisse lassen sich gar nicht ausdrücken, erfundene Bauteile fallen vor der Vorschau auf |
| Keine Weiterleitungen, außen nur https | Daten und Schlüssel können nicht umgelenkt werden |
| Claude über direktes HTTP | Für Rust gibt es kein offizielles SDK |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| **„Anhalten“ schickte die Anfrage erneut ab:** Während des Klicks wurde aus dem Knopf der Senden-Knopf, der Browser löste dann das Formular aus | End-to-End-Test | Eigene Schlüssel für beide Knöpfe, Sperre in der Sendefunktion, Test zählt die Anfragen |
| Nicht ausführbare Änderungen waren wählbar und zugleich als abgelehnt gelistet | End-to-End-Test | Nur funktionierende Änderungen sind wählbar |
| Abhängige Änderungen entfielen beim Abwählen stillschweigend | Prüfung des Codes | Werden ausgelassen und gemeldet |
| Das Gespräch zeigte die kompletten Projektdaten statt der Frage | Screenshot | Getrennter Verlauf für Anzeige und Modell |
| Alle neuen Bauteile eines Vorschlags konnten auf demselben Platz landen | Prüfung des Codes | Platzierung in Reihen, Test |

### Nicht erledigt

Echtes Modell nicht angesprochen, Betrieb ohne Netz nicht mit getrenntem Netzwerk geprüft, Antwortqualität nicht bewertet. Vollständig in [KNOWN_ISSUES.md](KNOWN_ISSUES.md). macOS und Linux weiterhin nie ausgeführt, die CI-Datei nie gelaufen.

### Nächster Schritt

Phase 6, Cloud, nach Entscheidung des Auftraggebers.

## 04.11.2026: Phase 4, 3D und Simulation

Simulation und 3D-Ansicht sind echte Module. Spezifikationen: [simulation.md](../docs/specs/simulation.md), [view3d.md](../docs/specs/view3d.md), Entscheidungen: [ADR 0002](../docs/adr/0002-3d-rendering.md).

| Entscheidung | Grund |
|---|---|
| Eigene Simulations-Engine statt ngspice | Keine offene Lizenzfrage im Produkt. Preis: deutlich weniger Modelltiefe |
| ngspice nur als Gegenprobe auf dem Entwicklerrechner | Unabhängige Prüfung ohne Weitergabe |
| WebGL2 statt wgpu | Auf allen drei Webviews vorhanden, keine neue Abhängigkeit |
| STEP nicht umgesetzt | Kein lizenzrechtlich freigegebener CAD-Kern |
| Kollisionsprüfung im Backend mit Quadern | Unabhängig von der Grafik, exakt für eingebaute Körper, bei Importen auf der sicheren Seite |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| `1N4148` wurde als Zahlenwert gelesen | Referenztest | Ziffern nach dem Vorsatz begrenzt |
| Newton meldete Konvergenz, solange die Spannungsbegrenzung griff, MOSFET und BJT lieferten Unsinn | Vergleich mit ngspice | Konvergenz erst am unbegrenzten Punkt |
| Zener wich ab | Vergleich mit ngspice | Durchbruch bei 1 mA definiert |
| Temperaturspannung galt für 300 K statt 27 °C | Vergleich mit ngspice | Korrigiert |
| Trapezregel schwang an Pulsflanken | Vergleich mit ngspice | Dämpfung mit Rückwärts-Euler |
| Diese Dämpfung verschob Flanken (Abweichung 14 % am MOSFET-Schalter) | Vergleich mit ngspice | Gedämpfte Schritte achtfach unterteilt |
| **Angegebener DC-Wert wurde ignoriert, sobald die Quelle auch einen PULSE hatte** | End-to-End-Test, der Vergleich mit ngspice deckte den Fall nicht ab | Verhalten wie ngspice, drei neue Vergleichsfälle |
| Azimut der Kamera lief bei negativen Winkeln aus dem Bereich | Unit-Test | Modulo korrigiert |
| Modellbeschreibungen kamen auf Englisch aus dem Backend | Screenshot | Code und Parameter statt Text, in vier Sprachen übersetzt |
| Ein Modell gilt für alle Bauteile des Footprints, die Oberfläche sagte das nicht | End-to-End-Test | Hinweis ergänzt |

### Nicht erledigt

STEP, Schrittweitensteuerung, IC-Modelle, Transformationen in WRL. Vollständig in [KNOWN_ISSUES.md](KNOWN_ISSUES.md). macOS und Linux weiterhin nie ausgeführt, die CI-Datei nie gelaufen.

### Nächster Schritt

Phase 5, KI, frühestens nach Entscheidung des Auftraggebers.

## 04.11.2026: Autorouter

Der Router verlegt offene Verbindungen als Vorschlag mit Vorschau. Spezifikation: [docs/specs/autorouter.md](../docs/specs/autorouter.md), Messwerte: [PERFORMANCE.md](PERFORMANCE.md).

| Entscheidung | Grund |
|---|---|
| Der echte DRC prüft jeden Vorschlag zum Schluss | Ein Fehler im Router kann so keine Verletzung auf die Platine bringen |
| Offene Verbindungen werden auf der fertigen Platine gemessen | Der Bericht kann nicht vom Ergebnis abweichen |
| Rechnen außerhalb der Sitzungssperre | Die Oberfläche bleibt bedienbar, automatisches Sichern läuft weiter |

| Während der Entwicklung gefundener Fehler | Gefunden durch | Lösung |
|---|---|---|
| Ein verspätetes Fortschrittsereignis öffnete den Dialog wieder | End-to-End-Test | Ereignisse nach Ende des Routers werden ignoriert |
| Das Vorschlagsbanner lag im Dokument, war aber unsichtbar | Screenshot, nachdem der Test hing | Flex-Layout statt Grid-Regeln, Test prüft jetzt die sichtbare Position |
| Vias wurden im Raster mit falschem Durchmesser eingetragen | Durchsicht | Echter Durchmesser, im selben Schritt behoben |

## 04.11.2026: Phase 3, Leiterplatte

### Ergebnis

Der PCB-Editor mit Abgleich zum Schaltplan, DRC und Fertigungsdaten. Testergebnisse in [QUALITY_REPORT.md](QUALITY_REPORT.md), Messwerte in [PERFORMANCE.md](PERFORMANCE.md).

### Entscheidungen

| Entscheidung | Grund |
|---|---|
| Canvas 2D auch für PCB | Gemessen ausreichend, siehe ADR 0001 |
| Das Backend liefert alle Pads fertig umgerechnet | Oberfläche und Prüfung können nicht auseinanderlaufen |
| Abstände exakt über Kapseln und Rechtecke | Keine Rasterfehler, Grenzwerte sind exakt testbar |
| Kupferflächen werden berechnet, nicht gespeichert | Können nicht veralten, Dateien bleiben klein |
| Autorouter nicht gebaut, nur spezifiziert | Ein schlechter Router wäre eine Scheinfunktion |
| Transistor-Symbole mit SOT-23-Pinnummern | Pin und Pad müssen sich über die Nummer abbilden lassen |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| Nicht durchkontaktierte Bohrungen wurden mit der Pad-Größe statt der Bohrung geprüft | DRC-Testfall | Für solche Bohrungen zählt der Bohrdurchmesser |
| Neue Bauteile landeten außerhalb des Rasters, verschobene Teile trafen danach nie das Raster | End-to-End-Test | Platzierung auf vollen Millimetern |

### Nächster Schritt

Eine Testplatine bei einem Fertiger bestellen. Das ist die einzige Prüfung, die Fehler in den Fertigungsdaten wirklich aufdeckt.

## 04.11.2026: Automatisches Sichern

Ungespeicherte Arbeit wird alle 30 Sekunden gesichert und nach einem Absturz angeboten. Spezifikation: [docs/specs/autosave.md](../docs/specs/autosave.md).

| Entscheidung | Grund |
|---|---|
| Sicherungen im lokalen Benutzerprofil statt neben dem Projekt | Funktioniert auch bei schreibgeschütztem oder getrenntem Netzlaufwerk, hinterlässt keine Dateien in Projektordnern |
| Betriebssystem-Sperre je laufender Anwendung | Das System gibt sie bei jedem Prozessende frei. So wird kein Stand einer noch laufenden Anwendung als Absturz angeboten |
| Wiederhergestellte Arbeit gilt als ungespeichert | Die Datei ändert sich erst auf Wunsch, die Konflikterkennung bleibt wirksam |
| Eigener Datenordner für End-to-End-Tests (`TRONICCAD_DATA_DIR`) | Absturz-Tests dürfen keine Dialoge in der echten Installation hinterlassen |

## 04.11.2026: Phase 2, Schaltplan

### Ergebnis

Ein bedienbarer Schaltplan-Editor mit eigener Bibliothek, ERC und Netzliste. Testergebnisse in [QUALITY_REPORT.md](QUALITY_REPORT.md), Messwerte in [PERFORMANCE.md](PERFORMANCE.md).

### Entscheidungen

| Entscheidung | Grund |
|---|---|
| Canvas 2D statt wgpu im Schaltplan | Gemessen ausreichend, überall verfügbar ([ADR 0001](../docs/adr/0001-schematic-rendering.md)) |
| Rückgängig über Zustände mit geteilten Seiten | Einfach und korrekt für jeden Befehl, kostet nur die jeweils geänderte Seite |
| Nur waagerechte und senkrechte Leitungen | Verbindungsregeln bleiben eindeutig und schnell |
| Eine Seite nur einmal als Block | Eindeutige Referenzen ohne Pfadzuordnung, spätere Erweiterung möglich |
| Projekte enthalten Kopien ihrer Symbole | Bleiben ohne Bibliothek lesbar, Bibliotheksänderungen verändern keine alten Pläne |

### Während der Entwicklung gefundene Fehler

| Fehler | Gefunden durch | Lösung |
|---|---|---|
| Beim ersten Öffnen wurde die Ansicht vor der Größenbestimmung der Zeichenfläche berechnet, Klicks landeten daneben | End-to-End-Test | Ansicht erst berechnen, wenn Dokument und Größe bekannt sind |
| Versorgungssymbol und globales Label gleichen Namens bildeten zwei Netze mit einem Namen | Durchsicht beim Schreiben der Regeln | Gemeinsame Namensgruppe, Test ergänzt |
| Zusammenfassen von Leitungen prüfte beim Einschluss eines Stücks den falschen Endpunkt | Durchsicht, abgesichert durch Zufallstest über 200 Schaltungen | Korrekter Punkt geprüft |
| Bauteilnamen der Bibliothek waren nicht übersetzt | Durchsicht der Oberfläche | Übersetzungen und Test für jedes allgemeine Bauteil |

### Nächster Schritt

Vor Phase 3 ist ein Praxistest sinnvoll.

## 04.11.2026: Phase 0 und erster Softwarestand

### Ausgangslage

Leerer Ordner. Vorhanden waren Rust 1.99, Visual Studio 2022 mit MSVC, Windows SDK und WebView2. Installiert wurden Node.js 26 LTS, cargo-deny und cargo-audit.

### Ergebnis

Eine lauffähige Windows-Anwendung, die Projekte anlegt, speichert und öffnet, zwischen vier Sprachen umschaltet und eine Grundoberfläche zeigt. Die geforderten Dokumente, automatischen Prüfungen und die CI-Definition sind angelegt. Testergebnisse stehen in [QUALITY_REPORT.md](QUALITY_REPORT.md).

### Getroffene Entscheidungen

| Entscheidung | Grund |
|---|---|
| Projektdatei als JSON statt SQLite | Vorgabe verlangt Netzlaufwerke und NAS, wo SQLite-Sperren unzuverlässig sein können (ARCHITECTURE.md) |
| Tauri 2 statt 3 | Tauri 3 ist nur als Alpha verfügbar |
| Backend besitzt das Dokument | Kernlogik in Rust, UI austauschbar und testbar |
| ngspice und STEP-Bibliothek noch nicht integriert | Lizenzprüfung ist Voraussetzung |

### Während der Entwicklung gefundene Fehler

| Fehler | Ursache | Lösung | Nachweis |
|---|---|---|---|
| Nach dem Öffnen einer Kopie mit gleicher Projekt-ID zeigte das Formular veraltete Daten und hätte sie beim nächsten Bearbeiten übernommen | Komponente war per Projekt-ID verschlüsselt, Kopien teilen die ID | Eigener Dokumentschlüssel je Öffnen | Test schlug vor der Korrektur fehl und besteht danach |

Der Test gegen fest eingebauten Text wurde zusätzlich mit einem absichtlich eingebauten Fehler geprüft und schlug wie erwartet an.

### Nicht erledigt

Siehe [KNOWN_ISSUES.md](KNOWN_ISSUES.md). Wichtigste Punkte: macOS und Linux ungetestet, CI nie ausgeführt, juristische Prüfung und Markenrecherche offen.

### Nächster Schritt

Phase 2, beginnend mit dem Grafik-Prototyp (Risiko R3) und dem Datenmodell für Schaltpläne.
