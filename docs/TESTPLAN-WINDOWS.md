# GhulSuite: Testplan Windows (Version 0.5.0-dev)

**Stand:** 20.11.2026 · **neu: Umbau zu GhulDocs** ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)): eine reine
Offline-App mit GhulPass, GhulAuth, GhulDocs und GhulCal. Die Suite wandert als eine Datei (`.ghlzip`, mit allen
Dokumenten) zwischen Android und Windows, z. B. auf dem USB-Stick (Abschnitt 6); GhulDocs unter Windows in Abschnitt 7.
GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel sind entfallen (Abschnitt 8 bleibt nur als
Überschrift), davor schon Synchronisation, FTPS, Sicherung und „Speicherort wählen“. Oberfläche und Kern sind dieselben wie unter Android; dieser Plan prüft, was
unter Windows anders ist (Installation, portabler Betrieb, Security Key über Windows, Sperre, Zwischenablage,
Bildschirmschutz, Dateipfade) und dass Android und Windows dieselbe Suite-Datei gemeinsam nutzen können. Den Android-Plan gibt es
getrennt: [`TESTPLAN.md`](TESTPLAN.md).

Jeder Test hat eine Nummer, die mit **W** beginnt (z. B. **WE3**), damit sie sich nicht mit Android-Tests vermischt.

---

## 0. Vorbereitung

| Nr. | Schritt |
|---|---|
| WV1 | Windows 10 (ab 22H2) oder Windows 11, 64 Bit. |
| WV2 | Download-Seite auf dem Testserver (Ordner `ghulsuite/download/`) öffnen oder das Artefakt „ghulsuite-windows-dev“ im Actions-Tab herunterladen. |
| WV3 | Security Key mit PIN (z. B. YubiKey), ein USB-Stick, das Android-Handy mit GhulSuite (für Abschnitt 6), ein paar Testdokumente (PDF, Fotos, Text, eine große Datei). |
| WV4 | Wichtig: Entwicklerversion, nicht signiert. **Keine echten Passwörter speichern.** |

## 1. Installation und portabler Betrieb

| Nr. | Schritt | Erwartet |
|---|---|---|
| WI1 | „Installer (MSI)“ herunterladen und starten. | SmartScreen warnt („Unbekannter Herausgeber“) → „Weitere Informationen“ → „Trotzdem ausführen“. Installation ohne Administratorrechte, Ordner wählbar. |
| WI2 | Startmenü → GhulSuite. | Fenster „GhulSuite“ mit lila Schild-Symbol, deutsche Oberfläche, Willkommensseite. |
| WI3 | Prüfsumme: PowerShell `Get-FileHash ghulsuite-dev.msi` und mit `ghulsuite-dev.msi.sha256` vergleichen. | Stimmt überein. |
| WI4 | „Portable Version (ZIP)“ auf den USB-Stick entpacken, dort `GhulSuite\GhulSuite.exe` starten. | Startet ohne Installation. Optionen → Über GhulSuite zeigt „0.5.0-dev (portabel)“. |
| WI5 | Portabel eine Suite einrichten (Abschnitt 2), dann den Stick ansehen. | Neben dem Programm entsteht `GhulSuite-Daten` mit der Arbeitskopie der Suite. In `%APPDATA%\GhulSuite` entsteht **nichts**. |
| WI6 | Stick an einem anderen Windows-PC einstecken und starten. | Dieselbe Suite (die Arbeitskopie reist im Ordner `GhulSuite-Daten` mit), Entsperren klappt. |
| WI7 | Installierte Version: Optionen → Apps → GhulSuite deinstallieren. | Programm weg; die Daten in `%APPDATA%\GhulSuite` bleiben (bewusst). |

## 2. Einrichtung und Entsperren

