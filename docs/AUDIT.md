# GhulSuite: Sicherheitsbericht

**Stand:** 17.11.2026, Formatversion 3 mit Schreibsiegel, **ohne Synchronisation** (Umbau nach
[`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)): Die Suite wandert als eine Datei (`.ghlzip`), Server nur per SFTP mit
SSH-Schlüssel, GhulDrive mit Sperre, Revision und Schlüsselwechsel. Befunde, die nur entfernte Funktionen betrafen
(Synchronisation, FTP/FTPS, Sicherung/Wiederherstellung, „Von meinem Server holen“, Autofill), bleiben in der
Geschichte stehen und sind als **obsolet** markiert.
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

**In Zahlen (Stand vor dem Umbau):** 149 Funde aus den Testrunden (F01 bis F149), alle behoben bzw. entschieden (F24 ist gewollt). Alle Punkte des Audits
vom 13.11.2026 sind erledigt (B1 bis B18, L1, L3, L5, L7, L10, L13) oder begründet entschieden (L4/B17). 269 Testfunktionen im Kern mit über einer Million Einzelprüfungen je Lauf, 64 Angriffe auf das
PHP-Modul, Nachtlauf mit langem Fuzzing, Monkey im Emulator und Server mit kaputtem TLS.

## 2. Wogegen GhulSuite schützt

| Angreifer | Schutz |
|---|---|
| **Server** (Hoster, Einbruch beim Hoster) | Sieht nur Verschlüsseltes (GhulDrive-Blöcke, Index, Freigaben): keine Inhalte, keine Datei- oder Ordnernamen; die Tresore liegen gar nicht auf dem Server. Gefälschte, vertauschte oder zurückgespielte Dateien werden erkannt, nie als richtig angezeigt |
| **Dieb von Dateien** (Gerät, USB-Stick mit der Suite-Datei) | Ohne Master-Passwort **und** zweiten Faktor (Key oder Schlüsseldatei) wertlos. Argon2id mit mindestens 64 MiB |
| **Finder eines Keys oder des Notfallblatts** | Key entfernen bzw. neues Blatt erstellen: Danach öffnet es nichts mehr, was gespeichert wird, auch nicht zusammen mit einer alten Kopie der Suite-Datei (B1); der Schlüsselwechsel sperrt alte Kopien von GhulDrive und den Servern aus |
| **Netzwerk** (WLAN, Provider) | TLS mit Zertifikatsprüfung oder bestätigtem Fingerabdruck, SFTP mit festem Host-Schlüssel, IMAP/SMTP nie unverschlüsselt |
| **Absender böswilliger Mails** | HTML ohne Skripte und ohne Zugriff aufs eigene Netz, echte Link-Ziele sichtbar, keine eingeschleusten Kopfzeilen, Autocrypt-Regeln gegen untergeschobene Schlüssel |
| **Andere Apps auf dem Gerät** | Keine privaten Dateien in fremde Mail-Entwürfe, keine Backups, Bildschirmschutz |
| **Präparierte Dateien und Antworten** | Alle Parser gefuzzt; kein Absturz, keine Datei außerhalb des Ordners, keine Zip-Bombe |

Nicht im Bedrohungsmodell: ein bereits kompromittiertes, entsperrtes Gerät (Schadsoftware mit Root, fremde
Tastatur-App, Bedienungshilfen-Dienste). Dagegen schützt keine App (siehe 4.3).

### 2.1 Suite-Datei, Server und alte Kopien (seit dem Umbau)

Die Suite reist als eine Datei (`.ghlzip`); aktuell ist der Stand, der zuletzt gespeichert wurde. Jede Datei trägt ein
Schreibsiegel, das nur mit dem Passwort entsteht; die Kopfdatei nennt Zähler, Stand-ID und Schutz-Epoche. Nach jeder
Schutzänderung bekommen GhulDrive und der Server-Zugang neue Schlüssel ([`KONZEPT.md`](KONZEPT.md), 8.1 und 8.4).

| Wer | Kann | Kann nicht |
|---|---|---|
| Hat den Stick, sonst nichts | Datei kopieren, löschen, durch eine ältere ersetzen | lesen; etwas unterschieben (Siegel); eine ältere Datei unbemerkt öffnen lassen (Zähler, Rückfrage) |
| Hat Stick und einen alten Code | lesen, was im alten Stand galt | etwas unterschieben; den aktuellen Stand lesen; nach dem Schlüsselwechsel an den Server |
| Hat Stick, alten Code und altes Passwort | einen alten Stand mit altem Schutz anbieten | ihn auf einem Gerät mit neuerem Stand öffnen lassen (Schutz-Epoche, Sperre); an den Server |
| Hat eine alte Suite-Datei (vor einer Schutzänderung) | die damaligen Inhalte lesen | sich am Server anmelden; den aktuellen Drive-Index öffnen; neue Drive-Dateien lesen |
| Betreibt den Server | Blöcke löschen, einen alten Index zurückspielen, Freigaben stören | Dateien lesen; einen Index fälschen (authentisiert mit dem Drive-Schlüssel); einen alten Index unbemerkt unterschieben (Revision) |
| Betreibt den Server und hat eine alte Kopie der Suite-Datei | einen Index unter dem alten Drive-Schlüssel ablegen | ihn unterschieben (nach dem Schlüsselwechsel kennt die Suite den alten Schlüssel nicht mehr; während des Wechsels nur nach Bestätigung einer eigenen Warnung) |

Bewusste Ausnahme: Wer nur den Wiederherstellungscode hat (Passwort vergessen), kann eine Suite-Datei öffnen, ohne
dass sich die Siegel prüfen lassen. Die App sagt das, prüft Stand und Schutz-Epoche trotzdem und versiegelt danach
alles mit dem neuen Passwort neu.

## 3. Behoben

Nach Bereich zusammengefasst; die Nummern führen ins [Testprotokoll](TESTPROTOKOLL.md) bzw. zur Tabelle 6.

| Bereich | Was behoben wurde | Nummern |
|---|---|---|
| **Synchronisation** (**obsolet**, Synchronisation am 17.11.2026 entfernt) | Eine kaputte, fehlende oder neuere Datei (auf dem Server oder lokal) löscht nie mehr einen Tresor; Löschungen nur über ausdrückliche Löschmarken. Abgerissene Uploads blockieren nichts. Geräte, die lange offline waren, verlieren nichts und überschreiben nichts, was sie nicht lesen können | F01 bis F06, F40, F41 |
| **Schlüssel und Format** | Neues Notfallblatt, neues Passwort, entfernter Key: neue innere Schlüssel in jeder Datei; ein entfernter Key erneuert das Faktor-Geheimnis für die übrigen Keys, ohne sie zu berühren (öffentlicher Schlüssel je Key, Formatversion 2). Dateinamen verraten keine Tresornamen | B1, F23 |
| **Netzwerk-Parser** | Abstürze durch präparierte HTTP-, SMTP- und FTP-Antworten (teils vor STARTTLS von jedem im WLAN auslösbar); der FTP-Teil ist **obsolet** (FTP/FTPS entfernt) | F07 bis F10 |
| **Mail** | Kopfzeilen-Einschleusung, versteckte Link-Ziele, Autocrypt-Schlüsseltausch durch gefälschten Absender, Bilder an `localhost` und das Heimnetz | F13, F16, F17, F28 |
| **Autofill** (**obsolet**, Autofill am 16.11.2026 entfernt) | Basis-Domain zu grob (`evil.github.io`), jede App konnte eine Webadresse behaupten, Adressen mit Backslash und IPv4-Schreibweisen anders gelesen als im Browser | F18, F19, F25, F26 |
| **Andere Apps** | Private Dateien über „Per E-Mail senden“ in Entwürfe geschmuggelt; Dev-APK debuggbar | F20, F21 |
| **Dateien von außen** | Sicherungen mit Pfad-Tricks und Zip-Bomben (die Sicherung ist **obsolet**; dieselben und strengere Regeln gelten für die `.ghlzip`: nur genau die erwarteten Einträge, Größen beim Lesen gezählt); Dateinamen vom Server mit Gerätenamen, Doppelpunkten, Steuer- und Richtungszeichen; Drive-Namen, die den Dateityp tarnen | F11, F14, F27 |
| **Daten, die das Gerät verlassen** | CSV-Export als Formel ausführbar; Geheimnisse in Debug-Ausgaben; Passwort blieb nach dem Sperren in der Zwischenablage | F15, F22, F29 |
| **Entsperren** | App-PIN-Zähler zählte erst nach dem Versuch (Freiversuch durch Abschießen der App) | F31 |
| **CI** | Tests gegen echte Server liefen nach einem Runner-Wechsel ins Leere und wurden grün gemeldet | F39 |
| **Freigaben und PHP-Modul** (Audit 0.5.0) | Abgelaufene Pakete abrufbar, beliebige PBKDF2-Runden aus dem Paket, ausführbare Dateien im Datenordner, Namen mit Punkt am Ende, fehlende Kopfzeilen | A3 bis A5, F30 |
| **Freigaben ohne Kopie** (Runde 9) | Einmal-Links durch Link-Scanner verbraucht, Blockliste des Servers ungeprüft, beschädigter Zähler als „nie geöffnet“ gelesen, Ticket-Flut verdrängte laufende Downloads, fehlende Prüfung von Schreibrecht und Web-Erreichbarkeit des GhulSuite-Ordners, verwaiste oder gelöschte Blöcke bei unlesbarer Freigabenliste | F110 bis F113, F115 |
| **Mail-Archiv** (Runde 9) | Gefälschte Message-ID verhinderte das Archivieren der echten Mail, Richtungsmarken im Dateinamen | F114 |
| **Android-Teilen und Anhänge** (Audit 0.5.0) | Dateinamen wie `..`, Anhänge beliebiger Größe | A6, A7 |
| **Sync, Runde 5** (**obsolet**, Synchronisation und „Passwort anderswo geändert“ entfernt; die Grenzen für Argon2-Werte aus Dateien gelten weiter, siehe Abschnitt 5) | Ein entfernter Key kommt nie per Sync zurück (Löschmarken je Key). Geänderte Server- und Kontodaten erreichen andere Geräte. Gelöscht auf A, geändert auf B: feste Regel, eine nie gesehene Änderung bleibt. Konfliktkopien fremder Sync-Programme werden zusammengeführt. Ein Server kann keine übermäßig teuren Argon2-Werte erzwingen | B2, B11, B13, B18, F42 |
| **Drive** | Ein zurückgesetzter Index auf dem Server wird abgelehnt (nichts verschwindet); zwei Geräte, die gleichzeitig ändern, verlieren nichts. Seit dem Umbau unter `index.lock` (exklusiv angelegt, verwaist nach fünf Minuten nach eigener Zeit und Änderungszeit des Servers), mit `index.bak` bei beschädigtem Index und automatischem Aufräumen | B3 |
| **Umbau ohne Synchronisation** (17.11.2026) | Ungeprüfte Ladewege (Sicherung, „Von meinem Server holen“, Übernahme fremder Dateien aus dem Ordner) ersetzt durch die geprüfte `.ghlzip`; Schreibsiegel gegen untergeschobene Dateien mit altem Code; ältere, abgezweigte und fremde Suite-Dateien nur nach Rückfrage, ein älterer Schutz gar nicht; Gerätedaten des alten Stands (Schnell-Entsperren, Mail-Zwischenspeicher, Drive-Offline-Kopien, unfertige Uploads) werden beim Öffnen gelöscht; alte Suite-Dateien kommen nach dem Schlüsselwechsel nicht mehr an GhulDrive und die Server; kein Server-Passwort mehr in der Suite | Umbau, Pakete 2 bis 7 |
| **Android** | Benachrichtigungen, die bei gesperrtem Gerät eintreffen, enthalten nur ihre Sperrbildschirm-Fassung (Android lässt Apps die Kanal-Einstellung nicht vorgeben, daher entscheidet die App beim Eintreffen); Mail-Zugang für Benachrichtigungen bevorzugt in StrongBox, wahlweise löscht ihn die Sofortsperre; `.ics` von fremden Apps nur nach Vorschau; interne Aktionen nur aus eigenen Benachrichtigungen; Auto-Sperre zählt die echte Zeit; Autofill fragt vor dem Verknüpfen; kein Tippen durch darübergelegte Fenster | B4 bis B9, L10, F43 |
| **Windows** | Entschlüsselte Kopien werden überschrieben und gelöscht, auch wenn ein Programm sie erst später freigibt; portabel bleiben sie im Datenordner; Arbeitsverzeichnis nicht mehr im DLL-Suchpfad | B10, L13 |
| **Mail** (Runde 5) | Herkunft von Kontaktschlüsseln gemerkt; Fingerabdruck vergleichen und bestätigen, Warnung bei Wechsel; Bilder nur von genau der geprüften Adresse (kein DNS-Rebinding) | B12, B16, L7 |
| **PHP-Modul und Freigaben** (Runde 5) | Gültig signierte Anfragen werden nie gesperrt, Doppel-Endungen abgelehnt, Dateien 0600; Server ohne PHP-Ausführung wird erkannt und die Datei sofort gelöscht; Zurückziehen meldet Fehler ehrlich | B14, B15, L5 |
| **Hinweise** | Nach einem Verlust sagt die App, was zu wechseln ist (Mail- und Server-Passwörter) | L1 |
| **Abhängigkeiten** | `russh` (Speicher-Angriff durch bösartigen SSH-Server), `zip`, `crypto-bigint` über `p256` auf sichere Versionen | A1, A2, R1 |

## 4. Offen

Aus den Testrunden und Audits bis 16.11.2026 ist **nichts mehr offen**. Die Punkte der letzten Durchsichten, die
nicht mit der Synchronisation entfallen sind, werden nach dem Umbau neu gesichtet (Paket 9 in
[`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)). Was sonst bleibt, sind zwei bewusste Entscheidungen und die Grenzen,
die keine App lösen kann (4.3). Nichts davon gibt einem Angreifer ohne Passwort **und** zweiten Faktor
Zugriff auf Tresorinhalte.

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
| L6 | Der Server sieht Anzahl, Größe und Änderungszeit der Dateien und die IP-Adresse |
| - | Verliert der Server Daten (Hoster setzt zurück, Datenträger kaputt), merkt die App es (Revision, fehlende Blöcke), retten kann sie es nicht. Dagegen hilft nur eine Sicherung beim Hoster von **nach** der letzten Schutzänderung; ältere sind mit einem Drive-Schlüssel verschlüsselt, den die Suite nicht mehr hat |
| - | Wer den Stick hat, kann die Suite-Datei löschen oder durch eine ältere ersetzen. Die App erkennt das beim Öffnen, gegen den Verlust helfen nur Kopien an einem zweiten Ort |
| - | Das Schreibsiegel beweist, wer eine Datei geschrieben hat, nicht dass sie aktuell ist. Ein Gerät, das den neueren Stand nie gesehen hat, öffnet auch eine ältere Datei (ohne Rückfrage, es kennt nichts Neueres); nach dem Schlüsselwechsel kommt sie aber nicht mehr an den Server |
| - | Ein Hoster, der SSH-Schlüssel nur im Webpanel verwaltet: Den Tausch nach einer Schutzänderung macht der Nutzer dort selbst, die App prüft ihn. Bis dahin funktioniert auf diesem Server auch der alte Schlüssel |
| L8 | Geheimnisse im Arbeitsspeicher der Oberfläche (Kotlin-Zeichenketten lassen sich nicht sicher löschen), Auslagerungsdatei und Ruhezustand unter Windows. Nur der Rust-Kern löscht sicher; Geräteverschlüsselung nutzen |
| L9 | Schadsoftware mit Root, fremde Tastatur-Apps, Bedienungshilfen-Dienste |
| L11 | Zertifikats-Widerruf wird nicht geprüft (wie bei den meisten Apps; mit festem Fingerabdruck ohne Belang) |
| L12 | Zeit-Seitenkanäle sind nicht gemessen; Vergleiche von Geheimnissen laufen in konstanter Zeit |
| - | Passwort vergessen heißt Daten weg, wenn auch das Notfallblatt fehlt. Es gibt bewusst keine Hintertür |

