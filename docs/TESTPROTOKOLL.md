# Testprotokoll und Fehlerbericht

Stand: 13. November 2026 (dritte Runde: Kalender, Zeitzonen, Emulator-Abläufe für alle Bereiche) · Branch `test/chaos` (noch nicht gemergt) · Grundlage: Audit vom selben Tag (`docs/AUDIT-2026-11.md`) und Testkonzept (`docs/TESTKONZEPT.md`).

Dieses Protokoll ist die Arbeitsliste für die nächsten Durchgänge: Jeder Fund hat eine Nummer, eine Schwere, den Test, der ihn zeigt, und, wenn behoben, den Commit. Offene Punkte haben einen Vorschlag, wie sie behoben werden, und den Test, der danach grün werden muss.

## 1. Ergebnis in Zahlen

| | |
|---|---|
| Testfunktionen im Kern | 219 (215 bei jedem Lauf, 4 lange nur nachts) |
| Einzelprüfungen pro Lauf | über 1,1 Millionen (u. a. 777.000 Zeitzonen-Umrechnungen in 446 Zonen, 292.000 Kalendertage, 45.000 Fuzz-Eingaben, 20.000 Base32-Schreibweisen, 12.000 Wiederherstellungscodes, 5.000 HOTP-Fälle, 5.000 Passwortgenerator-Varianten, 3.000 Zusammenführungen, 3.000+ Aktionen „bekloppter Nutzer“, 515 Naughty Strings × 8 Felder) |
| Davon gegen echte Server | 54 (SFTP, FTPS, IMAP, SMTP, PHP-Modul) |
| Externe Testsammlungen | Big List of Naughty Strings, Project Wycheproof (5 Algorithmen), Public-Suffix-Liste mit offiziellen Testfällen, RFC 4226/6238, PayloadsAllTheThings (XSS, CRLF), SecLists (XSS), DOMPurify-Testfälle, WHATWG-URL-Testdaten, JSONTestSuite, 304 Kalenderdateien aus den Tests von iCal4j und python-icalendar (auch deren Fuzzing-Funde), Zeitzonen-Regeln aller Zonen der IANA-Datenbank |
| Gefundene Fehler | 39, davon **36 behoben**, 2 bewusst als Entscheidung offen, 1 als Einzelheit (B12) |
| Offene Punkte aus dem Audit | 22 (Abschnitt 4) + Bedienung/Texte (Abschnitt 5) |
| Laufzeit | Kern-Tests ca. 95 s, Android/Desktop-Build ca. 10 min, Nachtlauf ca. 2 h |

Alle Tests sind grün (Kern, Netzwerk, Android-Kotlin, Desktop-Tests). Die App-Tests im Emulator (Maestro, Monkey) laufen in der CI.

## 2. Testsuite: was es gibt und wie man es startet

