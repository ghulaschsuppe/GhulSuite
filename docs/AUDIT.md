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

**In Zahlen:** 41 Funde aus den Testrunden, davon 39 behoben (offen: F24, eine vertretbare Kleinigkeit, und die
Einzelheit B12). 230 Testfunktionen im Kern mit über einer Million Einzelprüfungen je Lauf, 335 Angriffe auf das
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
| **Abhängigkeiten** | `russh` (Speicher-Angriff durch bösartigen SSH-Server), `zip`, `crypto-bigint` über `p256` auf sichere Versionen | A1, A2, R1 |

## 4. Offen

Alles, was nicht behoben ist, ehrlich aufgelistet. Nichts davon gibt einem Angreifer ohne Passwort **und** zweiten
Faktor Zugriff auf Tresorinhalte.

### 4.1 Mittel

| Nr. | Punkt | Vorschlag |
|---|---|---|
| B2 | Ein entfernter Key kann per Sync zurückkommen: Ein Gerät war offline und fügt dort selbst einen Key hinzu; seine Schutzänderung ist dann die neuere und bringt den alten Key mit | Löschmarken je Key in der Suite-Datei |
| B3 | Drive-Index ohne Schutz gegen Zurücksetzen (ein alter Index vom Server lässt Drive älter aussehen) und gegen gleichzeitige Änderungen zweier Geräte | Revision merken, älteren Index ablehnen, vor dem Schreiben vergleichen |
| B4 | Sperrbildschirm: Mail-Absender, Betreff und Termindetails sind sichtbar, wenn Android auf „alle Inhalte anzeigen“ steht (Standard). Die neutrale Version greift nur bei „sensible Inhalte ausblenden“ | Benachrichtigungskanäle mit privater Sichtbarkeit erzwingen |
| B5 | Mail-Zugang für Benachrichtigungen liegt im Android-Keystore ohne StrongBox und bleibt bei der Panik-Sperre (gewollt, damit Mails weiter ankommen); das Konzept beschreibt noch das alte Verhalten | Doku angleichen, StrongBox versuchen, wahlweise „harte Panik“ |
| L10 | Tapjacking: Eine fremde App kann ein Fenster über GhulSuite legen und Tipps umleiten | `filterTouchesWhenObscured` für Entsperren, Löschen und Autofill-Auswahl |
| L3 | Gradle-Abhängigkeiten der App ohne Prüfsummen: Ein unterwegs manipuliertes Paket würde eingebaut | `gradle/verification-metadata.xml` |
| L4, B17 | Build nicht nachprüfbar; der Signaturschlüssel steht bei Pushes auf jeden Branch bereit | Reproduzierbare Builds; signieren nur für `main` in einer geschützten Umgebung |
| L5 | Der Zugangsschlüssel des PHP-Moduls steht in der PHP-Datei. Liefert ein falsch eingestellter Server PHP als Text aus, ist er lesbar (Daten bleiben verschlüsselt, ließen sich aber löschen) | Schlüssel in den gesperrten Datenordner, Selbsttest „wird PHP ausgeführt?“ |
| L7 | PGP-Schlüssel von Kontakten: Vertrauen beim ersten Kontakt; der erste Schlüssel kann untergeschoben sein | Fingerabdruck-Vergleich per QR oder Vorlesen, Warnung bei Schlüsselwechsel |

### 4.2 Niedrig

| Nr. | Punkt |
|---|---|
| L1 (Rest) | Eine alte Sicherung öffnet sich mit dem damals Gültigen weiter und zeigt den Stand von damals, auch den Drive-Schlüssel und Mail-Zugänge von damals. Nach einem Verlust sollten diese gewechselt werden; ein Hinweis in der App fehlt noch |
| B6 | `.ics`-Dateien aus fremden Apps werden ohne Rückfrage importiert |
| B7 | Interne Aktionen („Mail öffnen“, „Termin öffnen“) lassen sich über die exportierte Haupt-Activity von fremden Apps auslösen (öffnet nur, zeigt nichts ohne Entsperren) |
| B8 | Auto-Sperre: Der Zeitgeber läuft im Tiefschlaf nicht weiter; nach Teilen oder Kamera gilt dauerhaft eine Mindestfrist von 120 s |
| B9 | Autofill verknüpft eine App nach einmaliger Auswahl dauerhaft, ohne Rückfrage mit App-Name |
| B10 | Windows: entschlüsselte Kopien in `%TEMP%` bleiben liegen, wenn ein Programm sie noch offen hat |
| B11 | „Passwort anderswo geändert“: Ein Server kann teure Argon2-Werte (bis 2 GiB) vorgeben, alte Handys können daran scheitern |
| B12 | Erste Autocrypt-Mail eines Kontakts zeigt beim zweiten Öffnen „gültig“ statt „unbestätigt“ |
| B13 | Geänderte Server- und Kontodaten in der Suite-Datei erreichen andere Geräte nicht |
| B14 | Freigabe zurückziehen meldet Erfolg, auch wenn das Paket auf dem Server bleibt |
| B15 | PHP-Modul: Fehlversuchssperre je IP (hinter einem Proxy aussperrbar), `x.php.jpg` wird nicht abgelehnt, Modul-Datei ohne `chmod` |
| B16 (Rest) | Mail-Bilder: Die Namensauflösung könnte zwischen Prüfung und Verbindung wechseln (DNS-Rebinding) |
| B18 | Gleichzeitig auf Gerät A gelöscht und auf Gerät B bearbeitet: Das Ergebnis hängt von Revision und Zeit ab, eine feste Regel fehlt |
| L13 | Windows portable: Rechte des Datenordners, DLL-Suchpfad, Nutzung auf fremden PCs |

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

