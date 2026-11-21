# GhulSuite: Sicherheitsbericht

**Stand:** 20.11.2026, Formatversion 3 mit Schreibsiegel, **eine reine Offline-App** (Umbau nach
[`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md) und [`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)): Die Suite wandert als eine
Datei (`.ghlzip`, bis 3,75 GiB), Dokumente liegen verschlüsselt in GhulDocs (seit 20.11.2026 bis zu 3 Tresore), es gibt keine Server und keine Konten.
Nach draußen gehen nur die Update-Prüfung und die Leak-Prüfung von GhulPass, beide abschaltbar. Befunde, die nur
entfernte Funktionen betrafen (Synchronisation, FTP/FTPS, Sicherung/Wiederherstellung, „Von meinem Server holen“,
Autofill, und seit dem Umbau zu GhulDocs: GhulMail, GhulDrive, Server und SFTP, PHP-Modul, Freigabelinks,
Schlüsselwechsel, Mail-Archiv, Hochladen von Fehlerberichten), bleiben in der Geschichte stehen und sind als
**obsolet** markiert.
**Art:** Internes Audit des eigenen Codes plus eine dauerhafte Testsuite mit Angriffen. Es gibt **keinen externen
Audit durch Dritte**: GhulSuite ist ein Soloprojekt. Wer sich mit Sicherheit auskennt, ist eingeladen mitzulesen,
anzugreifen und zu melden ([`SECURITY.md`](../SECURITY.md)).

Dieser Bericht fasst alle bisherigen Runden zusammen und sagt ehrlich, was offen ist. Die Einzelheiten jedes Fundes
(Test, der ihn zeigt, Commit) stehen im [Testprotokoll](TESTPROTOKOLL.md), wie getestet wird im
[Testkonzept](TESTKONZEPT.md), das Dateiformat in [`FORMAT.md`](FORMAT.md).

## 1. Die Runden

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

**In Zahlen (Stand vor den Umbauten):** 149 Funde aus den Testrunden (F01 bis F149), alle behoben bzw. entschieden
(F24 ist gewollt). Alle Punkte des Audits vom 13.11.2026 sind erledigt (B1 bis B18, L1, L3, L5, L7, L10, L13) oder
begründet entschieden (L4/B17). Damals 269 Testfunktionen im Kern und 64 Angriffe auf das PHP-Modul (**obsolet**, das
Modul ist entfernt). **Heute:** die Zahl der Testfunktionen steht im [Testprotokoll](TESTPROTOKOLL.md), Abschnitt 1;
über eine Million Einzelprüfungen je Lauf, Nachtlauf mit langem Fuzzing, Monkey im Emulator und Servern mit kaputtem
TLS gegen den TLS-Prüfer der beiden Verbindungen.

## 2. Wogegen GhulSuite schützt

| Angreifer | Schutz |
|---|---|
| **Dieb von Dateien** (Gerät, USB-Stick mit der Suite-Datei) | Ohne Master-Passwort **und** zweiten Faktor (Key oder Schlüsseldatei) wertlos. Argon2id mit mindestens 64 MiB. Von GhulDocs sieht er nur, wie viele Inhaltsdateien es gibt (genau: eine je Dokument, der Papierkorb eingeschlossen), wie groß sie ungefähr sind (aufgerundet auf 64 KiB) und auf dem Gerät an den Zeiten der Inhaltsdateien, wann die Dokumente hinzukamen (in der Suite-Datei stehen feste Zeiten); Namen, Ordner und Typen stehen nur im verschlüsselten Inhaltsverzeichnis |
| **Finder eines Keys oder des Notfallblatts** | Key entfernen bzw. neues Blatt erstellen und die Suite-Datei speichern: Danach öffnet es nichts mehr, was gespeichert wird, auch nicht zusammen mit einer alten Kopie der Suite-Datei (B1). Die alte Kopie selbst zeigt weiter den Stand von damals (2.1) |
| **Präparierte Dateien** (Suite-Datei, GhulDocs-Tresor, Importe) | Alles wird geprüft, bevor etwas ersetzt wird; Inhaltsdateien gegen ihre Prüfsumme und ihr Inhaltsverzeichnis, jeder Abschnitt gegen seine Nummer und die Kennung der Datei; alle Parser gefuzzt (auch der ZIP-Leser der Suite-Datei, Inhaltsdateien und Inhaltsverzeichnisse von GhulDocs: Ziele `parsers` und `docs_files` in `core/fuzz`, kurz in jedem Testlauf, lang im Nachtlauf); kein Absturz, keine Datei außerhalb des Ordners, keine Zip-Bombe |
| **Netzwerk** (WLAN, Provider) | Sieht nur, dass die App GitHub (Update-Prüfung) und, falls eingeschaltet, Pwned Passwords anfragt. TLS mit Zertifikatsprüfung (webpki); Updates nur mit gültiger Signatur; von Passwörtern gehen nur die ersten 5 Zeichen eines SHA-1-Werts hinaus |
| **Andere Apps auf dem Gerät** | Keine Backups, Bildschirmschutz; Dokumente bekommt eine andere App nur auf ausdrückliches „Mit App öffnen“ (eine befristete Kopie, gelöscht beim Sperren) oder „Exportieren“ (mit Rückfrage) |

