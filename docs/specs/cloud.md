# Spezifikation: Cloud, Synchronisierung und Sicherung

Stand 04.11.2026, Phase 6. Code: `crates/tronic-storage`, `services/cloud`, `apps/desktop/src-tauri/src/cloud.rs`, `apps/desktop/src/cloud`.

Lokales Arbeiten braucht nichts davon. Ohne eingerichteten Speicher baut die Anwendung keine Verbindung auf.

## Speicher

`StorageProvider` kennt nur Ablegen, Lesen und Auflisten von Objekten. Löschen gibt es in dieser Schnittstelle absichtlich nicht.

| Anbieter | Für | Geprüft gegen |
|---|---|---|
| Ordner | Externe Festplatte, Netzlaufwerk, NAS | Lokales Dateisystem. Echte Netzlaufwerke nicht |
| WebDAV | Nextcloud, NAS und andere | rclone 1.75 als Server. Nextcloud und andere nicht |
| S3-kompatibel | Objektspeicher, Signatur V4 | rclone 1.75 als Server. Amazon S3 und andere nicht |
| TronicCAD-Cloud | Eigener Dienst | Eigener Dienst mit PostgreSQL 18 |

Klartext-http ist nur zu diesem Rechner erlaubt, Weiterleitungen werden nicht verfolgt. Zertifikate prüft der Zertifikatspeicher des Betriebssystems. Eigene Zertifizierungsstellen funktionieren also, wenn sie dort eingetragen sind.

## Synchronisierung

Jeder gespeicherte Stand ist ein unveränderlicher Eintrag, wie bei Git:

- `projects/<id>/blobs/<sha256>`: die Projektdatei
- `projects/<id>/commits/<sha256>.json`: Vorgänger, Blob, Gerät, Zeit

Der aktuelle Stand sind die Einträge, auf denen kein anderer aufbaut. Sperren oder bedingte Schreibvorgänge braucht das nicht: Schreiben zwei Geräte gleichzeitig, entstehen zwei Einträge ohne Nachfolger, und die nächste Synchronisierung meldet den Konflikt.

| Lokal | Speicher | Ergebnis |
|---|---|---|
| Unverändert | Unverändert | Aktuell |
| Geändert | Unverändert | Neue Version |
| Unverändert | Neuer | Laden, vorheriger Stand bleibt als `.bak` |
| Geändert | Neuer | **Konflikt, nichts wird geändert** |
| Beliebig | Zwei Stände | **Konflikt, nichts wird geändert** |
| Gleicher Inhalt | Gleicher Inhalt | Aktuell, auch nach einem Abbruch |

Weitere Regeln:

- Übertragen wird nur die gespeicherte Datei. Bei ungespeicherten Änderungen läuft die automatische Synchronisierung nicht, die manuelle verlangt erst das Speichern.
- Ein Blob wird nach dem Hochladen zurückgelesen und geprüft, bevor ein Eintrag ihn nennt.
- Geladene Daten werden gegen ihre Prüfsumme und als Projektdatei geprüft, bevor sie die lokale Datei ersetzen. Das Ersetzen ist atomar.
- Ein Eintrag, dessen Inhalt nicht zu seinem Namen passt, zählt nicht. Abbrüche hinterlassen also nur ungenutzte Objekte.
- Der Sync-Zustand wird erst gesichert, wenn die Datei am Platz ist. Ein Abbruch dazwischen erkennt der nächste Lauf.

**Konflikt:** Der Benutzer wählt „Beide als Dateien behalten“, „Mit diesem Stand weiter“ oder „Mit dem anderen Stand weiter“. Der nicht gewählte Stand bleibt in den Versionen oder als Kopie neben dem Projekt. Zusammenführen auf Inhaltsebene gibt es nicht.

**Versionen:** Auflisten, mit dem gespeicherten Projekt vergleichen (Bauteile, Werte, Netze, Zahlen der Leiterplatte) und wiederherstellen. Wiederherstellen ergibt eine neue Version, die Geschichte wächst nur.

**Automatik:** je Verbindung einschaltbar. Läuft nach dem Speichern und jede Minute. Ist der Speicher nicht erreichbar, steht das in der Statusleiste und es gibt einen neuen Versuch.