| Nr. | Schritt | Erwartet |
|---|---|---|
| WE1 | „Neu einrichten“ → Passwort → „Security Key (empfohlen)“ → Schritt 3 · Anlegen. | Statt eines PIN-Felds der Hinweis, dass Windows nach PIN und Berührung fragt. |
| WE2 | Key einstecken, „Anlegen“, die übrigen Schritte durchgehen. | Windows-Sicherheitsfenster: PIN eingeben, Key berühren, **zweimal** (Registrieren und Schlüssel ableiten). Am Ende der Wiederherstellungscode. |
| WE3 | „Notfallblatt drucken / als PDF speichern“ → Drucker „Microsoft Print to PDF“. | PDF mit Code, QR-Code und Anleitung. Danach das PDF vom PC löschen. |
| WE4 | Code bestätigen (Gruppe 3). | Hauptansicht GhulPass. |
| WE5 | Strg+L drücken. | Sofort gesperrt (Entsperren-Seite). |
| WE6 | Passwort eingeben → „Entsperren“. | Windows-Fenster für PIN und Berührung; danach offen. |
| WE7 | Entsperren, im Windows-Fenster „Abbrechen“. | Verständliche Meldung, App bleibt gesperrt und bedienbar. |
| WE8 | Mit einem **anderen**, nicht registrierten Key entsperren. | Windows bzw. GhulSuite meldet, dass dieser Key nicht passt. |
| WE9 | Über eine vorhandene Suite neu einrichten (wie E22 bis E24 im Android-Testplan): „Vorhandene sichern und ersetzen“ mit dem Windows-Speichern-Dialog, danach „Vorhandene löschen und ersetzen“. | Wie E23 und E24; nach dem Löschen enthält `%APPDATA%\GhulSuite\suite` (portabel `GhulSuite-Daten\suite`) nur die Dateien der neuen Suite, keine `docs-…`-Ordner der alten. |
| WE9 | Variante Schlüsseldatei: neue Suite (z. B. portabel auf einem zweiten Stick) mit „Schlüsseldatei“ einrichten, speichern (Windows-Speichern-Dialog), sperren, mit Passwort und Datei entsperren. | Klappt. |
| WE10 | „Mit Wiederherstellungscode öffnen“. | Mit Code und neuem Passwort wieder offen. |

## 3. Sperre und Privatsphäre

| Nr. | Schritt | Erwartet |
|---|---|---|
| WL1 | Optionen → Sperren: „Sofort“. Zu einem anderen Programm wechseln, 5 Sekunden warten, zurück. | Gesperrt. |
| WL2 | „Datei wählen“ (z. B. Import) öffnen und wieder schließen. | **Nicht** gesperrt (eigene Dialoge zählen nicht als Hintergrund). |
| WL3 | Windows mit Win+L sperren und wieder anmelden. | GhulSuite ist gesperrt („Sperren, wenn Windows gesperrt wird“). |
| WL4 | 15 Minuten nichts in GhulSuite tun (Fenster im Vordergrund). | Gesperrt. |
| WL5 | Ausschneiden-Tool (Win+Umschalt+S) oder Bildschirmfreigabe (Teams/Zoom) auf das GhulSuite-Fenster. | Das Fenster erscheint schwarz bzw. leer. |
| WL6 | Optionen → Privatsphäre → Bildschirmfotos erlauben, erneut aufnehmen. | Jetzt sichtbar. Danach wieder ausschalten. |
| WL7 | Passwort eines Eintrags kopieren, in Notepad einfügen, dann Win+V. | Einfügen klappt. Im Zwischenablage-Verlauf (Win+V) taucht es **nicht** auf. |
| WL8 | Nach der eingestellten Zeit (Standard 45 s) erneut einfügen. | Zwischenablage ist leer. |

## 4. GhulPass und GhulAuth

| Nr. | Schritt | Erwartet |
|---|---|---|
| WP1 | Einträge anlegen, bearbeiten, suchen, löschen; Passwort-Generator. | Wie unter Android; Ziffern und Sonderzeichen farbig. |
| WP2 | GhulAuth → „+“ → QR-Code eines Dienstes im Browser anzeigen → „Bildschirm scannen“. | Konto wird erkannt und übernommen. |
| WP3 | QR-Code als Bild kopieren (Rechtsklick → Bild kopieren) → „Bild aus der Zwischenablage“. | Wird erkannt. |
| WP4 | Ohne QR-Code auf dem Bildschirm scannen. | „Kein QR-Code gefunden.“ |
| WP5 | Codes mit dem Handy vergleichen (gleiches Konto, gleiche Zeit). | Identisch. |