Nicht im Bedrohungsmodell: ein bereits kompromittiertes, entsperrtes Gerät (Schadsoftware mit Root, fremde
Tastatur-App, Bedienungshilfen-Dienste). Dagegen schützt keine App (siehe 4.3).

Klartext entsteht nur beim Ansehen eines Dokuments und beim Scannen (beides im Arbeitsspeicher, beim Verlassen und
Sperren überschrieben) und beim Exportieren oder Öffnen mit einer anderen App (befristete Kopie).

### 2.1 Suite-Datei und alte Kopien

Die Suite reist als eine Datei (`.ghlzip`); aktuell ist der Stand, der zuletzt gespeichert wurde. Jede Datei trägt ein
Schreibsiegel, das nur mit dem Passwort entsteht; die Kopfdatei nennt Zähler, Stand-ID und Schutz-Epoche. Jede
Schutzänderung erneuert die inneren Schlüssel und erhöht die Schutz-Epoche ([`KONZEPT.md`](KONZEPT.md), 8.1 und 8.2).

| Wer | Kann | Kann nicht |
|---|---|---|
| Hat den Stick, sonst nichts | Datei kopieren, löschen, durch eine ältere ersetzen | lesen; etwas unterschieben (Siegel, Prüfsummen der Inhaltsdateien); eine ältere Datei unbemerkt öffnen lassen (Zähler, Rückfrage) |
| Hat Stick und einen alten Code | lesen, was im alten Stand galt | etwas unterschieben; den aktuellen Stand lesen |
| Hat eine alte Suite-Datei (vor einer Schutzänderung) und das alte Passwort **und** den alten zweiten Faktor | die alte Kopie mit einer frischen Installation öffnen, mit dem Stand und den Dokumenten von damals | sie auf einem Gerät öffnen lassen, das den neueren Stand kennt (Schutz-Epoche, auch nicht nach Rückfrage); lesen, was danach gespeichert wurde (neue innere Schlüssel, neue Dokumente mit neuen Schlüsseln) |
| Hat eine alte Suite-Datei, aber den alten zweiten Faktor nicht mehr (Key zurückgesetzt, Schlüsseldatei vernichtet) | nichts | sie öffnen, auch nicht mit dem alten Passwort |

**Grenze ohne Server** ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md), Abschnitt 2): Eine Kopie der Suite-Datei, die schon
irgendwo liegt (Stick, Cloud, anderes Gerät), lässt sich nicht nachträglich unbrauchbar machen. Die App sagt nach
jeder Schutzänderung deutlich: alte Kopien löschen. Sicher unbrauchbar werden sie, wenn der zweite Faktor getauscht
wird (neuen hinzufügen, alten entfernen, alte Schlüsseldatei vernichten bzw. alten Key zurücksetzen) **und** ein neuer
Wiederherstellungscode erstellt wird (der bisherige steckt auch in alten Kopien).

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
| **Windows** | Entschlüsselte Kopien werden überschrieben und gelöscht, auch wenn ein Programm sie erst später freigibt; portabel bleiben sie im Datenordner; Arbeitsverzeichnis nicht mehr im DLL-Suchpfad | B10, L13 |
| **Mail** (Runde 5; **obsolet**) | Herkunft von Kontaktschlüsseln gemerkt; Fingerabdruck vergleichen und bestätigen, Warnung bei Wechsel; Bilder nur von genau der geprüften Adresse (kein DNS-Rebinding) | B12, B16, L7 |
| **PHP-Modul und Freigaben** (Runde 5; **obsolet**) | Gültig signierte Anfragen werden nie gesperrt, Doppel-Endungen abgelehnt, Dateien 0600; Server ohne PHP-Ausführung wird erkannt und die Datei sofort gelöscht; Zurückziehen meldet Fehler ehrlich | B14, B15, L5 |
| **Hinweise** | Nach einem Verlust sagt die App, was zu wechseln ist (Mail- und Server-Passwörter; **obsolet** ohne Mail und Server, heute: alte Kopien löschen, zweiten Faktor tauschen) | L1 |
| **Abhängigkeiten** | `russh` (Speicher-Angriff durch bösartigen SSH-Server; **obsolet**, entfernt), `zip`, `crypto-bigint` über `p256` auf sichere Versionen | A1, A2, R1 |

