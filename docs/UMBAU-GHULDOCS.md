# Umbau: GhulDocs statt GhulDrive und GhulMail

Stand: 17.11.2026, freigegeben. Grundlage ist der Umbau ohne Synchronisation
([`UMBAU-OHNE-SYNC.md`](UMBAU-OHNE-SYNC.md)). Umsetzung auf dem Branch `feature/ghulvault`.

**Status (20.11.2026):** Pakete G1 bis G8 umgesetzt; aus G9 die unabhängige Durchsicht mit 14 Befunden, alle
behoben ([`TESTPROTOKOLL.md`](TESTPROTOKOLL.md), Abschnitt 10); der PR nach `dev` ist offen. Danach entschieden
(20.11.2026): **genau ein GhulDocs-Tresor je Suite** und eine **eigene Endung `.ghldocs`** für den weitergegebenen
GhulDocs-Tresor (GhulDocs-Paket); beides umgesetzt, siehe Abschnitt 11. Abweichungen bei der Umsetzung stehen in
Abschnitt 11.

## 1. Grundidee

GhulSuite wird eine reine Offline-App. Alles liegt verschlüsselt auf dem Gerät und in der Suite-Datei (`.ghlzip`),
es gibt keine Server, keine Konten und keine Verbindungen nach draußen. Ausnahmen sind nur die Update-Prüfung und
die Leak-Prüfung von GhulPass (Have I Been Pwned, es gehen nur die ersten 5 Zeichen eines SHA-1-Werts hinaus, nie das
Passwort); beide lassen sich abschalten.

| Bereich | Inhalt |
|---|---|
| **GhulPass** | Passwörter, Notizen, Karten, Anhänge |
| **GhulAuth** | 2FA-Codes |
| **GhulCal** | Kalender, Erinnerungen, ICS-Import und -Export als Datei |
| **GhulDocs** | neu: verschlüsselter Speicher für wichtige Scans und Dokumente, bis 1 GB |

Das Prinzip der Tresore bleibt: Jeder Bereich besteht aus Tresoren (eigene verschlüsselte Dateien), eine Suite kann
mehrere Tresore je Bereich haben (GhulDocs ausgenommen: genau einer, siehe Abschnitt 11), und alle zusammen sind die
Suite-Datei.

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
- Update-Prüfung und Leak-Prüfung von GhulPass (beide abschaltbar, die einzigen Verbindungen)

## 4. Was wegfällt

- **GhulMail** ganz: IMAP/SMTP, Konten, PGP und Autocrypt, Mail-Archiv, Mail-Benachrichtigungen, Dauerdienst,
  „Standard-App für Mail“
- **GhulDrive** ganz: Speicherorte, Index auf dem Server, `index.lock`, Aufräumen, Offline-Kopien
- **Server**: SFTP, der SSH-Schlüssel der Suite, das PHP-Modul, Freigabe-Links und Einmal-Links
- **Schlüsselwechsel** auf Servern und in GhulDrive (ohne Server gibt es nichts zu wechseln)
- **Fehlerbericht hochladen**: Der Bericht wird als Text angezeigt, zum Kopieren oder zum Teilen über eine andere
  App. `report.php` auf der Website entfällt.
- Android-Berechtigungen, die nur Mail oder Server brauchten (Netzwerkstatus, Dauerdienst für Mail,
  Akku-Ausnahme). `INTERNET` bleibt nur für die Update- und die Leak-Prüfung.
- Testserver (SFTP, Mail) in der CI und lokal

## 5. GhulDocs

### 5.1 Ein Tresor wie die anderen

Ein GhulDocs-Tresor besteht aus

- der Tresor-Datei `<id>.ghldoc`: ein Container wie bei GhulPass (gleiche Schlüssel, gleiches Schreibsiegel) mit dem
  **Inhaltsverzeichnis**: Ordner, Dateinamen, Größen, Datum, Typ, Favoriten, Papierkorb und je Datei ihr eigener
  Schlüssel und ihre Prüfsumme;
- je Datei einer **Inhaltsdatei** `docs-<id>/<zufällige Kennung>.ghlblob` mit dem verschlüsselten Inhalt.

Mehrere GhulDocs-Tresore sind möglich, z. B. „Wichtige Dokumente“ und „Steuer 2025“. Ein einzelner Tresor lässt sich
wie jeder andere für sich öffnen und speichern, etwa um ihn weiterzugeben (siehe 7). (Geändert: genau einer je Suite,
siehe Abschnitt 11.)

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
(Inhaltsverzeichnis und Inhaltsdateien) und eigenem Passwort. Der Empfänger öffnet ihn mit GhulSuite. (Umgesetzt mit
eigener Endung `.ghldocs`, siehe Abschnitt 11.)

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

| Paket | Inhalt | Stand |
|---|---|---|
| G1 | GhulMail raus (Kern, App, Android-Dienste und -Berechtigungen, Tests, Emulator-Abläufe) | erledigt |
| G2 | GhulDrive, Server, PHP-Modul, Freigaben und Schlüsselwechsel raus; Fehlerbericht ohne Hochladen; Testserver raus | erledigt |
| G3 | Altlasten raus (ältere Formate); Schutzänderung: ältere Suite-Dateien werden nie mehr geöffnet | erledigt |
| G4 | Suite-Datei als Datenstrom, bis 1,25 GiB, mit Fortschritt | erledigt |
| G5 | GhulDocs im Kern: Tresor-Art, Inhaltsverzeichnis, Inhaltsdateien, Grenzen, Tests (auch Angriffe) | erledigt |
| G6 | GhulDocs in der App: Liste, Ordner, Hinzufügen, Ansehen, Exportieren, Papierkorb, Belegung | erledigt |
| G7 | Scannen: Kamera, mehrere Seiten, Zuschneiden mit verschiebbaren Ecken, Entzerren, PDF | erledigt |
| G8 | Doku (FORMAT, KONZEPT, AUDIT, Testpläne), README, Website, Emulator-Abläufe | erledigt |
| G9 | Unabhängige Durchsicht, Befunde beheben, PR | Befunde behoben, PR offen |

