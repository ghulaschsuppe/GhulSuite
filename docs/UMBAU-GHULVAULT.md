# Umbau: GhulVault statt GhulDrive und GhulMail

Stand: Entwurf vom 17.11.2026, Grundlage ist der Umbau ohne Synchronisation ([`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)).
Noch nicht umgesetzt; erst nach Freigabe.

## 1. Grundidee

GhulSuite wird eine reine Offline-App. Alles liegt verschlüsselt auf dem Gerät und in der Suite-Datei (`.ghlzip`),
es gibt keine Server, keine Konten und keine Verbindungen nach draußen. Einzige Ausnahme ist die Update-Prüfung, und
die lässt sich abschalten.

Die Bereiche danach:

| Bereich | Inhalt |
|---|---|
| **GhulPass** | Passwörter, Notizen, Karten, Anhänge (wie bisher) |
| **GhulAuth** | 2FA-Codes (wie bisher) |
| **GhulCal** | Kalender, Erinnerungen, ICS-Import und -Export als Datei (wie bisher) |
| **GhulVault** | neu: verschlüsselter Speicher für wichtige Scans und Dokumente, bis 1 GB |

Das Prinzip der Tresore bleibt: Jeder Bereich besteht aus Tresoren (eigene verschlüsselte Dateien), eine Suite kann
mehrere Tresore je Bereich haben, und alle zusammen sind die Suite-Datei.

## 2. Was bleibt

- Formatversion 3 mit Schreibsiegel, Argon2id, zweiter Faktor (Security Key oder Schlüsseldatei), Wiederherstellungscode
- Die Suite-Datei `.ghlzip` mit vollständiger Prüfung vor dem Öffnen (Zähler, Stand-ID, Schutz-Epoche)
- Einzelne Tresore öffnen und speichern, eine Auswahl als eigenen Tresor weitergeben
- Schnell-Entsperren, Panik-Sperre, Auto-Sperre, Bildschirmfoto-Schutz
- Import und Export (Passwort-Manager, 2FA-Apps, ICS)
- Update-Prüfung (abschaltbar, einzige Verbindung)

## 3. Was wegfällt

- **GhulMail** ganz: IMAP/SMTP, Konten, PGP und Autocrypt, Mail-Archiv, Mail-Benachrichtigungen, Dauerdienst,
  „Standard-App für Mail“
- **GhulDrive** ganz: Speicherorte, Index auf dem Server, `index.lock`, Aufräumen, Offline-Kopien
- **Server**: SFTP, der SSH-Schlüssel der Suite, das PHP-Modul, Freigabe-Links und Einmal-Links
- **Schlüsselwechsel** nach einer Schutzänderung (den gab es nur wegen der Server und GhulDrive)
- **Fehlerbericht hochladen**: Der Bericht wird stattdessen als Text angezeigt, zum Kopieren oder zum Teilen über
  eine andere App. `report.php` auf der Website entfällt.
- Android-Berechtigungen, die nur Mail oder Server brauchten (Netzwerkstatus, Dauerdienst für Mail, Akku-Ausnahme).
  `INTERNET` bleibt nur für die Update-Prüfung.

## 4. GhulVault

### 4.1 Ein Tresor wie die anderen

Ein GhulVault-Tresor besteht aus

- der Tresor-Datei `<id>.ghlvlt`: ein Container wie bei GhulPass (gleiche Schlüssel, gleiches Schreibsiegel) mit dem
  **Inhaltsverzeichnis**: Ordner, Dateinamen, Größen, Datum, Typ, Favoriten, Papierkorb und je Datei ihr eigener
  Schlüssel und ihre Prüfsumme;
- je Datei einer **Inhaltsdatei** `vault-<id>/<zufällige Kennung>.ghlblob` mit dem verschlüsselten Inhalt.

Mehrere GhulVault-Tresore sind möglich, z. B. „Wichtige Dokumente“ und „Steuer 2025“. Ein einzelner Tresor lässt sich
wie jeder andere für sich öffnen und speichern, etwa um ihn weiterzugeben (siehe 6).

### 4.2 Verschlüsselung

- Das Inhaltsverzeichnis ist durch den Container geschützt: Argon2id mit Passwort und zweitem Faktor, Schreibsiegel.
- Jede Datei hat einen eigenen zufälligen Schlüssel (32 Byte), der nur im Inhaltsverzeichnis steht.
- Der Inhalt wird in Abschnitten zu 4 MiB mit XChaCha20-Poly1305 verschlüsselt. Jeder Abschnitt ist an die Kennung
  der Datei, seine Nummer und „letzter Abschnitt ja/nein“ gebunden: Abschnitte lassen sich weder vertauschen noch
  weglassen noch an eine andere Datei hängen, ohne dass es auffällt.
- Dateinamen, Ordner und Typen stehen nur im verschlüsselten Inhaltsverzeichnis. Von außen sichtbar sind nur die
  Anzahl der Inhaltsdateien und ihre ungefähre Größe (aufgerundet auf 64 KiB).

### 4.3 Funktionen

- Ordner anlegen, umbenennen, verschieben
- Dateien hinzufügen: aus dem Dateisystem, mehrere auf einmal, auf Android auch per „Teilen mit GhulSuite“ aus
  anderen Apps
- **Ansehen in der App** ohne Klartext auf dem Datenträger: Bilder, PDF, Text
- **Exportieren** (entschlüsselt an einen gewählten Ort, mit Rückfrage) und **Öffnen mit anderer App** (befristete
  Kopie, wird danach gelöscht, wie bisher bei Anhängen)
- Papierkorb, Favoriten, Suche über Namen
- Anzeige der Belegung (z. B. „412 MB von 1 GB“)

### 4.4 Grenzen

- Höchstens **1 GB Inhalt je Suite** (alle GhulVault-Tresore zusammen), eine einzelne Datei ebenfalls bis 1 GB.
  Darüber lehnt die App das Hinzufügen mit einer klaren Meldung ab.
- Keine Fassungen (eine Datei hat einen Inhalt; wer eine neue Fassung hinzufügt, bekommt eine zweite Datei).

## 5. Suite-Datei und Speichern

- **Arbeitskopie:** Auf dem Gerät schreibt die App nur, was sich ändert (eine neue Inhaltsdatei, das
  Inhaltsverzeichnis). Das geht sofort.
- **Suite-Datei:** Beim Speichern wird die ganze `.ghlzip` neu geschrieben. Die Inhaltsdateien sind schon
  verschlüsselt und werden nur hineinkopiert, unkomprimiert. Ihre Prüfsummen stehen schon im Inhaltsverzeichnis und
  müssen nicht neu berechnet werden.
- **Nichts im Arbeitsspeicher:** Schreiben und Prüfen der `.ghlzip` laufen als Datenstrom. Heute wird die Datei ganz
  in den Speicher gelesen; das muss für 1 GB auf dem Handy geändert werden.
- Größte Suite-Datei: 1,25 GiB (1 GB Inhalt plus die übrigen Tresore).
- Speichern und Öffnen zeigen einen Fortschritt.
- Beim Speichern wird zuerst die neue Datei geschrieben und dann die alte ersetzt: kurzzeitig ist doppelter Platz nötig.

Geschätzte Dauer beim Speichern mit vollem GhulVault (1 GB):

| Ziel | Dauer |
|---|---|
| Windows, SSD | etwa 2 bis 5 s |
| Handy, interner Speicher | etwa 5 bis 12 s |
| USB-Stick oder SD-Karte | etwa 40 s bis 1,5 min |

Mit typischen 100 bis 300 MB ist es ein Bruchteil davon.

## 6. Einen GhulVault-Tresor weitergeben

Ein einzelner GhulVault-Tresor wird wie die anderen Einzeltresore gespeichert, aber als `.ghlzip` mit nur diesem
Tresor (Inhaltsverzeichnis und Inhaltsdateien), mit eigenem Passwort. Der Empfänger öffnet ihn mit GhulSuite.

## 7. Bedrohungsmodell (Kurzfassung)

| Wer | Was er hat | Was er erreicht |
|---|---|---|
| Finder des Handys oder Sticks | die Suite-Datei oder die Arbeitskopie | nichts ohne Passwort **und** zweiten Faktor |
| Jemand mit dem alten Passwort | eine Kopie der Suite-Datei von **vor** einer Passwortänderung | öffnet diese alte Kopie. Ohne Server lässt sich eine Kopie nachträglich nicht sperren (wie bei VeraCrypt). Die App sagt nach jeder Schutzänderung deutlich: alte Kopien löschen |
| Schadsoftware auf dem entsperrten Gerät | alles, was gerade angezeigt wird | nicht verhinderbar (Grenze, wie bisher) |

Klartext entsteht nur beim Ansehen (im Arbeitsspeicher) und beim Exportieren oder Öffnen mit einer anderen App
(befristete Kopie). Ein unabhängiges Sicherheitsaudit gibt es weiterhin nicht (Grenze L2 in `AUDIT.md`).

## 8. Folgen für die offenen Befunde

Mit Mail, Drive und Servern entfallen rund 70 der 146 offenen Befunde, darunter drei der fünf hohen (H2, H5, H7).
Es bleiben vor allem Sperren und Entsperren, Suite-Dateien, GhulCal (u. a. H8), GhulPass-Import (H4), Windows und
Update.

## 9. Umsetzung in Paketen

Auf dem Branch `feature/ghulvault` (vom Umbau-Branch), am Ende eine PR nach `dev`.

| Paket | Inhalt |
|---|---|
| G1 | GhulMail raus (Kern, App, Android-Dienste und -Berechtigungen, Tests, Emulator-Abläufe) |
| G2 | GhulDrive, Server, PHP-Modul, Freigaben und Schlüsselwechsel raus; Fehlerbericht ohne Hochladen |
| G3 | Suite-Datei als Datenstrom, bis 1,25 GiB, mit Fortschritt |
| G4 | GhulVault im Kern: Tresor-Art, Inhaltsverzeichnis, Inhaltsdateien, Grenzen, Tests (auch Angriffe) |
| G5 | GhulVault in der App: Liste, Ordner, Hinzufügen, Ansehen, Exportieren, Papierkorb, Belegung |
| G6 | Doku (FORMAT, KONZEPT, AUDIT, Testpläne), README, Website, Emulator-Abläufe |
| G7 | Unabhängige Durchsicht, Befunde beheben, PR |

## 10. Offene Fragen

1. Name in der App: „GhulVault“ als Bereich, aber „Tresor“ heißt in der App schon die einzelne Datei eines Bereichs.
   Passt das, oder soll GhulVault anders heißen (z. B. „GhulDocs“)?
2. Dokumente direkt mit der Kamera scannen (zuschneiden, als PDF), oder nur vorhandene Dateien hinzufügen?
3. Bleibt die 1-GB-Grenze fest, oder soll sie einstellbar sein?