### 4.4 Prüfwerkzeuge im Nachtlauf

Alle früher als „noch nicht eingebaut“ geführten Werkzeuge laufen jetzt jede Nacht (`.github/workflows/nightly.yml`):

| Werkzeug | Was es prüft | Ergebnis beim Einbau |
|---|---|---|
| **x509-limbo** (C2SP) | 9.807 Zertifikatsketten gegen den TLS-Prüfer der App | 9.707 wie erwartet; 54 Fälle, in denen die Bibliothek bewusst großzügiger ist (z. B. Felder des Wurzelzertifikats, Namen nur aus dem SAN, schwache Schlüssel, die der Handshake danach ablehnt), stehen begründet in `core/ghul-core/tests/limbo-known.txt`. Keiner lässt eine falsche Signatur, einen falschen Namen, eine kaputte Kette oder ein abgelaufenes Zertifikat durch. Jeder neue Fall bricht den Lauf ab |
| **OpenPGP mit GnuPG** | Verschlüsseln, Signieren und Entschlüsseln in beide Richtungen, auch abgetrennte Signaturen; gefälschte Signaturen werden auf beiden Seiten abgelehnt | grün mit Ed25519/X25519, RSA-3072 und NIST P-256 |
| **OWASP ZAP** (Baseline) | PHP-Modul und Empfängerseite, nur auf 127.0.0.1 | bricht bei fehlenden Schutz-Kopfzeilen, CORS- oder Cookie-Fehlern ab |
| **MobSF** (statisch, MASVS) | die fertige APK | Bericht als Artefakt, schwere Befunde als Warnung |
| **Intent-Angriffe** (statt drozer) | jede von außen erreichbare Komponente im Emulator: falsche und riesige Extras, interne Aktionen ohne Kennung, fremde Datei- und Inhalts-Adressen | bricht bei Absturz, ANR oder wenn eine geschützte Komponente einen fremden Aufrufer annimmt ab |
| **Reproduzierbarer Kern** | zwei Builds in verschiedenen Ordnern | müssen byte-gleich sein |

