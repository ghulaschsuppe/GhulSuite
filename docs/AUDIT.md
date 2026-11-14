# GhulSuite: Sicherheitsbericht

**Stand:** 14.11.2026, Entwicklungsversion 0.5.0-dev mit Formatversion 2 (B1, F23).
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
| 14.11.2026 | Runde 6: Bedienung (U4 bis U15), Mail-Ablauf mit echten Servern im Emulator (B21), neues verschlüsseltes Mail-Archiv mit Prüfung jeder Mail vor dem Löschen beim Anbieter | 2 weitere Funde (F44, F45), behoben; nichts offen |

**In Zahlen:** 45 Funde aus den Testrunden, alle behoben bzw. entschieden (F24 ist gewollt). Alle Punkte des Audits
vom 13.11.2026 sind erledigt (B1 bis B18, L1, L3, L5, L7, L10, L13) oder begründet entschieden (L4/B17). 263 Testfunktionen im Kern mit über einer Million Einzelprüfungen je Lauf, 347 Angriffe auf das
PHP-Modul, Nachtlauf mit langem Fuzzing, Monkey im Emulator und Server mit kaputtem TLS.

## 2. Wogegen GhulSuite schützt

| Angreifer | Schutz |
|---|---|
| **Server** (Hoster, Einbruch beim Hoster) | Sieht nur Verschlüsseltes: keine Inhalte, keine Datei-, Ordner- oder Tresornamen. Gefälschte, vertauschte oder zurückgespielte Dateien werden erkannt, nie als richtig angezeigt |
| **Dieb von Dateien** (Gerät, Sicherung, USB-Stick) | Ohne Master-Passwort **und** zweiten Faktor (Key oder Schlüsseldatei) wertlos. Argon2id mit mindestens 64 MiB |
| **Finder eines Keys oder des Notfallblatts** | Key entfernen bzw. neues Blatt erstellen: Danach öffnet es nichts mehr, was gespeichert wird, auch nicht zusammen mit einer alten Sicherung (B1) |
| **Netzwerk** (WLAN, Provider) | TLS mit Zertifikatsprüfung oder bestätigtem Fingerabdruck, SFTP mit festem Host-Schlüssel, IMAP/SMTP nie unverschlüsselt |
| **Absender böswilliger Mails** | HTML ohne Skripte und ohne Zugriff aufs eigene Netz, echte Link-Ziele sichtbar, keine eingeschleusten Kopfzeilen, Autocrypt-Regeln gegen untergeschobene Schlüssel |
| **Andere Apps auf dem Gerät** | Autofill nur auf der echten Seite und nur aus vertrauenswürdigen Browsern, keine privaten Dateien in fremde Mail-Entwürfe, keine Backups, Bildschirmschutz |
| **Präparierte Dateien und Antworten** | Alle Parser gefuzzt; kein Absturz, keine Datei außerhalb des Ordners, keine Zip-Bombe |

Nicht im Bedrohungsmodell: ein bereits kompromittiertes, entsperrtes Gerät (Schadsoftware mit Root, fremde
Tastatur-App, Bedienungshilfen-Dienste). Dagegen schützt keine App (siehe 4.3).

## 3. Behoben

Nach Bereich zusammengefasst; die Nummern führen ins [Testprotokoll](TESTPROTOKOLL.md) bzw. zur Tabelle 6.

