# GhulSuite: Sicherheits-Audit 0.5.0-dev (11.11.2026)

Internes Audit des eigenen Codes vor dem Test der Version 0.5.0-dev. Schwerpunkt: alles, was mit dieser Version neu
ist und von außen erreichbar ist, Freigabelinks, Empfängerseite, PHP-Server-Modul, FTPS, HTTPS-Client, Teilen an
andere Apps, plus eine Prüfung aller Rust-Abhängigkeiten auf bekannte Sicherheitslücken. Es ersetzt kein externes
Audit durch Dritte.

## Vorgehen

- Code-Durchsicht: `share.rs`, `app/shares.rs`, `net/http.rs`, `net/tls.rs`, `net/ftps.rs`, `net/phpfs.rs`,
  `server/ghulsuite.php`, `share-viewer/viewer.js`, Android-Teilen (`AndroidNetBackend.kt`, FileProvider).
- Angriffstests als automatische Tests (laufen in der CI):
  - Browser (Chromium/Playwright) gegen echte, vom Kern erzeugte Links, statischer Teilen-Ordner und PHP-Modul:
    CSP- und Referrer-Header, Schlüssel verschwindet aus der Adresszeile, falsches Passwort, falscher Schlüssel,
    unvollständiger Link, fehlendes Paket, abgelaufenes Paket, Einmal-Link zweimal, unsignierte und gefälschte
    API-Aufrufe, Pfad im Freigabe-ID-Parameter, keine Skriptfehler.
  - Kern: Wiederholungsangriff (Replay) auf das PHP-Modul, falscher Zugangsschlüssel, Pfade außerhalb des Speichers,
    ausführbare Dateien und Server-Konfiguration schreiben, manipulierte Pakete (Kopf, Stücke, Abschneiden),
    absurde PBKDF2-Rundenzahl.
- `cargo audit` gegen die RustSec-Datenbank (jetzt auch als eigener CI-Job).

## Ergebnisse

| # | Befund | Schwere | Status |
|---|---|---|---|
| A1 | `russh` 0.54: ein bösartiger SSH-Server kann unbegrenzte Speicherzuweisungen auslösen (RUSTSEC-2026-0153/0154) → App-Absturz beim Verbinden mit einem fremden/kompromittierten Server | hoch | **behoben:** russh 0.65; Host-Zertifikate werden zusätzlich abgelehnt, nur der bestätigte Schlüssel zählt |
| A2 | `zip` 2.6: Lücken in `extract()` (RUSTSEC-2026-0367/0368/0369/0429) | hoch (nicht ausnutzbar: GhulSuite liest ZIPs nur im Speicher und ruft `extract()` nie auf) | **behoben:** zip 9 |
| A3 | Teilen-Ordner auf reinen Webservern (SFTP/FTPS): abgelaufene Pakete blieben bis zum nächsten Aufräumen durch die App abrufbar | mittel | **behoben:** die Empfängerseite prüft das verschlüsselte, unveränderbare Ablaufdatum selbst; Browser-Test ergänzt |
| A4 | Empfängerseite und Kern akzeptierten jede PBKDF2-Rundenzahl aus dem Paketkopf → ein manipuliertes Paket konnte den Browser des Empfängers minutenlang blockieren | niedrig | **behoben:** Obergrenze 10 Mio.; Test ergänzt |
| A5 | PHP-Modul: wer den Zugangsschlüssel hat, konnte `.php`-Dateien oder `.htaccess` im Datenordner ablegen (auf Servern ohne `.htaccess`-Unterstützung, z. B. nginx, eventuell ausführbar) | niedrig (setzt den Schlüssel voraus) | **behoben:** ausführbare Endungen, `.htaccess`, `.htpasswd`, `.user.ini` werden abgelehnt; Test ergänzt |
| A6 | Android „An App senden“: Dateinamen wie `..` und gleichnamige Dateien bei Mehrfachauswahl wurden nicht abgefangen; ganze Datei im Speicher | niedrig | **behoben:** Namen bereinigt und eindeutig, Schreiben blockweise |
| A7 | Mail-Anhänge beliebiger Größe → Speicherdruck, Ablehnung durch den Mailserver | niedrig | **behoben:** über 20 MB nur noch als Link |
| A8 | Test-Fixtures leiteten die Freigabe-ID aus dem Link-Schlüssel ab (Produktivcode war korrekt: unabhängige Zufalls-ID) | Hinweis | **behoben**, damit es nicht kopiert wird |
| R1 | `crypto-bigint` 0.5 (über `p256` 0.13): mögliche Laufzeitunterschiede in Operationen, die konstant sein sollten (RUSTSEC-2026-0387). Betraf den Schlüsselaustausch mit dem Sicherheitsschlüssel (CTAP2-PIN-Protokoll) | niedrig | **behoben:** `p256` 0.14 (`crypto-bigint` 0.7). Schlüssel werden aus dem Zufallsgenerator des Betriebssystems per Verwerfungsverfahren erzeugt; neuer Test mit festen Werten aus der P-256-Definition (Kodierung, Schlüsselaustausch, ungültige Punkte und Schlüssel) |
| R2 | `basic-toml`, `pin-utils` nicht mehr gepflegt | Hinweis | akzeptiert (keine bekannte Lücke; nur Build-Werkzeug bzw. intern) |

