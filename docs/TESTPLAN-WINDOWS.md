# GhulSuite: Testplan Windows (Version 0.5.0-dev)

**Stand:** 15.11.2026 · erste Windows-Version (neu: Abschnitt 13, Aufgaben im Hintergrund und Mail-Archiv; Abschnitt 11, Assistent und Bereiche), mit neuem Design. Oberfläche und Kern sind dieselben wie unter Android; dieser Plan prüft,
was unter Windows anders ist (Installation, portabler Betrieb, Security Key über Windows, Sperre, Zwischenablage,
Bildschirmschutz) und dass Android und Windows dieselbe Suite gemeinsam nutzen können. Den Android-Plan gibt es
getrennt: [`TESTPLAN.md`](TESTPLAN.md).

Jeder Test hat eine Nummer, die mit **W** beginnt (z. B. **WE3**), damit sie sich nicht mit Android-Tests vermischt.

---

## 0. Vorbereitung

| Nr. | Schritt |
|---|---|
| WV1 | Windows 10 (ab 22H2) oder Windows 11, 64 Bit. |
| WV2 | Download-Seite auf dem Testserver (Ordner `ghulsuite/download/`) öffnen oder das Artefakt „ghulsuite-windows-dev“ im Actions-Tab herunterladen. |
| WV3 | Security Key mit PIN (z. B. YubiKey), ein USB-Stick, das Android-Handy mit GhulSuite (für die Abschnitte 7 und 8). |
| WV4 | Wichtig: Entwicklerversion, nicht signiert. **Keine echten Passwörter speichern.** |

## 1. Installation und portabler Betrieb

| Nr. | Schritt | Erwartet |
|---|---|---|
| WI1 | „Installer (MSI)“ herunterladen und starten. | SmartScreen warnt („Unbekannter Herausgeber“) → „Weitere Informationen“ → „Trotzdem ausführen“. Installation ohne Administratorrechte, Ordner wählbar. |
| WI2 | Startmenü → GhulSuite. | Fenster „GhulSuite“ mit lila Schild-Symbol, deutsche Oberfläche, Willkommensseite. |
| WI3 | Prüfsumme: PowerShell `Get-FileHash ghulsuite-dev.msi` und mit `ghulsuite-dev.msi.sha256` vergleichen. | Stimmt überein. |
| WI4 | „Portable Version (ZIP)“ auf den USB-Stick entpacken, dort `GhulSuite\GhulSuite.exe` starten. | Startet ohne Installation. Optionen → Über GhulSuite zeigt „0.5.0-dev (portabel)“. |
| WI5 | Portabel eine Suite einrichten (Abschnitt 2), dann den Stick ansehen. | Neben dem Programm entsteht `GhulSuite-Daten` mit der Suite. In `%APPDATA%\GhulSuite` entsteht **nichts**. |
| WI6 | Stick an einem anderen Windows-PC einstecken und starten. | Dieselbe Suite, Entsperren klappt. |
| WI7 | Installierte Version: Optionen → Apps → GhulSuite deinstallieren. | Programm weg; die Daten in `%APPDATA%\GhulSuite` bleiben (bewusst). |

## 2. Einrichtung und Entsperren

| Nr. | Schritt | Erwartet |
|---|---|---|
| WE1 | „Neu einrichten“ → „Security Key (empfohlen)“ → Passwort → Tresore → „Einrichten“. | Statt eines PIN-Felds der Hinweis, dass Windows nach PIN und Berührung fragt. |
| WE2 | Key einstecken, weiter. | Windows-Sicherheitsfenster: PIN eingeben, Key berühren, **zweimal** (Registrieren und Schlüssel ableiten). Danach Wiederherstellungscode. |
| WE3 | „Notfallblatt drucken / als PDF speichern“ → Drucker „Microsoft Print to PDF“. | PDF mit Code, QR-Code und Anleitung. Danach das PDF vom PC löschen. |
| WE4 | Code bestätigen (Gruppe 3). | Hauptansicht GhulPass. |
| WE5 | Strg+L drücken. | Sofort gesperrt (Entsperren-Seite). |
| WE6 | Passwort eingeben → „Entsperren“. | Windows-Fenster für PIN und Berührung; danach offen. |
| WE7 | Entsperren, im Windows-Fenster „Abbrechen“. | Verständliche Meldung, App bleibt gesperrt und bedienbar. |
| WE8 | Mit einem **anderen**, nicht registrierten Key entsperren. | Windows bzw. GhulSuite meldet, dass dieser Key nicht passt. |
| WE9 | Variante Schlüsseldatei: neue Suite (portabel oder anderer Ordner) mit „Schlüsseldatei“ einrichten, speichern (Windows-Speichern-Dialog), sperren, mit Passwort und Datei entsperren. | Klappt. |
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