| Bereich | Was behoben wurde | Nummern |
|---|---|---|
| **Synchronisation** | Eine kaputte, fehlende oder neuere Datei (auf dem Server oder lokal) löscht nie mehr einen Tresor; Löschungen nur über ausdrückliche Löschmarken. Abgerissene Uploads blockieren nichts. Geräte, die lange offline waren, verlieren nichts und überschreiben nichts, was sie nicht lesen können | F01 bis F06, F40, F41 |
| **Schlüssel und Format** | Neues Notfallblatt, neues Passwort, entfernter Key: neue innere Schlüssel in jeder Datei; ein entfernter Key erneuert das Faktor-Geheimnis für die übrigen Keys, ohne sie zu berühren (öffentlicher Schlüssel je Key, Formatversion 2). Dateinamen verraten keine Tresornamen | B1, F23 |
| **Netzwerk-Parser** | Abstürze durch präparierte HTTP-, SMTP- und FTP-Antworten (teils vor STARTTLS von jedem im WLAN auslösbar) | F07 bis F10 |
| **Mail** | Kopfzeilen-Einschleusung, versteckte Link-Ziele, Autocrypt-Schlüsseltausch durch gefälschten Absender, Bilder an `localhost` und das Heimnetz | F13, F16, F17, F28 |
| **Autofill** | Basis-Domain zu grob (`evil.github.io`), jede App konnte eine Webadresse behaupten, Adressen mit Backslash und IPv4-Schreibweisen anders gelesen als im Browser | F18, F19, F25, F26 |
| **Andere Apps** | Private Dateien über „Per E-Mail senden“ in Entwürfe geschmuggelt; Dev-APK debuggbar | F20, F21 |
| **Dateien von außen** | Sicherungen mit Pfad-Tricks und Zip-Bomben; Dateinamen vom Server mit Gerätenamen, Doppelpunkten, Steuer- und Richtungszeichen; Drive-Namen, die den Dateityp tarnen | F11, F14, F27 |
| **Daten, die das Gerät verlassen** | CSV-Export als Formel ausführbar; Geheimnisse in Debug-Ausgaben; Passwort blieb nach dem Sperren in der Zwischenablage | F15, F22, F29 |
| **Entsperren** | App-PIN-Zähler zählte erst nach dem Versuch (Freiversuch durch Abschießen der App) | F31 |
| **CI** | Tests gegen echte Server liefen nach einem Runner-Wechsel ins Leere und wurden grün gemeldet | F39 |
| **Freigaben und PHP-Modul** (Audit 0.5.0) | Abgelaufene Pakete abrufbar, beliebige PBKDF2-Runden aus dem Paket, ausführbare Dateien im Datenordner, Namen mit Punkt am Ende, fehlende Kopfzeilen | A3 bis A5, F30 |
| **Android-Teilen und Anhänge** (Audit 0.5.0) | Dateinamen wie `..`, Anhänge beliebiger Größe | A6, A7 |
| **Sync, Runde 5** | Ein entfernter Key kommt nie per Sync zurück (Löschmarken je Key). Geänderte Server- und Kontodaten erreichen andere Geräte. Gelöscht auf A, geändert auf B: feste Regel, eine nie gesehene Änderung bleibt. Konfliktkopien fremder Sync-Programme werden zusammengeführt. Ein Server kann keine übermäßig teuren Argon2-Werte erzwingen | B2, B11, B13, B18, F42 |
| **Drive** | Ein zurückgesetzter Index auf dem Server wird abgelehnt (nichts verschwindet); zwei Geräte, die gleichzeitig ändern, verlieren nichts | B3 |
| **Android** | Benachrichtigungen, die bei gesperrtem Gerät eintreffen, enthalten nur ihre Sperrbildschirm-Fassung (Android lässt Apps die Kanal-Einstellung nicht vorgeben, daher entscheidet die App beim Eintreffen); Mail-Zugang für Benachrichtigungen bevorzugt in StrongBox, wahlweise löscht ihn die Sofortsperre; `.ics` von fremden Apps nur nach Vorschau; interne Aktionen nur aus eigenen Benachrichtigungen; Auto-Sperre zählt die echte Zeit; Autofill fragt vor dem Verknüpfen; kein Tippen durch darübergelegte Fenster | B4 bis B9, L10, F43 |
| **Windows** | Entschlüsselte Kopien werden überschrieben und gelöscht, auch wenn ein Programm sie erst später freigibt; portabel bleiben sie im Datenordner; Arbeitsverzeichnis nicht mehr im DLL-Suchpfad | B10, L13 |
| **Mail** (Runde 5) | Herkunft von Kontaktschlüsseln gemerkt; Fingerabdruck vergleichen und bestätigen, Warnung bei Wechsel; Bilder nur von genau der geprüften Adresse (kein DNS-Rebinding) | B12, B16, L7 |
| **PHP-Modul und Freigaben** (Runde 5) | Gültig signierte Anfragen werden nie gesperrt, Doppel-Endungen abgelehnt, Dateien 0600; Server ohne PHP-Ausführung wird erkannt und die Datei sofort gelöscht; Zurückziehen meldet Fehler ehrlich | B14, B15, L5 |
| **Hinweise** | Nach einem Verlust sagt die App, was zu wechseln ist (Mail- und Server-Passwörter) | L1 |
| **Abhängigkeiten** | `russh` (Speicher-Angriff durch bösartigen SSH-Server), `zip`, `crypto-bigint` über `p256` auf sichere Versionen | A1, A2, R1 |

## 4. Offen

Aus den Testrunden und Audits ist **nichts mehr offen**. Was bleibt, sind zwei bewusste Entscheidungen und die
Grenzen, die keine App lösen kann (4.3). Nichts davon gibt einem Angreifer ohne Passwort **und** zweiten Faktor
Zugriff auf Tresorinhalte.

### 4.1 Bewusst entschieden

| Nr. | Entscheidung | Begründung |
|---|---|---|
| L4, B17 | **Ein Signaturschlüssel für alles:** Entwicklerversionen aller eigenen Branches und spätere Veröffentlichungen werden mit demselben Schlüssel signiert, ein getrennter Release-Schlüssel ist nicht vorgesehen | Entwicklerversionen sollen sich über die installierte App aktualisieren lassen; mit einem anderen Schlüssel müsste man sie erst deinstallieren und verlöre die Daten auf dem Gerät. Ein zweiter Schlüssel schützte vor allem gegen einen übernommenen GitHub-Zugang, und mit dem ließe sich auch `main` ändern. Schutz: Der Schlüssel steht nur bei Pushes im eigenen Repository bereit, nie bei Pull Requests oder Forks; pushen kann nur der Inhaber. Voraussetzungen: Zwei-Faktor-Anmeldung bei GitHub (Passkey oder Security Key), eine Offline-Sicherung der Keystore-Datei und ihres Passworts (ohne sie lässt sich keine installierte App mehr aktualisieren), Abhängigkeiten nur bewusst aktualisieren (Gradle-Prüfsummen, `Cargo.lock`) |
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

