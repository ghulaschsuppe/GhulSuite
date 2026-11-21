# Testkonzept GhulSuite

Ziel: Die Suite läuft stabil, verliert nie Daten und gibt nie Geheimnisse preis, auch wenn Netz, Dateien, USB-Sticks, andere Apps oder der Nutzer selbst „allen möglichen Mist“ machen. Dieses Konzept beschreibt, *was* geprüft wird, *womit* (eigene Tests und fertige Testsammlungen aus dem Netz) und *wann* es in der CI läuft.

**Stand:** 20.11.2026, nach dem Umbau zu GhulDocs ([`UMBAU-GHULDOCS.md`](UMBAU-GHULDOCS.md)): eine reine
Offline-App. Die Suite wandert als eine Datei (`.ghlzip`, Datenstrom bis 3,75 GiB), die man speichert und öffnet;
Dokumente liegen verschlüsselt in GhulDocs (seit 20.11.2026 bis zu 3 Tresore, je bis 1 GB), Scannen auf Android nur im Arbeitsspeicher. Tests, die nur die
Synchronisation, FTP/FTPS, die frühere Sicherung, GhulMail, GhulDrive, Server, das PHP-Modul, Freigaben oder den
Schlüsselwechsel prüften, sind mit diesen Funktionen entfallen; Testserver gibt es nicht mehr.

## 1. Was immer gelten muss (Orakel)

Jeder Test prüft eine oder mehrere dieser Zusagen. Ein Test, der keine davon prüft, ist kein Test.

| Nr. | Zusage |
|---|---|
| Z1 | **Kein Datenverlust.** Was gespeichert wurde, ist nach jedem Fehler (Absturz, volle Platte, abgezogener Stick, kaputte Datei, Abbruch beim Hinzufügen, Öffnen oder Speichern) noch da: in der Arbeitskopie bzw. in der zuletzt gespeicherten Suite-Datei. Gelöscht wird nur, was jemand ausdrücklich löscht; was beim Öffnen einer Suite-Datei verloren ginge, sagt die App vorher. |
| Z2 | **Kein Klartext außerhalb.** In der Arbeitskopie, der Suite-Datei, temporären Dateien, Logs und Benachrichtigungen (Sperrbildschirm) steht nichts Geheimes; Dokumente und Scans erreichen den Datenträger nur verschlüsselt (Ausnahme: ausdrücklicher Export und die befristete Kopie für „Mit App öffnen“). |
| Z3 | **Kein Absturz durch fremde Daten.** Suite-Dateien, Tresore, Dokumente, Importe, Links, Antworten von GitHub und Pwned Passwords, andere Apps: alles darf kaputt, riesig oder bösartig sein, die App meldet einen Fehler und läuft weiter. |
| Z4 | **Richtiger Empfänger.** Ein Dokument geht nur an die App oder den Ort, die der Nutzer wählt; nach draußen gehen nur die Update-Anfrage und 5 Zeichen eines SHA-1-Werts (Leak-Prüfung). |
| Z5 | **Der Stand ist eindeutig.** Aktuell ist die zuletzt gespeicherte Suite-Datei. Eine ältere, abgezweigte oder fremde Datei ersetzt nichts ohne Rückfrage, eine mit älterem Schutz gar nicht; geöffnet wird ganz oder gar nicht, mit allen Dokumenten. |
| Z6 | **Nichts geht still schief.** Jeder Fehler wird verständlich gemeldet (deutsch, mit nächstem Schritt), keine Sackgasse, keine Endlosschleife. |
| Z7 | **Sperre hält.** Gesperrt heißt: keine Daten lesbar, kein Weg vorbei (Intents, Benachrichtigungen, Zwischenablage, Bildschirmfotos). |
| Z9 | **Alte Kopien bleiben alt.** Eine alte Suite-Datei, ein alter Code, ein entfernter Key oder ein altes Passwort öffnet nichts, was danach gespeichert wurde (auch keine Dokumente, die danach hinzukamen), eine Suite-Datei mit älterem Schutz öffnet auf einem Gerät mit neuerem Stand nie, und ohne das aktuelle Passwort lässt sich keine Datei unterschieben (Schreibsiegel). |
| Z8 | **Zeit ist kein Feind.** Zeitzonen, Sommerzeit, Schaltjahre, Uhr falsch gestellt, Jahr 0001/9999, nichts verrutscht, nichts stürzt ab. |

