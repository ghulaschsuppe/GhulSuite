# GhulSuite: Projekt- und Entwicklerhinweise

Diese Datei ist der Einstieg für alle, die am Code arbeiten (Menschen wie KI-Assistenten).
Was GhulSuite ist und kann, steht in der [README](README.md), das vollständige Konzept in
[`docs/KONZEPT.md`](docs/KONZEPT.md), das Dateiformat in [`docs/FORMAT.md`](docs/FORMAT.md). Warum es keine
Synchronisation mehr gibt, steht in [`docs/UMBAU-OHNE-SYNC.md`](docs/UMBAU-OHNE-SYNC.md); warum GhulDocs GhulDrive und
GhulMail ersetzt und die App ohne Server auskommt, in [`docs/UMBAU-GHULDOCS.md`](docs/UMBAU-GHULDOCS.md).

## Grundsätze

- **Funktionsstopp seit 21.11.2026 (Entscheidung des Inhabers):** Es kommen keine neuen Funktionen mehr dazu, nur
  noch Fehlerbehebungen und kleinere Anpassungen an Design und Dialogen. Ziel ist eine Version, die man der
  Öffentlichkeit sicher geben kann (Gutachten und Weg dorthin: [`docs/PRUEFCHRONIK.md`](docs/PRUEFCHRONIK.md)).
- **Sicherheit zuerst, dicht gefolgt von Benutzerfreundlichkeit.** Alles wird auf dem Gerät verschlüsselt;
  die Suite-Datei enthält nur Chiffretext.
- **Offline, ohne bestimmten Anbieter:** keine Server, keine Konten, kein Cloud-Dienst. Nach draußen gehen nur die
  Update-Prüfung (GitHub) und die Leak-Prüfung von GhulPass (Pwned Passwords, k-Anonymität); beide sind abschaltbar.
  Fehlerberichte werden nur angezeigt, nie hochgeladen.
- **Vier Bereiche in einer App:** GhulPass (Passwörter), GhulAuth (2FA), GhulDocs (Dokumente, bis zu 3 Tresore
  mit je bis 1 GB, mit Scannen auf Android), GhulCal (Kalender). Je Bereich kann es mehrere Tresore geben (GhulDocs bis zu 3); jeder
  lässt sich exportieren (für sich selbst) oder als Tresor weitergeben (für jemand anderen, mit eigenem Schutz).
- **Die Suite ist eine Datei, ohne Synchronisation:** Die Suite-Datei `.ghlzip` (bis 3,75 GiB, unter 4 GiB wegen
  ZIP ohne ZIP64 und FAT32, als Datenstrom geschrieben und gelesen) enthält die Tresore (`.ghlpss` Passwörter,
  `.ghltotp` 2FA, `.ghldoc` Dokumente mit ihren Inhaltsdateien `.ghlblob`, `.ghlcal` Kalender),
  `suite.ghlsuite` (Einstellungen, Schutz-Epoche) und die Kopfdatei `export.ghlexp`. Öffnen prüft alles, bevor etwas ersetzt wird; Speichern schreibt die Arbeitskopie zurück.
  Abgeglichen oder zusammengeführt wird nichts: Aktuell ist der Stand, der zuletzt gespeichert wurde. `.ghlkey` ist
  eine Schlüsseldatei, `.ghldocs` ein einzelner GhulDocs-Tresor mit seinen Dokumenten (GhulDocs-Paket); hinzugefügt
  wird es ein eigener GhulDocs-Tresor der Suite.
- **Schutz:** Master-Passwort **plus** zweiter Faktor (Security Key oder Schlüsseldatei); der Wiederherstellungscode
  ist Pflicht und steckt in jeder Datei.