Bewusst nicht automatisiert: eine Geräte-Matrix (Samsung, Pixel, Xiaomi; Android 10 bis 16). Dafür braucht es echte
Geräte oder einen bezahlten Gerätedienst; bis dahin deckt der Testplan die Hersteller-Eigenheiten von Hand ab.

## 5. Geprüft und in Ordnung

- **Dateiformat:** strenge Prüfung aller Längen, Versionen und Arten; Header als AAD (jede Änderung macht die Datei
  unbrauchbar); Argon2id-Grenzen gegen geschwächte Werte; neuere Dateien werden als „bitte aktualisieren“ erkannt.
  Formatversion 3: Schreibsiegel (HMAC-SHA256 mit einem Schlüssel aus Passwort-Schlüssel und `S`) über jede Datei;
  ein falsches Siegel heißt „verändert“, nie „falsches Passwort“. Ältere Formatversionen werden nicht mehr gelesen.
  Testvektoren für Version 3, unabhängig nachgerechnet in Python mit anderen Krypto-Bibliotheken.
- **Suite-Datei (`.ghlzip`):** nur genau die erwarteten Einträge (keine Ordner, keine doppelten oder zusätzlichen),
  Größen beim Lesen gezählt; jede Datei in der versiegelten Kopfdatei mit Art, `container_id` und SHA-256; alles
  gehört zu derselben Suite; geprüft wird alles, bevor etwas ersetzt wird. Getestet mit fehlenden, vertauschten,
  alten, fremden und abgezweigten Teilen, falschem Siegel, Code ohne Passwort und ZIP-Tricks.
