# Dateien im Format 3 (eingefroren)

Geschrieben am 21.11.2026 von der App dieses Stands. Jede spätere Version muss sie öffnen
(Test `app::tests::format_freeze`, Zusage in `docs/FORMAT.md`, Abschnitt „Kompatibilitätszusage“).

- `suite.ghlzip`: Suite-Datei mit GhulPass, GhulAuth, GhulCal und GhulDocs (je ein Eintrag, ein Dokument)
- `privat.ghlpss`: der GhulPass-Tresor als einzelne Datei
- `key.ghlkey`: die Schlüsseldatei (zweiter Faktor)
- `recovery-code.txt`: der Wiederherstellungscode
- Passwort: `Format-3-bleibt-lesbar!`

Nur Testdaten, keine echten Daten. **Nie neu erzeugen oder ändern:** Die Prüfsummen stehen im Test.
Für ein neues Format kommt ein neuer Ordner dazu, dieser bleibt.