- **Plattformen:** Android und Windows (auch portabel), eine gemeinsame Oberfläche. Kein Web-Client.
- **Sprachen:** Deutsch und Englisch (`app/composeApp/src/commonMain/composeResources/values*/strings.xml`).
- **Lizenz:** AGPL-3.0-or-later mit Zusatzbedingung (Urhebervermerk bleibt sichtbar), siehe [`LICENSE`](LICENSE)
  und [`NOTICE`](NOTICE). Name und Logo: [`TRADEMARKS.md`](TRADEMARKS.md). Beiträge: [`CONTRIBUTING.md`](CONTRIBUTING.md).
- **Keine Geheimnisse im Repository**: keine Zugangsdaten, keine Signaturschlüssel, keine echten Daten.

## Aufbau

```
core/ghul-core/        Rust-Kern: Krypto, FIDO2/CTAP2, Dateiformat, Suite-API, Suite-Datei (Datenstrom), GhulDocs,
                       Kalender, Update- und Leak-Prüfung
                       (je Bereich Tresore: .ghlpss, .ghltotp, .ghldoc + .ghlblob, .ghlcal; dazu suite.ghlsuite)
core/fuzz/             Fuzzing-Ziele (cargo-fuzz)
app/                   Kotlin + Compose Multiplatform (Android und Windows), Anbindung per UniFFI
  composeApp/src/commonMain   gemeinsame Oberfläche und Texte
  composeApp/src/jvmShared    gemeinsamer Code Android/Windows (Kern-Anbindung)
  composeApp/src/androidMain  Android (USB/NFC-Key, Benachrichtigungen, Kamera zum Scannen, PDF-Anzeige)
  composeApp/src/desktopMain  Windows (WebAuthn, Fenster, Pakete, PDF-Anzeige mit PDFBox)
  composeApp/src/desktopTest  FFI-, GhulDocs-, Scan-, Layout- und Screenshot-Tests
app/maestro/           Oberflächen-Abläufe für Emulator oder Gerät (Maestro)
public/                Testseiten für den Testserver (WebAuthn-Test, Download-Seite) und die alte Designvorschau
docs/                  Konzept, Format, Audit, Testpläne, Testvektoren, Bilder
tools/                 Prüfskripte, Website-Bau, Emulator-Skripte
.github/               Workflows und FTPS-Upload-Action (nur für Testserver und Website, nicht Teil der App)
```

## Bauen und testen

- **Vor jedem Push: `tools/precheck.sh`.** Führt alle Prüfungen der CI aus, die lokal gehen (Texte, Testvektoren,
  Format, Clippy für Linux und Windows, alle Kerntests, Website, Workflows, signierte APK mit Lint,
  Desktop-Paketierung mit den Gradle-Prüfsummen), bricht beim ersten Fehler **nicht** ab und listet am Ende alle
  Fehler mit Log. Erst wenn alles grün ist, pushen; mehrere Korrekturen gesammelt pushen. `--quick` lässt Gradle
  weg, `--full` nimmt die langen Release-Tests dazu. Nur Windows selbst und der Emulator bleiben der CI.
- **Emulator-Abläufe (Maestro) vor dem Push mitdenken.** Sie laufen nur in der CI. Deshalb gilt:
  1. `tools/check-maestro.py` (läuft im Precheck mit) prüft, dass jeder Text, den ein Ablauf in `app/maestro`
     antippt, abwartet oder sucht, ein Text der App ist. Texte des Systems oder der Testdaten stehen in
     `tools/maestro-texts.txt`.
  2. **Ändert sich Verhalten oder Ablauf in der Oberfläche** (ein Schritt, ein Dialog, ein Knopf kommt dazu oder
     fällt weg, etwas bleibt erhalten statt gelöscht zu werden), werden alle Abläufe nach den betroffenen Texten
     und Schritten durchsucht (`grep -rn "<Text>" app/maestro`) und jeder Treffer Schritt für Schritt gegen das
     neue Verhalten gelesen und angepasst, **vor** dem Push. Das Skript sieht nur Texte, keine Reihenfolge.
- Voraussetzungen: Rust (Ziele `aarch64-linux-android`, `x86_64-linux-android`), `cargo-ndk`, Android SDK mit
  NDK 28.2.13676358, JDK 17+.
