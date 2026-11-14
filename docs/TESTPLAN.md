# GhulSuite: Testplan Android (Version 0.5.0-dev)

**Stand:** 13.11.2026 · neu: Assistent in acht Schritten und „Ein Bereich, eine Datei“ (Abschnitte 1 und 23) · deckt Phase 2 (Etappen 1, 2a, 2b, „Der Ordner ist die Suite“) und Phase 3 ab: eigene Server,
Synchronisation, GhulDrive und GhulMail (Abschnitte 12 bis 15) sowie 0.5.0: neues Design, Servertypen FTPS und
PHP-Server-Modul, Teilen mit Freigabelinks und die Modultrennung (Abschnitte 16 bis 19), verschlüsselte Mails (Abschnitt 20), Korrekturen nach dem ersten Test (Abschnitt 21).

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
| V3 | Bereitlegen: Security Key mit PIN (USB-C), möglichst ein zweiter Key, Stift und Papier, ein USB-Stick oder ein Ordner, den du selbst anlegen kannst. |
| V4 | Für den Import Exportdateien bereitlegen, z. B. aus Proton Pass (ZIP oder CSV), Proton Authenticator, Bitwarden, KeePassXC, Aegis, 2FAS, und Google Authenticator auf einem zweiten Gerät. **Nur Testdaten** verwenden. |
| V5 | Wichtig: Diese Entwicklerversion ist mit einem öffentlichen Testschlüssel signiert. Keine echten Passwörter speichern. |
| V6 | Für Abschnitt 12 bis 13: deinen Server mit SSH/SFTP (Adresse, Port, Benutzer) und Zugriff auf `~/.ssh/authorized_keys`. Fingerabdruck des Servers bereithalten: auf dem Server `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`. Ein zweites Android-Gerät (oder eine zweite Installation nach Deinstallation) für die Synchronisation. |
| V7 | Für Abschnitt 14 bis 15: ein **Test**-Mailkonto mit IMAP/SMTP (Server, Benutzer, Passwort bzw. App-Passwort) und eine zweite Adresse, von der du Mails schicken kannst. |
| V8 | Für Abschnitt 17 bis 18: ein **Webspace** mit FTP-Zugang (FTPS) und PHP 8.1 oder neuer, darin ein öffentlich erreichbarer Ordner (z. B. `public_html/teilen`) mit https-Adresse. Optional: dein SFTP-Server mit Webserver. |

---

## 1. Erster Start und Einrichtung

Der Assistent hat acht Schritte; oben steht „Schritt n von 8“ mit Fortschrittsbalken. Die Suite entsteht beim Verlassen
von Schritt 3, danach geht „Zurück“ nur noch bis Schritt 4.

| Nr. | Schritt | Erwartet |
|---|---|---|
| E1 | App öffnen. | Startbildschirm mit „Neu einrichten“, „Von meinem Server holen“, „Vorhandene Suite öffnen“, „Einzelnen Tresor öffnen“. |
| E2 | „Neu einrichten“. | Schritt 1 von 8 · Master-Passwort. |
| E3 | Passwort „abc“ eingeben, Weiter. Dann ein schwaches 10-Zeichen-Passwort. Dann zwei verschiedene Passwörter. | Jeweils klare Meldung: zu kurz / zu leicht zu erraten / stimmen nicht überein. |
| E4 | „Passwort vorschlagen“ antippen, Wiederholung eingeben, Weiter. | Langes Zufallspasswort, Stärkebalken voll; Schritt 2 · Zweiter Faktor. |
| E5 | „Security Key“ gewählt lassen, Name des Keys, Key-PIN eingeben (Android: „Key prüfen“ zeigt „Erkannt: …“). Weiter. | Schritt 3 · Speicherort. |
| E6 | App-Speicher lassen, „Key registrieren“, Key **zweimal** berühren. | Schritt 4 · Synchronisation; kein „Zurück“ zu den Schritten 1 bis 3 mehr. |
| E7 | „Eigener Server“ wählen, ohne Angaben „Weiter“. | Meldung „Noch offen: Anmeldung“ (bzw. der fehlende Schritt). Checkliste unter dem Formular zeigt Anmeldung, Ordner, Verbindung testen. |
| E8 | SFTP-Server eintragen (siehe Abschnitt 12), „Ordner wählen“. | Fingerabdruck bestätigen, dann die Ordner des Servers; „ghulsuite, Neu anlegen (empfohlen)“, Unterordner öffnen, „Eine Ebene höher“, „Neuer Ordner …“. |
| E9 | Ordner wählen → „Verbindung testen“ → Weiter. | „Verbunden · host:/ordner“, Haken in der Checkliste; Schritt 5 · Bereiche. |
| E10 | Alle fünf Bereiche einschalten. Bei GhulDrive „Wie Synchronisation“. Weiter. | Namen für GhulPass, GhulAuth, GhulCal änderbar; „Wie Synchronisation“ nur wählbar, weil Schritt 4 eingerichtet wurde. Schritt 6 · Sperren. |
| E11 | Sperrzeit wählen, Schalter für Bildschirm aus und Schütteln. Weiter. | Schritt 7 · Entsperren mit Hinweis, dass „Schnell entsperren“ Fingerabdruck und App-PIN meint. |
| E12 | „Zusätzlich Fingerabdruck“, Gültigkeit „Immer“. Weiter. | Hinweis: ohne Ablauf, aber nach Neustart, Sofortsperre oder Passwortwechsel wieder Passwort und Key. Fingerabdruck-Abfrage; Schritt 8. |
| E13 | Code ansehen, „Notfallblatt drucken“ → „Als PDF speichern“. | PDF mit Code, QR-Code und Anleitung. PDF danach vom Handy löschen. |
| E14 | „Ich habe den Code notiert“ → Weiter → falsche Gruppe 3 eingeben. | Meldung „Das ist nicht die dritte Gruppe“. Es gibt kein „Später“. |
| E15 | Richtige Gruppe 3 eingeben → Abschließen. | Hauptansicht mit allen fünf Bereichen und Optionen; GhulMail zeigt „Noch kein Konto“ mit dem vollständigen Kontoformular. |
| E16 | Zweite Einrichtung: in Schritt 4 „Später“, in Schritt 5 GhulDrive einschalten. | „Wie Synchronisation“ ist ausgegraut („Erst Synchronisation einrichten (Schritt 4)“); „Eigener Server“ klappt das Formular direkt auf. |
| E17 | GhulDrive „Eigener Server“ → PHP-Modul: FTPS-Daten des Webhostings, Ordner wählen (z. B. `public_html/ghulsuite`), „Modul hochladen“, vorgeschlagene Adresse „Adresse prüfen“. | Modul liegt im Ordner, Adresse wird aus Domain und Ordner abgeleitet und geprüft; „FTPS-Zugang behalten“ wählbar. Bei kleinem Upload-Limit ein Hinweis auf kleinere Teile. |
| E18 | Einrichtung mit „Schlüsseldatei“ statt Key: Datei erstellen und z. B. auf den USB-Stick speichern. | Ohne gespeicherte Datei geht es nicht weiter. Danach wie E6 bis E15 (ohne Key-Berührung). |
| E19 | In Schritt 3 einen eigenen Ordner wählen, in dem schon GhulSuite-Dateien liegen. | Hinweis „Hier liegen bereits GhulSuite-Dateien“ und „Diese Suite öffnen“. |
| E20 | Nur GhulAuth wählen und einrichten. | In der Leiste nur GhulAuth und Optionen; keine weiteren Dateien im Ordner. |
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

