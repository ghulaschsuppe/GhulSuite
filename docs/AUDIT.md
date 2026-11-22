# GhulSuite: Sicherheitsbericht

**Stand:** 22.11.2026 (nach der Großprüfung vom 21. und 22.11.2026 und den Runden 7 bis 14 mit der Angreifer-Runde, Abschnitt 3 und 4.6), Formatversion 3 mit Schreibsiegel, **eine reine Offline-App** (Umbau nach
[`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md) und [`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)): Die Suite wandert als eine
Datei (`.ghlzip`, bis 3,75 GiB), Dokumente liegen verschlüsselt in GhulDocs (seit 20.11.2026 bis zu 3 Tresore), es gibt keine Server und keine Konten.
Nach draußen gehen nur die Update-Prüfung und die Leak-Prüfung von GhulPass, beide abschaltbar. Befunde, die nur
entfernte Funktionen betrafen (Synchronisation, FTP/FTPS, Sicherung/Wiederherstellung, „Von meinem Server holen“,
Autofill, und seit dem Umbau zu GhulDocs: GhulMail, GhulDrive, Server und SFTP, PHP-Modul, Freigabelinks,
Schlüsselwechsel, Mail-Archiv, Hochladen von Fehlerberichten), bleiben in der Geschichte stehen und sind als
**obsolet** markiert.
**Art:** Internes Audit des eigenen Codes (Durchsichten durch KI-Agenten) plus eine dauerhafte Testsuite mit
Angriffen. GhulSuite ist ein Hobbyprojekt ohne Prüfung durch Dritte. Wer sich mit Sicherheit auskennt, ist eingeladen
mitzulesen, anzugreifen und zu melden ([`SECURITY.md`](../SECURITY.md)).

Dieser Bericht fasst alle bisherigen Runden zusammen und sagt ehrlich, was offen ist. Die Einzelheiten jedes Fundes
(Test, der ihn zeigt, Commit) stehen im [Testprotokoll](TESTPROTOKOLL.md), wie getestet wird im
[Testkonzept](TESTKONZEPT.md), das Dateiformat in [`FORMAT.md`](FORMAT.md).

## 1. Die Runden

Alle Prüfläufe, auch die künftigen, und die Geschichte des Konzepts stehen chronologisch in der
[Prüf-Chronik](PRUEFCHRONIK.md); hier die Runden bis 20.11.2026 im Einzelnen.

| Datum | Runde | Ergebnis |
|---|---|---|
| 11.11.2026 | Audit 0.5.0: Freigabelinks, Empfängerseite, PHP-Server-Modul, FTPS, HTTPS-Client, Teilen an andere Apps, OpenPGP, Abhängigkeiten | 8 Befunde (A1 bis A8) und 2 Abhängigkeits-Hinweise, alle behoben bzw. begründet |
| 13.11.2026 | Audit des ganzen Codes in drei unabhängigen Durchsichten: Kern (Rust, PHP), App (Android, Windows, CI), Bedienung bei Fehlern | 6 Punkte „vor echtem Einsatz“, 10 für den nächsten Durchgang, Wortwahl; alle 6 dringenden behoben |
| 13.11.2026 | Testrunden 1 bis 3: Chaos-Sync, Fuzzing, absurde Eingaben, Angriffe, öffentliche Testsammlungen, Kalender und Zeitzonen, Emulator | 39 Funde, 36 behoben |
| 14.11.2026 | Runde 4: Schlüssel bei jeder Schutzänderung erneuern (B1), neutrale Dateinamen (F23), Geräte lange offline | 2 weitere Funde, alle behoben |
| 14.11.2026 | Runde 5: alle übrigen Punkte des Audits, Konfliktkopien fremder Sync-Programme, Prüfwerkzeuge im Nachtlauf (x509-limbo, GnuPG, ZAP, MobSF, Intent-Angriffe, reproduzierbarer Kern), Prüfsummen für Gradle | 2 weitere Funde (F42, F43), behoben; nichts mehr offen |
| 14.11.2026 | Runde 6: Bedienung (U4 bis U15), Mail-Ablauf mit echten Servern im Emulator (B21), neues verschlüsseltes Mail-Archiv mit Prüfung jeder Mail vor dem Löschen beim Anbieter | 4 weitere Funde (F44 bis F47), behoben; nichts offen |
| 14.11.2026 | Runde 7: Updates aus der App (signierte Prüfsummen, nur GitHub, Kanäle Stabil und Vorab), Gmail vorerst ohne Löschen beim Anbieter | keine Funde; Schutz des Updaters in 4.2 |
| 14.11.2026 | Runde 8: Website www.ghulsuite.de (keine Cookies, nichts von fremden Servern, Content-Security-Policy), Fehlerberichte aus der App über `report.php` mit Spamschutz (Proof-of-Work, einmalige Beweise, Begrenzung je Adresse und Tag, 20 Angriffsfälle in `tools/test-report-php.py`) | 1 Fund (F48, Wettlauf beim Schreiben gleicher Archiv-Blöcke), behoben |
| 15.11.2026 | Runde 9: Teilen ohne Kopie (Schlüssel je Datei, Server-Modul ohne Geheimnis und ohne Schreibzugriff übers Web), Mail-Archiv als Kopie in GhulDrive, Löschen nur mit Rückfrage | 30 Funde (F106 bis F135), behoben; F110 bis F117 aus eigenen, unabhängigen Durchsichten der Runde |
| 16.11.2026 | Runde 10: Autofill und Foto-Sicherung gestrichen, mehrere Speicherorte für GhulDrive (getrennt von der Synchronisation, Entfernen beendet die Freigabelinks, Löschen der Dateien nur auf ausdrückliche Wahl) | 14 Funde (F136 bis F149), erledigt; F138 bis F144 aus der unabhängigen Durchsicht, F145, F146 und F149 aus den Emulator-Läufen, F147 (die Synchronisation hält den Kern nicht mehr während der Netzwerkarbeit, **obsolet** seit dem Umbau) und F148 (Testschlüssel der CI) aus der CI |
| 17.11.2026 | **Umbau ohne Synchronisation.** Anlass: die Befunde der letzten Durchsichten (u. a. H1, S1, M18, viele M- und N-Punkte), die meisten zur Synchronisation (fremde Dateien übernehmen, Schutzstände zusammenführen, ersetzte Codes, gleichzeitige Änderungen, gefälschte Server-Dateien). Entfernt: Synchronisation, Zusammenführen, „Der Ordner ist die Suite“, FTP/FTPS, Sicherung/Wiederherstellung und „Von meinem Server holen“. Neu: Suite-Datei `.ghlzip` mit versiegelter Kopfdatei und vollständiger Prüfung vor dem Ersetzen (Zähler, Stand-ID, Schutz-Epoche), Formatversion 3 mit Schreibsiegel, GhulDrive mit `index.lock`, `index.bak`, Revision und automatischem Aufräumen, Schlüsselwechsel (Drive-Schlüssel und SSH-Schlüssel) nach jeder Schutzänderung, einzelne Tresore | H1, S1, M18 und die Sync-Teile der M- und N-Punkte sind **obsolet** (die Funktion gibt es nicht mehr); die übrigen werden neu gesichtet (Paket 9 des Umbaus). Tests für Angriffe auf die Suite-Datei, gleichzeitige Drive-Änderungen, Abbrüche im Schlüsselwechsel, alte Datei nach dem Wechsel, zurückgespielten und beschädigten Index |
| 17. bis 19.11.2026 | **Umbau zu GhulDocs, offline** (Pakete G1 bis G8): GhulMail, GhulDrive, alle Server (SFTP, PHP-Modul), Freigabelinks, Schlüsselwechsel, das Hochladen von Fehlerberichten (`report.php`) und das Lesen älterer Formate entfernt; nach einer Schutzänderung öffnet die App keine Suite-Datei mit älterem Schutz mehr. Neu: Suite-Datei als Datenstrom bis 1,25 GiB mit Fortschritt; GhulDocs (Inhaltsverzeichnis im Container, Inhaltsdateien in Abschnitten zu 4 MiB mit eigenem Schlüssel je Datei, aufgefüllt auf 64 KiB, Grenzen 1 GB und 20 000 Dokumente); Ansehen ohne Klartext auf dem Datenträger; Scannen auf Android nur im Arbeitsspeicher | Die Befunde zu Mail, Drive, Servern, Freigaben und Schlüsselwechsel sind **obsolet** (rund 70 der 146 offenen Befunde der letzten Durchsichten, darunter H2, H5 und H7). Neue Tests: Angriffe auf Inhaltsdateien (vertauschte, abgeschnittene, fremde Abschnitte, falsche Füllung, falsche Länge), auf das Inhaltsverzeichnis und auf die Suite-Datei mit Dokumenten, volles GhulDocs, Abbruch beim Hinzufügen, Öffnen und Speichern; Oberfläche am Desktop mit echtem Kern, Scannen mit gezeichneten Fotos. Die unabhängige Durchsicht folgt in G9 |
| 20.11.2026 | **Mehrere GhulDocs-Tresore, ganzen Tresor weitergeben:** eine Suite hat wieder mehrere GhulDocs-Tresore, höchstens 3 (hart begrenzt, eine Suite-Datei mit mehr wird nicht geöffnet), je bis 1 GB, zusammen also bis 3 GB, und 20 000 Dokumente; die Suite-Datei bis 3,75 GiB (unter 4 GiB: kein ZIP64, FAT32). Ein hinzugefügtes GhulDocs-Paket wird ein eigener Tresor (geprüft und zwischengelegt, bevor etwas geändert wird); das Einsortieren in den einen Tresor und das Ersetzen (`docs_replace_vault`) sind entfernt. „Als Tresor weitergeben“ geht aus dem Menü jedes Tresors für den ganzen Tresor, auch für GhulCal | Weniger Code (kein Einsortieren mit neuen Kennungen, kein Ersetzen mit Sichern). Neue bzw. angepasste Tests im Kern und am Desktop, siehe [Testprotokoll](TESTPROTOKOLL.md), Abschnitt 11 |
| 20.11.2026 | **Unabhängige Durchsicht des Umbaus auf mehrere GhulDocs-Tresore** | 3 Befunde im Kern (1 hoch, 1 mittel, 1 niedrig) und 6 in der App, alle behoben; siehe Abschnitt 3 und [Testprotokoll](TESTPROTOKOLL.md), Abschnitt 11 |
| 21. und 22.11.2026 | **Großprüfung:** 25 KI-Agenten in vier Wellen (ganzheitlich, Angreifer, Nutzerwege, Methoden), Behebung durch zwei Entwickler-Agenten mit gegenseitiger Durchsicht, Runde 5 mit fünf Prüfern, Schlussprüfung ([Prüf-Chronik](PRUEFCHRONIK.md), Abschnitt 2.3) | 13 hoch, 44 mittel, dazu kleine Punkte; behoben bis auf GP-H11 (zum Teil), GP-M8, GP-M21, GP-M44 und die Restrisiken in 4.6 ([Testprotokoll](TESTPROTOKOLL.md), Abschnitt 12); GP-M8 und GP-M21 am 22.11.2026 nachgeholt (Runde 7) |
| 22.11.2026 | **App-PIN neu, Android 12, Release aus dem CI-Lauf** (Gutachten mit 2 Beratern, Runde 8), **Runde 9 mit fünf Angreifern** (Dieb und Finder, präparierte Dateien, Schadsoftware auf dem Gerät, Lieferkette, Kryptografie und Format), danach **Runden 10 bis 14** mit je einem Prüfer nur für die Behebungen ([Prüf-Chronik](PRUEFCHRONIK.md), Abschnitte 2.4 und 2.5) | Runde 9: 4 hoch, 13 mittel, 32 niedrig; Runden 10 bis 14 zusammen 1 hoch, 10 mittel, 20 niedrig; Runde 14 ohne hohen und mittleren Fund. Behoben bis auf die Punkte in 4.6 ([Testprotokoll](TESTPROTOKOLL.md), Runden 7 bis 14) |

