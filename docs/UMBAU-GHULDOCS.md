# Umbau: GhulDocs statt GhulDrive und GhulMail

Stand: 17.11.2026, freigegeben. Grundlage ist der Umbau ohne Synchronisation
([`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)). Umsetzung auf dem Branch `feature/ghulvault`.

## 1. Grundidee

GhulSuite wird eine reine Offline-App. Alles liegt verschlüsselt auf dem Gerät und in der Suite-Datei (`.ghlzip`),
es gibt keine Server, keine Konten und keine Verbindungen nach draußen. Einzige Ausnahme ist die Update-Prüfung, und
die lässt sich abschalten.

| Bereich | Inhalt |
|---|---|
| **GhulPass** | Passwörter, Notizen, Karten, Anhänge |
| **GhulAuth** | 2FA-Codes |
| **GhulCal** | Kalender, Erinnerungen, ICS-Import und -Export als Datei |
| **GhulDocs** | neu: verschlüsselter Speicher für wichtige Scans und Dokumente, bis 1 GB |

Das Prinzip der Tresore bleibt: Jeder Bereich besteht aus Tresoren (eigene verschlüsselte Dateien), eine Suite kann
mehrere Tresore je Bereich haben, und alle zusammen sind die Suite-Datei.

## 2. Keine Altlasten

Die App ist in Entwicklung und hat keine Nutzer. Deshalb:

- Kein Übergang und keine Übernahme alter Daten. Ältere Formate (Tresore, Indexe, Einstellungen) werden nicht mehr
  gelesen, der Code dafür fällt weg.
- Eine **Schutzänderung** (neues Passwort, neuer Wiederherstellungscode, Key oder Schlüsseldatei entfernt) erneuert
  alle inneren Schlüssel. Die App öffnet danach keine Suite-Datei mit älterem Schutz mehr, auch nicht nach Rückfrage.

**Was technisch nicht geht:** Eine Kopie der Suite-Datei, die schon irgendwo liegt (Stick, Cloud, anderes Gerät),
lässt sich ohne Server nicht nachträglich unbrauchbar machen. Wer das alte Passwort **und** den alten zweiten
Faktor hat, öffnet sie mit einer frischen Installation (wie bei VeraCrypt). Deshalb:

- Die App sagt nach jeder Schutzänderung deutlich: Alte Kopien der Suite-Datei löschen.
- Sicher unbrauchbar werden alte Kopien, wenn der zweite Faktor getauscht wird: neuen Security Key oder neue
  Schlüsseldatei hinzufügen, den alten entfernen und die alte Schlüsseldatei vernichten bzw. den alten Key
  zurücksetzen. Ohne den alten Faktor öffnet keine alte Kopie mehr, auch nicht mit dem alten Passwort.

## 3. Was bleibt

- Formatversion 3 mit Schreibsiegel, Argon2id, zweiter Faktor (Security Key oder Schlüsseldatei), Wiederherstellungscode
- Die Suite-Datei `.ghlzip` mit vollständiger Prüfung vor dem Öffnen (Zähler, Stand-ID, Schutz-Epoche)
- Einzelne Tresore öffnen und speichern, eine Auswahl als eigenen Tresor weitergeben
- Schnell-Entsperren, Panik-Sperre, Auto-Sperre, Bildschirmfoto-Schutz
- Import und Export (Passwort-Manager, 2FA-Apps, ICS)
- Update-Prüfung (abschaltbar, einzige Verbindung)

## 4. Was wegfällt

- **GhulMail** ganz: IMAP/SMTP, Konten, PGP und Autocrypt, Mail-Archiv, Mail-Benachrichtigungen, Dauerdienst,
  „Standard-App für Mail“
- **GhulDrive** ganz: Speicherorte, Index auf dem Server, `index.lock`, Aufräumen, Offline-Kopien
- **Server**: SFTP, der SSH-Schlüssel der Suite, das PHP-Modul, Freigabe-Links und Einmal-Links
- **Schlüsselwechsel** auf Servern und in GhulDrive (ohne Server gibt es nichts zu wechseln)
- **Fehlerbericht hochladen**: Der Bericht wird als Text angezeigt, zum Kopieren oder zum Teilen über eine andere
  App. `report.php` auf der Website entfällt.
- Android-Berechtigungen, die nur Mail oder Server brauchten (Netzwerkstatus, Dauerdienst für Mail,
  Akku-Ausnahme). `INTERNET` bleibt nur für die Update-Prüfung.
- Testserver (SFTP, Mail) in der CI und lokal

## 5. GhulDocs

### 5.1 Ein Tresor wie die anderen

Ein GhulDocs-Tresor besteht aus

- der Tresor-Datei `<id>.ghldoc`: ein Container wie bei GhulPass (gleiche Schlüssel, gleiches Schreibsiegel) mit dem
  **Inhaltsverzeichnis**: Ordner, Dateinamen, Größen, Datum, Typ, Favoriten, Papierkorb und je Datei ihr eigener
  Schlüssel und ihre Prüfsumme;
- je Datei einer **Inhaltsdatei** `docs-<id>/<zufällige Kennung>.ghlblob` mit dem verschlüsselten Inhalt.

Mehrere GhulDocs-Tresore sind möglich, z. B. „Wichtige Dokumente“ und „Steuer 2025“. Ein einzelner Tresor lässt sich
wie jeder andere für sich öffnen und speichern, etwa um ihn weiterzugeben (siehe 7).

### 5.2 Verschlüsselung

- Das Inhaltsverzeichnis ist durch den Container geschützt: Argon2id mit Passwort und zweitem Faktor, Schreibsiegel.
- Jede Datei hat einen eigenen zufälligen Schlüssel (32 Byte), der nur im Inhaltsverzeichnis steht.
- Der Inhalt wird in Abschnitten zu 4 MiB mit XChaCha20-Poly1305 verschlüsselt. Jeder Abschnitt ist an die Kennung
  der Datei, seine Nummer und „letzter Abschnitt ja/nein“ gebunden: Abschnitte lassen sich weder vertauschen noch
  weglassen noch an eine andere Datei hängen, ohne dass es auffällt.
- Dateinamen, Ordner und Typen stehen nur im verschlüsselten Inhaltsverzeichnis. Von außen sichtbar sind nur die
  Anzahl der Inhaltsdateien und ihre ungefähre Größe (aufgerundet auf 64 KiB).

### 5.3 Dokumente scannen (Android)

- **Fotografieren** mit der Kamera in der App, auch mehrere Seiten nacheinander.
- **Zuschneiden** je Seite mit einem Rahmen, dessen vier Ecken man mit dem Finger verschiebt. Die App schlägt die
  Ecken des Blatts vor und entzerrt das Bild danach (schräg fotografiertes Blatt wird gerade).
- Optional „Dokument“: Graustufen und mehr Kontrast für Text.
- **Als PDF speichern**: alle Seiten in einer PDF-Datei, Name vorgeschlagen (Datum), änderbar.
- Kein Klartext auf dem Datenträger: Fotos bleiben im Arbeitsspeicher, bis die PDF verschlüsselt im Tresor liegt.
  Die Kamera-App des Systems wird nicht benutzt.

Unter Windows gibt es keinen Scan; dort werden Dateien hinzugefügt.

### 5.4 Weitere Funktionen

- Beliebige Dateien hinzufügen: aus dem Dateisystem, mehrere auf einmal, auf Android auch per „Teilen mit
  GhulSuite“ aus anderen Apps
- Ordner anlegen, umbenennen, verschieben
- **Ansehen in der App** ohne Klartext auf dem Datenträger: Bilder, PDF, Text
- **Exportieren** (entschlüsselt an einen gewählten Ort, mit Rückfrage) und **Öffnen mit anderer App** (befristete
  Kopie, wird danach gelöscht, wie bisher bei Anhängen)
- Papierkorb, Favoriten, Suche über Namen
- Anzeige der Belegung (z. B. „412 MB von 1 GB“)

### 5.5 Grenzen

- Fest **1 GB Inhalt je Suite** (alle GhulDocs-Tresore zusammen), eine einzelne Datei ebenfalls bis 1 GB. Darüber
  lehnt die App das Hinzufügen mit einer klaren Meldung ab.
- Keine Fassungen: eine Datei hat einen Inhalt.

## 6. Suite-Datei und Speichern

- **Arbeitskopie:** Auf dem Gerät schreibt die App nur, was sich ändert (eine neue Inhaltsdatei, das
  Inhaltsverzeichnis). Das geht sofort.
- **Suite-Datei:** Beim Speichern wird die ganze `.ghlzip` neu geschrieben. Die Inhaltsdateien sind schon
  verschlüsselt und werden nur hineinkopiert, unkomprimiert. Ihre Prüfsummen stehen schon im Inhaltsverzeichnis.
- **Nichts im Arbeitsspeicher:** Schreiben und Prüfen der `.ghlzip` laufen als Datenstrom (heute wird die Datei
  ganz in den Speicher gelesen; das muss für 1 GB auf dem Handy geändert werden).
- Größte Suite-Datei: 1,25 GiB (1 GB Inhalt plus die übrigen Tresore).
- Speichern und Öffnen zeigen einen Fortschritt.
- Beim Speichern wird zuerst die neue Datei geschrieben und dann die alte ersetzt: kurzzeitig ist doppelter Platz
  nötig.

Geschätzte Dauer beim Speichern mit vollem GhulDocs (1 GB):

| Ziel | Dauer |
|---|---|
| Windows, SSD | etwa 2 bis 5 s |
| Handy, interner Speicher | etwa 5 bis 12 s |
| USB-Stick oder SD-Karte | etwa 40 s bis 1,5 min |

Mit typischen 100 bis 300 MB ist es ein Bruchteil davon.

## 7. Einen GhulDocs-Tresor weitergeben

Ein einzelner GhulDocs-Tresor wird wie die anderen Einzeltresore gespeichert, als `.ghlzip` mit nur diesem Tresor
(Inhaltsverzeichnis und Inhaltsdateien) und eigenem Passwort. Der Empfänger öffnet ihn mit GhulSuite.

## 8. Bedrohungsmodell (Kurzfassung)

| Wer | Was er hat | Was er erreicht |
|---|---|---|
| Finder des Handys oder Sticks | die Suite-Datei oder die Arbeitskopie | nichts ohne Passwort **und** zweiten Faktor |
| Jemand mit altem Passwort und altem zweiten Faktor | eine Kopie von **vor** einer Schutzänderung | öffnet diese alte Kopie (siehe 2); ohne den alten zweiten Faktor nichts |
| Schadsoftware auf dem entsperrten Gerät | alles, was gerade angezeigt wird | nicht verhinderbar (Grenze, wie bisher) |

Klartext entsteht nur beim Ansehen (im Arbeitsspeicher) und beim Exportieren oder Öffnen mit einer anderen App
(befristete Kopie). Ein unabhängiges Sicherheitsaudit gibt es weiterhin nicht (Grenze L2 in `AUDIT.md`).

## 9. Folgen für die offenen Befunde

Mit Mail, Drive und Servern entfallen rund 70 der 146 offenen Befunde, darunter drei der fünf hohen (H2, H5, H7).
Es bleiben vor allem Sperren und Entsperren, Suite-Dateien, GhulCal (u. a. H8), GhulPass-Import (H4), Windows und
Update.

## 10. Umsetzung in Paketen

Auf dem Branch `feature/ghulvault` (vom Umbau-Branch), am Ende eine PR nach `dev`.

| Paket | Inhalt |
|---|---|
| G1 | GhulMail raus (Kern, App, Android-Dienste und -Berechtigungen, Tests, Emulator-Abläufe) |
| G2 | GhulDrive, Server, PHP-Modul, Freigaben und Schlüsselwechsel raus; Fehlerbericht ohne Hochladen; Testserver raus |
| G3 | Altlasten raus (ältere Formate); Schutzänderung: ältere Suite-Dateien werden nie mehr geöffnet |
| G4 | Suite-Datei als Datenstrom, bis 1,25 GiB, mit Fortschritt |
| G5 | GhulDocs im Kern: Tresor-Art, Inhaltsverzeichnis, Inhaltsdateien, Grenzen, Tests (auch Angriffe) |
| G6 | GhulDocs in der App: Liste, Ordner, Hinzufügen, Ansehen, Exportieren, Papierkorb, Belegung |
| G7 | Scannen: Kamera, mehrere Seiten, Zuschneiden mit verschiebbaren Ecken, Entzerren, PDF |
| G8 | Doku (FORMAT, KONZEPT, AUDIT, Testpläne), README, Website, Emulator-Abläufe |
| G9 | Unabhängige Durchsicht, Befunde beheben, PR |