## 6. Tresore: „Der Ordner ist die Suite“

Für T5 bis T8 brauchst du einen eigenen Ordner als Speicherort (z. B. USB-Stick oder ein Ordner im internen Speicher).

| Nr. | Schritt | Erwartet |
|---|---|---|
| T1 | Optionen → Tresore → „Neuer Tresor“, umbenennen. Sperren und entsperren. | Neuer Name bleibt erhalten. |
| T2 | Menü eines Tresors → „Exportieren“, Datei speichern. | Verschlüsselte `.ghlpss`/`.ghltotp`-Datei. |
| T3 | „Tresordatei hinzufügen“ → die gerade exportierte Datei. | Meldung „schon vorhanden“ (gleicher Tresor). |
| T4 | Tresor löschen (Menü → Löschen). | Warnung; danach ist er weg. Über „Tresordatei hinzufügen“ + Export aus T2 wieder da. |
| T5 | Mit einem Dateimanager eine Tresordatei derselben Suite (z. B. vom zweiten Gerät oder aus T2) in den Ordner kopieren, App sperren und entsperren. | Tresor ist ohne Rückfrage da. |
| T6 | Tresordatei einer **anderen** Suite (anderes Passwort) in den Ordner kopieren, entsperren. | Dialog „Tresore mit anderem Passwort“ → „Mit Passwort öffnen“ → altes Passwort + Key/Datei → Tresor gehört dazu und öffnet sich ab jetzt mit deinem Passwort. |
| T7 | Eine ältere Kopie eines Tresors über die aktuelle Datei kopieren, entsperren. | Hinweis „älter als zuletzt gesehen“, im Tresor-Menü „Diesen Stand übernehmen“. |
| T8 | `suite.ghlsuite` im Ordner löschen, App entsperren. | Entsperren klappt trotzdem, die Datei wird neu angelegt. |
| T9 | Nur eine einzelne Tresordatei auf ein anderes Gerät/in einen leeren Ordner legen, dort „Einzelnen Tresor öffnen“. | Öffnet mit Passwort + Key; Hinweis „dein bisheriges Notfallblatt gilt weiter“; nur dieser Bereich ist eingeschaltet. |

## 7. Schutz und Wiederherstellung

| Nr. | Schritt | Erwartet |
|---|---|---|
| S1 | Optionen → Sicherheitsschlüssel & Schlüsseldateien → „Security Key hinzufügen“ mit zweitem Key (zwei Berührungen). | Zweiter Key in der Liste; beide entsperren. Code bleibt gültig. |
| S2 | „Schlüsseldatei hinzufügen“ (erstellen und speichern). | Datei in der Liste; Entsperren damit möglich (U6). |
| S3 | Schlüsseldatei entfernen, später den letzten Key entfernen versuchen. | Entfernen geht; der letzte Key/die letzte Datei lässt sich **nicht** entfernen. |
| S4 | Suite mit Schlüsseldatei (E11): Key hinzufügen, dann Datei entfernen. | Key funktioniert, Datei nicht mehr, **alter Wiederherstellungscode funktioniert weiterhin**. |
| S5 | „Neuer Wiederherstellungscode“. Sperren, alten Code probieren, dann neuen. | Alter Code: ungültig. Neuer: geht. |
| S6 | Passwort ändern (aktuelles falsch, dann richtig). | Falsch: Meldung. Richtig: neues Passwort gilt, Fingerabdruck/PIN-Zugang ist neu einzurichten. |
| S7 | Vor S5: den Suite-Ordner kopieren (alte Sicherung). Nach S5 die Kopie in einen leeren Ordner legen und mit dem **alten** Code öffnen; dann die **aktuellen** Dateien mit dem alten Code. | Die alte Kopie öffnet sich (Stand von damals). Die aktuellen Dateien öffnen sich mit dem alten Code nicht. Fingerabdruck/PIN funktionieren nach S5 weiter. |
| S8 | Zwei Keys (oder Key + Schlüsseldatei). Ordner kopieren, dann einen Key entfernen. Mit dem entfernten Key + Passwort die Kopie öffnen, dann die aktuellen Dateien. | Kopie: öffnet sich. Aktuelle Dateien: „Key nicht eingetragen“. Der verbliebene Key öffnet alles, **ohne** dass er beim Entfernen berührt werden musste. Fingerabdruck/PIN ist neu einzurichten. |
| S9 | Suite aus einer älteren App-Version (vor Formatversion 2) mit zwei Keys öffnen. Optionen → Keys. | Der benutzte Key ist normal, der andere zeigt „einmal damit entsperren …“. Nach einem Entsperren mit ihm verschwindet der Hinweis. |
| S10 | Zwei Geräte mit Sync. Gerät B in den Flugmodus, dort Einträge ändern. Auf A einen Key entfernen und einen neuen Code erstellen, Sync. B wieder online, Sync. | B fragt einmal „Schutz geändert“ nach einem übrigen Key oder der Schlüsseldatei (bei neuem Passwort auch danach). Danach sind die Änderungen beider Geräte auf beiden. Auf dem Server wurde nichts überschrieben. |
| S11 | Suite-Ordner auf dem Gerät und auf dem Server ansehen. | Tresordateien heißen nur nach ihrer Kennung (z. B. `3f9a1c2b7d4e5f60.ghlpss`), kein Tresorname; ältere Namen sind nach dem Entsperren bzw. nächsten Sync verschwunden. Die Tresorliste zeigt keine Dateinamen. |

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

## 10. Autofill

| Nr. | Schritt | Erwartet |
|---|---|---|
| F1 | Optionen → GhulPass → Autofill → „Als Autofill-Dienst einstellen“, GhulSuite wählen. | Zurück in der App steht „aktiv“. |
| F2 | GhulSuite **sperren**. Im Browser (z. B. Chrome) eine Login-Seite öffnen, für die ein Eintrag existiert, ins Benutzerfeld tippen. | Vorschlag „GhulSuite entsperren“, **kein** Benutzername sichtbar. |
| F3 | Antippen, entsperren, Anmeldung wählen. | Benutzer und Passwort werden eingefügt. |
| F4 | Entsperrt erneut ins Feld tippen. | Passende Anmeldung direkt vorgeschlagen, plus „Andere Anmeldung wählen …“. |
| F5 | Eine Seite mit ähnlicher, aber anderer Domain (z. B. `github.com.example.org`). | **Keine** direkte Anmeldung angeboten. |
| F6 | Eine App mit Login (z. B. eine Test-App) beim ersten Mal. | Kein direkter Vorschlag; über „Andere Anmeldung wählen …“ auswählen. |
| F7 | Dieselbe App beim zweiten Mal. | Jetzt wird die gewählte Anmeldung direkt angeboten. |
| F8 | Neue Anmeldung auf einer Website/in einer App eingeben und absenden. | Android fragt „In GhulSuite speichern?“ → gespeichert, Meldung. |
| F9 | Passwort einer bestehenden Anmeldung ändern und absenden. | „Passwort in GhulSuite aktualisiert“. |
| F10 | GhulSuite gesperrt, neue Anmeldung absenden. | „GhulSuite ist gesperrt, Anmeldung nicht gespeichert“. |
| F11 | Tastatur mit Vorschlagsleiste (z. B. Gboard), Android 11 oder neuer: in ein Anmeldefeld tippen. | Die Vorschläge erscheinen als Chips oben in der Tastatur, mit Schloss-Symbol. |
| F12 | Vivaldi, Brave, Edge oder Samsung Internet: Login-Seite öffnen, ins Feld tippen. | Vorschlag „GhulSuite entsperren“ bzw. die passende Anmeldung. |
| F13 | Chrome: Einstellungen → Autofill-Dienste → „Autofill über einen anderen Dienst“, Chrome neu starten, F2 bis F4 wiederholen. | Wie F2 bis F4. |
| F14 | In einem Browser, dessen Seite Android nicht erkennt, „Andere Anmeldung wählen …“ benutzen. | Ausfüllen klappt; die Anmeldung wird **nicht** mit dem Browser verknüpft (in anderen Seiten kein direkter Vorschlag). Kein Angebot zum Speichern. |
| F15 | Es erscheint nichts: lange auf das Feld drücken → „Automatisch ausfüllen“. | GhulSuite-Vorschläge erscheinen. |
| F16 | Optionen → GhulPass → Autofill bei aktivem Dienst. | Abschnitt „So erscheinen die Vorschläge“ mit Hinweisen zu Tastatur, Apps, Chrome und langem Drücken. |

