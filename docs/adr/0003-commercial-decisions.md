# ADR 0003: Geschäftsentscheidungen für Lizenzen und Verkauf

**Überholt durch [ADR 0004](0004-open-source.md): TronicCAD ist freie Software. Dieses Dokument ist nur noch Geschichte.**

Stand 04.11.2026. Status: vom Auftraggeber entschieden und in Phase 8 technisch umgesetzt, siehe [docs/specs/license.md](../specs/license.md). Die Anbindung an Paddle ist nicht gegen Paddle geprüft, weil das Konto fehlt.

Rechtlich ungeprüft: Diese Festlegungen sind Produktentscheidungen, keine geprüften Vertragsbedingungen.

## Entscheidungen

| Thema | Entscheidung |
|---|---|
| Zahlungsdienstleister | Paddle als Händler des Vertrags (Merchant of Record), angebunden über eine austauschbare Schnittstelle. Aktuelle Gebühren und Länderabdeckung sind vor dem Abschluss zu prüfen |
| Offline-Nutzung, Einmalkauf | Nach der Aktivierung unbegrenzt ohne Server, über eine signierte Lizenzdatei |
| Offline-Nutzung, Abonnement | 30 Tage ohne Verbindung zum Lizenzserver |
| Umfang des Einmalkaufs | Alle Updates und Sicherheitskorrekturen innerhalb der gekauften Hauptversion. Die nächste Hauptversion ist ein vergünstigtes Upgrade |
| Cloud beim Einmalkauf | Die TronicCAD-Cloud kostet extra. Eigener Speicher und Selbsthosting bleiben ohne Zusatzkosten |
| Nach Ablauf oder bei ungültiger Lizenz | Rückfall auf den Umfang von TronicCAD Free. Projekte lassen sich immer öffnen und exportieren, nichts wird gelöscht. Wer eine Lizenz mit Fertigungsdaten hatte, kann auch diese weiter erzeugen. Inhalte aus höheren Editionen bleiben sichtbar und eingeschränkt bearbeitbar |
| Preise | Vorerst klar gekennzeichnete Testpreise in der Sandbox. Die echten Preise werden vor der Veröffentlichung festgelegt |
| Geräte je Einzellizenz | 3 gleichzeitig aktivierte Geräte bei Personal und Professional, selbst abmeldbar, zentral einstellbar |
| Testzeitraum | 30 Tage TronicCAD Professional, mit Konto und ohne Zahlungsart, danach Rückfall auf Free |
| Verkäufer | Noch offen. Bis dahin neutrale Platzhalter und keine fertigen Verträge |

## Folgen

- **Kein Sperren durch Ablauf:** Eine fehlende Lizenz darf nie den Zugang zu den eigenen Projektdaten nehmen.
- **Widerruf:** Eine widerrufene Kauflizenz wirkt auf Geräten ohne Verbindung nicht. Das ist der bewusste Preis für die dauerhafte Offline-Nutzung.
- **Missbrauch:** Lizenzdateien lassen sich mit vertretbarem Aufwand nicht vollständig gegen Kopieren schützen. Die Prüfung soll ehrliche Kunden nicht behindern.
- **Signaturschlüssel:** Der private Schlüssel für Lizenzdateien liegt getrennt vom Quellcode.
- **Zahlungsdaten:** Sie bleiben bei Paddle. Eigene Systeme speichern nur Bezüge wie Kunden- und Abonnementkennungen.
- **Paddle-Konto:** Für die Sandbox braucht es ein Konto, das nur der Auftraggeber anlegen kann. Ohne Sandbox-Zugang bleibt dieses Abnahmekriterium offen. Für den echten Betrieb ist außerdem ein Verkäufer nötig.

## Offen vor einem Verkaufsstart

- Verkäufer, Rechtsform und Sitz
- Juristische Prüfung der Vertragsbedingungen, Abnahmekriterium von Phase 8
- Echte Preise und Upgrade-Rabatt
- Betrieb, Kosten und Hosting des Lizenzservers (Risiko R13)
- Eignung von Paddle für die Zielmärkte, einschließlich China, sowie Exportkontrolle (Risiko R12)
- Regelungen für Lizenzübertragung und Unternehmenslizenzen (Edition Business)
