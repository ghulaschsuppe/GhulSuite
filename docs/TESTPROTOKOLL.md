# Testprotokoll und Fehlerbericht

Stand: 13. November 2026 · Branch `test/chaos` (noch nicht gemergt) · Grundlage: Audit vom selben Tag (`docs/AUDIT-2026-11.md`) und Testkonzept (`docs/TESTKONZEPT.md`).

Dieses Protokoll ist die Arbeitsliste für die nächsten Durchgänge: Jeder Fund hat eine Nummer, eine Schwere, den Test, der ihn zeigt, und – wenn behoben – den Commit. Offene Punkte haben einen Vorschlag, wie sie behoben werden, und den Test, der danach grün werden muss.

## 1. Ergebnis in Zahlen

| | |
|---|---|
| Testfunktionen im Kern | 195 (192 bei jedem Lauf, 3 lange nur nachts) |
| Einzelprüfungen pro Lauf | über 380.000 (u. a. 292.000 Kalendertage, 45.000 Fuzz-Eingaben, 20.000 Base32-Schreibweisen, 12.000 Wiederherstellungscodes, 5.000 HOTP-Fälle, 5.000 Passwortgenerator-Varianten, 3.000 Zusammenführungen, 3.000+ Aktionen „bekloppter Nutzer“, 515 Naughty Strings × 8 Felder) |
| Davon gegen echte Server | 54 (SFTP, FTPS, IMAP, SMTP, PHP-Modul) |
| Externe Testsammlungen | Big List of Naughty Strings, Project Wycheproof (5 Algorithmen), Public-Suffix-Liste mit offiziellen Testfällen, RFC 4226/6238, PayloadsAllTheThings (XSS, CRLF), SecLists (XSS), DOMPurify-Testfälle, WHATWG-URL-Testdaten, JSONTestSuite |
| Gefundene Fehler | 26, davon **23 behoben**, 2 bewusst als Entscheidung offen, 1 als Einzelheit (B12) |
| Offene Punkte aus dem Audit | 19 (Abschnitt 4) + Bedienung/Texte (Abschnitt 5) |
| Laufzeit | Kern-Tests ca. 95 s, Android/Desktop-Build ca. 10 min, Nachtlauf ca. 2 h |

Alle Tests sind grün (Kern, Netzwerk, Android-Kotlin, Desktop-Tests). Die App-Tests im Emulator (Maestro, Monkey) laufen in der CI.

## 2. Testsuite – was es gibt und wie man es startet

| Datei | Was | Zusage* |
|---|---|---|
| `core/ghul-core/src/app/tests/chaos.rs` | Server fällt aus, Uploads reißen ab, Dateien fehlen oder sind kaputt, neuere App-Version, leerer/falscher Ordner | Z1, Z5 |
| `…/tests/crazy_user.rs` | „Bekloppter Nutzer“: tausende zufällige, oft unsinnige Aktionen auf drei Geräten; danach nichts verloren, nichts Gelöschtes zurück, alle Geräte gleich, nur verständliche Fehler | Z1, Z3, Z5, Z6 |
| `…/tests/absurd.rs` | Naughty Strings in jedem Feld, riesige/leere Werte, seltsame Master-Passwörter, unmögliche Termine, kaputte Drive-Namen, Aufrufe im gesperrten Zustand | Z3, Z6, Z8 |
| `…/tests/attacks.rs` | Datenklau (Klartextsuche in allen Dateien), gestohlene Dateien, manipulierter Drive-Server, gefälschte/geschwächte Tresordateien, Sicherungen mit Pfad-Tricks und Zip-Bomben, Mail-Kopfzeilen-Injektion, 34 HTML-Mail-Tricks, CSV-Formeln | Z2, Z3, Z4, Z7 |
| `…/tests/properties.rs` | Generierte Reihen: Kalender 1600–2400, RFC-Vektoren, Base32, Wiederherstellungscodes, Zusammenführen (Reihenfolge, Wiederholung), Passwortgenerator, Lookalike-Domains, ICS-Rundreise | Z5, Z8 |
| `…/tests/everyday.rs` | Alltagstest (2 Geräte, 30 Tage) und Stresstests (5.000 Einträge, 64 MB, 8 Threads) | Z1, Z5 |
| `core/ghul-core/src/fuzz_tests.rs` | Schnelles Fuzzing aller Parser bei jedem Lauf | Z3 |
| `…/tests/collections.rs` | Öffentliche Angriffssammlungen: ~500 XSS-Muster (in 3 Schreibweisen), CRLF-Muster in jedem Mail-Kopffeld, WHATWG-URL-Testdaten, JSONTestSuite durch alle JSON-Importe | Z3, Z4 |
| `core/ghul-core/src/wycheproof_tests.rs` | Wycheproof-Vektoren für XChaCha20-Poly1305, AES-GCM, HKDF, HMAC, P-256 | Z2 |
| `core/ghul-core/src/app/autofill.rs` (Tests) | Public-Suffix-Testfälle, Phishing-Matrix | Z4 |
| `core/fuzz/` | libFuzzer-Ziele `parsers`, `container_parse`, `ctap_responses` (nachts 15 min je Ziel) | Z3 |
| `tools/check-texts.py` | Texte: fehlende Übersetzungen und Platzhalter (Fehler), uneinheitliche Begriffe und ungenutzte Texte (Hinweise) | Z6 |
| `tools/ci-emulator.sh` | Maestro-Abläufe; nachts zusätzlich Monkey (3 × 30.000 zufällige Gesten) | Z3, Z6 |
| `.github/workflows/nightly.yml` | Nachtlauf: Fuzzing lang, Langläufe in Release, Monkey, gitleaks (ganze Historie), PHPStan, cargo audit | alle |

