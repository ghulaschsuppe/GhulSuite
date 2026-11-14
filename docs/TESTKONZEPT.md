# Testkonzept GhulSuite

Ziel: Die Suite läuft stabil, verliert nie Daten und gibt nie Geheimnisse preis, auch wenn Server, Netz, Dateien, andere Apps oder der Nutzer selbst „allen möglichen Mist“ machen. Dieses Konzept beschreibt, *was* geprüft wird, *womit* (eigene Tests und fertige Testsammlungen aus dem Netz) und *wann* es in der CI läuft.

## 1. Was immer gelten muss (Orakel)

Jeder Test prüft eine oder mehrere dieser Zusagen. Ein Test, der keine davon prüft, ist kein Test.

| Nr. | Zusage |
|---|---|
| Z1 | **Kein Datenverlust.** Was gespeichert wurde, ist nach jedem Fehler (Netz, Server, Absturz, volle Platte, kaputte Datei) noch da, auf mindestens einem Gerät und nach dem nächsten Sync überall. Gelöscht wird nur, was jemand ausdrücklich löscht. |
| Z2 | **Kein Klartext außerhalb.** Auf Server, in Sicherungen, Freigaben, temporären Dateien, Logs und Benachrichtigungen (Sperrbildschirm) steht nichts Geheimes. |
| Z3 | **Kein Absturz durch fremde Daten.** Antworten von Servern, Mails, Dateien, Importe, Links, andere Apps: alles darf kaputt, riesig oder bösartig sein, die App meldet einen Fehler und läuft weiter. |
| Z4 | **Richtiger Empfänger.** Passwörter gehen nur an die richtige Website/App (Autofill), Mails nur an gewollte Empfänger, Freigaben nur an Linkinhaber. |
| Z5 | **Geräte werden gleich.** Nach Synchronisieren zeigen alle Geräte dasselbe, egal in welcher Reihenfolge, wie oft und mit welchen Unterbrechungen synchronisiert wurde (Kommutativität, Idempotenz). |
| Z6 | **Nichts geht still schief.** Jeder Fehler wird verständlich gemeldet (deutsch, mit nächstem Schritt), keine Sackgasse, keine Endlosschleife. |
| Z7 | **Sperre hält.** Gesperrt heißt: keine Daten lesbar, kein Weg vorbei (Intents, Autofill, Benachrichtigungen, Zwischenablage, Bildschirmfotos). |
| Z8 | **Zeit ist kein Feind.** Zeitzonen, Sommerzeit, Schaltjahre, Uhr falsch gestellt, Jahr 0001/9999, nichts verrutscht, nichts stürzt ab. |

## 2. Ebenen

### E1: Testvektoren (Richtigkeit der Bausteine)
- **Eigene Vektoren** (`docs/testvectors`, unabhängig in Python geprüft), vorhanden.
- **RFC-Vektoren:** TOTP/HOTP (RFC 6238/4226), HKDF (RFC 5869), Argon2id (RFC 9106), Base32 (RFC 4648).
- **Wycheproof** (Google/C2SP, Apache-2.0): tausende Grenz- und Angriffsfälle für XChaCha20-Poly1305, AES-GCM, HKDF, HMAC-SHA256, ECDH P-256, ECDSA P-256. Prüft, dass unsere Krypto-Bausteine falsche Tags, kurze Nonces, ungültige Kurvenpunkte usw. ablehnen.

### E2: Eigenschaftstests (Rundreisen und Regeln)
Zufällig erzeugte, aber gültige Daten; geprüft wird eine Eigenschaft statt eines festen Ergebnisses:
- verschlüsseln → entschlüsseln = gleich (Container, Freigaben, Drive-Blöcke, Sicherungen);
- exportieren → importieren = gleich (CSV, Bitwarden, Aegis, ICS, otpauth-Links);
- Zusammenführen: `merge(a,b) = merge(b,a)`, `merge(a,a) = a`, Reihenfolge der Geräte egal (Z5);
- Termin-Serien: jede berechnete Wiederholung liegt im Zeitraum und auf dem richtigen Wochentag.

