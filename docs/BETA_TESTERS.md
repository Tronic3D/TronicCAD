# TronicCAD Beta: Anleitung für Testerinnen und Tester

*English version below.*

Danke fürs Testen. Bitte lesen Sie das vor der Installation.

## Wichtig

- **Eine Beta enthält Fehler. Verlassen Sie sich nicht auf sie.** Pinbelegungen der mitgelieferten Bauteile sind nicht gegen Datenblätter geprüft, und kein Fertiger hat Daten dieser Software je geprüft. **Bestellen Sie keine Platinen, ohne alles selbst zu prüfen.**
- Sichern Sie Ihre Projekte. Ein Projekt, das Sie mit einer Beta gespeichert haben, lässt sich vielleicht in einer späteren Fassung nicht öffnen.
- TronicCAD ist freie Software, alle Funktionen sind ohne Aktivierung nutzbar.
- Es gibt keine automatischen Updates. Eine neue Beta installieren Sie von Hand über die alte.
- Das Programm sendet keine Nutzungsdaten. Cloud-KI und Speicheranbieter nutzt es nur, wenn Sie sie selbst einrichten. Für Team und Cloud gibt es in dieser Beta keinen Server von uns.

## Installieren unter Windows

Das Programm ist **nicht digital signiert**. Windows zeigt daher „Der Computer wurde durch Windows geschützt“. Das ist bei unsignierten Programmen normal, sagt aber auch: Windows kann die Herkunft nicht bestätigen.

1. Prüfen Sie, dass Sie die Datei von der Stelle haben, die Ihnen genannt wurde.
2. Vergleichen Sie die Prüfsumme in `SHA256SUMS-windows.txt`:

   ```
   Get-FileHash .\TronicCAD-*-setup.exe
   ```

3. Starten Sie die Datei, wählen Sie „Weitere Informationen“ und dann „Trotzdem ausführen“. Es sind keine Administratorrechte nötig.

## Installieren unter Linux

Debian und Ubuntu, geprüft nur auf Ubuntu 26.04 in WSL:

```
sha256sum -c SHA256SUMS-linux.txt
sudo apt install ./TronicCAD-*-linux-amd64.deb
```

Für macOS gibt es keine Beta.

## Was wir wissen möchten

Was hat nicht funktioniert, was war unverständlich, und was fehlt Ihnen? Bitte mit den Angaben aus **Hilfe > Support > Kopieren**, und mit den Schritten, die zum Fehler geführt haben. Schicken Sie Projektdateien nur, wenn Sie das möchten, und nichts Vertrauliches.

## Deinstallieren

Unter Windows über „Installierte Apps“, unter Linux mit `sudo apt remove tronic-cad`. Ihre Projekte und Einstellungen bleiben dabei erhalten.

---

# TronicCAD beta: guide for testers

Thank you for testing. Please read this before installing.

## Important

- **A beta contains errors. Do not rely on it.** Pin assignments of the included parts have not been checked against datasheets, and no manufacturer has ever checked data from this software. **Do not order boards without checking everything yourself.**
- Back up your projects. A project saved with a beta may not open in a later build.
- TronicCAD is free software, all features work without activation.
- There are no automatic updates. Install a new beta by hand over the old one.
- The program sends no usage data. It uses cloud AI and storage providers only if you set them up yourself. For teams and cloud, there is no server of ours in this beta.

## Installing on Windows

The program is **not digitally signed**. Windows therefore shows “Windows protected your PC”. That is normal for unsigned programs, and it also means that Windows cannot confirm where it came from.

1. Make sure you got the file from the place you were told.
2. Compare the checksum in `SHA256SUMS-windows.txt`:

   ```
   Get-FileHash .\TronicCAD-*-setup.exe
   ```

3. Start the file, choose “More info” and then “Run anyway”. No administrator rights are needed.

## Installing on Linux

Debian and Ubuntu, checked only on Ubuntu 26.04 in WSL:

```
sha256sum -c SHA256SUMS-linux.txt
sudo apt install ./TronicCAD-*-linux-amd64.deb
```

There is no beta for macOS.

## What we would like to know

What did not work, what was hard to understand, and what is missing? Please include the details from **Help > Support > Copy** and the steps that led to the error. Send project files only if you want to, and nothing confidential.

## Uninstalling

On Windows under “Installed apps”, on Linux with `sudo apt remove tronic-cad`. Your projects and settings stay.