**In Zahlen (Stand vor den Umbauten):** 149 Funde aus den Testrunden (F01 bis F149), alle behoben bzw. entschieden
(F24 ist gewollt). Alle Punkte des Audits vom 13.11.2026 sind erledigt (B1 bis B18, L1, L3, L5, L7, L10, L13) oder
begründet entschieden (L4/B17). Damals 269 Testfunktionen im Kern und 64 Angriffe auf das PHP-Modul (**obsolet**, das
Modul ist entfernt). **Heute:** die Zahl der Testfunktionen steht im [Testprotokoll](TESTPROTOKOLL.md), Gesamtstand;
über eine Million Einzelprüfungen je Lauf, Nachtlauf mit langem Fuzzing, Monkey im Emulator und Servern mit kaputtem
TLS gegen den TLS-Prüfer der beiden Verbindungen.

## 2. Wogegen GhulSuite schützt

| Angreifer | Schutz |
|---|---|
| **Dieb von Dateien** (Gerät, USB-Stick mit der Suite-Datei) | Ohne Master-Passwort **und** zweiten Faktor (Key oder Schlüsseldatei) wertlos. Argon2id mit mindestens 64 MiB. Von GhulDocs sieht er nur, wie viele Inhaltsdateien es gibt (genau: eine je Dokument, der Papierkorb eingeschlossen), wie groß sie ungefähr sind (aufgerundet auf 64 KiB) und auf dem Gerät an den Zeiten der Inhaltsdateien, wann die Dokumente hinzukamen (in der Suite-Datei stehen feste Zeiten); Namen, Ordner und Typen stehen nur im verschlüsselten Inhaltsverzeichnis |
| **Finder eines Keys oder des Notfallblatts** | Key entfernen bzw. neues Blatt erstellen und die Suite-Datei speichern: Danach öffnet es nichts mehr, was gespeichert wird, auch nicht zusammen mit einer alten Kopie der Suite-Datei (B1). Die alte Kopie selbst zeigt weiter den Stand von damals (2.1) |
| **Präparierte Dateien** (Suite-Datei, GhulDocs-Tresor, Importe) | Alles wird geprüft, bevor etwas ersetzt wird; Inhaltsdateien gegen ihre Prüfsumme und ihr Inhaltsverzeichnis, jeder Abschnitt gegen seine Nummer und die Kennung der Datei; alle Parser gefuzzt (auch der ZIP-Leser der Suite-Datei, Inhaltsdateien und Inhaltsverzeichnisse von GhulDocs: Ziele `parsers` und `docs_files` in `core/fuzz`, kurz in jedem Testlauf, lang im Nachtlauf); kein Absturz, keine Datei außerhalb des Ordners, keine Zip-Bombe. Seit 22.11.2026 (Runde 9) auch im Import: kein ZIP im ZIP, CSV höchstens 1000 Spalten und 2 Mio. Felder, `.ics` höchstens 16 MiB, jeder Tresor bleibt unter seiner Grenze, Schlüsselableitung aus fremden Dateien höchstens so teuer wie 512 MiB mal 3. Ausnahme: ein fremder Tresor mit sehr viel CBOR kann die App beim Öffnen beenden (4.6) |
| **Netzwerk** (WLAN, Provider) | Sieht nur, dass die App GitHub (Update-Prüfung) und, falls eingeschaltet, Pwned Passwords anfragt. TLS mit Zertifikatsprüfung (webpki); Updates nur mit gültiger Signatur; von Passwörtern gehen nur die ersten 5 Zeichen eines SHA-1-Werts hinaus |
| **Andere Apps auf dem Gerät** | Keine Backups, Bildschirmschutz; Dokumente bekommt eine andere App nur auf ausdrückliches „Mit App öffnen“ (eine befristete Kopie in einem eigenen Ordner, die Freigabe wird beim Sperren und vor der nächsten Kopie zurückgenommen) oder „Exportieren“ (mit Rückfrage). Eine Datei, die eine andere App übergibt, wird erst nach Rückfrage Speicherort. **Nicht geschützt:** Apps mit Zugriff auf Benachrichtigungen sehen „entsperrt“ und „gesperrt“, Erinnerungen im gewählten Umfang und können „Sperren“ und „Abbrechen“ auslösen (nicht das Speichern) |

Nicht im Bedrohungsmodell: ein bereits kompromittiertes, entsperrtes Gerät (Schadsoftware mit Root, fremde
Tastatur-App, Bedienungshilfen-Dienste, Apps mit Zugriff auf Benachrichtigungen). Dagegen schützt keine App (siehe
4.3). **Unter Windows genügt dafür Schadsoftware mit den Rechten des Benutzers** (kein Administrator): Die App ist
für den Benutzer installiert (`perUserInstall`), ihre Dateien lassen sich also verändern; so verändert liest sie beim
nächsten Entsperren Passwort und Key-PIN mit, auch wenn die Suite bei der Infektion gesperrt war (Runde 9,
Malware N-4). Ebenso die portable Version auf einem Stick, den jemand eine Weile hatte (2.1).

Klartext entsteht nur beim Ansehen eines Dokuments und beim Scannen (beides im Arbeitsspeicher, beim Verlassen und
Sperren überschrieben) und beim Exportieren oder Öffnen mit einer anderen App (befristete Kopie).

### 2.1 Suite-Datei und alte Kopien

Die Suite reist als eine Datei (`.ghlzip`); aktuell ist der Stand, der zuletzt gespeichert wurde. Jede Datei trägt ein
Schreibsiegel, das nur mit dem Passwort entsteht; die Kopfdatei nennt Zähler, Stand-ID und Schutz-Epoche. Jede
Schutzänderung erneuert die inneren Schlüssel und erhöht die Schutz-Epoche ([`KONZEPT.md`](KONZEPT.md), 8.1 und 8.2).

| Wer | Kann | Kann nicht |
|---|---|---|
| Hat den Stick, sonst nichts | Datei kopieren, löschen, durch eine ältere ersetzen | lesen; etwas unterschieben (Siegel, Prüfsummen der Inhaltsdateien); eine ältere Datei unbemerkt öffnen lassen (Zähler, Rückfrage) |
| Hat Stick und einen alten Code | lesen, was im alten Stand galt | etwas unterschieben; den aktuellen Stand lesen, sofern jedes Gerät nach dem neuen Code zuerst die neue Suite-Datei geöffnet hat (sonst siehe unten) |
| Hat den Stick der **portablen Windows-Version** eine Weile | das Programm oder die unversiegelten Einstellungen auf dem Stick austauschen: Beim nächsten Entsperren liest es Passwort und zweiten Faktor mit | nichts, wenn der Besitzer einen Stick, der aus der Hand war, nicht mehr startet und die portable Version neu von der Release-Seite lädt (Prüfsumme in `SHA256SUMS`) |
| Hat eine alte Suite-Datei (vor einer Schutzänderung) und das alte Passwort **und** den alten zweiten Faktor | die alte Kopie mit einer frischen Installation öffnen, mit dem Stand und den Dokumenten von damals | sie auf einem Gerät öffnen lassen, das den neueren Stand kennt (Schutz-Epoche, auch nicht nach Rückfrage); lesen, was danach gespeichert wurde (neue innere Schlüssel, neue Dokumente mit neuen Schlüsseln) |
| Hat eine alte Suite-Datei, aber den alten zweiten Faktor nicht mehr (Key zurückgesetzt, Schlüsseldatei vernichtet) | nichts | sie öffnen, auch nicht mit dem alten Passwort |

**Grenze ohne Server** ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md), Abschnitt 2): Eine Kopie der Suite-Datei, die schon
irgendwo liegt (Stick, Cloud, anderes Gerät), lässt sich nicht nachträglich unbrauchbar machen. Die App sagt nach
jeder Schutzänderung deutlich: alte Kopien löschen. Sicher unbrauchbar werden sie, wenn der zweite Faktor getauscht
wird (neuen hinzufügen, alten entfernen, alte Schlüsseldatei vernichten bzw. alten Key zurücksetzen) **und** ein neuer
Wiederherstellungscode erstellt wird (der bisherige steckt auch in alten Kopien).

**Neuer Wiederherstellungscode und mehrere Geräte** (Runde 9, Krypto H1): Speichert ein Gerät, das die neue Datei
noch nicht geöffnet hat, auf eine Datei mit neuerem Code, lehnt die App das Überschreiben ab (neuerer Schutz). Ohne
Server nicht zu verhindern ist aber: Ein Gerät mit altem Stand schreibt mit „Speichern unter“ eine neue Datei, und
eine alte Suite-Datei auf einem neuen Gerät öffnet mit dem aktuellen Passwort und Key; beide tragen den **alten** Code
weiter. Darum: Nach einem neuen Code öffnet jedes Gerät zuerst die neue Suite-Datei, alte Kopien werden gelöscht.

Bewusste Ausnahme: Wer nur den Wiederherstellungscode hat (Passwort vergessen), kann eine Suite-Datei öffnen, ohne
dass sich die Siegel prüfen lassen. Die App sagt das, prüft Stand und Schutz-Epoche trotzdem und versiegelt danach
alles mit dem neuen Passwort neu.