- **Kryptografie:** XChaCha20-Poly1305, HKDF-SHA256, Argon2id, X25519; alle Bausteine bestehen die Wycheproof-Fälle.
  Frische Nonces bei jedem Schreiben, Geheimnisse werden im Kern genullt, Vergleiche in konstanter Zeit.
- **FIDO2:** nur mit PIN-Prüfung, eigene RP-ID, die keine Webseite ansprechen kann.
- **Schnell-Entsperren:** Android-Keystore, Fingerabdruck „stark“, ungültig bei neuem Fingerabdruck, Fehlversuche
  begrenzt, an den Neustart gebunden; Sofortsperre löscht es.
- **Dateien:** kein Klartext auf Gerät, Server, in der Suite-Datei oder im Export (gesucht auch als UTF-16, Hex,
  Base64), auch nicht in Datei- und ZIP-Namen. Gestohlene Dateien öffnen sich nur mit beiden Faktoren.
- **Drive:** gekippte Bits, vertauschte oder gelöschte Blöcke, gefälschter Index: immer eine Fehlermeldung, nie
  falsche Daten. Index an die Suite gebunden; älterer, beschädigter oder fehlender Index wird nicht still übernommen;
  Änderungen nur unter `index.lock`, auch mit zwei Geräten gleichzeitig ohne Verlust. Schlüsselwechsel fortsetzbar
  nach Abbruch an jeder Stelle; danach öffnet ein Index unter dem alten Schlüssel nicht mehr. Unfertige Uploads auf
  dem Gerät mit eigenem Geräteschlüssel versiegelt.