### E3: Absurde Eingaben
- **Big List of Naughty Strings** (MIT): ~500 bekannt gefährliche Zeichenketten (Unicode-Tricks, Steuerzeichen, SQL/Shell/Script-Fragmente, Emoji-Ketten, Rechts-nach-links, Null-Bytes), durch **jedes** Textfeld der API: Titel, Benutzernamen, Passwörter, Notizen, Ordner- und Dateinamen, Kalendertitel, Mail-Betreff, Kontonamen, Servernamen.
- Grenzwerte: leer, 1 Zeichen, 1 MB, 10.000 Felder, 100.000 Einträge; Passwörter in NFC/NFD, mit Leerzeichen, 10.000 Zeichen.
- Unmögliche Daten: 30. Februar, 24:00, Ende vor Beginn, Serie bis vor dem Start, 29. Februar jährlich, Sommerzeit-Lücke 02:30, Jahr 0001/9999.
- Dateinamen: `..`, `/`, `\`, `CON`, 255+ Zeichen, nur Leerzeichen, gleiche Namen, Unicode-Normalisierung.

### E4: Fuzzing (Zufall in jeden Parser)
- **Schnell, bei jedem Push:** Zufallsdaten und Mutationen echter Beispiele (Bits kippen, abschneiden, verdoppeln, Sonderzeichen einfügen) gegen jeden Parser: HTTP, SMTP, FTP (PASV, LIST, MLSD), Mails (MIME, Header, HTML-Filter, Autocrypt), ICS, Schnell-Eintragen, alle Importformate, otpauth- und Migrationslinks, Freigabe-Pakete, Containerformat, CBOR. Zusage Z3.
- **Lang, regelmäßig:** `cargo-fuzz` (libFuzzer, mit AddressSanitizer), je Parser ein Ziel, Korpus aus echten Dateien; Funde werden als Regressionstest übernommen.

### E5: Chaos und Fehlerinjektion
- Server, der zufällig ausfällt, Uploads halb schreibt, Dateien verliert oder beschädigt (`FaultyFs`), vorhanden seit diesem Durchgang.
- Lokale Datei kaputt, Speicher voll, Schreiben bricht nach N Bytes ab, App „stürzt“ mitten im Speichern ab (Fehler nach dem n-ten Schreibvorgang, dann Neustart).
- Uhr springt vor/zurück, Zeitzone wechselt zwischen Geräten.
- Kaputte Server: kleine Testserver, die absichtlich Müll antworten (falsche Längen, abgebrochene Verbindung, Mehrbyte-Zeichen, riesige Zeilen) für HTTP, SMTP, IMAP, FTP.

### E6: Modell-Simulation (viele Geräte, viele Tage)
- Alltagstest (vorhanden): 2 Geräte, 30 Tage.
- Zufällige Aktionsfolgen auf 3 Geräten gegen ein einfaches Referenzmodell, mit festen Startwerten (wiederholbar) und Verkleinerung eines Fehlers auf die kürzeste Folge.

### E7: Nebenläufigkeit
Parallele Threads (vorhanden), dazu: Sperren während Sync/Upload/Import, Entsperren und Sperren gleichzeitig, zwei Syncs gleichzeitig.

### E8: Angriffe (Sicherheit)
- **Autofill-Phishing-Matrix:** Public-Suffix-Liste und deren offizielle Testdatei `test_psl.txt` (MPL-2.0), IP-Adressen, Punycode/IDN-Verwechslungen, Ports, Groß/Klein, Punkt am Ende, Benutzer@Host, gefälschte Webadressen aus Apps.
- **Andere Apps:** Intents mit eigenen privaten Dateien, `file://`, fremde Content-Provider, riesige oder endlose Streams, gefälschte Benachrichtigungs-Aktionen.
- **Mail:** Header-Injection (CR/LF), gefälschter Absender mit Autocrypt, HTML-Mails mit Skript-, Event-, CSS- und Link-Tricks (Vektoren nach dem OWASP-XSS-Cheat-Sheet, selbst formuliert), Tracking-Pixel, Weiterleitungen auf `localhost`.
- **PHP-Modul:** Pfad-Tricks, Wiederholungsangriffe, falsche Signaturen, Zeitfenster, Teil-Uploads außer der Reihe, Endungs-Tricks (`x.php.jpg`).
- **Rückfall-Angriffe:** alte Dateiversionen einspielen, entfernte Keys/alte Codes, Server mit teuren KDF-Werten.
- **Geheimnis-Suche:** Marker-Text in allen Eingaben, danach alle geschriebenen Dateien, Server-Ordner, temporären Dateien und Logs nach dem Marker durchsuchen (Z2).

### E9: Echte Server (Integration)
SFTP, FTPS, IMAP, SMTP, PHP-Modul im Browser, vorhanden; erweitert um die kaputten Server aus E5.

### E10: App und Oberfläche
- **Desktop-UI-Tests** (Compose, Fake-Backends): alle Hauptwege, absurde Eingaben (Naughty Strings in Formularen), schnelles Doppeltippen, Zurück an jeder Stelle.
- **Maestro-Abläufe** im Android-Emulator, vorhanden: bei Pull Requests und auf `main` die Grundabläufe aller fünf Bereiche, nachts zusätzlich gegen echte Testserver.
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
| Public Suffix List + `test_psl.txt` | MPL-2.0 | E8: Autofill-Domains |
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
| Nachts / wöchentlich (geplant) | cargo-fuzz je Ziel 10 min, Monkey 50.000 Gesten × 3, Mutationstests, MobSF, Semgrep, PHPStan, gitleaks, cargo-deny | 1 bis 2 h |

Jeder gefundene Fehler wird zuerst als Test festgehalten (rot), dann behoben (grün), so kommt er nie zurück.

## 5. Reihenfolge der Umsetzung

1. **Chaos-Sync (E5)**: erledigt: Datenverlust bei kaputten/fehlenden Dateien gefunden und behoben.
2. **Fuzz-Kurzläufe aller Parser (E4)**: Abstürze beheben (Audit A4).
3. **Naughty Strings und Grenzwerte (E3)** durch alle Kern-APIs.
4. **Angriffs-Matrix (E8):** Autofill mit Public-Suffix-Liste (A2, A3), Intents (A6), Mail-Header und HTML.
5. **Wycheproof und RFC-Vektoren (E1).**
6. **Android Monkey und Desktop-UI-Tests (E10), Dev-APK nicht debuggbar (A5).**
7. **Geplanter Nachtlauf (E4 lang, E11, E12).**
8. Danach: Audit-Punkte B (Schlüsselrotation, Autocrypt, Löschbestätigungen, deutsche Fehlermeldungen).
