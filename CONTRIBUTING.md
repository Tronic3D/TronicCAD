# Arbeitsweise

## Git

- `main` bleibt lauffähig und wird nicht direkt bearbeitet.
- Branches: `feature/<thema>`, `fix/<thema>`, `docs/<thema>`.
- Commits nach [Conventional Commits](https://www.conventionalcommits.org/): `feat(schematic): …`, `fix(project): …`, `docs: …`.
- Zusammenführen erst nach grünen Prüfungen. Signaturschlüssel, Zugangsdaten und `.env`-Dateien kommen nie ins Repository.

## Definition of Done für eine größere Funktion

1. Technische Spezifikation in `docs/specs/`
2. Implementierung ohne Scheinfunktionen
3. Tests inklusive Fehlerfällen
4. Dokumentation und Übersetzungen in allen vier Sprachen
5. Lizenzprüfung neuer Abhängigkeiten nach [IP_COMPLIANCE.md](IP_COMPLIANCE.md), inklusive Pflichtangaben
6. Abnahmekriterien in [ROADMAP.md](ROADMAP.md) abgehakt und belegt
7. `node scripts/quality.mjs` läuft durch

## Regeln

- Nicht Implementiertes wird als geplant markiert, in der UI und in der Doku.
- Keine erfundenen Testergebnisse. Berichte entstehen aus echten Läufen.
- Kein fester Anzeigetext im Code, jeder Text kommt aus den Sprachdateien.
- Neue Abhängigkeiten nur mit Begründung.

## Lizenz von Beiträgen

Beiträge stehen unter der [Apache-Lizenz 2.0](LICENSE), wie in deren Abschnitt 5 beschrieben. Fremder Code darf nur mit einer verträglichen Lizenz und mit Eintrag in den Lizenzlisten hinein, siehe [IP_COMPLIANCE.md](IP_COMPLIANCE.md).