## 5. Tresore, Import und Export

| Nr. | Schritt | Erwartet |
|---|---|---|
| WT1 | Optionen → Suite-Datei → „Speichern unter …“ (Windows-Speichern-Dialog), z. B. auf den Stick. | Datei `GhulSuite.ghlzip` entsteht; die Seite zeigt den Pfad („Suite-Datei: …“). „Speichern“ schreibt danach ohne Dialog an denselben Pfad. |
| WT2 | Import: Proton-Pass- oder Bitwarden-Export wählen (Windows-Dateidialog). | Wie unter Android (Anzahl, Ziel, Import). |
| WT3 | Export eines Tresors (verschlüsselt) speichern; unverschlüsselt exportieren. | Dateien entstehen am gewählten Ort; unverschlüsselt nur mit Master-Passwort. |
| WT4 | „Einzelnen Tresor öffnen“ mit einer `.ghlpss` → „Nur diesen Tresor öffnen“; etwas ändern; „Tresor schließen“. | Wie Android-Plan T6 und T7: Änderungen gehen direkt in die Datei am selben Pfad zurück; die eigene Suite bleibt gesperrt. |
| WT5 | GhulPass: Einträge auswählen → „Als Tresor weitergeben“ mit neuer Schlüsseldatei. | Wie Android-Plan T13: Schlüsseldatei und Tresordatei über den Windows-Speichern-Dialog, Code wird einmal gezeigt. |

## 6. Android und Windows gemeinsam (Suite-Datei)

Die Suite wandert als **eine Datei** (`.ghlzip`). Abgeglichen wird nichts: Aktuell ist der Stand, der zuletzt
gespeichert wurde. Alle Rückfragen sind dieselben wie im Android-Plan, Abschnitt 35.

| Nr. | Schritt | Erwartet |
|---|---|---|
| WX1 | Am Handy die Suite-Datei auf den USB-Stick speichern. Unter Windows (ohne Suite) „Suite-Datei öffnen“ → die Datei auf dem Stick. | Entsperren mit **demselben** Security Key klappt unter Windows (Windows-Fenster). Fortschritt beim Öffnen; alle Einträge, Dokumente (auch die gescannten PDFs vom Handy) und Einstellungen sind da. |
| WX2 | Unter Windows einen zweiten Key hinzufügen (Optionen → Keys & Schlüsseldateien → Security Key hinzufügen), speichern, am Handy die Datei öffnen, sperren und mit diesem Key entsperren. | Klappt, Keys sind zwischen Android und Windows austauschbar. |
| WX3 | Unter Windows etwas ändern, nicht speichern. | Hinweis „Nicht gespeicherte Änderungen. Tippen zum Speichern.“ auf den Bereichsseiten. Nach „Speichern“ verschwindet er. |
| WX4 | Am Handy etwas ändern und speichern, unter Windows dieselbe Datei öffnen (Windows hat selbst nichts Ungespeichertes). | Öffnet ohne Rückfrage; die Änderung vom Handy ist da. |
| WX5 | Unter Windows etwas ändern (nicht speichern), dann die Datei vom Handy öffnen. | Rückfrage nennt die nicht gespeicherten Änderungen, die verloren gehen; „Abbrechen“ lässt alles, wie es ist. |
| WX6 | Handy und Windows öffnen denselben Stand; beide ändern; das Handy speichert; Windows tippt „Speichern“ auf dieselbe Datei. | „Die Datei wurde inzwischen anderswo gespeichert. Überschreiben … oder unter neuem Namen speichern?“ |
| WX7 | Stick während „Speichern“ abziehen bzw. vorher abziehen. | „Die Suite-Datei ist gerade nicht erreichbar …“ oder „… konnte nicht geschrieben werden. Die bisherige Datei ist unverändert.“; die Änderungen bleiben als nicht gespeichert markiert. |
| WX8 | Unter Windows das Passwort ändern. | Dialog „Suite-Datei jetzt speichern“ mit dem Hinweis, alte Kopien zu löschen. Eine ältere Kopie der Datei öffnet sich unter Windows nicht mehr (älterer Schutz); auf einem Gerät, das den neuen Stand nie gesehen hat, nur mit dem alten Passwort und dem alten zweiten Faktor. |