## 3. Behoben

Nach Bereich zusammengefasst; die Nummern führen ins [Testprotokoll](TESTPROTOKOLL.md) bzw. zur Tabelle 6.

| Bereich | Was behoben wurde | Nummern |
|---|---|---|
| **Synchronisation** (**obsolet**, Synchronisation am 17.11.2026 entfernt) | Eine kaputte, fehlende oder neuere Datei (auf dem Server oder lokal) löscht nie mehr einen Tresor; Löschungen nur über ausdrückliche Löschmarken. Abgerissene Uploads blockieren nichts. Geräte, die lange offline waren, verlieren nichts und überschreiben nichts, was sie nicht lesen können | F01 bis F06, F40, F41 |
| **Schlüssel und Format** | Neues Notfallblatt, neues Passwort, entfernter Key: neue innere Schlüssel in jeder Datei; ein entfernter Key erneuert das Faktor-Geheimnis für die übrigen Keys, ohne sie zu berühren (öffentlicher Schlüssel je Key, Formatversion 2). Dateinamen verraten keine Tresornamen | B1, F23 |
| **Netzwerk-Parser** | Abstürze durch präparierte HTTP-, SMTP- und FTP-Antworten (teils vor STARTTLS von jedem im WLAN auslösbar); der FTP- und der SMTP-Teil sind **obsolet** (FTP/FTPS und GhulMail entfernt), der HTTP-Teil gilt weiter für Update- und Leak-Prüfung | F07 bis F10 |
| **Mail** (**obsolet**, GhulMail am 17.11.2026 entfernt) | Kopfzeilen-Einschleusung, versteckte Link-Ziele, Autocrypt-Schlüsseltausch durch gefälschten Absender, Bilder an `localhost` und das Heimnetz | F13, F16, F17, F28 |
| **Autofill** (**obsolet**, Autofill am 16.11.2026 entfernt) | Basis-Domain zu grob (`evil.github.io`), jede App konnte eine Webadresse behaupten, Adressen mit Backslash und IPv4-Schreibweisen anders gelesen als im Browser | F18, F19, F25, F26 |
| **Andere Apps** | Private Dateien über „Per E-Mail senden“ in Entwürfe geschmuggelt (**obsolet** mit GhulMail); Dev-APK debuggbar | F20, F21 |
| **Dateien von außen** | Sicherungen mit Pfad-Tricks und Zip-Bomben (die Sicherung ist **obsolet**; dieselben und strengere Regeln gelten für die `.ghlzip`: nur genau die erwarteten Einträge, Größen beim Lesen gezählt); Dateinamen vom Server mit Gerätenamen, Doppelpunkten, Steuer- und Richtungszeichen (**obsolet**, keine Server); Drive-Namen, die den Dateityp tarnen (dieselbe Regel gilt für Namen in GhulDocs) | F11, F14, F27 |
| **Daten, die das Gerät verlassen** | CSV-Export als Formel ausführbar; Geheimnisse in Debug-Ausgaben; Passwort blieb nach dem Sperren in der Zwischenablage | F15, F22, F29 |
| **Entsperren** | App-PIN-Zähler zählte erst nach dem Versuch (Freiversuch durch Abschießen der App) | F31 |
| **CI** | Tests gegen echte Server liefen nach einem Runner-Wechsel ins Leere und wurden grün gemeldet (**obsolet**, keine Testserver mehr) | F39 |
| **Freigaben und PHP-Modul** (Audit 0.5.0; **obsolet**, entfernt am 17.11.2026) | Abgelaufene Pakete abrufbar, beliebige PBKDF2-Runden aus dem Paket, ausführbare Dateien im Datenordner, Namen mit Punkt am Ende, fehlende Kopfzeilen | A3 bis A5, F30 |
| **Freigaben ohne Kopie** (Runde 9; **obsolet**) | Einmal-Links durch Link-Scanner verbraucht, Blockliste des Servers ungeprüft, beschädigter Zähler als „nie geöffnet“ gelesen, Ticket-Flut verdrängte laufende Downloads, fehlende Prüfung von Schreibrecht und Web-Erreichbarkeit des GhulSuite-Ordners, verwaiste oder gelöschte Blöcke bei unlesbarer Freigabenliste | F110 bis F113, F115 |
| **Mail-Archiv** (Runde 9; **obsolet**) | Gefälschte Message-ID verhinderte das Archivieren der echten Mail, Richtungsmarken im Dateinamen | F114 |
| **Android-Teilen und Anhänge** (Audit 0.5.0) | Dateinamen wie `..`, Anhänge beliebiger Größe; gilt weiter für „In GhulDocs speichern“ aus anderen Apps (Namen geprüft, Grenzen von GhulDocs) | A6, A7 |
| **Sync, Runde 5** (**obsolet**, Synchronisation und „Passwort anderswo geändert“ entfernt; die Grenzen für Argon2-Werte aus Dateien gelten weiter, siehe Abschnitt 5) | Ein entfernter Key kommt nie per Sync zurück (Löschmarken je Key). Geänderte Server- und Kontodaten erreichen andere Geräte. Gelöscht auf A, geändert auf B: feste Regel, eine nie gesehene Änderung bleibt. Konfliktkopien fremder Sync-Programme werden zusammengeführt. Ein Server kann keine übermäßig teuren Argon2-Werte erzwingen | B2, B11, B13, B18, F42 |
| **Drive** (**obsolet**, GhulDrive am 17.11.2026 entfernt) | Ein zurückgesetzter Index auf dem Server wird abgelehnt (nichts verschwindet); zwei Geräte, die gleichzeitig ändern, verlieren nichts. Seit dem Umbau unter `index.lock` (exklusiv angelegt, verwaist nach fünf Minuten nach eigener Zeit und Änderungszeit des Servers), mit `index.bak` bei beschädigtem Index und automatischem Aufräumen | B3 |
| **Umbau ohne Synchronisation** (17.11.2026) | Ungeprüfte Ladewege (Sicherung, „Von meinem Server holen“, Übernahme fremder Dateien aus dem Ordner) ersetzt durch die geprüfte `.ghlzip`; Schreibsiegel gegen untergeschobene Dateien mit altem Code; ältere, abgezweigte und fremde Suite-Dateien nur nach Rückfrage, ein älterer Schutz gar nicht; Gerätedaten des alten Stands (Schnell-Entsperren, Mail-Zwischenspeicher, Drive-Offline-Kopien, unfertige Uploads) werden beim Öffnen gelöscht; alte Suite-Dateien kommen nach dem Schlüsselwechsel nicht mehr an GhulDrive und die Server (**obsolet** mit dem Umbau zu GhulDocs); kein Server-Passwort mehr in der Suite | Umbau, Pakete 2 bis 7 |
| **Umbau zu GhulDocs** (17. bis 19.11.2026) | Kein Netzzugang außer Update- und Leak-Prüfung; ältere Formate werden nicht gelesen; eine Suite-Datei mit älterem Schutz öffnet nie mehr; die Suite-Datei kommt nie ganz in den Arbeitsspeicher (Datenstrom, höchstens 1,25 GiB und 21 000 Einträge, beim Lesen gezählt); Inhaltsdateien beim Öffnen erst in einen Zwischenordner und erst nach dem Bestätigen an ihren Platz; ein Dokument wird beim Einlesen verschlüsselt, Klartext erreicht den Datenträger nie; ein volles GhulDocs lehnt ab, bevor etwas geschrieben wird; Scans nur im Arbeitsspeicher, überschrieben beim Speichern, Verlassen und Sperren | Umbau, Pakete G1 bis G7 |
| **Durchsicht G9** (19.11.2026, unabhängig, 14 Befunde) | Windows: PDF-Seiten (JPEG eines Scans) kamen über den Zwischenspeicher von ImageIO in den Temp-Ordner, jetzt nur im Arbeitsspeicher. Der Zwischenordner einer wartenden Prüfung wurde nach einem Tag aufgeräumt und das Bestätigen verlor dann alle Dokumente; jetzt nie aufgeräumt, und vor dem Ersetzen muss jede Inhaltsdatei da sein. Speichern konnte eine Suite-Datei über 1,25 GiB schreiben, die sich nie öffnen ließ; jetzt vorher abgelehnt, die Tresore neben GhulDocs höchstens 200 MiB. Die allgemeinen Eintragsfunktionen (kopieren, verschieben, Favorit, Anhänge, 2FA umbenennen) lehnen GhulDocs ab. Ein übernommener GhulDocs-Tresor, der sich nicht speichern ließ, blieb halb da. Endgültiges Löschen während des Speicherns ließ die Datei scheitern (unter Android nach dem Kürzen der alten); jetzt wartet das Löschen der Inhaltsdatei, bis die Datei geschrieben ist. Klartext-Puffer der Datenströme werden überschrieben. Namen anderer Apps mit Richtungszeichen werden bereinigt statt abgelehnt. Der PDF-Betrachter schließt, was beim Verlassen noch öffnete. Windows: zweiter Start sicher erkannt (Sperre auf den Datenordner als Erstes), Übergabe begrenzt. Neue Parser im Fuzzing. Dateien älterer Entwicklerversionen werden als solche gemeldet; Reste entfernter Funktionen auf Android einmal entfernt. Bisherige Dokumente, die sich beim Ersetzen nicht zurücklegen lassen, werden nie gelöscht | G9-1 bis G9-14 |
| **Mehrere GhulDocs-Tresore** (20.11.2026) | Höchstens 3 GhulDocs-Tresore: ein vierter wird abgelehnt, wo immer er hinzukäme, eine Suite-Datei mit mehr wird nicht geöffnet; Grenzen je Tresor (1 GB) und für alle zusammen (3 GB, 20 000 Dokumente, 3,3 GB Inhaltsdateien) greifen, bevor etwas geschrieben wird, auch beim Hinzufügen eines Pakets; die größte Suite-Datei (3,75 GiB) mit vollen GhulDocs-Tresoren und 200 MiB übrigen Tresoren bleibt mit ihrem ZIP-Verzeichnis unter 4 GiB; ein Paket wird erst nach vollständiger Prüfung ein eigener Tresor, und ein Tresor, der sich nicht speichern lässt, bleibt nicht halb da; dasselbe Paket zweimal wird abgelehnt | `several_docs_vaults_each_on_their_own`, `the_limit_holds_with_the_trash`, `an_adopted_docs_vault_that_cannot_be_saved_is_not_half_added`, `a_full_suite_fits_into_the_largest_file` |
| **Durchsicht Mehrere GhulDocs-Tresore** (20.11.2026, unabhängig) | **Hoch:** Ein manipuliertes Paket, dessen Kennung in den ersten 8 Byte mit einem vorhandenen Tresor übereinstimmt (der Dateiname enthält nur 16 Hex-Zeichen), konnte dessen Tresordatei überschreiben; jetzt lehnt das Hinzufügen in allen Bereichen jeden Tresor ab, dessen Dateiname schon vergeben ist, auch durch eine nicht lesbare Datei im Ordner. **Mittel:** Ein Paket mit derselben Kennung wie ein beschädigter, nicht lesbarer GhulDocs-Tresor löschte dessen Dokumentordner; jetzt „schon vorhanden“, nichts wird gelöscht. **Niedrig:** Eine Suite mit mehr Einträgen, als gelesen werden (21 000), wird nicht mehr geschrieben. Dazu App-Funde: Kopfzeile bei „Alle Tresore“, Belegung in den Optionen, „fast voll“ ohne Angabe, was voll ist, springender Kopf bei der Auswahl, „Vorher exportieren“ unverschlüsselt, leerer Tresor weitergeben | `adopting_never_overwrites_a_vault_with_the_same_file_name`, `adopting_never_removes_a_damaged_docs_vault`, `a_file_with_too_many_entries_is_not_written`. **Mittel:** Ein ganz weitergegebener GhulCal-Tresor behielt seine Kennungen (zurück in derselben Suite traf Löschen den falschen Kalender), jetzt neue Kennungen, `a_whole_vault_is_handed_on`; ein Docs-Tresor, der während des Speicherns entfernt und gleich wieder hinzugefügt wurde, verlor seine Dokumente, jetzt abgelehnt, bis der alte Ordner weg ist, `a_vault_added_again_while_saving_never_loses_its_documents` |
| **Android** | Benachrichtigungen, die bei gesperrtem Gerät eintreffen, enthalten nur ihre Sperrbildschirm-Fassung (Android lässt Apps die Kanal-Einstellung nicht vorgeben, daher entscheidet die App beim Eintreffen); Mail-Zugang für Benachrichtigungen bevorzugt in StrongBox, wahlweise löscht ihn die Sofortsperre (**obsolet** mit GhulMail); `.ics` von fremden Apps nur nach Vorschau; interne Aktionen nur aus eigenen Benachrichtigungen; Auto-Sperre zählt die echte Zeit; Autofill fragt vor dem Verknüpfen; kein Tippen durch darübergelegte Fenster | B4 bis B9, L10, F43 |
| **Großprüfung** (21. und 22.11.2026; Nummern GP-… im [Testprotokoll](TESTPROTOKOLL.md), Abschnitt 12) | **Behoben:** Signaturschlüssel nur noch in den Environments `signing` und `release`, sonst ein Wegwerf-Schlüssel, kein Release von Hand (GP-H1, GP-H2, 4.1); nach dem Notfallblatt reicht bis zum Sperren das neue Passwort für den Tausch der Faktoren (GP-H3); „Mit App öffnen“ lehnt Programme ab, Mark-of-the-Web (GP-H4); Löschen und Ersetzen der Suite nur mit Nachweis (GP-H5); Schnell-Entsperren nur mit beiden Faktoren, Gültigkeit fest in der Schnell-Datei Version 3, abgelaufen heißt abgelehnt statt gelöscht (GP-H6, GP-M30); alte `.bak`-Kopien nach Schutzänderungen entfernt (GP-H7); Tags, Ordner und unbekannte Felder bleiben beim Bearbeiten (GP-H8, GP-H9); Sperren während des Entsperrens gewinnt (GP-H10); freier Platz vorher geprüft, „voll“ am Fehlercode (GP-H12); Update-Prüfung als Ganzes getestet, je Schutzänderung „nur Passwort → abgelehnt“ (GP-H13); 2FA-Geheimnisse als QR und Abschwächen der Sperre nur nach dem Master-Passwort (GP-M13, GP-M14); Weitergabe als Tresor mit beiden Faktoren (GP-M15); Passwort- und PIN-Regeln im Kern (GP-M42); CSV ohne Apostroph, Tabellenschutz als Schalter (GP-M29); Grenzen für präparierte Pakete und ICS (GP-M25 bis GP-M27); Gradle-Wrapper und -Verteilung mit Prüfsumme (GP-M23); Website-Zugangsdaten im Environment `website` (GP-M22); Texte, die mehr versprachen als der Code (GP-M35 bis GP-M41) | GP-H1 bis GP-H13, GP-M1 bis GP-M43 (außer GP-M8, GP-M21) |
| **Angreifer-Runde und Runden 10 bis 14** (22.11.2026; [Prüf-Chronik](PRUEFCHRONIK.md) 2.5, Nummern R9-… bis R14-… im [Testprotokoll](TESTPROTOKOLL.md)) | **Behoben:** Eine Schutzänderung wird ganz versiegelt, bevor etwas geschrieben wird, und über ein Journal geschrieben (Kopien `.prev`, Zurückschreiben nach einem Fehler oder Abbruch, gebunden an die Suite, beendet beim Löschen und Neuanlegen, geschützt durch eine Sperre und einen Zähler gegen gleichzeitiges Entsperren, Löschen und Übernehmen); Duplizieren, Importieren, Verschieben und neue 2FA-Konten halten die Grenze je Tresor und nehmen bei Fehlern zurück; eine größere Schutz-Epoche am Ziel (auch nach einem neuen Code) ist „neuerer Schutz“; „Neue Suite mit diesem Tresor“ bekommt einen eigenen Code; das Windows-Update startet PowerShell und `msiexec` mit vollem Pfad; `publish` nimmt nur die zwei Release-Artefakte und prüft ihre Prüfsummen, Python-Pakete mit Hash, der echte Schlüssel baut ohne wiederhergestellte Caches, Fuzzing und Stresstest vor jedem Release, keine Release-Notizen aus PR-Titeln, Abbruch bei einem fremden Tag; Freigaben an andere Apps werden zurückgenommen; dauerhafte Freigaben erst nach Rückfrage; USB-Empfänger nicht exportiert; UNC-Pfade in der Übergabe abgelehnt; einfache App-PINs, ZIP im ZIP, riesige CSV und ICS, CTAP-2.0-Keys ohne `credProtect` und teure Schlüsselableitungen abgelehnt; nur Slots mit der eigenen RP-ID werden angefragt (K-20); ganzer Wiederherstellungscode bestätigt, in einem Feld, das die Tastatur nicht lernt; Leerlauf-Sperre im Vordergrund und Sperre nach dem Ruhezustand; Master-Passwort vor „Papierkorb leeren“ und vor dem Löschen von 10 oder mehr Einträgen; kein stiller Rückfall auf die Zwischenablage mit Verlauf; gespeicherte Dateien werden zurückgelesen; Speicherfehler und ein Wettlauf beim Entsperren schalten das Schnell-Entsperren nicht mehr ab | R9-Dateien-H1, R9-Malware-H1, R9-Lieferkette-H1, R9-Krypto-H1, R11-H1, R12-M1 und die übrigen |
| **App-PIN, Android 12, Release** (22.11.2026, nach der Großprüfung; [Prüf-Chronik](PRUEFCHRONIK.md) 2.4) | Die App-PIN war mit Root außerhalb des Geräts durchprobierbar; jetzt zwei Einmal-Schlüssel im Keystore, an die PIN gebunden, 8 bis 12 Ziffern, 2 Versuche (KONZEPT 6.5, Restrisiko GP-M19). Mindestversion Android 12 (API 31). Der Release startete seit PR #50 nicht mehr (`workflow_run` lief nur mit der Fassung von `main`); jetzt ruft ihn der letzte Job der CI auf (4.2, K-7) | GP-M19, K-7 |
| **Windows** | Entschlüsselte Kopien werden überschrieben und gelöscht, auch wenn ein Programm sie erst später freigibt; Arbeitsverzeichnis nicht mehr im DLL-Suchpfad. **Richtiggestellt am 22.11.2026** (Runde 9, Malware N-6): Hier stand „portabel bleiben sie im Datenordner“; seit GP-M16 liegen sie auch portabel im Temp-Ordner des PCs, nie auf dem Stick | B10, L13, GP-M16 |
| **Mail** (Runde 5; **obsolet**) | Herkunft von Kontaktschlüsseln gemerkt; Fingerabdruck vergleichen und bestätigen, Warnung bei Wechsel; Bilder nur von genau der geprüften Adresse (kein DNS-Rebinding) | B12, B16, L7 |
| **PHP-Modul und Freigaben** (Runde 5; **obsolet**) | Gültig signierte Anfragen werden nie gesperrt, Doppel-Endungen abgelehnt, Dateien 0600; Server ohne PHP-Ausführung wird erkannt und die Datei sofort gelöscht; Zurückziehen meldet Fehler ehrlich | B14, B15, L5 |
| **Hinweise** | Nach einem Verlust sagt die App, was zu wechseln ist (Mail- und Server-Passwörter; **obsolet** ohne Mail und Server, heute: alte Kopien löschen, zweiten Faktor tauschen) | L1 |
| **Abhängigkeiten** | `russh` (Speicher-Angriff durch bösartigen SSH-Server; **obsolet**, entfernt), `zip`, `crypto-bigint` über `p256` auf sichere Versionen | A1, A2, R1 |