- Kern: `cd core && cargo fmt --all && cargo clippy -p ghul-core --all-targets -- -D warnings && cargo test -p ghul-core`.
- App: `cd app && ./gradlew :composeApp:desktopTest :composeApp:assembleDebug` (baut Kern und Anbindung mit).
- Oberfläche im Emulator: `tools/run-maestro.sh <apk>`, unter Windows `maestro test app\maestro`.
- Weitere Prüfungen: `tools/check-texts.py`, `tools/check-android-manifest.py`, `tools/verify_testvectors.py`; nachts
  läuft `.github/workflows/nightly.yml` (Fuzzing lang, Langläufe, Monkey, x509-limbo, MobSF).
- Bilder für README und Website: `ScreenshotTest` (und `DocScanTest` für das Zuschneiden) schreiben nach
  `app/composeApp/build/screenshots/`; auf die halbe Größe verkleinert (Handy 412 × 892, Windows 1280 × 820) liegen sie
  in `docs/images/`.
- Manuelle Tests: [`docs/TESTPLAN.md`](docs/TESTPLAN.md) (Android), [`docs/TESTPLAN-WINDOWS.md`](docs/TESTPLAN-WINDOWS.md).

## CI und Testserver

- `.github/workflows/release.yml`: stabile Versionen (`main`) und Vorabversionen (`dev`) als GitHub-Releases,
  siehe „Versionen und Veröffentlichung“.
- `.github/workflows/android.yml`: Kern (Format, Clippy, Tests, Fuzzing, Audit), Windows-Pakete (MSI und portabel),
  APK, Emulator-Abläufe. Format, Clippy, Tests und der App-Build laufen auch nach einem Fehler weiter, damit ein Lauf
  alle Fehler auf einmal zeigt. Artefakte: `ghulsuite-dev`, `ghulsuite-windows-dev`, `screenshots`, `emulator-base`.
- **Emulator-Abläufe** (langsam, weil ein ganzes Android startet) laufen nicht bei jedem Push eines Arbeits-Branches,
  sondern bei jedem Pull Request, auf `main` und nachts, in einem Teil (`base`; Testserver für SFTP und Mail gibt es nicht mehr). Die
  Kamera hat keinen Ablauf im Emulator; das Scannen prüfen die Desktop-Tests. Von Hand für jeden Branch:
  `.github/workflows/emulator.yml` („Run workflow“, nutzt die zuletzt gebaute APK des Branches).
- **Kein doppelter Lauf:** Ein Push auf einen Arbeits-Branch mit offenem, konfliktfreiem Pull Request startet in
  `android.yml` keine Jobs; der Lauf des Pull Requests prüft dieselben Commits (zusammengeführt mit dem Ziel-Branch)
  und lädt sie hoch. Ohne Pull Request, bei Konflikten, wenn die Abfrage scheitert sowie auf `main` und `dev` läuft
  der Push wie gewohnt. Die Pflicht-Checks `build` und `Windows-App (Installer und portable Version)` sind die
  Sammel-Jobs `build-check` und `windows-check`: sie laufen in jedem Lauf (auch bei reinen Doku-Änderungen, sonst
  käme der Check nie an und der Pull Request bliebe blockiert) und sind grün, wenn der eigentliche Job grün war oder
  übersprungen wurde (keine passenden Änderungen oder Push-Lauf neben einem Pull Request). Im Push-Lauf eines Arbeits-Branches heißen sie `build (Push)` und
  `Windows-App (Push)`: die Pflicht-Checks kommen so nur aus dem Lauf des Pull Requests.
  Dessen APK ist darum mit dem echten Schlüssel signiert und nicht debuggable: der Emulator-Schritt „vom System
  beendet“ beendet den Prozess dann als root (`su` im google_apis-Abbild) statt über `run-as`.
