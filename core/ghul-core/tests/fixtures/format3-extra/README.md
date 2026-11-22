# Weitere Dateien im Format 3 (eingefroren)

Geschrieben am 21.11.2026 von der App dieses Stands, zur Ergänzung von `../format3`. Jede spätere
Version muss sie öffnen (Tests in `app::tests::format_freeze`).

- `dokumente.ghldocs`: ein GhulDocs-Paket (Tresor `.ghldoc` mit einem Dokument `probe.txt`)
- `2fa.ghltotp`: ein GhulAuth-Tresor als einzelne Datei
- `kalender.ghlcal`: ein GhulCal-Tresor als einzelne Datei
- `anhang.ghlpss`: ein GhulPass-Tresor mit einem Anhang und einem unbekannten Schlüssel
  (`x_future_field`) im Eintrag und in den Nutzdaten (er bleibt beim Bearbeiten erhalten)
- `key.ghlkey`: die Schlüsseldatei (zweiter Faktor)
- Passwort: `Format-3-bleibt-lesbar!`

Ohne FIDO2-Slot: Der Software-Authenticator der Tests erzeugt bei jedem Lauf neue Schlüssel.

Nur Testdaten, keine echten Daten. **Nie neu erzeugen oder ändern:** Die Prüfsummen stehen im Test.