\* Zusagen Z1–Z8 siehe `docs/TESTKONZEPT.md`, Abschnitt 1.

Lokal starten:

```
cd core
cargo test -p ghul-core                                   # alles außer den Langläufen
cargo test --release -p ghul-core -- --ignored stress     # Stresstests
cargo test --release -p ghul-core -- --ignored crazy      # bekloppter Nutzer, lang
sudo -E bash ../tools/test-servers.sh > servers.env && set -a && . ./servers.env && set +a
cargo test -p ghul-core -- sftp ftps mail sync drive share php   # gegen echte Testserver
python3 ../tools/check-texts.py                            # Texte
```

## 3. Gefundene Fehler

Schwere: **K** kritisch (Datenverlust, Geheimnis preisgegeben) · **H** hoch (Angriff möglich, Absturz von außen) · **M** mittel · **N** niedrig.

| Nr. | Schwere | Fund | Gezeigt durch | Status |
|---|---|---|---|---|
| F01 | K | Eine abgeschnittene oder fehlende Server-Datei löschte den Tresor auf **allen** Geräten | `chaos::a_damaged_server_file_never_deletes_a_vault`, `…a_missing_server_file…` | behoben `bfd99b2` |
| F02 | K | Ein leerer oder vertippter Server-Ordner löschte lokale Tresore | `chaos::an_empty_or_wrong_server_folder_deletes_nothing` | behoben `bfd99b2` |
| F03 | K | Eine kaputte lokale Datei löschte die gute Kopie auf dem Server | `chaos::a_damaged_local_file_does_not_delete_the_server_copy` | behoben `bfd99b2` |
| F04 | K | Eine Datei einer neueren App-Version wurde überschrieben bzw. führte zum Löschen | `chaos::a_file_from_a_newer_app_is_left_alone` | behoben `bfd99b2` |
| F05 | H | Eine halb hochgeladene Suite-Datei blockierte jede weitere Synchronisation | `chaos::flaky_network_and_torn_uploads_lose_nothing` | behoben `bfd99b2` |
| F06 | M | FTPS löschte die alte Datei vor dem Umbenennen (Lücke ohne Datei) | Code-Prüfung, Chaos-Test | behoben `bfd99b2` |
| F07 | H | HTTP-Antwort „chunked“ ohne Abschluss oder mit Riesenlänge → Absturz (WKD, Mail-Bilder, PHP-Modul) | `fuzz_tests::http_responses` | behoben `21c4ff9` |
| F08 | H | SMTP-Zeile mit Mehrbyte-Zeichen → Absturz; vor STARTTLS von jedem im Netz auslösbar | `fuzz_tests::smtp_reply_lines` | behoben `21c4ff9` |
| F09 | N | FTP-PASV mit Zahlen über 255 → Überlauf | `fuzz_tests::ftp_replies_and_listings` | behoben `21c4ff9` |
| F10 | N | Endlose SMTP-Antwort füllte den Speicher | Code-Prüfung | behoben `21c4ff9` |
| F11 | M | Drive-Namen aus nur unsichtbaren Zeichen (leer aussehend), mit Steuerzeichen oder Richtungszeichen, die den Dateityp tarnen („rechnung‮fdp.exe“) | `absurd::naughty_strings_come_back_exactly_everywhere` | behoben `f01361f` |
| F12 | N | Termintitel behielten unsichtbare Zeichen an den Enden | dto. | behoben `f01361f` |
| F13 | H | Zeilenumbrüche in Betreff, Antwort-Bezug, Anhangname/-typ konnten Mail-Kopfzeilen einschleusen (z. B. heimliches Bcc) | `attacks::mail_headers_cannot_be_injected` | behoben `471710c` |
| F14 | H | Sicherung mit Laufwerks-/Stream-Namen; Zip-Bombe wurde ohne Grenze entpackt (angegebene Größe vertraut) | `attacks::backups_with_path_tricks_and_bombs_write_nothing_outside` | behoben `471710c` |
| F15 | M | CSV-Export: Zellen mit `= + - @` liefen in Tabellenprogrammen als Formel (CSV-Injection); eigener Import schnitt Leerzeichen in Passwörtern ab | `attacks::exports_are_safe_to_open_in_a_spreadsheet` | behoben `471710c` |
| F16 | M | HTML-Mail-Link zeigt „https://bank.example“, führt aber woanders hin | `attacks::html_mails_cannot_run_code_or_hide_links` | behoben `471710c` (echtes Ziel wird angezeigt) |
| F17 | H | Autocrypt: gefälschter Absender ersetzt einen gelernten PGP-Schlüssel; eigene Adresse lernbar; Signatur mit dem Schlüssel aus derselben Mail galt als „gültig“ | Code-Prüfung, Netzwerktest `pgp_mail_between_two_users` | behoben `471710c` |
| F18 | H | Autofill: Basis-Domain zu grob – `evil.github.io` bekam Login von `alice.github.io`, fremde IPs den Router-Login | `autofill::phishing_matrix`, `public_suffix_list_test_cases`, `properties::autofill_never_fills_a_lookalike` | behoben `7c0c49f` |
| F19 | H | Autofill glaubte jeder App ihre Webadresse (eigene WebView, gefälschter Browser) | Code-Prüfung (App) | behoben `7c0c49f` |
| F20 | H | „Per E-Mail senden“ las Anhänge mit GhulSuites Rechten – fremde App konnte private Dateien/entschlüsselte Drive-Kopien in einen Entwurf schmuggeln | Code-Prüfung (App) | behoben `7c0c49f` |
| F21 | H | Verteilte Dev-APK war debuggbar und mit dem echten Schlüssel signiert | Code-Prüfung | behoben `7c0c49f` |
| F22 | N | Kopiertes Passwort blieb nach dem Sperren in der Zwischenablage | Code-Prüfung | behoben `7c0c49f` |
| F25 | H | Adressen mit Backslash las GhulSuite anders als ein Browser: aus `https://evil.com\@bank.de` wurde `bank.de`, der Browser geht zu `evil.com` | `collections::urls_belong_to_the_same_site_as_in_a_browser` (WHATWG-Testdaten) | behoben (Branch `test/chaos`) |
| F26 | N | IPv4 in anderen Schreibweisen (`0xC0A80001`, `192.168.1`) las GhulSuite anders als ein Browser | dto. | behoben (Branch `test/chaos`) |
| F23 | M | **Tresornamen stehen im Dateinamen** (Server, Sicherung): „bank-privat.ghlpss“ verrät, welche Tresore es gibt | `attacks::stolen_device_files…` (bewusst ausgenommen) | **offen – Entscheidung nötig** |
| F24 | N | Termintitel und Import-Titel werden getrimmt (führender Tab geht verloren); Passwörter bleiben exakt | `attacks::exports_are_safe…` | **offen – vertretbar** |