- **Schlüsselwechsel am Server:** Der neue SSH-Schlüssel wird nur übernommen, wenn er sich anmeldet; aus
  `~/.ssh/authorized_keys` werden nur die Zeilen der früheren Schlüssel entfernt (alle anderen bleiben Byte für
  Byte); danach wird geprüft, dass der neue weiter funktioniert und der alte abgewiesen wird. Nur eine abgewiesene
  Anmeldung zählt, ein Verbindungsabbruch heißt „später wieder versuchen“.
- **Mail-Archiv (seit 0.7.0):** Kopien als `.eml` in GhulDrive, also mit dessen Schutz (Dateischlüssel, verschlüsselte
  Namen); beim Anbieter ändert das Archivieren nichts und löscht nichts. Doppelte werden am Code der Message-ID
  erkannt; Dateinamen ohne Pfad-, Steuer- und Richtungszeichen.
- **Freigaben:** zufälliger 256-Bit-Schlüssel je Freigabe, nur im Teil nach `#`; das Paket enthält nur die
  Schlüssel der geteilten Dateien (jede Datei hat einen eigenen), nie den Drive-Schlüssel; AES-256-GCM mit eindeutigen
  Nonces, Abschneiden und Vertauschen werden erkannt; optional Passwort (PBKDF2, 600 000 Runden). Empfängerseite mit
  strenger CSP, ohne fremde Inhalte, ohne Referrer.