- Uploads auf den Testserver des Inhabers laufen **nur im Original-Repository**: bei Pushes und bei Pull Requests
  aus diesem Repository (dieselbe Vertrauensstufe wie ein Push), **nie bei Pull Requests aus Forks**.
  Ziel ist für `main` der Ordner `ghulsuite/` (Testseiten) bzw. `ghulsuite/download/` (App-Pakete), für jeden
  anderen Branch `ghulsuite/branches/<branch>/` bzw. `…/download/` (im Lauf eines Pull Requests der Ordner seines
  Branches; Pull Requests von `main` oder `dev` laden nichts hoch, das tun deren Push-Läufe); so bleibt
  `ghulsuite/download/` der stabile Stand.
- **Secrets** (vom Inhaber selbst eingetragen, niemals im Repository): `FTP_SERVER`, `FTP_USER`, `FTP_PASSWORD`,
  `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`.
- **Signatur:** Der Android-Schlüssel kommt nur aus den Secrets (`GHUL_KEYSTORE_FILE`, `GHUL_KEYSTORE_PASSWORD`,
  `GHUL_KEY_ALIAS` in `app/composeApp/build.gradle.kts`). Ohne ihn signiert Gradle mit dem lokalen Debug-Schlüssel.
  **Ein Schlüssel für alles** (Entwicklerversionen und Veröffentlichungen, Begründung in `docs/AUDIT.md` 4.1). Die
  Keystore-Datei und ihr Passwort gehören zusätzlich offline gesichert: Ohne sie lässt sich keine installierte App
  mehr aktualisieren.

## Versionen und Veröffentlichung

- **Zwei Kanäle.** `main` ist der freigegebene Stand: Jede neue Version dort wird zur **stabilen Version** `v0.6.0`
  (GitHub-Release, „latest“). `dev` ist der Zwischenschritt: Jeder Push dort wird zur **Vorabversion**
  `v0.6.0-dev.<n>` (GitHub-Pre-Release, die neuesten fünf bleiben). Beides baut und signiert
  `.github/workflows/release.yml`; die README verlinkt die Dateien, die App holt daraus ihre Updates.
  **Erst nach grüner CI:** Der Release startet, wenn der Lauf von `android.yml` für genau diesen Commit grün endete
  (`workflow_run`), prüft das selbst noch einmal und gibt nur eine neue Version heraus, wenn sich an der App etwas
  geändert hat. Die Rust-Version steht in `rust-toolchain.toml` (für lokal, CI und Release gleich).
- **Versionsnummern ohne Handarbeit:** Ablauf: Arbeits-Branches per PR nach `dev`, dort Vorabversionen testen; zum
  Freigeben PR `dev` → `main`. Die Nummer ergibt sich von selbst: nach `v0.5.0` heißt die nächste Version `0.6.0`
  (auf `dev` `0.6.0-dev.1`, `-dev.2` …, bis `main` sie veröffentlicht). **`VERSION`** (Wurzel) braucht man nur für
  einen Sprung (`1.0.0`) oder eine Fehlerbehebung (`0.5.1`); ist die Version darin schon veröffentlicht oder älter,
  zählt der Workflow selbst weiter. Jeder Merge nach `main`, der App, Kern oder `VERSION` ändert, wird eine stabile
  Version.
- **Versionsnummern der Pakete** (`app/composeApp/build.gradle.kts`): Android `versionCode` = Version × 100 plus
  laufende Nummer (Vorabversionen 0 bis 98, die stabile 99), Windows-MSI `Major.Minor.(Patch × 100 + Nummer)`. So
  ist jede Version höher als die vorige und eine stabile höher als alle ihre Vorabversionen.
- **Signierte Prüfsummen:** Jedes Release enthält `SHA256SUMS` und `SHA256SUMS.sig` (signiert im Workflow mit dem
  Signaturschlüssel der App). Der öffentliche Teil steckt als `BuildInfo.UPDATE_KEY` in der App (Gradle-Variable
  `GHUL_UPDATE_KEY`); ohne ihn (lokale Builds, F-Droid, Forks) aktualisiert sich die App nicht selbst.