## 5. Tresore, Speicherort, Import und Export

| Nr. | Schritt | Erwartet |
|---|---|---|
| WT1 | Einrichtung mit „Ordner wählen“ (z. B. ein Ordner in OneDrive oder auf dem Stick). | Suite liegt dort; Speicherort wird als Pfad angezeigt. |
| WT2 | Import: Proton-Pass- oder Bitwarden-Export wählen (Windows-Dateidialog). | Wie unter Android (Anzahl, Ziel, Import). |
| WT3 | Export eines Tresors (verschlüsselt) speichern; unverschlüsselt exportieren. | Dateien entstehen am gewählten Ort; unverschlüsselt nur mit Master-Passwort. |

## 6. Android und Windows gemeinsam

| Nr. | Schritt | Erwartet |
|---|---|---|
| WX1 | Den Suite-Ordner vom Handy auf den PC bringen (Synchronisation über eigenen Server, Abschnitt 7, oder den Ordner kopieren) und unter Windows „Vorhandene Suite öffnen“. | Entsperren mit **demselben** Security Key klappt unter Windows (Windows-Fenster). |
| WX2 | Unter Windows einen zweiten Key hinzufügen (Optionen → Keys & Schlüsseldateien → Security Key hinzufügen), synchronisieren, am Handy sperren und mit diesem Key entsperren. | Klappt, Keys sind zwischen Android und Windows austauschbar. |
| WX3 | Auf beiden Geräten Einträge ändern, synchronisieren. | Beide zeigen alle Änderungen. |

## 7. Server, Sync, Drive und Teilen

| Nr. | Schritt | Erwartet |
|---|---|---|
| WS1 | Server hinzufügen: SFTP (Schlüssel oder Passwort) oder FTPS; danach in der Serverliste „Modul installieren“. | Wie unter Android (Android-Plan 12 und 17): nur SFTP und FTPS zur Wahl, das Modul wird über dieselbe Anmeldung installiert und geprüft. |
| WS2 | GhulDrive: große Datei (> 100 MB) hochladen, herunterladen. | Streifen unter der Kopfleiste mit Balken und „Abbrechen“, auch wenn du in einen anderen Bereich wechselst; Ergebnis identisch. |
| WS3 | Datei → Teilen → „Mit Programm öffnen“. | Öffnet im passenden Programm (PDF-Betrachter usw.). Mehrere Dateien: der Ordner öffnet sich. |
| WS4 | Danach GhulSuite sperren. | Die entschlüsselten Kopien im Temp-Ordner (`%TEMP%\GhulSuite-Weitergabe`) sind gelöscht. |
| WS5 | Freigabelink erstellen, im Browser öffnen; ohne Modul auf dem Server von GhulDrive erneut versuchen. | Wie Android-Plan 18: Ablauf und Abrufe wählbar, keine Serverauswahl. Ohne Modul führt „Freigabelink“ zur Modul-Installation. |
| WS6 | Freigabelink zurückziehen, synchronisieren, Freigabelinks erneut öffnen. | Bleibt weg (auch auf dem Handy nach dessen Sync). |

## 8. GhulMail mit Verschlüsselung

| Nr. | Schritt | Erwartet |
|---|---|---|
| WM1 | Mailkonto hinzufügen (wie Android-Plan 14). | Signatur mit deinem Namen vorbelegt; IMAP nur SSL/TLS. Ein Schalter für Benachrichtigungen fehlt unter Windows (gewollt). |
| WM2 | Mail mit Anhang schreiben (Windows-Dateidialog), senden. | Kommt an; „Wird signiert …“ bzw. „… verschlüsselt …“ unter den Empfängern. |
| WM3 | Zwischen Windows und Android hin und her mailen (zwei Konten oder dasselbe Konto). | Ab der zweiten Mail verschlüsselt; beide Seiten zeigen Schloss und „Wirklich von …“. |
| WM4 | Mit Thunderbird/Proton wie Android-Plan P5 bis P13. | Gleiches Ergebnis. |
| WM5 | Anhang einer verschlüsselten Mail speichern. | Inhalt identisch. |