- **PHP-Modul (seit 0.7.0 nur für Freigabelinks):** kein Geheimnis, kein Schreibzugriff übers Web, nur `GET`. Es
  liest die Beschreibungen, die die App über SFTP ablegt, und gibt nur deren Blöcke heraus, nur mit einem
  Ticket (24 Stunden, gehasht gespeichert); Kennungen und Nummern nur in genau ihrer Form, keine Pfade aus Eingaben;
  Ablauf und Einmal-Links unter Dateisperre; keine Fehlerdetails. 64 gezielte Angriffe, PHPStan im Nachtlauf,
  PHP-8.1-Syntax in der CI.
- **Netz:** SFTP mit festem Host-Schlüssel (Host-Zertifikate abgelehnt), Anmeldung nur mit dem SSH-Schlüssel der
  Suite (ein Passwort nur einmal zum Eintragen des Schlüssels, nie gespeichert), TLS mit webpki oder bestätigtem
  Fingerabdruck, `http://` nur für `localhost`. (FTPS mit `PROT P` und Schutz gegen „FTP bounce“ war geprüft und ist
  **obsolet**, FTP/FTPS ist entfernt.)
- **OpenPGP (rPGP 0.21):** eigene Schlüssel Ed25519/X25519 mit SHA-512/256 und AES-256; gegen GnuPG 2.4 in beide
  Richtungen getestet; PGP/MIME byte-genau; Betreff geschützt; Anhänge nur innerhalb der Verschlüsselung.