## 11. Einstellungen und Allgemeines

| Nr. | Schritt | Erwartet |
|---|---|---|
| O1 | Module: GhulAuth ausschalten, wieder einschalten. | Verschwindet aus dem Menü, Daten bleiben. |
| O2 | Design hell/dunkel/System. | Wechselt sofort, alles lesbar. |
| O3 | Android-Optionen → Apps → GhulSuite → Sprache: Englisch. | Oberfläche englisch. |
| O4 | Flugmodus an, alles Obige kurz wiederholen (Entsperren, Codes, GhulPass, GhulAuth). | Funktioniert vollständig offline (nur Synchronisation, Drive und Mail brauchen das Netz). |
| O5 | Handy drehen in Listen, Bearbeiten und Dialogen. | Kein Datenverlust, keine Abstürze. |

## 12. Eigene Server und Synchronisation

| Nr. | Schritt | Erwartet |
|---|---|---|
| SY1 | Optionen → Server, Sync & Teilen → „Server hinzufügen“ → Verbindungsart **SFTP (SSH)**. Adresse, Port, Benutzer, Ordner (z. B. `ghulsuite`) eingeben, „SSH-Schlüssel“ gewählt lassen. | Ein öffentlicher Schlüssel `ssh-ed25519 …` wird angezeigt; „Kopieren“ legt ihn in die Zwischenablage. |
| SY2 | Schlüssel auf dem Server in `~/.ssh/authorized_keys` eintragen, dann „Verbinden“. | Dialog „Server-Schlüssel prüfen“ mit `SHA256:…`. Er stimmt mit dem Fingerabdruck aus V6 überein. |
| SY3 | „Fingerabdruck stimmt“. | Server erscheint in der Liste. Auf dem Server gibt es den Ordner `ghulsuite/suite/` (leer). |
| SY4 | Server mit falschem Fingerabdruck-Abgleich: „Abbrechen“ im Dialog. | Nichts wird gespeichert. |
| SY4b | Einfachster Weg: „Server hinzufügen“ → „Passwort“ → SSH-Passwort eingeben, „SSH-Schlüssel automatisch einrichten“ eingeschaltet lassen → „Verbinden“ → Fingerabdruck bestätigen. | Server erscheint mit „Ordner: /home/…/ghulsuite“. Auf dem Server steht in `~/.ssh/authorized_keys` eine neue Zeile `ssh-ed25519 … GhulSuite`, alte Zeilen sind unverändert. Das Passwort ist nicht gespeichert (Server funktioniert auch nach Passwortänderung weiter). |
| SY5 | Variante Passwort: zweiten Server-Eintrag mit „Passwort“ anlegen (falls der Server Passwort-Anmeldung erlaubt). | Funktioniert wie SY2 bis SY3. Falsches Passwort → „Der Server hat die Anmeldung abgelehnt …“. |
| SY6 | Synchronisation → Server wählen. | Synchronisiert sofort; „Zuletzt synchronisiert: …“. Auf dem Server liegen jetzt `suite.ghlsuite` und die Tresordateien, alle verschlüsselt (Inhalt unlesbar). |
| SY7 | **Zweites Gerät:** Startbildschirm → „Von meinem Server holen“. Adresse usw. eingeben; den angezeigten **Geräteschlüssel** zusätzlich in `authorized_keys` eintragen; „Suite holen“, Fingerabdruck bestätigen. | „Suite geladen. Jetzt entsperren.“ → Entsperren mit Passwort und Key/Schlüsseldatei klappt, alle Einträge sind da. |
| SY8 | Gerät 1: Eintrag A anlegen. Gerät 2: Eintrag B anlegen (beide mit „Automatisch“). Danach auf beiden „Jetzt synchronisieren“. | Beide Geräte zeigen A **und** B. Nichts geht verloren. |
| SY9 | Denselben Eintrag auf beiden Geräten unterschiedlich ändern, nacheinander synchronisieren. | Die später gespeicherte Änderung gewinnt auf beiden Geräten. |
| SY10 | Gerät 1: Eintrag löschen, synchronisieren; Gerät 2 synchronisieren. | Eintrag ist auf beiden weg. |
| SY11 | Gerät 1: neuen Tresor anlegen → synchronisieren → Gerät 2 synchronisieren. | Tresor erscheint auf Gerät 2. Danach auf Gerät 1 Tresor entfernen, beide synchronisieren: auf beiden weg. |
| SY12 | Gerät 1: Master-Passwort ändern, synchronisieren. Gerät 2: synchronisieren. | Gerät 2 fragt „Neues Master-Passwort“. Neues Passwort eingeben → übernommen; nach Sperren entsperrt Gerät 2 nur noch mit dem neuen Passwort. |
| SY13 | Gerät 1: zusätzlichen Key oder Schlüsseldatei hinzufügen, synchronisieren; Gerät 2 synchronisieren, sperren. | Gerät 2 lässt sich auch mit dem neuen Key/der neuen Schlüsseldatei entsperren. |
| SY14 | Flugmodus an, etwas ändern. | Keine Fehlermeldung (automatische Synchronisation still). „Jetzt synchronisieren“ meldet „Keine Verbindung …“. Nach dem Flugmodus synchronisiert es wieder. |
| SY15 | Auf dem Server den Host-Key tauschen (oder einen anderen Server unter derselben Adresse) und synchronisieren. | Warnung „Der Server zeigt einen anderen Schlüssel …“, es wird nichts übertragen. |
| SY16 | Server entfernen, dann auf einem zweiten Gerät synchronisieren. | Server weg, Synchronisation aus; Dateien auf dem Server bleiben. Der Server taucht auch nach dem Synchronisieren nicht wieder auf. |

## 13. GhulDrive