### 4.4 Noch nicht eingebaute Prüfwerkzeuge

MobSF nach OWASP MASVS, drozer, OWASP ZAP, x509-limbo, die OpenPGP Interoperability Test Suite und eine
Geräte-Matrix (Samsung, Pixel, Xiaomi; Android 10 bis 16). Liste mit Aufwand im Testprotokoll, Abschnitt 8.

## 5. Geprüft und in Ordnung

- **Dateiformat:** strenge Prüfung aller Längen, Versionen und Arten; Header als AAD (jede Änderung macht die Datei
  unbrauchbar); Argon2id-Grenzen gegen geschwächte Werte; neuere Dateien werden als „bitte aktualisieren“ erkannt.
  Testvektoren für beide Formatversionen, unabhängig nachgerechnet in Python mit anderen Krypto-Bibliotheken.
- **Kryptografie:** XChaCha20-Poly1305, HKDF-SHA256, Argon2id, X25519; alle Bausteine bestehen die Wycheproof-Fälle.
  Frische Nonces bei jedem Schreiben, Geheimnisse werden im Kern genullt, Vergleiche in konstanter Zeit.
- **FIDO2:** nur mit PIN-Prüfung, eigene RP-ID, die keine Webseite ansprechen kann.
- **Schnell-Entsperren:** Android-Keystore, Fingerabdruck „stark“, ungültig bei neuem Fingerabdruck, Fehlversuche
  begrenzt, an den Neustart gebunden; Panik-Sperre löscht es.
- **Dateien:** kein Klartext auf Gerät, Server, in Sicherung oder Export (gesucht auch als UTF-16, Hex, Base64),
  auch nicht in Datei- und ZIP-Namen. Gestohlene Dateien öffnen sich nur mit beiden Faktoren.
- **Drive:** gekippte Bits, vertauschte oder gelöschte Blöcke, gefälschter Index: immer eine Fehlermeldung, nie
  falsche Daten.
- **Freigaben:** zufälliger 256-Bit-Schlüssel je Freigabe, nur im Teil nach `#`; AES-256-GCM mit eindeutigen Nonces,
  Abschneiden und Vertauschen werden erkannt; optional Passwort (PBKDF2, 600 000 Runden). Empfängerseite mit
  strenger CSP, ohne fremde Inhalte, ohne Referrer.
- **PHP-Modul:** jeder Aufruf signiert (HMAC über Methode, Parameter, Zeit, Nonce, Inhalt), Einmal-Nonces gegen
  Wiederholung, Pfadprüfung, atomares Schreiben, zufälliger gesperrter Datenordner, keine Datenbank, keine
  Fehlerdetails. 335 gezielte Angriffe, PHPStan im Nachtlauf, PHP-8.1-Syntax in der CI.
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
  nichts mehr, was gespeichert wird. Alte Sicherungen zeigen weiter den Stand von damals: Drive-Schlüssel und
  Mail-Passwörter von damals bei Bedarf wechseln.
- **Freigabelinks:** Wer den Link hat, kann die Datei öffnen. Für Sensibles ein Passwort setzen und getrennt
  mitteilen. Ein Einmal-Link gilt als verbraucht, sobald der Abruf beginnt.
- **Webserver ohne `.htaccess`** (z. B. nginx): Dort schützt nur der zufällige Name den Datenordner bzw. die
  zufällige ID das Paket; Inhalte bleiben verschlüsselt.
- **Sperrbildschirm:** Bis B4 behoben ist, in Android „sensible Inhalte ausblenden“ einschalten, wenn Absender und
  Termine dort nicht zu sehen sein sollen.
- **Windows portable:** nur auf eigenen Geräten entsperren.
- GhulSuite ist eine **Entwicklungsversion**: mit regelmäßiger Sicherung nutzen.