## 2. Ebenen

### E1: Testvektoren (Richtigkeit der Bausteine)
- **Eigene Vektoren** (`docs/testvectors`, unabhängig in Python geprüft), vorhanden.
- **RFC-Vektoren:** TOTP/HOTP (RFC 6238/4226), HKDF (RFC 5869), Argon2id (RFC 9106), Base32 (RFC 4648).
- **Wycheproof** (Google/C2SP, Apache-2.0): tausende Grenz- und Angriffsfälle für XChaCha20-Poly1305, HKDF, HMAC-SHA256 und ECDH P-256. Prüft, dass unsere Krypto-Bausteine falsche Tags, kurze Nonces, ungültige Kurvenpunkte usw. ablehnen.

### E2: Eigenschaftstests (Rundreisen und Regeln)
Zufällig erzeugte, aber gültige Daten; geprüft wird eine Eigenschaft statt eines festen Ergebnisses:
- verschlüsseln → entschlüsseln = gleich (Container, Inhaltsdateien von GhulDocs in allen Größen um die Abschnitts- und Füllgrenzen);
- speichern → öffnen = gleich (Suite-Datei `.ghlzip` mit allen Bereichen und Dokumenten, auch über viele Geräte und Speicherungen; ein GhulDocs-Tresor als GhulDocs-Paket `.ghldocs`, für sich geöffnet oder als eigener GhulDocs-Tresor in eine andere Suite übernommen; ein ganzer Tresor jedes Bereichs als Tresor weitergegeben und wieder geöffnet);
- exportieren → importieren = gleich (CSV, Bitwarden, Aegis, ICS, otpauth-Links);
- Stand der Suite-Datei: neuer, älter, Abzweig und andere Suite werden richtig erkannt, auch viele Speicherungen auseinander (Z5);
- Termin-Serien: jede berechnete Wiederholung liegt im Zeitraum und auf dem richtigen Wochentag.

