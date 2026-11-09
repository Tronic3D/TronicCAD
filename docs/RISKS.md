# Risikobewertung

Stand 04.11.2026. Bewertung: Eintritt und Auswirkung jeweils niedrig, mittel oder hoch.

| Nr. | Risiko | Eintritt | Auswirkung | Maßnahme |
|---|---|---|---|---|
| R1 | **Umfang.** Der Auftrag beschreibt mehrere Produkte: EDA-Suite, Autorouter, 3D, Simulation, KI, Cloud, Kollaboration, Lizenzsystem. Etablierte Systeme haben dafür viele Personenjahre gebraucht | Hoch | Hoch | Strikte Phasen, früher nutzbarer Kern, Editionen schrittweise; pro Phase Ziel neu bewerten |
| R2 | **Fachliche Korrektheit.** Fehler in ERC, DRC oder Gerber-Ausgabe kosten Kunden echte Fertigungsläufe | Mittel | Hoch | Referenzdesigns, unabhängiger Gerber-Viewer, Testfertigung vor Freigabe, Haftungsfragen juristisch klären |
| R3 | **Grafik in Tauri.** Für den Schaltplan entschieden: Canvas 2D ([ADR 0001](adr/0001-schematic-rendering.md)). Für PCB ebenfalls Canvas 2D nach Messung. Offen für 3D sowie die Messung auf macOS und Linux | Mittel | Mittel | In Phase 4 neu entscheiden |
| R4 | **Simulationsgenauigkeit.** Die eigene Engine hat vereinfachte Modelle und einen festen Zeitschritt. Ein zu großer Schritt ergibt ein plausibles, aber falsches Ergebnis. Die frühere Lizenzfrage zu ngspice entfällt, weil es nicht ausgeliefert wird | Mittel | Mittel | Vergleich mit ngspice an 21 Schaltungen, Grenzen in der Oberfläche genannt, Schrittweitensteuerung als offene Aufgabe |
| R5 | **STEP-Unterstützung.** Nicht umgesetzt. Viele Hersteller liefern 3D-Modelle nur als STEP, OCCT ist groß und LGPL | Hoch | Mittel | Import von STL, OBJ und WRL als Ersatz. Für STEP: separater Prozess, Rechtsprüfung, Alternativen und kommerzielle Lizenz bewerten |
| R6 | **Autorouter.** Grundfunktion vorhanden, regelkonform, aber auf engen Zweilagen-Platinen unvollständig und nicht mit kommerziellen Routern verglichen | Mittel | Mittel | Keine Marketingaussage vor Vergleich und Praxistest |
| R7 | **Plattformunterschiede.** WebKit unter macOS und Linux verhält sich anders als WebView2. Bisher nur Windows getestet | Hoch | Mittel | CI auf drei Plattformen, früher manueller Test, keine Annahmen aus Windows übertragen |
| R8 | **Marke.** „TRONI-CAD“ existierte bereits in derselben Warengruppe | Mittel | Mittel | Professionelle Recherche vor weiteren Markeninvestitionen, siehe [TRADEMARKS.md](../TRADEMARKS.md) |
| R9 | **Urheberrecht an KI-gestütztem Code** | Mittel | Mittel | Juristische Prüfung, siehe [COPYRIGHT.md](../COPYRIGHT.md) |
| R10 | **Bauteildaten.** Rechte an Symbolen, Footprints und 3D-Modellen sind oft unklar, Fehler führen zu Fehlfertigung | Mittel | Hoch | Herkunftsdatensatz je Element, eigene Bibliothek, Prüfprozess |
| R11 | **KI.** Erfundene Pinbelegungen oder Empfehlungen können Schaltungen beschädigen. Prompt-Injection über Bauteilwerte und Datenblätter lässt sich nicht sicher verhindern. Mit echten Modellen noch nicht erprobt | Hoch | Hoch | Vorschau und Freigabe, geschlossene Änderungsliste, Prüfung auf einer Kopie, echter ERC in der Vorschau, Trennung von ERC und DRC, Kennzeichnung als ungeprüft |
| R12 | **Regulatorik.** CRA gilt voll ab Dezember 2027, DSGVO mit Cloud und KI, Exportkontrolle für China nicht geprüft | Mittel | Hoch | Früh einplanen, externe Prüfung |
| R13 | **Betrieb.** Cloud, Lizenzserver und Support brauchen dauerhaft Personal und Budget. Der Clouddienst ist gebaut, aber nicht in Betrieb, Betriebsmodell und Kosten sind ungeklärt | Hoch | Hoch | Vor einem Start klären. Bis dahin funktionieren WebDAV, S3 und Ordner ohne eigenen Dienst |
| R14 | **Lieferkette.** Über 400 Fremdpakete | Mittel | Mittel | Audit in CI, Sperrdateien, Positivliste, nur crates.io |
