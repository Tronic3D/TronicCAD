# IP- und Lizenz-Compliance

> Dieses Dokument ist eine technische Vorprüfung und **keine Rechtsberatung**. Vor einer kommerziellen Veröffentlichung ist eine Prüfung durch fachkundige Juristen nötig. Automatische Prüfungen ersetzen sie nicht.

## Grundsätze

- Eigener Quellcode, eigene Architektur, eigene Oberfläche, eigenes Dateiformat, eigene Icons.
- Keine Übernahme proprietären Codes oder geschützter Designs. Fremde Dateiformate und Marken werden nur nach Prüfung und nur beschreibend verwendet.
- Funktionale Orientierung an etablierten CAD-Systemen ist zulässig, soweit keine Rechte verletzt werden. Während der Entwicklung werden keine Oberflächen fremder Systeme nachgebaut.
- Das App-Icon und alle UI-Icons wurden für dieses Projekt neu gezeichnet (`assets/brand/icon.svg`, `apps/desktop/src/components/icons.tsx`). Schriften werden nicht mitgeliefert, es gelten die Systemschriften.

## Freigabetor

Ein kommerzielles Release ist blockiert, solange einer dieser Punkte offen ist:

1. `cargo deny check licenses sources bans` meldet einen Fehler.
2. `node scripts/licenses.mjs --check` meldet eine Abweichung von der Positivliste.
3. Der Abschnitt „Ohne mitgelieferte Lizenzdatei“ in `THIRD_PARTY_NOTICES.md` ist nicht manuell abgearbeitet.
4. Eine Komponente aus der Tabelle unten steht nicht auf „freigegeben“.
5. Die juristische Prüfung fehlt.

Die ersten beiden Punkte erzwingt die CI, die übrigen sind manuell.

**Generierte Nachweise:** [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md), [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), [SOFTWARE_BILL_OF_MATERIALS.md](SOFTWARE_BILL_OF_MATERIALS.md). Sie enthalten Name, Version, Herkunft, Lizenz und Autoren aller ausgelieferten Pakete.

## Positivliste

Zulässig ohne Einzelprüfung: MIT, MIT-0, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, CC0-1.0, Unlicense, BSL-1.0, Unicode-3.0, CDLA-Permissive-2.0.

Mit Bedingung: **MPL-2.0**. Dateibasiertes Copyleft, Änderungen an diesen Dateien sind offenzulegen. Aktuell über Tauri-Abhängigkeiten enthalten und unverändert verwendet.

Einzelprüfung nötig: LGPL, EPL und alles andere. **Ausgeschlossen für Auslieferung:** GPL, AGPL und Lizenzen mit Nutzungsbeschränkung, sofern kein gesonderter Rechtsrat etwas anderes ergibt.

## Prüfung der geplanten Hauptkomponenten

Lizenzangaben für Rust- und npm-Pakete stammen aus den Paketmetadaten. Patentbedingungen wurden nicht einzeln geprüft, dies bleibt Teil der juristischen Prüfung.