### E3: Absurde Eingaben
- **Big List of Naughty Strings** (MIT): ~500 bekannt gefährliche Zeichenketten (Unicode-Tricks, Steuerzeichen, SQL/Shell/Script-Fragmente, Emoji-Ketten, Rechts-nach-links, Null-Bytes), durch **jedes** Textfeld der API: Titel, Benutzernamen, Passwörter, Notizen, Ordner- und Dateinamen (auch in GhulDocs), Kalendertitel, Tresornamen.
- Grenzwerte: leer, 1 Zeichen, 1 MB, 10.000 Felder, 100.000 Einträge; Passwörter in NFC/NFD, mit Leerzeichen, 10.000 Zeichen.
- Unmögliche Daten: 30. Februar, 24:00, Ende vor Beginn, Serie bis vor dem Start, 29. Februar jährlich, Sommerzeit-Lücke 02:30, Jahr 0001/9999.
- Dateinamen: `..`, `/`, `\`, `CON`, 255+ Zeichen, nur Leerzeichen, gleiche Namen, Unicode-Normalisierung.

### E4: Fuzzing (Zufall in jeden Parser)
- **Schnell, bei jedem Push:** Zufallsdaten und Mutationen echter Beispiele (Bits kippen, abschneiden, verdoppeln, Sonderzeichen einfügen) gegen jeden Parser: HTTP (Update- und Leak-Prüfung), ICS, Schnell-Eintragen, alle Importformate, otpauth- und Migrationslinks, Containerformat, Suite-Datei (ZIP und Kopfdatei), Inhaltsverzeichnis und Inhaltsdateien von GhulDocs, CBOR. Zusage Z3.
- **Lang, regelmäßig:** `cargo-fuzz` (libFuzzer, mit AddressSanitizer), je Parser ein Ziel, Korpus aus echten Dateien; Funde werden als Regressionstest übernommen.

### E5: Chaos und Fehlerinjektion
- Lokale Datei kaputt, Speicher voll, Schreiben bricht nach N Bytes ab, App „stürzt“ mitten im Speichern ab (Fehler nach dem n-ten Schreibvorgang, dann Neustart).
- GhulDocs: Hinzufügen bricht mitten im Inhalt ab (keine halbe Inhaltsdatei, keine Belegung), Inhaltsdatei fehlt oder ist verändert („Dokumente prüfen“, Öffnen der Suite-Datei), Reste nach einem Absturz werden beim Entsperren weggeräumt, Zwischenordner beim Öffnen werden nach Abbruch gelöscht.
- Öffnen einer Suite-Datei bricht beim Ersetzen ab: danach ist der alte oder der neue Stand vollständig da, nie eine Mischung. Speichern auf einen Stick, der fehlt oder mittendrin verschwindet: die bisherige Datei bleibt.
- Uhr springt vor/zurück, Zeitzone wechselt zwischen Geräten.
- Kaputte HTTP-Antworten (falsche Längen, abgebrochene Verbindung, riesige Zeilen) für Update- und Leak-Prüfung, im Fuzzing.

### E6: Modell-Simulation (viele Geräte, viele Tage)
- Alltagstest (vorhanden): 2 Geräte, 30 Tage; die Suite wandert wie auf einem USB-Stick zwischen ihnen.
- „Bekloppter Nutzer“ (vorhanden): zufällige, oft unsinnige Aktionsfolgen auf 3 Geräten, zwischen denen die Suite-Datei wandert, gegen ein einfaches Referenzmodell, mit festen Startwerten (wiederholbar) und Verkleinerung eines Fehlers auf die kürzeste Folge.

### E7: Nebenläufigkeit
Parallele Threads (vorhanden), dazu: Sperren während Hinzufügen/Import/Speichern/Scannen, Entsperren und Sperren gleichzeitig, eine Änderung während die Suite-Datei geschrieben wird (bleibt „nicht gespeichert“).

### E8: Angriffe (Sicherheit)
- **Andere Apps:** Intents mit eigenen privaten Dateien, `file://`, fremde Content-Provider, riesige oder endlose Streams, gefälschte Benachrichtigungs-Aktionen.
- **Rückfall-Angriffe:** alte Suite-Dateien und Tresore einspielen, entfernte Keys, alte Codes (auch mit altem Passwort), geschwächte KDF-Werte.
- **Angriffe auf GhulDocs:** vertauschte, weggelassene, abgeschnittene, veränderte und fremde Abschnitte einer Inhaltsdatei, falsche Füllung und Länge, Inhaltsdatei unter falschem Namen, kaputte Inhaltsverzeichnisse (Kreise, doppelte Kennungen, fehlende Felder), Grenzen (1 GB je Tresor, 3 GB und 20 000 Dokumente für alle GhulDocs-Tresore zusammen, Platz auf dem Datenträger).
- **Angriffe auf die Suite-Datei:** fehlende, vertauschte, alte, fremde und abgezweigte Teile (auch Inhaltsdateien); falsches oder fehlendes Schreibsiegel; Code ohne Passwort (öffnet nur ungeprüft und mit Hinweis); ZIP-Tricks (Ordner, doppelte und zusätzliche Einträge, falsche Größen im Verzeichnis, Zip-Bomben, zu viele Einträge).
- **Geheimnis-Suche:** Marker-Text in allen Eingaben, danach alle geschriebenen Dateien (Arbeitskopie, Suite-Datei, Inhaltsdateien), temporären Dateien und Logs nach dem Marker durchsuchen (Z2).