Ausnahmen für `cargo audit` stehen mit Begründung in `core/.cargo/audit.toml`; es sind nur noch die zwei Hinweise
aus R2 (keine Sicherheitslücke).

## Geprüft und in Ordnung

- **Kryptografie der Freigaben:** zufälliger 256-Bit-Schlüssel je Freigabe, AES-256-GCM mit eindeutigen Nonces
  (Präfix zufällig, Zähler je Stück), Kopf und „letztes Stück“ in den Zusatzdaten (erkennt Vertauschen und Abschneiden),
  Passwort zusätzlich über PBKDF2 (600 000 Runden) und HKDF, der Link allein reicht dann nicht. Freigabe-ID
  unabhängig vom Schlüssel. Schlüssel nur im URL-Fragment; die Seite entfernt ihn aus der Adresszeile.
- **Empfängerseite:** keine fremden Inhalte, strenge CSP (`default-src 'none'`, kein Inline-Code), `no-referrer`,
  kein Einbetten (`frame-ancestors 'none'`), Dateiname nur als Text ausgegeben.
- **PHP-Modul:** HMAC-SHA256 über Methode, Parameter, Zeit, Nonce und Body-Hash, zeitkonstanter Vergleich,
  Zeitfenster ±5 min, Einmal-Nonces (Replay-Test grün), Fehlversuch-Sperre, Pfadprüfung ohne `..`, atomares
  Schreiben, zufälliger 96-Bit-Datenordner mit „deny“, Abrufzählung unter Dateisperre (kein doppelter Einmal-Abruf),
  keine Datenbank, keine Ausgabe von Fehlerdetails. PHP-8.1-Syntax wird in der CI geprüft.
- **HTTPS/FTPS:** öffentlich vertrauenswürdige Zertifikate (webpki) oder ein vom Nutzer bestätigter Fingerabdruck;
  `http://` nur für `localhost`. FTPS nur mit TLS (explizit, `PROT P` für Datenkanäle), passive Verbindungen gehen
  immer zum eingetragenen Server (kein „FTP bounce“). Teilen-Adressen nur https.
- **Zugangsdaten:** FTPS-Passwort, PHP-Schlüssel, SSH-Schlüssel und Freigabelinks liegen nur in den verschlüsselten
  Suite-Einstellungen. Der Teilen-Ordner darf nicht im Suite-Ordner liegen.
- **Android:** FileProvider nicht exportiert, nur `cache/shared/`, Leserecht nur für die gewählte App; der Ordner
  wird beim Sperren, bei der Panik-Sperre und beim Start geleert. Kopierte Links: sensible Zwischenablage, automatisch
  geleert. Bildschirmfotos bleiben gesperrt (`FLAG_SECURE`).
