# Testkonzept GhulSuite

Ziel: Die Suite läuft stabil, verliert nie Daten und gibt nie Geheimnisse preis, auch wenn Server, Netz, Dateien, USB-Sticks, andere Apps oder der Nutzer selbst „allen möglichen Mist“ machen. Dieses Konzept beschreibt, *was* geprüft wird, *womit* (eigene Tests und fertige Testsammlungen aus dem Netz) und *wann* es in der CI läuft.

**Stand:** 17.11.2026, nach dem Umbau ohne Synchronisation ([`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)): Die Suite
wandert als eine Datei (`.ghlzip`), die man speichert und öffnet; Server nur per SFTP; GhulDrive mit Sperre, Revision
und Schlüsselwechsel. Tests, die nur die Synchronisation, FTP/FTPS oder die frühere Sicherung prüften, sind entfallen.

## 1. Was immer gelten muss (Orakel)

Jeder Test prüft eine oder mehrere dieser Zusagen. Ein Test, der keine davon prüft, ist kein Test.

| Nr. | Zusage |
|---|---|
| Z1 | **Kein Datenverlust.** Was gespeichert wurde, ist nach jedem Fehler (Netz, Server, Absturz, volle Platte, abgezogener Stick, kaputte Datei) noch da: in der Arbeitskopie, in der zuletzt gespeicherten Suite-Datei bzw. in GhulDrive auf dem Server. Gelöscht wird nur, was jemand ausdrücklich löscht; was beim Öffnen einer Suite-Datei verloren ginge, sagt die App vorher. |
| Z2 | **Kein Klartext außerhalb.** Auf Server, in der Suite-Datei, in Freigaben, temporären Dateien, Logs und Benachrichtigungen (Sperrbildschirm) steht nichts Geheimes. |
| Z3 | **Kein Absturz durch fremde Daten.** Antworten von Servern, Mails, Dateien, Importe, Links, andere Apps: alles darf kaputt, riesig oder bösartig sein, die App meldet einen Fehler und läuft weiter. |
| Z4 | **Richtiger Empfänger.** Mail-Links zeigen ihr echtes Ziel, Mails gehen nur an gewollte Empfänger, Freigaben nur an Linkinhaber. |
| Z5 | **Der Stand ist eindeutig.** Aktuell ist die zuletzt gespeicherte Suite-Datei. Eine ältere, abgezweigte oder fremde Datei ersetzt nichts ohne Rückfrage, eine mit älterem Schutz gar nicht; geöffnet wird ganz oder gar nicht. GhulDrive zeigt auf allen Geräten dasselbe, auch wenn zwei gleichzeitig ändern. |
| Z6 | **Nichts geht still schief.** Jeder Fehler wird verständlich gemeldet (deutsch, mit nächstem Schritt), keine Sackgasse, keine Endlosschleife. |
| Z7 | **Sperre hält.** Gesperrt heißt: keine Daten lesbar, kein Weg vorbei (Intents, Benachrichtigungen, Zwischenablage, Bildschirmfotos). |
| Z9 | **Alte Kopien bleiben alt.** Eine alte Suite-Datei, ein alter Code, ein entfernter Key oder ein altes Passwort öffnet nichts, was danach gespeichert wurde, kommt nach dem Schlüsselwechsel nicht mehr an Server und GhulDrive, und ohne das aktuelle Passwort lässt sich keine Datei unterschieben (Schreibsiegel). |
| Z8 | **Zeit ist kein Feind.** Zeitzonen, Sommerzeit, Schaltjahre, Uhr falsch gestellt, Jahr 0001/9999, nichts verrutscht, nichts stürzt ab. |

## 2. Ebenen

### E1: Testvektoren (Richtigkeit der Bausteine)
- **Eigene Vektoren** (`docs/testvectors`, unabhängig in Python geprüft), vorhanden.
- **RFC-Vektoren:** TOTP/HOTP (RFC 6238/4226), HKDF (RFC 5869), Argon2id (RFC 9106), Base32 (RFC 4648).
- **Wycheproof** (Google/C2SP, Apache-2.0): tausende Grenz- und Angriffsfälle für XChaCha20-Poly1305, AES-GCM, HKDF, HMAC-SHA256, ECDH P-256, ECDSA P-256. Prüft, dass unsere Krypto-Bausteine falsche Tags, kurze Nonces, ungültige Kurvenpunkte usw. ablehnen.

### E2: Eigenschaftstests (Rundreisen und Regeln)
Zufällig erzeugte, aber gültige Daten; geprüft wird eine Eigenschaft statt eines festen Ergebnisses:
- verschlüsseln → entschlüsseln = gleich (Container, Freigaben, Drive-Blöcke);
- speichern → öffnen = gleich (Suite-Datei `.ghlzip` mit allen Bereichen, auch über viele Geräte und Speicherungen);
- exportieren → importieren = gleich (CSV, Bitwarden, Aegis, ICS, otpauth-Links);
- Stand der Suite-Datei: neuer, älter, Abzweig und andere Suite werden richtig erkannt, auch viele Speicherungen auseinander (Z5);
- Termin-Serien: jede berechnete Wiederholung liegt im Zeitraum und auf dem richtigen Wochentag.

### E3: Absurde Eingaben
- **Big List of Naughty Strings** (MIT): ~500 bekannt gefährliche Zeichenketten (Unicode-Tricks, Steuerzeichen, SQL/Shell/Script-Fragmente, Emoji-Ketten, Rechts-nach-links, Null-Bytes), durch **jedes** Textfeld der API: Titel, Benutzernamen, Passwörter, Notizen, Ordner- und Dateinamen, Kalendertitel, Mail-Betreff, Kontonamen, Servernamen.
- Grenzwerte: leer, 1 Zeichen, 1 MB, 10.000 Felder, 100.000 Einträge; Passwörter in NFC/NFD, mit Leerzeichen, 10.000 Zeichen.
- Unmögliche Daten: 30. Februar, 24:00, Ende vor Beginn, Serie bis vor dem Start, 29. Februar jährlich, Sommerzeit-Lücke 02:30, Jahr 0001/9999.
- Dateinamen: `..`, `/`, `\`, `CON`, 255+ Zeichen, nur Leerzeichen, gleiche Namen, Unicode-Normalisierung.

### E4: Fuzzing (Zufall in jeden Parser)
- **Schnell, bei jedem Push:** Zufallsdaten und Mutationen echter Beispiele (Bits kippen, abschneiden, verdoppeln, Sonderzeichen einfügen) gegen jeden Parser: HTTP, SMTP, Mails (MIME, Header, HTML-Filter, Autocrypt), ICS, Schnell-Eintragen, alle Importformate, otpauth- und Migrationslinks, Freigabe-Pakete, Containerformat, Suite-Datei (ZIP und Kopfdatei), Drive-Index, CBOR. Zusage Z3.
- **Lang, regelmäßig:** `cargo-fuzz` (libFuzzer, mit AddressSanitizer), je Parser ein Ziel, Korpus aus echten Dateien; Funde werden als Regressionstest übernommen.

### E5: Chaos und Fehlerinjektion
- Server, der zufällig ausfällt, Uploads halb schreibt, Dateien verliert oder beschädigt (der frühere `FaultyFs` der Sync-Tests ist mit der Synchronisation entfallen; GhulDrive prüfen die Tests mit einem absichtlich veränderten Server-Ordner).
- Lokale Datei kaputt, Speicher voll, Schreiben bricht nach N Bytes ab, App „stürzt“ mitten im Speichern ab (Fehler nach dem n-ten Schreibvorgang, dann Neustart).
- Öffnen einer Suite-Datei bricht beim Ersetzen ab: danach ist der alte oder der neue Stand vollständig da, nie eine Mischung. Speichern auf einen Stick, der fehlt oder mittendrin verschwindet: die bisherige Datei bleibt.
- GhulDrive: Index beschädigt (`index.bak` springt ein), Index älter als gesehen oder fehlend, verwaiste und frische Sperre (`index.lock`), Hochladen bricht zwischen Blöcken und Eintrag ab (Aufräumen).
- Schlüsselwechsel: Abbruch an jeder Stelle (Index umgeschlüsselt oder nicht, SSH-Schlüssel eingetragen oder nicht, alte Zeilen entfernt oder nicht), Server nicht erreichbar, Verbindung bricht ab; der Wechsel setzt fort und endet erst, wenn alles durch ist.
- Uhr springt vor/zurück, Zeitzone wechselt zwischen Geräten.
- Kaputte Server: kleine Testserver, die absichtlich Müll antworten (falsche Längen, abgebrochene Verbindung, Mehrbyte-Zeichen, riesige Zeilen) für HTTP, SMTP, IMAP.

### E6: Modell-Simulation (viele Geräte, viele Tage)
- Alltagstest (vorhanden): 2 Geräte, 30 Tage; die Suite wandert wie auf einem USB-Stick zwischen ihnen.
- „Bekloppter Nutzer“ (vorhanden): zufällige, oft unsinnige Aktionsfolgen auf 3 Geräten, zwischen denen die Suite-Datei wandert, gegen ein einfaches Referenzmodell, mit festen Startwerten (wiederholbar) und Verkleinerung eines Fehlers auf die kürzeste Folge.

### E7: Nebenläufigkeit
Parallele Threads (vorhanden), dazu: Sperren während Upload/Import/Speichern, Entsperren und Sperren gleichzeitig, eine Änderung während die Suite-Datei geschrieben wird (bleibt „nicht gespeichert“), zwei Geräte ändern GhulDrive gleichzeitig (Sperre `index.lock`, nichts geht verloren).

### E8: Angriffe (Sicherheit)
- **Domain-Phishing-Matrix (Mail-Links):** Public-Suffix-Liste und deren offizielle Testdatei `test_psl.txt` (MPL-2.0), IP-Adressen, Punycode/IDN-Verwechslungen, Ports, Groß/Klein, Punkt am Ende, Benutzer@Host.
- **Andere Apps:** Intents mit eigenen privaten Dateien, `file://`, fremde Content-Provider, riesige oder endlose Streams, gefälschte Benachrichtigungs-Aktionen.
- **Mail:** Header-Injection (CR/LF), gefälschter Absender mit Autocrypt, HTML-Mails mit Skript-, Event-, CSS- und Link-Tricks (Vektoren nach dem OWASP-XSS-Cheat-Sheet, selbst formuliert), Tracking-Pixel, Weiterleitungen auf `localhost`.
- **PHP-Modul:** Pfad-Tricks, Wiederholungsangriffe, falsche Signaturen, Zeitfenster, Teil-Uploads außer der Reihe, Endungs-Tricks (`x.php.jpg`).
- **Rückfall-Angriffe:** alte Suite-Dateien und Tresore einspielen, entfernte Keys, alte Codes (auch mit altem Passwort), geschwächte KDF-Werte.
- **Angriffe auf die Suite-Datei:** fehlende, vertauschte, alte, fremde und abgezweigte Teile; falsches oder fehlendes Schreibsiegel; Code ohne Passwort (öffnet nur ungeprüft und mit Hinweis); ZIP-Tricks (Ordner, doppelte und zusätzliche Einträge, falsche Größen im Verzeichnis, Zip-Bomben, zu viele Einträge).
- **Angriffe des Servers auf GhulDrive:** alten, beschädigten oder fehlenden Index zurückspielen, Index einer anderen Suite, Blöcke vertauschen oder löschen, Index unter einem alten Drive-Schlüssel ablegen (mit einer alten Kopie der Suite-Datei).
- **Nach dem Schlüsselwechsel:** Die alte Suite-Datei kommt nicht mehr an den Server und öffnet den Index nicht; aus `~/.ssh/authorized_keys` verschwinden nur die alten Schlüssel der Suite.
- **Geheimnis-Suche:** Marker-Text in allen Eingaben, danach alle geschriebenen Dateien, Server-Ordner, temporären Dateien und Logs nach dem Marker durchsuchen (Z2).

### E9: Echte Server (Integration)
SFTP, IMAP, SMTP, PHP-Modul im Browser, vorhanden; dazu der Schlüsselwechsel gegen einen echten SFTP-Server (`authorized_keys`), erweitert um die kaputten Server aus E5. FTPS ist mit dem Umbau entfallen.

### E10: App und Oberfläche
- **Desktop-UI-Tests** (Compose, Fake-Backends): alle Hauptwege, absurde Eingaben (Naughty Strings in Formularen), schnelles Doppeltippen, Zurück an jeder Stelle.
- **Maestro-Abläufe** im Android-Emulator, vorhanden: bei Pull Requests und auf `main` die Grundabläufe aller fünf Bereiche und Speichern/Öffnen der Suite-Datei (`14-suite-file.yaml`), nachts zusätzlich gegen echte Testserver (GhulDrive, Mail, Server und Freigaben).
- **Android Monkey:** zehntausende zufällige Gesten mit mehreren festen Startwerten; jeder Absturz oder „App reagiert nicht“ ist ein Fehler.
- Barrierefreiheit (Kontraste, Beschriftungen, Zielgrößen).

### E11: Statische Prüfung und Lieferkette
`cargo clippy` (vorhanden), `cargo audit` (vorhanden), `cargo deny` (Lizenzen, Quellen, doppelte Versionen), Android Lint, Semgrep-Regeln (Android, Kotlin, PHP), PHPStan für das PHP-Modul, gitleaks (Zugangsdaten im Repo), MobSF-Scan der APK nach OWASP MASVS.

### E12: Wie gut sind die Tests? (Mutationstests)
`cargo-mutants` ändert den Code absichtlich (z. B. `>` zu `>=`) und prüft, ob ein Test das bemerkt. Überlebende Mutanten zeigen Lücken.

### E13: Last
Stresstest (vorhanden), dazu: 50.000 Mails im Postfach (Fake-IMAP), 100.000 Einträge, Speicherverbrauch auf schwachen Geräten.

## 3. Fertige Testsammlungen aus dem Netz

| Sammlung | Lizenz | Wofür |
|---|---|---|
| Big List of Naughty Strings | MIT | E3: alle Textfelder |
| Project Wycheproof (C2SP) | Apache-2.0 | E1: Krypto-Bausteine |
| Public Suffix List + `test_psl.txt` | MPL-2.0 | E8: Domains von Mail-Links |
| RFC-Testvektoren (6238, 4226, 5869, 9106, 4648) | frei | E1 |
| PayloadsAllTheThings (XSS, CRLF) | MIT | E8: HTML-Mails, Mail-Kopfzeilen |
| SecLists (XSS) | MIT | E8: HTML-Mails |
| DOMPurify-Testfälle (223 XSS-Muster) | Apache-2.0/MPL-2.0 | E8: HTML-Mails |
| WHATWG/web-platform-tests URL-Testdaten | BSD-3 | E8: GhulSuite und Browser lesen Adressen gleich |
| JSONTestSuite (318 Dateien) | MIT | E4: alle JSON-Importe |
| Kalenderdateien aus den Tests von iCal4j und python-icalendar (304 Dateien, auch Fuzzing-Funde) | BSD | E2/E4: Kalender-Import, Rundreise |
| Zeitzonen-Regeln aller Zonen als VTIMEZONE (tzurl.org, IANA-Datenbank) | gemeinfrei | E1: Zeitzonen-Umrechnung gegen die Datenbank des Rechners |
| badssl.com (21 Server mit kaputtem TLS) | keine (Dienst) | E9 nachts: Zertifikats- und Protokollprüfung |
| OWASP MASVS/MASTG | CC BY-SA | Prüfliste Mobil-Sicherheit (nur als Liste, nichts kopiert) |
| OWASP XSS Filter Evasion Cheat Sheet | CC BY-SA | Ideen für HTML-Mail-Angriffe (Vektoren selbst formuliert) |
| libFuzzer / cargo-fuzz | Apache/MIT | E4 lang |
| Android Monkey, Maestro | Apache | E10 |
| Semgrep-Regeln, gitleaks, PHPStan, MobSF, cargo-deny, cargo-mutants | jeweils frei | E11, E12 |

Daten aus Sammlungen liegen unter `core/ghul-core/tests/data/` mit Herkunft und Lizenz in `NOTICE`.

## 4. Wann was läuft

| Wann | Was | Dauer |
|---|---|---|
| Jeder Push | E1 bis E9 Kurzläufe (Fuzz-Kurzlauf, Chaos, Naughty Strings, Wycheproof, Alltagstest, Angriffs-Matrix), clippy, audit | < 15 min zusätzlich |
| Jeder Push (eigener Job) | Stresstest (Release) | ~5 min |
| Nachts (`nightly.yml`) | cargo-fuzz je Ziel 15 min, lange Chaos-/Alltags-/Stresstests, Monkey 30.000 Gesten × 3 mit Intent-Angriffen, Emulator-Abläufe mit echten Testservern, gitleaks, PHPStan, Angriffe aufs PHP-Modul, cargo-audit, x509-limbo, OpenPGP mit GnuPG, OWASP ZAP, MobSF, reproduzierbarer Kern | 2 bis 3 h |
| Später (geplant) | Mutationstests (cargo-mutants), Semgrep | |

Jeder gefundene Fehler wird zuerst als Test festgehalten (rot), dann behoben (grün), so kommt er nie zurück.

## 5. Reihenfolge der Umsetzung

1. **Chaos-Sync (E5)**: erledigt: Datenverlust bei kaputten/fehlenden Dateien gefunden und behoben (die Synchronisation ist seit dem Umbau entfernt und mit ihr diese Tests; Abbrüche und kaputte Dateien beim Speichern, Öffnen und in GhulDrive prüfen jetzt `archive.rs`, `security.rs` und `keychange.rs`).
2. **Fuzz-Kurzläufe aller Parser (E4)**: Abstürze beheben (Audit A4).
3. **Naughty Strings und Grenzwerte (E3)** durch alle Kern-APIs.
4. **Angriffs-Matrix (E8):** Domains mit Public-Suffix-Liste (A2, A3), Intents (A6), Mail-Header und HTML.
5. **Wycheproof und RFC-Vektoren (E1).**
6. **Android Monkey und Desktop-UI-Tests (E10), Dev-APK nicht debuggbar (A5).**
7. **Geplanter Nachtlauf (E4 lang, E11, E12).**
8. Danach: Audit-Punkte B (Schlüsselrotation, Autocrypt, Löschbestätigungen, deutsche Fehlermeldungen).
9. **Umbau ohne Synchronisation:** Tests für Speichern und Öffnen der Suite-Datei mit allen Rückfragen und Angriffen (`app/tests/archive.rs`), einzelne Tresore und Weitergeben (`single.rs`), GhulDrive mit Sperre, `index.bak`, Revision und Aufräumen (`security.rs`), Schlüsselwechsel mit Abbrüchen an jeder Stelle und alter Datei danach (`keychange.rs`, `rekey.rs`).