## 4. Offen

**Aus der Großprüfung (21. und 22.11.2026) offen:** GP-H11 zum Teil, GP-M44 und die Restrisiken in
4.6; Einzelheiten im [Testprotokoll](TESTPROTOKOLL.md), Abschnitt 12. **Aus den Runden 9 bis 14 offen:** R14-N2,
R9-Krypto-N2 (Klartext-Export mit dem Passwort allein, Entscheidung steht aus), R9-Krypto-N4 (nur als Handtest),
R9-Lieferkette-M3 und die Restrisiken in 4.6; der Gesamtstand steht oben im Testprotokoll.

Aus den Testrunden und Audits bis 16.11.2026 ist **nichts mehr offen**. Von den 146 offenen Befunden der letzten
Durchsichten entfallen mit Mail, Drive und Servern rund 70 (darunter H2, H5 und H7); die übrigen (vor allem Sperren und
Entsperren, Suite-Dateien, GhulCal mit H8, GhulPass-Import mit H4, Windows und Update) werden in Paket G9 von
[`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md) neu gesichtet, zusammen mit einer unabhängigen Durchsicht von GhulDocs und
dem Scannen. Was sonst bleibt, sind zwei bewusste Entscheidungen und die Grenzen, die keine App lösen kann (4.3).
Nichts davon gibt einem Angreifer ohne Passwort **und** zweiten Faktor Zugriff auf Tresorinhalte.

### 4.1 Bewusst entschieden

| Nr. | Entscheidung | Begründung |
|---|---|---|
| L4, B17 | **Ein Signaturschlüssel für Testversionen von `main` und `dev` und für Veröffentlichungen**, ein getrennter Release-Schlüssel ist nicht vorgesehen; die Schlüssel bleiben (Entscheidung des Inhabers vom 21.11.2026, nach dem Gutachten und erneut nach der Großprüfung) | Die Testversionen von `main` und `dev` sollen sich über die installierte App aktualisieren lassen; mit einem anderen Schlüssel müsste man sie erst deinstallieren und verlöre die Daten auf dem Gerät. Ein zweiter Schlüssel schützte vor allem gegen einen übernommenen GitHub-Zugang, und mit dem ließe sich auch `main` ändern. **Schutz seit der Großprüfung (GP-H1, GP-H2):** Der Schlüssel liegt nur noch in zwei GitHub-Environments: `signing` (APK-Bau in `android.yml` bei einem Push auf `main` oder `dev`, Deployment-Branches `main` und `dev`) und `release` (Jobs `android` und `publish` in `release.yml`, Deployment-Branches `main` und `dev`, seit 22.11.2026; vorher nur `main`). Jeder andere Lauf (Arbeits-Branches, Pull Requests, Forks, manueller Start von `android.yml`) signiert mit einem Wegwerf-Schlüssel, den der Gradle-Schritt selbst erzeugt; Keystore und Passwort gibt es nur in diesem Schritt. Branch- und PR-APKs wechseln darum bei jedem Lauf die Signatur (vor dem Installieren die `.dev`-App deinstallieren) und können die echte App nicht ersetzen (`applicationIdSuffix = ".dev"`). `release.yml` hat keinen Start von Hand. Der Job `secrets-check` meldet Signatur- und Website-Secrets, die noch auf Repository-Ebene liegen (Fehler bei einem Push auf `main` oder `dev`, sonst eine Warnung). Die Environments und ihre Deployment-Branches richtet der Inhaber in GitHub ein; das ist aus dem Repository heraus nicht nachprüfbar. Weiterhin gilt: Sein GitHub-Konto ist mit einem Hardware-Key gesichert, veröffentlicht wird nur nach grüner CI. Voraussetzungen: Zwei-Faktor-Anmeldung bei GitHub (Passkey oder Security Key), eine Offline-Sicherung der Keystore-Datei und ihres Passworts (ohne sie lässt sich keine installierte App mehr aktualisieren), Abhängigkeiten nur bewusst aktualisieren (Gradle-Prüfsummen, `Cargo.lock`) |
| F24 | Titel werden ohne Leerraum am Rand gespeichert | Absicht (Suche, Sortierung, Anzeige); alles Geheime bleibt Zeichen für Zeichen erhalten |

### 4.2 Nachprüfbarkeit des Builds

- **Abhängigkeiten der App (L3, erledigt):** `app/gradle/verification-metadata.xml` enthält die SHA-256-Prüfsumme
  jedes Gradle-Pakets (607 Pakete, auch die Windows-Varianten). Ein Paket, das unterwegs verändert wurde oder neu
  dazukommt, bricht den Build ab. Neue oder aktualisierte Abhängigkeiten: `./gradlew --write-verification-metadata
  sha256 help` und die Änderung der Datei im Pull Request prüfen. Für den Rust-Kern sichert `Cargo.lock` die
  Versionen; `cargo audit` prüft sie in der CI und nachts auf bekannte Lücken (ein `cargo deny` mit Regeln für
  Quellen und Lizenzen ist nicht eingerichtet). Die Python-Pakete der CI (Testvektoren, Maestro-Prüfung, Bilder)
  stehen mit Version und Hash in `tools/requirements/*.txt` (erzeugt mit `pip-compile --generate-hashes`) und werden
  mit `--require-hashes` installiert (seit 22.11.2026, Runde 9).
- **Gradle selbst (GP-M23):** Vor jedem `gradlew` prüfen `android.yml` und `release.yml` die SHA-256 von
  `gradle-wrapper.jar` (gegen die Prüfsumme von services.gradle.org); scheitert sie, läuft Gradle nicht. Die
  heruntergeladene Verteilung prüft `distributionSha256Sum` in `app/gradle/wrapper/gradle-wrapper.properties`.
- **Reproduzierbarkeit (L4, B17):** Der Nachtlauf baut den Kern zweimal in verschiedenen Ordnern und verlangt
  byte-gleiche Ergebnisse. Für die ganze APK (Kotlin, Ressourcen) ist das noch nicht nachgewiesen; das gehört zu
  festen Releases (Issue #6). Seit 21.11.2026 legt `rust-toolchain.toml` eine Rust-Version für lokale Builds, CI
  und Releases fest: Veröffentlicht wird, was getestet wurde.

- **Updates (Runde 7):** Der Updater ist ein neuer Weg, Code auf das Gerät zu bringen. Geschützt durch: signierte
  Prüfsummen (RSA oder P-256, geprüft mit `ring`), Größe und SHA-256 der Datei vor der Übergabe an den Installer,
  nur `https` zu `api.github.com`, `github.com` und `*.githubusercontent.com` (auch Weiterleitungen), Download
  höchstens 600 MB in eine Datei statt in den Speicher, Entwürfe und auf dem stabilen Kanal Vorabversionen werden
  nie angeboten, Installation nur nach Zustimmung. Wer den Signaturschlüssel hat, kann Updates ausliefern; das war
  mit der APK-Signatur schon vorher so (AUDIT 4.1). **Richtiggestellt am 21.11.2026:** Hier stand, ein
  kompromittiertes GitHub-Konto allein reiche nicht. Das stimmt nicht. Wer Schreibrecht hat, kann einen Workflow
  ändern und damit signieren. Ein übernommenes Konto des Inhabers reicht also für ein signiertes Update. **Seit der
  Großprüfung** liegt der Schlüssel nicht mehr auf Repository-Ebene, sondern nur in den Environments
  `signing` und `release` (4.1): Ein Push auf einen anderen Branch als `main` oder `dev` erreicht ihn auch mit einem
  geänderten Workflow nicht mehr; wer auf `main` oder `dev` schreiben kann, weiterhin. Schutz dagegen: Schreibrecht
  hat nur der Inhaber, sein Konto ist mit einem Hardware-Key gesichert; veröffentlicht wird nur nach einer grünen CI
  für genau diesen Commit, kein `workflow_dispatch`. Seit dem 22.11.2026 ruft der letzte Job von `android.yml`
  (`release`) den Release-Workflow selbst auf (`workflow_call`), nur bei einem Push auf `main` oder `dev` und nur,
  wenn alle Pflicht-Checks grün sind und kein Job scheiterte; es gilt also die `release.yml` des gepushten Commits.
  Ein Pull Request erreicht den Job nie, und seine geänderte Fassung käme nicht an den Schlüssel (das Environment
  `release` lässt nur `main` und `dev` zu). Veröffentlicht wird nur, wenn der Commit noch der Stand des Branches ist
  (ein erneuter Lauf eines älteren Commits gibt nichts heraus). Vorher startete der Release über `workflow_run`; das
  läuft immer mit der Fassung des Standardzweigs `main`, und dort gab es `release.yml` noch gar nicht: Seit PR #50
  entstand darum keine Vorabversion mehr. Der öffentliche Teil des Schlüssels kommt aus der
  Repository-Variable `GHUL_UPDATE_KEY`; `publish` prüft die Signatur dagegen, bevor etwas veröffentlicht wird. Die signierte Prüfsummenliste nennt ihre Version, die App prüft sie: Eine alte, gültig signierte
  Datei lässt sich nicht unter einem neuen Tag anbieten (K-6). Rücksprung auf ältere Versionen bietet die App nie
  an (nur neuere).

### 4.3 Grenzen, die keine App lösen kann

| Nr. | Grenze |
|---|---|
| L6 | Der Server sah Anzahl, Größe und Änderungszeit der Dateien und die IP-Adresse (**obsolet**, keine Server mehr). Ähnlich sichtbar ist heute in der Suite-Datei und der Arbeitskopie: Anzahl und ungefähre Größe der Dokumente (aufgerundet auf 64 KiB), nicht ihre Namen |
| - | **Alte Kopien ohne Server** ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md), Abschnitt 2): Eine Kopie der Suite-Datei, die schon irgendwo liegt, lässt sich nachträglich nicht unbrauchbar machen. Wer das alte Passwort **und** den alten zweiten Faktor hat, öffnet sie mit einer frischen Installation (wie bei VeraCrypt), mit dem Stand und den Dokumenten von damals. Abhilfe: alte Kopien löschen (die App sagt das nach jeder Schutzänderung) und den zweiten Faktor tauschen |
| - | Wer den Stick hat, kann die Suite-Datei löschen oder durch eine ältere ersetzen. Die App erkennt das beim Öffnen, gegen den Verlust helfen nur Kopien an einem zweiten Ort |
| - | Das Schreibsiegel beweist, wer eine Datei geschrieben hat, nicht dass sie aktuell ist. Ein Gerät, das den neueren Stand nie gesehen hat, öffnet auch eine ältere Datei (ohne Rückfrage, es kennt nichts Neueres) |
| - | Exportierte Dokumente und die befristeten Kopien für „Mit App öffnen“ sind Klartext. Die App fragt vor dem Export und löscht die befristeten Kopien beim Sperren; was die andere App mit ihrer Kopie macht (eigene Zwischenspeicher, zuletzt geöffnete Dateien), liegt bei ihr |
| - | Ein Scan mit vielen Seiten braucht viel Arbeitsspeicher; die App warnt und nimmt keine weitere Seite, wenn er nicht reicht. Ob das System Teile des Arbeitsspeichers auslagert (Windows) bzw. ob eine Kamera-Bibliothek Zwischenpuffer anlegt, liegt außerhalb der App (siehe L8) |
| L8 | Geheimnisse im Arbeitsspeicher der Oberfläche (Kotlin-Zeichenketten lassen sich nicht sicher löschen), Auslagerungsdatei und Ruhezustand unter Windows. Nur der Rust-Kern löscht sicher; Geräteverschlüsselung nutzen |
| L9 | Schadsoftware mit Root, fremde Tastatur-Apps, Bedienungshilfen-Dienste, Apps mit Zugriff auf Benachrichtigungen (sie sehen den Zustand und Erinnerungen, können „Sperren“ und „Abbrechen“ auslösen); unter Windows schon Schadsoftware mit den Rechten des Benutzers (Abschnitt 2) |
| - | Druck-Apps: Das Notfallblatt geht beim Drucken an den gewählten Druckdienst, oft eine App des Herstellers. Die App rät, ohne Zusatz-App zu drucken oder den Code abzuschreiben |
| L11 | Zertifikats-Widerruf wird nicht geprüft (wie bei den meisten Apps); betrifft nur die Update- und die Leak-Prüfung, Updates prüft die App zusätzlich mit ihrer Signatur |
| L12 | Zeit-Seitenkanäle sind nicht gemessen; Vergleiche von Geheimnissen laufen in konstanter Zeit |
| - | Passwort vergessen heißt Daten weg, wenn auch das Notfallblatt fehlt. Es gibt bewusst keine Hintertür |

### 4.4 Prüfwerkzeuge im Nachtlauf

Alle früher als „noch nicht eingebaut“ geführten Werkzeuge laufen jetzt jede Nacht (`.github/workflows/nightly.yml`):

| Werkzeug | Was es prüft | Ergebnis beim Einbau |
|---|---|---|
| **x509-limbo** (C2SP) | 9.807 Zertifikatsketten gegen den TLS-Prüfer der App | 9.707 wie erwartet; 54 Fälle, in denen die Bibliothek bewusst großzügiger ist (z. B. Felder des Wurzelzertifikats, Namen nur aus dem SAN, schwache Schlüssel, die der Handshake danach ablehnt), stehen begründet in `core/ghul-core/tests/limbo-known.txt`. Keiner lässt eine falsche Signatur, einen falschen Namen, eine kaputte Kette oder ein abgelaufenes Zertifikat durch. Jeder neue Fall bricht den Lauf ab |
| **OpenPGP mit GnuPG** | Verschlüsseln, Signieren und Entschlüsseln in beide Richtungen | **entfernt** mit GhulMail (17.11.2026) |
| **OWASP ZAP** (Baseline) | PHP-Modul und Empfängerseite | **entfernt** mit dem PHP-Modul (17.11.2026) |
| **MobSF** (statisch, MASVS) | die fertige APK | Bericht als Artefakt, schwere Befunde als Warnung |
| **Intent-Angriffe** (statt drozer) | jede von außen erreichbare Komponente im Emulator: falsche und riesige Extras, interne Aktionen ohne Kennung, fremde Datei- und Inhalts-Adressen | bricht bei Absturz, ANR oder wenn eine geschützte Komponente einen fremden Aufrufer annimmt ab |
| **Reproduzierbarer Kern** | zwei Builds in verschiedenen Ordnern | müssen byte-gleich sein |

Bewusst nicht automatisiert: eine Geräte-Matrix (Samsung, Pixel, Xiaomi; Android 12 bis 16). Dafür braucht es echte
Geräte oder einen bezahlten Gerätedienst; bis dahin deckt der Testplan die Hersteller-Eigenheiten von Hand ab.

### 4.5 Kleine Punkte aus den Prüfrunden vom 21.11.2026

Fünf Runden mit je zwei unabhängigen Prüfern über das ganze Projekt. Regel: „hoch“ und „mittel“ werden sofort
behoben (Testprotokoll PR-…), „klein“ (Randfall, Komfort, Werkzeug) kommt hierher und wird später erledigt.

| Nr. | Bereich | Punkt |
|---|---|---|
| K-1 | Windows | Der portable Modus hinterlässt Spuren auf dem PC: zuletzt benutzte Ordner und Dateinamen der Windows-Dateidialoge (Registry, ComDlg32), die von JNA entpackte DLL und `hsperfdata` in `%TEMP%`. Inhalte der Suite sind nicht darunter |
| K-2 | Release | **Erledigt (Großprüfung):** Ab der 99. Vorabversion einer Version bekämen Builds denselben `versionCode` (Begrenzung auf 98); `release.yml` bricht dann ab und verlangt, `VERSION` anzuheben |
| K-3 | CI | Ein sofort abgebrochener Monkey-Lauf gilt als „ohne Absturz“; ein leeres Systemprotokoll im Emulator nur als Warnung |
| K-4 | Werkzeuge | `precheck.sh` räumt relativ zum Aufrufordner auf und meldet bei übersprungenen Prüfungen trotzdem „Alles grün“ |
| K-5 | Werkzeuge | `intent-attacks.sh` greift noch Aktionen von GhulMail und GhulDrive an, die es nicht mehr gibt; der deutsche Rundgang (`tour-de.yaml`) zeigt GhulDocs und zwei Optionsseiten nicht |
| K-6 | Update | **Erledigt (21.11.2026):** Die signierte Prüfsummenliste nennt ihre Version (`version x.y.z`), die App lehnt eine Liste für eine andere Version ab (`the_signed_checksums_name_their_version`) |
| K-7 | Release | **Erledigt (21.11.2026, geändert am 22.11.2026):** Veröffentlicht wird nur nach einer grünen CI für genau diesen Commit (Kern, Android, Windows; der Emulator läuft bei einem Push nur auf `main`, auf `dev` wird er übersprungen): Der letzte Job von `android.yml` ruft `release.yml` auf (`workflow_call`), das selbst prüft, ob der Commit noch der Stand des Branches ist und ob sich an der App etwas geändert hat. Bis zum 22.11.2026 über `workflow_run`, das nur die Fassung von `main` ausführte, wo es `release.yml` noch nicht gab (seit PR #50 keine Vorabversion mehr). Der Schutz des Branches `dev` ist im Repository weiterhin nicht nachprüfbar |
| K-8 | GhulDocs | „Mit App öffnen“: Dateinamen mit `: * ? " < > |` (von Android) lassen sich unter Windows nicht öffnen; zwei gleichnamige Dokumente teilen sich einen Pfad |
| K-9 | GhulDocs | Wird während eines Speicherns der Papierkorb geleert und zugleich ein älterer Stand übernommen, kann das aufgeschobene Löschen Inhaltsdateien des neuen Stands treffen (die App lässt Öffnen während des Speicherns ohnehin nicht zu) |
| K-10 | Scan | Zurück während des Speicherns eines Scans: scheitert das Speichern danach, sind die Seiten schon gelöscht |
| K-11 | GhulPass | Die Leak-Prüfung läuft nach dem Ausschalten bis zum Ende der laufenden Runde weiter (nur 5-stellige Hash-Präfixe, kein Klartext) |
| K-12 | GhulPass | Eigene geheime Felder (z. B. „PIN“ aus einem Import) stehen im Editor im Klartext, in der Ansicht verdeckt |
| K-13 | GhulAuth | Beim Scannen eines Google-Authenticator-Exports werden übersprungene Konten (HOTP, ungültig) nicht genannt; MD5-Konten werden still als SHA1 übernommen |
| K-14 | GhulPass/Auth | `steam://` im TOTP-Feld eines Logins wird nicht erkannt (das Feld fehlt dann in der Ansicht); Steam-Konten werden als `algorithm=STEAM` exportiert, leerer Aussteller als `&issuer=` |
| K-15 | GhulCal | Ein Termin, der in einen anderen Kalender-Tresor verschoben wird, wird in der Quelle zuerst gelöscht (bei einem Schreibfehler am Ziel fehlt er) |
| K-16 | GhulCal | Android-Erinnerungen von Serien mit eigener Zone kommen nach einem Zonenwechsel bis zum nächsten Entsperren zur falschen Zeit; ein verschobener Einzeltermin einer solchen Serie kann den falschen Tag streichen |
| K-17 | Import | Eine Zeile, die mit „otpauth“ beginnt (auch in einer Notiz), macht eine ganze CSV-Datei zur otpauth-Liste (die Vorschau zeigt dann 0 Einträge) |
| K-18 | GhulCal | ICS-Export: „monatlich am 31.“ endet in anderen Apps anders als in GhulCal (RFC 5545 lässt kurze Monate aus); `UNTIL` als Ortszeit neben `TZID`, kein `VTIMEZONE` |
| K-19 | Kern | **Erledigt (21.11.2026):** Abgeleitet wird mit höchstens 512 MiB (die App legt höchstens 256 MiB an); eine Datei, die mehr verlangt, wird vor dem Ableiten abgelehnt (`a_key_derivation_beyond_what_the_app_uses_is_refused_at_once`) |
| K-20 | Kern | **Erledigt (22.11.2026, Runde 9):** Die RP-ID eines Key-Slots kam ungeprüft aus der Datei (unter Windows zeigte der Systemdialog dann eine fremde Domain an; nichts verließ das Gerät); jetzt werden nur Slots mit `ghulsuite.invalid` angefragt (`a_slot_of_another_rp_id_is_not_asked_for`) |
| K-21 | Windows | Die PDF-Ansicht begrenzt die Seitenhöhe nicht (ein präpariertes PDF kann den Speicher ausschöpfen) |
| K-22 | Android | Der Absturzbericht erscheint vor dem Bildschirmschutz und wird ohne „sensibel“-Markierung kopiert (er enthält nur Stacktraces) |
| K-23 | Website | Der Hinweis „Keine Cookies“ bleibt unsichtbar in der Tab-Reihenfolge; Kontrast der blassen Schrift knapp unter WCAG AA; doppelt vorgelesene Laufband-Liste und Bildtexte |
| K-24 | Texte | „Alles wird nur auf diesem Gerät geprüft“ auch bei eingeschalteter Leak-Prüfung; Hinweis der Sperr-Benachrichtigung „Schütteln oder ‚Sperren‘ sperrt sofort“; Reste von GhulMail/GhulDrive im Testplan; Hinweis zu Testschlüsseln auf der Download-Seite des Testservers (**obsolet:** Testserver und `public/` entfernt) |
| K-25 | Website | Ein früher hochgeladenes `report.php` könnte auf dem Webspace noch liegen (der Upload löscht nichts): von Hand prüfen und löschen |
| K-26 | Kern | `create_from_vault` und `share_as_vault` legen noch Suiten bzw. Tresordateien mit einem einzelnen Key ohne Backup an; die App fordert danach beim Entfernen ein Backup, verlangt es aber nicht beim Anlegen |
| K-27 | App | Das Löschsymbol eines Faktors erscheint auch, wenn Entfernen abgelehnt wird (letzter Faktor, letztes Backup); der Hinweis „Backup fehlt“ steht doppelt (Liste und Dialog) |
| K-28 | Kern | `FactorProof` (Passwort, Key-PIN, Schlüsseldatei) wird über UniFFI kopiert und nicht ausdrücklich überschrieben; Kopien liegen bis zur Speicherbereinigung im Speicher |
| K-29 | Kern | Scheitert beim Übernehmen einer Suite-Datei das Zurückrollen der Arbeitskopie, wird dieser zweite Fehler nicht gemeldet (der erste schon) |
| K-30 | Tests | Fehlende Oberflächentests: Backup-Schritt im Assistenten mit zweitem Key, Konflikt „anderer Schutz“ im Speichern-Dialog, Hinweis beim Öffnen einer schreibgeschützten Datei |
| K-31 | GhulDocs | Mehrere Dokumente auf einmal verschieben oder löschen fehlt (nur einzeln) |
| K-32 | App | Barrierefreiheit: einigen Symbolknöpfen fehlt ein Text für Screenreader; die blasse Hinweisschrift liegt knapp unter WCAG AA |
| K-33 | App | **Teilweise erledigt (21.11.2026):** Unter Android schreibt die App, wenn der Ordner der Suite-Datei einmal freigegeben ist (Stick, SD-Karte, Gerätespeicher), die neue Datei daneben, prüft sie und tauscht erst dann; vorher bleibt die alte unberührt, ein voller Stick scheitert ohne Schaden. Ohne Freigabe und bei Cloud-Apps wird weiter direkt überschrieben (mit Hinweis). Auf einem Gerät noch nicht geprüft (Testplan E19e) |
| K-34 | Update | Der Installer prüft nicht noch einmal auf eine neuere Version, wenn zwischen Prüfung und Start Zeit vergangen ist |
| K-35 | Format | **Erledigt (21.11.2026):** Format 3 ist eingefroren, jede spätere Version liest es (Zusage in FORMAT.md, echte Dateien als Testfälle in `core/ghul-core/tests/fixtures/format3`) |
| K-36 | Kern | Entfernen zwei Geräte unabhängig denselben Key, haben beide dieselben Slots, aber verschiedene Faktorgeheimnisse; öffnet das eine die Datei des anderen, bleibt das Schnell-Entsperren eingerichtet und scheitert beim nächsten Mal (dann Passwort und Key; kein Zugang für Fremde) |
| K-37 | Android | Nach dem ersten sicheren Speichern hängt der Zugriff auf die Suite-Datei an der Freigabe des Ordners (Android nimmt beim Umbenennen den Zugriff auf das einzelne Dokument weg). Wird die Freigabe zurückgenommen oder von Android verdrängt (höchstens 128 dauerhafte Freigaben), ist die Datei „nicht erreichbar“, bis der Ordner wieder freigegeben oder die Datei neu gewählt wird; kein Datenverlust. Gezielt „Ordner erneut freigeben“ anbieten. Auf einem Gerät prüfen, ob die Kennung nach dem Umbenennen dieselbe Form hat wie aus der Dateiauswahl (Testplan E19e) |
| K-38 | Android | Während eines sicheren Speicherns sehen gleichzeitige Leser kurz keine Suite-Datei (höchstens eine falsche Meldung „nicht lesbar“, nichts geht kaputt); eine liegen gebliebene `<Name>.alt`, die sich nicht wegräumen lässt, verhindert jedes weitere Speichern an diesem Ort, ohne dass der Dialog sagt warum (eigener Fehlertext fehlt) |

### 4.6 Restrisiken nach der Großprüfung

| Nr. | Restrisiko |
|---|---|
| GP-M24 | Das MSI-Update unter Windows (Prüfsumme direkt vor `msiexec`, Datei bis zum Ende offen gehalten, Ergebnis beim nächsten Start; seit Runde 9 PowerShell und `msiexec` mit vollem Pfad) ist nur als Text des erzeugten Skripts getestet (`UpdateScriptTest`); ausgeführt wurde es auf Windows noch nicht. Code desselben Windows-Benutzers, der vor dem Start läuft, kann das Update weiter stören (Testplan Windows WG5) |
| - | Windows selbst (Clippy, Build und Tests für Windows) und die Emulator-Abläufe (Maestro) laufen nur in der CI, nicht im lokalen Precheck |
| GP-M19 | **Neu gebaut am 22.11.2026** (KONZEPT 6.5): App-PIN mit 8 bis 12 Ziffern, jeder Versuch verbraucht einen von zwei Einmal-Schlüsseln im Android-Keystore (`setMaxUsageCount(1)`), in deren Ergebnis die PIN eingeht; nach 2 falschen ist sie weg. Die Datei allein prüft keine PIN, auch nicht außerhalb des Geräts. **Restrisiko:** Auf Geräten ohne rollback-festen Keystore (die meisten ohne StrongBox) könnte jemand mit Root die Keystore-Datenbank zurückspielen und weiter raten, aber nur auf dem Gerät. **Richtiggestellt am 22.11.2026 (Runde 9, Krypto N1):** Hier stand „langsam (bei 8 Ziffern Jahre)“. Das ist zu günstig: Argon2id(PIN, Salt) lässt sich mit dem Salz aus der Datei für alle 10^8 PINs auf fremder Hardware vorrechnen (grob ein Tag auf einem großen Server), auf dem Gerät bleibt je Versuch nur ein HMAC im TEE und das Zurückspielen: **Tage bis wenige Wochen** auf Geräten ohne rollback-festen KeyMint. Ob `setMaxUsageCount(1)` für importierte Schlüssel in der Hardware oder nur von Android gezählt wird, kann die App nicht prüfen. Auf Geräten mit StrongBox erzwingt das die Hardware; das beruht auf der AOSP-Dokumentation und ist auf einem Gerät noch nicht nachgeprüft. **Richtiggestellt am 22.11.2026:** Hier stand für die App-PIN bis dahin (6 Ziffern, Zähler in den Einstellungen), man könne sie mit Root „auf dem Gerät“ durchprobieren. Es ging auch außerhalb: Der Keystore-Schlüssel gab den Schnellschlüssel ohne PIN frei, mit ihm und der Datei ließ sich die PIN auf fremder Hardware ohne Zähler durchprobieren |
| GP-M12, GP-M18 | Mit „Alles“ als Inhalt der Erinnerungen liegen Titel, Uhrzeit und Ort der Termine der nächsten 30 Tage mit einem Schlüssel ohne Bindung an ein entsperrtes Gerät auf dem Handy und gehen an Apps mit Zugriff auf Benachrichtigungen (unter Windows ins Info-Center); Standard ist „Mit Titel“ |
| GP-H11 | Ändern zwei Geräte unabhängig den Schutz, wird nicht zusammengeführt; der Dialog sagt, was verloren geht, und nennt den Weg über „Als Tresor weitergeben“ |
| GP-M44 | Der Android-Code (Schnell-Entsperren, Erinnerungen, USB-Key, sicheres Speichern) hat keine Unit-Tests; geprüft wird er im Emulator und von Hand |
| - | Die Environments `signing`, `release` und `website` und ihre Deployment-Branches (`signing` und `release`: `main` und `dev`; `website`: `main`) richtet der Inhaber in GitHub ein; bis die Secrets auf Repository-Ebene gelöscht sind, wirkt der Schutz aus 4.1 nicht (der Job `secrets-check` meldet es) |
| R9-Dateien-N4 | Ein präparierter fremder Tresor (öffnen oder übernehmen, mit dem Passwort, das der Absender mitgibt) darf 128 MiB Nutzdaten haben; unbekannte Schlüssel werden vollständig aufgebaut. Viele kleine CBOR-Werte vervielfachen den Speicher, die App kann beim Öffnen beendet werden (nicht nachgestellt, kein Datenverlust) |
| R9-Malware-N2 | Apps mit Zugriff auf Benachrichtigungen sehen „entsperrt“ und „gesperrt“, mit „Alles“ auch die Erinnerungen, und können laufende Aufträge abbrechen und sperren (nicht das Speichern, keine Sofortsperre) |
| R9-Malware-N4 | Windows: Schadsoftware desselben Benutzers (kein Administrator) kann die für den Benutzer installierte App verändern (`app\*.jar`, die mitgelieferte Laufzeit, `JAVA_TOOL_OPTIONS`) und liest dann beim nächsten Entsperren Passwort und Key-PIN mit. Eine Installation für alle Benutzer unter `Program Files` wäre eine neue Funktion (nach dem Funktionsstopp) |
| R9-Dieb-M1 | Portabler Stick: Wer ihn eine Weile hat, kann Programm oder Einstellungen austauschen (ohne Siegel); die App sagt es in „Über und Hilfe“ der portablen Version, die README ebenso |
| R9-Krypto-H1 (Rest) | Nach einem neuen Wiederherstellungscode tragen „Speichern unter“ auf einem Gerät mit altem Stand und eine alte Suite-Datei auf einem neuen Gerät den alten Code weiter (2.1) |
| R9-Lieferkette-M3, N1, N2 | Erstinstallation und portable Version lassen sich nur über `SHA256SUMS` im selben Release prüfen (der Fingerabdruck des Zertifikats ist nirgends veröffentlicht); Tags und ein Schein-Release können Versionsnummer und Update-Prüfung stören. Alles braucht Schreibrecht am Repository; „Immutable releases“ und Rulesets für Tags hat der Inhaber abgelehnt |
| R14-N2 | Ein Schnell-Entsperren genau während des Übernehmens einer Suite-Datei kann den Merker des übernommenen Zustands verlieren (kein Datenverlust, nur mit zwei gleichzeitigen Abläufen) |
| - | Die Leerlauf-Sperre unter Android prüft nur, solange die App im Vordergrund aktiv ist (ein Systemdialog darüber mit „Bei Laden aktiv lassen“ hält sie auf); Löschen in Schritten unter 10 Einträgen braucht kein Passwort |
| - | Festgehaltene Tests auf echten Geräten fehlen noch (Testplan, besonders E19e und die Abschnitte 40 bis 42; Testplan Windows, Abschnitte 14 und 15) |

## 5. Geprüft und in Ordnung

- **Dateiformat:** strenge Prüfung aller Längen, Versionen und Arten; Header als AAD (jede Änderung macht die Datei
  unbrauchbar); Argon2id-Grenzen gegen geschwächte Werte; neuere Dateien werden als „bitte aktualisieren“ erkannt.
  Formatversion 3: Schreibsiegel (HMAC-SHA256 mit einem Schlüssel aus Passwort-Schlüssel und `S`) über jede Datei;
  ein falsches Siegel heißt „verändert“, nie „falsches Passwort“. Ältere Formatversionen werden nicht mehr gelesen.
  Testvektoren für Version 3, unabhängig nachgerechnet in Python mit anderen Krypto-Bibliotheken.
- **Suite-Datei (`.ghlzip`):** nur genau die erwarteten Einträge in genau einer festen ZIP-Form (keine Ordner, keine
  doppelten oder zusätzlichen, unkomprimiert, ohne Data Descriptors und ZIP64), Größen und Zahl der Einträge beim
  Lesen gezählt; jede Datei in der versiegelten Kopfdatei mit Art, `container_id` und SHA-256, jede Inhaltsdatei im
  Inhaltsverzeichnis ihres Tresors mit Länge und SHA-256; alles gehört zu derselben Suite; geprüft wird alles, bevor
  etwas ersetzt wird; ein älterer Schutz öffnet nie. Gelesen und geschrieben als Datenstrom, beim Speichern
  zurückgelesen. Getestet mit fehlenden, vertauschten, alten, fremden und abgezweigten Teilen, falschem Siegel, Code
  ohne Passwort, ZIP-Tricks, Abbruch an jeder Stelle und einer Suite-Datei mit vollem GhulDocs.
  Ein GhulDocs-Paket (`.ghldocs`, dieselbe ZIP-Form, der Tresor als erster Eintrag) wird nie als Suite-Datei
  geöffnet und umgekehrt (der erste Eintrag entscheidet, nicht der Name). Ein hinzugefügtes Paket wird ein eigener
  GhulDocs-Tresor mit dem Schutz der Suite und neuen Schlüsseln; seine Inhaltsdateien werden vorher geprüft und in
  einen Zwischenordner gelegt, scheitert etwas, bleibt die Suite, wie sie war; dasselbe Paket ein zweites Mal wird
  abgelehnt. Die größte Suite-Datei (3,75 GiB) bleibt mit ihrem ZIP-Verzeichnis unter 4 GiB (ohne ZIP64).
- **GhulDocs:** jede Datei mit eigenem zufälligem Schlüssel, Abschnitte zu 4 MiB mit XChaCha20-Poly1305, an Kennung,
  Nummer und „letzter Abschnitt“ gebunden, die Länge verschlüsselt am Ende, Füllung muss null sein. Getestet:
  vertauschte, weggelassene, abgeschnittene, veränderte und fremde Abschnitte, falsche Füllung und Länge, eine
  Inhaltsdatei unter falschem Namen, kaputte Inhaltsverzeichnisse (Kreise, doppelte Kennungen, fehlende Felder):
  immer eine Fehlermeldung, nie falsche Daten. Grenzen (1 GB je Tresor, 3 GB und 20 000 Dokumente für alle GhulDocs-Tresore zusammen, Platz auf dem Datenträger) greifen,
  bevor etwas geschrieben wird; ein abgebrochenes Hinzufügen hinterlässt nichts. Testvektoren für die Inhaltsdateien
  (`docs/testvectors/docs-blob-v1.json`), unabhängig nachgerechnet in Python.
- **Scannen (Android):** Fotos, Seiten und PDF nur im Arbeitsspeicher, beim Speichern, Verlassen und Sperren
  überschrieben (am Desktop geprüft, auch dass nach dem Sperren nichts zurückbleibt); keine Kamera-App des Systems.
- **Kryptografie:** XChaCha20-Poly1305, HKDF-SHA256, Argon2id, X25519; alle Bausteine bestehen die Wycheproof-Fälle.
  Frische Nonces bei jedem Schreiben, Geheimnisse werden im Kern genullt, Vergleiche in konstanter Zeit.
- **FIDO2:** nur mit PIN-Prüfung, eigene RP-ID, die keine Webseite ansprechen kann; nur Slots mit dieser RP-ID werden
  angefragt. Neue Keys brauchen `credProtect` (Stufe 3) oder CTAP 2.1, damit die Key-PIN das `hmac-secret` schützt
  (seit 22.11.2026; vorhandene Keys entsperren weiter); unter Windows verlangt die App `credProtect` immer und nimmt
  nur USB oder NFC an.
- **Schnell-Entsperren:** Android-Keystore, Fingerabdruck „stark“, ungültig bei neuem Fingerabdruck; App-PIN mit
  zwei Einmal-Schlüsseln (2 Versuche, gesperrtes Gerät und Ablauf kosten keinen, leicht zu erratende PINs abgelehnt,
  Grenze gegen Root siehe GP-M19); an den Neustart gebunden (das setzt die App durch, nicht der Keystore); die Frist
  zählt auch nach der Zeit seit dem Start (eine zurückgestellte Uhr hilft nicht); Sofortsperre löscht es.
- **Dateien:** kein Klartext auf dem Gerät, in der Suite-Datei oder im verschlüsselten Export (gesucht auch als
  UTF-16, Hex, Base64), auch nicht in Datei- und ZIP-Namen. Gestohlene Dateien öffnen sich nur mit beiden Faktoren.
- **Netz:** nur die Update-Prüfung (`https` zu GitHub, Signatur und Prüfsumme, siehe 4.2) und die Leak-Prüfung
  (`https` zu `api.pwnedpasswords.com`, nur 5 Zeichen eines SHA-1-Werts); TLS mit webpki, `http://` nur für
  `localhost`; präparierte HTTP-Antworten gefuzzt (F07). GhulDrive, SFTP, Schlüsselwechsel am Server, Mail-Archiv,
  Freigaben, PHP-Modul und OpenPGP waren geprüft und sind **obsolet** (entfernt am 17.11.2026).
- **Android:** keine Backups, FileProvider und interne Dienste nicht exportiert, PendingIntents unveränderlich,
  Bildschirmschutz auch in der App-Vorschau, Passwortfelder ohne Vorschläge, Zwischenablage als sensibel markiert
  und beim Sperren geleert.
- **CI und Repository:** kein `pull_request_target`, Actions auf feste Versionen gepinnt, nur Leserechte, keine
  Secrets bei Pull Requests (sie laufen im Environment `ci` ohne Secrets und signieren mit einem Wegwerf-Schlüssel),
  Signatur- und Website-Secrets nur in Environments (Kontrolle: Job `secrets-check`), Prüfsummen für
  Gradle-Wrapper und Gradle-Verteilung, `cargo audit` und gitleaks; keine Zugangsdaten, Serveradressen oder echten
  Daten im Repository.
- **Website:** keine Cookies, nichts von fremden Servern, Content-Security-Policy; seit dem 17.11.2026 kein Skript
  mehr, das Daten annimmt (`report.php` entfernt).

## 6. Abhängigkeiten

`cargo audit` läuft bei jeder Änderung. Ausnahmen stehen mit Begründung in `core/.cargo/audit.toml`:

| Paket | Warum akzeptiert |
|---|---|
| `basic-toml` | nicht mehr gepflegt, keine bekannte Lücke; nur Build-Werkzeug (UniFFI) |

Mit GhulMail und den Servern sind rPGP (samt `rsa` 0.9 und `crypto-bigint` 0.5, RUSTSEC-2023-0071 und
RUSTSEC-2026-0387), `russh` und `pin-utils` aus dem Kern verschwunden; ihre Ausnahmen sind entfernt.

## 7. Hinweise für Nutzer

- **Key oder Notfallblatt verloren:** Key entfernen bzw. neues Notfallblatt erstellen, dann die Suite-Datei speichern
  (die App fordert dazu auf). Danach öffnet das Verlorene nichts mehr, was gespeichert wird.
- **Nach einer Schutzänderung alte Kopien löschen:** Eine Kopie der Suite-Datei, die schon anderswo liegt (Stick,
  Cloud, anderes Gerät), öffnet sich weiter mit dem alten Passwort **und** dem alten zweiten Faktor, mit dem Stand von
  damals. Sicher unbrauchbar wird sie, wenn du den zweiten Faktor tauschst: neuen Key oder neue Schlüsseldatei
  hinzufügen, den alten entfernen, die alte Schlüsseldatei vernichten bzw. den alten Key zurücksetzen.
- **Suite-Datei:** Die Arbeitskopie liegt im App-Speicher. Die Suite-Datei (`.ghlzip`) liegt, wo du sie speicherst;
  andere Apps und wer den Stick hat, können sie sehen, kopieren oder löschen, aber nicht öffnen. Bewahre eine Kopie an
  einem zweiten Ort auf. Mit vollen GhulDocs-Tresoren (3 GB) ist sie gut 3 GB groß, höchstens 3,75 GiB; beim
  Speichern ist kurzzeitig doppelter Platz nötig.
- **Dokumente exportieren oder mit einer App öffnen** heißt Klartext: Exporte liegen unverschlüsselt, wo du sie
  speicherst; die Kopie für „Mit App öffnen“ wird beim Sperren gelöscht, was die andere App damit macht, liegt bei
  ihr.
- **Windows portable:** nur auf eigenen Geräten entsperren; auf einem fremden PC kann ein Keylogger mitlesen. Einen
  Stick, der aus der Hand war, nicht mehr starten: die portable Version neu von der Release-Seite laden und die
  Prüfsumme prüfen (Programm und Einstellungen auf dem Stick haben kein Siegel).
- **Neuer Wiederherstellungscode:** danach auf jedem Gerät zuerst die neue Suite-Datei öffnen und alte Kopien
  löschen; ein Gerät mit altem Stand trüge sonst den alten Code in neue Dateien.
- **Schnell-Entsperren nach dem Update neu einrichten:** Fingerabdruck und App-PIN aus einer Version vor dem
  21.11.2026 öffnen nicht mehr (die Gültigkeit steht jetzt fest in der Schnell-Datei). Die App sagt das; unter
  Optionen → Entsperren richtest du sie mit Passwort und zweitem Faktor neu ein. Ebenso die **App-PIN** aus einer
  Version vor dem 22.11.2026 (sie ist jetzt 8 bis 12 Ziffern lang und hat 2 Versuche); der Fingerabdruck bleibt.
- **Android 12 oder neuer:** Seit dem 22.11.2026 braucht GhulSuite Android 12 (API 31). Wer die App auf Android 10
  oder 11 direkt installiert hat, bekommt keine Updates mehr: Der Updater lädt die neue Version, der System-Installer
  lehnt sie ab, die App sagt nur „Das Update konnte nicht installiert werden …“. Die installierte Version läuft
  weiter, ohne Sicherheits-Updates.
- GhulSuite ist eine **Entwicklungsversion**: die Suite-Datei regelmäßig speichern und eine Kopie aufbewahren.