Zu F23: Vorschlag – Dateien künftig nach ihrer Kennung benennen (`3f9a….ghlpss`), den Namen nur verschlüsselt führen; alte Dateinamen beim nächsten Sync umbenennen. Nachteil: Im Ordner sieht man nicht mehr, welche Datei welcher Tresor ist. Test danach: Marker im Tresornamen darf auch in Datei- und ZIP-Namen nicht vorkommen.

Zu F17 (Verhalten seit der Behebung): Die erste Mail eines neuen Kontakts mit Autocrypt-Schlüssel zeigt „Schlüssel unbekannt/unbestätigt“; ab der nächsten Mail „gültig“ (Vertrauen beim ersten Kontakt). Öffnet man dieselbe erste Mail erneut, zeigt sie „gültig“ – das ist eine kleine Unstimmigkeit (siehe B12).

## 4. Offene Punkte (Audit und Tests)

### Sicherheit und Datenhaltung

| Nr. | Schwere | Punkt | Vorschlag | Test, der grün werden muss |
|---|---|---|---|---|
| B1 | H | Neuer Wiederherstellungscode / entfernter Key schützt nicht gegen **alte Dateikopien**: der innere Schlüssel wird nicht erneuert | Beim Code-Wechsel und Key-Entfernen VMK und Faktor-Geheimnis neu erzeugen, alles neu verschlüsseln (Formatänderung, mit Umzug) | neu: altes Notfallblatt + alte Kopie öffnet aktuelle Datei **nicht** |
| B2 | M | Entfernte Keys können per Sync zurückkommen (Gerät war offline und fügt selbst einen Key hinzu) | Löschmarken je Key in der Suite-Datei | neu in `chaos.rs` |
| B3 | M | Drive-Index ohne Schutz gegen Zurücksetzen (alter Index vom Server) und gleichzeitige Änderungen zweier Geräte | Revision im Drive-Bereich merken, älteren Index ablehnen; vor dem Schreiben vergleichen | neu: Index-Rollback wird erkannt |
| B4 | M | Sperrbildschirm zeigt Absender/Betreff/Termin, wenn Android „alle Inhalte“ zeigt | Kanäle mit `lockscreenVisibility = PRIVATE` (neue Kanal-IDs) | Emulator-Ablauf |
| B5 | M | Mail-Zugang für Benachrichtigungen: Keystore ohne StrongBox; Doku (KONZEPT) beschreibt noch „opt-in, Panik löscht“ | Doku angleichen, StrongBox versuchen, optional „harte Panik“ | – |
| B6 | N | `.ics` aus fremden Apps wird ohne Rückfrage importiert | Vorschau mit Bestätigung | Emulator-Ablauf |
| B7 | N | Interne Aktionen (`OPEN_MAIL`, `OPEN_EVENT`) über die exportierte MainActivity von fremden Apps auslösbar | Nicht exportierte Ziel-Activity oder Zufallskennung im PendingIntent | – |
| B8 | N | Auto-Sperre: Timer läuft im Tiefschlaf nicht weiter; nach Teilen/Kamera gilt dauerhaft eine Mindestfrist von 120 s | Zeit beim Verlassen merken und beim Zurückkommen vergleichen; Frist nach Rückkehr zurücksetzen | – |
| B9 | N | Autofill verknüpft eine App nach einmaliger Auswahl dauerhaft, ohne Rückfrage | Bestätigung mit App-Name und Paket | – |
| B10 | N | Windows: entschlüsselte Kopien in `%TEMP%` bleiben liegen, wenn ein Programm sie noch offen hat | Löschergebnis prüfen, Hinweis; Ordner im Profil mit Zufallsnamen | – |
| B11 | N | „Passwort anderswo geändert“: Server kann teure Argon2-Werte erzwingen (bis 2 GiB) | Werte nicht über die eigenen hinaus annehmen oder vorher fragen | `attacks::forged_or_weakened…` erweitern |
| B12 | N | Erste Autocrypt-Mail: beim zweiten Öffnen „gültig“ statt „unbestätigt“ | Herkunft des Schlüssels (Mail-ID) merken | Netzwerktest erweitern |
| B13 | N | Geänderte Server-/Kontodaten in der Suite-Datei werden nicht an andere Geräte weitergegeben (gleiche ID gewinnt lokal) | Zeitstempel je Listeneintrag wie bei Bereichs-Optionen | neu in `chaos.rs` |
| B14 | N | Freigabe zurückziehen meldet Erfolg, auch wenn das Paket auf dem Server bleibt | Paket immer löschen, Fehler melden | – |
| B15 | N | PHP-Modul: Fehlversuchssperre per IP (hinter Proxy aussperrbar), Endungsliste erfasst `x.php.jpg` nicht, Modul-Datei ohne `chmod` | Sperre an Schlüssel statt IP koppeln; Allowlist der Endungen; `chmod 0600` | PHP-Test |
| B16 | N | Mail-Bilder folgen Weiterleitungen auch zu `localhost`/privaten Adressen | Private Adressen nach Auflösung ablehnen | Fuzz/Unit-Test |
| B17 | N | CI: Signaturschlüssel und FTP-Zugang stehen bei Pushes auf jeden Branch bereit | Nur `main` (GitHub-Environment mit Freigabe) | – |
| B18 | N | Zusammenführen bei gleichzeitigem Löschen (Gerät A) und Bearbeiten (Gerät B) desselben Eintrags: Ergebnis hängt von Revision/Zeit ab | Regel festlegen („Bearbeiten gewinnt, Löschung wird gemeldet“) und testen | neu in `crazy_user.rs` |
| B19 | N | Test-Hilfe: Nach Sperren/Entsperren muss Drive im Test neu verbunden werden (`NotConfigured`); in der App verbindet sie sich selbst | prüfen, ob die App nach Entsperren immer neu verbindet | Desktop-UI-Test |