## 7. GhulDocs unter Windows

Dieselben Abläufe wie im Android-Plan, Abschnitt 39 (GD1, GD2, GD4 bis GD6, GD8 bis GD15, mit GD13a). Scannen gibt es unter
Windows nicht. Unter Windows anders:

| Nr. | Schritt | Erwartet |
|---|---|---|
| WD1 | GhulDocs → „Hinzufügen“. | Nur „Dateien hinzufügen“ (Windows-Dateidialog, mehrere auf einmal), kein „Dokument scannen“. |
| WD2 | Eine PDF mit mehreren Seiten und ein großes Foto ansehen. | Alle Seiten in der App (PDFBox, nur im Arbeitsspeicher); im Benutzerordner und in `%TEMP%` entsteht dabei keine Datei. Portabel: auch auf dem Stick nichts außer dem Schriften-Zwischenspeicher von PDFBox im Ordner `GhulSuite-Daten`. |
| WD3 | „Mit App öffnen“ für eine PDF, danach Strg+L. | Öffnet im Standardprogramm; nach dem Sperren ist der Weitergabe-Ordner leer, auch wenn das Programm die Datei erst später freigibt (B10). |
| WD4 | „Exportieren“ in einen Ordner auf dem Stick; im Streifen „Abbrechen“ bei einer großen Datei. | Rückfrage vorher; vollständige Datei bzw. nach dem Abbruch keine halbe Datei. |
| WD5 | Suite-Datei mit etwa 500 MB Dokumenten auf SSD und auf einen USB-Stick speichern. | Fortschritt; auf der SSD wenige Sekunden, auf dem Stick deutlich länger; daneben kurzzeitig eine `.ghltmp`, danach nur die neue Datei. |
| WD6 | Breites Fenster: Dokument anklicken, „Zurück“. | Die Ansicht steht in der Spalte an der Stelle der Liste, die Seitenleiste bleibt. |
| WD7 | Einen Scan vom Handy (PDF mit JPEG-Seiten) ansehen; vorher und nachher `%TEMP%` nach `imageio*.tmp` durchsuchen (auch mit einem Programm, das gelöschte Dateien zeigt). | Keine Datei von ImageIO; die Seiten entstehen nur im Arbeitsspeicher. |
| WD8 | GhulSuite starten und sofort, noch während es startet, eine `.ghlzip` doppelklicken; dann ein zweites Mal GhulSuite starten. | Es bleibt bei einem Fenster: die Datei wird im ersten geöffnet, der zweite Start holt es nur nach vorn. Mit „Mit App öffnen“ geöffnete Dokumente bleiben dabei offen (der zweite Start löscht keine Kopien). |
| WD9 | Nach der Installation (MSI) eine `.ghldocs` (GhulDocs-Paket) im Explorer doppelklicken, einmal mit gesperrter, einmal mit entsperrter Suite; ebenso während GhulSuite schon läuft. | Das Symbol von GhulSuite steht an der Datei („GhulDocs-Paket“). Gesperrt: Auswahl „Nur diesen Tresor öffnen“ / „Zu meiner Suite hinzufügen“; entsperrt: „In GhulDocs aufnehmen“ mit dem Namen der Datei als Ordner. Läuft GhulSuite schon, übernimmt das laufende Fenster die Datei. Der Speichern-Dialog beim Weitergeben schlägt `.ghldocs` vor. |