| Komponente | Herkunft | Lizenz | Kommerziell | Bedingungen | Status |
|---|---|---|---|---|---|
| Tauri 2 | tauri.app | MIT oder Apache-2.0 | Ja | Lizenztexte beilegen | Im Einsatz, Rechtsprüfung offen |
| React | react.dev | MIT | Ja | Hinweis beilegen | Im Einsatz, Rechtsprüfung offen |
| i18next, react-i18next | i18next.com | MIT | Ja | Hinweis beilegen | Im Einsatz, Rechtsprüfung offen |
| rusqlite | crates.io | MIT | Ja | Hinweis beilegen | Im Einsatz, Rechtsprüfung offen |
| SQLite | sqlite.org, gebündelt über libsqlite3-sys | Public Domain | Ja | Laut Herausgeber gemeinfrei; in manchen Rechtsordnungen wird dafür eine gekaufte Lizenz angeboten | Im Einsatz, Rechtsprüfung offen |
| WebView2 Runtime | Microsoft, Teil von Windows | Microsoft-Softwarelizenz | Zu prüfen | Bedingungen bei Mitlieferung des Installers | Nicht mitgeliefert, Prüfung für Installer in Phase 10 |
| WebKitGTK, GTK (Linux) | Distribution | LGPL | Dynamisch gebunden üblich | Nicht mitliefern oder LGPL-Pflichten erfüllen. AppImage bündelt sie, daher dort besonders prüfen | Phase 10 |
| i_overlay | github.com/iShape-Rust/iOverlay | MIT oder Apache-2.0 | Ja | Lizenztexte beilegen | Im Einsatz, Rechtsprüfung offen |
| gerbonara | PyPI | Nur Entwicklungswerkzeug, Lizenz in den Metadaten nicht angegeben | Wird nicht ausgeliefert | Vor dauerhafter CI-Nutzung die Lizenz im Repository prüfen | Nur zur Prüfung, nicht im Produkt |
| wgpu | gfx-rs | MIT oder Apache-2.0 (laut bisheriger Kenntnis) | Voraussichtlich | Bei Einführung bestätigen | Geplant |
| ngspice | ngspice.sourceforge.io | Basis BSD-3-Clause; Teile LGPL, GPL | Wird nicht ausgeliefert | Nur Gegenprobe auf Entwicklerrechnern, siehe unten | **Nicht im Produkt, nicht freigegeben** |
| Open CASCADE (STEP) | dev.opencascade.org | LGPL-2.1 mit Ausnahme | Voraussichtlich | Siehe unten | **Nicht freigegeben** |
| Lokale KI-Server und Modelle (llama.cpp, Ollama, LM Studio) | Hersteller | Je nach Server und Modell | **Wird nicht ausgeliefert** | Der Benutzer installiert sie selbst und ist für die Modelllizenz verantwortlich. Sollte je ein Modell mitgeliefert werden, ist es einzeln zu prüfen | Nicht im Produkt |
| Cloud-KI-Anbieter | Hersteller | Vertrag des Benutzers | Kein Code enthalten | Nur Netzzugriff mit eigenem Schlüssel. Bedingungen des Anbieters gelten für die gesendeten Daten | Vom Benutzer gewählt |
| PostgreSQL | postgresql.org | PostgreSQL License | Ja (laut bisheriger Kenntnis) | Nur vom Clouddienst genutzt, nicht mit der Desktop-App ausgeliefert. Bei einer Weitergabe Hinweis beilegen | Phase 6, Rechtsprüfung offen |
| Caddy | caddyserver.com | Apache-2.0 | Wird nicht ausgeliefert | Vom Betreiber beim Selbsthosting installiert, nur in der Anleitung und den Dateien genannt | Rechtsprüfung offen |
| Docker-Abbilder (Rust, Debian, PostgreSQL, Caddy) | Docker Hub | Je Abbild verschieden | Beim Bauen geladen, nicht von uns verteilt | Wer eigene Abbilder weitergibt, muss deren Lizenzen beachten | Rechtsprüfung offen |
| NSIS | nsis.sourceforge.io | zlib/libpng, Teile bzip2 und andere | Zu prüfen | Baut den Windows-Installer, und sein Stub steckt im ausgelieferten Installer. Beim Bauen über Tauri geladen, Prüfsumme kontrolliert | Im Einsatz, **Rechtsprüfung offen, Hinweise fehlen noch in THIRD_PARTY_NOTICES.md** |
| nsis-tauri-utils | github.com/tauri-apps | MIT oder Apache-2.0 | Ja | Steckt im Installer, beim Bauen geladen | Im Einsatz, Rechtsprüfung offen |
| linuxdeploy (AppImage) | github.com/linuxdeploy | MIT | Nur Werkzeug | Bündelt WebKitGTK unter LGPL ins AppImage | **AppImage nicht freigegeben,** siehe WebKitGTK |
| rclone | rclone.org | MIT | Wird nicht ausgeliefert | Nur als Testserver für WebDAV und S3 auf Entwicklerrechnern | Entwicklungswerkzeug |