- **Android:** keine Backups, FileProvider und interne Dienste nicht exportiert, PendingIntents unveränderlich,
  Bildschirmschutz auch in der App-Vorschau, Passwortfelder ohne Vorschläge, Zwischenablage als sensibel markiert
  und beim Sperren geleert.
- **CI und Repository:** kein `pull_request_target`, Actions auf feste Versionen gepinnt, nur Leserechte, keine
  Secrets bei Pull Requests, `cargo audit`, `cargo deny` und gitleaks; keine Zugangsdaten, Serveradressen oder
  echten Daten im Repository.

## 6. Abhängigkeiten

`cargo audit` läuft bei jeder Änderung. Ausnahmen stehen mit Begründung in `core/.cargo/audit.toml`:

| Paket | Warum akzeptiert |
|---|---|
| `basic-toml`, `pin-utils` | nicht mehr gepflegt, keine bekannte Lücke; nur Build-Werkzeug bzw. intern |
| `rsa` 0.9 (RUSTSEC-2023-0071), `crypto-bigint` 0.5 (RUSTSEC-2026-0387), beide über rPGP | betreffen Operationen mit *Geheimschlüsseln* dieser Verfahren. GhulMail erzeugt und importiert nie RSA- oder NIST-Geheimschlüssel; mit Schlüsseln von Kontakten laufen nur öffentliche Operationen. Entfällt, sobald rPGP neuere Versionen nutzt |

## 7. Hinweise für Nutzer

- **Key oder Notfallblatt verloren:** Key entfernen bzw. neues Notfallblatt erstellen, dann die Suite-Datei speichern
  (die App fordert dazu auf) und ältere Kopien ersetzen. Danach öffnet das Verlorene nichts mehr, was gespeichert wird,
  und der Schlüsselwechsel sperrt alte Kopien von GhulDrive und den Servern aus. Alte Kopien zeigen weiter den Stand
  von damals, auch die Mail-Zugänge von damals: Mail-Passwörter beim Anbieter wechseln (die App erinnert unter
  Optionen → Sicherheit daran).
- **Nach einer Schutzänderung:** Bis der Schlüsselwechsel auf allen Servern durch ist, zeigt die App, welche Server
  noch warten. Wo der Hoster SSH-Schlüssel im Webpanel verwaltet, den neuen dort eintragen und den alten entfernen.
  Andere Geräte öffnen zuerst die neue Suite-Datei.
- **Freigabelinks:** Wer den Link hat, kann die Datei öffnen. Für Sensibles ein Passwort setzen und getrennt
  mitteilen. Ein Einmal-Link gilt als verbraucht, sobald der Abruf beginnt.
- **Webserver ohne `.htaccess`** (z. B. nginx): Dort schützt nur der zufällige Name den Datenordner bzw. die
  zufällige ID das Paket; Inhalte bleiben verschlüsselt.
- **Suite-Datei:** Die Arbeitskopie liegt im App-Speicher. Die Suite-Datei (`.ghlzip`) liegt, wo du sie speicherst;
  andere Apps und wer den Stick hat, können sie sehen, kopieren oder löschen, aber nicht öffnen. Bewahre eine Kopie an
  einem zweiten Ort auf. **GhulDrive und Freigabelinks** brauchen einen Server mit SSH-Zugang (SFTP); reiner
  FTP-Webspace reicht nicht.
- **Windows portable:** nur auf eigenen Geräten entsperren; auf einem fremden PC kann ein Keylogger mitlesen.
- GhulSuite ist eine **Entwicklungsversion**: die Suite-Datei regelmäßig speichern und eine Kopie aufbewahren.