## 8. GhulMail mit Verschlüsselung (entfernt)

Entfällt: GhulMail wurde mit dem Umbau zu GhulDocs entfernt ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)).

## 9. Allgemein

| Nr. | Schritt | Erwartet |
|---|---|---|
| WO1 | Fenster vergrößern, verkleinern (bis ganz klein), maximieren. | Bleibt bedienbar; kleiner als etwa 380 × 560 geht nicht. |
| WO2 | Optionen → Ansicht → Dunkel. | Alles gut lesbar. |
| WO3 | Windows auf Englisch umstellen (oder Benutzer mit englischer Sprache). | GhulSuite auf Englisch. |
| WO4 | Fenster schließen (X) und neu starten. | Beim Schließen wird gesperrt; nach dem Start Entsperren nötig. |
| WO6 | Zwischen den Bereichen wechseln (z. B. GhulDocs-Unterordner → GhulPass → GhulDocs). | Bereiche bleiben, wie sie waren (kein Neuladen). |
| WO5 | Bei einem Absturz: `%APPDATA%\GhulSuite\absturz.txt` (portabel: `GhulSuite-Daten\absturz.txt`). | Datei mit Fehlerbericht, bitte mitschicken. |

---

## 10. Neue Funktionen aus der Vorschau

Wie im Android-Testplan Abschnitt 22 (N2 bis N9, N23, N24).
Zusätzlich:

| Nr. | Schritt | Erwartet |
|---|---|---|
| W-N3 | App starten. | Fenster im Querformat (ca. 1280 × 820): Seitenleiste links mit GhulPass, GhulAuth, GhulDocs, GhulCal, Optionen und „Sperren (Strg+L)“ unten. |
| W-N4 | GhulPass: Eintrag anklicken, „Zurück“, einen anderen anklicken. | Der Eintrag öffnet sich in der Spalte an der Stelle der Liste (links bleibt die Seitenleiste); „Zurück“ zeigt die Liste wieder. |
| W-N6 | Optionen: Bereich anklicken; im Editor etwas tippen und in der Seitenleiste einen anderen Bereich wählen. | Die Einstellungsseite öffnet sich in der Spalte; beim Bereichswechsel mit ungespeicherter Eingabe kommt „Änderungen verwerfen?“. |
| W-N7 | Einen Termin öffnen, dann Esc drücken; erneut öffnen, Alt+Pfeil links; erneut öffnen, die Zurück-Taste der Maus. Dann einen Eintrag lang drücken (Auswahl) und Esc. In der Liste ohne offene Seite Esc. | Jedes Mal einen Schritt zurück zur Liste; die Auswahl wird zuerst beendet; ohne offene Seite passiert nichts (das Fenster bleibt offen). Ein offenes Menü, Blatt oder ein Dialog schließt zuerst (auch mit der Maustaste), erst dann die Seite. Esc gedrückt halten geht nur einen Schritt zurück. In einem Editor mit ungespeicherter Eingabe kommt erst „Änderungen verwerfen?“. Im Einrichtungsassistenten ab Schritt 4 tut Esc nichts. |
| W-N7b | GhulAuth in Kachelansicht. | Mehrere Kacheln nebeneinander über die ganze Breite. |
| W-N8 | „+“ in GhulPass, Kontextzeile antippen, „Hinzufügen“ in GhulDocs. | Auswahl erscheint als Dialog in der Fenstermitte (nicht von unten). |
| W-N9 | Fenster schmal ziehen (unter ca. 840 Pixel). | Wechselt auf das Handy-Layout mit Leiste unten; wieder breit ziehen stellt die Seitenleiste wieder her. |
| W-N2 | Optionen → Über → „Fehlerbericht“. | Datei `absturz.txt` neben der App bzw. Text in der Zwischenablage, ohne Passwörter. |

## 11. Assistent in sieben Schritten und „Ein Bereich, eine Datei“ (#24)