### E9: Echte Server (entfallen)
Testserver für SFTP, IMAP, SMTP und das PHP-Modul gibt es seit dem Umbau zu GhulDocs nicht mehr. Nachts prüfen nur
noch 21 Server mit kaputtem TLS (badssl.com) und 9.807 Zertifikatsketten (x509-limbo) den TLS-Prüfer der beiden
Verbindungen.

### E10: App und Oberfläche
- **Desktop-UI-Tests** (Compose, Fake-Backends und echter Kern): alle Hauptwege, absurde Eingaben (Naughty Strings in Formularen), schnelles Doppeltippen, Zurück an jeder Stelle; GhulDocs (`DocsInteractionTest`, `DocsBackendTest`) und das Scannen mit gezeichneten Fotos und einer Ersatz-Kamera (`DocScanTest`, `ScanAlgorithmsTest`).
- **Maestro-Abläufe** im Android-Emulator, vorhanden: bei Pull Requests und auf `main` die Grundabläufe aller vier Bereiche (GhulDocs: `07-docs.yaml`) und Speichern/Öffnen der Suite-Datei mit Dokumenten (`14-suite-file.yaml`), in einem Teil. Die Kamera hat im Emulator keinen Ablauf; das Scannen prüfen die Desktop-Tests und der Testplan (Abschnitt 38). Vorab prüft `tools/check-maestro.py` (Precheck und CI, vor dem Emulator), dass jeder Text, den ein Ablauf sucht, noch ein Text der App ist.
- **Android Monkey:** zehntausende zufällige Gesten mit mehreren festen Startwerten; jeder Absturz oder „App reagiert nicht“ ist ein Fehler.
- Barrierefreiheit (Kontraste, Beschriftungen, Zielgrößen).

### E11: Statische Prüfung und Lieferkette
`cargo clippy` (vorhanden), `cargo audit` (vorhanden), `cargo deny` (Lizenzen, Quellen, doppelte Versionen), Android Lint, Semgrep-Regeln (Android, Kotlin), gitleaks (Zugangsdaten im Repo), MobSF-Scan der APK nach OWASP MASVS.

### E12: Wie gut sind die Tests? (Mutationstests)
`cargo-mutants` ändert den Code absichtlich (z. B. `>` zu `>=`) und prüft, ob ein Test das bemerkt. Überlebende Mutanten zeigen Lücken.

### E13: Last
Stresstest (vorhanden), dazu: volle GhulDocs-Tresore (zusammen 3 GB, 20 000 Dokumente) in der Suite-Datei als Datenstrom (bis 3,75 GiB, unter 4 GiB), 100.000 Einträge, Speicherverbrauch beim Scannen vieler Seiten auf schwachen Geräten.

## 3. Fertige Testsammlungen aus dem Netz

| Sammlung | Lizenz | Wofür |
|---|---|---|
| Big List of Naughty Strings | MIT | E3: alle Textfelder |
| Project Wycheproof (C2SP) | Apache-2.0 | E1: Krypto-Bausteine |
| RFC-Testvektoren (6238, 4226, 5869, 9106, 4648) | frei | E1 |
| JSONTestSuite (318 Dateien) | MIT | E4: alle JSON-Importe |
| Kalenderdateien aus den Tests von iCal4j und python-icalendar (304 Dateien, auch Fuzzing-Funde) | BSD | E2/E4: Kalender-Import, Rundreise |
| Zeitzonen-Regeln aller Zonen als VTIMEZONE (tzurl.org, IANA-Datenbank) | gemeinfrei | E1: Zeitzonen-Umrechnung gegen die Datenbank des Rechners |
| badssl.com (21 Server mit kaputtem TLS) | keine (Dienst) | E9 nachts: Zertifikats- und Protokollprüfung der Update- und Leak-Prüfung |
| OWASP MASVS/MASTG | CC BY-SA | Prüfliste Mobil-Sicherheit (nur als Liste, nichts kopiert) |
| libFuzzer / cargo-fuzz | Apache/MIT | E4 lang |
| Android Monkey, Maestro | Apache | E10 |
| Semgrep-Regeln, gitleaks, MobSF, cargo-deny, cargo-mutants | jeweils frei | E11, E12 |

