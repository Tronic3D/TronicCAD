# Veröffentlichung und Schlüsselverwaltung

Stand 05.11.2026. **Bisher gab es keine Veröffentlichung.** Dieser Ablauf ist einzeln erprobt, aber nie als Ganzes gegen einen echten Server gelaufen. Die Sätze mit „erprobt“ nennen, was tatsächlich lief.

## Schlüssel

| Schlüssel | Zweck | Liegt | Öffentlicher Teil |
|---|---|---|---|
| Release-Schlüssel | Signiert Update-Manifeste | Offline, nie auf einem Server | `TRONICCAD_UPDATE_PUBLIC_KEYS` beim Bauen |
| Nächster Release-Schlüssel | Vorbereiteter Wechsel | Getrennt vom ersten | Ebenfalls beim Bauen eingetragen |
| Windows-Codesignatur | Phase 10 | Noch keiner | – |

**Regeln:**

- Private Schlüssel gehören nie ins Repository. `tronic-release` verweigert Schlüssel innerhalb eines Git-Repositories, `scripts/secrets_check.mjs` sucht in der Qualitätsprüfung nach Schlüsseln.
- Der Release-Schlüssel berührt keinen mit dem Internet verbundenen Rechner. Das ist Vorgabe, keine Technik: **Das Werkzeug liest einen Schlüssel aus einer Datei und unterstützt noch keinen Hardware-Schlüssel.** Bis dahin liegt die Datei auf einem verschlüsselten Datenträger, getrennt von einer ebenfalls verschlüsselten Sicherung an einem zweiten Ort.
- Wer Zugriff hat, ist namentlich festzuhalten. Das ist vor dem ersten Release zu entscheiden.

**Geht der Schlüssel verloren:** Mit dem vorbereiteten nächsten Schlüssel weiter signieren. Sind beide weg, können bestehende Installationen keine Updates mehr annehmen. Kunden müssen dann von Hand neu installieren.

**Wird der Schlüssel bekannt:** Das ist der schwerste Fall, denn ein Angreifer kann dann Schadprogramme signieren.

1. Sofort mit dem nächsten Schlüssel ein Sicherheitsupdate signieren, das den bekannten Schlüssel nicht mehr enthält.
2. Weil beide Schlüssel gelten, kann der Angreifer bis dahin ebenfalls signieren. Kunden müssen über alle Wege gewarnt werden.
3. Meldepflichten prüfen (CRA Artikel 14).
4. Neuen „nächsten“ Schlüssel erzeugen.

Eine Sperrliste für Schlüssel gibt es nicht.

**Geplanter Wechsel:** Jede Fassung enthält zwei Schlüssel. Eine Fassung, die beide kennt, wird veröffentlicht. Sobald sie ausreichend verbreitet ist, wird mit dem zweiten signiert, und die nächste Fassung trägt einen neuen zweiten Schlüssel. Fassungen, die den neuen Schlüssel nicht kennen, bekommen keine Updates mehr.

## Ablauf

1. **Stand festlegen:** Version in `Cargo.toml` und `tauri.conf.json` setzen, auf einem sauberen Stand mit Git-Tag.
2. **Prüfen:** `node scripts/quality.mjs` muss grün sein, mit allen Testwerkzeugen. Die CI muss auf allen drei Systemen grün sein. **Sie lief noch nie.**
3. **Bauen:** Auf einem sauberen Rechner, mit `--locked` und den Umgebungsvariablen für Schlüssel und Adressen. Die Werkzeugversion legt `rust-toolchain.toml` fest.

   ```bash
   TRONICCAD_UPDATE_URL=https://... TRONICCAD_UPDATE_PUBLIC_KEYS=<aktuell>,<nächster> node scripts/release_build.mjs
   ```

   Pakete entstehen zusätzlich mit `TRONICCAD_RELEASE_BUNDLES=nsis` oder `deb`. **AppImage nicht ausliefern,** solange die LGPL-Pflichten für das enthaltene WebKitGTK nicht geklärt sind. Danach signieren: `signtool` unter Windows, Paketsignatur unter Linux, Notarisierung unter macOS. **Dafür fehlen noch die Zertifikate, und keiner dieser Schritte ist erprobt.** Wird die Programmdatei signiert, ist das vor dem Hash im Manifest zu tun.

4. **Kontrollieren:** `node scripts/check_release_binary.mjs <programm>` stellt sicher, dass keine Testschalter und alle Schlüssel enthalten sind. Erprobt.
5. **Stückliste:** `node scripts/licenses.mjs --check` erzeugt die Stückliste, auch als `sbom.cdx.json`. Ungeklärte Lizenzen blockieren die Veröffentlichung.
6. **Signieren,** auf dem Rechner mit dem Schlüssel. Erprobt im End-to-End-Test.

   ```bash
   tronic-release sign --key <datei außerhalb des repos> --channel stable --version 1.2.0 --kind feature --previous <letztes manifest> --valid-days 90 --notes notes.json --artifact windows-x86_64=troniccad.exe=https://.../troniccad.exe --out stable.json
   ```

   Das Änderungsprotokoll muss alle vier Sprachen enthalten. `--previous` ist Pflicht, damit die Nummer nicht versehentlich neu beginnt.
7. **Veröffentlichen:** Erst die Programmdatei, dann das Manifest. Der Server muss https sprechen und ohne Weiterleitung antworten.
8. **Beobachten:** Auf einem Testrechner installieren. Fehlerberichte beobachten.

**Manifest erneuern:** Manifeste laufen ab, damit niemand Updates zurückhalten kann. Spätestens vor Ablauf ist also neu zu signieren, auch ohne neue Version, sonst melden alle Installationen `update.expired`. Das ist eine dauerhafte Betriebsaufgabe.

**Zurückziehen:** Eine fehlerhafte Fassung lässt sich nicht zurückrollen, weil ältere Versionen abgelehnt werden. Es braucht eine neue, höhere Version.

## Beta

`node scripts/build_beta.mjs --label "Beta 1" --support <kontakt>` baut das Paket auf einem sauberen Stand nach `dist-beta/`. Der Workflow „Beta package“ baut von Hand Windows und Debian, sobald er auf dem Standardzweig liegt. Veröffentlicht wird dabei nichts. `node apps/desktop/e2e/beta.mjs dist-beta/beta-1` prüft das Windows-Paket wie bei einer Testperson.

## Reproduzierbarkeit

Festgelegt sind Abhängigkeiten über `Cargo.lock` und `package-lock.json` sowie die Rust-Version in `rust-toolchain.toml`. `scripts/release_build.mjs` hält Pfade und Linkzeiten aus dem Programm heraus. Das Ergebnis der Messung steht in [reports/PERFORMANCE.md](../reports/PERFORMANCE.md). **Geprüft ist das nur auf einem Rechner.** Andere Rechner, Node- und Visual-Studio-Versionen sowie macOS und Linux sind nicht geprüft, und die Versionen von Node und Visual Studio sind nicht festgelegt.
