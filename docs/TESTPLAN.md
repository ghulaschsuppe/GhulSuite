# GhulSuite: Testplan Android (Version 0.5.0-dev)

**Stand:** 20.11.2026 · **neu: Umbau zu GhulDocs** ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)): eine reine
Offline-App mit GhulPass, GhulAuth, GhulDocs und GhulCal. GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38, die
Suite-Datei mit Dokumenten Abschnitt 35, einzelne Tresore Abschnitt 6. Die Abschnitte zu GhulMail, GhulDrive, Servern,
Freigabelinks, Mail-Archiv und Schlüsselwechsel (12 bis 15, 17 bis 20, 32, 34, 36, 37) sind entfallen und bleiben
nur als Überschrift stehen, damit die Nummern gleich bleiben. Davor entfallen (Umbau ohne Synchronisation,
[`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)): Synchronisation, FTPS, Sicherung/Wiederherstellung und „Von meinem Server
holen“.

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
| V6 | Für die Abschnitte 35 und 39: ein **zweites Gerät** (zweites Android-Gerät oder Windows-PC mit GhulSuite) und ein paar Testdokumente (PDF, Fotos, Text, eine große Datei von mehreren hundert MB). Für 38: ein paar Blätter Papier. |

---

## 1. Erster Start und Einrichtung

Der Assistent hat acht Schritte (der letzte legt die Suite-Datei an); oben steht „Schritt n von 8“ mit Fortschrittsbalken. Die Suite entsteht beim
Verlassen von Schritt 3 im App-Speicher, danach geht „Zurück“ nur noch bis Schritt 4.

| Nr. | Schritt | Erwartet |
|---|---|---|
| E1 | App öffnen. | Startbildschirm mit „Neu einrichten“, „Suite-Datei öffnen“ (.ghlzip, z. B. von einem USB-Stick) und „Einzelnen Tresor öffnen“. Kein „Von meinem Server holen“, kein „Sicherung wiederherstellen“. |
| E2 | „Neu einrichten“. | Schritt 1 von 8 · Master-Passwort. |
| E3 | Passwort „abc“ eingeben, Weiter. Dann ein schwaches 10-Zeichen-Passwort. Dann zwei verschiedene Passwörter. | Jeweils klare Meldung: zu kurz / zu leicht zu erraten / stimmen nicht überein. |
| E4 | „Passwort vorschlagen“ antippen, Wiederholung eingeben, Weiter. | Langes Zufallspasswort, Stärkebalken voll; Schritt 2 · Zweiter Faktor. |
| E5 | „Security Key“ gewählt lassen, Name des Keys, Key-PIN eingeben (Android: „Key prüfen“ zeigt „Erkannt: …“). Ohne Backup Weiter; dann Backup „Schlüsseldatei“ anlegen und speichern bzw. „Zweiter Key“ mit Name und PIN. Weiter. | Ohne Backup geht es nicht weiter („Backup für den Key (Pflicht)“). Mit Backup Schritt 3 · Anlegen: erst der erste Key (zweimal berühren: Anlegen, Bestätigen), beim zweiten Key danach dieser, ebenfalls zweimal. Unter Optionen → Schlüssel stehen beide, beide entsperren gleich. |
| E6 | „Anlegen“, Key **zweimal** berühren; bei „Zweiter Key“ danach diesen einstecken und **zweimal** berühren. Zur Gegenprobe: als zweiten Key noch einmal den ersten nehmen. | Schritt 4 · Bereiche; kein „Zurück“ zu den Schritten 1 bis 3 mehr. Derselbe Key als Backup: „schon vorhanden“, die Suite ist angelegt, das Backup wird erneut verlangt (anderer Key oder Schlüsseldatei). |
| E7 | Alle vier Bereiche einschalten. | Namen für GhulPass, GhulAuth, GhulDocs und GhulCal änderbar; kein Bereich fragt nach einem Server oder Konto. |
| E11 | Sperrzeit wählen, Schalter für Bildschirm aus und Schütteln. Weiter. | Schritt 6 · Entsperren mit Hinweis, dass „Schnell entsperren“ Fingerabdruck und App-PIN meint. |
| E12 | „Zusätzlich Fingerabdruck“, Gültigkeit „Immer“. Weiter. | Hinweis: ohne Ablauf, aber nach Neustart, Sofortsperre oder Passwortwechsel wieder Passwort und Key. Fingerabdruck-Abfrage; Schritt 7. |
| E13 | Code ansehen, „Notfallblatt drucken“ → „Als PDF speichern“. | PDF mit Code, QR-Code und Anleitung. PDF danach vom Handy löschen. |
| E14 | „Ich habe den Code notiert“ → Weiter → falsche Gruppe 3 eingeben. | Meldung „Das ist nicht die dritte Gruppe“. Es gibt kein „Später“. |
| E15 | Richtige Gruppe 3 eingeben → Abschließen. | Hauptansicht mit allen vier Bereichen und Optionen; GhulDocs zeigt einen leeren Tresor mit Belegung „0 B von 1 GB“ (bei nur einem GhulDocs-Tresor ohne die Zeile „Alle GhulDocs-Tresore …“). Oben der Hinweis „Nicht gespeicherte Änderungen“. |
| E18 | Einrichtung mit „Schlüsseldatei“ statt Key: Datei erstellen und z. B. auf den USB-Stick speichern. | Ohne gespeicherte Datei geht es nicht weiter. Danach wie E6 bis E15 (ohne Key-Berührung). |
| E19 | Nach E15: Optionen → Suite-Datei → „Speichern unter …“ → auf den USB-Stick. | Datei `GhulSuite.ghlzip` entsteht; „Gespeichert in …“; der Hinweis „Nicht gespeicherte Änderungen“ verschwindet. |
| E19a | Etwas ändern (z. B. einen Eintrag anlegen), nicht speichern, sperren. | Der Entsperren-Bildschirm sagt „Es gibt nicht gespeicherte Änderungen. Entsperre, um sie in der Suite-Datei zu speichern.“ und ist nicht antippbar; nach dem Entsperren steht oben in GhulDocs „Nicht gespeicherte Änderungen. Tippen zum Speichern.“, Tippen führt zu Optionen → Suite-Datei. |
| E19b | **Speichern-Button** nach E19: einen Eintrag anlegen, ein Dokument scannen; oben auf das Speichern-Symbol tippen (Windows: Seitenleiste oder Strg+S), „Speichern“. Dann den Stick abziehen und noch einmal speichern; Stick wieder einstecken, „Erneut versuchen“. Zum Schluss die Datei auf einem zweiten Gerät öffnen, dort etwas ändern und speichern, Stick zurück, hier etwas ändern und speichern. | Das Symbol (mit Punkt) und der Punkt an Optionen erscheinen nach jeder Änderung, nichts wird von selbst geschrieben. Der Dialog zeigt Datei, „Zuletzt gespeichert“, den Hinweis zum Stick und den Fortschritt in Prozent, dann „Gespeichert in …“; das Symbol verschwindet. Ohne Stick: „Die Suite-Datei ist gerade nicht erreichbar …“ mit „Erneut versuchen“. Anderswo gespeichert: Gerät und Zeit, „Meine als neue Datei sichern …“, „Überschreiben“, „Abbrechen“; die Datei bleibt unberührt, bis du wählst. |
| E19c | Sperren mitten im Speichern (große Suite, langsamer Stick): speichern, „Im Hintergrund weiter“, dann sperren; Android auch Bildschirm aus. Danach entsperren. Außerdem: Windows-Fenster schließen mit ungespeicherten Änderungen, einmal entsperrt, einmal gesperrt, einmal während eines Speicherns. | Gesperrt: „Die Suite-Datei wird noch gespeichert (… %). USB-Stick nicht abziehen.“, Android zeigt eine Benachrichtigung ohne Namen; danach ist alles gespeichert (kein Symbol, die Datei lässt sich auf einem anderen Gerät öffnen). Schließen: „Ungespeicherte Änderungen“ mit „Speichern“ (danach beendet sich die App) und „Ohne Speichern beenden“; gesperrt „Entsperren“; beim Speichern „Nach dem Speichern beenden“. Kein `.ghltmp` bleibt neben der Datei. |
| E19d | Einrichtung und Erinnerung: neue Suite einrichten bis Schritt 8 „Suite-Datei“, „Ort wählen …“ einmal abbrechen, einmal einen nicht beschreibbaren Ort (Stick ziehen), dann einen guten Ort. Danach etwas ändern, drei Tage nicht speichern (oder Gerätezeit vorstellen). Eine andere Suite-Datei öffnen, während etwas ungespeichert ist. | Schritt 8 erklärt Arbeitskopie und Suite-Datei; ohne Datei geht es nicht weiter, außer nach einem gescheiterten Speichern mit Warnung („Ohne Suite-Datei weiter“). Mit gutem Ort: „Gespeichert in …“, die App startet, kein Speichern-Symbol. Nach drei Tagen über den Listen „Seit … nicht in der Suite-Datei gespeichert.“. Beim Öffnen der anderen Datei gibt es „Erst speichern“. |
| E20 | Nur GhulAuth wählen und einrichten. | In der Leiste nur GhulAuth und Optionen; Optionen → Bereiche zeigt nur GhulAuth als eingerichtet. |
| E22 | **Über eine vorhandene Suite:** Entsperren-Bildschirm → „Andere Suite oder Tresor öffnen“ → „Neu einrichten“, Schritte 1 und 2, dann Schritt 3. | Der Hinweis „Auf diesem Gerät gibt es schon eine Suite …“ steht **einmal** (keine zweite rote Meldung); „Key registrieren“ bzw. „Einrichten“ ist gesperrt. Drei Wege: „Diese Suite öffnen“, „Vorhandene sichern und ersetzen“, „Vorhandene löschen und ersetzen“. „Diese Suite öffnen“ führt zum Entsperren der vorhandenen Suite. |
| E23 | E22, dann „Vorhandene sichern und ersetzen“: Master-Passwort und Key der **vorhandenen** Suite, „Entsperren“; im Speichern-Dialog erst „Abbrechen“, dann einen Ort auf dem USB-Stick wählen. | Nach „Abbrechen“: „Nicht gesichert. Es wurde nichts gelöscht.“, die Suite ist noch da. Mit Ort: Fortschritt, „Vorhandene Suite gesichert als …“, danach direkt Schritt 4 · Bereiche der neuen Suite. Die gesicherte `.ghlzip` öffnet auf einem zweiten Gerät mit allen alten Einträgen und Dokumenten. Ein Ort, an dem eine Datei einer anderen Suite liegt, wird abgelehnt („Wähle einen neuen Namen …“). |
| E24 | E22, dann „Vorhandene löschen und ersetzen“: erst „Abbrechen“, dann „LÖSCHEN“ eintippen und „Löschen und ersetzen“. | Rückfrage in Rot mit Warnung (alle Tresore samt GhulDocs-Dokumenten und die Schnellentsperrung werden endgültig gelöscht; eine Suite-Datei an einem anderen Ort bleibt unberührt; Tipp, vorher zu sichern). Der Knopf geht erst mit dem Wort. „Abbrechen“ lässt alles, wie es war. Bestätigt: „Vorhandene Suite gelöscht.“, danach Schritt 4 der neuen Suite. Die alte Suite entsperrt nicht mehr: kein Fingerabdruck, keine App-PIN (auch nicht nach einem Neustart der App), keine Erinnerungen ihrer Termine; unter Optionen → Suite-Datei ist keine Datei vermerkt. |
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
| P7a | Benachrichtigungen aufziehen, während GhulSuite entsperrt ist. | Die dauerhafte Benachrichtigung „GhulSuite entsperrt“ zeigt das **Schutzschild**. Text: „Tippen zum Öffnen. Schütteln oder „Sperren“ sperrt sofort.“ |
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
| G8 | Drei Einträge lange drücken → im Auswahlkopf „Zu Favoriten“; dieselben (jetzt alle Favoriten) wählen → „Aus Favoriten“. Ebenso gemischt (ein Favorit, zwei nicht). | Alle gewählten Einträge werden auf einmal Favoriten bzw. keine mehr; Chip „Favoriten“ zeigt genau sie. Wie in GhulDocs. |
| G9 | Einen Eintrag öffnen und zurück, einen zweiten ebenso; oben „Zuletzt verwendet“ ansehen, auch mit großer Schrift. | Runde Pillen mit rundem Buchstaben-Symbol, Luft links und rechts; lange Namen enden mit „…“; Abstand zur Überschrift der Liste darunter. |

## 5. GhulAuth

| Nr. | Schritt | Erwartet |
|---|---|---|
| A1 | „+“ → QR-Code einer Test-2FA-Seite scannen (z. B. eine Demo-Seite für TOTP). | Felder vorausgefüllt, Konto hinzugefügt. |
| A2 | Codes mit einer anderen Authenticator-App oder der Website vergleichen. | Identisch, Countdown passt. |
| A3 | Manuell anlegen (Base32-Schlüssel), otpauth-Link einfügen. | Beides funktioniert, ungültiger Schlüssel wird abgelehnt. |
| A4 | Lang drücken → umbenennen, löschen. | Funktioniert. |
| A5 | „Codes verbergen“ und „Nächsten Code zeigen“ unter Optionen → GhulAuth. | Codes erst nach Antippen sichtbar; in den letzten Sekunden erscheint der nächste Code. |
| A6 | Google Authenticator (zweites Gerät): Konten exportieren, den Export-QR in GhulAuth scannen. | Alle Konten auf einmal übernommen, Meldung mit Anzahl. Erneut scannen: „schon vorhanden“. |
| A7 | Mehrere Konten lange drücken → „Zu Favoriten“, danach „Aus Favoriten“. | Alle gewählten Konten auf einmal (nicht nur eines); Favoriten oben. Wie in GhulPass und GhulDocs. |

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
| T6 | „Nur diesen Tresor öffnen“ → Passwort und Key bzw. Schlüsseldatei **des Tresors**. | Oben „Einzeln geöffnet: … Deine Suite bleibt gesperrt. Jede Änderung wird direkt in diese Datei zurückgeschrieben.“ Nur dieser Tresor ist zu sehen. Schutz, weitere Tresore und Suite-Datei lassen sich in diesem Modus nicht ändern. |
| T7 | Im einzeln geöffneten Tresor einen Eintrag anlegen und einen ändern; dann „Tresor schließen“. Die Datei erneut einzeln öffnen. | Nach jeder Änderung kurz „Die Änderungen werden in die Datei geschrieben …“, dann „Alles ist in der Datei.“ Nach dem erneuten Öffnen sind beide Änderungen da. Die eigene Suite ist unverändert. |
| T8 | Einzeln geöffnet: USB-Stick abziehen, einen Eintrag ändern. Dann den Stick wieder einstecken und „Erneut versuchen“. | Warnstreifen „Die Datei ließ sich nicht schreiben (ist der Speicher noch da?). Deine Änderungen sind noch hier.“ Nach dem Einstecken wird geschrieben; nichts geht verloren. Bleibt der Stick weg, kommen die Änderungen beim nächsten Mal als „Änderungen nicht in der Datei“ wieder (jetzt schreiben, als neue Datei speichern oder verwerfen). |
| T9 | Einzeln geöffnet: Dieselbe Tresordatei auf einem zweiten Gerät ändern (oder durch eine andere Kopie ersetzen), dann hier etwas ändern. | „Die Datei wurde seit dem Öffnen anderswo geändert. Mit diesem Tresor überschreiben oder ihn als neue Datei speichern.“ Nichts wird still überschrieben. |
| T10 | „Einzelnen Tresor öffnen“ mit einem GhulDocs-Paket (`.ghldocs`, siehe GD12). | Der Tresor öffnet sich mit seinen Dokumenten (geprüft wie eine Suite-Datei); „Nur diesen Tresor öffnen“ und „Zu meiner Suite hinzufügen“ gehen (er wird ein eigener GhulDocs-Tresor, auch wenn die Suite schon GhulDocs hat, siehe GD13c). |
| T11 | „Zu meiner Suite hinzufügen“ mit einem Tresor einer **anderen** Suite (anderes Passwort). | „Nach dem Entsperren wird … zu deiner Suite hinzugefügt.“ Nach dem Entsperren der eigenen Suite einmal Passwort und Key bzw. Schlüsseldatei des Tresors; danach gehört er dazu und öffnet sich mit deinem Passwort, deinen Keys und deinem Wiederherstellungscode. Die Ausgangsdatei öffnet weiterhin nur ihren alten Stand. Hinweis „Nicht gespeicherte Änderungen“. |
| T12 | Auf einem Gerät **ohne** Suite: „Einzelnen Tresor öffnen“ → „Neue Suite damit anlegen“. | Öffnet mit Passwort + Key des Tresors; Hinweis „dein bisheriges Notfallblatt gilt weiter“ (bzw. neuer Code, wenn der Tresor keinen hatte); nur dieser Bereich ist eingeschaltet. |
| T13 | GhulPass: zwei Einträge lange drücken → Auswahlkopf → „Als Tresor weitergeben“. Name, neues Passwort, zweiter Faktor „Neue Schlüsseldatei“, zur Bestätigung dein eigenes Master-Passwort (erst falsch, dann richtig). | Falsches Master-Passwort: abgelehnt. Richtig: „Speichere jetzt die neue Schlüsseldatei …“, dann „Tresordatei speichern“, dann der Wiederherstellungscode des neuen Tresors (nur jetzt angezeigt). Am Ende „Fertig. Gib die Tresordatei, das Passwort und gegebenenfalls die Schlüsseldatei getrennt weiter; den Wiederherstellungscode bewahrt der Empfänger sicher auf.“ |
| T14 | Den Tresor aus T13 auf dem zweiten Gerät mit „Nur diesen Tresor öffnen“ öffnen: erst mit deinem eigenen Passwort und Key, dann mit dem neuen Passwort und der neuen Schlüsseldatei, dann mit seinem Wiederherstellungscode. | Eigener Schutz: öffnet nicht. Neuer Schutz: genau die zwei Einträge, ohne Passwort-Verlauf. Code: öffnet ebenfalls. |
| T15 | Wie T13 in GhulAuth, zweiter Faktor „Security Key“ (zweiter Key). | Zwei Berührungen; der neue Tresor öffnet nur mit seinem Passwort und diesem Key. |
| T16 | **Ganzen Tresor weitergeben, alle vier Bereiche:** Optionen → Tresore, den Hinweis über der Liste lesen; ⋮ an einem Tresor öffnen und die Untertitel von „Exportieren“ und „Als Tresor weitergeben“ lesen. Dann „Als Tresor weitergeben“ je einmal für einen GhulPass-, einen GhulAuth-, einen GhulDocs- und einen GhulCal-Tresor (eigenes Passwort, abwechselnd neue Schlüsseldatei und Security Key, eigenes Master-Passwort zur Bestätigung). Jede Datei auf einem zweiten Gerät mit „Nur diesen Tresor öffnen“ und mit „Zu meiner Suite hinzufügen“ öffnen. Zum Schluss einen **leeren** Tresor (neu angelegt) weitergeben. | Hinweis: „Exportieren“ ist für dich (dein Schutz, Sicherung oder anderes Gerät), „Als Tresor weitergeben“ für jemand anderen (eigenes Passwort, eigener zweiter Faktor, eigener Wiederherstellungscode). Untertitel „Für dich, verschlüsselt mit deinem Passwort und Key bzw. deiner Schlüsseldatei: als Sicherung oder für ein anderes Gerät“ bzw. „Für jemand anderen: …“. Der Name ist mit dem Tresornamen vorbelegt; der Text nennt „Der ganze Tresor …“. Ablauf wie T13 (Schlüsseldatei bzw. zwei Berührungen, Code nur einmal angezeigt, „Fertig. Gib die Tresordatei, das Passwort und gegebenenfalls die Schlüsseldatei getrennt weiter; den Wiederherstellungscode bewahrt der Empfänger sicher auf.“). GhulDocs: Speichern-Dialog schlägt `.ghldocs` vor, alle Dokumente ohne Papierkorb. Auf dem zweiten Gerät: alle Einträge bzw. Codes, Dokumente, Kalender mit ihren Terminen (jeder Termin in seinem Kalender, mit Farbe); GhulPass ohne Passwort-Verlauf. Mit dem eigenen Schutz öffnet keine der Dateien. Der leere Tresor: es entsteht ein leerer Tresor, der so öffnet (kein „Nicht gefunden“). |

## 7. Schutz und Wiederherstellung

| Nr. | Schritt | Erwartet |
|---|---|---|
| S1 | Optionen → Sicherheitsschlüssel & Schlüsseldateien → „Security Key hinzufügen“: Passwort und Key-PIN des vorhandenen Keys (oder Schlüsseldatei) angeben, vorhandenen Key berühren, dann den zweiten Key (zwei Berührungen). Gegenprobe: denselben Key noch einmal hinzufügen. | Ohne beide Faktoren: abgelehnt. Derselbe Key: „schon vorhanden“. Zweiter Key in der Liste; beide entsperren. Code bleibt gültig. **Kein** Schlüsselwechsel und keine Aufforderung zum Speichern der Suite-Datei (Hinzufügen ist keine Schutzänderung, die etwas ungültig macht); nur der Hinweis „Nicht gespeicherte Änderungen“. |
| S2 | „Schlüsseldatei hinzufügen“ (erstellen und speichern). | Datei in der Liste; Entsperren damit möglich (U6). |
| S3 | Schlüsseldatei entfernen, später den letzten Key entfernen versuchen. | Entfernen geht; der letzte Key/die letzte Datei lässt sich **nicht** entfernen. |
| S4 | Suite mit Schlüsseldatei (E18): Key hinzufügen, dann Datei entfernen versuchen. Dann eine zweite Schlüsseldatei hinzufügen und die erste entfernen. | Erst abgelehnt: „Ein Security Key braucht ein Backup …“. Danach: Key und zweite Datei funktionieren, die erste nicht mehr, **alter Wiederherstellungscode funktioniert weiterhin**. |
| S5 | „Neuer Wiederherstellungscode“. Sperren, alten Code probieren, dann neuen. | Alter Code: ungültig. Neuer: geht. |
| S6 | Passwort ändern (aktuelles falsch, dann richtig; dazu Key-PIN und Key oder die Schlüsseldatei, einmal auch ohne zweiten Faktor). | Ohne zweiten Faktor oder falsch: Meldung. Richtig: neues Passwort gilt, Fingerabdruck/PIN-Zugang ist neu einzurichten. Danach Dialog „Suite-Datei jetzt speichern“ (S12). |
| S7 | Vor S5: die Suite-Datei speichern und eine Kopie davon aufheben. Nach S5 die Suite-Datei neu speichern. Auf einem Gerät ohne Suite die **alte** Kopie mit dem **alten** Code öffnen; dann die **neue** Datei mit dem alten Code. | Die alte Kopie öffnet sich (Stand von damals, Hinweis, dass ohne Passwort nicht geprüft werden konnte). Die neue Datei öffnet sich mit dem alten Code nicht. Fingerabdruck/PIN funktionieren nach S5 weiter. |
| S8 | Zwei Keys und eine Schlüsseldatei. Suite-Datei speichern und eine Kopie aufheben, dann einen Key entfernen (Passwort und der andere Key oder die Datei als Nachweis) und neu speichern. Mit dem entfernten Key + Passwort die Kopie öffnen, dann die neue Datei. Auf dem ersten Gerät danach über „Speichern“ die Kopie als Ziel wählen. | Kopie: öffnet sich (auf einem Gerät, das den neueren Stand nie gesehen hat). Neue Datei: „Key nicht eingetragen“. Der verbliebene Key und die Datei öffnen alles; der entfernte Key musste dafür nicht berührt werden. Fingerabdruck/PIN ist neu einzurichten. Ein Gerät mit dem alten Schutz, das die neue Datei überschreiben will: „anderer Schutz“, kein „Überschreiben“, nur „als neue Datei“. |
| S11 | Die Suite-Datei mit einem ZIP-Programm (z. B. Dateien-App oder 7-Zip) ansehen. | Darin nur `suite.ghlsuite`, `export.ghlexp` und Tresordateien, die nach ihrer Kennung heißen (z. B. `3f9a1c2b7d4e5f60.ghlpss`); kein Tresorname, kein lesbarer Inhalt. |
| S12 | Nach S3 (Datei entfernt), S5 und S6 jeweils auf den Dialog achten. | Jedes Mal „Suite-Datei jetzt speichern“: Der Schutz wurde geändert; ältere Kopien der Suite-Datei sollen gelöscht werden. Eine alte Kopie öffnet sich auf diesem Gerät nicht mehr (SD14); anderswo nur mit dem alten Passwort **und** dem alten zweiten Faktor. „Jetzt speichern“ führt zu Optionen → Suite-Datei; „Später“ schließt ihn, der Hinweis „Nicht gespeicherte Änderungen“ bleibt. |
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
| O4 | Flugmodus an, alles Obige kurz wiederholen (Entsperren, Codes, GhulPass, GhulAuth, GhulDocs, GhulCal). | Funktioniert vollständig offline. Nur die Update-Prüfung und die Prüfung auf Datenlecks melden verständlich, dass kein Netz da ist. |
| O5 | Handy drehen in Listen, Bearbeiten und Dialogen. | Kein Datenverlust, keine Abstürze. |

## 12. Eigene Server (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 13. GhulDrive (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 14. GhulMail (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 15. Benachrichtigungen (Mail) (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 16. Neues Design und Einstellungen

| Nr. | Schritt | Erwartet |
|---|---|---|
| DS1 | Durch alle Tabs blättern. | Überall dieselbe Kopfzeile: Bereichsname, darunter farbige Kontextzeile (Tresor, Speicherort bzw. „Konto · Ordner“) mit Pfeil; rechts das Schloss. |
| DS2 | Kontextzeile in GhulPass antippen. | Auswahl „Tresor wählen“ (Alle / einzelne Tresore mit Anzahl Einträgen, „Tresore“). Wahl wirkt sofort. |
| DS3 | Kontextzeile in GhulDocs und GhulCal antippen (mit zwei GhulDocs-Tresoren). | GhulDocs: wie GhulPass „Tresor wählen“ (Alle Tresore / einzelne GhulDocs-Tresore, „Tresore“); mit nur einem GhulDocs-Tresor steht sein Name. Nach einem Wechsel zu „Alle Tresore“ nennt die Kopfzeile „Alle Tresore“, nicht den zuvor betretenen Tresor. GhulCal: die Kalender. Wahl wirkt sofort. |
| DS4 | Passwort eines Eintrags anzeigen lassen. | Ziffern und Sonderzeichen sind farbig, Buchstaben normal. |
| DS5 | Tab Einstellungen. | Karte „Meine Suite“ mit „Sperren“, darunter Gruppen Sicherheit, Schutz, Daten, Bereiche, Allgemein; jede Zeile mit kurzem Status (z. B. „1 Schlüssel, 1 Schlüsseldateien“; unter Daten „Suite-Datei“ mit „Öffnen und speichern (.ghlzip)“ bzw. „Nicht gespeicherte Änderungen“). Keine Gruppe „Server“. |
| DS6 | Jede Zeile öffnen, Zurück. | Eigene Seite mit Titel; Zurück führt zur Übersicht. Abgeschaltete Module erscheinen nicht unter „Bereiche“. |
| DS7 | Dunkles Design (Optionen → Ansicht). | Alle Seiten gut lesbar. |

## 17. Servertypen und Server-Modul (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 18. Teilen per Freigabelink (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 19. Modultrennung und GhulMail-Anbindung (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 20. Verschlüsselte Mails (PGP) (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 21. Korrekturen nach dem ersten Test

| Nr. | Schritt | Erwartet |
|---|---|---|
| K3 | In GhulDocs in einem Unterordner die Android-Zurück-Geste. | Geht einen Ordner nach oben, nicht aus der App. |
| K5 | Design: alle Bereiche, Unterseiten und Blätter hell und dunkel durchsehen. | Gleiches Aussehen: Kopfzeile mit farbiger Kontextzeile, Navigationsleiste „Optionen“, Eingabefelder als Karten mit Beschriftung oben, Pillen-Knöpfe, Blätter von unten statt Dialoge („Neuer Eintrag“, „2FA-Konto hinzufügen“, „Zu GhulDocs hinzufügen“, Generator). |

## 22. Neue Funktionen aus der Vorschau

| Nr. | Schritt | Erwartet |
|---|---|---|
| N2 | GhulPass: Eintrag öffnen, Stern antippen. Liste → Chip „Favoriten“. | Eintrag erscheint unter Favoriten; oben in der Liste „Zuletzt verwendet“. |
| N3 | Passwort eines Eintrags zweimal ändern, dann „Mehr“ → „Passwort-Verlauf“. | Alte Passwörter mit Datum, kopierbar. |
| N4 | Eintrag bearbeiten → „Eigenes Feld“ hinzufügen; „Mehr“ → Anhang (Foto/PDF) hinzufügen, öffnen, entfernen. | Feld bleibt nach Speichern; Anhang lässt sich speichern/öffnen und löschen. |
| N5 | Bei einer Anmeldung einen 2FA-Schlüssel eintragen. | Im Eintrag läuft der 6-stellige Code mit Ring. |
| N6 | GhulPass → Karte „Sicherheitscheck“. | Listet doppelte und schwache Passwörter sowie Einträge ohne 2FA; mit eingeschalteter Option auch „in Datenlecks gefunden“ (nur Hash-Anfang wird gesendet). |
| N7 | Generator: „Passphrase“ wählen, Wortzahl/Trennzeichen ändern. | Wörter wie „Kabel-Sonne-…“; Stärkebalken grün. |
| N8 | Eintrag-Menü: Duplizieren, In anderen Tresor verschieben, WLAN-Eintrag → QR. | Kopie „(Kopie)“; Eintrag im Zieltresor; Handy kann per QR ins WLAN. |
| N9 | GhulAuth: Stern, Sortieren (Name/zuletzt), Kachelansicht, „QR aus Bild“, Konto → „Als QR exportieren“. | Favoriten oben; Sortierung bleibt; Bild-QR wird erkannt; zweites Gerät scannt den QR. |
| N23 | Optionen → Darstellung → Start-Bereich „Zuletzt geöffnet“ / Bereiche ausblenden. | Startet im gewählten Bereich; ausgeblendete fehlen in der Leiste. |
| N24 | Optionen → Über: Lizenzen, Bibliotheken, „Fehlerbericht“. | „Made by Daniel Erdmann · 2026“, AGPL-3.0 und Liste (inkl. EFF-Wortliste); Fehlerbericht ohne Geheimnisse. |
| N30 | Optionen → Darstellung → Sprache: „English“, dann „Wie das System“. | Die App wechselt sofort die Sprache (Android); zurück auf Systemsprache. |
| N31 | Import einer 1Password-Datei (.1pux oder CSV) bzw. LastPass-CSV. | Anmeldungen, Notizen, Karten und 2FA-Codes übernommen; archivierte Einträge nicht. |
| N32 | Eintrag → Menü → „Eintrag exportieren“ mit Master-Passwort. | CSV nur mit diesem Eintrag; falsches Passwort wird abgelehnt. |

---

## 23. Ein Bereich, eine Datei (#24)

| Nr. | Schritt | Erwartet |
|---|---|---|
| B1 | Optionen → Bereiche verwalten. | Alle vier Bereiche mit „aktiv“, „hier ausgeblendet“ oder „nicht eingerichtet“ und dem Dateinamen. |
| B2 | Nicht eingerichteten Bereich antippen (z. B. GhulCal oder GhulDocs) → Einrichten. | Name abgefragt; die Datei (`….ghlcal` bzw. `….ghldoc`) entsteht, Bereich erscheint in der Leiste. |
| B3 | Bereich → „Auf diesem Gerät ausblenden“. | Verschwindet nur hier aus der Leiste; nach Speichern und Öffnen der Suite-Datei auf dem zweiten Gerät ist er dort weiter sichtbar. Wieder einblenden geht. |
| B4 | Bereich → „Bereich entfernen“. | Rückfrage mit Warnung; danach ist der Bereich weg und „Nicht gespeicherte Änderungen“ erscheint; nach Speichern und Öffnen der Suite-Datei auch auf dem zweiten Gerät. Bei GhulDocs sind auch die Inhaltsdateien weg. |
| B5 | Optionen → GhulCal: Bundesland ändern, Suite-Datei speichern, auf dem zweiten Gerät öffnen. | Oben „Für GhulCal · alle Geräte“, unten „Nur auf diesem Gerät“. Das Bundesland ist auf dem zweiten Gerät übernommen, die Startansicht nicht. |
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
| I9 | In allen vier Bereichen (Pass, Auth, Docs, Kalender-Liste) einen Eintrag lange drücken, weitere antippen, „Alle“, Löschen. | Gleicher Auswahlkopf oben überall; Löschen ohne Rückfrage, Meldung mit „Rückgängig“ (in GhulDocs: in den Papierkorb). |
| I10 | GhulAuth (Listenansicht): Konto nach rechts wischen, dann nach links. | Rechts: Bearbeiten (Anbieter, Konto). Links: gelöscht, Meldung mit „Rückgängig“. |

## 25. Optionen und Sofortsperre (zweite Runde)

| Nr. | Schritt | Erwartet |
|---|---|---|
| R8 | Optionen-Übersicht → „Entsperren“ mit nur Fingerabdruck, nur App-PIN, beidem, nichts. | Untertitel „Fingerabdruck · 24 Stunden“, „App-PIN · …“, „Fingerabdruck und App-PIN · …“ bzw. „Nur Passwort“; bei „Immer“: „ohne Ablauf“. |
| R9 | Optionen → Schlüssel und Schlüsseldateien. | Kein zweites „Master-Passwort ändern“ (das steht als eigene Zeile in der Übersicht). |
| R9a | Optionen → Schlüssel: „Neuer Code“, Schlüsseldatei hinzufügen, Key hinzufügen, Faktor entfernen; jeweils einmal mit falschem Passwort, einmal ohne zweiten Faktor, einmal richtig. Bei einer Suite mit Key und einer Schlüsseldatei die Schlüsseldatei entfernen. Fingerabdruck und App-PIN einschalten. | Jeder Dialog verlangt Master-Passwort und Key oder Schlüsseldatei (beide Wege wählbar); falsch: „Falsches Passwort“ bzw. Schlüsseldatei-Fehler, nichts ändert sich. Das letzte Backup eines Keys lässt sich nicht entfernen („Ein Security Key braucht ein Backup …“). Fingerabdruck und App-PIN verlangen das Master-Passwort. Eine alte Suite mit nur einem Key zeigt den Hinweis „Dein Key hat kein Backup“. |
| R9b | Entsperren, Suite-Datei öffnen, Tresor öffnen, Tresor übernehmen: bei einer Suite nur mit Schlüsseldatei und bei einer nur mit Key. | Immer beide Wege „Security Key“ und „Schlüsseldatei“ wählbar; vorgewählt ist der zuletzt benutzte; der falsche Weg sagt, was fehlt. |
| R10 | Android-Einstellungen → Apps → Standard-Apps → „Kalender“. | GhulSuite ist als Kalender-App wählbar; als Mail-App erscheint sie nicht mehr. |
| R13 | Eine `.ics`-Datei (z. B. Einladung im Download-Ordner) mit GhulSuite öffnen. | Nach dem Entsperren: „n Termine importiert“ in den Standardkalender. |
| R14 | In einer App „Zum Kalender hinzufügen“ (z. B. Bahn- oder Kino-App). | Neuer Termin mit Titel, Zeit und Ort, zum Speichern bereit. |
| R15 | Schnell entsperren mit Fingerabdruck an. App gesperrt im Hintergrund lassen, wieder hineinwechseln. | Die Fingerabdruck-Abfrage erscheint **sofort**, ohne den Knopf zu drücken. Abbrechen: sie kommt nicht in Dauerschleife wieder, der Knopf bleibt. |

## 26. Alltagstest (eine Woche, Handy und Windows)

Ziel: GhulSuite eine Woche wie im echten Leben benutzen, mit echten Konten, aber mit einer **Kopie** wichtiger Daten. Jeden Tag kurz notieren, was hakt (Fehler melden siehe unten).

| Tag | Was du tust | Worauf du achtest |
|---|---|---|
| 1 | Neue Suite mit Assistent: alle Bereiche, Fingerabdruck. Passwörter aus deinem alten Manager importieren. Suite-Datei auf den USB-Stick speichern; unter Windows öffnen. | Alles da, nichts doppelt; Entsperren mit Key auf beiden Geräten. |
| 2 | Den Tag über Anmeldedaten aus GhulPass nutzen (kopieren). 2FA-Codes aus GhulAuth nutzen. Abends ein Passwort ändern, Suite-Datei speichern, unter Windows öffnen. | Ist die Änderung auf Windows da? Erinnert der Hinweis „Nicht gespeicherte Änderungen“ zuverlässig ans Speichern? |
| 3 | Wichtige Papiere in GhulDocs scannen und Dateien aus anderen Apps dorthin teilen; einige ansehen, eine exportieren, eine mit einer App öffnen. Handy zwischendurch neu starten. | Alles lesbar; die Belegung stimmt; nach dem Sperren ist die befristete Kopie weg. |
| 4 | GhulDocs mit 200 bis 500 MB füllen (Ordner, Umbenennen, Papierkorb); Suite-Datei speichern und auf Windows öffnen. | Speichern und Öffnen mit Fortschritt in vertretbarer Zeit (KONZEPT 8.1); alle Dokumente auf beiden Geräten. |
| 5 | Termine in GhulCal, auch wiederkehrend und mit Erinnerung; einen davon auf Windows ändern (Suite-Datei hin und zurück). Sofortsperre durch Schütteln testen (in der App und im Hintergrund). | Erinnerung kommt pünktlich; Schütteln sperrt beim ersten Versuch, Mails und Erinnerungen kommen danach weiter. |
| 6 | Beide Geräte am selben Tag: Auf einem arbeiten und speichern, auf dem anderen öffnen und weiterarbeiten; einmal absichtlich ohne vorheriges Öffnen auf dem zweiten Gerät ändern und speichern. | Rückfragen wie in Abschnitt 35; nichts wird still überschrieben oder zusammengeführt. |
| 7 | Master-Passwort ändern, Suite-Datei speichern (Aufforderung kommt), unter Windows die neue Datei öffnen. Eine alte Kopie auf dem Handy öffnen. | Die App sagt nach der Änderung, dass alte Kopien gelöscht werden sollen; die alte Kopie öffnet auf dem Handy nicht mehr (SD14). |

## 27. Stresstest (von Hand)

| Nr. | Schritt | Erwartet |
|---|---|---|
| S1 | 5.000 Anmeldungen importieren (z. B. große CSV aus einem anderen Passwortmanager). | Import in unter einer Minute; Liste scrollt flüssig; Suche antwortet sofort. |
| S2 | 200 2FA-Codes. | Codes laufen flüssig, keine Ruckler beim Wechsel der 30-Sekunden-Runde. |
| S3 | Ein Video von etwa 900 MB zu einem GhulDocs-Tresor hinzufügen, währenddessen App in den Hintergrund. Dann eine zweite große Datei in denselben Tresor, danach in einen anderen GhulDocs-Tresor. | Fortschritt, danach ansehen bzw. exportieren: die Datei ist unversehrt. Die zweite Datei wird im selben Tresor abgelehnt („Die Datei braucht …, in diesem GhulDocs-Tresor sind noch … frei …“ bzw. „Dieser GhulDocs-Tresor ist voll …“), im anderen Tresor geht sie. |
| S4 | GhulDocs mit 2.000 kleinen Dateien in vielen Ordnern: schnell scrollen, suchen. | Liste lädt flüssig, nichts friert ein. |
| S5 | 50-mal schnell hintereinander sperren (Knopf) und entsperren (Fingerabdruck). | Kein Absturz, kein „hängendes“ Entsperren. |
| S6 | Suite-Datei mit vollen GhulDocs-Tresoren (drei Tresore, zusammen fast 3 GB) auf einen langsamen USB-Stick mit FAT32 speichern und wieder öffnen. | Fortschritt bis zum Ende, nach Schätzung bis etwa viereinhalb Minuten (Dauer notieren); die Datei ist unter 4 GiB und der Stick nimmt sie; danach alles da. Abbrechen lässt die alte Datei bzw. den Stand auf dem Gerät unverändert. |
| S7 | Handy mit wenig freiem Speicher (unter 500 MB) benutzen. | Klare Meldung statt Absturz, keine kaputten Dateien. |

Automatisch läuft bei jedem Push der Alltagstest im Kern (zwei Geräte, 30 simulierte Tage mit allen Bereichen) und in einem eigenen Job der Stresstest (5.000 Anmeldungen, 2.000 Termine, 300 Dateien und 64 MB, 8 parallele Threads); dabei wandert die Suite wie auf einem USB-Stick zwischen den Geräten.

## 28. Sicherheitspunkte (Runde 5)

| Nr. | Schritt | Erwartet |
|---|---|---|
| R5 | Android-Einstellung „alle Inhalte anzeigen“, Handy mit PIN sperren; eine Termin-Erinnerung kommen lassen. | Gesperrt eingetroffen: nur die in GhulSuite eingestellte Sperrbildschirm-Fassung (B4). |
| R7 | Eine `.ics`-Datei aus einer anderen App (z. B. Dateien) mit GhulSuite öffnen. | Vorschau mit Anzahl und ersten Terminen, Import erst nach „Importieren“ (B6). |
| R8 | App sperren, Handy 20 Minuten in den Standby (Bildschirm aus), dann App wieder öffnen. | Gesperrt, auch wenn das Handy zwischendurch tief geschlafen hat (B8). |
| R10 | Eine App mit Bildschirm-Overlay (z. B. Blaulichtfilter) einschalten und GhulSuite entsperren. | Tipps werden ignoriert, solange etwas darüberliegt (L10); Overlay aus, dann geht es. |
| R12 | Windows: ein Dokument aus GhulDocs „Mit App öffnen“, das Programm offen lassen, GhulSuite sperren; Programm schließen, eine Minute warten. | Der Weitergabe-Ordner ist danach leer (B10). Portabel: der Ordner liegt in `GhulSuite-Daten`. |
| R13 | Optionen → Sicherheit: Hinweis „Etwas verloren …“ lesen. | Hinweis nennt: entfernen bzw. neues Notfallblatt, Suite-Datei speichern und alte Kopien löschen, den zweiten Faktor tauschen (L1). |

## 29. Bedienung (Runde 6)

| Nr. | Schritt | Erwartet |
|---|---|---|
| V1 | Optionen → Bereiche → einen Bereich entfernen; ebenso Optionen → Tresore → einen Tresor löschen. | Dialog nennt die Anzahl der Einträge, bietet „Vorher exportieren“; „Entfernen“ bzw. „Löschen“ geht erst, wenn der Name eingetippt ist (U4). „Vorher exportieren“ beim Tresor startet den verschlüsselten Export dieses Tresors (wie im Menü, bei GhulDocs als `.ghldocs`), beim Bereich führt es zu Optionen → Tresore; nie ein unverschlüsselter Export. |
| V2 | Etwas Falsches eingeben, das der Kern ablehnt (z. B. einen Ordnernamen mit `/` in GhulDocs). | Meldung auf Deutsch, kein englischer Rest (U5). |
| V3 | Gesperrt mit vorhandener Suite: „Suite-Datei öffnen“ mit der Datei einer anderen Suite, im Dialog „Abbrechen“. | Rückfrage „… andere Suite … ersetzt alles hier“; nach „Abbrechen“ bleibt die vorhandene Suite unverändert (U6). |
| V5 | Einrichtung: Key wählen, keine PIN eingeben, „Weiter“. | Meldung „Bitte gib die PIN deines Keys ein …“ (U9). |
| V6 | Für die Abschnitte 35 und 39: ein **zweites Gerät** (zweites Android-Gerät oder Windows-PC mit GhulSuite) und ein paar Testdokumente (PDF, Fotos, Text, eine große Datei von mehreren hundert MB). Für 38: ein paar Blätter Papier. |
| V9 | Flugmodus, App entsperren, „Jetzt nach Updates suchen“. | Verständlicher Hinweis, dass kein Netz da ist; sonst nichts (U13). |
| V11 | Kalender ohne Kalender: Import; dann mit zwei Kalendern: Import; Export mit einem ausgeblendeten Kalender. | Meldung „Lege zuerst einen Kalender an“; Wahl des Ziels; Export enthält alle Kalender, Hinweis darunter (U15). |
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
| K14 | Eine `.ics` importieren, die Zeiten in UTC (`…Z`, z. B. Google-Export) und in einer anderen Zeitzone (`TZID=America/New_York`) enthält, je ein Termin im Sommer und im Winter. | Alle Termine stehen auf der Stunde, zu der sie hier stattfinden (10:00 New York = 16:00 in Deutschland), unabhängig davon, ob gerade Sommer- oder Winterzeit ist. Ganztägige Termine bleiben auf ihrem Tag. |
| K15 | Termin mit Erinnerung anlegen, App schließen, in den Systemeinstellungen die Zeitzone ändern (z. B. London). | Die Erinnerung kommt zur selben Uhrzeit der neuen Zone (Termin 10:00 → Erinnerung 09:45 Londoner Zeit), ohne dass die App geöffnet wurde. |
| K16 | Eine Serie aus einer anderen Zeitzone importieren (z. B. Einladung „jeden Dienstag 10:00 New York“ aus Outlook oder Google) und durch das Jahr blättern: Januar, Mitte März, April, Ende Oktober, November. | In Deutschland steht der Termin um 16:00, in den Wochen, in denen nur die USA oder nur Europa Sommerzeit haben, um 15:00. Die Termin-Ansicht zeigt zusätzlich „Ortszeit in New York: 10:00 - 11:00“; beim Bearbeiten steht unter „Zeitzone“ der Ort. „Nur diesen Termin löschen“ entfernt genau den angezeigten Tag. |
| K17 | Einen einzelnen Eintrag löschen: GhulPass-Eintrag, GhulAuth-Konto, Dokument, Termin. Einmal „Rückgängig“ tippen, einmal die Meldung ablaufen lassen; einmal löschen und sofort sperren. | Erst eine Rückfrage mit dem Namen („‚Codeberg‘ wird gelöscht.“); „Abbrechen“ ändert nichts. Bestätigt: weg, Meldung mit „Rückgängig“; auch ein Wisch nach links fragt erst. Rückgängig: wieder da, nichts passiert. Abgelaufen: endgültig gelöscht (nach Sperren/Entsperren weiterhin weg). Sofort gesperrt: die Löschung wird vor dem Sperren ausgeführt. Sofortsperre direkt nach dem Löschen: der Eintrag bleibt. |
| K17a | In GhulPass ins Suchfeld tippen (Tastatur auf), Tastatur schließen, zu GhulAuth wechseln, ein Konto löschen und die Rückfrage bestätigen. | Die Tastatur bleibt zu; „1 gelöscht“ mit „Rückgängig“ ist sichtbar. |
| K18 | Ganzes löschen: Tresor, Bereich, Papierkorb leeren, mehrere endgültig löschen. | Immer eine Rückfrage mit rotem Knopf, Warnung als Hinweisfeld wie bei „Bereich entfernen“. |
| K19 | Breites Fenster (Windows oder Handy/Tablet quer): Termin öffnen, Termin bearbeiten, in GhulAuth „Konto hinzufügen“. | Links die Seitenleiste mit den Bereichen, daneben eine Spalte in der Mitte; Geöffnetes steht an der Stelle der Liste, „Zurück“ führt zur Liste. Keine leere dritte Spalte. |
| K20 | Termine in der Liste auswählen → „Alle wählen“, dann noch einmal „Alle wählen“. | Erst sind alle gewählt, beim zweiten Tippen ist die Auswahl zurückgenommen, wie in den anderen Listen. |
| K21 | **Mehrere Kalender-Tresore:** Optionen → Tresore → „Neuer Tresor“ → GhulCal („Verein“). Kopfzeile „Alle Kalender“ antippen; „Neuer Kalender“. Danach den GhulCal-Tresor löschen, bis keiner mehr da ist, und wieder „Neuer Kalender“. | Im Kalenderblatt stehen die Kalender unter dem Namen ihres Tresors. „Neuer Kalender“ fragt, in welchen Tresor (Tresorwahl mit „Neuer Tresor“, wie beim neuen Eintrag in GhulPass); der neue Kalender steht unter diesem Tresor. Mit nur einem Tresor fragt es nicht; ohne Tresor bietet die Wahl „Neuer Tresor“ an. |

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
| W2 | Optionen → Über → „Fehlerbericht anzeigen“. | Der Bericht mit Version, Gerät und ggf. letztem Absturz, ohne Tresor-Inhalte; Hinweis, dass nichts von selbst gesendet wird. Es gibt keinen Knopf „Senden“. |
| W4 | Bericht „Kopieren“, „Teilen“ (z. B. an die Mail-App) und „Speichern“. | Alle drei funktionieren, auch im Flugmodus; GhulSuite selbst baut dabei keine Verbindung auf. |
| W6 | Optionen und eine Unterseite (z. B. Über) öffnen. | Zwischen Kopfleiste und Inhalt kein zusätzlicher leerer Streifen. |

## 32. Server, Teilen und Anzeige (Runde 7) (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 33. Aufgaben im Hintergrund

Lange Arbeiten (Dokumente hinzufügen, exportieren, Suite-Datei speichern und öffnen) laufen im Hintergrund weiter, auch wenn du den Bereich verlässt. Ein Streifen unter der Kopfleiste zeigt, was läuft, wie weit es ist, und „Abbrechen“; am Ende sagt eine Meldung, wie es ausging.

| Nr. | Schritt | Erwartet |
|---|---|---|
| BG1 | In GhulDocs drei große Dateien hinzufügen, dann zu GhulPass wechseln und einen Eintrag öffnen. | Der Streifen mit Balken und „1 von 3 fertig“ bleibt unter jeder Kopfleiste sichtbar; das Hinzufügen läuft weiter. Am Ende eine Meldung, in GhulDocs stehen alle drei. |
| BG2 | Während BG1 im Streifen „Abbrechen“. | Sofort Schluss; die fertigen Dateien sind da, von der abgebrochenen bleibt nichts (auch kein Platz in der Belegung). |
| BG3 | Hinzufügen einer großen Datei starten, App in den Hintergrund (Home-Taste), Benachrichtigungen aufziehen. | Die Benachrichtigung „GhulSuite ist entsperrt“ zeigt jetzt den Auftrag mit Balken, „Abbrechen“ und „Sperren“. „Abbrechen“ dort stoppt ihn. Auf dem Sperrbildschirm steht nur „GhulSuite ist entsperrt“ (keine Dateinamen). |
| BG4 | Hinzufügen einer großen Datei starten, App in den Hintergrund, länger warten als die Zeit für automatisches Sperren. | Die App sperrt nicht mitten im Hinzufügen, sondern erst, wenn er fertig ist; die Meldung „… hinzugefügt.“ kommt erst nach dem Entsperren. Kommt zwei Minuten lang kein Fortschritt (z. B. WLAN aus), sperrt sie trotzdem und bricht das Hinzufügen ab. |
| BG5 | Wie BG4, aber vor dem Ende zurück in die App. | Die App bleibt entsperrt; das Hinzufügen läuft zu Ende. |
| BG6 | Hinzufügen einer großen Datei starten, dann von Hand sperren (Schloss, Schütteln oder „Sperren“ in der Benachrichtigung). | Sofort gesperrt, das Hinzufügen ist gestoppt. Nach dem Entsperren: „GhulSuite wurde gesperrt. Hinzufügen abgebrochen: …“. |
| BG7 | Auftrag läuft, App im Hintergrund, Auftrag endet. | Eine kurze Benachrichtigung mit dem Ergebnis (Android 13+: nur wenn Benachrichtigungen erlaubt sind); auf dem Sperrbildschirm nur eine Meldung ohne Dateinamen. Ist die App vorne, kommt nur die Meldung in der App. |
| BG8 | Mehrere Aufträge nacheinander starten (z. B. Hinzufügen und Exportieren). | Der zweite zeigt „Wartet auf die Aufgabe davor …“ und startet danach; beide sind im Streifen. |
| BG12 | Nur Android 11 oder älter: Hinzufügen einer großen Datei starten, auf der Startseite „Zurück“. | Die App geht in den Hintergrund, das Hinzufügen läuft weiter (Streifen und Benachrichtigung). Ohne laufenden Auftrag schließt „Zurück“ die App wie gewohnt. |

## 34. Mail-Archiv in GhulDrive (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 35. Suite-Datei: speichern und öffnen

Die Suite wandert als **eine Datei** (`.ghlzip`) zwischen den Geräten, z. B. auf dem USB-Stick. Abgeglichen wird
nichts: Aktuell ist der Stand, der zuletzt gespeichert wurde. GhulDocs reist mit allen Dokumenten in der Datei mit.
Gerät A und Gerät B sind zwei Geräte mit GhulSuite (Android oder Windows). Diese Abläufe prüft auch
`app/maestro/14-suite-file.yaml`.

| Nr. | Schritt | Erwartet |
|---|---|---|
| SD1 | Gerät A: Optionen → Suite-Datei. | Seite mit „Speichern“, „Speichern unter …“, „Andere Suite-Datei öffnen“ und dem Stand („Noch nicht als Datei gespeichert“ bzw. „Suite-Datei: …“, „Alles ist in der Suite-Datei gespeichert.“). Kurze Erklärung, dass die Datei alle Tresore verschlüsselt und versiegelt enthält. |
| SD2 | Gerät A: Einen Eintrag ändern. | Auf den Bereichsseiten oben „Nicht gespeicherte Änderungen. Tippen zum Speichern.“; in der Optionen-Übersicht unter „Suite-Datei“ ebenfalls. Antippen führt zur Seite Suite-Datei. |
| SD3 | „Speichern“ (die Datei liegt auf dem Stick). | „Gespeichert in …“; der Hinweis aus SD2 verschwindet; „Alles ist in der Suite-Datei gespeichert.“ |
| SD4 | Stick abziehen, etwas ändern, „Speichern“. | „Die Suite-Datei ist gerade nicht erreichbar (ist der USB-Stick eingesteckt?) …“; die Änderung bleibt als nicht gespeichert markiert. Stick wieder einstecken, „Speichern“: klappt. |
| SD5 | Gerät B (ohne Suite): Startbildschirm → „Suite-Datei öffnen“ → die Datei vom Stick → Passwort und Key. | „… wird zuerst vollständig geprüft. Erst danach ersetzt sie den Stand auf diesem Gerät.“ Fortschritt beim Öffnen; danach ohne Rückfrage geöffnet; alle Einträge, Dokumente und Einstellungen sind da. |
| SD6 | Gerät B: etwas ändern, speichern. Gerät A: „Andere Suite-Datei öffnen“ → dieselbe Datei. | Öffnet ohne Rückfrage (neuerer Stand derselben Suite); die Änderung von B ist auf A. |
| SD7 | **Älterer Stand:** Vor SD6 eine Kopie der Datei aufheben. Gerät A (hat jetzt den neueren Stand) öffnet die alte Kopie. | Rückfrage „Diese Datei öffnen?“ mit „Diese Datei ist älter als dein Stand hier.“ „Abbrechen“ ändert nichts; „Öffnen“ lädt den alten Stand. |
| SD8 | **Abzweig:** A und B öffnen denselben Stand. A ändert und speichert unter neuem Namen, B ändert und speichert unter einem anderen Namen. A öffnet die Datei von B. | „Diese Datei wurde von einem anderen Stand aus weiterbearbeitet als deinem hier.“ Nichts wird zusammengeführt: Nach „Öffnen“ gilt nur der Stand von B. |
| SD9 | **Andere Suite:** Auf A die Datei einer anderen Suite (z. B. aus E18) öffnen. | „Diese Datei enthält eine andere Suite als die auf diesem Gerät. Sie ersetzt alles hier.“ Passwort und Key der anderen Suite werden verlangt. |
| SD10 | **Nicht gespeicherte Änderungen:** Auf A etwas ändern (nicht speichern), dann eine Suite-Datei öffnen. | Rückfrage nennt zusätzlich: „Auf diesem Gerät gibt es Änderungen, die noch in keiner Suite-Datei sind. Sie gehen verloren. Speichere vorher, wenn du sie behalten willst.“ Abbrechen, speichern, erneut öffnen: keine Rückfrage mehr. |
| SD11 | Mehrere Gründe auf einmal (z. B. ältere Datei und nicht gespeicherte Änderungen). | Alle Gründe stehen in **einem** Dialog. |
| SD12 | **Überschreiben:** A und B öffnen dieselbe Datei. B ändert und speichert. A ändert und tippt „Speichern“ auf dieselbe Datei. | „Die Datei wurde inzwischen anderswo gespeichert. Überschreiben (die Änderungen von dort gehen verloren) oder unter neuem Namen speichern?“ „Speichern unter …“ legt eine zweite Datei an; „Überschreiben“ ersetzt sie. |
| SD13 | „Speichern unter …“ auf eine Datei, die eine andere Suite oder etwas ganz anderes (z. B. ein Foto, umbenannt in `.ghlzip`) ist. | „An diesem Ort liegt eine andere Suite oder keine GhulSuite-Datei. Überschreiben oder unter neuem Namen speichern?“ |
| SD14 | **Älterer Schutz:** Datei speichern, Kopie aufheben. Dann Passwort ändern und neu speichern. Auf demselben Gerät die alte Kopie öffnen (mit dem alten Passwort). | Wird **nicht** geöffnet: „Diese Suite-Datei ist durch eine neuere mit geändertem Schutz … ersetzt und nicht mehr gültig. Öffne die neueste Datei.“ |
| SD15 | Wie SD14, aber die alte Kopie auf einem Gerät öffnen, das den neuen Stand nie gesehen hat (z. B. frisch installiert). | Öffnet sich mit dem alten Passwort und dem alten zweiten Faktor (bekannte Grenze, KONZEPT 8.2). Nach dem Tausch des zweiten Faktors (alten Key zurücksetzen bzw. alte Schlüsseldatei löschen) öffnet sie gar nicht mehr. |
| SD16 | Eine Suite-Datei mit einem ZIP-Programm verändern: eine Tresordatei entfernen, eine fremde hinzufügen, eine Datei durch die aus einer älteren Kopie ersetzen oder einen Ordner hinzufügen. Dann öffnen. | Jedes Mal wird nichts geladen, und die App sagt, dass die Datei nicht stimmt. Der bisherige Stand auf dem Gerät bleibt unverändert. |
| SD17 | Eine beliebige andere Datei (Foto, normale ZIP) als Suite-Datei öffnen. | „Das ist keine GhulSuite-Datei.“ |
| SD18 | **Mit dem Wiederherstellungscode:** „Suite-Datei öffnen“ → „Passwort vergessen? Mit dem Wiederherstellungscode öffnen“ → Code und neues Passwort. | Hinweis, dass die Datei ohne Passwort nicht geprüft werden konnte; danach offen mit dem neuen Passwort, Hinweis „Nicht gespeicherte Änderungen“ (alles wurde neu versiegelt). Ältere Datei, Abzweig und älterer Schutz werden trotzdem erkannt wie in SD7 bis SD14. |
| SD19 | **Gerätedaten:** Schnell-Entsperren (Fingerabdruck) einrichten, dann (a) einen neueren Stand derselben Suite mit demselben Schutz öffnen, (b) eine Suite-Datei, deren Passwort anderswo geändert wurde, (c) eine andere Suite. | (a) Fingerabdruck und App-PIN bleiben. (b) und (c): Danach braucht das Entsperren wieder Passwort und Key (Schnell-Entsperren ist gelöscht). |
| SD20 | Während des Öffnens (nach der Prüfung, beim Ersetzen) die App über die Liste der letzten Apps beenden; neu starten. | Entweder der alte oder der neue Stand ist vollständig da, nie eine Mischung. |
| SD21 | Eine Suite mit allen vier Bereichen speichern, auf dem anderen Gerät öffnen. | Alle Bereiche, Tresornamen, Dokumente und Bereichs-Einstellungen (z. B. Bundesland in GhulCal) sind da; was nur ein Gerät betrifft (Sperren, Schnell-Entsperren, Ansicht), nicht. |

## 36. GhulDrive auf dem Server: zwei Geräte, Index (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

## 37. Schlüsselwechsel nach einer Schutzänderung (entfernt)

Entfällt: GhulMail, GhulDrive, Server, Freigabelinks und der Schlüsselwechsel wurden mit dem Umbau zu GhulDocs entfernt
([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)). GhulDocs testet Abschnitt 39, das Scannen Abschnitt 38.

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
| SC13 | GhulDocs-Tresor fast voll (Testtresor nahe 1 GB): in einem Ordner dieses Tresors den Scanner öffnen (Tresor und Ordner stehen damit fest), Seiten aufnehmen, speichern; dann „Abbrechen“. Danach in einem anderen GhulDocs-Tresor einen neuen Scan. | Meldung mit den echten Zahlen „Die Datei braucht …, in diesem GhulDocs-Tresor sind noch … frei. Es wurde nichts hinzugefügt …“, die Seiten bleiben erhalten, bis „Abbrechen“; im anderen Tresor wird der neue Scan gespeichert. |
| SC14 | Querformat, Taschenlampe („Licht“) an und aus, Kassenbon (lang und schmal). | Alles bedienbar; der Kassenbon wird eine schmale, hohe Seite. |
| SC15 | **Form des Blatts:** ein A4-Blatt schräg von unten (etwa 35°) und schräg von der Seite fotografieren, ebenso quer liegend; dann ein quadratisches Blatt und eine Karte (z. B. Scheckkarte) schräg. | Die fertige Seite hat die Form des Blatts (A4 etwa 1 : 1,41, hoch oder quer; quadratisch bleibt quadratisch; die Karte etwa 1,59 : 1), nicht die verkürzte Form aus dem Foto (früher kam ein schräg fotografiertes A4-Blatt fast quadratisch heraus). Keine Form wird auf A4 gezogen. |

## 39. GhulDocs

Verschlüsselter Speicher für Dokumente, bis 1 GB je GhulDocs-Tresor und 3 GB für alle GhulDocs-Tresore einer Suite. Vorher: GhulDocs eingerichtet, ein paar Testdateien auf dem
Gerät (PDF mit mehreren Seiten, Foto, Textdatei, eine Datei, die die App nicht anzeigen kann, z. B. `.docx`). Diese
Abläufe prüft auch `app/maestro/07-docs.yaml`.

| Nr. | Schritt | Erwartet |
|---|---|---|
| GD1 | GhulDocs öffnen, erst mit einem, dann mit zwei GhulDocs-Tresoren (einen bestimmten wählen). | Karte mit Belegung des gezeigten Tresors („… von 1 GB“, Zahl der Dokumente, „verschlüsselt auf diesem Gerät“), Kacheln Favoriten und Papierkorb, darunter die Liste (Ordner zuerst). Bei zwei Tresoren unter der Karte „Alle GhulDocs-Tresore: … von 3 GB“. „Alle Tresore“: Hauptzeile die Summe „… von 3 GB“, darunter die Liste der Tresore (keine Dateien). Einen Tresor daraus antippen: dieselbe Karte wie oben mit Kacheln, darunter der Brotkrumen „Alle Tresore › Name“. Mehrere Dateien auswählen: der Kopf springt nicht. |
| GD2 | „Hinzufügen“ → „Dateien hinzufügen“, mehrere Dateien auf einmal wählen. | Fortschritt im Streifen, danach „n Dateien hinzugefügt.“; jede Datei mit Größe, Datum und Symbol nach Typ; die Belegung wächst. |
| GD3 | In einer anderen App (Galerie, Dateimanager) eine Datei teilen → „In GhulDocs speichern“. | Nach dem Entsperren „In GhulDocs speichern“ mit Ziel (Ordner, bei mehreren GhulDocs-Tresoren auch der Tresor, änderbar); danach liegt die Datei dort. Ist GhulDocs nicht eingerichtet, sagt die App, wie man es einrichtet. |
| GD4 | Ordner anlegen, umbenennen, Datei hinein verschieben, Ordner verschieben, Namen mit `/` oder einen schon vorhandenen Namen versuchen. | Alles wirkt sofort; doppelte und ungültige Namen werden mit einer verständlichen Meldung abgelehnt. |
| GD5 | Suchen nach einem Teil eines Namens; Stern an einer Datei; Ansicht Favoriten. | Treffer aus allen Ordnern; Favorit erscheint unter Favoriten. |
| GD6 | Ein Bild, eine PDF (mehrere Seiten) und eine Textdatei antippen. | Ansicht in der App: Bild, alle Seiten der PDF (Android 11 und neuer; darunter Hinweis mit „Mit App öffnen“ und „Exportieren“), Text. Bei einer Datei, die die App nicht anzeigen kann: Hinweis mit beiden Möglichkeiten. |
| GD7 | Während der Ansicht eine Bildschirmaufnahme versuchen; App sperren; nach dem Entsperren im Dateimanager und unter `Android/data/…` suchen. | Kein Bildschirmfoto (außer erlaubt); nach dem Sperren ist die Ansicht weg; nirgends eine entschlüsselte Kopie. |
| GD8 | Datei → „Exportieren“. | Rückfrage „Unverschlüsselt exportieren?“; nach „Exportieren“ und Wahl des Orts liegt die Datei dort, lesbar. „Abbrechen“ im Streifen lässt keine halbe Datei zurück. |
| GD9 | Datei → „Mit App öffnen“, eine App wählen; dann GhulSuite sperren. | Die andere App zeigt die Datei; nach dem Sperren ist die befristete Kopie gelöscht (die andere App kann sie nicht mehr neu laden). |
| GD10 | Datei löschen, Papierkorb öffnen, wiederherstellen; nochmal löschen, „Endgültig löschen“; „Papierkorb leeren“. | Zurück am alten Ort; danach endgültig weg. Hinweis: Was im Papierkorb liegt, zählt zur Belegung, bis er geleert wird. |
| GD11 | **Grenze je Tresor:** In einen GhulDocs-Tresor Dateien hinzufügen, bis fast 1 GB belegt ist (der Papierkorb zählt mit), dann eine weitere große Datei. Ebenso eine einzelne Datei über 1 GB. | „Die Datei braucht 180 MB, in diesem GhulDocs-Tresor sind noch 40 MB frei. Es wurde nichts hinzugefügt. Leere den Papierkorb, lösche Dokumente oder nimm einen anderen GhulDocs-Tresor.“ (mit den echten Zahlen; bei einer Datei unbekannter Größe „Dieser GhulDocs-Tresor ist voll: …“) bzw. „Diese Datei ist zu groß für GhulDocs (höchstens 1 GB je Tresor) …“; Belegung unverändert, nichts zurückgeblieben. Nach dem Leeren des Papierkorbs geht es wieder. |
| GD12 | Den GhulDocs-Tresor für sich speichern (Optionen → Tresore → Exportieren) und in GhulDocs Dokumente lange drücken → „Als Tresor weitergeben“ (eigenes Passwort, neue Schlüsseldatei). | Der Speichern-Dialog schlägt `<Name>.ghldocs` vor; es entsteht ein GhulDocs-Paket mit dem Tresor und seinen Dokumenten, mit Fortschritt. Auf einem zweiten Gerät ohne Suite „Einzelnen Tresor öffnen“: alle Dokumente da, lesbar. Den ganzen Tresor über das Tresor-Menü weitergeben: T16. |
| GD13 | **Mehrere GhulDocs-Tresore (höchstens 3):** Optionen → Tresore → „Neuer Tresor“ → GhulDocs, Name „Zweiter“; einen dritten anlegen, dann einen vierten versuchen (auch über die Tresorwahl in GhulDocs); einen Tresor löschen und wieder einen anlegen. In GhulDocs die Kontextzeile antippen, „Alle Tresore“ und jeden Tresor einzeln wählen. Ordner anlegen, Dateien in beide Tresore hinzufügen (die Ordnerwahl beim Hinzufügen nennt den Tresor), je eine löschen, Favoriten setzen. In einer anderen App eine Datei teilen → „In GhulDocs speichern“ und als Ziel „Zweiter“ wählen. Optionen → GhulDocs ansehen. | „Neuer Tresor“ bietet GhulDocs an. Bei drei Tresoren bietet „Neuer Tresor“ GhulDocs gar nicht mehr an, nur den Hinweis „Eine Suite hat höchstens 3 GhulDocs-Tresore (je bis 1 GB). Lösche einen, um einen neuen anzulegen oder ein GhulDocs-Paket hinzuzufügen.“; die Tresorwahl bietet keinen neuen an; nach dem Löschen eines Tresors geht es wieder. Jeder Tresor hat eigene Ordner, eigenen Papierkorb und eigene Favoriten; „Alle Tresore“ zeigt die Liste der Tresore; Suche, Favoriten und Papierkorb nennen bei „Alle Tresore“ zu jeder Datei ihren Tresor. Die geteilte Datei liegt in „Zweiter“. Die Belegung wie GD1; einen Tresor fast füllen: „Dieser GhulDocs-Tresor ist fast voll.“; sind nur alle zusammen fast voll: „Alle GhulDocs-Tresore zusammen sind fast voll (höchstens 3 GB je Suite).“ Optionen → GhulDocs zeigt bei einem Tresor „… von 1 GB“, bei mehreren die Summe „… von 3 GB“ und darunter jeden Tresor einzeln. Suite-Datei speichern und auf Gerät B öffnen: alle Tresore mit ihren Dokumenten da. |
| GD13b | **Grenze für alle zusammen:** Drei GhulDocs-Tresore mit je fast 1 GB füllen (Testtresore), dann in einen davon eine große Datei hinzufügen. Ebenso viele kleine Dateien bis 20 000 Dokumente in allen drei Tresoren zusammen. | Abgelehnt mit „… in diesem GhulDocs-Tresor sind noch … frei …“ oder „Das braucht …, in allen GhulDocs-Tresoren zusammen sind noch … frei (höchstens 3 GB je Suite). Es wurde nichts hinzugefügt …“ (je nachdem, was zuerst greift) bzw. bei unbekannter Größe oder zu vielen Dokumenten „Alle GhulDocs-Tresore zusammen sind voll: … (höchstens 20 000 Dokumente) …“; nichts geschrieben, nichts zurückgeblieben. Nach dem Leeren eines Papierkorbs oder Löschen von Dokumenten geht es wieder. |
| GD13c | **Paket wird ein eigener Tresor:** Ein GhulDocs-Paket (GD12, von einem anderen Gerät) auf einem Gerät mit eigenem GhulDocs über „Tresordatei hinzufügen“ bzw. im Dateimanager antippen → „Zu meiner Suite hinzufügen“; im Feld „Passwort dieses Tresors“ erst ein falsches, dann das richtige Passwort des Pakets, dazu sein Key bzw. seine Schlüsseldatei. Dasselbe Paket ein zweites Mal hinzufügen. Zum Schluss ein Paket, das größer als 1 GB ist, und ein Paket bei schon drei GhulDocs-Tresoren. | Dialog „Tresor übernehmen“, Name vorbelegt mit dem Namen der Datei (änderbar); kein „In GhulDocs aufnehmen“, kein „Name des Ordners“, keine Wege zum Ersetzen. Falsches Passwort: nichts geändert. Danach ist das Paket ein eigener GhulDocs-Tresor mit seinen Ordnern, Papierkorb und Favoriten, das eigene GhulDocs bleibt unverändert; nach Sperren mit eigenem Passwort und Key lesbar (Schutz der Suite). Das zweite Mal: „Das ist schon vorhanden, es wurde nichts hinzugefügt.“; der vorhandene Tresor und seine Dokumente bleiben unverändert (auch wenn er beschädigt ist: nichts wird gelöscht oder überschrieben; ein manipuliertes Paket mit gleichem Dateinamen prüft der Kerntest `adopting_never_overwrites_a_vault_with_the_same_file_name`, von Hand nur mit demselben Paket). Zu großes Paket: „Das Paket braucht …, ein GhulDocs-Tresor fasst höchstens 1 GB. Es wurde nichts hinzugefügt.“ Passt es nicht mehr zu den anderen (20 000 Dokumente, 3 GB): Meldung wie GD13b. Bei schon drei GhulDocs-Tresoren: „Eine Suite hat höchstens 3 GhulDocs-Tresore (je bis 1 GB). Lösche einen, um einen neuen anzulegen oder ein GhulDocs-Paket hinzuzufügen.“, nichts übernommen. In keinem Fall bleibt etwas halb zurück. Ein GhulDocs ersetzen: alten Tresor unter Optionen → Tresore löschen, dann das Paket hinzufügen. |
| GD13a | **Dateiarten verwechselt:** Eine `.ghldocs` über „Suite-Datei öffnen“ wählen; eine `.ghlzip` über „Einzelnen Tresor öffnen“ bzw. „Tresordatei hinzufügen“ wählen. Im Dateimanager eine `.ghldocs` und eine `.ghlzip` antippen. | „Das ist ein GhulDocs-Paket (.ghldocs), keine Suite-Datei …“ bzw. „Das ist eine Suite-Datei (.ghlzip), kein GhulDocs-Paket …“; nichts geöffnet. Im Dateimanager öffnet die `.ghldocs` die Auswahl für ein GhulDocs-Paket (entsperrt: „Tresor übernehmen“, siehe GD13c), die `.ghlzip` „Suite-Datei öffnen“. |
| GD14 | **Suite-Datei mit Dokumenten:** Suite-Datei speichern (Fortschritt), auf Gerät B öffnen. | Alle Dokumente sind da und lassen sich ansehen; die Datei ist etwa so groß wie die Dokumente zusammen. |
| GD15 | Optionen → GhulDocs: Belegung, „Dokumente prüfen“. | „Alle Dokumente sind in Ordnung.“ Nichts wird geändert. |
| GD16 | Während des Hinzufügens einer großen Datei die App über die Liste der letzten Apps beenden; neu starten und entsperren. | Die Datei fehlt, die Belegung stimmt, es bleibt kein Rest (Reste räumt die App beim Entsperren weg). |
| GD17 | Dunkles Design und Querformat: Liste, Ansicht, Papierkorb, Optionen → GhulDocs. | Alles lesbar, nichts abgeschnitten. |
| GD18 | Suite-Datei speichern (viele MB Dokumente) und währenddessen ein Dokument endgültig löschen oder den Papierkorb leeren. | Das Speichern läuft zu Ende und die Datei öffnet auf Gerät B mit allen Dokumenten des Stands beim Speichern; danach ist das gelöschte Dokument auch auf dem Datenträger weg. |
| GD19 | Auf Gerät B eine Suite-Datei öffnen, die Rückfrage stehen lassen (Auto-Sperre aus), später bestätigen. | Alle Dokumente sind da, auch nach langer Wartezeit oder wenn die Uhr des Geräts inzwischen springt. |
| GD20 | Aus einer anderen App eine Datei mit Richtungszeichen im Namen teilen (z. B. `rechnung`, das Zeichen U+202E und `fdp.exe`), ebenso einen Namen nur aus geschützten Leerzeichen. | Gespeichert als `rechnungfdp.exe` bzw. `file`, keine Meldung „ungültiger Name“. |
| GD21 | GhulPass: einem Eintrag große Anhänge hinzufügen, bis es nicht mehr geht. | Bei etwa 200 MiB in allen Tresoren neben GhulDocs: „Die Tresore neben GhulDocs sind voll …“, nichts hinzugefügt; die Suite-Datei lässt sich speichern und öffnen. |
| GD22 | Nach dem Update von einer Version mit GhulMail, GhulDrive oder automatischer Sicherung: App starten. | Einmal still entfernt: Mail-Zwischenspeicher, Drive-Offline-Kopien, Datenbank der alten Hintergrundaufgaben, alte Einstellungen, Zugriff auf den Ordner der alten Sicherung (Android-Einstellungen → Apps → GhulSuite → Speicher), Benachrichtigungskanäle von Mail. Tresore, Dokumente und Schnell-Entsperren bleiben. |
| GD23 | Eine Suite-Datei oder Arbeitskopie einer älteren Entwicklerversion (vor Formatversion 3) öffnen. | „Diese Datei stammt von einer älteren Entwicklerversion von GhulSuite …“ statt „keine GhulSuite-Datei“. |

## Fehler melden

Bitte je Fehler: **Testnummer**, Handy und Android-Version, App-Version (Optionen → Über GhulSuite), was du gemacht hast,
was erwartet war, was passiert ist, ggf. Bildschirmfoto (dafür vorübergehend „Bildschirmfotos erlauben“ einschalten).