## 11. Abweichungen bei der Umsetzung

Was beim Bauen anders oder genauer wurde als oben geplant:

- **Zusätzliche Grenzen von GhulDocs:** höchstens **20 000 Dokumente** je Suite (jedes ist ein Eintrag der
  Suite-Datei, die höchstens 21 000 Einträge hat) und höchstens 1 100 000 000 Byte Inhaltsdateien auf dem Datenträger
  (Inhalt plus Aufrundung auf 64 KiB). Die übrigen Tresore dürfen zusammen höchstens 200 MiB belegen (geprüft vor
  allem bei Anhängen). So passt ein volles GhulDocs mit den übrigen Tresoren immer in die größte Suite-Datei
  (1,25 GiB), auch bei sehr vielen kleinen Dateien; eine größere schreibt die App nicht. Der Papierkorb zählt mit, bis er geleert wird.
- **Ansehen von PDF:** unter Android mit dem `PdfRenderer` des Systems über eine anonyme Datei im Arbeitsspeicher
  (`memfd`), das gibt es erst ab **Android 11**; darunter bietet die App „Mit App öffnen“ und „Exportieren“ an. Unter
  Windows zeigt **Apache PDFBox** die Seiten, nur im Arbeitsspeicher.
- **Scannen:** eigener Kamera-Bildschirm mit CameraX; Blatterkennung, Entzerren und der Filter „Dokument“ in eigenem
  Kotlin-Code; ein **eigener kleiner PDF-Schreiber** bettet die JPEG-Seiten unverändert ein (keine PDF-Bibliothek auf
  Android). Dazu „Drehen“, „Ganzes Foto“ und eine Lupe beim Ziehen der Ecken.
- **Kein Emulator-Ablauf für die Kamera:** Das Scannen prüfen Desktop-Tests mit gezeichneten Fotos und einer
  Ersatz-Kamera (`DocScanTest`, `ScanAlgorithmsTest`) und der Testplan von Hand (Abschnitt 38). Die übrigen
  Emulator-Abläufe laufen ohne Testserver in einem Teil.
- **Suite-Datei:** eine feste, einfache ZIP-Form (unkomprimiert, ohne Data Descriptors und ZIP64), damit Schreiben und
  Prüfen als Datenstrom gehen; beim Öffnen kommen die Inhaltsdateien erst in einen Zwischenordner und nach dem
  Bestätigen an ihren Platz. Unter Android wird direkt in das gewählte Dokument geschrieben (keine Zwischendatei
  möglich), danach zurückgelesen.
- **Ein GhulDocs-Tresor für sich** ist ein GhulDocs-Paket ohne Kopfdatei und Suite-Container (der Tresor als erster
  Eintrag); „Als Tresor weitergeben“ geht auch für ausgewählte Ordner und Dateien.
- **Genau ein GhulDocs-Tresor je Suite** (entschieden am 20.11.2026, statt mehrerer wie in 5.1 geplant): Der Kern lehnt
  einen zweiten ab, wo immer ein Tresor hinzukommt (anlegen, einrichten, hinzufügen); eine Suite-Datei mit mehr als
  einem ist ungültig. Die Oberfläche bietet keinen zweiten an; „Alle Tresore“, die Liste der Tresore in GhulDocs und
  die Auswahl in der Kontextzeile von GhulDocs sind entfallen. Ein GhulDocs-Paket, das einer Suite mit GhulDocs
  hinzugefügt wird, kommt in deren Tresor, in einen neuen Ordner (benannt nach dem Paket, änderbar); jeder Eintrag
  bekommt eine neue Kennung, die geprüften Inhaltsdateien werden unverändert verschoben (ihre Schlüssel stehen im
  Inhaltsverzeichnis), die Grenzen (1 GB, 20 000 Dokumente) gelten wie beim Hinzufügen. Steht eine Inhaltsdatei des
  Pakets schon im Tresor (dasselbe Paket noch einmal), wird nichts übernommen. Die 1 GB gelten weiter nur für
  GhulDocs, die Suite-Datei bleibt bis 1,25 GiB.
- **Eigene Endung `.ghldocs`** (entschieden am 20.11.2026, statt `.ghlzip` wie in Abschnitt 7): Der GhulDocs-Tresor
  für sich (exportiert oder weitergegeben) heißt GhulDocs-Paket und hat die Endung `.ghldocs`; die Suite-Datei bleibt
  `.ghlzip`. Innen unterscheidet sie der erste Eintrag (Kopf oder GhulDocs-Tresor): Ein Paket, das als Suite-Datei
  geöffnet werden soll, und eine Suite-Datei, die als Paket geöffnet werden soll, werden mit einer Meldung abgelehnt,
  die sagt, was die Datei ist. Speichern-Dialoge schlagen `.ghldocs` vor; Android öffnet `.ghldocs` aus dem
  Dateimanager wie `.ghlzip` (nur `content://`), Windows verknüpft beide Endungen (MSI) und reicht beide an ein schon
  laufendes Programm weiter.
- **Website:** `report.php` und sein Datenordner sind entfernt; die Datenschutzerklärung der App nennt nur noch die
  Update- und die Leak-Prüfung. Auf dem Webspace der Live-Seite liegen die alten Dateien, bis sie von Hand gelöscht
  werden (der Upload löscht nichts).