### ngspice

Die Basis steht laut FAQ der Entwickler unter New BSD, einzelne Teile unter anderen Lizenzen. Debians Copyright-Datei führt LGPL-2 für numparam und den Parser sowie GPL-2+ für ADMS-Gerätecode auf. CIDER und XSPICE haben eigene Herkunft, die Angaben sind teils widersprüchlich.

**Stand Phase 4:** Das Produkt enthält ngspice nicht und bindet es nicht ein. Die Simulation ist eigener Code. Bei den Modellgleichungen (Shockley, Ebers-Moll, MOSFET Level 1) und numerischen Verfahren handelt es sich um veröffentlichte Lehrbuchinhalte, nicht um übernommenen Code. ngspice 47 wird nur auf dem Entwicklerrechner als unabhängige Gegenprobe aufgerufen (`scripts/verify_sim.py`). Das bedeutet keine Weitergabe. Die Anwendung gibt nur eine SPICE-Netzliste als Text aus.

**Falls ngspice später doch gebraucht wird:** Build-Konfiguration ohne GPL-Anteile festlegen, Dateiköpfe und `COPYING` der eingesetzten Version prüfen, ngspice als separates Programm statt fest gebunden betreiben, die einzelnen Gerätemodelle (BSIM und andere) prüfen, dann juristisch freigeben lassen.

### Open CASCADE

**Stand Phase 4:** Nicht eingebunden, deshalb liest die Anwendung kein STEP. Die Parser für STL, OBJ und WRL sind eigener Code. Importierte Modelle stammen vom Benutzer, ihre Rechte prüft der Benutzer. Die Oberfläche weist beim Import darauf hin.

LGPL-2.1 mit Ausnahme zu Header-Material und Pflicht zu einem deutlichen Hinweis. Ein offizieller Text zu statischem Linken wurde nicht gefunden, daher nur dynamisch oder als separater Prozess. Ob die Lizenz „nur 2.1“ oder „2.1 oder später“ meint, ist in Diskussionen umstritten. Der Text `OCCT_LGPL_EXCEPTION.txt` der eingesetzten Version ist zu prüfen. Alternative: kommerzielle Lizenz des Herstellers.

## Bauteilbibliotheken

- TronicCAD erhält eine eigene Bibliothek, jedes Symbol, jeder Footprint und jedes 3D-Modell mit Datensatz zu Herkunft, Autor, Lizenz und Prüfdatum.
- Die Footprints sind nach gängigen Gehäusemaßen neu erstellt, keine Daten aus fremden Bibliotheken ([docs/LIBRARY_PROVENANCE.md](docs/LIBRARY_PROVENANCE.md)).
- Fremde Bibliotheken werden nicht einfach importiert und mitgeliefert. Offene EDA-Bibliotheken stehen häufig unter Copyleft-artigen Lizenzen mit Ausnahmen für erzeugte Designs, Herstellermodelle oft unter Bedingungen, die die Weitergabe einschränken. Jede Quelle wird einzeln geprüft.
- Nutzereigene Importe bleiben in der Verantwortung des Nutzers, die Software zeigt die bekannte Herkunft an.

## Quellen der Vorprüfung

[ngspice bei Wikipedia](https://en.wikipedia.org/wiki/Ngspice) · [ngspice-FAQ, Debian](https://sources.debian.org/src/ngspice/45.2%2Bds-1/FAQ) · [ngspice-Copyright, Debian](https://tracker.debian.org/media/packages/n/ngspice/copyright-46ds-1) · [debian-legal zu CIDER](https://lists.debian.org/debian-legal/2016/10/msg00022.html) · [OCCT-Copyright, Debian](https://metadata.ftp-master.debian.org/changelogs/main/o/opencascade/testing_copyright) · [Open CASCADE Exception 1.0](https://licenses.nuget.org/OCCT-exception-1.0) · [KiCad-Diskussion zur OCCT-Version](https://lists.launchpad.net/kicad-developers/msg33575.html)