### Bedienung (Fehler, die Nutzer machen)

| Nr. | Punkt | Vorschlag |
|---|---|---|
| U1 | Mail löschen ohne Rückfrage/Rückgängig; Wischen „Archivieren“ löscht still ohne Archivordner | „Rückgängig“ in der Meldungsleiste; ohne Archivordner nichts tun und sagen |
| U2 | Einzelnen Termin löschen ohne Rückgängig | „Rückgängig“ |
| U3 | Drive „Endgültig löschen“ ohne Rückfrage; Löschtext sagt „vom Server gelöscht“, obwohl Papierkorb | Rückfrage; Text „kommt in den Papierkorb (n Tage)“ |
| U4 | Bereich/Tresor löschen: ein Tipp vernichtet alles auf allen Geräten | Anzahl nennen, Export anbieten, Namen abtippen |
| U5 | ~80 englische Fehlermeldungen aus dem Kern im deutschen UI | Fehlercodes statt Text, je Fall deutsche Meldung mit nächstem Schritt |
| U6 | „Sicherung wiederherstellen“ bei vorhandener Suite: technische Meldung, keine Ordnerwahl | Ordnerwahl anbieten |
| U7 | „Von meinem Server holen“ nur SFTP | FTPS und PHP-Modul ergänzen |
| U8 | Windows-Assistent bietet App-PIN an, die dort nicht geht | Schritt auf Windows auslassen |
| U9 | Assistent Schritt 2: Key ohne PIN → „Weiter“ tut nichts | Meldung |
| U10 | Einzelne Tresordatei öffnen: Kalenderdatei wird als GhulAuth geöffnet | Art aus der Datei lesen |
| U11 | Verlorene Schlüsseldatei: nach Wiederherstellung kein Hinweis, alte entfernen/neue anlegen | Geführter Notfall |
| U12 | Formulare verwerfen Ungespeichertes ohne Nachfrage (Zurück, Auto-Sperre) | Entwurf merken oder fragen |
| U13 | Synchronisation scheitert offline still, kein Status | Status sichtbar (Konzept §10.8) |
| U14 | Drive-Speicher „Wie Synchronisation“ umschalten wirkt sofort, Drive scheinbar leer | Rückfrage mit Erklärung |
| U15 | Kalender-Import ohne Standardkalender tut nichts; Export nimmt still nur sichtbare Kalender | Zielkalender fragen; Hinweis |