## 9. Allgemein

| Nr. | Schritt | Erwartet |
|---|---|---|
| WO1 | Fenster vergrößern, verkleinern (bis ganz klein), maximieren. | Bleibt bedienbar; kleiner als etwa 380 × 560 geht nicht. |
| WO2 | Optionen → Ansicht → Dunkel. | Alles gut lesbar. |
| WO3 | Windows auf Englisch umstellen (oder Benutzer mit englischer Sprache). | GhulSuite auf Englisch. |
| WO4 | Fenster schließen (X) und neu starten. | Beim Schließen wird gesperrt; nach dem Start Entsperren nötig. |
| WO6 | Zwischen den Bereichen wechseln (z. B. Drive-Unterordner → Mail → Drive). | Bereiche bleiben, wie sie waren (kein Neuladen). |
| WO7 | GhulMail offen lassen, Mail an das Konto schicken. | Erscheint nach spätestens etwa einer Minute von selbst. |
| WO5 | Bei einem Absturz: `%APPDATA%\GhulSuite\absturz.txt` (portabel: `GhulSuite-Daten\absturz.txt`). | Datei mit Fehlerbericht, bitte mitschicken. |

---

## 10. Neue Funktionen aus der Vorschau

Wie im Android-Testplan Abschnitt 22 (N2 bis N13, N16 bis N24). Nicht unter Windows: Dokument scannen (N14) und Foto-Sicherung (N15).
Zusätzlich:

| Nr. | Schritt | Erwartet |
|---|---|---|
| W-N1 | Optionen → Sicherung → automatische Sicherung in einen Ordner, Uhr eine Woche vorstellen (oder Datum in der Einstellung zurücksetzen), entsperren. | Neue Datei `GhulSuite-Backup-JJJJ-MM-TT-HHMM.zip` im Ordner. |
| W-N3 | App starten. | Fenster im Querformat (ca. 1280 × 820): Seitenleiste links mit GhulPass, GhulAuth, GhulDrive, GhulMail, GhulCal, Optionen und „Sperren (Strg+L)“ unten. |
| W-N4 | GhulPass: Eintrag anklicken, „Zurück“, einen anderen anklicken. | Der Eintrag öffnet sich in der Spalte an der Stelle der Liste (links bleibt die Seitenleiste); „Zurück“ zeigt die Liste wieder. |
| W-N5 | GhulMail: Mail anklicken; „Schreiben“. | Mail bzw. Entwurf in der Spalte an der Stelle des Posteingangs; „Zurück“ führt zum Posteingang. |
| W-N6 | Optionen: Bereich anklicken; im Editor etwas tippen und in der Seitenleiste einen anderen Bereich wählen. | Die Einstellungsseite öffnet sich in der Spalte; beim Bereichswechsel mit ungespeicherter Eingabe kommt „Änderungen verwerfen?“. |
| W-N7 | Einen Termin öffnen, dann Esc drücken; erneut öffnen, Alt+Pfeil links; erneut öffnen, die Zurück-Taste der Maus. Dann einen Eintrag lang drücken (Auswahl) und Esc. In der Liste ohne offene Seite Esc. | Jedes Mal einen Schritt zurück zur Liste; die Auswahl wird zuerst beendet; ohne offene Seite passiert nichts (das Fenster bleibt offen). Ein offenes Menü, Blatt oder ein Dialog schließt zuerst (auch mit der Maustaste), erst dann die Seite. Esc gedrückt halten geht nur einen Schritt zurück. In einem Editor mit ungespeicherter Eingabe kommt erst „Änderungen verwerfen?“. Im Einrichtungsassistenten ab Schritt 4 tut Esc nichts. |
| W-N7 | GhulDrive und GhulAuth in Kachelansicht. | Mehrere Kacheln nebeneinander über die ganze Breite. |
| W-N8 | „+“ in GhulPass, Kontextzeile antippen, Datei-Menü in GhulDrive. | Auswahl erscheint als Dialog in der Fenstermitte (nicht von unten). |
| W-N9 | Fenster schmal ziehen (unter ca. 840 Pixel). | Wechselt auf das Handy-Layout mit Leiste unten; wieder breit ziehen stellt die Seitenleiste wieder her. |
| W-N2 | Optionen → Über → „Fehlerbericht“. | Datei `absturz.txt` neben der App bzw. Text in der Zwischenablage, ohne Passwörter. |

