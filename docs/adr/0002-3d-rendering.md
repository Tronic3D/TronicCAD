# ADR 0002: 3D-Darstellung und eigener Simulator

Stand 04.11.2026. Status: entschieden für Phase 4.

## 3D-Darstellung mit WebGL2 statt wgpu

**Vorgabe:** Der Auftrag nennt wgpu als Grafikbasis. ADR 0001 hat das für Schaltplan und Leiterplatte begründet abgelöst und die 3D-Entscheidung auf Phase 4 verschoben.

**Entscheidung:** Ein eigener WebGL2-Renderer in `apps/desktop/src/view3d`. Die Geometrie entsteht in Rust (`tronic-3d`), die Oberfläche zeichnet sie nur.

**Begründung:**

- WebGL2 ist in WebView2, WebKitGTK und WKWebView vorhanden. WebGPU ist in den WebKit-Varianten nicht zuverlässig verfügbar, und Tauri-Fenster mit eigenem wgpu-Oberflächenzugriff bräuchten ein zweites Fenstersystem neben der React-Oberfläche.
- Keine neue Abhängigkeit, also keine neue Lizenzfrage.
- Gemessen reicht es: 600 000 Dreiecke mit etwa 60 Bildern je Sekunde auf dem Entwicklungsrechner ([PERFORMANCE](../../reports/PERFORMANCE.md)). Das gilt nur für diesen Rechner. Auf macOS und Linux ist es nicht geprüft.

**Folgen:** Ohne WebGL2 zeigt die Anwendung eine Meldung statt der Ansicht, die mechanische Prüfung arbeitet weiter, weil sie im Backend läuft. Der Verlust des Grafikkontexts wird behandelt und per Test ausgelöst.

## Eigener Simulator statt ngspice

**Vorgabe:** Der Auftrag verlangt eine rechtlich geprüfte Simulations-Engine, die Roadmap nannte ngspice und eine Lizenzfreigabe als Abnahme.

**Entscheidung:** Eine eigene Engine in Rust (`tronic-sim`). ngspice wird weder ausgeliefert noch gebunden. Es dient nur auf Entwicklerrechnern als unabhängige Gegenprobe (`scripts/verify_sim.py`).

**Begründung:**

- Eine ngspice-Freigabe hängt am Build, an einzelnen Gerätemodellen und an einer juristischen Prüfung, die wir nicht selbst leisten können (IP_COMPLIANCE.md). Eigener Code hat diese Abhängigkeit nicht.
- Für den geforderten Umfang (Arbeitspunkt, DC, AC, Transient) ist die Mathematik überschaubar: modifizierte Knotenanalyse, Newton-Verfahren, Trapezregel.
- Nachvollziehbarkeit: Die Anwendung zeigt zu jedem Lauf die verwendeten Modelle und die gleichwertige SPICE-Netzliste. Jeder kann das Ergebnis mit einem anderen Simulator nachrechnen.

**Preis:** Der Funktionsumfang bleibt deutlich hinter ngspice zurück. Es fehlen Sperrschichtkapazitäten, BSIM-Modelle, Herstellermodelle und `.subckt`. Die Grenzen stehen in [docs/specs/simulation.md](../specs/simulation.md). Wenn diese Tiefe gebraucht wird, ist die Frage neu zu stellen, ngspice dann als separater Prozess und nach juristischer Prüfung.

## STEP ist nicht umgesetzt

STEP zuverlässig zu lesen braucht einen CAD-Kern. Der einzige ernsthafte freie Kandidat, Open CASCADE, ist nicht freigegeben (LGPL-2.1 mit Ausnahme, IP_COMPLIANCE.md). Der Auftrag fordert STEP nur „soweit technisch und lizenzrechtlich möglich“. Die Anwendung lehnt STEP-Dateien daher mit einer klaren Begründung ab und empfiehlt den Export als STL oder WRL.
