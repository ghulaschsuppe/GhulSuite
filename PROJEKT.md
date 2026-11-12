# GhulSuite – Projekt- und Entwicklerhinweise

Diese Datei ist der Einstieg für alle, die am Code arbeiten (Menschen wie KI-Assistenten).
Was GhulSuite ist und kann, steht in der [README](README.md), das vollständige Konzept in
[`docs/KONZEPT.md`](docs/KONZEPT.md), das Dateiformat in [`docs/FORMAT.md`](docs/FORMAT.md).

## Grundsätze

- **Sicherheit zuerst, dicht gefolgt von Benutzerfreundlichkeit.** Alles wird auf dem Gerät verschlüsselt;
  Server sehen nur Chiffretext.
- **Selbst hostbar, ohne bestimmten Anbieter:** beliebige IMAP/SMTP-Mailserver, eigene Speicher per SFTP, FTPS
  oder das GhulSuite-Server-Modul. Das Server-Modul läuft auf **Standard-Webhosting mit PHP 8.1, ohne Datenbank**.
- **Der Ordner ist die Suite:** `.ghlpss` (Passwörter) und `.ghltotp` (2FA) im Ordner gehören dazu, `.ghlsuite`
  hält Einstellungen, Mail-Konten und den Drive-Schlüssel, `.ghlkey` ist eine Schlüsseldatei.
- **Schutz:** Master-Passwort **plus** zweiter Faktor (Security Key oder Schlüsseldatei); der Wiederherstellungscode
  ist Pflicht und steckt in jeder Datei.
- **Plattformen:** Android und Windows (auch portabel), eine gemeinsame Oberfläche. Kein Web-Client.
- **Sprachen:** Deutsch und Englisch (`app/composeApp/src/commonMain/composeResources/values*/strings.xml`).
- **Lizenz:** AGPL-3.0-or-later mit Zusatzbedingung (Urhebervermerk bleibt sichtbar), siehe [`LICENSE`](LICENSE)
  und [`NOTICE`](NOTICE). Name und Logo: [`TRADEMARKS.md`](TRADEMARKS.md). Beiträge: [`CONTRIBUTING.md`](CONTRIBUTING.md).
- **Keine Geheimnisse im Repository** – keine Zugangsdaten, keine Signaturschlüssel, keine echten Daten.

## Aufbau

```
core/ghul-core/        Rust-Kern: Krypto, FIDO2/CTAP2, Dateiformat, Suite-API, Sync, Drive, Mail, PGP, Teilen
core/ghul-core/server/ PHP-Server-Modul (ghulsuite.php, PHP 8.1, ohne Datenbank)
core/fuzz/             Fuzzing-Ziele (cargo-fuzz)
app/                   Kotlin + Compose Multiplatform (Android und Windows), Anbindung per UniFFI
  composeApp/src/commonMain   gemeinsame Oberfläche und Texte
  composeApp/src/jvmShared    gemeinsamer Code Android/Windows (Kern-Anbindung)
  composeApp/src/androidMain  Android (USB/NFC-Key, Autofill, Benachrichtigungen)
  composeApp/src/desktopMain  Windows (WebAuthn, Fenster, Pakete)
  composeApp/src/desktopTest  FFI-, PDF- und Screenshot-Tests
app/maestro/           Oberflächen-Abläufe für Emulator oder Gerät (Maestro)
core/ghul-core/share-viewer/  Empfängerseite für Freigabelinks
public/                Testseiten und Designvorschau für den Testserver
docs/                  Konzept, Format, Audit, Testpläne, Testvektoren, Bilder
tools/                 Testserver (SFTP/IMAP/SMTP), Prüfskripte, Emulator-Skripte
.github/               Workflows und FTPS-Upload-Action
```

## Bauen und testen

- Voraussetzungen: Rust (Ziele `aarch64-linux-android`, `x86_64-linux-android`), `cargo-ndk`, Android SDK mit
  NDK 28.2.13676358, JDK 17+.
- Kern: `cd core && cargo fmt --all && cargo clippy -p ghul-core --all-targets -- -D warnings && cargo test -p ghul-core`.
  Netzwerk-Tests gegen echte lokale Server: vorher `eval "$(sudo -E tools/test-servers.sh)"`.
- App: `cd app && ./gradlew :composeApp:desktopTest :composeApp:assembleDebug` (baut Kern und Anbindung mit).
- Oberfläche im Emulator: `tools/run-maestro.sh <apk>`, unter Windows `maestro test app\maestro`.
- Manuelle Tests: [`docs/TESTPLAN.md`](docs/TESTPLAN.md) (Android), [`docs/TESTPLAN-WINDOWS.md`](docs/TESTPLAN-WINDOWS.md).

## CI und Testserver

- `.github/workflows/android.yml`: Kern (Format, Clippy, Tests, Fuzzing, Audit), Netzwerk-Tests, Browser-Tests,
  Windows-Pakete (MSI und portabel), APK, Emulator-Abläufe. Artefakte: `ghulsuite-dev`, `ghulsuite-windows-dev`,
  `screenshots`, `emulator-tests`.
- Uploads auf den Testserver des Inhabers laufen **nur im Original-Repository** und nie bei Pull Requests.
  Ziel ist der Ordner `ghulsuite/` (Testseiten) bzw. `ghulsuite/download/` (App-Pakete).
- **Secrets** (vom Inhaber selbst eingetragen, niemals im Repository): `FTP_SERVER`, `FTP_USER`, `FTP_PASSWORD`,
  `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`.
- **Signatur:** Der Android-Schlüssel kommt nur aus den Secrets (`GHUL_KEYSTORE_FILE`, `GHUL_KEYSTORE_PASSWORD`,
  `GHUL_KEY_ALIAS` in `app/composeApp/build.gradle.kts`). Ohne ihn signiert Gradle mit dem lokalen Debug-Schlüssel.

## Regeln für Änderungen

- Vor jedem Commit: `cargo fmt`, Clippy (auch `--target x86_64-pc-windows-gnu`), Kern-Tests, Desktop-Tests.
- Neue Texte immer auf Deutsch **und** Englisch.
- Neue Funktionen bekommen Tests im Kern und einen Eintrag im Testplan.
- Kommentare und Commit-Nachrichten auf Englisch, Dokumentation auf Deutsch.
- Neue Quelldateien beginnen mit `// SPDX-License-Identifier: AGPL-3.0-or-later`.