## 4. Offen

Aus den Testrunden und Audits bis 16.11.2026 ist **nichts mehr offen**. Von den 146 offenen Befunden der letzten
Durchsichten entfallen mit Mail, Drive und Servern rund 70 (darunter H2, H5 und H7); die übrigen (vor allem Sperren und
Entsperren, Suite-Dateien, GhulCal mit H8, GhulPass-Import mit H4, Windows und Update) werden in Paket G9 von
[`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md) neu gesichtet, zusammen mit einer unabhängigen Durchsicht von GhulDocs und
dem Scannen. Was sonst bleibt, sind zwei bewusste Entscheidungen und die Grenzen, die keine App lösen kann (4.3).
Nichts davon gibt einem Angreifer ohne Passwort **und** zweiten Faktor Zugriff auf Tresorinhalte.

### 4.1 Bewusst entschieden

| Nr. | Entscheidung | Begründung |
|---|---|---|
| L4, B17 | **Ein Signaturschlüssel für alles:** Entwicklerversionen aller eigenen Branches und spätere Veröffentlichungen werden mit demselben Schlüssel signiert, ein getrennter Release-Schlüssel ist nicht vorgesehen | Entwicklerversionen sollen sich über die installierte App aktualisieren lassen; mit einem anderen Schlüssel müsste man sie erst deinstallieren und verlöre die Daten auf dem Gerät. Ein zweiter Schlüssel schützte vor allem gegen einen übernommenen GitHub-Zugang, und mit dem ließe sich auch `main` ändern. Schutz: Der Schlüssel steht nur im eigenen Repository bereit (bei Pushes und bei Pull Requests aus diesem Repository, seit 16.11.2026, damit ein Push mit offenem Pull Request nicht doppelt läuft), nie bei Pull Requests aus Forks; pushen und solche Pull Requests anlegen kann nur, wer Schreibrecht hat, also der Inhaber. Voraussetzungen: Zwei-Faktor-Anmeldung bei GitHub (Passkey oder Security Key), eine Offline-Sicherung der Keystore-Datei und ihres Passworts (ohne sie lässt sich keine installierte App mehr aktualisieren), Abhängigkeiten nur bewusst aktualisieren (Gradle-Prüfsummen, `Cargo.lock`) |
| F24 | Titel werden ohne Leerraum am Rand gespeichert | Absicht (Suche, Sortierung, Anzeige); alles Geheime bleibt Zeichen für Zeichen erhalten |

### 4.2 Nachprüfbarkeit des Builds

- **Abhängigkeiten der App (L3, erledigt):** `app/gradle/verification-metadata.xml` enthält die SHA-256-Prüfsumme
  jedes Gradle-Pakets (607 Pakete, auch die Windows-Varianten). Ein Paket, das unterwegs verändert wurde oder neu
  dazukommt, bricht den Build ab. Neue oder aktualisierte Abhängigkeiten: `./gradlew --write-verification-metadata
  sha256 help` und die Änderung der Datei im Pull Request prüfen. Für den Rust-Kern sichern `Cargo.lock` und
  `cargo deny` dasselbe.
- **Reproduzierbarkeit (L4, B17):** Der Nachtlauf baut den Kern zweimal in verschiedenen Ordnern und verlangt
  byte-gleiche Ergebnisse. Für die ganze APK (Kotlin, Ressourcen) ist das noch nicht nachgewiesen; das gehört zu
  festen Releases (Issue #6).

- **Updates (Runde 7):** Der Updater ist ein neuer Weg, Code auf das Gerät zu bringen. Geschützt durch: signierte
  Prüfsummen (RSA oder P-256, geprüft mit `ring`), Größe und SHA-256 der Datei vor der Übergabe an den Installer,
  nur `https` zu `api.github.com`, `github.com` und `*.githubusercontent.com` (auch Weiterleitungen), Download
  höchstens 600 MB in eine Datei statt in den Speicher, Entwürfe und auf dem stabilen Kanal Vorabversionen werden
  nie angeboten, Installation nur nach Zustimmung. Wer den Signaturschlüssel hat, kann Updates ausliefern; das war
  mit der APK-Signatur schon vorher so (AUDIT 4.1). Ein kompromittiertes GitHub-Konto allein reicht nicht: Ohne den
  Schlüssel passt die Signatur nicht. Rücksprung auf ältere Versionen bietet die App nie an (nur neuere).

### 4.3 Grenzen, die keine App lösen kann

| Nr. | Grenze |
|---|---|
| L2 | Kein unabhängiger Audit: Eigene Tests finden nur, woran man denkt |
| L6 | Der Server sah Anzahl, Größe und Änderungszeit der Dateien und die IP-Adresse (**obsolet**, keine Server mehr). Ähnlich sichtbar ist heute in der Suite-Datei und der Arbeitskopie: Anzahl und ungefähre Größe der Dokumente (aufgerundet auf 64 KiB), nicht ihre Namen |
| - | **Alte Kopien ohne Server** ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md), Abschnitt 2): Eine Kopie der Suite-Datei, die schon irgendwo liegt, lässt sich nachträglich nicht unbrauchbar machen. Wer das alte Passwort **und** den alten zweiten Faktor hat, öffnet sie mit einer frischen Installation (wie bei VeraCrypt), mit dem Stand und den Dokumenten von damals. Abhilfe: alte Kopien löschen (die App sagt das nach jeder Schutzänderung) und den zweiten Faktor tauschen |
| - | Wer den Stick hat, kann die Suite-Datei löschen oder durch eine ältere ersetzen. Die App erkennt das beim Öffnen, gegen den Verlust helfen nur Kopien an einem zweiten Ort |
| - | Das Schreibsiegel beweist, wer eine Datei geschrieben hat, nicht dass sie aktuell ist. Ein Gerät, das den neueren Stand nie gesehen hat, öffnet auch eine ältere Datei (ohne Rückfrage, es kennt nichts Neueres) |
| - | Exportierte Dokumente und die befristeten Kopien für „Mit App öffnen“ sind Klartext. Die App fragt vor dem Export und löscht die befristeten Kopien beim Sperren; was die andere App mit ihrer Kopie macht (eigene Zwischenspeicher, zuletzt geöffnete Dateien), liegt bei ihr |
| - | Ein Scan mit vielen Seiten braucht viel Arbeitsspeicher; die App warnt und nimmt keine weitere Seite, wenn er nicht reicht. Ob das System Teile des Arbeitsspeichers auslagert (Windows) bzw. ob eine Kamera-Bibliothek Zwischenpuffer anlegt, liegt außerhalb der App (siehe L8) |
| L8 | Geheimnisse im Arbeitsspeicher der Oberfläche (Kotlin-Zeichenketten lassen sich nicht sicher löschen), Auslagerungsdatei und Ruhezustand unter Windows. Nur der Rust-Kern löscht sicher; Geräteverschlüsselung nutzen |
| L9 | Schadsoftware mit Root, fremde Tastatur-Apps, Bedienungshilfen-Dienste |
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

Bewusst nicht automatisiert: eine Geräte-Matrix (Samsung, Pixel, Xiaomi; Android 10 bis 16). Dafür braucht es echte
Geräte oder einen bezahlten Gerätedienst; bis dahin deckt der Testplan die Hersteller-Eigenheiten von Hand ab.

### 4.4 Kleine Punkte aus den Prüfrunden vom 21.11.2026

Fünf Runden mit je zwei unabhängigen Prüfern über das ganze Projekt. Regel: „hoch“ und „mittel“ werden sofort
behoben (Testprotokoll PR-…), „klein“ (Randfall, Komfort, Werkzeug) kommt hierher und wird später erledigt.

| Nr. | Bereich | Punkt |
|---|---|---|
| K-1 | Windows | Der portable Modus hinterlässt Spuren auf dem PC: zuletzt benutzte Ordner und Dateinamen der Windows-Dateidialoge (Registry, ComDlg32), die von JNA entpackte DLL und `hsperfdata` in `%TEMP%`. Inhalte der Suite sind nicht darunter |
| K-2 | Release | Ab der 99. Vorabversion einer Version bekommen Builds denselben `versionCode` (Begrenzung auf 98); `release.yml` sollte dann abbrechen |
| K-3 | CI | Ein sofort abgebrochener Monkey-Lauf gilt als „ohne Absturz“; ein leeres Systemprotokoll im Emulator nur als Warnung |
| K-4 | Werkzeuge | `precheck.sh` räumt relativ zum Aufrufordner auf und meldet bei übersprungenen Prüfungen trotzdem „Alles grün“ |
| K-5 | Werkzeuge | `intent-attacks.sh` greift noch Aktionen von GhulMail und GhulDrive an, die es nicht mehr gibt; der deutsche Rundgang (`tour-de.yaml`) zeigt GhulDocs und zwei Optionsseiten nicht |
| K-6 | Update | Die signierte Prüfsummendatei nennt die Version nicht; ein altes, korrekt signiertes Paket ließe sich unter neuem Tag anbieten (Android lehnt kleinere Versionen ab, beim MSI schützt nur jpackage) |
| K-7 | Release | Jeder Push auf `dev` baut und signiert eine Vorabversion ohne vorgeschaltete Tests; der Schutz des Branches `dev` ist im Repository nicht nachprüfbar |
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
| K-19 | Kern | Argon2-Werte einer fremden Datei (bis 2 GiB Speicher) werden vor dem Ableiten nicht gegen das Gerät geprüft: eine präparierte Datei kann die App nach Eingabe des Passworts beenden (nichts wird ersetzt) |
| K-20 | Kern | Die RP-ID eines Key-Slots kommt ungeprüft aus der Datei (unter Windows zeigt der Systemdialog dann eine fremde Domain an; nichts verlässt das Gerät) |
| K-21 | Windows | Die PDF-Ansicht begrenzt die Seitenhöhe nicht (ein präpariertes PDF kann den Speicher ausschöpfen) |
| K-22 | Android | Der Absturzbericht erscheint vor dem Bildschirmschutz und wird ohne „sensibel“-Markierung kopiert (er enthält nur Stacktraces) |
| K-23 | Website | Der Hinweis „Keine Cookies“ bleibt unsichtbar in der Tab-Reihenfolge; Kontrast der blassen Schrift knapp unter WCAG AA; doppelt vorgelesene Laufband-Liste und Bildtexte |
| K-24 | Texte | „Alles wird nur auf diesem Gerät geprüft“ auch bei eingeschalteter Leak-Prüfung; Hinweis der Sperr-Benachrichtigung „Schütteln oder ‚Sperren‘ sperrt sofort“; Reste von GhulMail/GhulDrive im Testplan; Hinweis zu Testschlüsseln auf der Download-Seite des Testservers |
| K-25 | Website | Ein früher hochgeladenes `report.php` könnte auf dem Webspace noch liegen (der Upload löscht nichts): von Hand prüfen und löschen |

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
- **FIDO2:** nur mit PIN-Prüfung, eigene RP-ID, die keine Webseite ansprechen kann.
- **Schnell-Entsperren:** Android-Keystore, Fingerabdruck „stark“, ungültig bei neuem Fingerabdruck, Fehlversuche
  begrenzt, an den Neustart gebunden; Sofortsperre löscht es.
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
  Secrets bei Pull Requests, `cargo audit`, `cargo deny` und gitleaks; keine Zugangsdaten, Serveradressen oder
  echten Daten im Repository.
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
- **Windows portable:** nur auf eigenen Geräten entsperren; auf einem fremden PC kann ein Keylogger mitlesen.
- GhulSuite ist eine **Entwicklungsversion**: die Suite-Datei regelmäßig speichern und eine Kopie aufbewahren.
