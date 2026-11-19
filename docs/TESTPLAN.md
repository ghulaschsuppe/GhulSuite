# GhulSuite: Testplan Android (Version 0.5.0-dev)

**Stand:** 17.11.2026 · **neu: Umbau ohne Synchronisation** ([`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)): Die Suite
wandert als eine Datei (`.ghlzip`), die du speicherst und öffnest (Abschnitt 35); einzelne Tresore öffnen, hinzufügen
und weitergeben (Abschnitt 6); Server nur per SFTP mit SSH-Schlüssel (Abschnitte 12 und 17); GhulDrive mit zwei
Geräten gleichzeitig und beschädigtem oder älterem Index (Abschnitt 36); Schlüsselwechsel nach einer Schutzänderung
(Abschnitt 37). Tests zu Synchronisation, FTPS, Sicherung/Wiederherstellung und „Von meinem Server holen“ sind
entfallen. Außerdem: Aufgaben im Hintergrund und Mail-Archiv in GhulDrive (Abschnitte 33 und 34), Freigabelinks nur
über das Server-Modul (Abschnitte 17 und 18), verschlüsselte Mails (Abschnitt 20).

**Neu in 0.5.0:** Die Einstellungen sind jetzt eine Übersicht mit Unterseiten. Pfade wie „Optionen → Tresore“
meinen: Tab **Optionen** → Zeile **Tresore**. Den Testplan für Windows gibt es getrennt:
[`TESTPLAN-WINDOWS.md`](TESTPLAN-WINDOWS.md).

Jeder Test hat eine Nummer (z. B. **E3**), damit du Fehler eindeutig melden kannst. „Erwartet“ beschreibt, was passieren
soll; weicht etwas ab, notiere die Nummer, was du gemacht hast und was stattdessen passiert ist.

---

## 0. Vorbereitung

| Nr. | Schritt |
|---|---|
| V1 | Alte GhulSuite-Version deinstallieren (das Dateiformat hat sich geändert, alte Testdaten lassen sich nicht öffnen). |
| V2 | APK `ghulsuite-dev.apk` installieren (Testserver, Ordner `ghulsuite/download/`, oder Artefakt „ghulsuite-dev“ im Actions-Tab auf GitHub). |
| V3 | Bereitlegen: Security Key mit PIN (USB-C), möglichst ein zweiter Key, Stift und Papier, ein **USB-Stick** (USB-C oder mit Adapter) für die Suite-Datei. |
| V4 | Für den Import Exportdateien bereitlegen, z. B. aus Proton Pass (ZIP oder CSV), Proton Authenticator, Bitwarden, KeePassXC, Aegis, 2FAS, und Google Authenticator auf einem zweiten Gerät. **Nur Testdaten** verwenden. |
| V5 | Wichtig: Diese Entwicklerversion ist mit einem öffentlichen Testschlüssel signiert. Keine echten Passwörter speichern. |
| V6 | Für die Abschnitte 12, 13, 36 und 37: einen Server mit **SSH/SFTP** (Adresse, Port, Benutzer, SSH-Passwort für das einmalige Eintragen des Schlüssels) und Zugriff auf `~/.ssh/authorized_keys`. Fingerabdruck des Servers bereithalten: auf dem Server `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`. Für die Abschnitte 35 bis 37 ein **zweites Gerät** (zweites Android-Gerät oder Windows-PC mit GhulSuite). Für KW5 bis KW7 möglichst einen Hoster, der SSH-Schlüssel nur in seinem Webpanel verwaltet. |
| V7 | Für Abschnitt 14 bis 15: ein **Test**-Mailkonto mit IMAP/SMTP (Server, Benutzer, Passwort bzw. App-Passwort) und eine zweite Adresse, von der du Mails schicken kannst. |
| V8 | Für Abschnitt 17 bis 18: ein **Webhosting mit SSH-Zugang** (SFTP) und PHP 8.1 oder neuer, dessen Website über https erreichbar ist (z. B. Ordner `public_html`). Reiner FTP-Webspace reicht nicht. Optional: ein Webhosting ohne PHP und eines mit `open_basedir`, für ST9 und ST10. |

---

## 1. Erster Start und Einrichtung

Der Assistent hat sieben Schritte; oben steht „Schritt n von 7“ mit Fortschrittsbalken. Die Suite entsteht beim
Verlassen von Schritt 3 im App-Speicher, danach geht „Zurück“ nur noch bis Schritt 4.

| Nr. | Schritt | Erwartet |
|---|---|---|
| E1 | App öffnen. | Startbildschirm mit „Neu einrichten“, „Suite-Datei öffnen“ (.ghlzip, z. B. von einem USB-Stick) und „Einzelnen Tresor öffnen“. Kein „Von meinem Server holen“, kein „Sicherung wiederherstellen“. |
| E2 | „Neu einrichten“. | Schritt 1 von 7 · Master-Passwort. |
| E3 | Passwort „abc“ eingeben, Weiter. Dann ein schwaches 10-Zeichen-Passwort. Dann zwei verschiedene Passwörter. | Jeweils klare Meldung: zu kurz / zu leicht zu erraten / stimmen nicht überein. |
| E4 | „Passwort vorschlagen“ antippen, Wiederholung eingeben, Weiter. | Langes Zufallspasswort, Stärkebalken voll; Schritt 2 · Zweiter Faktor. |
| E5 | „Security Key“ gewählt lassen, Name des Keys, Key-PIN eingeben (Android: „Key prüfen“ zeigt „Erkannt: …“). Weiter. | Schritt 3 · Anlegen, mit dem Hinweis, dass die Suite auf diesem Gerät liegt und du sie später als Datei (.ghlzip) unter Optionen → Suite-Datei speicherst. Keine Ordnerwahl. |
| E6 | „Anlegen“, Key **zweimal** berühren. | Schritt 4 · Bereiche; kein „Zurück“ zu den Schritten 1 bis 3 mehr. |
| E7 | Alle fünf Bereiche einschalten. Bei GhulDrive „Eigener Server“, ohne Angaben „Weiter“. | Namen für GhulPass, GhulAuth, GhulCal änderbar. Meldung „GhulDrive, noch offen: Anmeldung“ (bzw. der fehlende Schritt). Checkliste unter dem Formular zeigt Anmeldung, Verbindung testen, Ordner wählen, Ordner beschreibbar, Speichern. |
| E8 | SFTP-Server eintragen (siehe Abschnitt 12), „Ordner wählen“. | Fingerabdruck bestätigen, dann die Ordner des Servers; „ghulsuite, Neu anlegen (empfohlen)“, Unterordner öffnen, „Eine Ebene höher“, „Neuer Ordner …“. |
| E9 | Ordner wählen → „Verbindung prüfen“ → „Server speichern“ → Weiter. | Vor der Prüfung gibt es kein „Server speichern“. Nach der Prüfung „Verbindung klappt, der Ordner ist beschreibbar …“, dann nach dem Speichern „Verbunden · host:/ordner“, Haken in der Checkliste; Schritt 5 · Sperren. |
| E10 | Im Server-Formular die Verbindungsarten ansehen. | Es gibt nur **SFTP**, kein FTP/FTPS und kein PHP-Modul als Verbindungsart. |
| E11 | Sperrzeit wählen, Schalter für Bildschirm aus und Schütteln. Weiter. | Schritt 6 · Entsperren mit Hinweis, dass „Schnell entsperren“ Fingerabdruck und App-PIN meint. |
| E12 | „Zusätzlich Fingerabdruck“, Gültigkeit „Immer“. Weiter. | Hinweis: ohne Ablauf, aber nach Neustart, Sofortsperre oder Passwortwechsel wieder Passwort und Key. Fingerabdruck-Abfrage; Schritt 7. |
| E13 | Code ansehen, „Notfallblatt drucken“ → „Als PDF speichern“. | PDF mit Code, QR-Code und Anleitung. PDF danach vom Handy löschen. |
| E14 | „Ich habe den Code notiert“ → Weiter → falsche Gruppe 3 eingeben. | Meldung „Das ist nicht die dritte Gruppe“. Es gibt kein „Später“. |
| E15 | Richtige Gruppe 3 eingeben → Abschließen. | Hauptansicht mit allen fünf Bereichen und Optionen; GhulMail zeigt „Noch kein Konto“ mit dem vollständigen Kontoformular. Oben der Hinweis „Nicht gespeicherte Änderungen. Tippen zum Speichern.“ (die Suite ist noch in keiner Datei). |
| E16 | Zweite Einrichtung: in Schritt 4 bei GhulDrive „Später“. | GhulDrive zeigt danach „Noch nicht eingerichtet“ mit der Einrichtung direkt im Bereich. |
| E17 | GhulDrive „Eigener Server“ mit einem Webhosting, das nur FTP und keinen SSH-Zugang bietet. | „Verbindung testen“ scheitert mit einer verständlichen Meldung; nichts wird gespeichert. GhulDrive und Freigabelinks brauchen SSH-Zugang. |
| E18 | Einrichtung mit „Schlüsseldatei“ statt Key: Datei erstellen und z. B. auf den USB-Stick speichern. | Ohne gespeicherte Datei geht es nicht weiter. Danach wie E6 bis E15 (ohne Key-Berührung). |
| E19 | Nach E15: Optionen → Suite-Datei → „Speichern unter …“ → auf den USB-Stick. | Datei `GhulSuite.ghlzip` entsteht; „Gespeichert in …“; der Hinweis „Nicht gespeicherte Änderungen“ verschwindet. |
| E20 | Nur GhulAuth wählen und einrichten. | In der Leiste nur GhulAuth und Optionen; Optionen → Bereiche zeigt nur GhulAuth als eingerichtet. |
| E21 | Einrichtung in Schritt 6 abbrechen (App schließen), App neu öffnen. | Entsperren-Bildschirm; nach dem Entsperren verlangt GhulSuite einen neuen Wiederherstellungscode (ohne „später“), bestätigt per Gruppe. |

## 2. Entsperren

| Nr. | Schritt | Erwartet |
|---|---|---|
| U1 | App sperren (Schloss oben rechts). Passwort und Key-PIN eingeben, Key berühren. | Entsperrt, **eine** Berührung. |
| U2 | Falsches Passwort. | „Falsches Passwort“ (Key wurde trotzdem berührt). |
| U3 | Falsche Key-PIN. | Meldung mit verbleibenden Versuchen. **Nicht** bis zur Sperre ausreizen. |
| U4 | Ohne eingesteckten Key auf Entsperren tippen. | Meldung „Kein Key gefunden“ (o. ä.). |
| U5 | Suite mit Schlüsseldatei: Passwort + Datei wählen. Dann eine falsche Datei. | Entsperrt / „Diese Schlüsseldatei gehört nicht zu dieser Suite“. |
| U6 | Wenn Key **und** Schlüsseldatei eingetragen sind (siehe S2): Umschalter oben. | Man kann zwischen „Security Key“ und „Schlüsseldatei“ wählen, beides funktioniert. |
| U7 | „Mit Wiederherstellungscode öffnen“ → Code (Kleinbuchstaben, ohne Bindestriche) + neues Passwort. | Entsperrt; Hinweis, einen neuen Code zu erstellen. |
| U8 | Sperren, mit dem **alten** Passwort entsperren. Dann mit dem neuen. | Altes Passwort: falsch. Neues Passwort + Key: geht. Key und Code gelten weiter. |
| U9 | Optionen → Entsperren: Fingerabdruck aktivieren. Sperren, App öffnen. | Fingerabdruck-Abfrage erscheint direkt. |
| U10 | App-PIN (6 bis 12 Ziffern) aktivieren, sperren, mit PIN entsperren. 5× falsche PIN. | Danach ist der PIN-Zugang gelöscht, Passwort + Key nötig. |
| U11 | Handy neu starten, App öffnen. | Fingerabdruck/PIN gehen nicht, Passwort + Key nötig. |
| U12 | Gültigkeit auf 1 Stunde stellen, nach über 1 Stunde öffnen. | Passwort + Key nötig. |
| U13 | Handy **ohne** Bildschirmsperre: Optionen → Entsperren → App-PIN einschalten. | PIN lässt sich setzen und entsperrt die App (früher: „Keystore key generation failed“). |
| U14 | Key abziehen, App sperren, „Entsperren“ tippen und **erst danach** den Key einstecken (innerhalb einer Minute). | Unten erscheint „Steck jetzt deinen Security Key ein“; nach dem Einstecken geht es von selbst weiter (berühren, wenn er blinkt). Ohne Key nach einer Minute: „No security key found“. |
| U15 | Neue Suite mit Key einrichten (oder Key hinzufügen) und danach mit einem anderen FIDO-Programm prüfen, ob der Key das Credential ohne PIN preisgibt (optional, Fachleute). | Der Key verlangt die PIN (credProtect Stufe 3). |

## 3. Sperren

| Nr. | Schritt | Erwartet |
|---|---|---|
| P1 | Auto-Sperre „Sofort“, App in den Hintergrund. | Beim Zurückkehren gesperrt. |
| P2 | Auto-Sperre „1 Minute“, kurz weg und zurück; dann über 1 Minute weg. | Kurz: noch offen. Lange: gesperrt. |
| P3 | „Bei Bildschirm aus sperren“ an, Bildschirm aus und an. | Gesperrt. |
| P4 | Benachrichtigung „GhulSuite entsperrt“ → „Sperren“. | Gesperrt. |
| P5 | Schnelleinstellungs-Kachel „GhulSuite sperren“ hinzufügen und antippen. | Gesperrt, Fingerabdruck/PIN-Zugang gelöscht. |
| P6 | „Schütteln zum Sperren“ an, Handy eine halbe Sekunde deutlich hin und her schütteln, in der App, mit der App im Hintergrund und mit gesperrtem Startbildschirm-Wechsel. Jede Empfindlichkeit ausprobieren. | Gesperrt, kurzes Vibrieren, Fingerabdruck/PIN-Zugang gelöscht. Gehen, Treppensteigen, Handy auf den Tisch legen sperren **nicht**. |
| P7 | Bildschirmfoto in der App versuchen; App-Übersicht öffnen. | Bildschirmfoto blockiert, Vorschau schwarz (solange „Bildschirmfotos erlauben“ aus ist). |
| P7a | Benachrichtigungen aufziehen, während GhulSuite entsperrt ist (und mit eingerichtetem Mail-Konto). | Die dauerhaften Benachrichtigungen („GhulSuite entsperrt“, Mail-Empfang) zeigen das **Schutzschild**; ein Briefumschlag erscheint nur bei einer neu eingegangenen Mail. Text: „Tippen zum Öffnen. Schütteln oder „Sperren“ sperrt sofort.“ |
| P7b | Optionen → Sperren → „Schütteln zum Sperren“ aus, Benachrichtigungen erneut aufziehen. | Der Text ändert sich sofort zu „Tippen zum Öffnen. „Sperren“ sperrt sofort.“ (kein Schütteln mehr erwähnt). Wieder an: Schütteln steht wieder da. |

## 4. GhulPass

| Nr. | Schritt | Erwartet |
|---|---|---|
| G1 | „+“ → Anmeldung anlegen (Titel, Benutzer, Passwort über Generator, URL, Notiz). | Eintrag erscheint in der Liste. |
| G2 | Je einen Eintrag Notiz, Karte, WLAN, Identität, SSH anlegen. | Alle speicherbar und anzeigbar. |
| G3 | Eintrag öffnen, Passwort anzeigen und kopieren. Nach eingestellter Zeit Zwischenablage prüfen. | Kopiert; Zwischenablage wird automatisch geleert. Android zeigt keine Vorschau des Passworts. |
| G4 | Bearbeiten, speichern; löschen. | Änderungen bleiben nach Sperren/Entsperren; gelöscht ist weg. |
| G5 | Suche nach Teil des Titels oder Benutzernamens. | Liste filtert. |
| G6 | Zweiten GhulPass-Tresor „Arbeit“ anlegen (Optionen → Tresore). „+“ antippen. | Abfrage „In welchen Tresor?“. |
| G7 | Filter-Chips „Alle / Privat / Arbeit“. | Liste zeigt jeweils die richtigen Einträge; bei „Alle“ steht der Tresorname dabei. |

## 5. GhulAuth

| Nr. | Schritt | Erwartet |
|---|---|---|
| A1 | „+“ → QR-Code einer Test-2FA-Seite scannen (z. B. eine Demo-Seite für TOTP). | Felder vorausgefüllt, Konto hinzugefügt. |
| A2 | Codes mit einer anderen Authenticator-App oder der Website vergleichen. | Identisch, Countdown passt. |
| A3 | Manuell anlegen (Base32-Schlüssel), otpauth-Link einfügen. | Beides funktioniert, ungültiger Schlüssel wird abgelehnt. |
| A4 | Lang drücken → umbenennen, löschen. | Funktioniert. |
| A5 | „Codes verbergen“ und „Nächsten Code zeigen“ unter Optionen → GhulAuth. | Codes erst nach Antippen sichtbar; in den letzten Sekunden erscheint der nächste Code. |
| A6 | Google Authenticator (zweites Gerät): Konten exportieren, den Export-QR in GhulAuth scannen. | Alle Konten auf einmal übernommen, Meldung mit Anzahl. Erneut scannen: „schon vorhanden“. |

## 6. Tresore und einzelne Tresore

Für T5 bis T14 brauchst du Tresordateien auf dem USB-Stick oder in einem Ordner (z. B. aus T2) und für T13 ein zweites
Gerät.

| Nr. | Schritt | Erwartet |
|---|---|---|
| T1 | Optionen → Tresore → „Neuer Tresor“, umbenennen. Sperren und entsperren. | Neuer Name bleibt erhalten. |
| T2 | Menü eines Tresors → „Exportieren“, Datei speichern. | Verschlüsselte `.ghlpss`/`.ghltotp`-Datei. |
| T3 | „Tresordatei hinzufügen“ → die gerade exportierte Datei. | Meldung „schon vorhanden“ (gleicher Tresor). |
| T4 | Tresor löschen (Menü → Löschen). | Warnung; danach ist er weg. Über „Tresordatei hinzufügen“ + Export aus T2 wieder da. Hinweis „Nicht gespeicherte Änderungen“ erscheint. |
| T5 | Gesperrt: Startbildschirm bzw. Entsperren-Seite → „Einzelnen Tresor öffnen“ → eine `.ghlpss` (z. B. aus T2 oder von einer anderen Suite). | Auswahl „Tresordatei öffnen“: „… ist ein GhulPass-Tresor. Was möchtest du tun?“ mit „Nur diesen Tresor öffnen“ und „Zu meiner Suite hinzufügen“ (ohne Suite auf dem Gerät: „Neue Suite damit anlegen“). |
| T6 | „Nur diesen Tresor öffnen“ → Passwort und Key bzw. Schlüsseldatei **des Tresors**. | Oben „Einzeln geöffnet: … Deine Suite bleibt gesperrt. Jede Änderung wird direkt in diese Datei zurückgeschrieben.“ Nur dieser Tresor ist zu sehen. Schutz, weitere Tresore, Server und Suite-Datei lassen sich in diesem Modus nicht ändern. |
| T7 | Im einzeln geöffneten Tresor einen Eintrag anlegen und einen ändern; dann „Tresor schließen“. Die Datei erneut einzeln öffnen. | Nach jeder Änderung kurz „Die Änderungen werden in die Datei geschrieben …“, dann „Alles ist in der Datei.“ Nach dem erneuten Öffnen sind beide Änderungen da. Die eigene Suite ist unverändert. |
| T8 | Einzeln geöffnet: USB-Stick abziehen, einen Eintrag ändern. Dann den Stick wieder einstecken und „Erneut versuchen“. | Warnstreifen „Die Datei ließ sich nicht schreiben (ist der Speicher noch da?). Deine Änderungen sind noch hier.“ Nach dem Einstecken wird geschrieben; nichts geht verloren. Bleibt der Stick weg, kommen die Änderungen beim nächsten Mal als „Änderungen nicht in der Datei“ wieder (jetzt schreiben, als neue Datei speichern oder verwerfen). |
| T9 | Einzeln geöffnet: Dieselbe Tresordatei auf einem zweiten Gerät ändern (oder durch eine andere Kopie ersetzen), dann hier etwas ändern. | „Die Datei wurde seit dem Öffnen anderswo geändert. Mit diesem Tresor überschreiben oder ihn als neue Datei speichern.“ Nichts wird still überschrieben. |
| T10 | „Einzelnen Tresor öffnen“ mit einer `.ghldrv`- oder `.ghlmail`-Datei. | „GhulDrive- und GhulMail-Tresore brauchen die Server und Konten einer Suite. Sie lassen sich nur zu einer hinzufügen.“ Nur „Zu meiner Suite hinzufügen“ ist möglich. |
| T11 | „Zu meiner Suite hinzufügen“ mit einem Tresor einer **anderen** Suite (anderes Passwort). | „Nach dem Entsperren wird … zu deiner Suite hinzugefügt.“ Nach dem Entsperren der eigenen Suite einmal Passwort und Key bzw. Schlüsseldatei des Tresors; danach gehört er dazu und öffnet sich mit deinem Passwort, deinen Keys und deinem Wiederherstellungscode. Die Ausgangsdatei öffnet weiterhin nur ihren alten Stand. Hinweis „Nicht gespeicherte Änderungen“. |
| T12 | Auf einem Gerät **ohne** Suite: „Einzelnen Tresor öffnen“ → „Neue Suite damit anlegen“. | Öffnet mit Passwort + Key des Tresors; Hinweis „dein bisheriges Notfallblatt gilt weiter“ (bzw. neuer Code, wenn der Tresor keinen hatte); nur dieser Bereich ist eingeschaltet. |
| T13 | GhulPass: zwei Einträge lange drücken → Auswahlkopf → „Als Tresor weitergeben“. Name, neues Passwort, zweiter Faktor „Neue Schlüsseldatei“, zur Bestätigung dein eigenes Master-Passwort (erst falsch, dann richtig). | Falsches Master-Passwort: abgelehnt. Richtig: „Speichere jetzt die neue Schlüsseldatei …“, dann „Tresordatei speichern“, dann der Wiederherstellungscode des neuen Tresors (nur jetzt angezeigt). Am Ende „Fertig. Gib Tresordatei, Schlüsseldatei und Passwort getrennt weiter.“ |
| T14 | Den Tresor aus T13 auf dem zweiten Gerät mit „Nur diesen Tresor öffnen“ öffnen: erst mit deinem eigenen Passwort und Key, dann mit dem neuen Passwort und der neuen Schlüsseldatei, dann mit seinem Wiederherstellungscode. | Eigener Schutz: öffnet nicht. Neuer Schutz: genau die zwei Einträge, ohne Passwort-Verlauf. Code: öffnet ebenfalls. |
| T15 | Wie T13 in GhulAuth, zweiter Faktor „Security Key“ (zweiter Key). | Zwei Berührungen; der neue Tresor öffnet nur mit seinem Passwort und diesem Key. |

## 7. Schutz und Wiederherstellung

| Nr. | Schritt | Erwartet |
|---|---|---|
| S1 | Optionen → Sicherheitsschlüssel & Schlüsseldateien → „Security Key hinzufügen“ mit zweitem Key (zwei Berührungen). | Zweiter Key in der Liste; beide entsperren. Code bleibt gültig. **Kein** Schlüsselwechsel und keine Aufforderung zum Speichern der Suite-Datei (Hinzufügen ist keine Schutzänderung, die etwas ungültig macht); nur der Hinweis „Nicht gespeicherte Änderungen“. |
| S2 | „Schlüsseldatei hinzufügen“ (erstellen und speichern). | Datei in der Liste; Entsperren damit möglich (U6). |
| S3 | Schlüsseldatei entfernen, später den letzten Key entfernen versuchen. | Entfernen geht; der letzte Key/die letzte Datei lässt sich **nicht** entfernen. |
| S4 | Suite mit Schlüsseldatei (E18): Key hinzufügen, dann Datei entfernen. | Key funktioniert, Datei nicht mehr, **alter Wiederherstellungscode funktioniert weiterhin**. |
| S5 | „Neuer Wiederherstellungscode“. Sperren, alten Code probieren, dann neuen. | Alter Code: ungültig. Neuer: geht. |
| S6 | Passwort ändern (aktuelles falsch, dann richtig). | Falsch: Meldung. Richtig: neues Passwort gilt, Fingerabdruck/PIN-Zugang ist neu einzurichten. Danach Dialog „Suite-Datei jetzt speichern“ (S12). |
| S7 | Vor S5: die Suite-Datei speichern und eine Kopie davon aufheben. Nach S5 die Suite-Datei neu speichern. Auf einem Gerät ohne Suite die **alte** Kopie mit dem **alten** Code öffnen; dann die **neue** Datei mit dem alten Code. | Die alte Kopie öffnet sich (Stand von damals, Hinweis, dass ohne Passwort nicht geprüft werden konnte). Die neue Datei öffnet sich mit dem alten Code nicht. Fingerabdruck/PIN funktionieren nach S5 weiter. |
| S8 | Zwei Keys (oder Key + Schlüsseldatei). Suite-Datei speichern und eine Kopie aufheben, dann einen Key entfernen und neu speichern. Mit dem entfernten Key + Passwort die Kopie öffnen, dann die neue Datei. | Kopie: öffnet sich (auf einem Gerät, das den neueren Stand nie gesehen hat). Neue Datei: „Key nicht eingetragen“. Der verbliebene Key öffnet alles, **ohne** dass er beim Entfernen berührt werden musste. Fingerabdruck/PIN ist neu einzurichten. |
| S11 | Die Suite-Datei mit einem ZIP-Programm (z. B. Dateien-App oder 7-Zip) ansehen. | Darin nur `suite.ghlsuite`, `export.ghlexp` und Tresordateien, die nach ihrer Kennung heißen (z. B. `3f9a1c2b7d4e5f60.ghlpss`); kein Tresorname, kein lesbarer Inhalt. |
| S12 | Nach S3 (Datei entfernt), S5 und S6 jeweils auf den Dialog achten. | Jedes Mal „Suite-Datei jetzt speichern“: Der Schutz wurde geändert; ältere Kopien öffnen sich zwar weiter mit dem alten Schutz, kommen aber nicht mehr an GhulDrive und die Server. „Jetzt speichern“ führt zu Optionen → Suite-Datei; „Später“ schließt ihn, der Hinweis „Nicht gespeicherte Änderungen“ bleibt. |
| S13 | Eine Tresordatei mit einem Hex-Editor an einer Stelle im Inhalt verändern (oder ein Byte am Ende) und mit „Einzelnen Tresor öffnen“ öffnen. | Mit dem richtigen Passwort und Key: Meldung „verändert“ bzw. „beschädigt“, **nie** „falsches Passwort“; nichts wird geöffnet. |

## 8. Import

| Nr. | Schritt | Erwartet |
|---|---|---|
| I1 | Optionen → Import und Export → Importieren → Proton-Pass-Export (ZIP). | „Erkannt: Proton Pass“ mit Anzahl. Ziel wählen, Importieren → Einträge da, Notizen/Karten/Zusatzfelder übernommen, Papierkorb nicht. |
| I2 | Option „2FA-Codes aus Anmeldungen auch in GhulAuth übernehmen“. | Die TOTP aus Proton-Pass-Logins erscheinen in GhulAuth. |
| I3 | Dieselbe Datei erneut importieren. | „0 importiert, n waren schon vorhanden“. |
| I4 | Proton Pass CSV, Bitwarden JSON und CSV, KeePassXC CSV (soweit vorhanden). | Jeweils richtig erkannt, Felder korrekt (auch Passwörter mit Komma/Anführungszeichen). |
| I5 | Proton Authenticator, Aegis (unverschlüsselt), 2FAS. | 2FA-Konten erscheinen, Codes stimmen (A2). |
| I6 | Eine **verschlüsselte** Exportdatei (z. B. Aegis oder Bitwarden verschlüsselt). | Meldung „Diese Exportdatei ist verschlüsselt …“. |
| I7 | Eine beliebige andere Datei (z. B. ein Foto). | „Dieses Dateiformat wird nicht erkannt“. |
| I8 | Exportdateien danach löschen. | - |

## 9. Export

| Nr. | Schritt | Erwartet |
|---|---|---|
| X1 | Unverschlüsselt exportieren → GhulPass-Tresor → falsches Passwort. | „Falsches Passwort“. |
| X2 | Richtiges Passwort → speichern. | CSV-Datei; Hinweis, sie zu löschen. |
| X3 | GhulAuth-Tresor exportieren. | Textdatei mit `otpauth://`-Zeilen. |
| X4 | Die CSV in einen neuen Tresor zurück importieren (oder in eine andere App). | Gleiche Einträge. Danach Dateien löschen. |

## 10. Autofill (entfernt)

Entfällt: Autofill wurde entfernt, weil es nicht zuverlässig funktionierte (Testprotokoll F136).

## 11. Einstellungen und Allgemeines

| Nr. | Schritt | Erwartet |
|---|---|---|
| O1 | Module: GhulAuth ausschalten, wieder einschalten. | Verschwindet aus dem Menü, Daten bleiben. |
| O2 | Design hell/dunkel/System. | Wechselt sofort, alles lesbar. |
| O3 | Android-Optionen → Apps → GhulSuite → Sprache: Englisch. | Oberfläche englisch. |
| O4 | Flugmodus an, alles Obige kurz wiederholen (Entsperren, Codes, GhulPass, GhulAuth). | Funktioniert vollständig offline (nur GhulDrive, Freigabelinks und Mail brauchen das Netz). |
| O5 | Handy drehen in Listen, Bearbeiten und Dialogen. | Kein Datenverlust, keine Abstürze. |

## 12. Eigene Server (nur SFTP)

Ein Server wird nur für GhulDrive und Freigabelinks gebraucht. Angemeldet wird immer mit dem SSH-Schlüssel der Suite;
ein Passwort dient höchstens einmal dazu, diesen Schlüssel auf dem Server einzutragen.

| Nr. | Schritt | Erwartet |
|---|---|---|
| SY1 | Optionen → Server und Freigaben → „Server hinzufügen“ (oder Optionen → GhulDrive → „Speicherort hinzufügen“). Adresse, Port und Benutzer eingeben, „SSH-Schlüssel“ gewählt lassen. | Nur SFTP zur Wahl. Ein öffentlicher Schlüssel `ssh-ed25519 …` wird angezeigt; „Kopieren“ legt ihn in die Zwischenablage. Es gibt noch **keine** Ordnerwahl, nur „Verbindung testen“; darunter die Schritte 1. bis 4. untereinander. |
| SY2 | Schlüssel auf dem Server in `~/.ssh/authorized_keys` eintragen, dann „Verbindung testen“. | Dialog „Server-Schlüssel prüfen“ mit `SHA256:…`. Er stimmt mit dem Fingerabdruck aus V6 überein. |
| SY3 | „Fingerabdruck stimmt“, in der Ordnerliste „ghulsuite (Neu anlegen, empfohlen)“ wählen, „Verbindung prüfen“, „Server speichern“. | Nach dem Fingerabdruck: „Verbindung klappt. Wähle jetzt den Ordner …“ und die Ordnerliste öffnet sich von selbst. Nach dem Speichern erscheint der Server in der Liste. Auf dem Server gibt es den Ordner `ghulsuite/` (kein Unterordner `suite/`: die Suite liegt nicht auf dem Server). |
| SY4 | Server mit falschem Fingerabdruck-Abgleich: „Abbrechen“ im Dialog. | Nichts wird gespeichert. |
| SY4b | Einfachster Weg: „Server hinzufügen“ → „Passwort“ → SSH-Passwort eingeben → „Verbindung testen“ → Fingerabdruck bestätigen → Ordner wählen → „Verbindung prüfen“ → „Server speichern“. | Hinweis, dass das Passwort nur einmal benutzt wird, um den Schlüssel einzutragen. Server erscheint mit „Ordner: /home/…/ghulsuite“. Auf dem Server steht in `~/.ssh/authorized_keys` eine neue Zeile `ssh-ed25519 … GhulSuite`, alte Zeilen sind unverändert. Das Passwort ist nicht gespeichert (der Server funktioniert auch nach einer Passwortänderung weiter). |
| SY5 | Wie SY4b mit falschem SSH-Passwort. | „Der Server hat die Anmeldung abgelehnt …“; nichts gespeichert. |
| SY6 | Server, der nur Passwort-Anmeldung erlaubt (Schlüssel werden nicht gelesen), wie SY4b einrichten. | Der Server lässt sich nicht speichern, weil die Anmeldung mit dem Schlüssel scheitert. Ein Passwort als dauerhafter Zugang wird nicht angeboten. |
| SY7 | Suite-Datei speichern und auf dem zweiten Gerät öffnen (Abschnitt 35), dort Optionen → Server und Freigaben. | Derselbe Server ist da und verbindet ohne weitere Eingabe (der SSH-Schlüssel reist in der Suite-Datei mit). |
| SY14 | Flugmodus an, GhulPass und GhulAuth benutzen, GhulDrive öffnen. | GhulPass, GhulAuth und GhulCal funktionieren ohne Fehlermeldung. GhulDrive meldet „Keine Verbindung …“; nach dem Flugmodus geht es wieder. |
| SY15 | Auf dem Server den Host-Key tauschen (oder einen anderen Server unter derselben Adresse) und GhulDrive öffnen. | Warnung „Der Server zeigt einen anderen Schlüssel …“, es wird nichts übertragen. |
| SY16 | Server entfernen. | Rückfrage; Server weg; die Dateien auf dem Server bleiben, wenn nicht ausdrücklich gewählt (B7b). |

## 13. GhulDrive

| Nr. | Schritt | Erwartet |
|---|---|---|
| D1 | Tab GhulDrive (ggf. unter Optionen → Module einschalten). | „GhulDrive einrichten“ mit deinen Servern. |
| D2 | „<Server> verwenden“. | Leere Liste „Meine Dateien“, unten „Auf dem Server: /home/…/ghulsuite/drive“. Dort entsteht `index.ghldrv`. |
| D3 | „Hochladen“, ein Foto oder PDF wählen. | Unter der Kopfleiste der Streifen „Lade … hoch“ mit Balken und Größe; danach „… hochgeladen.“; Datei mit Größe und Datum in der Liste. |
| D4 | Auf dem Server in `ghulsuite/drive/` schauen. | Nur `index.ghldrv`, `index.bak` (nach der zweiten Änderung), während einer Änderung kurz `index.lock`, und `blocks/xx/<lange ID>`: **kein** Dateiname, Inhalt unlesbar. |
| D5 | Große Datei (> 50 MB, z. B. Video) hochladen. | Fortschritt läuft durch; App bleibt bedienbar; Datei erscheint. |
| D5a | App ganz schließen, neu starten, entsperren und sofort GhulDrive öffnen. | Die Karte oben zeigt nie „Noch kein Server“; höchstens kurz „Wird geladen …“, dann den Servernamen. |
| D6 | Datei antippen → „Speichern“ → Speicherort wählen. | Streifen „Lade … herunter“ mit Fortschritt, dann „… gespeichert.“; die Datei ist identisch (öffnen, ansehen). |
| D7 | Upload starten und im Streifen „Abbrechen“. | Bricht ab mit „Hochladen von … abgebrochen.“, keine halbe Datei in der Liste. |
| D7a | „Hochladen“ und im Auswahlfenster mehrere Dateien markieren (Android: lange drücken; Windows: Strg oder Umschalt). | Streifen „Lade 5 Dateien hoch“, darunter „2 von 5 fertig · Name · Größe“; nach jeder Datei stimmen Liste und „X Dateien“; am Ende „5 Dateien hochgeladen.“. Scheitert eine Datei, laufen die anderen weiter und die Meldung nennt sie; „Abbrechen“ stoppt den Rest mit „Hochladen abgebrochen: 2 von 5 Dateien fertig.“. |
| D7b | In der Galerie mehrere Fotos auswählen → Teilen → „In GhulDrive speichern“ (gesperrt: erst entsperren). | GhulDrive öffnet sich, die Fotos werden nacheinander in „Meine Dateien“ hochgeladen; ohne eingerichtetes GhulDrive kommt der Hinweis, es zuerst einzurichten. Andere Apps können so keine Dateien von GhulSuite selbst hineinschieben. |
| D8 | „Hinzufügen“ → „Neuer Ordner“ → Name; Ordner öffnen, Datei hineinladen; Zurück-Pfeil. | Ordnerstruktur stimmt, Pfad oben passt. |
| D9 | Datei umbenennen, in einen anderen Ordner verschieben („Hierher verschieben“). | Name bzw. Ort ändern sich. |
| D10 | Suche nach einem Teil des Dateinamens. | Treffer aus allen Ordnern. |
| D11 | Datei löschen; Ordner mit Inhalt löschen. | Weg; nach dem Löschen auf dem Server weniger Blöcke. |
| D12 | Zweites Gerät (nach dem Öffnen der Suite-Datei, SD5): GhulDrive öffnen, aktualisieren. | Dieselben Dateien und Ordner; Herunterladen klappt. Mehr zu zwei Geräten: Abschnitt 36. |
| D13 | Dieselbe Datei zweimal hochladen. | Zwei Einträge in der Liste, auf dem Server aber nicht doppelter Platz. |

## 14. GhulMail

| Nr. | Schritt | Erwartet |
|---|---|---|
| M1 | Tab GhulMail → „E-Mail-Konto hinzufügen“. E-Mail-Adresse eingeben. | Unter „Eingang/Ausgang“ werden passende Server vorgeschlagen (bei bekannten Anbietern genau, sonst `imap.`/`smtp.` + Domain). |
| M2 | Passwort (bzw. App-Passwort), Name eingeben. Erst „Verbindung prüfen“, dann „Speichern“. | Vor der Prüfung gibt es kein „Speichern“. Die Prüfung zeigt vier Haken: Posteingangsserver erreichbar, Anmeldung am Posteingang, Postausgangsserver erreichbar, Anmeldung am Postausgang, und „Verbindung klappt …“; erst dann erscheint „Speichern“. Durch die Prüfung wird nichts gespeichert. Signatur ist mit „Viele Grüße“ und deinem Namen vorbelegt. Nach „Speichern“ erscheint der Posteingang. |
| M2a | Nach erfolgreicher Prüfung Passwort, Adresse, Benutzername, Server oder Port ändern. | Ergebnis und „Speichern“ verschwinden sofort; erst eine neue Prüfung bringt sie zurück. Nur Name, Absendername oder Signatur ändern: „Speichern“ bleibt. |
| M2b | Bestehendes Konto bearbeiten, nur den Namen oder die Signatur ändern. | „Speichern“ ist ohne Prüfung möglich. Ein neues Passwort verlangt erst „Verbindung prüfen“. |
| M3 | Falsches Passwort, „Verbindung prüfen“. | Server erreichbar (grün), beide Anmeldungen rot mit „Anmeldung abgelehnt. Prüfe Benutzername und Passwort …“; kein „Speichern“, nichts gespeichert. Falsche Serveradresse: „Posteingangsserver erreichbar“ rot mit dem Grund, die Anmeldung grau „Nicht geprüft“. |
| M4 | „Server-Einstellungen anzeigen“: Ports ändern, falscher Server. | Für IMAP gibt es nur SSL/TLS (keine Wahl), für den Ausgang SSL/TLS oder STARTTLS. Falscher Server: klare Meldung „Keine Verbindung …“. |
| M5 | Eigener Mailserver mit selbst signiertem Zertifikat (falls vorhanden), „Verbindung prüfen“. | Dialog „Zertifikat prüfen“ mit Fingerabdruck; nach „Fingerabdruck stimmt“ läuft die Prüfung von selbst noch einmal und zeigt die Haken; dann „Speichern“. |
| M6 | Liste: ungelesene fett, Stern, Büroklammer bei Anhängen, Datum. „Weitere laden“ bei mehr als 50. | Stimmt mit dem Postfach im Webmail überein. |
| M7 | Ordner wechseln (Gesendet, Papierkorb …), Aktualisieren. | Richtige Inhalte. |
| M8 | Mail öffnen. | Text lesbar, Absender/Empfänger/Datum; bei HTML-Mails Hinweis „Als Text angezeigt …“; **keine** Bilder aus dem Internet. Im Webmail ist sie jetzt gelesen. |
| M9 | Anhang „Speichern“. | Datei korrekt gespeichert. |
| M10 | „Schreiben“: an die zweite Adresse, mit Anhang, senden. | „Nachricht gesendet.“; kommt an (inkl. Anhang, Umlaute korrekt, Signatur); liegt in „Gesendet“. |
| M11 | Antworten, Allen antworten, Weiterleiten. | Betreff „AW:“/„WG:“, Zitat, Empfänger vorausgefüllt; Antwort erscheint beim Empfänger im selben Gesprächsverlauf. |
| M12 | Markieren, Als ungelesen, Verschieben in einen Ordner. | Wirkt sich auch im Webmail aus. |
| M13 | Löschen im Posteingang, dann im Papierkorb. | Erst in den Papierkorb, dort endgültig gelöscht. |
| M14 | Suche nach einem Wort aus Betreff oder Text. | Passende Mails. |
| M15 | Zweites Konto anlegen. | Konto-Auswahl oben; beim Schreiben Absender wählbar. |
| M16 | Optionen → GhulMail → Konto → Bearbeiten, Passwort leer lassen, Namen ändern, speichern. | Name geändert, Anmeldung funktioniert weiter. |
| M17 | Konto entfernen. | Konto weg; Mails auf dem Server unverändert. |
| M18 | Sperren und entsperren; Suite-Datei speichern und auf Gerät 2 öffnen (SD5). | Konten sind nach Entsperren bzw. auf Gerät 2 vorhanden. |

## 15. Benachrichtigungen

| Nr. | Schritt | Erwartet |
|---|---|---|
| N1 | Neues Mailkonto anlegen (oder nach dem Update entsperren). | Benachrichtigungen für das Konto sind **von selbst an** (Optionen → GhulMail → Benachrichtigungen). Android fragt einmal nach Benachrichtigungen und Akku-Optimierung → erlauben. |
| N1b | Optionen → GhulMail. | Grün: „GhulMail darf im Hintergrund laufen“. Sonst Hinweis mit Knopf „Im Hintergrund erlauben“; danach grün. In der Statusleiste steht die stille Benachrichtigung „GhulMail ist bereit“. |
| N2 | GhulSuite sperren, Handy sperren. Von der zweiten Adresse eine Mail schicken. | **Innerhalb weniger Sekunden** Benachrichtigung „<Kontoname>, 1 neue E-Mail“, **ohne** Absender und Betreff. |
| N2b | Handy neu starten, ohne GhulSuite zu öffnen eine Mail schicken. | „GhulMail ist bereit“ ist wieder da, die Benachrichtigung kommt sofort. |
| N3 | Benachrichtigung antippen (vorher in GhulMail den Postausgang oder einen anderen Ordner offen lassen). | Nach dem Entsperren öffnet sich **genau diese Mail** im Posteingang des Kontos; zurück führt in den Posteingang. Bei mehreren neuen Mails: der Posteingang. Die Benachrichtigung verschwindet. |
| N4 | Schalter aus, wieder Mail schicken. | Keine Benachrichtigung mehr; „GhulMail ist bereit“ verschwindet (wenn kein anderes Konto benachrichtigt). |
| N5 | Schalter an, dann **Sofortsperre** (Schütteln oder Schnellkachel), danach Mail schicken. | Gesperrt, aber **Benachrichtigungen kommen weiter**; Schalter bleibt an. |
| N6 | Konto-Passwort in GhulSuite ändern (M16 mit neuem Passwort), Benachrichtigung an. | Benachrichtigungen kommen weiter. |
| N7 | Mailpasswort auf dem **anderen** Gerät ändern, Suite-Datei dort speichern, hier öffnen und einmal entsperren. | Benachrichtigungen kommen weiter (die Kopie für den Empfang wird beim Entsperren erneuert). |
| N8 | GhulSuite aus der Liste der letzten Apps wischen, Mail schicken. | Benachrichtigung kommt sofort. |
| N9 | WLAN aus (nur mobile Daten), Mail schicken; dann WLAN wieder an, Mail schicken. | Beide Male Benachrichtigung innerhalb weniger Sekunden (bei Netzwechsel verbindet GhulMail sofort neu). |
| N10 | Handy 30 Minuten unberührt liegen lassen (Bildschirm aus), dann Mail schicken. | Benachrichtigung innerhalb von 1 bis 2 Minuten, meist sofort. |
| N11 | Android-Einstellungen → Apps → GhulSuite → „Beenden erzwingen“, Mail schicken. | **Keine** Benachrichtigung (Android-Regel). Nach dem nächsten Öffnen von GhulSuite kommt sie wieder. |
| N12 | Zwei Konten mit Benachrichtigung, an beide gleichzeitig eine Mail. | Zwei Benachrichtigungen; jede öffnet die Mail ihres Kontos. Keine Mail meldet sich doppelt. |

## 16. Neues Design und Einstellungen

| Nr. | Schritt | Erwartet |
|---|---|---|
| DS1 | Durch alle Tabs blättern. | Überall dieselbe Kopfzeile: Bereichsname, darunter farbige Kontextzeile (Tresor, Speicherort bzw. „Konto · Ordner“) mit Pfeil; rechts das Schloss. |
| DS2 | Kontextzeile in GhulPass antippen. | Auswahl „Tresor wählen“ (Alle / einzelne Tresore mit Anzahl Einträgen, „Tresore verwalten“). Wahl wirkt sofort. |
| DS3 | Kontextzeile in GhulDrive und GhulMail antippen. | Drive: alle Speicherorte (der aktive markiert, ein anderer schaltet um), „Freigabelinks“, „Speicherorte verwalten“. Mail: Konten, Ordner mit Symbolen, „Neues Konto“. |
| DS4 | Passwort eines Eintrags anzeigen lassen. | Ziffern und Sonderzeichen sind farbig, Buchstaben normal. |
| DS5 | Tab Einstellungen. | Karte „Meine Suite“ mit „Sperren“, darunter Gruppen Sicherheit, Schutz, Daten, Server, Bereiche, Allgemein; jede Zeile mit kurzem Status (z. B. „1 Schlüssel, 1 Schlüsseldateien“; unter Daten „Suite-Datei“ mit „Öffnen und speichern (.ghlzip)“ bzw. „Nicht gespeicherte Änderungen“ und „Server und Freigaben“). |
| DS6 | Jede Zeile öffnen, Zurück. | Eigene Seite mit Titel; Zurück führt zur Übersicht. Abgeschaltete Module erscheinen nicht unter „Bereiche“. |
| DS7 | Dunkles Design (Optionen → Ansicht). | Alle Seiten gut lesbar. |

## 17. Servertypen und Server-Modul

Ein Server in der App ist ein Webspace bzw. Server: seine SFTP-Anmeldung (mit dem SSH-Schlüssel der Suite) und, wenn gewünscht, das GhulSuite-Server-Modul (eine PHP-Datei), das die App über dieselbe Anmeldung installiert. Das Modul wird nur für Freigabelinks gebraucht. FTP und FTPS gibt es nicht mehr: Ein Webhosting braucht SSH-Zugang.

| Nr. | Schritt | Erwartet |
|---|---|---|
| ST1 | Optionen → Server und Freigaben → „Server hinzufügen“, ebenso Optionen → GhulDrive → „Speicherort hinzufügen“. | Nur SFTP, kein FTP/FTPS und kein PHP-Modul als eigene Verbindungsart. |
| ST2 | Webhosting **mit SSH-Zugang**: Adresse, Port (oft 22), Benutzer, SSH-Passwort → „Verbindung testen“ (trägt den Schlüssel ein, SY4b) → Ordner `ghulsuite` → „Verbindung prüfen“ → „Server speichern“. | Danach steht der Server in der Liste mit Anmeldung und „Ordner: /…/ghulsuite“; das Passwort ist nicht gespeichert. |
| ST3 | Webhosting, das nur FTP anbietet (kein SSH). | Keine Verbindung möglich, verständliche Meldung; nichts gespeichert. |
| ST4 | GhulDrive mit dem Webhosting-Server (wie D2 bis D6). | Funktioniert; auf dem Webhosting nur verschlüsselte Dateien. |
| ST4a | Server hinzufügen, beim GhulSuite-Ordner „Ordner wählen“ und in `public_html` (oder `htdocs`, einen Ordner mit Domainnamen) wechseln. | Der Ordner ist in der Liste als „Ordner der Website, nicht für GhulSuite“ markiert; darin steht rot, den GhulSuite-Ordner besser daneben anzulegen. Der Hinweis unter dem Feld sagt, dass der Ordner nicht in der Website liegen soll. |
| ST5 | Serverliste ansehen. | Je Server: Name, Anmeldung (Benutzer@Adresse:Port), Ordner, wofür er genutzt wird (GhulDrive) bzw. „Noch nicht genutzt“ und „Server-Modul: nicht installiert (nur für Freigabelinks nötig)“ mit „Modul installieren“. Ein eigener Server von GhulDrive steht auch da, ohne Entfernen-Knopf („Du änderst ihn in GhulDrive“). |
| ST6 | Beim Server des Webhostings „Modul installieren“ → im Ordner-Browser den Ordner der Website wählen (z. B. `public_html`), Adresse leer lassen → „Installieren und prüfen“. | Vier grüne Haken: „Moduldatei hochgeladen“ (mit Pfad), „Antwortet über HTTPS“ (mit Adresse), „Liest den GhulSuite-Ordner“, „GhulSuite-Ordner aus dem Web nicht abrufbar“. „Das Server-Modul läuft unter …“, „Fertig“ führt zurück; die Liste zeigt „Server-Modul: Version …“. |
| ST7 | Wie ST6, aber die Website hat eine andere Adresse als die üblichen (z. B. eigene Domain auf einem Unterordner). | „Antwortet über HTTPS“ rot: „Keine Antwort unter … Trag oben die Adresse des Moduls ein …“ mit den probierten Adressen; „Liest den GhulSuite-Ordner“ grau mit „Nicht geprüft, weil der HTTPS-Schritt fehlschlug“. Richtige Adresse eintragen, erneut „Installieren und prüfen“: alles grün. |
| ST8 | Adresse mit `http://` eintragen. | Hinweis „Die Adresse muss mit https:// beginnen.“, „Installieren und prüfen“ bleibt gesperrt. |
| ST9 | Webhosting ohne PHP (oder ein Ordner, in dem PHP nicht läuft). | „PHP läuft“ rot: „Dort läuft kein PHP … Die App hat sie wieder gelöscht. Auf diesem Webhosting kann das Modul nicht laufen.“ `ghulsuite.php` liegt nicht mehr auf dem Server; die Liste zeigt weiter „nicht installiert“. |
| ST10 | Webhosting, dessen PHP den GhulSuite-Ordner nicht lesen darf (z. B. `open_basedir`). | „Liest den GhulSuite-Ordner“ rot mit Erklärung, dass Freigabelinks dort nicht möglich sind. Nichts wird gespeichert. |
| ST10a | Webhosting, dessen PHP im GhulSuite-Ordner nicht schreiben darf (Rechte von `shares/` z. B. auf 555). | „Liest den GhulSuite-Ordner“ rot: „PHP darf den GhulSuite-Ordner lesen, aber in seinem Ordner „shares“ nicht schreiben …“. Nichts wird gespeichert; Rechte korrigieren, erneut prüfen: grün. |
| ST10b | GhulSuite-Ordner innerhalb der Website anlegen (z. B. `public_html/ghulsuite`) auf einem Webserver, der `.htaccess` nicht beachtet (z. B. nginx). | „GhulSuite-Ordner aus dem Web nicht abrufbar“ rot mit der Adresse, unter der er abrufbar war; das Modul ist wieder entfernt und nicht gespeichert. Mit Apache (`.htaccess` wirkt) bleibt der Schritt grün. |
| ST11 | Im Browser `https://deine-domain/ghulsuite.php` öffnen. | Seite „Geteilte Datei“ mit „Der Link ist unvollständig“ (es fehlt der Teil nach #). |
| ST12 | Liegt der GhulSuite-Ordner innerhalb der Website: ihn im Browser öffnen. | Zugriff verweigert (403), kein Inhalt. |
| ST13 | Server mit älterer Modulversion (mit einer älteren App installiert). | Liste: „Server-Modul: Version x, Version y ist verfügbar“ und „Modul aktualisieren“; danach „Server-Modul aktualisiert (Version y)“. |
| ST14 | „Modul entfernen“. | Rückfrage „Server-Modul von … entfernen?“: „Abbrechen“ lässt alles, wie es ist. „Entfernen“: Datei weg, Links dieses Servers funktionieren nicht mehr, Liste „nicht installiert“. |
| ST15 | Einen zweiten Server (anderes Webhosting) hinzufügen und dort ebenfalls das Modul installieren. | Beide Server haben ihre eigene Anmeldung und ihr eigenes Modul. Freigabelinks kommen immer vom Server von GhulDrive. |

## 18. Teilen

Freigabelinks gibt es nur über das Server-Modul auf dem Server von GhulDrive (Abschnitt 17). Einen statischen Teilen-Ordner gibt es nicht mehr.

| Nr. | Schritt | Erwartet |
|---|---|---|
| SH1 | GhulDrive auf einem Server **ohne** Modul: Datei → Teilen. | Zeile „Freigabelink“ mit „Braucht das GhulSuite-Server-Modul. Zum Installieren antippen.“ Antippen öffnet die Modul-Installation genau dieses Servers. |
| SH2 | Ohne eingerichtetes GhulDrive: GhulPass-Eintrag → ⋮; Optionen → GhulDrive → Freigabelinks. | „Sicher teilen …“ ist ausgegraut mit „Freigabelinks kommen vom Server von GhulDrive. Richte zuerst GhulDrive ein.“; die leere Liste der Freigabelinks sagt dasselbe. |
| SH3 | Mit Modul: Datei → Teilen → Freigabelink. | Keine Serverauswahl. „Gültig für“: 1 Tag, 7 Tage (vorgewählt), 30 Tage, Unbegrenzt. „Abrufe“: Unbegrenzt, 1× (einmalig), 3×, 5×, 10×. Schalter „Zusätzliches Passwort“. |
| SH4 | „Link erstellen“. | „Link bereit“ mit Link, Größe und Ablauf; „Link kopieren“ legt ihn in die Zwischenablage, QR-Code zeigt ihn. |
| SH5 | Link auf einem anderen Gerät/PC im Browser öffnen. | Seite zeigt Dateiname und Größe, „Herunterladen“ liefert die identische Datei. Der Teil nach # verschwindet aus der Adresszeile. |
| SH6 | Link mit „Zusätzliches Passwort“ erstellen, im Browser öffnen. | Passwortfeld; falsches Passwort → Fehlermeldung; richtiges → Download. |
| SH7 | Link um ein Zeichen kürzen oder ändern und öffnen. | „Der Link ist unvollständig …“ bzw. „Falsches Passwort oder der Link ist beschädigt“. Kein Download. |
| SH8 | „Abrufe“ auf „1× (einmalig)“ → Link erstellen, zweimal öffnen. | Erster Abruf klappt, zweiter: „Diese Freigabe gibt es nicht mehr …“. |
| SH9 | „5×“ → zweimal öffnen → GhulDrive → Kontextzeile → Freigabelinks. | Eintrag zeigt Ablauf und „2 von 5 Abrufen“. |
| SH10 | Freigabe mit „1 Tag“ erstellen, am nächsten Tag öffnen, ohne GhulSuite vorher zu öffnen. | „Diese Freigabe gibt es nicht mehr …“: das Modul beendet den Link selbst. |
| SH11 | Freigabelinks → Eintrag „Zurückziehen“ → Rückfrage bestätigen → Link im Browser neu laden. | Eintrag verschwindet sofort, Meldung „Link zurückgezogen“. Im Browser: „Diese Freigabe gibt es nicht mehr …“. Die Datei bleibt in GhulDrive. Auf einem zweiten Gerät mit der Suite-Datei: Freigabelinks öffnen, der Eintrag ist auch dort weg (die Freigaben liegen auf dem Server). |
| SH12 | Freigabelinks im Flugmodus öffnen. | Einträge mit Größe und Ablauf, ohne Abrufzahl (nie „-1“). |
| SH13 | In GhulDrive lange drücken, zwei Dateien und einen Ordner dazu antippen → Teilen → Freigabelink. | Feld „Titel des Links“ (vorbelegt „GhulDrive“) und Hinweis „Der Empfänger bekommt jede Datei einzeln.“ Im Browser: jede Datei einzeln zum Herunterladen, auch große Dateien (kein ZIP, keine Größengrenze). |
| SH14 | Eine geteilte Datei in GhulDrive in den Papierkorb (einzeln über das Datei-Blatt und mit Mehrfachauswahl). | Die Rückfrage sagt zusätzlich „1 aktiver Freigabelink endet.“ „Abbrechen“: Datei und Link bleiben. Bestätigt: Der Link endet zuerst (Browser: „Diese Freigabe gibt es nicht mehr …“, Freigabelinks ohne den Eintrag), dann liegt die Datei im Papierkorb. |
| SH15 | Eine Datei in einem geteilten Ordner im Papierkorb „Endgültig löschen“, bzw. den geteilten Ordner selbst in den Papierkorb. | Wie SH14: die Rückfrage nennt die Links, die enden; nach Bestätigung sind sie beendet. |
| SH16 | Eine geteilte Datei ändern (neue Version hochladen), dann den Link öffnen. | Der Link funktioniert weiter. |
| SH17 | Mehrfachauswahl ohne geteilte Dateien → „In den Papierkorb“. | Rückfrage „In den Papierkorb?“ mit der Anzahl, ohne Zeile zu Links; „Abbrechen“ lässt alles da. Bestätigt: weg, Meldung „Rückgängig“; „Rückgängig“ bringt alle zurück. |
| SH17a | Gleich nach „Rückgängig“ eine geteilte Datei wieder „In den Papierkorb“. | Die Rückfrage kommt sofort; solange „Freigabelinks werden geprüft …“ dasteht, ist „In den Papierkorb“ grau, danach steht die Zeile zu den endenden Links da und der Knopf geht. |
| SH18 | Datei → Teilen → „An App senden“ → z. B. Messenger oder Dateien-App. | Android-Teilen-Menü; die Ziel-App erhält die **entschlüsselte** Datei mit richtigem Namen. |
| SH19 | Nach SH18 GhulSuite sperren. | Die entschlüsselte Kopie im App-Cache wird gelöscht (ein erneutes Öffnen aus der Ziel-App kann fehlschlagen, gewollt). |
| SH20 | GhulPass → Eintrag öffnen → „Sicher teilen (Einmal-Link)“. | Formular mit „1 Tag“ und „1× (einmalig)“ vorgewählt, keine Serverauswahl. Link erstellen und im Browser öffnen: Textdatei mit Titel und Feldern (Benutzer, Passwort, …). Ohne Modul auf dem Server von GhulDrive führt „Sicher teilen …“ zur Modul-Installation. |

## 19. Modultrennung und GhulMail-Anbindung

| Nr. | Schritt | Erwartet |
|---|---|---|
| MO1 | GhulMail eingeschaltet, mindestens ein Konto: GhulDrive → Datei → Teilen. | „Als Mail-Anhang“ wird angeboten. |
| MO2 | „Als Mail-Anhang“. | „Schreiben“ öffnet sich: Betreff = Dateiname, Datei als Anhang. Senden an die zweite Adresse: Anhang kommt identisch an. |
| MO3 | Freigabelink erstellen → „Mit GhulMail senden“. | Entwurf mit Text „Hallo, hier ist … für dich:“ und dem Link. Nach dem Senden: Link in der empfangenen Mail anklicken → Datei öffnet sich im Browser. |
| MO4 | Datei über 20 MB → Teilen. | „Als Mail-Anhang“: „Zu groß für eine Mail, bitte als Link teilen“, nicht antippbar. |
| MO5 | Optionen → Module → GhulMail **aus**. Dann MO1 und SH6 wiederholen. | Kein „Als Mail-Anhang“, im Link-Dialog kein „Mit GhulMail senden“, nur „Kopieren“. Freigabelinks und „An App senden“ funktionieren normal. |
| MO6 | GhulMail ein, aber **kein** Konto (alle entfernt). | Wie MO5: keine Mail-Optionen. |
| MO7 | GhulDrive **aus**, GhulMail ein. | GhulMail funktioniert vollständig (Lesen, Schreiben, Anhänge aus Dateien). Der GhulPass-Einmal-Link funktioniert weiter, solange GhulDrive eingerichtet ist und sein Server das Server-Modul hat. |
| MO8 | Alle Module außer GhulPass aus. | App läuft normal; Einstellungen zeigen unter „Bereiche“ nur GhulPass. Das Server-Modul gehört zu GhulDrive (Speicherort-Block) und ist mit GhulDrive aus nicht zu sehen. |

## 20. Verschlüsselte Mails (PGP): läuft automatisch

Du brauchst dafür nichts einzurichten. Für P5 bis P9 eine zweite Adresse, deren Programm OpenPGP kann: ein zweites
GhulSuite-Konto (z. B. auf dem zweiten Gerät), Thunderbird (Konto-Optionen → Ende-zu-Ende-Verschlüsselung)
oder Proton.

| Nr. | Schritt | Erwartet |
|---|---|---|
| P1 | Optionen → GhulMail. | Abschnitt „Verschlüsselung (OpenPGP)“ mit dem Fingerabdruck deines Kontos (10 Vierergruppen). |
| P2 | „Meinen öffentlichen Schlüssel speichern (.asc)“. | Datei wird gespeichert; sie beginnt mit `-----BEGIN PGP PUBLIC KEY BLOCK-----`. |
| P3 | Neue Mail an eine Adresse ohne PGP (z. B. Gmail). Empfänger eintragen und kurz warten. | Unter den Empfängern: „Wird signiert, aber nicht verschlüsselt gesendet“ und der Hinweis, warum. |
| P4 | Senden und im Webmail der Gegenseite öffnen. | Mail normal lesbar; zusätzlich ein Anhang `signature.asc` (das ist die digitale Unterschrift). |
| P5 | Von der zweiten PGP-Adresse eine Mail an dich schicken (Thunderbird: signiert). In GhulSuite öffnen. | „Wirklich von …, unverändert.“ Unter Optionen → GhulMail → „Schlüssel von Kontakten“ steht die Adresse „aus einer Mail“. *(Thunderbird schickt den Schlüssel mit, wenn „Autocrypt“ an ist; sonst seine .asc importieren, P10.)* |
| P6 | Dieser Person antworten, Empfänger kurz stehen lassen. | „Wird Ende-zu-Ende verschlüsselt gesendet.“ |
| P7 | Senden; bei der Gegenseite öffnen, dann im Webmail deines Anbieters (Gesendet) ansehen. | Gegenseite kann lesen (Betreff und Text stimmen). Im Webmail: Betreff „...“, Inhalt unlesbar. |
| P8 | Gegenseite antwortet verschlüsselt mit Anhang. In GhulSuite öffnen. | Grünes Schloss „Ende-zu-Ende verschlüsselt“, richtiger Betreff, Text, Anhang speicherbar. In der Liste steht der Betreff erst nach dem Öffnen richtig (außen „...“). |
| P9 | In GhulSuite den Ordner „Gesendet“ öffnen, deine verschlüsselte Mail. | Lesbar (du verschlüsselst immer auch an dich selbst). |
| P10 | Optionen → GhulMail → „Schlüssel eines Kontakts importieren“ → .asc-Datei. | „Schlüssel für … importiert“; in der Liste „importiert“. Mails an diese Adresse werden verschlüsselt. |
| P11 | Bei einem Kontakt „Vergessen“. | Rückfrage; danach wieder „nicht verschlüsselt“ beim Schreiben. |
| P12 | An eine Adresse schreiben, deren Anbieter Schlüssel veröffentlicht (z. B. Proton oder Posteo mit eingerichtetem Schlüssel). | Auch ohne vorherige Mail: „Wird Ende-zu-Ende verschlüsselt gesendet“; in der Kontaktliste „vom Anbieter“. |
| P13 | Fingerabdruck mit der Gegenseite vergleichen (sie liest ihn vor). | Stimmt mit der Anzeige bei dir überein. |
| P14 | Suche nach einem Wort aus einer verschlüsselten Mail. | Wird **nicht** gefunden (der Server kann verschlüsselte Mails nicht durchsuchen), Suche nach dem Absender klappt. *Bekannte Grenze.* |
| P15 | Eine signierte Mail, bei der unterwegs etwas verändert wurde (falls verfügbar, sonst überspringen). | Rote Warnung „Achtung: Die Signatur passt nicht …“. |

## 21. Korrekturen nach dem ersten Test

| Nr. | Schritt | Erwartet |
|---|---|---|
| K1 | GhulDrive öffnen, in einen Unterordner gehen, zu GhulMail und zurück zu GhulDrive wechseln. | GhulDrive steht sofort im selben Unterordner, ohne neu zu laden. Gleiches für GhulMail (Liste und Scrollposition bleiben). |
| K2 | In GhulMail eine Mail öffnen und zurück. | Die Liste ist sofort da (kein erneutes Laden). |
| K3 | In GhulDrive in einem Unterordner die Android-Zurück-Geste. | Geht einen Ordner nach oben, nicht aus der App. |
| K4 | GhulMail offen lassen, von der zweiten Adresse eine Mail schicken. | Taucht nach spätestens etwa einer Minute ohne Antippen in der Liste auf. |
| K5 | Design: alle Bereiche, Unterseiten und Blätter mit der Webvorschau (`public/design/`, auf dem Testserver unter `ghulsuite/design/`) vergleichen, hell und dunkel. | Gleiches Aussehen: Kopfzeile mit farbiger Kontextzeile, Navigationsleiste „Optionen“, Eingabefelder als Karten mit Beschriftung oben, Pillen-Knöpfe, Blätter von unten statt Dialoge („Neuer Eintrag“, „2FA-Konto hinzufügen“, „Zu GhulDrive hinzufügen“, Datei, Teilen, Generator), „Neue Mail“ mit Zeilen Von/An/Betreff und Senden oben, Lesen mit Aktionsleiste unten. |

## 22. Neue Funktionen aus der Vorschau

| Nr. | Schritt | Erwartet |
|---|---|---|
| N1 | Optionen → GhulMail → „Inhalt der Benachrichtigung“: nacheinander „Nur Hinweis“, „Mit Absender“, „Mit Absender und Betreff“ wählen, je eine Mail von der zweiten Adresse schicken. | Benachrichtigung zeigt nur „Neue Mail“ bzw. zusätzlich den Absender bzw. Absender und Betreff. Auf dem Sperrbildschirm immer nur „Neue Mail“. |
| N2 | GhulPass: Eintrag öffnen, Stern antippen. Liste → Chip „Favoriten“. | Eintrag erscheint unter Favoriten; oben in der Liste „Zuletzt verwendet“. |
| N3 | Passwort eines Eintrags zweimal ändern, dann „Mehr“ → „Passwort-Verlauf“. | Alte Passwörter mit Datum, kopierbar. |
| N4 | Eintrag bearbeiten → „Eigenes Feld“ hinzufügen; „Mehr“ → Anhang (Foto/PDF) hinzufügen, öffnen, entfernen. | Feld bleibt nach Speichern; Anhang lässt sich speichern/öffnen und löschen. |
| N5 | Bei einer Anmeldung einen 2FA-Schlüssel eintragen. | Im Eintrag läuft der 6-stellige Code mit Ring. |
| N6 | GhulPass → Karte „Sicherheitscheck“. | Listet doppelte und schwache Passwörter sowie Einträge ohne 2FA; mit eingeschalteter Option auch „in Datenlecks gefunden“ (nur Hash-Anfang wird gesendet). |
| N7 | Generator: „Passphrase“ wählen, Wortzahl/Trennzeichen ändern. | Wörter wie „Kabel-Sonne-…“; Stärkebalken grün. |
| N8 | Eintrag-Menü: Duplizieren, In anderen Tresor verschieben, WLAN-Eintrag → QR. | Kopie „(Kopie)“; Eintrag im Zieltresor; Handy kann per QR ins WLAN. |
| N9 | GhulAuth: Stern, Sortieren (Name/zuletzt), Kachelansicht, „QR aus Bild“, Konto → „Als QR exportieren“. | Favoriten oben; Sortierung bleibt; Bild-QR wird erkannt; zweites Gerät scannt den QR. |
| N10 | GhulDrive: Datei löschen → Ansicht „Papierkorb“ → Wiederherstellen; „Endgültig löschen“; „Papierkorb leeren“. | Datei zurück; danach endgültig weg. |
| N11 | Datei mit gleichem Namen erneut hochladen → Datei → „Versionen“. | Ältere Version wiederherstellbar (bis 5 Versionen). |
| N12 | Datei als Favorit und „Offline verfügbar“; Flugmodus; Ansichten Favoriten/Zuletzt/Offline. | Offline-Datei öffnet sich ohne Netz; Speicherbalken zeigt die Belegung. |
| N13 | Mehrere Dateien wählen → „Als ZIP“ mit Passwort. | ZIP öffnet sich (z. B. 7-Zip) nur mit Passwort. |
| N14 | Drive → „+“ → „Dokument scannen“. Seite fotografieren. | PDF „Scan …“ im aktuellen Ordner, Seite lesbar. *(Nur Android.)* |
| N16 | GhulMail: Postfach-Blatt → „Alle Posteingänge“; Liste zeigt Vorschautext; Unterhaltung mit Antworten. | Mails aller Konten gemischt; Antworten unter einem Eintrag gruppiert. |
| N17 | Mail markieren (Stern), Label vergeben, nach links wischen. | Stern auch im Webmail; Label sichtbar; Wischen führt die gewählte Aktion aus (Optionen → GhulMail). |
| N18 | Neue Mail anfangen, zurück. Postfach → „Entwürfe“. | Entwurf ist da und lässt sich fortsetzen. Adressvorschläge beim Tippen. |
| N19 | „Später senden“ in 2 Minuten. App entsperrt lassen. | Mail geht zur Zeit raus; vorher unter „Geplant“ abbrechbar. *Nur bei entsperrter App.* |
| N20 | Mail → Menü: als .eml, als PDF, in GhulDrive speichern, „Original anzeigen“. | Dateien korrekt (Umlaute im PDF richtig); Kopfzeilen sichtbar. |
| N21 | Optionen → GhulMail → „Offline lesen“ an, Posteingang öffnen, Flugmodus. | Liste und geöffnete Mails bleiben lesbar, Hinweis „offline“. |
| N23 | Optionen → Darstellung → Start-Bereich „Zuletzt geöffnet“ / Bereiche ausblenden. | Startet im gewählten Bereich; ausgeblendete fehlen in der Leiste. |
| N24 | Optionen → Über: Lizenzen, Bibliotheken, „Fehlerbericht“. | „Made by Daniel Erdmann · 2026“, AGPL-3.0 und Liste (inkl. EFF-Wortliste); Fehlerbericht ohne Geheimnisse. |
| N25 | Newsletter (HTML-Mail mit Bildern und Links) öffnen. | Formatiert: Überschriften, fett/kursiv, Listen, Links. Hinweis „x Bild(er) aus dem Internet blockiert“; Bilder in der Mail selbst (z. B. Logo) sichtbar. Keine Skripte, keine Formulare. |
| N26 | „Bilder laden“ bzw. „Immer für diesen Absender“. | Bilder erscheinen; bei „Immer“ auch bei der nächsten Mail dieses Absenders ohne Nachfrage. |
| N27 | Link in einer HTML-Mail antippen. | Rückfrage mit Ziel-Domain und vollständiger Adresse, erst „Öffnen“ öffnet den Browser. |
| N28 | „Als reinen Text anzeigen“ und zurück. | Wechselt zwischen Text- und formatierter Ansicht. |
| N29 | Neue Mail mit Anhang beginnen, zurück, später unter „Entwürfe“ fortsetzen. | Anhang ist noch da. Anhänge über 10 MB zusammen werden im Entwurf nicht gespeichert. |
| N30 | Optionen → Darstellung → Sprache: „English“, dann „Wie das System“. | Die App wechselt sofort die Sprache (Android); zurück auf Systemsprache. |
| N31 | Import einer 1Password-Datei (.1pux oder CSV) bzw. LastPass-CSV. | Anmeldungen, Notizen, Karten und 2FA-Codes übernommen; archivierte Einträge nicht. |
| N32 | Eintrag → Menü → „Eintrag exportieren“ mit Master-Passwort. | CSV nur mit diesem Eintrag; falsches Passwort wird abgelehnt. |

---

## 23. Ein Bereich, eine Datei (#24)

| Nr. | Schritt | Erwartet |
|---|---|---|
| B1 | Optionen → Bereiche verwalten. | Alle fünf Bereiche mit „aktiv“, „hier ausgeblendet“ oder „nicht eingerichtet“ und dem Dateinamen. |
| B2 | Nicht eingerichteten Bereich antippen (z. B. GhulCal) → Einrichten. | Name abgefragt; Datei `….ghlcal` entsteht, Bereich erscheint in der Leiste. Bei GhulDrive: „Einrichten“ öffnet direkt das Server-Formular für den ersten Speicherort. |
| B2a | Bereiche verwalten → GhulMail (nicht eingerichtet) → Einrichten. Danach GhulMail ausblenden und wieder einblenden, solange es kein Konto hat. | Gleich danach öffnet sich „E-Mail-Konto hinzufügen“, ohne Umweg über GhulMail; beim Wieder-Einblenden ohne Konto ebenso. |
| B3 | Bereich → „Auf diesem Gerät ausblenden“. | Verschwindet nur hier aus der Leiste; nach Speichern und Öffnen der Suite-Datei auf dem zweiten Gerät ist er dort weiter sichtbar. Wieder einblenden geht. |
| B4 | Bereich → „Bereich entfernen“. | Rückfrage mit Warnung; danach ist der Bereich weg und „Nicht gespeicherte Änderungen“ erscheint; nach Speichern und Öffnen der Suite-Datei auch auf dem zweiten Gerät. Die Dateien von GhulDrive auf dem Server bleiben, wenn nicht ausdrücklich gelöscht. |
| B5 | Optionen → GhulCal: Bundesland ändern, Suite-Datei speichern, auf dem zweiten Gerät öffnen. | Oben „Für GhulCal · alle Geräte“, unten „Nur auf diesem Gerät“. Das Bundesland ist auf dem zweiten Gerät übernommen, die Startansicht nicht. |
| B6 | Optionen → GhulDrive. | Ein Block je Speicherort: Name, Art, Benutzer@Server:Port, Ordner, „Aktiv“/„Mail-Archiv“, Server-Modul mit Installieren/Aktualisieren/Entfernen, Zahl der Links, roter Papierkorb. Darunter „Speicherort hinzufügen“. |
| B7a | Zwei Speicherorte. GhulDrive öffnen. | In der Speicher-Karte steht der Knopf „Speicherort wechseln“; er listet beide (der aktive markiert). Der andere zeigt seine eigenen Dateien, zurück wieder die ersten. Mit nur einem Speicherort gibt es den Knopf nicht. |
| B7b | Optionen → GhulDrive → Papierkorb an einem Speicherort. | Rückfrage „… entfernen?“ mit Schalter „Auch die Dateien auf dem Server löschen“ und Erklärung, was jeweils passiert. Abbrechen ändert nichts. Ohne Schalter bleiben die Dateien auf dem Server, mit Schalter sind GhulDrive-Ordner, Freigaben und Modul dort weg. Die Freigabelinks des Speicherorts enden in beiden Fällen; war er aktiv, wird der nächste aktiv. |
| B7c | Optionen → GhulMail → Archiv-Speicherort auf einen anderen Speicherort stellen, Mails archivieren. | Die Mails landen im eingestellten Speicherort, auch wenn GhulDrive einen anderen zeigt. „Archiv“ in GhulMail sagt dann, wo das Archiv liegt, und bietet „Zu … wechseln“ an. |
| B8 | GhulDrive ohne Speicher öffnen. | „Noch nicht eingerichtet“ mit „Eigener Server“ direkt im Bereich. |
| B9 | GhulMail ohne Konto öffnen, Anbieter mit eigenem Benutzernamen einrichten. | Vollständiges Formular: Mailadresse, Passwort, Benutzername, IMAP und SMTP getrennt; bekannte Anbieter vorgeschlagen, alles änderbar. |
| B11 | Optionen → Entsperren → Gültigkeit „Immer“, App neu starten, Gerät neu starten. | Nach App-Neustart Fingerabdruck reicht; nach Geräte-Neustart wieder Passwort und Key. |

## 24. Import ohne Verluste, Listen bedienen

| Nr. | Schritt | Erwartet |
|---|---|---|
| I1 | GhulPass → „+“ → „Aus anderer App importieren“. | Der Import öffnet sich direkt (nicht nur über Optionen). |
| I2 | Proton-Pass- oder Proton-Authenticator-Export wählen, in dem Aliase, SSH-Schlüssel, Steam- oder HOTP-Codes vorkommen. | Unter „Nicht umgewandelt (n)“ steht jeder Eintrag mit Namen und Grund. Aliase landen als Login, SSH-Schlüssel und eigene Arten als Notiz mit allen Feldern. |
| I3 | „Als Notiz in GhulPass sichern“ an lassen → Importieren. | Ergebnis direkt auf der Seite (kein Infokasten): Anzahl übernommen, doppelt, als Notiz gesichert, und die Liste. Im Ordner „Import (nicht umgewandelt)“ liegen Notizen mit „Grund“ und „Originaldaten“. |
| I4 | Dieselbe Datei noch einmal importieren. | Nichts doppelt, auch keine doppelten Notizen. |
| I5 | Steam-Code importieren oder in GhulAuth von Hand mit „Steam“ anlegen. | Fünfstelliger Code aus Buchstaben und Ziffern, der zum Steam-Code passt. |
| I6 | Irgendeine Aktion mit Meldung (z. B. Passwort kopieren). | Die Meldung erscheint einmal, dunkel und abgerundet. |
| I7 | GhulPass und GhulAuth: der runde Knopf unten rechts. | Zeigt ein „+“. |
| I8 | GhulPass: Eintrag lange drücken. | Auswahlkopf oben (Anzahl, Alle, Kopieren, Löschen, ⋮ mit Passwort kopieren und Bearbeiten). Ein ⋮ je Zeile gibt es nicht mehr. |
| I9 | In allen fünf Bereichen (Pass, Auth, Drive, Mail, Kalender-Liste) einen Eintrag lange drücken, weitere antippen, „Alle“, Löschen. | Gleicher Auswahlkopf oben überall; Löschen ohne Rückfrage, Meldung mit „Rückgängig“ (ca. 6 s), „Rückgängig“ bringt alle zurück; Zurück beendet die Auswahl. |
| I10 | GhulAuth (Listenansicht): Konto nach rechts wischen, dann nach links. | Rechts: Bearbeiten (Anbieter, Konto). Links: gelöscht, Meldung mit „Rückgängig“. |

## 25. Mail, Optionen und Sofortsperre (zweite Runde)

| Nr. | Schritt | Erwartet |
|---|---|---|
| R1 | GhulMail ohne Konto öffnen. | Karte „GhulMail einrichten“ und Zeile „E-Mail-Konto hinzufügen“, wie bei GhulDrive. Antippen öffnet das Formular in einem Blatt. |
| R2 | Optionen → GhulMail. | Konten als Zeilen (antippen = bearbeiten im Blatt, Papierkorb = entfernen mit Rückfrage), darunter „E-Mail-Konto hinzufügen“. Die Seite bleibt offen. |
| R3 | Kontoauswahl oben in GhulMail → „E-Mail-Konto hinzufügen“. | Dasselbe Blatt. |
| R4 | Neue Suite: im Assistenten bei „Bereiche“ GhulMail einschalten → „E-Mail-Konto hinzufügen“. | Blatt mit dem Formular; danach „E-Mail-Konto eingerichtet“. Nach dem Assistenten ist das Konto in GhulMail da. |
| R5 | Mail lesen. | Unten nur **Antworten** und **Weiterleiten**. „Allen antworten“ steht im ⋮-Menü, wenn die Mail mehrere Empfänger hat. |
| R6 | Mail schreiben, unten „PGP an“ antippen. | Wird „PGP aus“, Hinweis „ohne PGP: nicht signiert, nicht verschlüsselt, ohne Schlüssel“. Beim Empfänger: keine signature.asc, kein Autocrypt-Kopf, Betreff lesbar. |
| R7 | Dasselbe mit „Später senden“. | Die geplante Mail geht ebenfalls ohne PGP raus. |
| R8 | Optionen-Übersicht → „Entsperren“ mit nur Fingerabdruck, nur App-PIN, beidem, nichts. | Untertitel „Fingerabdruck · 24 Stunden“, „App-PIN · …“, „Fingerabdruck und App-PIN · …“ bzw. „Nur Passwort“; bei „Immer“: „ohne Ablauf“. |
| R9 | Optionen → Schlüssel und Schlüsseldateien. | Kein zweites „Master-Passwort ändern“ (das steht als eigene Zeile in der Übersicht). |
| R10 | Android-Einstellungen → Apps → Standard-Apps (je nach Hersteller „E-Mail“, „Kalender“). | GhulSuite ist als Mail- und Kalender-App wählbar. |
| R11 | In einer anderen App (Browser, Kontakte) einen `mailto:`-Link antippen. | Auswahl zeigt GhulSuite; nach dem Entsperren ist eine neue Mail mit Empfänger, Betreff und Text offen. |
| R12 | In der Galerie ein Foto teilen → „Per E-Mail senden (GhulMail)“. | Neue Mail mit dem Foto als Anhang. |
| R13 | Eine `.ics`-Datei (z. B. Einladung im Download-Ordner) mit GhulSuite öffnen. | Nach dem Entsperren: „n Termine importiert“ in den Standardkalender. |
| R14 | In einer App „Zum Kalender hinzufügen“ (z. B. Bahn- oder Kino-App). | Neuer Termin mit Titel, Zeit und Ort, zum Speichern bereit. |
| R15 | Schnell entsperren mit Fingerabdruck an. App gesperrt im Hintergrund lassen, wieder hineinwechseln. | Die Fingerabdruck-Abfrage erscheint **sofort**, ohne den Knopf zu drücken. Abbrechen: sie kommt nicht in Dauerschleife wieder, der Knopf bleibt. |

## 26. Alltagstest (eine Woche, Handy und Windows)

Ziel: GhulSuite eine Woche wie im echten Leben benutzen, mit echten Konten, aber mit einer **Kopie** wichtiger Daten. Jeden Tag kurz notieren, was hakt (Fehler melden siehe unten).

| Tag | Was du tust | Worauf du achtest |
|---|---|---|
| 1 | Neue Suite mit Assistent: alle Bereiche, GhulDrive mit eigenem Server (SFTP), Mailkonto im Assistenten, Fingerabdruck. Passwörter aus deinem alten Manager importieren. Suite-Datei auf den USB-Stick speichern; Windows: diese Datei öffnen. | Alles importiert oder als Notiz behalten? Beide Geräte zeigen dasselbe? |
| 2 | Den Tag über Anmeldedaten aus GhulPass nutzen (kopieren). 2FA-Codes aus GhulAuth nutzen. Abends ein Passwort ändern, Suite-Datei speichern, unter Windows öffnen. | Ist die Änderung auf Windows da? Erinnert der Hinweis „Nicht gespeicherte Änderungen“ zuverlässig ans Speichern? |
| 3 | Mails nur in GhulMail lesen und schreiben, auch mit Anhang und an jemanden mit PGP. Handy zwischendurch neu starten. | Kommt **jede** Mail sofort als Benachrichtigung, auch nach dem Neustart? Öffnet das Antippen die Mail? |
| 4 | Fotos und Dokumente in GhulDrive (auch Ordner, Umbenennen, Papierkorb), eine Datei teilen. Zwischen WLAN und mobilen Daten wechseln. | Uploads brechen nicht ab oder setzen fort; Freigabelink funktioniert. |
| 5 | Termine in GhulCal, auch wiederkehrend und mit Erinnerung; einen davon auf Windows ändern (Suite-Datei hin und zurück). Sofortsperre durch Schütteln testen (in der App und im Hintergrund). | Erinnerung kommt pünktlich; Schütteln sperrt beim ersten Versuch, Mails und Erinnerungen kommen danach weiter. |
| 6 | Beide Geräte am selben Tag: Auf einem arbeiten und speichern, auf dem anderen öffnen und weiterarbeiten; einmal absichtlich ohne vorheriges Öffnen auf dem zweiten Gerät ändern und speichern. GhulDrive auf beiden gleichzeitig benutzen. | Die Rückfragen (älter, abgezweigt, überschreiben) sind verständlich; nichts geht unbemerkt verloren. GhulDrive zeigt auf beiden alles. |
| 7 | Master-Passwort ändern, Suite-Datei speichern (Aufforderung kommt), unter Windows die neue Datei öffnen. Eine alte Kopie der Datei auf einem dritten Gerät öffnen und GhulDrive versuchen. | Schlüsselwechsel läuft ohne Zutun durch (bzw. Anleitung für das Webpanel); die alte Kopie kommt nicht mehr an GhulDrive. |

## 27. Stresstest (von Hand)

| Nr. | Schritt | Erwartet |
|---|---|---|
| S1 | 5.000 Anmeldungen importieren (z. B. große CSV aus einem anderen Passwortmanager). | Import in unter einer Minute; Liste scrollt flüssig; Suche antwortet sofort. |
| S2 | 200 2FA-Codes. | Codes laufen flüssig, keine Ruckler beim Wechsel der 30-Sekunden-Runde. |
| S3 | Ein Video über 500 MB in GhulDrive hochladen, währenddessen App in den Hintergrund und WLAN aus/an. | Upload setzt fort; danach herunterladen, Datei ist unversehrt. |
| S4 | Ein Postfach mit über 10.000 Mails öffnen, schnell scrollen, suchen. | Liste lädt nach, nichts friert ein. |
| S5 | 50-mal schnell hintereinander sperren (Knopf) und entsperren (Fingerabdruck). | Kein Absturz, kein „hängendes“ Entsperren. |
| S6 | Zwei Geräte laden gleichzeitig je 20 Dateien in GhulDrive hoch. | Keine Fehlermeldung, keine fehlenden oder doppelten Dateien (DX2). |
| S7 | Handy mit wenig freiem Speicher (unter 500 MB) benutzen. | Klare Meldung statt Absturz, keine kaputten Dateien. |

Automatisch läuft bei jedem Push der Alltagstest im Kern (zwei Geräte, 30 simulierte Tage mit allen Bereichen) und in einem eigenen Job der Stresstest (5.000 Anmeldungen, 2.000 Termine, 300 Dateien und 64 MB, 8 parallele Threads); dabei wandert die Suite wie auf einem USB-Stick zwischen den Geräten.

## 28. Sicherheitspunkte (Runde 5)

| Nr. | Schritt | Erwartet |
|---|---|---|
| R5 | Android-Einstellung „alle Inhalte anzeigen“, Handy mit PIN sperren; Mail und Termin-Erinnerung kommen lassen. Danach entsperren und noch eine Mail kommen lassen. | Gesperrt eingetroffen: nur die in GhulSuite gewählte Sperrbildschirm-Stufe (Mail: Kontoname und Anzahl; Termin: wie in den Kalender-Optionen gewählt), auch nach dem Entsperren in der Leiste. Entsperrt eingetroffen: mit Details (B4, F43). |
| R6 | Optionen → Sperren → „Sofortsperre stoppt auch Mail-Benachrichtigungen“ an; schütteln; Mail schicken; dann entsperren. | Gesperrt keine Mail-Benachrichtigung; nach dem Entsperren kommen sie wieder (B5). |
| R7 | Eine `.ics`-Datei aus einer anderen App (z. B. Dateien) mit GhulSuite öffnen. | Vorschau mit Anzahl und ersten Terminen, Import erst nach „Importieren“ (B6). |
| R8 | App sperren, Handy 20 Minuten in den Standby (Bildschirm aus), dann App wieder öffnen. | Gesperrt, auch wenn das Handy zwischendurch tief geschlafen hat (B8). |
| R10 | Eine App mit Bildschirm-Overlay (z. B. Blaulichtfilter) einschalten und GhulSuite entsperren. | Tipps werden ignoriert, solange etwas darüberliegt (L10); Overlay aus, dann geht es. |
| R12 | Windows: eine Drive-Datei öffnen, das Programm offen lassen, GhulSuite sperren; Programm schließen, eine Minute warten. | Der Weitergabe-Ordner ist danach leer (B10). Portabel: der Ordner liegt in `GhulSuite-Daten`, nicht im Windows-Temp. |
| R13 | Optionen → Sicherheit: Hinweis „Etwas verloren …“ lesen. | Hinweis nennt: entfernen bzw. neues Notfallblatt, Suite-Datei speichern und alte Kopien ersetzen, Mail-Passwörter wechseln (L1). |
| R14 | GhulMail: bei einem Kontakt mit PGP-Schlüssel „Vergleichen“ öffnen. | Fingerabdruck in Vierergruppen; nach dem Bestätigen gilt er als geprüft; ein später geänderter Schlüssel wird gemeldet (L7). |
| R15 | Drive-Server auf einen alten Stand zurücksetzen (alte Index-Datei zurückkopieren), dann GhulDrive öffnen. | Meldung „Drive auf dem Server ist älter …“, nichts verschwindet; „Älteren Serverstand übernehmen“ (Optionen → GhulDrive) nur nach Rückfrage (B3). |

## 29. Bedienung und Mail (Runde 6)

| Nr. | Schritt | Erwartet |
|---|---|---|
| V1 | Optionen → Bereiche → einen Bereich entfernen; ebenso Optionen → Tresore → einen Tresor löschen. | Dialog nennt die Anzahl der Einträge, bietet „Vorher exportieren“; „Entfernen“ bzw. „Löschen“ geht erst, wenn der Name eingetippt ist (U4). |
| V2 | Etwas Falsches eingeben, das der Kern ablehnt (z. B. IMAP mit Port 143). | Meldung auf Deutsch, kein englischer Rest (U5). |
| V3 | Gesperrt mit vorhandener Suite: „Suite-Datei öffnen“ mit der Datei einer anderen Suite, im Dialog „Abbrechen“. | Rückfrage „… andere Suite … ersetzt alles hier“; nach „Abbrechen“ bleibt die vorhandene Suite unverändert (U6). |
| V5 | Einrichtung: Key wählen, keine PIN eingeben, „Weiter“. | Meldung „Bitte gib die PIN deines Keys ein …“ (U9). |
| V6 | Startbildschirm: „Einzelne Tresordatei öffnen“ mit einer `.ghlcal`-Datei. | Wird als Kalender erkannt, GhulCal ist danach eingeschaltet (U10). |
| V7 | „Mit Wiederherstellungscode öffnen“. | Danach Dialog mit den drei Schritten; „Jetzt erledigen“ öffnet die Keys (U11). |
| V8 | Anmeldung bearbeiten, etwas ändern, Zurück; danach etwas ändern und die App sperren lassen. | Zurück fragt „Änderungen verwerfen?“; nach dem Entsperren die Meldung, dass eine Bearbeitung verworfen wurde (U12). Ebenso im Termin-Editor. |
| V9 | Flugmodus, App entsperren, GhulDrive öffnen. | Verständlicher Hinweis, dass der Server gerade nicht erreichbar ist; Tippen versucht es erneut; mit Netz verschwindet er (U13). |
| V10 | Optionen → GhulDrive → zweiten Speicherort hinzufügen. | Meldung „Speicherort hinzugefügt …“; der aktive bleibt aktiv, GhulDrive zeigt weiter dieselben Dateien. |
| V11 | Kalender ohne Kalender: Import; dann mit zwei Kalendern: Import; Export mit einem ausgeblendeten Kalender. | Meldung „Lege zuerst einen Kalender an“; Wahl des Ziels; Export enthält alle Kalender, Hinweis darunter (U15). |
| V12 | Mail-Ordner mit mehr als 50 Mails: eine lang drücken, „Alle“, dann „Alle N Mails in diesem Ordner auswählen“, „Als gelesen“. | Alle Mails des Ordners gelesen, Meldung mit Anzahl. Ebenso Verschieben, Archivieren, Löschen (Löschen fragt vorher). |
| V13 | Papierkorb bzw. Spam: „Ordner leeren“. | Rückfrage mit Anzahl, danach beim Anbieter leer; in anderen Ordnern gibt es den Knopf nicht. |

## GhulCal (Kalender)

| Nr. | Schritt | Erwartet |
|---|---|---|
| K1 | GhulCal zum ersten Mal öffnen. | Ein Kalender-Tresor „Kalender“ (`.ghlcal`) mit Privat, Arbeit, Familie, Sport, Geburtstage entsteht; Monatsansicht mit KW, heute rot eingekreist, Feiertage (NRW). |
| K2 | „+ Termin“ → „Zahnarzt Fr 10 Uhr #privat“ eintippen. | Vorschau zeigt Titel, Freitag, 10:00 bis 11:00, Kalender Privat; „Eintragen“ legt ihn an, Erinnerung nach Standard. |
| K3 | „Team-Call jeden Montag 9 Uhr #arbeit“ eintragen, Woche und Liste ansehen. | Serie jeden Montag; Wiederholungs-Symbol in der Liste. |
| K4 | Monat/Woche/Tag/Liste umschalten, wischen, ‹ › und „12“-Symbol (Heute). | Ansichten wie in der Design-Vorschau; Tag-Ansicht mit Jetzt-Linie; Tippen auf eine leere Stunde legt dort einen Termin an. |
| K5 | Termin öffnen → Bearbeiten: ganztägig, Datum/Uhrzeit, Wiederholen, zwei Erinnerungen, Ort, Notiz, Verknüpfung. | Alles wird gespeichert; Detailansicht zeigt Ort („In Karten-App öffnen“), Erinnerungen, Notiz, Verknüpfung. |
| K6 | Serientermin → Menü → Löschen: „Nur diesen“, dann „Diesen und alle folgenden“. | Erst fehlt nur der eine Tag, dann endet die Serie davor. |
| K7 | „Alle Kalender“ → Kalender aus-/einblenden, Farbe ändern, neuen Kalender anlegen, löschen. | Ansicht folgt sofort; Kopfzeile „x von y Kalendern“. |
| K8 | Import einer `.ics` aus Google/Outlook, Export „Alle“ und „Als .ics teilen“. | Termine samt Serien und Erinnerungen übernommen; zweiter Import legt keine Doppelten an; Export öffnet sich in anderen Kalendern. |
| K9 | Termin mit Erinnerung in 2 Minuten anlegen, App sperren (Android: auch schließen). | Benachrichtigung pünktlich; auf dem Sperrbildschirm nur die gewählte Detailstufe; „In 5 Min. erneut“, „Erledigt“; Tippen öffnet nach dem Entsperren den Termin. |
| K10 | Optionen → GhulCal: Detailstufe „Nur Hinweis“ → „Test-Erinnerung zeigen“. | Auf dem Sperrbildschirm nur „Termin in 15 Min.“ ohne Titel. |
| K11 | Sofortsperre auslösen. | Erinnerungen kommen weiter (die Sofortsperre sperrt nur die App). |
| K12 | Termin ändern, Suite-Datei speichern, auf dem zweiten Gerät öffnen. | Der geänderte Termin ist dort. Hatte das zweite Gerät vorher selbst etwas ohne Speichern geändert, warnt die Rückfrage (SD10), dass das verloren geht. |
| K13 | Mail-Benachrichtigung antippen (gesperrt und entsperrt). | Die neue Mail öffnet sich (nach dem Entsperren); Leiste und App-Symbol zeigen die Zahl ungelesener Mails, vollständig, nicht angeschnitten, ab 100 „99+“. Windows-Taskleiste: Briefumschlag mit Zahl. |
| K14 | Eine `.ics` importieren, die Zeiten in UTC (`…Z`, z. B. Google-Export) und in einer anderen Zeitzone (`TZID=America/New_York`) enthält, je ein Termin im Sommer und im Winter. | Alle Termine stehen auf der Stunde, zu der sie hier stattfinden (10:00 New York = 16:00 in Deutschland), unabhängig davon, ob gerade Sommer- oder Winterzeit ist. Ganztägige Termine bleiben auf ihrem Tag. |
| K15 | Termin mit Erinnerung anlegen, App schließen, in den Systemeinstellungen die Zeitzone ändern (z. B. London). | Die Erinnerung kommt zur selben Uhrzeit der neuen Zone (Termin 10:00 → Erinnerung 09:45 Londoner Zeit), ohne dass die App geöffnet wurde. |
| K16 | Eine Serie aus einer anderen Zeitzone importieren (z. B. Einladung „jeden Dienstag 10:00 New York“ aus Outlook oder Google) und durch das Jahr blättern: Januar, Mitte März, April, Ende Oktober, November. | In Deutschland steht der Termin um 16:00, in den Wochen, in denen nur die USA oder nur Europa Sommerzeit haben, um 15:00. Die Termin-Ansicht zeigt zusätzlich „Ortszeit in New York: 10:00 - 11:00“; beim Bearbeiten steht unter „Zeitzone“ der Ort. „Nur diesen Termin löschen“ entfernt genau den angezeigten Tag. |
| K17 | Einen einzelnen Eintrag löschen: GhulPass-Eintrag, GhulAuth-Konto, Drive-Datei, Mail, Termin. Einmal „Rückgängig“ tippen, einmal die Meldung ablaufen lassen; einmal löschen und sofort sperren. | Erst eine Rückfrage mit dem Namen („‚Codeberg‘ wird gelöscht.“); „Abbrechen“ ändert nichts. Bestätigt: weg, Meldung mit „Rückgängig“; auch ein Wisch nach links fragt erst. Rückgängig: wieder da, nichts passiert. Abgelaufen: endgültig gelöscht (nach Sperren/Entsperren weiterhin weg). Sofort gesperrt: die Löschung wird vor dem Sperren ausgeführt. Sofortsperre direkt nach dem Löschen: der Eintrag bleibt. |
| K17a | In GhulPass ins Suchfeld tippen (Tastatur auf), Tastatur schließen, zu GhulAuth wechseln, ein Konto löschen und die Rückfrage bestätigen. | Die Tastatur bleibt zu; „1 gelöscht“ mit „Rückgängig“ ist sichtbar. |
| K18 | Ganzes löschen: Tresor, Bereich, Mailkonto, Server, Papierkorb leeren, mehrere endgültig löschen. | Immer eine Rückfrage mit rotem Knopf, Warnung als Hinweisfeld wie bei „Bereich entfernen“. |
| K19 | Breites Fenster (Windows oder Handy/Tablet quer): Termin öffnen, Termin bearbeiten, in GhulAuth „Konto hinzufügen“. | Links die Seitenleiste mit den Bereichen, daneben eine Spalte in der Mitte; Geöffnetes steht an der Stelle der Liste, „Zurück“ führt zur Liste. Keine leere dritte Spalte. |
| K20 | Verschlüsselte Mail in der Liste. | Schloss und „Verschlüsselte Nachricht“ an der Stelle des Betreffs (kein „...“); geöffnet Betreff und Text. |

## 30. Updates

Vorher: eine Version aus den Releases (nicht die Testversion aus den Artefakten), installiert per APK-Datei.

| Nr. | Schritt | Erwartet |
|---|---|---|
| U1 | Optionen → Über → Updates. | Schalter „Automatisch nach Updates suchen“ (an), Kanal „Stabil“ (bei einer Vorabversion „Vorab“), Knopf „Jetzt nach Updates suchen“. In der Testversion steht nur der Hinweis, dass sie sich nicht selbst aktualisiert. |
| U2 | „Jetzt nach Updates suchen“ auf der neuesten Version. | „GhulSuite ist aktuell (…)“. |
| U3 | Ältere stabile Version installieren, „Jetzt nach Updates suchen“. | Dialog „Update auf X“ mit Größe und Änderungen; „Installieren“ fragt beim ersten Mal nach „Unbekannte Apps installieren“, danach der Dialog des Systems; nach der Installation ist die Suite noch da und entsperrbar. |
| U4 | Kanal „Vorab“, suchen. | Bietet die neueste Vorabversion an („Vorabversion 0.x.y-dev.n“), wenn es eine neuere als die installierte gibt. |
| U5 | Zurück auf „Stabil“ mit installierter Vorabversion. | Kein Angebot, bis eine neuere stabile Version erscheint; keine Rückstufung. |
| U6 | Flugmodus, „Jetzt nach Updates suchen“. | Verständliche Netzwerk-Fehlermeldung, sonst nichts. |
| U7 | Automatische Prüfung: App entsperren, am nächsten Tag noch einmal. | Höchstens einmal am Tag ein Dialog; „Später“ schließt ihn ohne Folgen. |
| U8 | Windows, installierte Version (MSI): Update installieren. | App schließt sich, der Installer läuft ohne Fragen, GhulSuite startet neu in der neuen Version; Daten bleiben. |
| U9 | Windows portabel: Update. | Es öffnet sich nur die Release-Seite; auf dem Stick wird nichts verändert. |

## 31. Über, Website und Fehlerbericht

| Nr. | Schritt | Erwartet |
|---|---|---|
| W1 | Optionen → Über: „Website“, „Impressum“, „Datenschutzerklärung“ antippen. | Die jeweilige Seite von www.ghulsuite.de öffnet sich im Browser, in der Sprache der App. Die Zeilen haben Innenabstand wie alle Listen. |
| W2 | Fehlerbericht öffnen, Notiz und E-Mail eintragen, „Senden“. | „Wird gesendet…“, nach wenigen Sekunden „Danke! Dein Bericht ist angekommen.“; beim Entwickler kommt eine Mail mit Notiz, Adresse und Bericht an. |
| W3 | Ungültige E-Mail-Adresse (z. B. „abc“), „Senden“. | Hinweis, dass die Adresse nicht gültig aussieht; nichts wird gesendet. |
| W4 | Flugmodus, „Senden“. | Verständliche Fehlermeldung; „Kopieren“ und „Speichern“ funktionieren weiter. |
| W5 | Sechs Berichte hintereinander senden. | Der sechste innerhalb einer Stunde wird mit „Gerade zu viele Berichte von hier“ abgelehnt. |
| W6 | Optionen und eine Unterseite (z. B. Über) öffnen. | Zwischen Kopfleiste und Inhalt kein zusätzlicher leerer Streifen. |

## 32. Server, Teilen und Anzeige (Runde 7)

Diese Punkte prüft auch der Emulator-Ablauf `app/maestro/server/3-servers-share.yaml` mit echtem SFTP-Server, mit einem Bild nach jedem Schritt.

| Nr. | Schritt | Erwartet |
|---|---|---|
| R1 | Server hinzufügen (SFTP): Zugangsdaten eingeben, ohne zu testen nach einem Ordner suchen. | Es gibt keine Ordnerwahl. Erst „Verbindung testen“; nach Erfolg „Verbindung klappt …“ und die Ordnerliste öffnet sich. Nach der Ordnerwahl „Verbindung prüfen“ (Anmeldung und Ordner beschreibbar); erst danach „Server speichern“. Eine geänderte Adresse, ein geänderter Benutzer, ein geändertes Passwort oder ein anderer Ordner verlangen eine neue Prüfung; nichts wird von selbst gespeichert. |
| R2 | Server mit nicht erreichbarer Adresse, „Verbindung testen“. | Die Fehlermeldung steht im Formular; keine Ordnerliste öffnet sich. |
| R3 | Server-Formular, Liste der Schritte unten ansehen. | Schritte 1. bis 4. untereinander, jeder in einer Zeile, nichts zerhackt. |
| R5 | Server-Modul installieren: im Ordner-Browser schnell hintereinander mehrere Ordner antippen (auch „..“). | Nie eine Meldung wie „The coroutine scope left the composition“; die Liste zeigt den zuletzt gewählten Ordner. |
| R6 | GhulDrive mit eigenem Server: Reiter GhulDrive. | Die Karte oben nennt den Server mit Ordner und „verschlüsselt“, dazu Dateien und Größe; kein „…“ und kein „Nicht eingerichtet“. Ohne Server: „Noch kein Server“ und „Server wählen“. |
| R7 | In einer Liste alles auswählen (oder das einzige Element). | Das Symbol „Alle“ verschwindet, sobald alles ausgewählt ist. |
| R9 | Handy auf große Schrift stellen (Einstellungen → Anzeige → Schriftgröße, größte Stufe) und alle Bereiche und Optionsseiten durchgehen. | Kein Wort ist mitten im Wort getrennt; zu lange Knopfpaare stehen untereinander, Einstellungswerte unter ihrer Beschriftung; Titel höchstens zweizeilig. |
| R10 | Auf „Speichern“, „Senden“ oder „Erstellen“ schnell zweimal tippen. | Der Eintrag, die Mail oder der Ordner entsteht genau einmal. |
| R8 | Etwas Langsames starten (z. B. Server speichern) und währenddessen auf Knöpfe tippen, auch in einem offenen Blatt. | Nach kurzer Zeit liegt „Arbeite …“ mit Kreisel über dem Bildschirm bzw. dem Blatt; Tippen dahinter bewirkt nichts. |

## 33. Aufgaben im Hintergrund

Lange Arbeiten (Hochladen, Herunterladen, Mail-Archiv) laufen im Hintergrund weiter, auch wenn du den Bereich verlässt. Ein Streifen unter der Kopfleiste zeigt, was läuft, wie weit es ist, und „Abbrechen“; am Ende sagt eine Meldung, wie es ausging.

| Nr. | Schritt | Erwartet |
|---|---|---|
| BG1 | In GhulDrive drei große Dateien hochladen, dann zu GhulMail wechseln und eine Mail öffnen. | Der Streifen „Lade 3 Dateien hoch“ bleibt unter jeder Kopfleiste sichtbar, mit Balken und „1 von 3 fertig“; der Upload läuft weiter. Am Ende „3 Dateien hochgeladen.“, in GhulDrive stehen alle drei. |
| BG2 | Während BG1 im Streifen „Abbrechen“. | Sofort Schluss; „Hochladen abgebrochen: 1 von 3 Dateien fertig.“; die fertigen Dateien sind da, keine halbe. |
| BG3 | Upload starten, App in den Hintergrund (Home-Taste), Benachrichtigungen aufziehen. | Die Benachrichtigung „GhulSuite ist entsperrt“ zeigt jetzt den Auftrag mit Balken, „Abbrechen“ und „Sperren“. „Abbrechen“ dort stoppt ihn. Auf dem Sperrbildschirm steht nur „GhulSuite ist entsperrt“ (keine Dateinamen). |
| BG4 | Upload starten, App in den Hintergrund, länger warten als die Zeit für automatisches Sperren. | Die App sperrt nicht mitten im Upload, sondern erst, wenn er fertig ist; die Meldung „… hochgeladen.“ kommt erst nach dem Entsperren. Kommt zwei Minuten lang kein Fortschritt (z. B. WLAN aus), sperrt sie trotzdem und bricht den Upload ab. |
| BG4a | Automatisches Mail-Archiv läuft (Streifen), App in den Hintergrund. | Die App sperrt nach der üblichen Zeit; der leise Auftrag stoppt und macht beim nächsten Entsperren an seiner Stelle weiter. |
| BG5 | Wie BG4, aber vor dem Ende zurück in die App. | Die App bleibt entsperrt; der Upload läuft zu Ende. |
| BG6 | Upload starten, dann von Hand sperren (Schloss, Schütteln oder „Sperren“ in der Benachrichtigung). | Sofort gesperrt, der Upload ist gestoppt. Nach dem Entsperren: „GhulSuite wurde gesperrt. Hochladen abgebrochen: …“. |
| BG7 | Auftrag läuft, App im Hintergrund, Auftrag endet. | Eine kurze Benachrichtigung mit dem Ergebnis (Android 13+: nur wenn Benachrichtigungen erlaubt sind); auf dem Sperrbildschirm nur „Hochladen fertig“ ohne Dateinamen. Ist die App vorne, kommt nur die Meldung in der App. |
| BG8 | Mehrere Aufträge nacheinander starten (z. B. Upload und Mail-Archiv). | Der zweite zeigt „Wartet auf die Aufgabe davor …“ und startet danach; beide sind im Streifen. |
| BG11 | Eine große Datei aus GhulDrive herunterladen (Speicherort wählen), im Streifen „Abbrechen“; dasselbe mit abgeschaltetem WLAN mitten im Download. | Beide Male bleibt keine halbe Datei am gewählten Ort liegen; ein vollständiger Download bleibt. |
| BG12 | Nur Android 11 oder älter: Upload starten, auf der Startseite „Zurück“. | Die App geht in den Hintergrund, der Upload läuft weiter (Streifen und Benachrichtigung). Ohne laufenden Auftrag schließt „Zurück“ die App wie gewohnt. |

## 34. Mail-Archiv in GhulDrive

Archivieren heißt: Mails werden als .eml-Datei nach GhulDrive kopiert (`Mail-Archiv/<Konto>/<Jahr>/`). Beim Anbieter ändert sich nichts, dort wird nichts gelöscht. Mails, die schon im Archiv sind, werden übersprungen. Diese Punkte prüft zum Teil auch `app/maestro/server/2-mail.yaml` mit echtem IMAP und SFTP.

| Nr. | Schritt | Erwartet |
|---|---|---|
| MA1 | GhulMail, eine Mail lange drücken, weitere antippen, „Mehr“ → „In GhulDrive archivieren“. | Frage „In GhulDrive archivieren?“ mit „3 gewählte Mails werden kopiert.“ und dem Hinweis, dass die Mails beim Anbieter bleiben. |
| MA2 | „Archivieren“. | Streifen „3 Mails werden archiviert“ mit Fortschritt; am Ende „3 Mails in GhulDrive archiviert.“. In GhulDrive: `Mail-Archiv/<Konto>/<Jahr>/` mit Dateien „Datum Absender - Betreff (Code).eml“. Im Webmail sind die Mails unverändert (auch „gelesen“ nicht geändert). |
| MA3 | Dieselben Mails noch einmal archivieren. | „0 Mails in GhulDrive archiviert. 3 waren schon im Archiv.“; keine doppelten Dateien. |
| MA4 | Kopfleiste „⋮“ (Ordner-Aktionen) → „Diesen Ordner in GhulDrive archivieren“ bei einem großen Ordner, nach einigen Mails im Streifen „Abbrechen“. | „Archivieren abgebrochen: 12 von 400 Mails fertig.“; das Archiv enthält genau die fertigen. |
| MA5 | „⋮“ → „Archivieren, was älter ist als …“ → „1 Jahr“. | Frage nennt Ordner und „1 Jahr“; archiviert werden nur Mails, die älter als ein Jahr sind. |
| MA6 | Ohne eingerichtetes GhulDrive „In GhulDrive archivieren“. | Statt der Frage: „GhulDrive nötig“ mit „GhulDrive einrichten“; das führt zur Einrichtung von GhulDrive. Nichts wird archiviert. |
| MA7 | Postfach-Auswahl (Zeile unter „GhulMail“) öffnen. | Unter den Ordnern „Archiv (GhulDrive)“, sobald etwas archiviert ist. Antippen: Jahre als Zeilen, darin die Mails mit Datum, Absender und Betreff; andere Dateien im Ordner erscheinen nicht, eigene Unterordner schon. |
| MA8 | Archivierte Mail antippen. | Sie öffnet sich wie eine normale Mail (Schlagwort „GhulDrive“), auch verschlüsselte Mails sind lesbar. Anhänge speichern, „Antworten“, „Weiterleiten“, „⋮“ → „Allen antworten“, „Als Datei speichern“ und „Als PDF speichern“ funktionieren. |
| MA9 | In der archivierten Mail „Löschen“. | Frage „In den Papierkorb?“ mit dem Hinweis, dass sie in den Papierkorb von GhulDrive kommt und die Mail beim Anbieter bleibt; danach „Rückgängig“ möglich. |
| MA10 | In GhulDrive eine beliebige Datei in einen Jahresordner legen und `kaputt.eml` nennen, im Archiv antippen. | „Lässt sich nicht als Mail lesen.“ mit „Als Datei speichern“; die Zeile sagt es danach auch. |
| MA11 | Optionen → GhulMail → „Automatisch archivieren“ → „Älter als 30 Tage“. | Erklärung: kopiert Posteingang und Gesendet, einmal am Tag nach dem Entsperren, beim Anbieter wird nichts gelöscht. Es startet gleich ein leiser Lauf (Streifen „Automatisches Mail-Archiv“), Meldung nur, wenn etwas archiviert wurde. |
| MA12 | Sperren und am selben Tag wieder entsperren. | Kein zweiter Lauf. Am nächsten Tag nach dem Entsperren läuft er wieder, nur mit neuen alten Mails. |
| MA13 | Ohne GhulDrive die Mail-Optionen öffnen. | „Automatisch archivieren“ lässt sich nicht einschalten; Hinweis „Das Archiv braucht GhulDrive“ mit „GhulDrive einrichten“. |
| MA14 | Suite-Datei speichern und auf dem zweiten Gerät öffnen. | Die Einstellung „Automatisch archivieren“ ist dort gleich; das Archiv ist in GhulDrive sichtbar. |

## 35. Suite-Datei: speichern und öffnen

Die Suite wandert als **eine Datei** (`.ghlzip`) zwischen den Geräten, z. B. auf dem USB-Stick. Abgeglichen wird
nichts: Aktuell ist der Stand, der zuletzt gespeichert wurde. GhulDrive liegt auf dem Server und ist überall gleich.
Gerät A und Gerät B sind zwei Geräte mit GhulSuite (Android oder Windows). Diese Abläufe prüft auch
`app/maestro/14-suite-file.yaml`.

| Nr. | Schritt | Erwartet |
|---|---|---|
| SD1 | Gerät A: Optionen → Suite-Datei. | Seite mit „Speichern“, „Speichern unter …“, „Andere Suite-Datei öffnen“ und dem Stand („Noch nicht als Datei gespeichert“ bzw. „Suite-Datei: …“, „Alles ist in der Suite-Datei gespeichert.“). Kurze Erklärung, dass die Datei alle Tresore verschlüsselt und versiegelt enthält. |
| SD2 | Gerät A: Einen Eintrag ändern. | Auf den Bereichsseiten oben „Nicht gespeicherte Änderungen. Tippen zum Speichern.“; in der Optionen-Übersicht unter „Suite-Datei“ ebenfalls. Antippen führt zur Seite Suite-Datei. |
| SD3 | „Speichern“ (die Datei liegt auf dem Stick). | „Gespeichert in …“; der Hinweis aus SD2 verschwindet; „Alles ist in der Suite-Datei gespeichert.“ |
| SD4 | Stick abziehen, etwas ändern, „Speichern“. | „Die Suite-Datei ist gerade nicht erreichbar (ist der USB-Stick eingesteckt?) …“; die Änderung bleibt als nicht gespeichert markiert. Stick wieder einstecken, „Speichern“: klappt. |
| SD5 | Gerät B (ohne Suite): Startbildschirm → „Suite-Datei öffnen“ → die Datei vom Stick → Passwort und Key. | „… wird zuerst vollständig geprüft. Erst danach ersetzt sie den Stand auf diesem Gerät.“ Danach ohne Rückfrage geöffnet; alle Einträge, Server und Einstellungen sind da; GhulDrive zeigt dieselben Dateien wie auf A. |
| SD6 | Gerät B: etwas ändern, speichern. Gerät A: „Andere Suite-Datei öffnen“ → dieselbe Datei. | Öffnet ohne Rückfrage (neuerer Stand derselben Suite); die Änderung von B ist auf A. |
| SD7 | **Älterer Stand:** Vor SD6 eine Kopie der Datei aufheben. Gerät A (hat jetzt den neueren Stand) öffnet die alte Kopie. | Rückfrage „Diese Datei öffnen?“ mit „Diese Datei ist älter als dein Stand hier.“ „Abbrechen“ ändert nichts; „Öffnen“ lädt den alten Stand. |
| SD8 | **Abzweig:** A und B öffnen denselben Stand. A ändert und speichert unter neuem Namen, B ändert und speichert unter einem anderen Namen. A öffnet die Datei von B. | „Diese Datei wurde von einem anderen Stand aus weiterbearbeitet als deinem hier.“ Nichts wird zusammengeführt: Nach „Öffnen“ gilt nur der Stand von B. |
| SD9 | **Andere Suite:** Auf A die Datei einer anderen Suite (z. B. aus E18) öffnen. | „Diese Datei enthält eine andere Suite als die auf diesem Gerät. Sie ersetzt alles hier.“ Passwort und Key der anderen Suite werden verlangt. |
| SD10 | **Nicht gespeicherte Änderungen:** Auf A etwas ändern (nicht speichern), dann eine Suite-Datei öffnen. | Rückfrage nennt zusätzlich: „Auf diesem Gerät gibt es Änderungen, die noch in keiner Suite-Datei sind. Sie gehen verloren. Speichere vorher, wenn du sie behalten willst. GhulDrive-Dateien, Freigabelinks und gesendete Mails liegen auf dem Server und bleiben.“ Abbrechen, speichern, dann öffnen: keine Rückfrage mehr dazu. |
| SD11 | Mehrere Gründe auf einmal (z. B. ältere Datei und nicht gespeicherte Änderungen). | Alle Gründe stehen in **einem** Dialog. |
| SD12 | **Überschreiben:** A und B öffnen dieselbe Datei. B ändert und speichert. A ändert und tippt „Speichern“ auf dieselbe Datei. | „Die Datei wurde inzwischen anderswo gespeichert. Überschreiben (die Änderungen von dort gehen verloren) oder unter neuem Namen speichern?“ „Speichern unter …“ legt eine zweite Datei an; „Überschreiben“ ersetzt sie. |
| SD13 | „Speichern unter …“ auf eine Datei, die eine andere Suite oder etwas ganz anderes (z. B. ein Foto, umbenannt in `.ghlzip`) ist. | „An diesem Ort liegt eine andere Suite oder keine GhulSuite-Datei. Überschreiben oder unter neuem Namen speichern?“ |
| SD14 | **Älterer Schutz:** Datei speichern, Kopie aufheben. Dann Passwort ändern und neu speichern. Auf demselben Gerät die alte Kopie öffnen (mit dem alten Passwort). | Wird **nicht** geöffnet: „Diese Suite-Datei ist durch eine neuere mit geändertem Schutz … ersetzt und nicht mehr gültig. Öffne die neueste Datei.“ |
| SD15 | Wie SD14, aber die alte Kopie auf einem Gerät öffnen, das den neuen Stand nie gesehen hat. | Öffnet sich (mit dem alten Passwort); nach dem Schlüsselwechsel kommt dieses Gerät aber nicht an GhulDrive und den Server (KW9). |
| SD16 | Eine Suite-Datei mit einem ZIP-Programm verändern: eine Tresordatei entfernen, eine fremde hinzufügen, eine Datei durch die aus einer älteren Kopie ersetzen oder einen Ordner hinzufügen. Dann öffnen. | Jedes Mal wird nichts geladen, und die App sagt, dass die Datei nicht stimmt. Der bisherige Stand auf dem Gerät bleibt unverändert. |
| SD17 | Eine beliebige andere Datei (Foto, normale ZIP) als Suite-Datei öffnen. | „Das ist keine GhulSuite-Datei.“ |
| SD18 | **Mit dem Wiederherstellungscode:** „Suite-Datei öffnen“ → „Passwort vergessen? Mit dem Wiederherstellungscode öffnen“ → Code und neues Passwort. | Hinweis, dass die Datei ohne Passwort nicht geprüft werden konnte; danach offen mit dem neuen Passwort, Hinweis „Nicht gespeicherte Änderungen“ (alles wurde neu versiegelt). Ältere Datei, Abzweig und älterer Schutz werden trotzdem erkannt wie in SD7 bis SD14. |
| SD19 | **Gerätedaten:** Schnell-Entsperren (Fingerabdruck) einrichten, Mail offline lesen lassen, eine Drive-Datei „Offline verfügbar“ machen. Dann eine Suite-Datei öffnen. | Danach braucht das Entsperren wieder Passwort und Key (Schnell-Entsperren ist gelöscht), Mail-Zwischenspeicher und Drive-Offline-Kopien sind weg (Drive-Dateien selbst sind weiter auf dem Server). |
| SD20 | Während des Öffnens (nach der Prüfung, beim Ersetzen) die App über die Liste der letzten Apps beenden; neu starten. | Entweder der alte oder der neue Stand ist vollständig da, nie eine Mischung. |
| SD21 | Eine Suite mit allen fünf Bereichen speichern, auf dem anderen Gerät öffnen. | Alle Bereiche, Tresornamen, Bereichs-Einstellungen (z. B. Bundesland in GhulCal), Mail-Konten und Speicherorte sind da; was nur ein Gerät betrifft (Sperren, Schnell-Entsperren, Ansicht), nicht. |

## 36. GhulDrive auf dem Server: zwei Geräte, Index

GhulDrive liegt nur auf dem Server (Index `drive/index.ghldrv`, voriger Stand `drive/index.bak`, Sperre
`drive/index.lock`, Blöcke). Es ist nicht Teil der Suite-Datei: Jede Kopie der Suite-Datei seit der letzten
Schutzänderung sieht denselben Stand.

| Nr. | Schritt | Erwartet |
|---|---|---|
| DX1 | Gerät A lädt eine Datei hoch. Gerät B (mit einer Suite-Datei von **vor** dem Hochladen) öffnet GhulDrive und aktualisiert. | Die neue Datei ist auf B zu sehen, ohne dass die Suite-Datei neu gespeichert wurde. |
| DX2 | A und B laden **gleichzeitig** je mehrere Dateien in denselben Ordner hoch (beide starten innerhalb weniger Sekunden). | Am Ende sind alle Dateien beider Geräte da; keine fehlt, keine doppelt. Ein Gerät wartet kurz, während das andere den Index schreibt. |
| DX3 | A benennt eine Datei um, B verschiebt gleichzeitig eine andere; dann beide aktualisieren. | Beide Änderungen sind auf beiden Geräten zu sehen. |
| DX4 | Während A hochlädt, auf dem Server `drive/` ansehen. | Kurz liegt `index.lock` dort (Zufallskennung und Zeit), danach ist sie weg. Neben `index.ghldrv` liegt `index.bak`. |
| DX5 | Eine Sperre von Hand anlegen, im Drive-Ordner auf dem Server: erst eine verwaiste (`echo "test $(( $(date +%s%3N) - 600000 ))" > index.lock`, zehn Minuten alt), dann in GhulDrive einen Ordner anlegen. Danach eine frische (`echo "test $(date +%s%3N)" > index.lock`) und wieder etwas ändern. | Verwaiste Sperre: wird übernommen, die Änderung klappt, die Sperre ist danach weg. Frische Sperre: die App wartet höchstens 30 Sekunden und meldet dann einen verständlichen Fehler; die fremde Sperre bleibt liegen, nichts geht verloren. |
| DX6 | **Älterer Index:** Auf dem Server eine alte Kopie von `index.ghldrv` zurückspielen (vorher kopieren, dann etwas hochladen, dann die Kopie zurück). GhulDrive öffnen. | Meldung „Der GhulDrive-Stand auf dem Server ist älter als der zuletzt gesehene …“. Nichts verschwindet, nichts wird geschrieben; Hochladen ist bis zur Entscheidung nicht möglich. |
| DX7 | Nach DX6: Optionen → GhulDrive → „Älteren Serverstand übernehmen“. | Rückfrage; danach zeigt GhulDrive den Stand des Servers; Dateien, die seitdem dazukamen, fehlen in der Liste. Änderungen gehen wieder. |
| DX8 | **Beschädigter Index:** `index.ghldrv` auf dem Server mit Unsinn überschreiben (z. B. ein paar Bytes ändern). GhulDrive öffnen. | GhulDrive zeigt den Stand aus `index.bak` und meldet einen älteren Stand; es wird nichts geschrieben und nichts aufgeräumt, bis der Stand übernommen wird (wie DX7). |
| DX9 | **Fehlender Index:** `index.ghldrv` und `index.bak` auf dem Server löschen (die Blöcke bleiben). GhulDrive öffnen. | Keine leere Liste ohne Warnung: Meldung wie in DX6; erst nach „Älteren Serverstand übernehmen“ beginnt GhulDrive leer. |
| DX10 | **Aufräumen:** Ein Hochladen mitten abbrechen (App beenden). Am nächsten Tag (oder nach über 24 Stunden) GhulDrive öffnen. | Blöcke des abgebrochenen Uploads, die in keinem Eintrag stehen, sind nach dem automatischen Aufräumen verschwunden (Anzahl der Blöcke auf dem Server zählen). Blöcke von Dateien, Papierkorb, Versionen und Freigabelinks bleiben. |
| DX11 | Hochladen einer großen Datei abbrechen (WLAN aus), WLAN wieder an, dieselbe Datei erneut hochladen. | Die schon vorhandenen Blöcke werden übersprungen (das Hochladen geht deutlich schneller weiter). |

## 37. Schlüsselwechsel nach einer Schutzänderung

Nach einer Schutzänderung (neues Passwort, neuer Wiederherstellungscode, Key oder Schlüsseldatei entfernt; **nicht**
beim Hinzufügen) bekommen GhulDrive einen neuen Drive-Schlüssel und die Suite einen neuen SSH-Schlüssel. Der alte
SSH-Schlüssel wird auf jedem Server entfernt. Danach kommt eine ältere Kopie der Suite-Datei weder an GhulDrive noch an
die Server. Vorher: GhulDrive auf einem Server (SFTP) mit einigen Dateien, Suite-Datei gespeichert, eine Kopie davon
aufgehoben, `~/.ssh/authorized_keys` auf dem Server angesehen.

| Nr. | Schritt | Erwartet |
|---|---|---|
| KW1 | **Automatisch:** Server, der `~/.ssh/authorized_keys` liest. Passwort ändern (Optionen → Passwort), im Dialog „Suite-Datei jetzt speichern“ speichern. | Erst nach dem Speichern und ohne weitere Fragen: In `authorized_keys` steht jetzt eine neue Zeile `ssh-ed25519 … GhulSuite`, die alte Zeile der Suite ist weg, **alle anderen Zeilen sind unverändert**. GhulDrive zeigt alle Dateien wie vorher. |
| KW2 | Nach KW1 auf dem Server `drive/` ansehen. | `index.ghldrv` wurde neu geschrieben (neue Änderungszeit); die Blöcke sind unverändert (gleiche Namen und Anzahl). Eine `index.bak` mit dem alten Schlüssel ist weg. |
| KW3 | Wie KW1 mit „Neuer Wiederherstellungscode“ und mit „Schlüsseldatei entfernen“. | Jedes Mal ein neuer SSH-Schlüssel und neuer Drive-Schlüssel wie in KW1 und KW2. |
| KW4 | Einen Key oder eine Schlüsseldatei **hinzufügen**. | **Kein** Schlüsselwechsel: `authorized_keys` und `index.ghldrv` bleiben unverändert, kein Dialog „Suite-Datei jetzt speichern“. |
| KW5 | **Von Hand:** Server bei einem Hoster, der SSH-Schlüssel nur im Webpanel verwaltet. Passwort ändern. | Dialog „Neuen Server-Schlüssel eintragen“: Der Anbieter verwaltet die Schlüssel in seinem Webpanel; für diesen Server „Den neuen Schlüssel (unten) im Webpanel des Anbieters hinzufügen, dann „Prüfen““, darunter der neue öffentliche Schlüssel zum Kopieren. GhulDrive funktioniert währenddessen weiter (mit dem alten Schlüssel). |
| KW6 | Nach KW5 den neuen Schlüssel im Webpanel eintragen, „Prüfen“. | „Der neue Schlüssel funktioniert. Jetzt den alten Schlüssel im Webpanel des Anbieters entfernen, dann „Prüfen““. |
| KW7 | Nach KW6 „Prüfen“, **ohne** den alten zu entfernen; dann den alten im Webpanel entfernen und „Prüfen“. | Solange der alte noch angenommen wird, bleibt die Aufforderung stehen. Danach verschwindet der Server aus dem Dialog; der Wechsel ist für ihn fertig. „Später“ schließt den Dialog, er kommt nach dem nächsten Entsperren wieder. |
| KW8 | **Fortsetzbar:** Flugmodus an, Passwort ändern, App schließen. Später ohne Flugmodus öffnen und entsperren. | Der Wechsel läuft nach dem Entsperren weiter, bis alle Server und Speicherorte durch sind (wie KW1 und KW2). Ein Server, der gerade nicht erreichbar ist, steht auf „Noch nicht erreicht, wird später wieder versucht.“ |
| KW9 | **Alte Kopie nach dem Wechsel:** Auf Gerät B (das den neuen Stand nie gesehen hat) die vor KW1 aufgehobene Kopie der Suite-Datei öffnen (altes Passwort), dann GhulDrive öffnen. | Die Datei öffnet sich (Stand von damals), aber GhulDrive kommt nicht an den Server: „Der Server hat die Anmeldung abgelehnt … Wurde der Schutz deiner Suite auf einem anderen Gerät geändert, öffne hier die aktuelle Suite-Datei.“ Nichts auf dem Server wird verändert. |
| KW10 | Nach KW9 auf Gerät B die aktuelle Suite-Datei öffnen. | GhulDrive funktioniert, alle Dateien (auch die, die B früher hochgeladen hat) sind da. |
| KW11 | Auf Gerät A (nach KW1) die alte Kopie öffnen. | Wird nicht geöffnet (älterer Schutz, wie SD14). |
| KW12 | **Alter Index unter dem alten Schlüssel:** Vor KW1 `drive/index.ghldrv` kopieren. Nach KW1 (Wechsel abgeschlossen) diese Kopie auf dem Server zurückspielen, GhulDrive öffnen. | Der zurückgespielte Index öffnet nicht mit dem neuen Schlüssel und wird nicht still übernommen: GhulDrive meldet einen beschädigten bzw. älteren Stand und schreibt nichts, bis du entscheidest. |
| KW13 | Während ein Wechsel läuft (ein Server wartet noch, z. B. KW5), auf einem schon umgeschlüsselten Speicherort einen Index mit dem alten Schlüssel zurückspielen. | Eigene Warnung („… mit einem Schlüssel von vor deiner letzten Schutzänderung …“); übernommen wird er nur nach Bestätigung unter Optionen → GhulDrive → „Älteren Serverstand übernehmen“. |
| KW14 | Mitten im Wechsel (nach KW5, vor KW7) den Server mit dem alten Schlüssel aus der Suite entfernen. | Der Wechsel endet, sobald alle übrigen Server und Speicherorte durch sind; kein Dialog mehr. |
| KW15 | Ein Upload läuft, dann Passwort ändern; nach dem Wechsel denselben Upload fortsetzen. | Das Hochladen setzt fort (der Schlüssel für unfertige Uploads bleibt), die Datei ist danach vollständig da. Im App-Ordner liegt keine Datei `drive-device.key`. |
| KW16 | **Erst speichern:** Passwort ändern, im Dialog „Später“. GhulDrive benutzen, dann auf einem zweiten Gerät die vorher aufgehobene Kopie öffnen (altes Passwort). App auf Gerät A sperren und entsperren. | Bis zum Speichern ändert sich auf dem Server nichts: `authorized_keys` unverändert, `index.ghldrv` mit dem alten Schlüssel; die alte Kopie öffnet GhulDrive weiter. Nach dem Entsperren fragt die App wieder nach dem Speichern. Nach dem Speichern läuft der Wechsel wie KW1. |
| KW17 | Wie KW16 (noch nicht gespeichert), einen zweiten Speicherort ohne seine Dateien entfernen. Dann speichern und noch einmal entfernen. | Erst: „Speichere zuerst die Suite-Datei …“, nichts entfernt. Nach dem Speichern wird er entfernt; später wieder hinzugefügt zeigt er seine Dateien. |

## 38. Dokumente scannen (GhulDocs, Android)

Die Kamera in der App (nicht die Kamera-App des Systems), mehrere Seiten in eine PDF. Fotos und Seiten liegen nur im
Arbeitsspeicher. Vorher: ein GhulDocs-Tresor mit einem Ordner, ein paar Blätter Papier (auch ein Kassenbon).

| Nr. | Schritt | Erwartet |
|---|---|---|
| SC1 | GhulDocs → „+“ → „Dokument scannen“, beim ersten Mal die Kamera ablehnen. | Hinweis, wozu die Kamera gebraucht wird, und „Kamera erlauben“. Nach „Kamera erlauben“ (bzw. in den App-Berechtigungen des Systems) erscheint das Kamerabild ohne Neustart. |
| SC2 | Ein Blatt schräg auf einem dunklen Tisch fotografieren. | Zuschneiden: Der Rahmen liegt auf den Ecken des Blatts („Blatt erkannt.“). Ohne erkennbares Blatt (z. B. eine Wand) steht ein Rahmen etwas innerhalb des Fotos („Kein Blatt erkannt.“). |
| SC3 | Eine Ecke mit dem Finger verschieben, dann über die gegenüberliegende Ecke hinaus ziehen. | Die Ecke folgt dem Finger, eine Lupe zeigt sie vergrößert. Der Rahmen klappt nie um: Die Ecke bleibt an der letzten gültigen Stelle stehen. „Ganzes Foto“ und „Erkanntes Blatt“ wechseln. |
| SC4 | „Übernehmen“, zwei weitere Seiten fotografieren, „Fertig“. | Seitenzähler und Vorschaubilder unter dem Kamerabild; die Seiten sind gerade (nicht mehr schräg) und scharf. |
| SC5 | Eine Seite drehen, „Dokument“ für eine Seite, dann „Dokument-Filter für alle Seiten“. | Die Vorschau zeigt die Änderung; mit Filter ist das Papier weiß (auch im Schatten), der Text schwarz. |
| SC6 | Eine Seite „Zuschneiden“ (Abbrechen), eine „Neu aufnehmen“, eine löschen. | Abbrechen lässt die Seite wie sie war; die neue Aufnahme ersetzt die Seite an ihrer Stelle; Löschen nimmt sie aus der Reihe. |
| SC7 | Namen ändern, „Als PDF speichern“. | Fortschritt unter der Kopfzeile, danach steht die PDF im geöffneten Ordner. In der App angesehen: alle Seiten, in der richtigen Reihenfolge, etwa A4, je Seite etwa 200 bis 600 KB. |
| SC8 | Mitten im Scan (Seiten aufgenommen) sperren: von Hand im Hintergrund, über die automatische Sperre und über die Panik-Sperre. | Nach dem Entsperren ist der Scan weg, nichts wurde gespeichert; die App sagt, dass nicht Gespeichertes verworfen wurde. |
| SC9 | Mit Seiten „Schließen“ bzw. Zurück. | Rückfrage „Änderungen verwerfen?“; „Verwerfen“ beendet den Scan ohne Speichern. |
| SC10 | Im Scan ein Bildschirmfoto versuchen und die Liste der letzten Apps öffnen. | Kein Bildschirmfoto, keine Vorschau (außer Bildschirmfotos sind in den Optionen erlaubt). |
| SC11 | Nach einem Scan im Dateimanager und unter `Android/data/…` nachsehen. | Keine Fotos, keine PDF außerhalb des Tresors; auch die Galerie zeigt keine neuen Fotos. |
| SC12 | Über 30 Seiten aufnehmen. | Hinweis „Viele Seiten …“; reicht der Speicher nicht mehr, nimmt die App keine weitere Seite auf und sagt es. |
| SC13 | GhulDocs fast voll (Testtresor nahe 1 GB), dann speichern. | Meldung „GhulDocs ist voll …“, die Seiten bleiben erhalten. |
| SC14 | Querformat, Taschenlampe („Licht“) an und aus, Kassenbon (lang und schmal). | Alles bedienbar; der Kassenbon wird eine schmale, hohe Seite. |

## Fehler melden

Bitte je Fehler: **Testnummer**, Handy und Android-Version, App-Version (Optionen → Über GhulSuite), was du gemacht hast,
was erwartet war, was passiert ist, ggf. Bildschirmfoto (dafür vorübergehend „Bildschirmfotos erlauben“ einschalten).