| Datei | Was | Zusage* |
|---|---|---|
| `core/ghul-core/src/app/tests/chaos.rs` | Server fällt aus, Uploads reißen ab, Dateien fehlen oder sind kaputt, neuere App-Version, leerer/falscher Ordner | Z1, Z5 |
| `…/tests/crazy_user.rs` | „Bekloppter Nutzer“: tausende zufällige, oft unsinnige Aktionen auf drei Geräten; danach nichts verloren, nichts Gelöschtes zurück, alle Geräte gleich, nur verständliche Fehler | Z1, Z3, Z5, Z6 |
| `…/tests/absurd.rs` | Naughty Strings in jedem Feld, riesige/leere Werte, seltsame Master-Passwörter, unmögliche Termine, kaputte Drive-Namen, Aufrufe im gesperrten Zustand | Z3, Z6, Z8 |
| `…/tests/attacks.rs` | Datenklau (Klartextsuche in allen Dateien), gestohlene Dateien, manipulierter Drive-Server, gefälschte/geschwächte Tresordateien, Sicherungen mit Pfad-Tricks und Zip-Bomben, Mail-Kopfzeilen-Injektion, 34 HTML-Mail-Tricks, CSV-Formeln | Z2, Z3, Z4, Z7 |
| `…/tests/properties.rs` | Generierte Reihen: Kalender 1600 bis 2400, RFC-Vektoren, Base32, Wiederherstellungscodes, Zusammenführen (Reihenfolge, Wiederholung), Passwortgenerator, Lookalike-Domains, ICS-Rundreise | Z5, Z8 |
| `…/tests/everyday.rs` | Alltagstest (2 Geräte, 30 Tage) und Stresstests (5.000 Einträge, 64 MB, 8 Threads) | Z1, Z5 |
| `core/ghul-core/src/fuzz_tests.rs` | Schnelles Fuzzing aller Parser bei jedem Lauf | Z3 |
| `…/tests/collections.rs` | Öffentliche Angriffssammlungen: ~500 XSS-Muster (in 3 Schreibweisen), CRLF-Muster in jedem Mail-Kopffeld, WHATWG-URL-Testdaten, JSONTestSuite durch alle JSON-Importe | Z3, Z4 |
| `…/tests/collections.rs` (Kalender) | 304 Kalenderdateien anderer Programme (echte Exporte, kaputte Dateien, Fuzzing-Funde): was der Import annimmt, ist ein gültiger Termin und übersteht Export → Import unverändert; alle Dateien zusätzlich durch einen echten Kalender (anzeigen, Erinnerungen, exportieren, sperren) | Z3, Z8 |
| `core/ghul-core/src/app/caltext.rs` `zone_tests` | Zeitzonen: UTC-Zeiten mit dem Versatz ihres Tages, Termine aus anderen Zonen, Zonen, die nur die Datei kennt (Outlook), Serien-Ausnahmen; dazu die Regeln **aller 446 Zeitzonen** gegen die Zeitzonendatenbank, 2000 bis 2023 (Vergleichswerte: `tools/make-zone-oracle.py`) | Z8 |
| `app/…/desktopTest/VaultFfiTest.kt` | Kalender-Import über die echte Anbindung mit der Zeitzonendatenbank des Geräts | Z8 |
| `core/ghul-core/src/wycheproof_tests.rs` | Wycheproof-Vektoren für XChaCha20-Poly1305, AES-GCM, HKDF, HMAC, P-256 | Z2 |
| `core/ghul-core/src/app/autofill.rs` (Tests) | Public-Suffix-Testfälle, Phishing-Matrix | Z4 |
| `core/fuzz/` | libFuzzer-Ziele `parsers`, `container_parse`, `ctap_responses` (nachts 15 min je Ziel) | Z3 |
| `tools/attack-php-module.py` | 335 Angriffe auf das PHP-Modul: gefälschte, alte und wiederholte Anfragen, geänderte Inhalte und Parameter, Pfade aus dem Datenordner, eingeschleuste Skripte, falsche Methoden, Freigabe-Tricks, Uploads außer der Reihe, Sperre nach Fehlversuchen | Z2, Z4 |
| `tools/check-android-manifest.py` | Von außen erreichbare Komponenten und Berechtigungen nur wie in einer festen Liste; verbotene Berechtigungen | Z7 |
| `net/tests.rs` `badssl_…` (nachts) | 21 echte Server mit kaputtem TLS (abgelaufen, falscher Name, selbst signiert, alte Protokolle, schwache Verfahren, bekannte Schad-Zertifikate) müssen abgelehnt werden | Z4 |
| `tools/check-texts.py` | Texte: fehlende Übersetzungen und Platzhalter (Fehler), uneinheitliche Begriffe und ungenutzte Texte (Hinweise) | Z6 |
| `tools/ci-emulator.sh` | Maestro-Abläufe (Einrichtung, GhulPass, GhulAuth, GhulCal, App-PIN, Optionen, GhulDrive und GhulMail mit nicht erreichbarem Server); nachts zusätzlich Monkey (3 × 30.000 zufällige Gesten) | Z3, Z6 |
| `app/…/desktopTest/InteractionTest.kt` | Bedienung am Desktop mit Compose UI Test: langes Drücken, Auswahlkopf, Löschen mit „Rückgängig“ in allen fünf Bereichen (der Aufruf erreicht den Speicher nur ohne Rückgängig), Rückfragen mit rotem Knopf, Aktionsblätter, zweite Spalte im breiten Fenster, verschlüsselte Mail in der Liste; speichert je ein Bild | Z1, Z6 |
| `app/maestro/server/` + `tools/check-server-flows.sh` | Im Emulator mit echten Testservern daneben: GhulDrive auf SFTP mit Passwort einrichten und einen Ordner anlegen, ein Mailkonto mit selbst signiertem Zertifikat anlegen, sich selbst eine Mail schicken und lesen; danach prüfen, dass auf den Servern nur Verschlüsseltes liegt | Z2, Z4 |
| `tools/check-reminders.sh` | Im Emulator nach den Abläufen: Der Termin hat einen Alarm im System; nach Zeitzonen-Wechsel steht er auf derselben Uhrzeit; nach „App beendet + Neustart“ ist er wieder da; beim Auslösen kommt die Benachrichtigung, bei beendeter, gesperrter App | Z1, Z8 |
| `.github/workflows/nightly.yml` | Nachtlauf: Fuzzing lang, Langläufe in Release, Monkey, gitleaks (ganze Historie), PHPStan, cargo audit | alle |