- **Updates in der App** (`core/ghul-core/src/update.rs`, `AppUpdater`): nur `https` zu GitHub, nur signierte
  Prüfsummen, Größe und SHA-256 vor der Installation. Android über den System-Installer (prüft zusätzlich den
  Signaturschlüssel), Windows-MSI über `msiexec`, portabel nur die Release-Seite. Testversionen (`.dev`) und
  Installationen aus einem Store aktualisieren sich nicht selbst.
- **F-Droid:** IzzyOnDroid kann die APKs direkt aus den Releases übernehmen (gleiche Signatur). Das offizielle F-Droid
  baut selbst, ohne `GHUL_UPDATE_KEY`: Der Updater ist dort von selbst aus. Mit reproduzierbaren Builds (Issue #6)
  kann F-Droid unser signiertes APK ausliefern.

## Website (www.ghulsuite.de)

- Quellen in `website/`: `templates/` (Seitenrahmen, Startseite), `i18n/de.json` und `en.json` (Texte der
  Startseite), `pages/<sprache>/` (Impressum und Datenschutzerklärungen), `static/` (CSS, JavaScript),
  `htaccess-live`. Gebaut mit `tools/build-website.py`: setzt Sprachen und Seiten zusammen und trägt Versionen,
  Daten, Dateigrößen (GitHub-Releases) und Kennzahlen (Testprotokoll, Sicherheitsbericht) selbst ein.
- **Rechtliche Angaben nur aus Secrets:** `IMPRESSUM_NAME`, `IMPRESSUM_ADRESSE` (mehrzeilig), `IMPRESSUM_EMAIL`,
  `IMPRESSUM_HOSTER` (mehrzeilig: Name, Anschrift). Ohne die ersten drei wird die Live-Seite nicht gebaut.
- `.github/workflows/website.yml`: `main` → Live-Seite (`WEBSITE_FTP_SERVER`, `WEBSITE_FTP_USER`,
  `WEBSITE_FTP_PASSWORD`), `dev` und andere Branches → Vorschau auf dem Testserver (`ghulsuite/website/` bzw.
  `ghulsuite/branches/<branch>/website/`, nicht für Suchmaschinen). Nach jedem Release wird beides neu gebaut.
- Die Seite setzt keine Cookies und lädt nichts von fremden Servern (keine Schriften, keine Skripte); die
  Content-Security-Policy in `htaccess-live` verbietet es auch. Neue Funktionen der App kommen mit dem
  jeweiligen PR in die Texte (wie bei der README). Ändert sich, was die App an Verbindungen aufbaut, wird
  `pages/*/datenschutz-app.html` bzw. `privacy-app.html` im selben PR angepasst.
- **Fehlerberichte aus der App** zeigt die App nur an; der Nutzer schickt sie selbst (z. B. mit seiner Mail-App). Die
  Website nimmt keine Daten an (`report.php` ist entfernt). Auf dem Webspace der Live-Seite liegen ältere Dateien,
  bis sie von Hand gelöscht werden: Der Upload ergänzt und ersetzt, löscht aber nichts.
- Lokal ansehen: `tools/build-website.py --out /tmp/site --preview` und den Ordner mit einem Webserver öffnen.

## Arbeitsweise: Branches, Pull Requests, Issues

- `main` ist immer lauffähig und geschützt: Änderungen kommen nur über einen **Pull Request** mit grünen Tests hinein.
- Jede Aufgabe bekommt einen eigenen **Branch** (`feature/…`, `fix/…`, `ci/…`, `docs/…`) und einen Pull Request,
  der das zugehörige **Issue** nennt (`Closes #12`). Ziel der Pull Requests ist `dev`, sobald es den Zweig gibt
  (siehe „Versionen und Veröffentlichung“).
- Getestet wird der Branch über die CI und die App-Pakete aus seinem Lauf (Artefakte oder der Branch-Ordner auf dem
  Testserver). Zusammengeführt wird nach Durchsicht durch den Inhaber.

## Regeln für Änderungen

- Vor jedem Commit: `cargo fmt`, Clippy (auch `--target x86_64-pc-windows-gnu`), Kern-Tests, Desktop-Tests.
- Neue Texte immer auf Deutsch **und** Englisch.
- **Keine langen Striche** (Geviert- und Halbgeviertstrich) in Texten der App und des Projekts: stattdessen Punkt,
  Komma, Doppelpunkt, Klammern oder „bis“; für Zeitspannen der einfache Bindestrich. `tools/check-texts.py` prüft das.
- Die Bereiche werden überall in derselben Reihenfolge genannt: Passwörter, 2FA, Dokumente, Kalender.
- Neue Funktionen bekommen Tests im Kern und einen Eintrag im Testplan.
- **Die README ist die Hauptseite des Projekts und bleibt aktuell:** Jeder Pull Request, der etwas ändert, das Nutzer
  oder Mitlesende betrifft (Funktionen, Bereiche, Sicherheit, Verbindungen, Plattformen, Download, Stand der
  Prüfungen), passt die README im selben Pull Request an. Vor dem Merge kurz gegenlesen: Stimmt noch, was dort steht?
  Die README verspricht nichts, was der Code nicht hält; bekannte offene Punkte stehen in
  [`docs/TESTPROTOKOLL.md`](docs/TESTPROTOKOLL.md).
- Gefundene Fehler und offene Punkte kommen mit Nummer ins Testprotokoll; Testkonzept: [`docs/TESTKONZEPT.md`](docs/TESTKONZEPT.md).
- **Getestet wird wie auf dem Handy.** Jede Änderung an der Oberfläche läuft vor dem Merge im Android-Emulator
  (`emulator.yml`, bei jedem Pull Request automatisch). Neue Funktionen bekommen einen Maestro-Ablauf in
  `app/maestro/` mit einem Bild nach jedem Schritt (wo der Emulator es nicht kann, wie bei der Kamera, einen
  Desktop-Test mit Ersatz).
  Testattrappen im Desktop-Test verhalten sich wie der echte Kern (Wartezeit, Abbruch erst nach der Antwort,
  gespeicherte Werte bleiben gespeichert); eine Attrappe, die alles sofort und fehlerfrei beantwortet, beweist nichts.
- **Bilder werden angesehen, nicht nur erzeugt.** Ein Pull Request ist erst fertig, wenn die Bilder aller Abläufe
  (Emulator und Desktop) durchgesehen sind. Dabei zählt nicht nur, ob etwas kaputt ist, sondern jede
  **Unstimmigkeit**: unterschiedliche Abstände, Schriftgrößen, Farben oder Ecken für dasselbe Element, abweichende
  Begriffe für dieselbe Sache, abgeschnittene oder umgebrochene Texte, schief ausgerichtete Elemente, doppelte
  Überschriften, Platzhalter wie „…“ oder „0“, wo ein Wert stehen müsste, leere oder halbe Zustände, technische
  Fehlertexte, Knöpfe ohne Wirkung, Probleme im dunklen Design und im Querformat. Jeder Fund kommt mit Nummer ins
  Testprotokoll und wird im selben Pull Request behoben oder ausdrücklich zurückgestellt.
- **Vor jedem Push eine unabhängige Durchsicht:** Ein zweiter Prüfer, der die Änderung nicht geschrieben hat, liest
  den Diff gezielt auf Fehler (verworfene oder doppelte Aktionen, Abbruch als Fehler angezeigt, fehlende
  Fehlerfälle, Speicher- und Lebenszyklusfehler). Was er findet, wird vor dem Push behoben oder begründet verworfen.
- Neue Bildschirme kommen in `LayoutCheckTest` (drei Handys, zwei Sprachen); gemeinsame Bausteine (`ButtonRow`,
  `LabelValueRow`, `ChipRow`, `SegLabel`, `ShrinkText`) statt fester Breiten nebeneinander.
- Kommentare und Commit-Nachrichten auf Englisch, Dokumentation auf Deutsch.
- Neue Quelldateien beginnen mit `// SPDX-License-Identifier: AGPL-3.0-or-later`.
