# Support

Stand 09.11.2026. TronicCAD ist freie Software, ein Anspruch auf Support besteht nicht. Für die Beta gilt die Adresse tronic3d@tronic3d.de. Dieses Dokument legt fest, wie Support ablaufen soll.

## Was vorhanden ist

- **Handbuch** in vier Sprachen, in der Anwendung unter „Hilfe“, auch ohne Verbindung.
- **Fehlerinformationen:** Unter „Hilfe > Support“ lässt sich ein Bericht kopieren: Version und System. Er enthält nichts über Projekte oder das Konto. Das Programm sendet ihn nicht selbst.
- **Kontaktadresse:** Wird beim Bauen mit `TRONICCAD_SUPPORT_CONTACT` eingetragen. Ohne sie sagt das Programm, dass es noch keinen Support gibt.
- **Bekannte Grenzen:** [reports/KNOWN_ISSUES.md](reports/KNOWN_ISSUES.md).
- **Sicherheitsmeldungen:** Prozess in [SECURITY.md](SECURITY.md), getrennt vom allgemeinen Support.

## Was vor einer Veröffentlichung zu entscheiden ist

| Frage | Warum |
|---|---|
| Kontaktadresse und Kanal | Ohne sie gibt es keinen Support |
| Wer antwortet, in welchen Sprachen | Das Produkt ist viersprachig, der Support muss es nicht sein. Das ist aber vorher zu sagen |
| Ablauf für Sicherheitsmeldungen und Pflichtmeldungen | [docs/CRA_ASSESSMENT.md](docs/CRA_ASSESSMENT.md) |

## Ablauf für eine Anfrage

1. Version und System aus dem Fehlerbericht aufnehmen.
2. Gegen bekannte Grenzen und das Handbuch prüfen.
3. Ist es ein Fehler: Nachstellen, als Regressionstest festhalten, beheben, in `reports/KNOWN_ISSUES.md` eintragen, solange er besteht.
4. Betrifft es die Sicherheit: Weiter nach [SECURITY.md](SECURITY.md).
5. Projektdateien werden nur angenommen, wenn der Kunde sie ausdrücklich schickt. Sie gelten als vertraulich und werden nach dem Fall gelöscht.