\* Zusagen Z1 bis Z8 siehe `docs/TESTKONZEPT.md`, Abschnitt 1.

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
| F18 | H | Autofill: Basis-Domain zu grob, `evil.github.io` bekam Login von `alice.github.io`, fremde IPs den Router-Login | `autofill::phishing_matrix`, `public_suffix_list_test_cases`, `properties::autofill_never_fills_a_lookalike` | behoben `7c0c49f` |
| F19 | H | Autofill glaubte jeder App ihre Webadresse (eigene WebView, gefälschter Browser) | Code-Prüfung (App) | behoben `7c0c49f` |
| F20 | H | „Per E-Mail senden“ las Anhänge mit GhulSuites Rechten, fremde App konnte private Dateien/entschlüsselte Drive-Kopien in einen Entwurf schmuggeln | Code-Prüfung (App) | behoben `7c0c49f` |
| F21 | H | Verteilte Dev-APK war debuggbar und mit dem echten Schlüssel signiert | Code-Prüfung | behoben `7c0c49f` |
| F22 | N | Kopiertes Passwort blieb nach dem Sperren in der Zwischenablage | Code-Prüfung | behoben `7c0c49f` |
| F25 | H | Adressen mit Backslash las GhulSuite anders als ein Browser: aus `https://evil.com\@bank.de` wurde `bank.de`, der Browser geht zu `evil.com` | `collections::urls_belong_to_the_same_site_as_in_a_browser` (WHATWG-Testdaten) | behoben (Branch `test/chaos`) |
| F26 | N | IPv4 in anderen Schreibweisen (`0xC0A80001`, `192.168.1`) las GhulSuite anders als ein Browser | dto. | behoben (Branch `test/chaos`) |
| F27 | M | Dateinamen, die ein Server oder eine Sicherung liefert, durften Windows-Gerätenamen (`CON`, `AUX`), Doppelpunkte (Laufwerk/Datenstrom), Steuer- und Richtungszeichen enthalten | `attacks::a_server_cannot_plant_files_through_names` | behoben (zweite Audit-Runde) |
| F28 | M | Bilder in Mails konnten das eigene Gerät (`http://localhost`), den Router und andere private Adressen ansprechen, auch über eine Weiterleitung | `mailhtml::mail_images_cannot_reach_private_addresses` | behoben: nur https zu öffentlichen Adressen (erledigt B16) |
| F29 | M | Debug-Ausgaben von 13 Datenstrukturen enthielten Passwörter, Codes und Schlüssel (Gefahr: Logzeile, Absturzbericht) | `attacks::secrets_never_show_up_in_debug_output_or_errors` | behoben: Geheimnisse erscheinen nie in Debug-Ausgaben |
| F30 | N | PHP-Modul: Namen mit Punkt/Leerzeichen am Ende (`x.php.` wird auf Windows-Servern zu `x.php`); Freigabe- und Viewer-Antworten ohne `nosniff`/`no-store` | `tools/attack-php-module.py` (335 Angriffe) | behoben (erledigt B15 teilweise) |
| F31 | M | App-PIN: der Fehlversuch wurde erst nach dem Versuch und verzögert gespeichert, App direkt nach falscher PIN abschießen gab einen Freiversuch | Code-Prüfung | behoben: erst zählen (sofort auf Platte), dann prüfen |
| F32 | M | Kalender-Import: Zeiten in UTC (so exportiert Google einzelne Termine) wurden mit dem Versatz von **heute** umgerechnet, ein Sommertermin, im Winter importiert, lag eine Stunde daneben | `caltext::zone_tests::utc_times_use_the_offset_of_their_own_day` | behoben: Versatz des Termintags aus der Zeitzonendatenbank des Geräts |
| F33 | M | Kalender-Import: Die Zeitzone eines Termins (`TZID`) wurde ignoriert, „10 Uhr New York“ stand um 10 Uhr Ortszeit | `…::times_of_another_zone_are_converted`, `…::zones_only_the_file_knows`, `…::world_time_zones_by_the_rules_of_a_file` | behoben: Zone über das Gerät, sonst über die Regeln in der Datei (Outlook-Namen); unbekannte Zone bleibt wie geschrieben |
| F34 | M | Android: Erinnerungen blieben nach einem Zeitzonen-Wechsel (Reise) auf dem alten Zeitpunkt, bis die App geöffnet wurde | `tools/check-reminders.sh` (Emulator) | behoben: Erinnerung merkt sich die Uhrzeit und wird beim Wechsel neu geplant |
| F35 | N | Kalenderdateien anderer Programme können Erinnerungen Jahrzehnte vor dem Termin tragen; sie wurden übernommen, gingen beim Export aber verloren | `collections::public_calendar_files_import_soundly…` | behoben: Erinnerungen höchstens ein Jahr vorher, beim Import und beim Speichern |
| F36 | N | README und Konzept sprachen von „vier Bereichen“, der Kalender fehlte auf der Hauptseite | Durchsicht | behoben; Regel in `PROJEKT.md`: README bei jedem relevanten Pull Request mitziehen |
| F37 | N | Schnell-Eintrag: Das Beispiel, das die App selbst vorschlägt („Run tomorrow 6:30pm-7:30pm #sports“), wurde nicht verstanden, ganztägiger Termin mit der Uhrzeit im Titel | `caltext::tests::quick_examples_of_the_app` (prüft alle Beispiele der App, deutsch und englisch) | behoben: Zeitspannen mit am/pm oder „Uhr“ auf beiden Seiten |
| F38 | M | Serien aus einer anderen Zeitzone (Einladung aus den USA, Termin in Tokio): Nur der erste Termin wurde umgerechnet. In den Wochen, in denen nur eine Seite die Uhr umstellt, und bei Zonen ohne Sommerzeit das halbe Jahr lang, lagen die Folgetermine eine Stunde daneben | `properties::series_from_other_time_zones_are_right_all_year`, `caltext::zone_tests::a_series_keeps_the_time_zone_it_came_from`, `…::windows_names_of_outlook` | behoben: Die Serie merkt sich ihre Zeitzone (neues Feld `zone`), jedes Vorkommen wird mit dem Versatz seines Tages umgerechnet; auch Anzeige, Erinnerungen, Löschen einzelner Tage und Export |
| F39 | H | **CI: Tests gegen echte Server liefen ins Leere.** GitHub hat die CI auf Ubuntu 26.04 umgestellt; dessen Dovecot 2.4 lehnte die Testkonfiguration ab. Der Startschritt verschluckte den Fehler (Rohrleitung) und die Prüfung `test -n A && test -n B` hält ein `bash -e`-Skript nicht an. Zwei Läufe meldeten 57 Netzwerk-Tests als grün, obwohl alle übersprungen wurden | CI-Log (`doveconf: Fatal`) | behoben: Konfiguration für Dovecot 2.3 und 2.4, Fehler beim Serverstart und fehlende Server-Variablen machen die CI rot; Netzwerk-Tests laufen wieder echt (31 s, 57 Tests) |
| F23 | M | **Tresornamen stehen im Dateinamen** (Server, Sicherung): „bank-privat.ghlpss“ verrät, welche Tresore es gibt | `attacks::stolen_device_files…` (bewusst ausgenommen) | **offen, Entscheidung nötig** |
| F24 | N | Termintitel und Import-Titel werden getrimmt (führender Tab geht verloren); Passwörter bleiben exakt | `attacks::exports_are_safe…` | **offen, vertretbar** |

Zu F23: Vorschlag, Dateien künftig nach ihrer Kennung benennen (`3f9a….ghlpss`), den Namen nur verschlüsselt führen; alte Dateinamen beim nächsten Sync umbenennen. Nachteil: Im Ordner sieht man nicht mehr, welche Datei welcher Tresor ist. Test danach: Marker im Tresornamen darf auch in Datei- und ZIP-Namen nicht vorkommen.

Zu F17 (Verhalten seit der Behebung): Die erste Mail eines neuen Kontakts mit Autocrypt-Schlüssel zeigt „Schlüssel unbekannt/unbestätigt“; ab der nächsten Mail „gültig“ (Vertrauen beim ersten Kontakt). Öffnet man dieselbe erste Mail erneut, zeigt sie „gültig“, das ist eine kleine Unstimmigkeit (siehe B12).

## 4. Offene Punkte (Audit und Tests)

### Sicherheit und Datenhaltung

| Nr. | Schwere | Punkt | Vorschlag | Test, der grün werden muss |
|---|---|---|---|---|
| B1 | H | Neuer Wiederherstellungscode / entfernter Key schützt nicht gegen **alte Dateikopien**: der innere Schlüssel wird nicht erneuert | Beim Code-Wechsel und Key-Entfernen VMK und Faktor-Geheimnis neu erzeugen, alles neu verschlüsseln (Formatänderung, mit Umzug) | neu: altes Notfallblatt + alte Kopie öffnet aktuelle Datei **nicht** |
| B2 | M | Entfernte Keys können per Sync zurückkommen (Gerät war offline und fügt selbst einen Key hinzu) | Löschmarken je Key in der Suite-Datei | neu in `chaos.rs` |
| B3 | M | Drive-Index ohne Schutz gegen Zurücksetzen (alter Index vom Server) und gleichzeitige Änderungen zweier Geräte | Revision im Drive-Bereich merken, älteren Index ablehnen; vor dem Schreiben vergleichen | neu: Index-Rollback wird erkannt |
| B4 | M | Sperrbildschirm zeigt Absender/Betreff/Termin, wenn Android „alle Inhalte“ zeigt | Kanäle mit `lockscreenVisibility = PRIVATE` (neue Kanal-IDs) | Emulator-Ablauf |
| B5 | M | Mail-Zugang für Benachrichtigungen: Keystore ohne StrongBox; Doku (KONZEPT) beschreibt noch „opt-in, Panik löscht“ | Doku angleichen, StrongBox versuchen, optional „harte Panik“ | - |
| B6 | N | `.ics` aus fremden Apps wird ohne Rückfrage importiert | Vorschau mit Bestätigung | Emulator-Ablauf |
| B7 | N | Interne Aktionen (`OPEN_MAIL`, `OPEN_EVENT`) über die exportierte MainActivity von fremden Apps auslösbar | Nicht exportierte Ziel-Activity oder Zufallskennung im PendingIntent | - |
| B8 | N | Auto-Sperre: Timer läuft im Tiefschlaf nicht weiter; nach Teilen/Kamera gilt dauerhaft eine Mindestfrist von 120 s | Zeit beim Verlassen merken und beim Zurückkommen vergleichen; Frist nach Rückkehr zurücksetzen | - |
| B9 | N | Autofill verknüpft eine App nach einmaliger Auswahl dauerhaft, ohne Rückfrage | Bestätigung mit App-Name und Paket | - |
| B10 | N | Windows: entschlüsselte Kopien in `%TEMP%` bleiben liegen, wenn ein Programm sie noch offen hat | Löschergebnis prüfen, Hinweis; Ordner im Profil mit Zufallsnamen | - |
| B11 | N | „Passwort anderswo geändert“: Server kann teure Argon2-Werte erzwingen (bis 2 GiB) | Werte nicht über die eigenen hinaus annehmen oder vorher fragen | `attacks::forged_or_weakened…` erweitern |
| B12 | N | Erste Autocrypt-Mail: beim zweiten Öffnen „gültig“ statt „unbestätigt“ | Herkunft des Schlüssels (Mail-ID) merken | Netzwerktest erweitern |
| B13 | N | Geänderte Server-/Kontodaten in der Suite-Datei werden nicht an andere Geräte weitergegeben (gleiche ID gewinnt lokal) | Zeitstempel je Listeneintrag wie bei Bereichs-Optionen | neu in `chaos.rs` |
| B14 | N | Freigabe zurückziehen meldet Erfolg, auch wenn das Paket auf dem Server bleibt | Paket immer löschen, Fehler melden | - |
| B15 | N | PHP-Modul: Fehlversuchssperre per IP (hinter Proxy aussperrbar), Endungsliste erfasst `x.php.jpg` nicht, Modul-Datei ohne `chmod` | Sperre an Schlüssel statt IP koppeln; Allowlist der Endungen; `chmod 0600` | PHP-Test |
| B16 | - | ~~Mail-Bilder zu `localhost`/privaten Adressen~~ | erledigt (F28); Rest: Namensauflösung kann zwischen Prüfung und Verbindung wechseln (DNS-Rebinding), Adresse nach dem Verbinden prüfen | - |
| B17 | N | CI: Signaturschlüssel und FTP-Zugang stehen bei Pushes auf jeden Branch bereit | Nur `main` (GitHub-Environment mit Freigabe) | - |
| B18 | N | Zusammenführen bei gleichzeitigem Löschen (Gerät A) und Bearbeiten (Gerät B) desselben Eintrags: Ergebnis hängt von Revision/Zeit ab | Regel festlegen („Bearbeiten gewinnt, Löschung wird gemeldet“) und testen | neu in `crazy_user.rs` |
| B20 | - | ~~Serie aus einer anderen Zeitzone: nur der erste Termin wurde umgerechnet~~ | erledigt (F38): die Serie merkt sich ihre Zeitzone | `properties::series_from_other_time_zones_are_right_all_year` |
| B21 | N | Emulator: GhulDrive und GhulMail mit echtem Server | Testserver laufen in der CI neben dem Emulator (SFTP mit Passwort, IMAP, SMTP); Abläufe `app/maestro/server/`; danach prüft `tools/check-server-flows.sh`, dass auf den Servern nur Verschlüsseltes liegt | teilweise: GhulDrive läuft verpflichtend durch (Ordner auf dem Server nicht lesbar). Der Mail-Ablauf verbindet sich und bestätigt das Zertifikat, scheitert aber noch an der Anmeldung im Testskript; bis er sicher läuft, ist er nur eine Warnung (`GHUL_MAIL_FLOW_REQUIRED=1` macht ihn verpflichtend). Beide laufen nachts und von Hand (`emulator.yml`) |
| B22 | N | Windows: Erinnerungen werden nur geplant, solange die App läuft; ein Zeitzonen-Wechsel im laufenden Programm ist ungeprüft | prüfen, bei Bedarf neu planen | Desktop-Test |
| B19 | N | Test-Hilfe: Nach Sperren/Entsperren muss Drive im Test neu verbunden werden (`NotConfigured`); in der App verbindet sie sich selbst | prüfen, ob die App nach Entsperren immer neu verbindet | Desktop-UI-Test |

### Bedienung (Fehler, die Nutzer machen)

| Nr. | Punkt | Vorschlag |
|---|---|---|
| U1 | ~~Mail löschen ohne Rückgängig; Wischen „Archivieren“ ohne Archivordner löscht still~~ | erledigt: Rückgängig nach jedem Löschen und Verschieben; ohne Archivordner kommt die Meldung „Dieses Postfach hat keinen Archivordner.“ |
| U2 | ~~Einzelnen Termin löschen ohne Rückgängig~~ | erledigt: Rückgängig |
| U3 | ~~Drive „Endgültig löschen“ ohne Rückfrage; Löschtext sagt „vom Server gelöscht“, obwohl Papierkorb~~ | erledigt: Rückfrage mit rotem Knopf; der Papierkorb fragt nicht und meldet „In den Papierkorb verschoben“ |
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
| K1 | ~~Listen bedienen sich je Bereich anders~~ erledigt: ein Auswahlkopf oben in allen fünf Listen, langes Drücken überall, gleiche Auswahl-Darstellung |
| K2 | ~~Löschschutz uneinheitlich~~ erledigt: einzelne Einträge ohne Rückfrage mit „Rückgängig“; Ganzes (Tresor, Bereich, Konto, Server, Papierkorb leeren, mehrere endgültig) mit Rückfrage und rotem Knopf |
| K3 | ~~Assistent und Optionen bieten verschiedene Werte~~ erledigt: eine gemeinsame Liste |
| K4 | ~~Kalender-Blatt „Fertig“, sonst „Speichern“~~ erledigt |
| K5 | ~~„1. Bereich“, aber „Fünfter Bereich“~~ erledigt |
| K6 | ~~Drive-Kontextblatt führt zur falschen Seite~~ erledigt |
| K7 | ~~Hinweise für Windows erscheinen auf Android und umgekehrt~~ erledigt |
| K8 | ~~Falscher Hinweis „Favorit: Konto lange drücken“~~ erledigt (entfernt) |

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
- Kalender-Import: 304 Dateien anderer Programme ohne Absturz; was angenommen wird, übersteht Export → Import unverändert. Zeiten aus UTC und aus anderen Zeitzonen landen auf der richtigen Stunde; die Regeln aller 446 Zeitzonen stimmen in 777.000 Stichproben mit der Zeitzonendatenbank überein.
- Bekloppter Nutzer (3 Geräte, 3.000+ Aktionen, auch Sperren, Neustarts, Müll-Importe, falsche Ordner): kein Absturz, nur verständliche Ablehnungen, nichts verloren, nichts Gelöschtes zurück.

## 7. Empfohlene Reihenfolge

1. **Diesen Branch mergen** (alle Funde F01 bis F22 behoben, Nachtlauf aktiv).
2. **F23** entscheiden (Tresornamen in Dateinamen), betrifft das Format, besser früh.
3. **B1** (Schlüssel bei Code-Wechsel erneuern), größter Sicherheitspunkt, Formatänderung.
4. **Texte (Abschnitt 5)** in einem Durchgang, danach `--strict` einschalten.
5. **U1 bis U5** (Rückgängig, Rückfragen, deutsche Fehlermeldungen).
6. Rest von B und U/K nach Aufwand.

Nach jedem Durchgang: dieses Protokoll aktualisieren (Status, Commit), neue Funde mit nächster Nummer anhängen.

## 8. Lückenanalyse Sicherheit (zweite Runde)

Was nach beiden Runden **nicht** oder nur teilweise abgedeckt ist, ehrlich aufgelistet, damit nichts als „geprüft“ gilt, was es nicht ist.

### Braucht eine Entscheidung oder größeren Umbau

| Nr. | Lücke | Bedeutung | Vorschlag |
|---|---|---|---|
| L1 | **Schlüssel werden nie erneuert** (B1) | Altes Notfallblatt oder entfernter Key + alte Dateikopie öffnet die aktuelle Suite | Formatänderung: bei Code-Wechsel/Key-Entfernen alles neu verschlüsseln |
| L2 | **Kein unabhängiger Audit** | Eigene Tests finden nur, woran man denkt | Externer Code-Audit und Pentest vor Version 1.0 |
| L3 | **Abhängigkeiten der App (Gradle) ohne Prüfsummen** | Manipuliertes Paket auf dem Weg vom Server würde eingebaut | `gradle/verification-metadata.xml` (SHA-256 je Paket); `cargo deny` für Quellen und Lizenzen |
| L4 | **Build nicht nachprüfbar** | Niemand kann prüfen, dass die APK aus diesem Quelltext stammt | Reproduzierbare Builds; signieren nur für `main`/Tags in geschützter Umgebung (B17) |
| L5 | **PHP-Modul: Schlüssel steht in der PHP-Datei** | Liefert ein falsch eingestellter Server PHP als Text aus, ist der Zugriffsschlüssel lesbar (Daten bleiben verschlüsselt, aber löschbar/überschreibbar) | Schlüssel in eine Datei im gesperrten Datenordner auslagern; Selbsttest „wird PHP ausgeführt?“ beim Einrichten |
| L6 | **Metadaten auf dem Server** | Server sieht Anzahl, Größe und Änderungszeit der Dateien, Tresornamen (F23), die IP | F23 beheben; Rest dokumentieren (unvermeidbar ohne eigenen Dienst) |
| L7 | **PGP-Schlüssel von Kontakten nur „Vertrauen beim ersten Kontakt“** | Erster Schlüssel kann untergeschoben sein | Fingerabdruck-Vergleich in der App (QR/vorlesen), Warnung bei Schlüsselwechsel |

### Technisch nicht testbar oder außerhalb der App

| Nr. | Lücke | Einordnung |
|---|---|---|
| L8 | Geheimnisse im Arbeitsspeicher der Oberfläche (Kotlin-Zeichenketten lassen sich nicht sicher löschen), Auslagerungsdatei/Ruhezustand unter Windows | Nur der Rust-Kern löscht sicher. Schutz: Sperren leert den Kern; Geräteverschlüsselung nutzen |
| L9 | Gerät mit Schadsoftware, Root, fremde Tastatur-App, Bedienungshilfen-Dienste, Bildschirmaufnahme durch das System | Dagegen schützt keine App. Hinweis in der Doku; Passwortfelder ohne Vorschläge sind gesetzt |
| L10 | Tapjacking (fremde App legt ein Fenster über GhulSuite) | Offen: `filterTouchesWhenObscured` für Entsperren, Löschen und Autofill-Auswahl setzen und im Emulator testen |
| L11 | Zertifikats-Widerruf wird nicht geprüft | Üblich bei Apps (auch Browser prüfen nur eingeschränkt); mit festem Fingerabdruck (eigene Server) ohne Belang |
| L12 | Zeit-Seitenkanäle | Nicht gemessen; Vergleiche von Geheimnissen laufen in konstanter Zeit, die Kryptobibliotheken sind dafür ausgelegt |
| L13 | Windows: portable Version auf fremden PCs (Keylogger), DLL-Suchpfad, Rechte des Datenordners | Offen: Rechte beim Anlegen setzen; Hinweis „nur auf eigenen Geräten entsperren“ |

### Weitere Sammlungen und Werkzeuge (noch nicht eingebaut)

| Was | Wofür | Aufwand |
|---|---|---|
| **MobSF** (statisch) nach **OWASP MASVS** | Standard-Prüfliste für Mobil-Apps gegen die fertige APK | klein, Nachtlauf |
| **drozer** / **QARK** | Angriffe auf von außen erreichbare Android-Komponenten im Emulator (Intents, Provider) | mittel |
| **Semgrep** (Regeln für Kotlin, PHP, Geheimnisse) | bekannte Fehlermuster im Quelltext | klein, Nachtlauf |
| **OWASP ZAP** (Baseline + aktiv) | automatischer Angriffsscan auf PHP-Modul und Empfängerseite | klein, Nachtlauf |
| **cargo-mutants** | prüft, ob die Tests absichtlich eingebaute Fehler bemerken | mittel, wöchentlich |
| **x509-limbo** (C2SP) | tausende Zertifikatsketten-Fälle | mittel; prüft v. a. die TLS-Bibliothek |
| **OpenPGP Interoperability Test Suite** (Sequoia) | PGP-Verhalten gegenüber anderen Programmen, bekannte Angriffe (z. B. EFAIL-Muster) | mittel |
| **Unicode `confusables.txt`**, **IDNA-Testdaten** | Warnung bei Namen/Domains, die wie andere aussehen | klein |
| **MIME-Torture-Tests** (jwz), Beispielmails großer Mailprogramme | kaputte und extreme Mails | klein |
| **Dovecot imaptest** | IMAP-Dauerlast und Protokollfehler | mittel |
| **NIST CAVP** (AES-GCM, HMAC, SHA) | weitere Krypto-Vektoren | klein |
| **RFC 8949 Anhang A** (CBOR), **RFC 9106** (Argon2) | Format-Vektoren | klein |
| **csv-spectrum**, **fuzzdb**, **IntruderPayloads** | weitere Eingabe-Sammlungen | klein |
| Geräte-Matrix (Samsung, Pixel, Xiaomi; Android 10 bis 16) | Hersteller-Eigenheiten bei Hintergrunddiensten, Autofill, Keystore | groß, braucht Testgeräte oder einen Gerätedienst |

### Stand der Abdeckung nach Angriffsfläche

| Angriffsfläche | Abdeckung | Rest |
|---|---|---|
| Dateien auf Gerät, Server, Sicherung | hoch (Klartextsuche, Manipulation, Fälschung, Namen) | L1, F23 |
| Synchronisation | hoch (Chaos, bekloppter Nutzer, Rollback einzelner Dateien) | B2, B3, B13 |
| Netzwerk-Parser | hoch (Fuzzing schnell und lang) | - |
| TLS | mittel → hoch mit badssl im Nachtlauf | L11 |
| PHP-Modul | hoch (335 Angriffe, PHPStan nachts) | L5, ZAP |
| Mail (HTML, Kopfzeilen, PGP) | hoch (≈500 XSS-Muster, CRLF, Autocrypt) | L7, Interop-Suite |
| Autofill | hoch (Suffix-Liste, Browser-URL-Daten, vertrauenswürdige Browser) | Geräte-Matrix, B9 |
| Andere Apps (Intents) | mittel (Manifest-Liste, nur fremde Inhalte) | drozer, B6, B7, L10 |
| Entsperren (Fingerabdruck, PIN) | mittel (Zähler, Bindung an Neustart) | Test im Emulator, L10 |
| Lieferkette | niedrig bis mittel (cargo audit, gepinnte Actions) | L3, L4 |
| Kalender (Rechnen, Import, Zeitzonen, Erinnerungen) | hoch (Kalendertage, Dateien anderer Programme, alle Zeitzonen, Alarm-Prüfung im Emulator) | B22, B6, B4 |
| Oberfläche (Fehlbedienung) | hoch (Maestro für alle fünf Bereiche, auch mit echtem Server; 10 Bedienungstests am Desktop; Monkey nachts) | Abschnitt 4 U |

## 9. Gestaltung (Durchsicht aller Bildschirme)

Der Oberflächen-Code wurde Bildschirm für Bildschirm gegen das eigene Design-System geprüft (rund 50 Abweichungen) und angeglichen. Geprüft an den gerenderten Bildschirmfotos (hell, dunkel, Handy, breites Fenster).

Jetzt überall gleich:

| Was | Regel |
|---|---|
| Bereiche | Fünf, immer in der Reihenfolge Passwörter, 2FA, Dateien, Mail, Kalender; Farbe und Symbol eines Bereichs kommen aus einer Stelle (auch im Assistenten und unter „Bereiche verwalten“, wo Mail und Kalender falsche Farben hatten). Logo-Streifen, Start- und Entsperrseite zeigen fünf. Das Android-Symbol ist der Schild des Logos (vorher ein Schloss auf Blau) |
| Farben | Gewähltes immer in der Akzentfarbe; die Bereichsfarbe nur für Leiste, Kontextzeile, Hauptknopf, Heute-Markierung und Ungelesen-Zähler. Keine fest eingebauten hellen Farben mehr (dunkles Design: Zähler, Sicherheitszeilen der Mail) |
| Kopfzeilen | Detailansichten ohne Titel, Bearbeiten als Stift oben. Editoren mit Kreuz links und der Hauptaktion oben rechts („Speichern“, „Senden“) |
| Knöpfe | Hauptknopf rechts, Abbrechen links; Bestätigen von Löschen in Rot. Der Knopf unten rechts hat in allen fünf Bereichen ein Symbol und ein Wort |
| Löschen | Endgültiges fragt vorher (Termin, Drive „Endgültig löschen“, Mail im Papierkorb, Anhang); der Papierkorb fragt nicht. Nach jedem Löschen eine kurze Meldung |
| Menüs | Titel ist der Name der Sache, Zeilen mit Symbol, Löschen zuletzt und rot |
| Abstände | Gleiche Ränder in allen Listen (das Suchfeld sitzt in jedem Reiter gleich hoch), eine Art Abschnittstitel, gleiche Symbolgrößen, Pfeile und Trennlinien |
| Leere Zustände | Symbol und Satz in allen Bereichen, auch im Kalender |
| Texte | Keine langen Striche. „Optionen“ statt „Einstellungen“, „Fertig“ nur noch für „fertig“ („Abschließen“ am Ende der Einrichtung), „Löschen“ für Gelöschtes und „Entfernen“ für Konten und Server, „Passwort und zweiter Faktor“ statt „Passwort und Key“, Suchfelder „… suchen“. Leerer Betreff heißt „(kein Betreff)“ |

Bedienung (zweiter Durchgang), alles erledigt:

| Nr. | Punkt | Stand |
|---|---|---|
| G1 | Mehrfachauswahl in allen fünf Bereichen, ein Auswahlkopf oben | erledigt |
| G2 | „Rückgängig“ nach jedem Löschen und Verschieben einzelner Einträge (die Aktion wird erst nach etwa 6 s, beim nächsten Löschen oder vor dem eigenen Sperren ausgeführt; bei Panik-Sperre bleibt der Eintrag) | erledigt |
| G3 | Breite Fenster: GhulAuth (Konto hinzufügen) und GhulCal (Termin ansehen und bearbeiten) öffnen rechts; GhulDrive bleibt ein Blatt | erledigt |
| G4 | Ladefehler in allen Listen sichtbar, mit „Erneut versuchen“ | erledigt |
| G5 | Aufklappmenüs sind Aktionsblätter im gemeinsamen Stil; Tresor löschen sieht aus wie Bereich entfernen | erledigt |
| G6 | Geprüft: 10 neue Bedienungstests klicken durch die echten Bildschirme (Auswahl, Löschen, Rückgängig, Rückfragen, Menüs, breites Fenster) und speichern 25 Bilder; App-Symbol in den drei Android-Formen gezeichnet | erledigt |

Noch nicht auf einem Gerät ausprobiert: Zurück-Taste beendet die Auswahl, Wechsel in den Hintergrund führt eine offene Löschung aus, Strg+L unter Windows.