### 4.3 Grenzen, die keine App lösen kann

| Nr. | Grenze |
|---|---|
| L2 | Kein unabhängiger Audit: Eigene Tests finden nur, woran man denkt |
| L6 | Der Server sieht Anzahl, Größe und Änderungszeit der Dateien und die IP-Adresse |
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
  Testvektoren für beide Formatversionen, unabhängig nachgerechnet in Python mit anderen Krypto-Bibliotheken.
- **Kryptografie:** XChaCha20-Poly1305, HKDF-SHA256, Argon2id, X25519; alle Bausteine bestehen die Wycheproof-Fälle.
  Frische Nonces bei jedem Schreiben, Geheimnisse werden im Kern genullt, Vergleiche in konstanter Zeit.
- **FIDO2:** nur mit PIN-Prüfung, eigene RP-ID, die keine Webseite ansprechen kann.
- **Schnell-Entsperren:** Android-Keystore, Fingerabdruck „stark“, ungültig bei neuem Fingerabdruck, Fehlversuche
  begrenzt, an den Neustart gebunden; Sofortsperre löscht es.
- **Dateien:** kein Klartext auf Gerät, Server, in Sicherung oder Export (gesucht auch als UTF-16, Hex, Base64),
  auch nicht in Datei- und ZIP-Namen. Gestohlene Dateien öffnen sich nur mit beiden Faktoren.
- **Drive:** gekippte Bits, vertauschte oder gelöschte Blöcke, gefälschter Index: immer eine Fehlermeldung, nie
  falsche Daten.
- **Mail-Archiv:** Der Server sieht keine Adressen, Ordner, Monate oder Inhalte (Namen sind HMACs, die zusätzlichen
  Daten jeder Verschlüsselung enthalten den Pfad, vertauschte Dateien werden erkannt). Beim Anbieter wird nur gelöscht,
  was über eine zweite Verbindung zurückgelesen und unmittelbar vorher erneut vollständig geprüft wurde, genau per
  UID und nur bei unveränderter UIDVALIDITY; Löschen ist aus, bis es eingeschaltet und einmal bestätigt ist.
  Abbrüche nach jedem Schritt, beschädigte, fehlende und vertauschte Archivdateien, neu nummerierte Ordner und
  doppelte Mails sind getestet; nach jedem Schritt gilt „Anbieter + Archiv nie weniger als vorher“.
- **Freigaben:** zufälliger 256-Bit-Schlüssel je Freigabe, nur im Teil nach `#`; AES-256-GCM mit eindeutigen Nonces,
  Abschneiden und Vertauschen werden erkannt; optional Passwort (PBKDF2, 600 000 Runden). Empfängerseite mit
  strenger CSP, ohne fremde Inhalte, ohne Referrer.
- **PHP-Modul:** jeder Aufruf signiert (HMAC über Methode, Parameter, Zeit, Nonce, Inhalt), Einmal-Nonces gegen
  Wiederholung, Pfadprüfung, atomares Schreiben, zufälliger gesperrter Datenordner, keine Datenbank, keine
  Fehlerdetails. 347 gezielte Angriffe, PHPStan im Nachtlauf, PHP-8.1-Syntax in der CI.
- **Netz:** SFTP mit festem Host-Schlüssel (Host-Zertifikate abgelehnt), TLS mit webpki oder bestätigtem
  Fingerabdruck, `http://` nur für `localhost`, FTPS nur mit TLS und `PROT P`, kein „FTP bounce“.
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

- **Key oder Notfallblatt verloren:** Key entfernen bzw. neues Notfallblatt erstellen. Danach öffnet das Verlorene
  nichts mehr, was gespeichert wird. Alte Sicherungen zeigen weiter den Stand von damals, auch die Zugänge von damals:
  Mail- und Server-Passwörter beim Anbieter wechseln (die App erinnert unter Optionen → Sicherheit daran).
- **Freigabelinks:** Wer den Link hat, kann die Datei öffnen. Für Sensibles ein Passwort setzen und getrennt
  mitteilen. Ein Einmal-Link gilt als verbraucht, sobald der Abruf beginnt.
- **Webserver ohne `.htaccess`** (z. B. nginx): Dort schützt nur der zufällige Name den Datenordner bzw. die
  zufällige ID das Paket; Inhalte bleiben verschlüsselt.
- **Speicherort:** Der App-Speicher ist für die meisten am sichersten. Einen eigenen Ordner (Optionen → Tresore →
  „Speicherort wechseln“) brauchst du nur, wenn du die Dateien selbst sichern oder mit einem eigenen Programm abgleichen
  willst. Dateien in einem gemeinsamen Ordner können andere Apps sehen, aber nicht öffnen.
- **Windows portable:** nur auf eigenen Geräten entsperren; auf einem fremden PC kann ein Keylogger mitlesen.
- GhulSuite ist eine **Entwicklungsversion**: mit regelmäßiger Sicherung nutzen.
