# Spezifikation: KI-Assistent

Stand 04.11.2026, Phase 5. Code: `crates/tronic-ai`, `apps/desktop/src-tauri/src/ai.rs`, `apps/desktop/src/ai`.

## Betriebsarten

Der Benutzer wählt selbst. Die Voreinstellung ist „Nur Regeln“: Ohne Einstellung baut die Anwendung keine Verbindung auf.

| Betriebsart | Was passiert | Netz |
|---|---|---|
| Nur Regeln | Feste Regeln im Backend, kein Modell | Keines |
| Lokaler Server | Eigener Server des Benutzers (Ollama, llama.cpp, LM Studio) über die OpenAI-kompatible Schnittstelle | Nur dieser Rechner |
| Anthropic | Messages API, Streaming | Extern, mit Zustimmung |
| Anderer Anbieter | OpenAI-kompatibel oder Anthropic-Protokoll, Adresse frei | Extern, mit Zustimmung, wenn nicht dieser Rechner |

**TronicCAD liefert kein Modell und keine Laufzeit aus.** Modellgrößen und GPU-Nutzung stellt der Benutzer im lokalen Server ein. Das ist eine Entscheidung: Jedes Modell hat eine eigene Lizenz (IP_COMPLIANCE.md), und ein mitgeliefertes Modell würde die Anwendung um Gigabyte vergrößern. Die Oberfläche zeigt, was der Server meldet: verfügbare Modelle, geladene Größe und Anteil auf der GPU (nur Ollama), außerdem freien Arbeitsspeicher und Tokens je Sekunde.

## Lokal oder extern

Das entscheidet allein die Adresse, nie eine Einstellung (`classify`):

- **Lokal** sind nur `localhost`, `127.0.0.0/8` und `::1`.
- **Extern** ist alles andere, auch ein Server im eigenen Netz und Namen wie `localhost.example.com`.
- Externe Adressen müssen https verwenden. Benutzername oder Kennwort in der Adresse werden abgelehnt.
- Weiterleitungen werden nicht verfolgt, damit eine Antwort Daten und Schlüssel nicht umlenken kann. Lokaler Verkehr läuft nie über einen Proxy.

## Zustimmung

Die Prüfung liegt im Backend (`ConsentStore`, `ai_send`). Ein Fehler in der Oberfläche kann also nichts senden.

1. `ai_prepare` baut die Anfrage, hält sie fest und gibt sie zurück. Es wurde nichts gesendet.
2. Bei einem externen Anbieter zeigt die Oberfläche genau diesen Inhalt, das Ziel und das Modell.
3. Der Benutzer wählt „Einmal senden“, „Für dieses Projekt erlauben“ oder Abbrechen.
4. `ai_send` sendet nur die festgehaltene Anfrage. Ohne Zustimmung gibt es `ai.consentRequired` und keine Verbindung.

Die Zustimmung für ein Projekt gilt für diesen Anbieter, bis das Projekt geschlossen wird, und wird nie gespeichert. Die Verbindungsprüfung sendet keine Projektdaten und läuft nur auf Knopfdruck.

## Was gesendet wird

Nur die Abschnitte, die bei der Anfrage angekreuzt sind: Bauteile und Netze, ERC-Ergebnis, Platinendaten, DRC-Ergebnis, Symbolbibliothek. Dazu der eingegebene Text, eingefügter Datenblatt-Text und der bisherige Gesprächsverlauf. **Nie** Dateipfade, Projektname oder Beschreibung. Bauteilwerte und Netznamen gehen mit, auch wenn sie vertraulich sein könnten. Gesendete Daten lassen sich nach jeder Anfrage ansehen.

Gespräche werden nicht gespeichert.

## API-Schlüssel

Der Schlüssel liegt im Schlüsselspeicher des Betriebssystems (Windows-Anmeldeinformationen, macOS-Schlüsselbund, Linux Secret Service), je Anbieteradresse. Er kommt nie zur Oberfläche zurück, sie erfährt nur, ob einer vorhanden ist. Ist der Speicher nicht verfügbar, gibt es eine Meldung und **keinen** Ersatz im Klartext.

## Abgeschnittene Anfragen

Ollama kürzt zu große Anfragen ohne Meldung (gemessen, siehe reports/KNOWN_ISSUES.md). Das Modell antwortet dann selbstsicher aus einem Teil der Daten. Die Anwendung vergleicht deshalb gesendete Zeichen, gezählte Tokens und den vom Server gemeldeten Kontext und warnt bei Verdacht (`cut_suspected`). Das ist eine Schätzung und verhindert das Abschneiden nicht.

## Änderungen

Ein Modell kann nur Text liefern und darin einen Block `tronic-changes` (auch in anderer Schreibweise, weil kleine Modelle sich nicht an die Form halten) mit Änderungen dieser geschlossenen Liste: `set_value`, `set_reference`, `place`, `connect`, `delete`. Zu den Änderungen:

- Jeder Eintrag wird einzeln gelesen. Unbekannte Typen und Felder werden abgelehnt und mit Grund gezeigt.
- Die Änderungen laufen zuerst auf einer Kopie. Was dort scheitert (erfundenes Bauteil, falscher Pin, unbekanntes Symbol), steht unter „Nicht angenommen“ und ist nicht wählbar.
- Die Vorschau listet jede Änderung mit Häkchen und nennt den echten ERC vorher und nachher.
- Erst „Übernehmen“ ändert das Projekt, als **ein** Rückgängig-Schritt.
- Hat sich der Schaltplan seit dem Vorschlag geändert, wird abgelehnt (`ai.stale`).
- Hängt eine gewählte Änderung von einer abgewählten ab, wird sie ausgelassen und gemeldet.

Änderungen an der Leiterplatte kann ein Modell nicht vorschlagen, nur beschreiben.

## Prüfergebnisse

ERC, DRC und Simulation laufen ohne KI. Der Typ `Change` kann kein Prüfergebnis ausdrücken, und die Zahlen im Bereich „Von TronicCAD geprüft“ kommen direkt aus den Prüfungen. Behauptet ein Modell „ERC bestanden“, bleibt das sein Text, gekennzeichnet als „Antwort des Modells, ungeprüft“. Das ist die Grenze: Die Anwendung kann die Behauptung nicht verhindern, aber sie kann sie nicht wie ein Ergebnis aussehen lassen.

## Prompt-Injection

Bauteilwerte, Bibliotheken und Datenblätter können Text enthalten, der sich an das Modell richtet. Der Systemprompt kennzeichnet sie als Material. **Das ist kein sicherer Schutz.** Sicher ist nur die Begrenzung des Schadens: Ein Modell kann weder senden noch etwas ändern, seine Vorschläge laufen durch dieselbe Freigabe. Antworten werden als reiner Text dargestellt, nie als HTML.

## Funktionen aus dem Auftrag

| Funktion | Umsetzung |
|---|---|
| Schaltung aus Text | Aufgabe mit Symbolbibliothek, Ergebnis als Änderungen |
| Schaltplan analysieren | Regelanalyse ohne Modell, Modell als Aufgabe |
| Bauteile empfehlen, Verbesserungen, Fehler erläutern, Pinbelegung erklären | Aufgabe an das Modell |
| Datenblätter auswerten | **Nur eingefügter Text.** PDF wird nicht gelesen |
| PCB-Layout, Fertigungsprobleme | Einfache Regeln ohne Modell, Modell sieht nur zusammengefasste Daten (Regeln, Zahlen, Lage der Bauteile), keine Leiterbahngeometrie |
| Dokumentation | Aufgabe an das Modell, Antwort als Datei speicherbar |

Die Qualität dieser Antworten hängt vom Modell ab und ist **nicht** bewertet.

## Regelanalyse

Vorwiderstand und Strom von LEDs, Abblockkondensatoren, Basiswiderstand, Rückkopplung am Operationsverstärker, Spannungsteiler, RC-Grenzfrequenz, Umriss, offene Verbindungen, knappe Fertigungswerte, Befestigungslöcher. Es sind Hinweise, keine Prüfergebnisse: Annahmen stehen im Text (zum Beispiel 2 V Flussspannung), und Fehlalarme sind möglich.

## Claude-Anbindung

Rust hat kein offizielles SDK, daher direktes HTTP auf `/v1/messages` mit `anthropic-version: 2026-10-21`, Streaming und voreingestelltem Modell `claude-opus-5-5`. `stop_reason` `refusal` und `max_tokens` werden angezeigt. Für aktuelle Modelle direkt bei Anthropic ist `fallbacks: "default"` mit dem Beta-Header `server-side-fallback-2026-07-01` eingeschaltet: Lehnt ein Sicherheitsfilter ab, kann der Anbieter die Anfrage auf einem anderen Modell beantworten, was auf dieselben Kosten und Daten geht.

## Nachweise

- `crates/tronic-ai/tests/behaviour.rs`: echter HTTP-Server auf diesem Rechner, Protokolle, Fehler, Weiterleitung, Abbruch, Regeln, Vorschläge. Eine aus Änderungen gebaute Schaltung rechnet der Simulator nach.
- `apps/desktop/e2e/ai.mjs`: echte Anwendung gegen einen aufzeichnenden Server. Gezeigte gleich gesendete Daten, Schlüssel im Windows-Speicher und in keiner Datei, keine Verbindung ohne Zustimmung auch unter Umgehung der Oberfläche.
- `crates/tronic-ai/examples/local_check.rs`: Prüfprogramm gegen einen echten lokalen Server.

- `apps/desktop/e2e/ai-real.mjs`: die echte Anwendung gegen einen echten Server. Nicht Teil der Qualitätsprüfung, weil Antworten von Lauf zu Lauf abweichen. Prüft das Verhalten der Anwendung und schreibt die Antworten zum Lesen in einen Bericht.

**Erprobt** ist nur ein sehr kleines Modell über Ollama. **Nicht geprüft:** ein stärkeres Modell, ein Cloud-Anbieter und ein von einem echten Modell erzeugter gültiger Vorschlag, siehe reports/KNOWN_ISSUES.md.