| Nr. | Schritt | Erwartet |
|---|---|---|
| D1 | Tab GhulDrive (ggf. unter Optionen → Module einschalten). | „GhulDrive einrichten“ mit deinen Servern. |
| D2 | „<Server> verwenden“. | Leere Liste „Meine Dateien“, unten „Auf dem Server: /home/…/ghulsuite/drive“. Dort entsteht `index.ghldrv`. |
| D3 | „Hochladen“, ein Foto oder PDF wählen. | Fortschritt, dann „… hochgeladen.“; Datei mit Größe und Datum in der Liste. |
| D4 | Auf dem Server in `ghulsuite/drive/` schauen. | Nur `index.ghldrv` und `blocks/xx/<lange ID>`: **kein** Dateiname, Inhalt unlesbar. |
| D5 | Große Datei (> 50 MB, z. B. Video) hochladen. | Fortschritt läuft durch; App bleibt bedienbar; Datei erscheint. |
| D6 | Datei antippen → Speicherort wählen. | Gespeicherte Datei ist identisch (öffnen, ansehen). |
| D7 | Upload starten und „Abbrechen“. | Bricht ab, keine halbe Datei in der Liste. |
| D8 | „Hinzufügen“ → „Neuer Ordner“ → Name; Ordner öffnen, Datei hineinladen; Zurück-Pfeil. | Ordnerstruktur stimmt, Pfad oben passt. |
| D9 | Datei umbenennen, in einen anderen Ordner verschieben („Hierher verschieben“). | Name bzw. Ort ändern sich. |
| D10 | Suche nach einem Teil des Dateinamens. | Treffer aus allen Ordnern. |
| D11 | Datei löschen; Ordner mit Inhalt löschen. | Weg; nach dem Löschen auf dem Server weniger Blöcke. |
| D12 | Zweites Gerät (nach Synchronisation, SY7): GhulDrive öffnen, aktualisieren. | Dieselben Dateien und Ordner; Herunterladen klappt. |
| D13 | Dieselbe Datei zweimal hochladen. | Zwei Einträge in der Liste, auf dem Server aber nicht doppelter Platz. |

## 14. GhulMail

| Nr. | Schritt | Erwartet |
|---|---|---|
| M1 | Tab GhulMail → „E-Mail-Konto hinzufügen“. E-Mail-Adresse eingeben. | Unter „Eingang/Ausgang“ werden passende Server vorgeschlagen (bei bekannten Anbietern genau, sonst `imap.`/`smtp.` + Domain). |
| M2 | Passwort (bzw. App-Passwort), Name eingeben, „Prüfen und speichern“. | Signatur ist mit „Viele Grüße“ und deinem Namen vorbelegt. Konto wird gespeichert, Posteingang erscheint. |
| M3 | Falsches Passwort. | „Der Server hat die Anmeldung abgelehnt …“, nichts gespeichert. |
| M4 | „Server-Einstellungen anzeigen“: Ports ändern, falscher Server. | Für IMAP gibt es nur SSL/TLS (keine Wahl), für den Ausgang SSL/TLS oder STARTTLS. Falscher Server: klare Meldung „Keine Verbindung …“. |
| M5 | Eigener Mailserver mit selbst signiertem Zertifikat (falls vorhanden). | Dialog „Zertifikat prüfen“ mit Fingerabdruck; nach Bestätigung funktioniert es. |
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
| M18 | Sperren und entsperren, Synchronisation (SY7) auf Gerät 2. | Konten sind nach Entsperren bzw. auf Gerät 2 vorhanden. |

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
| N7 | Mailpasswort auf dem **anderen** Gerät ändern, hier synchronisieren und einmal entsperren. | Benachrichtigungen kommen weiter (die Kopie für den Empfang wird beim Entsperren erneuert). |
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
| DS3 | Kontextzeile in GhulDrive und GhulMail antippen. | Drive: Speicherort, „Freigabelinks“, „Server, Sync & Teilen“. Mail: Konten, Ordner mit Symbolen, „Neues Konto“. |
| DS4 | Passwort eines Eintrags anzeigen lassen. | Ziffern und Sonderzeichen sind farbig, Buchstaben normal. |
| DS5 | Tab Einstellungen. | Karte „Meine Suite“ mit „Sperren“, darunter Gruppen Sicherheit, Schutz, Daten, Server, Bereiche, Allgemein; jede Zeile mit kurzem Status (z. B. „1 Schlüssel, 1 Schlüsseldateien“, „2 Server · Sync an · Teilen an“). |
| DS6 | Jede Zeile öffnen, Zurück. | Eigene Seite mit Titel; Zurück führt zur Übersicht. Abgeschaltete Module erscheinen nicht unter „Bereiche“. |
| DS7 | Dunkles Design (Optionen → Ansicht). | Alle Seiten gut lesbar. |

## 17. Servertypen: FTPS und PHP-Server-Modul