Daten aus Sammlungen liegen unter `core/ghul-core/tests/data/` mit Herkunft und Lizenz in `NOTICE`.

## 4. Wann was läuft

| Wann | Was | Dauer |
|---|---|---|
| Jeder Push | E1 bis E9 Kurzläufe (Fuzz-Kurzlauf, Chaos, Naughty Strings, Wycheproof, Alltagstest, Angriffs-Matrix), clippy, audit | < 15 min zusätzlich |
| Jeder Push (eigener Job) | Stresstest (Release) | ~5 min |
| Nachts (`nightly.yml`) | cargo-fuzz je Ziel 15 min, lange Chaos-/Alltags-/Stresstests, Monkey 30.000 Gesten × 3 mit Intent-Angriffen, Emulator-Abläufe, gitleaks, cargo-audit, x509-limbo, badssl, MobSF, reproduzierbarer Kern | 2 bis 3 h |
| Später (geplant) | Mutationstests (cargo-mutants), Semgrep | |

Jeder gefundene Fehler wird zuerst als Test festgehalten (rot), dann behoben (grün), so kommt er nie zurück.

## 5. Reihenfolge der Umsetzung

1. **Chaos-Sync (E5)**: erledigt: Datenverlust bei kaputten/fehlenden Dateien gefunden und behoben (die Synchronisation ist seit dem Umbau entfernt und mit ihr diese Tests; Abbrüche und kaputte Dateien beim Speichern und Öffnen prüfen jetzt `archive.rs` und `docs.rs`).
2. **Fuzz-Kurzläufe aller Parser (E4)**: Abstürze beheben (Audit A4).
3. **Naughty Strings und Grenzwerte (E3)** durch alle Kern-APIs.
4. **Angriffs-Matrix (E8):** Intents (A6); damals auch Domains mit der Public-Suffix-Liste (Autofill, Mail-Links), Mail-Header und HTML.
5. **Wycheproof und RFC-Vektoren (E1).**
6. **Android Monkey und Desktop-UI-Tests (E10), Dev-APK nicht debuggbar (A5).**
7. **Geplanter Nachtlauf (E4 lang, E11, E12).**
8. Danach: Audit-Punkte B (Schlüsselrotation, damals Autocrypt, Löschbestätigungen, deutsche Fehlermeldungen).
9. **Umbau ohne Synchronisation:** Tests für Speichern und Öffnen der Suite-Datei mit allen Rückfragen und Angriffen (`app/tests/archive.rs`), einzelne Tresore und Weitergeben (`single.rs`), damals GhulDrive und der Schlüsselwechsel (mit dem Umbau zu GhulDocs entfernt).
10. **Umbau zu GhulDocs:** Suite-Datei als Datenstrom mit Inhaltsdateien, Abbrüche und Angriffe (`archive.rs`, `zipstream.rs`), GhulDocs mit Inhaltsdateien, Grenzen und Angriffen (`docs.rs`, `docs/blob.rs`, Testvektoren `docs-blob-v1.json`), Oberfläche und Scannen am Desktop (`DocsInteractionTest`, `DocsBackendTest`, `DocScanTest`, `ScanAlgorithmsTest`).
11. **Mehrere GhulDocs-Tresore, ganzen Tresor weitergeben (20.11.2026):** Tresore für sich, Grenzen je Tresor und je Suite, Paket als eigener Tresor, größte Suite-Datei unter 4 GiB, ganzer Tresor auch für GhulCal (`docs.rs`, `archive.rs`, `single.rs`), am Desktop Tresorwahl, Paket und Weitergeben aus dem Tresor-Menü (`DocsInteractionTest`, `DocsBackendTest`); Liste im Testprotokoll, Abschnitt 11.
