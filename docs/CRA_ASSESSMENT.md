# Vorarbeit zur Anwendbarkeit des Cyber Resilience Act

Stand 05.11.2026. **Das ist keine fachliche oder rechtliche Prüfung.** Die Abnahme von Phase 9 verlangt eine solche, und sie kann nur extern erfolgen. Dieses Dokument soll sie abkürzen.

**Quellen:** Die Verordnung (EU) 2024/2847 wurde über die Konsolidierung von [Springlex](https://www.springlex.eu/en/packages/cra/cra-regulation/) gelesen, die selbst darauf hinweist, keine Rechtswirkung zu haben. Der Abruf bei EUR-Lex scheiterte. Die Zeitpunkte stammen aus Sekundärquellen, etwa [Jones Day](https://www.jonesday.com/en/insights/2026/07/eu-cyber-resilience-act-24hour-reporting-duties-start-september-11-2026) und [CMS](https://cms.law/en/aut/legal-updates/ready-respond-report-the-cyber-resilience-act-s-reporting-regime-is-here). Nicht gelesen wurden Artikel 32 und 71, die Durchführungsverordnung (EU) 2025/2392, die Änderungen durch (EU) 2025/327 und die Leitlinien der Kommission vom 27.07.2026.

**Seit [ADR 0004](adr/0004-open-source.md) ist TronicCAD freie Software.** Für freie Software, die nicht im Rahmen einer Geschäftstätigkeit bereitgestellt wird, gelten nach der Verordnung Ausnahmen. Die Pflichten unten treffen also vor allem, wer sie kommerziell anbietet. Das ist fachlich einzuschätzen.

## Einschätzung

| Frage | Einschätzung | Zu klären |
|---|---|---|
| Anwendbar? | Wahrscheinlich ja: Software, die in der EU kommerziell bereitgestellt wird (Artikel 3) | Rolle des Verkäufers neben Paddle als Händler des Vertrags |
| Klasse? | Annex III und IV nennen CAD und EDA nicht, also vermutlich die allgemeine Kategorie | Durchführungsverordnung 2025/2392 und Artikel 32 prüfen. Vor allem: Machen Lizenz- oder Anmeldefunktionen TronicCAD zu einem wichtigen Produkt? Laut Erwägungsgrund 45 zählt die Kernfunktion |
| Dienste? | Daten-Fernverarbeitung gehört dazu, wenn das Produkt ohne sie eine Funktion nicht erfüllen kann (Artikel 3 Absatz 2). Lizenzdienst und Clouddienst könnten darunterfallen | Beide Dienste einzeln einordnen |
| Free-Edition? | Eine kostenlose Edition kann trotzdem kommerziell sein | Einordnen |
| Zeitpunkte | Meldepflichten seit 11.09.2026, alles Weitere ab 11.12.2027. Die Sekundärquellen nennen auch Altprodukte. TronicCAD ist nicht auf dem Markt | Stichtag und Geltung beim ersten Verkauf bestätigen |

## Abgleich mit dem Stand

| Anforderung | Stelle | Stand |
|---|---|---|
| Sicherheitsupdates, wo anwendbar automatisch als Voreinstellung, mit Abschaltung, Benachrichtigung und Aufschub | Annex I Teil I (2)(c), Erwägungsgrund 56 | **Teilweise.** Suche und Benachrichtigung sind voreingestellt und abschaltbar, die Installation verlangt einen Klick. Erwägungsgrund 56 nimmt Produkte aus, bei denen Nutzer das nicht erwarten, etwa professionelle. Ob das für Personal und Free gilt, ist offen |
| Keine bekannten ausnutzbaren Schwachstellen beim Inverkehrbringen, sichere Voreinstellungen | Annex I Teil I | **Offen.** Automatische Prüfungen laufen, ein Penetrationstest fehlt |
| Stückliste, maschinenlesbar | Annex I Teil II (1) | **Teilweise.** `sbom.cdx.json` deckt die Desktop-Anwendung ab, nicht die Dienste |
| Schwachstellen ohne Verzögerung beheben, Sicherheitsupdates wo möglich getrennt von Funktionsupdates | Annex I Teil II (2) | **Teilweise.** Getrennte Kennzeichnung ja. Getrennte Auslieferung braucht Release-Zweige, die es nicht gibt |
| Informationen über behobene Schwachstellen | Annex I Teil II (4) | **Fehlt.** Kein öffentlicher Ort |
| Regeln zur koordinierten Offenlegung | Annex I Teil II (5) | **Teilweise.** Prozess in [SECURITY.md](../SECURITY.md), aber keine Meldeadresse |
| Sichere Verteilung von Updates | Annex I Teil II (7) | **Technisch umgesetzt,** [Spezifikation](specs/updates.md). Nicht extern geprüft |
| Sicherheitsupdates ohne Verzögerung und **kostenlos** | Annex I Teil II (8) | **Entscheidung nötig,** siehe unten |
| Unterstützungszeitraum mindestens 5 Jahre, oder die erwartete Nutzungsdauer wenn kürzer, Ende beim Kauf genannt | Artikel 13 (8), (19) | **Entscheidung nötig,** siehe unten |
| Jedes Sicherheitsupdate mindestens 10 Jahre oder den Rest des Zeitraums verfügbar | Artikel 13 (9) | **Fehlt.** Der Update-Server hält nur die neueste Version vor |
| Meldung ausgenutzter Schwachstellen und schwerer Vorfälle: 24 Stunden, 72 Stunden, Abschlussbericht | Artikel 14 | **Fehlt.** Kein Zuständiger, kein Ablauf, kein Zugang zur Meldeplattform |
| Technische Dokumentation, Konformitätsbewertung, CE-Kennzeichnung | Artikel 13, 28 und folgende | **Fehlt** |

Bußgelder laut Artikel 64: bis 15 Mio. Euro oder 2,5 % des weltweiten Jahresumsatzes bei Verstößen gegen Annex I und Artikel 13 und 14. Für Kleinst- und Kleinunternehmen gibt es eine Ausnahme bei der 24-Stunden-Frühwarnung.

## Was das für Geschäftsentscheidungen heißt

1. **Unterstützungszeitraum und Einmalkauf.** [ADR 0003](adr/0003-commercial-decisions.md) sieht vor, dass Updates innerhalb der Hauptversion enthalten sind und die nächste Hauptversion kostet. Falls der CRA gilt, müssen **Sicherheitsupdates für jede verkaufte Hauptversion über mindestens fünf Jahre kostenlos** bereitstehen. Mehrere Hauptversionen wären also parallel zu pflegen. Das sind dauerhafte Kosten und sollte vor der Preisgestaltung geklärt sein.
2. **Zuständigkeit.** Für Meldungen innerhalb von 24 Stunden braucht es jemanden, der erreichbar ist.
3. **Update-Server.** Er muss alte Sicherheitsupdates lange vorhalten und Manifeste regelmäßig neu signieren.

Das ist zusammen mit Risiko R13 zu bewerten.