Die Abläufe sind dieselben wie unter Android (Testplan Android, Abschnitte 1 und 23). Unter Windows zusätzlich:

| Nr. | Schritt | Erwartet |
|---|---|---|
| W11.1 | „Neu einrichten“ mit Security Key: Schritt 2 → Schritt 3 „Anlegen“. | Der Windows-Dialog fragt PIN und Berührung ab; danach Schritt 4 · Bereiche. |
| W11.2 | Schritt 6 · Entsperren. | „Zusätzlich Fingerabdruck“ ist ausgegraut (unter Windows nicht verfügbar); App-PIN geht. |
| W11.3 | Schritt 4: Bereiche. | Alle vier Bereiche wählbar; keiner fragt nach einem Server oder Konto. |
| W11.4 | Optionen → Bereiche verwalten → GhulDocs einrichten. | Name abgefragt, danach steht GhulDocs in der Leiste mit „0 B von 1 GB“. |
| W11.5 | Optionen → Suite-Datei → „Speichern unter …“, die `.ghlzip` mit 7-Zip oder dem Explorer öffnen. | Darin `export.ghlexp` (zuerst), je Tresor eine Datei (`.ghlpss`, `.ghltotp`, `.ghldoc`, `.ghlcal`, Namen nach der Kennung), `suite.ghlsuite` und je Dokument eine `docs-…/….ghlblob`; nichts lesbar, keine Dateinamen der Dokumente. |

## 12. Runde 6

| Nr. | Schritt | Erwartet |
|---|---|---|
| WR1 | Einrichtung bis Schritt 6 (Schnell-Entsperren). | Keine App-PIN zur Auswahl, Hinweis auf Windows Hello später (U8). |
| WR2 | Termin mit Erinnerung in 10 Minuten, dann in Windows die Zeitzone ändern und GhulSuite offen lassen. | Spätestens nach 5 Minuten wird neu geplant; die Erinnerung kommt zur richtigen Uhrzeit (B22). |
| WR3 | Optionen → GhulCal. | Hinweis: Erinnerungen kommen nur, solange GhulSuite läuft (B22). |

## 13. Aufgaben im Hintergrund

Wie im Android-Testplan Abschnitt 33 (BG1, BG2, BG6, BG8). Unter Windows anders:

| Nr. | Schritt | Erwartet |
|---|---|---|
| WJ1 | Eine große Datei zu GhulDocs hinzufügen, ein anderes Fenster in den Vordergrund holen und länger warten als die Zeit für automatisches Sperren. | GhulSuite sperrt nicht mitten im Hinzufügen, sondern erst, wenn es fertig ist (sofern das Fenster dann noch im Hintergrund ist). Ohne Fortschritt für zwei Minuten sperrt es trotzdem. |
| WJ2 | Wie WJ1, aber vor dem Ende zurück ins Fenster. | Bleibt entsperrt, das Hinzufügen läuft zu Ende. |
| WJ3 | Hinzufügen läuft, Strg+L. | Sofort gesperrt, Auftrag gestoppt, nichts Halbes bleibt; nach dem Entsperren „GhulSuite wurde gesperrt. … abgebrochen: …“. |
| WJ4 | Hinzufügen oder Speichern der Suite-Datei läuft, anderes Fenster vorne, Auftrag endet. | Windows-Benachrichtigung mit dem Ergebnis, ohne Dateinamen; ein Klick holt GhulSuite nach vorne. Ist GhulSuite vorne, nur die Meldung in der App. |
| WJ5 | Windows sperren (Win+L), während ein Auftrag läuft (mit „Sperren, wenn Windows gesperrt wird“). | GhulSuite sperrt, sobald der Auftrag fertig ist; seine Meldung erscheint erst nach dem Entsperren. |

## Fehler melden

Bitte je Fehler: **Testnummer**, Windows-Version (Optionen → System → Info), installiert oder portabel, was du
gemacht hast, was passiert ist, ggf. Bildschirmfoto (vorher in GhulSuite Bildschirmfotos erlauben).