## Sicherung

Eine Verbindung als „Sicherung“ schreibt jeden neuen Stand in eine Richtung, lädt nie etwas zurück und kennt keine Konflikte. Wiederherstellen geht über die Versionen. Es gibt keine Aufbewahrungsregeln: **Alte Versionen werden nie gelöscht, der Speicherbedarf wächst also.**

## Zustimmung und Zugangsdaten

Eine Verbindung zu einem Speicher außerhalb dieses Rechners verlangt eine ausdrückliche Zustimmung, im Backend geprüft. Danach gehen alle gespeicherten Stände dorthin. Die Daten liegen dort **unverschlüsselt** beim Anbieter, geschützt nur durch dessen Zugriffskontrolle und TLS auf dem Weg. Eine Ende-zu-Ende-Verschlüsselung gibt es nicht.

Passwörter, Schlüssel und Anmeldetoken liegen im Schlüsselspeicher des Betriebssystems. Das Passwort der TronicCAD-Cloud wird nur zum Anmelden verwendet und nicht aufbewahrt.

## TronicCAD-Cloud

`services/cloud`: Rust mit axum, PostgreSQL und einem Objektspeicher (Ordner oder S3). Instanzen haben keinen eigenen Zustand, Ereignisse laufen über PostgreSQL NOTIFY.

| Pfad | Zweck |
|---|---|
| `POST /v1/auth/register`, `login`, `logout` | Konto und Anmeldung |
| `GET`, `DELETE /v1/account` | Konto, Kontingent, Löschen mit Passwort |
| `GET /v1/repos` | Eigene und freigegebene Projekte |
| `POST`, `DELETE /v1/repos/{id}/shares` | Freigabe zum Lesen oder Schreiben, nur Eigentümer |
| `PUT`, `GET /v1/objects/{key}` | Objekte. Nur mit passender Prüfsumme im Namen, daher unveränderlich |
| `GET /v1/list/{prefix}` | Auflisten |
| `GET /v1/events` | WebSocket: Hinweise auf neue Versionen |
| `GET /healthz` | Betriebsprüfung |

Konfiguration über die Umgebung: `DATABASE_URL`, `TRONIC_CLOUD_BIND` (Voreinstellung `127.0.0.1:8787`), `TRONIC_STORE_DIR` oder `TRONIC_S3_ENDPOINT`, `_BUCKET`, `_REGION`, `_ACCESS_KEY`, `_SECRET_KEY`, außerdem `TRONIC_QUOTA_BYTES` und `TRONIC_ALLOW_REGISTRATION`.

**Der Dienst spricht selbst kein TLS und muss hinter einem Proxy laufen, der es beendet.** Die Datenbankverbindung ist unverschlüsselt und gehört in ein privates Netz.

**Es gibt keinen laufenden Dienst.** TronicCAD betreibt derzeit keine Cloud, das Programm läuft nur lokal zum Testen. Betriebsmodell und Kosten sind offen (Risiko R13).

Die Desktop-App nutzt die WebSocket-Ereignisse noch nicht, sie fragt jede Minute nach.

## Abnahme und Nachweise

| Kriterium | Stand | Nachweis |
|---|---|---|
| Übersteht Abbruch und Offlinebetrieb | Erfüllt | `crates/tronic-storage/tests/faults.rs`, `apps/desktop/e2e/cloud.mjs` |
| Nie stiller Datenverlust bei gleichzeitigen Änderungen | Erfüllt für die getesteten Fälle | Zufallstest: 40 Durchläufe, drei Geräte, je 200 Schritte, 20 % fehlerhafte Anfragen in vier Arten |
| S3 und WebDAV gegen echte Testserver | Erfüllt, nur gegen rclone | `tests/real_servers.rs` |
| Lastnachweis für horizontale Skalierung | **Nicht erfüllt** | `examples/load.rs`, siehe [PERFORMANCE](../../reports/PERFORMANCE.md) |
| Sicherheitsprüfung des Backends | **Nur Eigenprüfung** | [SECURITY.md](../../SECURITY.md), `services/cloud/tests/api.rs` |