| Nr. | Schritt | Erwartet |
|---|---|---|
| ST1 | Optionen → Server, Sync & Teilen → „Server hinzufügen“. | Auswahl Verbindungsart: SFTP (SSH), FTPS, PHP-Server-Modul, jeweils mit Erklärung. |
| ST2 | **FTPS**: Adresse, Port 21, Benutzer, Passwort, Ordner `ghulsuite` → Verbinden. | Dialog mit Zertifikat-Fingerabdruck (bei gültigem Zertifikat ggf. direkt gespeichert). Nach Bestätigung: Server in der Liste „Name · FTPS“, „Ordner: /ghulsuite“. |
| ST3 | FTPS mit falschem Passwort. | Fehlermeldung „Anmeldung abgelehnt“, nichts gespeichert. |
| ST4 | Synchronisation und GhulDrive mit dem FTPS-Server (wie SY6, D2 bis D6). | Funktioniert; auf dem Webspace nur verschlüsselte Dateien. |
| ST5 | **PHP-Modul**: Verbindungsart „PHP-Server-Modul“ → „Server-Modul erstellen“. | „Modul ghulsuite.php mit eigenem Zugangsschlüssel erstellt.“ Knöpfe „Datei speichern“ und (wenn ein FTPS/SFTP-Server existiert) „Über Server hochladen“. |
| ST6 | „Über Server hochladen“ → FTPS-Server wählen, Webordner (z. B. `public_html`) → Hochladen. | „Modul hochgeladen.“ Auf dem Webspace liegt `ghulsuite.php`. *(Alternativ: „Datei speichern“ und selbst per FTP hochladen.)* |
| ST7 | Adresse `https://deine-domain/ghulsuite.php` eintragen → Verbinden. | Server „Webspace · PHP-Modul“ in der Liste, „Teilen: https://…/ghulsuite.php“. Neben der PHP-Datei entsteht ein Ordner `ghul-data-…`. |
| ST8 | Im Browser `https://deine-domain/ghul-data-…/` öffnen. | Zugriff verweigert (403) bzw. leere Seite, kein Inhalt. |
| ST9 | Im Browser `https://deine-domain/ghulsuite.php` öffnen. | Seite „Geteilte Datei“ mit Meldung „Der Link ist unvollständig“ (es fehlt der Teil nach #). |
| ST10 | Adresse mit `http://` statt `https://`. | „Verbinden“ bleibt gesperrt. |
| ST11 | GhulDrive auf dem PHP-Modul einrichten (anderes Gerät oder nach Entfernen des Drive-Servers) und eine Datei hoch- und herunterladen. | Funktioniert. |

## 18. Teilen

| Nr. | Schritt | Erwartet |
|---|---|---|
| SH1 | Ohne eingerichteten Teilen-Ordner in GhulDrive eine Datei → ⋮ → Teilen. | Teilen-Menü: „Freigabelink, Zuerst einen Teilen-Ordner einrichten“, „An App senden“, ggf. „Als Mail-Anhang“. |
| SH2 | Optionen → Server, Sync & Teilen → beim SFTP- oder FTPS-Server „Teilen einrichten“. | Assistent: 1. Ordner auf dem Server durchblättern (Ordner antippen, „..“ zurück, „Neuer Ordner“), 2. Webadresse, 3. Prüfung. |
| SH3 | Ordner im Webbereich wählen (z. B. `public_html/teilen`), passende https-Adresse → „Einrichten und prüfen“. | Vier grüne Haken: Ordner beschreibbar, über HTTPS erreichbar, Ordnerinhalt wird nicht aufgelistet, Abrufseite funktioniert. „Alles funktioniert …“. |
| SH4 | Falsche Adresse (passt nicht zum Ordner) prüfen. | Mindestens ein roter Punkt mit Erklärung; nichts wird gespeichert. |
| SH5 | Den Suite-Ordner (`ghulsuite`) als Teilen-Ordner wählen. | Abgelehnt. |
| SH6 | GhulDrive → Datei → Teilen → Freigabelink → Gültig „7 Tage“ → „Link erstellen“. | „Link erstellt“ mit Link, Größe, „bis …“. „Kopieren“ legt ihn in die Zwischenablage. |
| SH7 | Link auf einem anderen Gerät/PC im Browser öffnen. | Seite zeigt Dateiname und Größe, „Herunterladen“ liefert die identische Datei. Der Teil nach # verschwindet aus der Adresszeile. |
| SH8 | Link mit „Zusätzliches Passwort“ erstellen, im Browser öffnen. | Passwortfeld; falsches Passwort → Fehlermeldung; richtiges → Download. |
| SH9 | Link um ein Zeichen kürzen oder ändern und öffnen. | „Der Link ist unvollständig …“ bzw. „Falsches Passwort oder der Link ist beschädigt“. Kein Download. |
| SH10 | Mit dem **PHP-Modul**: „Abrufe“ auf „1×“ → Link erstellen, zweimal öffnen. | Erster Abruf klappt, zweiter: „Diese Freigabe gibt es nicht mehr …“. |
| SH11 | Mit dem PHP-Modul „5×“ → zweimal öffnen → GhulDrive → Kontextzeile → Freigabelinks. | Eintrag zeigt „2 von 5 Abrufen“. |
| SH12 | Mit SFTP/FTPS-Teilen-Ordner. | Abrufbegrenzung nicht wählbar, Hinweis „geht nur mit dem PHP-Server-Modul“. |
| SH13 | Freigabelinks → Eintrag „Zurückziehen“ → Link im Browser neu laden. | Eintrag verschwindet sofort, Meldung „Link zurückgezogen“. Im Browser: „Diese Freigabe gibt es nicht mehr …“. Die Datei bleibt in GhulDrive. Mit eingeschalteter Synchronisation: einige Sekunden warten, Freigabelinks erneut öffnen, der Eintrag bleibt weg. |
| SH14 | In GhulDrive lange drücken, zwei Dateien und einen Ordner dazu antippen → Teilen → Freigabelink. | Ein Link; im Browser kommt ein ZIP mit allen Dateien in ihrer Ordnerstruktur. |
| SH15 | Mehrfachauswahl → „In den Papierkorb“. | Ohne Rückfrage weg, Meldung „Rückgängig“; „Rückgängig“ bringt alle zurück. |
| SH16 | Datei → Teilen → „An App senden“ → z. B. Messenger oder Dateien-App. | Android-Teilen-Menü; die Ziel-App erhält die **entschlüsselte** Datei mit richtigem Namen. |
| SH17 | Nach SH16 GhulSuite sperren. | Die entschlüsselte Kopie im App-Cache wird gelöscht (ein erneutes Öffnen aus der Ziel-App kann fehlschlagen, gewollt). |
| SH18 | GhulPass → Eintrag öffnen → „Sicher teilen (Einmal-Link)“. | Formular: 1 Tag, bei PHP-Modul 1 Abruf vorgewählt. Link erstellen und im Browser öffnen: Textdatei mit Titel und Feldern (Benutzer, Passwort, …). |
| SH19 | Ohne Server mit Teilen-Ordner einen GhulPass-Eintrag öffnen. | Kein „Sicher teilen“-Knopf. |
| SH20 | Abgelaufenen Link testen: Freigabe mit „1 Tag“ erstellen, am nächsten Tag öffnen. | „Diese Freigabe gibt es nicht mehr …“, auch beim SFTP/FTPS-Teilen-Ordner. |

## 19. Modultrennung und GhulMail-Anbindung

| Nr. | Schritt | Erwartet |
|---|---|---|
| MO1 | GhulMail eingeschaltet, mindestens ein Konto: GhulDrive → Datei → Teilen. | „Als Mail-Anhang“ wird angeboten. |
| MO2 | „Als Mail-Anhang“. | „Schreiben“ öffnet sich: Betreff = Dateiname, Datei als Anhang. Senden an die zweite Adresse: Anhang kommt identisch an. |
| MO3 | Freigabelink erstellen → „Mit GhulMail senden“. | Entwurf mit Text „Hallo, hier ist … für dich:“ und dem Link. Nach dem Senden: Link in der empfangenen Mail anklicken → Datei öffnet sich im Browser. |
| MO4 | Datei über 20 MB → Teilen. | „Als Mail-Anhang“: „Zu groß für eine Mail, bitte als Link teilen“, nicht antippbar. |
| MO5 | Optionen → Module → GhulMail **aus**. Dann MO1 und SH6 wiederholen. | Kein „Als Mail-Anhang“, im Link-Dialog kein „Mit GhulMail senden“, nur „Kopieren“. Freigabelinks und „An App senden“ funktionieren normal. |
| MO6 | GhulMail ein, aber **kein** Konto (alle entfernt). | Wie MO5: keine Mail-Optionen. |
| MO7 | GhulDrive **aus**, GhulMail ein. | GhulMail funktioniert vollständig (Lesen, Schreiben, Anhänge aus Dateien). GhulPass-Einmal-Link funktioniert weiter, wenn ein Teilen-Server existiert. |
| MO8 | Alle Module außer GhulPass aus. | App läuft normal; Einstellungen zeigen unter „Bereiche“ nur GhulPass. Teilen-Einrichtung bleibt unter „Server, Sync & Teilen“ erreichbar. |

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
| N15 | Optionen → GhulDrive → Foto-Sicherung an (nur WLAN). Foto machen, App entsperren. | Foto landet im Ordner „Fotos“. *Läuft nur, solange die App entsperrt ist.* |
| N16 | GhulMail: Postfach-Blatt → „Alle Posteingänge“; Liste zeigt Vorschautext; Unterhaltung mit Antworten. | Mails aller Konten gemischt; Antworten unter einem Eintrag gruppiert. |
| N17 | Mail markieren (Stern), Label vergeben, nach links wischen. | Stern auch im Webmail; Label sichtbar; Wischen führt die gewählte Aktion aus (Optionen → GhulMail). |
| N18 | Neue Mail anfangen, zurück. Postfach → „Entwürfe“. | Entwurf ist da und lässt sich fortsetzen. Adressvorschläge beim Tippen. |
| N19 | „Später senden“ in 2 Minuten. App entsperrt lassen. | Mail geht zur Zeit raus; vorher unter „Geplant“ abbrechbar. *Nur bei entsperrter App.* |
| N20 | Mail → Menü: als .eml, als PDF, in GhulDrive speichern, „Original anzeigen“. | Dateien korrekt (Umlaute im PDF richtig); Kopfzeilen sichtbar. |
| N21 | Optionen → GhulMail → „Offline lesen“ an, Posteingang öffnen, Flugmodus. | Liste und geöffnete Mails bleiben lesbar, Hinweis „offline“. |
| N22 | Optionen → Sicherung → „Komplette Suite sichern“; neues Gerät/leerer Ordner → Start → „Sicherung wiederherstellen“. | Alles wieder da (Tresore, Einstellungen); Hauptpasswort wie vorher. Automatische Wochensicherung in gewählten Ordner (hält die letzten 4). |
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
| N33 | Eintrag als Favorit markieren, dann in einer App das Autofill öffnen. | Favoriten stehen in der Auswahl ganz oben. |

---

## 23. Ein Bereich, eine Datei (#24)

| Nr. | Schritt | Erwartet |
|---|---|---|
| B1 | Optionen → Bereiche verwalten. | Alle fünf Bereiche mit „aktiv“, „hier ausgeblendet“ oder „nicht eingerichtet“ und dem Dateinamen. |
| B2 | Nicht eingerichteten Bereich antippen (z. B. GhulCal) → Einrichten. | Name abgefragt; Datei `….ghlcal` entsteht, Bereich erscheint in der Leiste. Bei GhulDrive: Speicherwahl (wie Synchronisation / eigener Server). |
| B3 | Bereich → „Auf diesem Gerät ausblenden“. | Verschwindet nur hier aus der Leiste; zweites Gerät zeigt ihn weiter. Wieder einblenden geht. |
| B4 | Bereich → „Bereich entfernen“. | Rückfrage mit Warnung; danach ist die Datei weg, nach dem Synchronisieren auch auf dem zweiten Gerät und dem Server. |
| B5 | Optionen → GhulCal: Bundesland ändern, synchronisieren, zweites Gerät synchronisieren. | Oben „Für GhulCal · alle Geräte“, unten „Nur auf diesem Gerät“. Das Bundesland ist auf dem zweiten Gerät übernommen, die Startansicht nicht. |
| B6 | Optionen → GhulDrive → Speicher. | „Wie Synchronisation“ (nur mit Sync) oder „Eigener Server“; der eigene Server erscheint nicht unter Optionen → Synchronisation. |
| B7 | GhulDrive nutzt „Wie Synchronisation“. Optionen → Synchronisation → Sync-Server entfernen oder wechseln. | Rückfrage: „Zugang für GhulDrive behalten“ oder „GhulDrive später neu einrichten“. Mit „behalten“ funktioniert GhulDrive weiter. |
| B8 | GhulDrive ohne Speicher öffnen. | „Noch nicht eingerichtet“ mit der Speicherwahl direkt im Bereich. |
| B9 | GhulMail ohne Konto öffnen, Anbieter mit eigenem Benutzernamen einrichten. | Vollständiges Formular: Mailadresse, Passwort, Benutzername, IMAP und SMTP getrennt; bekannte Anbieter vorgeschlagen, alles änderbar. |
| B10 | Alte Suite (Version vor diesem Stand) mit Drive und Mail öffnen. | Drive und Mail funktionieren weiter; im Ordner liegen danach `Drive.ghldrv` und `Mail.ghlmail`. |
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
| 1 | Neue Suite mit Assistent: Sync-Server, alle Bereiche, Mailkonto im Assistenten, Fingerabdruck. Passwörter aus deinem alten Manager importieren. Windows: Suite vom Server holen. | Alles importiert oder als Notiz behalten? Beide Geräte zeigen dasselbe? |
| 2 | Den Tag über nur mit GhulSuite anmelden (Autofill in Apps und Browser). 2FA-Codes aus GhulAuth nutzen. Abends ein Passwort ändern. | Schlägt Autofill passend vor? Kommt die Änderung auf Windows an? |
| 3 | Mails nur in GhulMail lesen und schreiben, auch mit Anhang und an jemanden mit PGP. Handy zwischendurch neu starten. | Kommt **jede** Mail sofort als Benachrichtigung, auch nach dem Neustart? Öffnet das Antippen die Mail? |
| 4 | Fotos und Dokumente in GhulDrive (auch Ordner, Umbenennen, Papierkorb), eine Datei teilen. Zwischen WLAN und mobilen Daten wechseln. | Uploads brechen nicht ab oder setzen fort; Freigabelink funktioniert. |
| 5 | Termine in GhulCal, auch wiederkehrend und mit Erinnerung; einen davon auf Windows ändern. Sofortsperre durch Schütteln testen (in der App und im Hintergrund). | Erinnerung kommt pünktlich; Schütteln sperrt beim ersten Versuch, Mails und Erinnerungen kommen danach weiter. |
| 6 | Beide Geräte gleichzeitig benutzen: dieselbe Anmeldung auf beiden ändern, offline (Flugmodus) Einträge anlegen, später wieder online. | Nach dem Synchronisieren nichts verloren, nichts doppelt. |
| 7 | Sicherung erstellen, auf einem zweiten Gerät oder in einem leeren Ordner wiederherstellen. Master-Passwort ändern; das andere Gerät fragt einmal nach dem neuen. | Wiederhergestellt ist alles da; danach weiter ganz normal. |

## 27. Stresstest (von Hand)

| Nr. | Schritt | Erwartet |
|---|---|---|
| S1 | 5.000 Anmeldungen importieren (z. B. große CSV aus einem anderen Passwortmanager). | Import in unter einer Minute; Liste scrollt flüssig; Suche antwortet sofort. |
| S2 | 200 2FA-Codes. | Codes laufen flüssig, keine Ruckler beim Wechsel der 30-Sekunden-Runde. |
| S3 | Ein Video über 500 MB in GhulDrive hochladen, währenddessen App in den Hintergrund und WLAN aus/an. | Upload setzt fort; danach herunterladen, Datei ist unversehrt. |
| S4 | Ein Postfach mit über 10.000 Mails öffnen, schnell scrollen, suchen. | Liste lädt nach, nichts friert ein. |
| S5 | 50-mal schnell hintereinander sperren (Knopf) und entsperren (Fingerabdruck). | Kein Absturz, kein „hängendes“ Entsperren. |
| S6 | 20-mal kurz hintereinander synchronisieren, auf beiden Geräten gleichzeitig. | Keine Fehlermeldung, keine doppelten Einträge. |
| S7 | Handy mit wenig freiem Speicher (unter 500 MB) benutzen. | Klare Meldung statt Absturz, keine kaputten Dateien. |

Automatisch läuft bei jedem Push der Alltagstest im Kern (zwei Geräte, 30 simulierte Tage mit allen Bereichen) und in einem eigenen Job der Stresstest (5.000 Anmeldungen, 2.000 Termine, 300 Dateien und 64 MB, 20 Sync-Runden, 8 parallele Threads).

## 28. Sicherheitspunkte und Speicherort (Runde 5)

| Nr. | Schritt | Erwartet |
|---|---|---|
| R1 | Optionen → Tresore → „Speicherort wechseln“ → „Ordner wählen“, einen leeren Ordner wählen. Danach entsperren. | Meldung „… Dateien kopiert und geprüft“, Entsperren nötig, danach alles da; „Speicherort:“ zeigt den neuen Ordner. Im alten Ort liegen die Dateien noch. |
| R2 | Dasselbe noch einmal mit einem Ordner, in dem schon eine Suite liegt. | Meldung „Im gewählten Ordner liegt schon eine Suite …“, nichts überschrieben, der alte Ort bleibt aktiv. |
| R3 | Aus dem eigenen Ordner „In den App-Speicher“ wählen. | Wie R1; „Speicherort: App-Speicher“. |
| R4 | Im eigenen Ordner eine Tresordatei umbenennen (z. B. in `Urlaub.ghlpss`), dann entsperren. | Tresor ist vollständig da, die Datei heißt wieder nach ihrer Kennung. |
| R5 | Android-Einstellung „alle Inhalte anzeigen“, Handy mit PIN sperren; Mail und Termin-Erinnerung kommen lassen. Danach entsperren und noch eine Mail kommen lassen. | Gesperrt eingetroffen: nur die in GhulSuite gewählte Sperrbildschirm-Stufe (Mail: Kontoname und Anzahl; Termin: wie in den Kalender-Optionen gewählt), auch nach dem Entsperren in der Leiste. Entsperrt eingetroffen: mit Details (B4, F43). |
| R6 | Optionen → Sperren → „Sofortsperre stoppt auch Mail-Benachrichtigungen“ an; schütteln; Mail schicken; dann entsperren. | Gesperrt keine Mail-Benachrichtigung; nach dem Entsperren kommen sie wieder (B5). |
| R7 | Eine `.ics`-Datei aus einer anderen App (z. B. Dateien) mit GhulSuite öffnen. | Vorschau mit Anzahl und ersten Terminen, Import erst nach „Importieren“ (B6). |
| R8 | App sperren, Handy 20 Minuten in den Standby (Bildschirm aus), dann App wieder öffnen. | Gesperrt, auch wenn das Handy zwischendurch tief geschlafen hat (B8). |
| R9 | In einer App ohne Verknüpfung über Autofill eine Anmeldung von Hand wählen. | Rückfrage mit App-Name und Paket: „Verknüpfen und ausfüllen“ oder „Nur diesmal ausfüllen“. Nur beim Verknüpfen wird sie beim nächsten Mal direkt angeboten (B9). |
| R10 | Eine App mit Bildschirm-Overlay (z. B. Blaulichtfilter) einschalten und in GhulSuite entsperren bzw. Autofill wählen. | Tipps werden ignoriert, solange etwas darüberliegt (L10); Overlay aus, dann geht es. |
| R11 | Zwei Geräte mit Sync: auf A einen Eintrag löschen, auf B (offline) denselben Eintrag ändern; dann beide synchronisieren. | Der geänderte Eintrag ist auf beiden Geräten wieder da (B18). Andersherum (B kannte die Löschung): bleibt gelöscht. |
| R12 | Windows: eine Drive-Datei öffnen, das Programm offen lassen, GhulSuite sperren; Programm schließen, eine Minute warten. | Der Weitergabe-Ordner ist danach leer (B10). Portabel: der Ordner liegt in `GhulSuite-Daten`, nicht im Windows-Temp. |
| R13 | Optionen → Sicherheit: Hinweis „Etwas verloren …“ lesen. | Hinweis nennt: entfernen bzw. neues Notfallblatt, alte Sicherungen, Mail- und Server-Passwörter wechseln (L1). |
| R14 | GhulMail: bei einem Kontakt mit PGP-Schlüssel „Vergleichen“ öffnen. | Fingerabdruck in Vierergruppen; nach dem Bestätigen gilt er als geprüft; ein später geänderter Schlüssel wird gemeldet (L7). |
| R15 | Drive-Server auf einen alten Stand zurücksetzen (alte Index-Datei zurückkopieren), dann GhulDrive öffnen. | Meldung „Drive auf dem Server ist älter …“, nichts verschwindet; „Älteren Serverstand übernehmen“ (Optionen → GhulDrive) nur nach Rückfrage (B3). |

## 29. Bedienung und Mail (Runde 6)

| Nr. | Schritt | Erwartet |
|---|---|---|
| V1 | Optionen → Bereiche → einen Bereich entfernen; ebenso Optionen → Tresore → einen Tresor löschen. | Dialog nennt die Anzahl der Einträge, bietet „Vorher exportieren“; „Entfernen“ bzw. „Löschen“ geht erst, wenn der Name eingetippt ist (U4). |
| V2 | Etwas Falsches eingeben, das der Kern ablehnt (z. B. beim Server-Abruf FTPS ohne Passwort, oder IMAP mit Port 143). | Meldung auf Deutsch, kein englischer Rest (U5). |
| V3 | Startbildschirm, obwohl schon eine Suite da ist: „Sicherung wiederherstellen“. | Frage „Hier liegt schon eine Suite“, dann einen leeren Ordner wählen; vorhandene Suite bleibt unverändert (U6). |
| V4 | Neues Gerät: „Von meinem Server holen“ mit FTPS, dann mit dem PHP-Modul (Adresse und Zugangsschlüssel). | Suite wird geladen, danach normal entsperren (U7). |
| V5 | Einrichtung: Key wählen, keine PIN eingeben, „Weiter“. | Meldung „Bitte gib die PIN deines Keys ein …“ (U9). |
| V6 | Startbildschirm: „Einzelne Tresordatei öffnen“ mit einer `.ghlcal`-Datei. | Wird als Kalender erkannt, GhulCal ist danach eingeschaltet (U10). |
| V7 | „Mit Wiederherstellungscode öffnen“. | Danach Dialog mit den drei Schritten; „Jetzt erledigen“ öffnet die Keys (U11). |
| V8 | Anmeldung bearbeiten, etwas ändern, Zurück; danach etwas ändern und die App sperren lassen. | Zurück fragt „Änderungen verwerfen?“; nach dem Entsperren die Meldung, dass eine Bearbeitung verworfen wurde (U12). Ebenso im Termin-Editor. |
| V9 | Flugmodus, Synchronisation automatisch an, App entsperren. | Auf den Bereichsseiten der Hinweis „Synchronisation gerade nicht möglich …“; Tippen versucht es erneut; mit Netz verschwindet er (U13). |
| V10 | Optionen → GhulDrive → Speicher von eigenem Server auf „wie Synchronisation“. | Rückfrage mit Erklärung, erst dann umgestellt (U14). |
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
| K12 | Zweites Gerät synchronisieren, dort einen Termin ändern, zurück synchronisieren. | Änderungen beider Seiten kommen an (Zusammenführen pro Termin). |
| K13 | Mail-Benachrichtigung antippen (gesperrt und entsperrt). | Die neue Mail öffnet sich (nach dem Entsperren); Leiste und App-Symbol zeigen die Zahl ungelesener Mails, vollständig, nicht angeschnitten, ab 100 „99+“. Windows-Taskleiste: Briefumschlag mit Zahl. |
| K14 | Eine `.ics` importieren, die Zeiten in UTC (`…Z`, z. B. Google-Export) und in einer anderen Zeitzone (`TZID=America/New_York`) enthält, je ein Termin im Sommer und im Winter. | Alle Termine stehen auf der Stunde, zu der sie hier stattfinden (10:00 New York = 16:00 in Deutschland), unabhängig davon, ob gerade Sommer- oder Winterzeit ist. Ganztägige Termine bleiben auf ihrem Tag. |
| K15 | Termin mit Erinnerung anlegen, App schließen, in den Systemeinstellungen die Zeitzone ändern (z. B. London). | Die Erinnerung kommt zur selben Uhrzeit der neuen Zone (Termin 10:00 → Erinnerung 09:45 Londoner Zeit), ohne dass die App geöffnet wurde. |
| K16 | Eine Serie aus einer anderen Zeitzone importieren (z. B. Einladung „jeden Dienstag 10:00 New York“ aus Outlook oder Google) und durch das Jahr blättern: Januar, Mitte März, April, Ende Oktober, November. | In Deutschland steht der Termin um 16:00, in den Wochen, in denen nur die USA oder nur Europa Sommerzeit haben, um 15:00. Die Termin-Ansicht zeigt zusätzlich „Ortszeit in New York: 10:00 - 11:00“; beim Bearbeiten steht unter „Zeitzone“ der Ort. „Nur diesen Termin löschen“ entfernt genau den angezeigten Tag. |
| K17 | Einen einzelnen Eintrag löschen: GhulPass-Eintrag, GhulAuth-Konto, Drive-Datei, Mail, Termin. Einmal „Rückgängig“ tippen, einmal die Meldung ablaufen lassen; einmal löschen und sofort sperren. | Ohne Rückfrage weg, Meldung mit „Rückgängig“. Rückgängig: wieder da, nichts passiert. Abgelaufen: endgültig gelöscht (nach Sperren/Entsperren weiterhin weg). Sofort gesperrt: die Löschung wird vor dem Sperren ausgeführt. Sofortsperre direkt nach dem Löschen: der Eintrag bleibt. |
| K18 | Ganzes löschen: Tresor, Bereich, Mailkonto, Server, Papierkorb leeren, mehrere endgültig löschen. | Immer eine Rückfrage mit rotem Knopf, Warnung als Hinweisfeld wie bei „Bereich entfernen“. |
| K19 | Breites Fenster (Windows oder Tablet quer): Termin öffnen, Termin bearbeiten, in GhulAuth „Konto hinzufügen“. | Öffnet rechts in der zweiten Spalte, die Liste bleibt links stehen. Ohne Auswahl rechts „Kein Termin geöffnet“ bzw. „Kein Konto geöffnet“. |
| K20 | Verschlüsselte Mail in der Liste. | Schloss und „Verschlüsselte Nachricht“ an der Stelle des Betreffs (kein „...“); geöffnet Betreff und Text. |

## 30. Mail-Archiv

Vorher: ein Mail-Konto mit einigen Mails, die älter als 30 Tage sind (am besten ein Testkonto), und ein eigener Server
oder die Synchronisation. **Bitte nicht mit dem einzigen Postfach eines wichtigen Kontos beginnen.**

| Nr. | Schritt | Erwartet |
|---|---|---|
| A1 | Ein neues Mail-Konto hinzufügen, solange kein Archiv eingerichtet ist. | Frage „Wo sollen deine Mails liegen?“; „Beim Anbieter lassen“ ändert nichts, „Verschlüsselt archivieren“ öffnet die Mail-Optionen. |
| A2 | Optionen → GhulMail → Mail-Archiv: „Wie Synchronisation“ oder „Eigener Server“ wählen. | „Mail-Archiv eingerichtet.“; Ort wird angezeigt. Auf dem Server liegt ein Ordner `mailarchive` mit unlesbaren Dateinamen. |
| A3 | Das Konto einschalten, Frist 30 Tage, „Jetzt archivieren“. | Meldung „N Mails archiviert, 0 beim Anbieter entfernt.“; beim Anbieter (Webmail) sind alle Mails noch da. |
| A4 | Optionen: „Nach dem Archivieren beim Anbieter löschen“ einschalten. | Erst eine Warnung mit „Einschalten“; danach beim nächsten Lauf die Frage „Alles sicher archiviert … N Mails entfernen?“. Danebentippen = Später: nichts wird gelöscht. |
| A5 | Die Frage mit „N Mails entfernen“ bestätigen. | Beim Anbieter fehlen genau die alten Mails; neue bleiben. In GhulMail erscheinen sie unter „Archiv · N“ unterhalb der Mails des Anbieters. |
| A6 | Eine archivierte Mail öffnen, Anhang speichern, antworten, als `.eml` speichern, löschen. | Alles wie bei normalen Mails; Löschen legt sie in den Archiv-Papierkorb (Rückgängig möglich), dort wiederherstellbar. |
| A7 | Zweites Gerät mit derselben Suite: GhulMail öffnen. | Dieselben archivierten Mails sind dort zu sehen und zu lesen. |
| A8 | „Vollständige Archivkopie auf diesem Gerät“ einschalten, dann Flugmodus, eine archivierte Mail öffnen. | Öffnet ohne Netz. |
| A9 | Den Ordner `mailarchive` auf dem Server umbenennen, „Jetzt archivieren“. | Hinweis „Mail-Archiv nicht erreichbar“ mit „Erneut versuchen“ und „Archiv-Ort ändern“; beim Anbieter wird nichts gelöscht. |
| A10 | „Archiv-Ort ändern“ auf einen leeren Ordner, während das alte Archiv noch erreichbar ist. | Frage „Archiv hierher umziehen?“; nach „Umziehen“ ist alles am neuen Ort. |
| A11 | Archiv-Ort auf einen leeren Ordner, das alte ist weg; auf dem Gerät mit vollständiger Kopie „Archiv aus dieser Kopie wiederherstellen“. | Meldung „N Mails ins Archiv wiederhergestellt.“; die anderen Geräte sehen sie wieder. |
| A12 | „Kopie auf diesem Gerät löschen“, während kein anderes Gerät eine vollständige Kopie hat. | Warnung, dass es die einzige Kopie sein könnte; Löschen erst nach Eintippen von „löschen“. |
| A13 | Spam-Ordner auf „Nach der Frist beim Anbieter löschen“. | Erst eine Warnung; danach sind alte Spam-Mails beim Anbieter weg, neue bleiben. |
| A14 | Auf zwei Geräten gleichzeitig „Jetzt archivieren“. | Eines meldet „Ein anderes Gerät archiviert dieses Konto gerade“; nichts doppelt. |

## Fehler melden

Bitte je Fehler: **Testnummer**, Handy und Android-Version, App-Version (Optionen → Über GhulSuite), was du gemacht hast,
was erwartet war, was passiert ist, ggf. Bildschirmfoto (dafür vorübergehend „Bildschirmfotos erlauben“ einschalten).