## 11. Assistent in acht Schritten und „Ein Bereich, eine Datei“ (#24)

Die Abläufe sind dieselben wie unter Android (Testplan Android, Abschnitte 1 und 23). Unter Windows zusätzlich:

| Nr. | Schritt | Erwartet |
|---|---|---|
| W11.1 | „Neu einrichten“ mit Security Key: Schritt 2 → Schritt 3 „Key registrieren“. | Der Windows-Dialog fragt PIN und Berührung ab; danach Schritt 4 · Synchronisation. |
| W11.2 | Schritt 7 · Entsperren. | „Zusätzlich Fingerabdruck“ ist ausgegraut (unter Windows nicht verfügbar); App-PIN geht. |
| W11.3 | Schritt 4 „Eigener Server“ → FTPS → „Ordner wählen“. | Zertifikats-Fingerabdruck bestätigen, dann Ordnerliste im Dialog in der Fenstermitte. |
| W11.4 | Optionen → Bereiche verwalten → GhulDrive einrichten → „Eigener Server“ → FTPS. | Nur SFTP und FTPS zur Wahl; nach „Server speichern“ erscheint GhulDrive in der Leiste. Das Server-Modul für Freigabelinks kommt über die Serverliste. |
| W11.5 | Ordner der Suite im Explorer ansehen. | Je gewähltem Bereich eine Datei (`.ghlpss`, `.ghltotp`, `.ghldrv`, `.ghlmail`, `.ghlcal`) und `suite.ghlsuite`. |

## 12. Runde 6

| Nr. | Schritt | Erwartet |
|---|---|---|
| WR1 | Einrichtung bis Schritt 7 (Schnell-Entsperren). | Keine App-PIN zur Auswahl, Hinweis auf Windows Hello später (U8). |
| WR2 | Termin mit Erinnerung in 10 Minuten, dann in Windows die Zeitzone ändern und GhulSuite offen lassen. | Spätestens nach 5 Minuten wird neu geplant; die Erinnerung kommt zur richtigen Uhrzeit (B22). |
| WR3 | Optionen → GhulCal. | Hinweis: Erinnerungen kommen nur, solange GhulSuite läuft (B22). |

## 13. Aufgaben im Hintergrund und Mail-Archiv

Wie im Android-Testplan Abschnitte 33 (BG1, BG2, BG6, BG8) und 34 (MA1 bis MA14). Unter Windows anders:

| Nr. | Schritt | Erwartet |
|---|---|---|
| WJ1 | Großen Upload starten, ein anderes Fenster in den Vordergrund holen und länger warten als die Zeit für automatisches Sperren. | GhulSuite sperrt nicht mitten im Upload, sondern erst, wenn er fertig ist (sofern das Fenster dann noch im Hintergrund ist). Ohne Fortschritt für zwei Minuten (Netz weg) sperrt es trotzdem. Ein leiser Auftrag (automatisches Mail-Archiv) hält die Sperre nicht auf. |
| WJ2 | Wie WJ1, aber vor dem Ende zurück ins Fenster. | Bleibt entsperrt, der Upload läuft zu Ende. |
| WJ3 | Upload läuft, Strg+L. | Sofort gesperrt, Upload gestoppt; nach dem Entsperren „GhulSuite wurde gesperrt. Hochladen abgebrochen: …“. |
| WJ4 | Upload oder Mail-Archiv läuft, anderes Fenster vorne, Auftrag endet. | Windows-Benachrichtigung „Hochladen fertig“ bzw. „Mail-Archiv fertig · 40 von 40 fertig“, ohne Datei- oder Mailnamen; ein Klick holt GhulSuite nach vorne. Ist GhulSuite vorne, nur die Meldung in der App. |
| WJ5 | Windows sperren (Win+L), während ein Auftrag läuft (mit „Sperren, wenn Windows gesperrt wird“). | GhulSuite sperrt, sobald der Auftrag fertig ist; seine Meldung erscheint erst nach dem Entsperren. |

## Fehler melden

Bitte je Fehler: **Testnummer**, Windows-Version (Optionen → System → Info), installiert oder portabel, was du
gemacht hast, was passiert ist, ggf. Bildschirmfoto (vorher in GhulSuite Bildschirmfotos erlauben).