### Uneinheitliche Bedienung

| Nr. | Punkt |
|---|---|
| K1 | Listen: GhulPass (⋮ + langes Drücken), GhulAuth (Wischen, ⋮ nur bei einer Auswahl), GhulMail (Wischen, keine Mehrfachauswahl), GhulDrive (Auswahlleiste unten statt oben) – auf ein Muster bringen |
| K2 | Löschschutz: Pass/Auth/Drive-Ordner fragen, Mail/Termin/„Endgültig“ nicht; Papierkorb nur bei Drive und Mail |
| K3 | Assistent und Optionen bieten verschiedene Werte (Auto-Sperre ohne „2 Minuten“, Schnell-Entsperren ohne „1 Std./8 Std.“) |
| K4 | Kalender-Blatt „Fertig“, sonst „Speichern“ |
| K5 | `set_start_first` „1. Bereich“, aber „Fünfter Bereich“ |
| K6 | Drive-Kontextblatt führt zu „Server, Sync & Teilen“, obwohl der Drive-Speicher unter Optionen → GhulDrive eingestellt wird |
| K7 | Hinweise für Windows erscheinen auf Android und umgekehrt („Unter Windows …“, „Oder Handy schütteln“ auch bei ausgeschaltetem Schütteln) |
| K8 | Hinweis „Favorit: Konto lange drücken“ falsch (langes Drücken wählt aus) |

