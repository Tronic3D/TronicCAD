# ADR 0001: Canvas 2D für den Schaltplan-Editor

Status: entschieden am 04.11.2026. Betrifft Risiko R3.

## Frage

Die Vorgabe bevorzugt wgpu für Grafikberechnungen. Wie zeichnet der Schaltplan-Editor innerhalb von Tauri?

## Optionen

| Option | Bewertung |
|---|---|
| Natives wgpu neben der WebView | Überlagerung von Fenster und WebView ist je Plattform anders und hier nicht auf macOS und Linux prüfbar. Hohes Risiko, für 2D ohne messbaren Nutzen |
| WebGPU in der WebView | Unter WebView2 vorhanden (gemessen). Auf WebKit unter Linux und macOS nicht verlässlich vorauszusetzen |
| **Canvas 2D in der WebView** | Überall verfügbar, kein zusätzlicher Code für Text und CJK-Schriften |

## Messung

`apps/desktop/e2e/spike-canvas.mjs`, in der echten Anwendung, Windows 11, WebView2 156, 1920 × 1080. Gemischt aus Symbolen (8 Linien, 2 Texte) und Leitungen. Gezeichnet ohne Sichtbereichsfilter, also der ungünstigste Fall. 16,7 ms entsprechen dem 60-Hz-Takt.

| Objekte | Median | 95. Perzentil |
|---|---|---|
| 1 000 | 16,7 ms | 17,1 ms |
| 10 000 | 16,7 ms | 17,3 ms |
| 50 000 | 25,8 ms | 27,1 ms |

## Entscheidung

Der Schaltplan-Editor zeichnet mit Canvas 2D. Die Zeichenlogik liegt in einem eigenen Modul ohne Abhängigkeit zu React, sodass sie austauschbar bleibt.

**Abweichung von der bevorzugten Technologie:** wgpu wird im Schaltplan nicht eingesetzt.

**Neubewertung in Phase 3:** Der PCB-Editor zeichnet ebenfalls mit Canvas 2D. Mit 8 000 Leiterbahnsegmenten hält das Zoomen den 60-Hz-Takt, siehe [reports/PERFORMANCE.md](../../reports/PERFORMANCE.md). Nicht gemessen sind sehr große Flächen mit vielen Löchern. Für 3D wird in Phase 4 neu entschieden.

## Grenzen

Nur auf dieser Windows-Maschine gemessen. Auf macOS und Linux ist die Messung zu wiederholen, sobald dort gebaut wird.