- **Modultrennung:** Drive, Pass und Teilen funktionieren ohne GhulMail (Test ohne Mailkonto); GhulMail erhält nur
  einen fertigen Entwurf. Screenshot-Test zeigt das Teilen-Menü mit und ohne GhulMail.

## OpenPGP in GhulMail (Nachtrag)

- **Bibliothek:** rPGP 0.21 (reines Rust, MIT/Apache). Eigene Schlüssel nur Ed25519/X25519 (OpenPGP v4,
  verträglich mit Thunderbird, GnuPG, Proton), Vorlieben SHA-512/256 und AES-256, ohne sie würden andere
  Programme auf SHA-1 und 3DES zurückfallen (beim Test gegen GnuPG aufgefallen und behoben).
- **Abhängigkeiten:** rPGP bringt `rsa` 0.9 (RUSTSEC-2023-0071, „Marvin“) und erneut `crypto-bigint` 0.5
  (RUSTSEC-2026-0387) mit. Beide betreffen Operationen mit *Geheimschlüsseln* dieser Verfahren. GhulMail erzeugt
  und importiert nie RSA- oder NIST-Geheimschlüssel; mit Schlüsseln von Kontakten werden nur öffentliche
  Operationen ausgeführt (an sie verschlüsseln, ihre Signatur prüfen). Ausnahmen begründet in
  `core/.cargo/audit.toml`; sobald rPGP neuere Versionen nutzt, entfallen sie.
- **Geprüft:** Signieren/Verschlüsseln und Entschlüsseln/Prüfen in beide Richtungen gegen GnuPG 2.4 (automatischer
  Test), PGP/MIME byte-genau (RFC 3156) inkl. Manipulation einer einzelnen Stelle → „Signatur passt nicht“,
  geschützter Betreff (außen „...“), Anhänge nur innerhalb der Verschlüsselung, WKD-Hash gegen das Beispiel der
  Spezifikation, Autocrypt nur für die eigene Absenderadresse, Schlüssel aus Mails ersetzen keine importierten
  oder beim Anbieter gefundenen Schlüssel.
- **Transport:** IMAP nur mit TLS von Anfang an (Port 993). SMTP mit TLS (465) oder STARTTLS (587), das bei einem
  Fehlschlag abbricht, nie unverschlüsselt.
- **Grenzen:** Verschlüsselte Mails kann der Server nicht durchsuchen (nur Absender); Autocrypt vertraut dem
  ersten Schlüssel einer Adresse („trust on first use“), für hohe Sicherheit den Fingerabdruck vergleichen
  (Einstellungen → GhulMail).

## Restrisiken und Hinweise für Nutzer

- Wer den **Link** hat, kann die Datei öffnen, für Sensibles ein zusätzliches Passwort setzen und getrennt mitteilen.
- Ein **Einmal-Link** gilt als verbraucht, sobald der Abruf beginnt (bricht die Übertragung ab, ist er trotzdem weg).
- Der **Zugangsschlüssel des PHP-Moduls** steht in der PHP-Datei auf dem Webspace. Wer Dateizugriff auf den Webspace
  hat, kann damit den Speicher nutzen, sieht aber nur verschlüsselte Daten.
- **Sofort-Benachrichtigungen (Android):** Für IMAP IDLE hält ein Dienst je Konto eine TLS-Verbindung offen. Er nutzt
  dieselbe Keystore-geschützte Kopie der Posteingangs-Anmeldung wie bisher (nur bei eingeschalteten
  Benachrichtigungen, Panik-Sperre löscht sie), die Suite bleibt gesperrt, Benachrichtigungen zeigen weiterhin nur
  Kontoname und Anzahl.
- Auf Webservern **ohne `.htaccess`** (nginx) schützt nur der zufällige Name den Datenordner bzw. die zufällige ID das
  Paket; Inhalte bleiben verschlüsselt.
- Die Android-App wurde in dieser Runde im Emulator nur auf fehlerfreien Start geprüft; die Bedienabläufe prüft der
  Testplan auf dem echten Gerät.