## 5. Texte und Begriffe

`tools/check-texts.py` findet aktuell **0 Fehler** (alle Texte in beiden Sprachen, Platzhalter gleich) und **121 Hinweise**: 73 Stellen mit uneinheitlichen Begriffen, 48 nicht mehr verwendete Texte. Die wichtigsten Gruppen (vollständige Liste: Skript ausführen):

| Gruppe | Beispiel | Einheitlich |
|---|---|---|
| Zweiter Faktor unvollständig | „Passwort + Key“ (u. a. `prot_pw_key`, `backup_hint`, `set_vaults_hint`, `set_quick_*`, `wz_quick_*`) | „Master-Passwort + Key oder Schlüsseldatei“ |
| Falsch | `set_panic_hint` „danach öffnet nur das Master-Passwort“; „1-MiB-Blöcke“ (4 MiB); NFC-Versprechen; Drive „vom Server gelöscht“ | korrigieren |
| Gerät | „Security Key“, „Key“, „FIDO-Security-Key“, „Ersatzkey“ | „Key (Sicherheitsschlüssel)“, „Key-PIN“ |
| Reiter | „Einstellungen“ neben „Optionen“; falsche Pfadangaben | „Optionen“, echte Pfade |
| Schnell-Entsperren | „Schnellzugang“, „Schnell entsperren“ | „Schnell-Entsperren“ |
| Englisch | „Login“, „Screenshot“, „Label“, „Sync“ | „Anmeldung“, „Bildschirmfoto“, „Schlagwort“, „Synchronisation“ |
| Löschen/Entfernen | Dialog „entfernen?“, Knopf „Löschen“ | „Entfernen“ nur, wenn Daten bleiben |

Sobald die Begriffe bereinigt sind: `tools/check-texts.py --strict` in der CI einschalten, dann kommen neue Unstimmigkeiten nicht mehr durch.

## 6. Was geprüft wurde und in Ordnung ist

- Kein Klartext auf Gerät, Server, in Sicherung oder Export (Marker in allen Bereichen, auch als UTF-16, Hex, Base64 gesucht).
- Gestohlene Dateien öffnen sich nur mit Master-Passwort **und** zweitem Faktor; eine um ein Bit veränderte Schlüsseldatei wird abgelehnt.
- Manipulierter Drive-Server: gekippte Bits, vertauschte Blöcke, gefälschter Index, gelöschte Blöcke → immer Fehlermeldung, nie falsche Daten.
- Gefälschte KDF-Werte (zu schwach, außerhalb des Bereichs, maximal) richten nichts an und kosten keine Zeit; Dateien einer fremden Suite werden ignoriert.
- Krypto-Bausteine bestehen alle Wycheproof-Fälle (gefälschte Tags, ungültige Kurvenpunkte usw.).
- 34 HTML-Angriffsmuster: keine Skripte, Ereignisse, `javascript:`/`data:`/`vbscript:`-Links, Formulare, Frames.
- Alle Parser überstehen Zufallsdaten und Mutationen ohne Absturz.
- Zusammenführen ist unabhängig von Reihenfolge und Wiederholung; nichts geht verloren.
- Kalender: jeder Tag von 1600 bis 2400 stimmt (Wochentag, Schaltjahr); unmögliche Daten werden abgelehnt oder bereinigt, ohne Absturz oder Hänger.
- Bekloppter Nutzer (3 Geräte, 3.000+ Aktionen, auch Sperren, Neustarts, Müll-Importe, falsche Ordner): kein Absturz, nur verständliche Ablehnungen, nichts verloren, nichts Gelöschtes zurück.

## 7. Empfohlene Reihenfolge

1. **Diesen Branch mergen** (alle Funde F01–F22 behoben, Nachtlauf aktiv).
2. **F23** entscheiden (Tresornamen in Dateinamen) – betrifft das Format, besser früh.
3. **B1** (Schlüssel bei Code-Wechsel erneuern) – größter Sicherheitspunkt, Formatänderung.
4. **Texte (Abschnitt 5)** in einem Durchgang, danach `--strict` einschalten.
5. **U1–U5** (Rückgängig, Rückfragen, deutsche Fehlermeldungen).
6. Rest von B und U/K nach Aufwand.

Nach jedem Durchgang: dieses Protokoll aktualisieren (Status, Commit), neue Funde mit nächster Nummer anhängen.
