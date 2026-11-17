# Umbau: GhulSuite ohne Synchronisation

**Stand:** 17.11.2026 · **Status:** beschlossen, Umsetzung in Paketen (Abschnitt 9)

Die Synchronisation zwischen mehreren Geräten wird vollständig entfernt. Sie war die Quelle der meisten
Sicherheitsfunde der letzten Durchsichten (fremde Dateien übernehmen, Schutzstände zusammenführen, ersetzte Codes,
gleichzeitige Änderungen, gefälschte Server-Dateien). An ihre Stelle tritt ein einfaches Modell:
**Die Suite ist eine Datei. Man öffnet sie, arbeitet darin und speichert sie.** Wohin die Datei wandert (USB-Stick,
eigener Ordner), entscheidet der Nutzer; die App gleicht nichts ab.

---

## 1. Grundidee

- Die Suite ist eine Datei `.ghlzip` mit allen Bereichen (GhulPass, GhulAuth, GhulCal, GhulDrive, GhulMail), wie
  bisher mit Passwort und Key bzw. Schlüsseldatei geschützt.
- **Öffnen** lädt die Datei (nach allen Prüfungen, Abschnitt 4) in die Arbeitskopie der App. **Speichern** schreibt die
  Arbeitskopie zurück in dieselbe Datei (oder „Speichern unter“ an einen neuen Ort).
- Die App merkt sich die zuletzt geöffnete Datei (Android: über die Dateiauswahl des Systems, mit dauerhafter
  Berechtigung; Windows: Pfad). Liegt sie auf einem USB-Stick, wird direkt dorthin gespeichert.
- Die **Arbeitskopie** im internen Speicher der App bleibt bestehen, damit die App auch ohne den Stick funktioniert.
  Sie ist Zwischenstand, nicht die Datenbank.
- **Kein Abgleich, kein Zusammenführen.** Aktuell ist immer der Stand, der zuletzt in die Datei gespeichert wurde.

Beispiel: Handy: Datei auf dem Stick öffnen, arbeiten, speichern. Windows: dieselbe Datei öffnen, arbeiten, speichern.
Wieder auf dem Handy: öffnen.

## 2. Die Datei `.ghlzip`

- Eigene Endung, damit Android und Windows sie der App zuordnen. **Kein zweites Passwort**: Die Inhalte sind die
  verschlüsselten, versiegelten Container (Formatversion 3, Schreibsiegel); die ZIP fügt nichts hinzu.
- Inhalt: die Suite-Datei, die Tresore (neutrale Namen, F23) und die Kopfdatei `export.ghlexp`, ein eigener kleiner
  Container (verschlüsselt und versiegelt wie die Tresore). Er enthält:
  - Suite-ID und `container_id` der Suite-Datei;
  - je Datei `container_id`, Art, Dateiname und SHA-256;
  - **Stand-ID** (zufällig, neu bei jedem Speichern), **Vorgänger-ID** (die Stand-ID, die beim Öffnen geladen wurde)
    und **Zähler** (höchster bekannter Zähler + 1);
  - **Schutz-Epoche** (wächst bei jeder Schutzänderung) und die IDs entfernter Keys und ersetzter Codes;
  - Zeitpunkt und Gerätename.
- Strenge Regeln für das Archiv: nur genau die genannten Einträge mit genau diesen Namen, keine Ordner, keine
  doppelten oder zusätzlichen Einträge, Größen beim Lesen gezählt (nicht aus dem ZIP-Verzeichnis übernommen).

## 3. Speichern

1. Alle Container werden frisch versiegelt (neue Revision) und mit `export.ghlexp` in eine temporäre Datei neben dem
   Ziel geschrieben, dann in einem Schritt ersetzt.
2. Vorher prüft die App, ob die Zieldatei seit dem Öffnen von jemand anderem gespeichert wurde (andere Stand-ID). Dann
   fragt sie: „Die Datei wurde inzwischen anderswo gespeichert. Überschreiben (die Änderungen von dort gehen verloren)
   oder unter neuem Namen speichern?“ Zusammengeführt wird nicht.
3. Die App merkt sich Stand-ID, Zähler und Schutz-Epoche.
4. Gibt es Änderungen seit dem letzten Speichern, zeigt die App das an („Nicht gespeicherte Änderungen“).

## 4. Öffnen

1. Die App liest `export.ghlexp` und fragt nach Passwort und Key bzw. Schlüsseldatei.
2. **Alles wird geprüft, bevor etwas ersetzt wird:**
   - Schreibsiegel jeder Datei und von `export.ghlexp`;
   - jede Datei steht in der Liste mit Art, `container_id` und Hash; die Liste stimmt genau mit den Tresoren der
     Suite-Datei überein; nichts fehlt, nichts ist zu viel;
   - alles gehört zu derselben Suite, die Kopfdatei zur Suite-Datei.
   Schlägt etwas fehl, wird nichts geladen, und die App sagt, was nicht stimmt.
3. **Rückfragen:**
   - **Andere Suite** als die auf dem Gerät: „Diese Datei enthält eine andere Suite. Sie ersetzt alles auf diesem Gerät.“
   - **Älterer Stand** (kleinerer Zähler als hier zuletzt gesehen) oder **Abzweig** (gleicher Zähler mit anderer
     Stand-ID, oder die Vorgänger-ID ist hier unbekannt, obwohl der Zähler kleiner ist): „Diese Datei ist älter als
     dein Stand hier“ bzw. „Diese Datei wurde von einem anderen Stand aus weiterbearbeitet.“
   - **Älterer Schutz** (kleinere Schutz-Epoche als hier zuletzt gesehen): „Diese Datei hat einen älteren Schutz: ein
     früheres Passwort, entfernte Keys oder ein ersetzter Code gelten darin noch.“ Wird sie trotzdem geöffnet, erneuert
     die App den Schutz sofort (entfernte Keys raus, neuer Code, neues Passwort).
   - **Nicht gespeicherte Änderungen** auf dem Gerät: „Sie gehen verloren.“ Mit dem Angebot, vorher zu speichern. Die
     Warnung nennt, was auf Servern daran hängt: seit dem letzten Speichern hochgeladene Drive-Dateien, neue
     Freigabelinks, Mails im Postausgang.
4. Ersetzt wird aus einem Zwischenordner in einem Schritt. Der bisherige Stand bleibt als Sicherung (einmal rückgängig).
5. **Gerätedaten des alten Stands werden gelöscht:** Schnell-Entsperren (Dateien und Zeitstempel, sie enthalten den
   alten Passwort-Schlüssel und `S`), Mail-Zwischenspeicher, Drive-Offline-Kopien, unfertige Uploads.
6. **Öffnen mit dem Wiederherstellungscode** (ohne Passwort): Die Siegel lassen sich dann nicht prüfen, nur lesen. Das
   ist die bewusste Notfall-Ausnahme: Danach setzt die App ein neues Passwort und versiegelt alles neu. Die Prüfungen
   auf Stand und Schutz-Epoche gelten trotzdem, und die App weist darauf hin, dass der Inhalt nicht geprüft werden
   konnte.

## 5. Was bleibt

- **Formatversion 3 mit Schreibsiegel.** Es beweist: Diese Datei wurde von jemandem mit diesem Passwort und `S`
  geschrieben. Wer den Stick und einen alten Code hat, kann lesen, was damals galt, aber nichts unterschieben, was ein
  Gerät öffnet. Dass eine Datei aktuell ist, beweist es nicht; dafür sind Zähler, Stand-ID und Schutz-Epoche da.
- **Wiederherstellungscode:** öffnet alles allein, auch eine einzelne Tresordatei oder eine `.ghlzip` (Abschnitt 4.6).
- **Schutzänderungen** (Passwort, Key hinzufügen/entfernen, neuer Code) wirken in der Arbeitskopie und reisen mit dem
  nächsten Speichern mit; sie erneuern die inneren Schlüssel wie bisher und erhöhen die Schutz-Epoche.
- **Keys und Schlüsseldateien** funktionieren auf jedem Gerät; Schnell-Entsperren bleibt je Gerät.
- **GhulMail:** Der Mail-Server ist die Wahrheit für Mails; Konten, Schlüssel und Entwürfe stehen im Mail-Tresor.
- **Freigabelinks** über das PHP-Modul bleiben. Modul und „Speicher aufräumen“ richten sich nach den
  Freigabebeschreibungen auf dem Server (`shares/*.json`). Ein Gerät kann Freigaben auch dann auflisten und beenden,
  wenn sie in seinem Stand fehlen (z. B. nach dem Öffnen einer älteren Datei).
- **SFTP/FTPS** bleibt, nur noch als Speicher für GhulDrive-Blöcke und Freigaben.

## 6. GhulDrive

- Der **Index** jedes Speicherorts (Ordner, Namen, Größen, Dateischlüssel, Blocklisten) liegt **im Drive-Tresor**,
  nicht mehr auf dem Server. Er reist mit der Datei. Auf dem Server liegen nur noch die verschlüsselten **Blöcke**
  (je Dateiinhalt ein eigener, zufälliger Schlüssel; die Blocknamen leiten sich davon ab, zwei Geräte erzeugen nie
  denselben Namen).
- Wer einen älteren Stand öffnet, sieht dessen Index: Dateien aus neueren Ständen fehlen dort. So gewollt: nicht
  bequem, aber es gibt keinen gemeinsamen Index mehr, den jemand fälschen oder zwei Geräte zugleich ändern könnten.
- **Löschen nimmt nur den Eintrag aus dem Index**, auch „endgültig löschen“. Blöcke bleiben auf dem Server, damit
  ältere Stände und die Sicherung vor dem Öffnen weiter funktionieren.
- **„Speicher aufräumen“** (ausdrücklich, mit Warnung) löscht Blöcke, die im aktuellen Index nicht vorkommen und von
  keiner Freigabe gebraucht werden. Die Warnung sagt, dass Dateien älterer oder anderer Stände dabei verloren gehen.
- Beim Öffnen prüft die App stichprobenweise bzw. beim ersten Zugriff, ob die Blöcke einer Datei da sind, und zeigt
  fehlende als beschädigt.
- Unfertige Uploads werden auf dem Gerät mit einem eigenen Geräteschlüssel versiegelt (bisher aus dem Drive-Schlüssel).
- Größe: Der Index macht den Drive-Tresor größer und wird bei jeder Änderung neu geschrieben (auch beim automatischen
  Archivieren von Mails). Das Mail-Archiv schreibt deshalb gesammelt, nicht je Mail; die Grenze von 256 MiB je
  Container wird beobachtet.

## 7. Was wegfällt

- **Kern:** die gesamte Synchronisation (`app/sync.rs`): Zusammenführen von Suite und Tresoren, Übernahme fremder
  Schutzstände, frühere Schutzstände und Passwort-Schlüssel, ersetzte Codes, `protection_changed`, Gleichstandsregel,
  Verwerfen veränderter Server-Dateien, der Sync-Server, `SyncReport`.
- **Zusammenführen in den Einstellungen:** `merge_settings`, Zeitstempel (`Stamped`) und Löschmarken (`mark_deleted`)
  in Servern, Freigaben und Bereichen; `REMOVED_SLOTS`, `revisions`, `ignored_files`.
- **„Ordner ist die Suite“:** Konfliktkopien fremder Sync-Programme, das Übernehmen gefundener Tresore aus dem Ordner
  und das Zusammenführen doppelter Kopien. Der Ordner ist nur noch die Arbeitskopie.
- **Drive:** Index auf dem Server, Drive-Schlüssel des Index samt Erneuern und Siegel, „älterer Index“ (`seen`),
  `use_sync` und das Umziehen vom Sync-Server.
- **Ungeprüfte Ladewege:** die bisherige Sicherung/Wiederherstellung (`backup.rs`, ZIP ohne Kopfdatei und Prüfung) und
  „Von meinem Server holen“ (`fetch_from_server`, Server-Ordner `suite/`). Beide werden durch Öffnen/Speichern einer
  `.ghlzip` ersetzt; die Sicherung bleibt nur bis Paket 4 (Übergang).
- **App:** Sync-Einstellungen, „Jetzt synchronisieren“, die Dialoge „Passwort / Schutz auf anderem Gerät geändert“,
  die Warnung über veränderte Server-Dateien.
- **Befunde:** Was nur die Synchronisation betraf (u. a. H1, S1, M18 und Teile vieler M- und N-Punkte), erledigt sich.
- Ältere Dateien und Formate werden nicht übernommen (Entwicklungsphase).

## 8. Bedrohungsmodell (Kurzfassung)

| Wer | Kann | Kann nicht |
|---|---|---|
| Hat den Stick, sonst nichts | Datei kopieren, löschen, durch eine ältere ersetzen | lesen; etwas unterschieben (Siegel); eine ältere Datei unbemerkt unterschieben (Zähler, Rückfrage) |
| Hat Stick und einen alten Code | lesen, was im alten Stand galt | etwas unterschieben; den aktuellen Stand lesen, wenn der Code ersetzt wurde |
| Hat Stick, alten Code und altes Passwort | einen alten Stand mit altem Schutz anbieten | ihn unbemerkt öffnen lassen (Schutz-Epoche, Rückfrage, sofortiges Erneuern) |
| Hat den Server | Blöcke löschen oder vertauschen, Freigaben stören | Dateien lesen; einen Index fälschen (es gibt keinen auf dem Server) |

## 9. Umsetzung in Paketen (je ein Pull Request, Doku jeweils mit)

1. **Konzept** (diese Datei).
2. **Kern und App: Sync raus.** `sync.rs`, Zusammenführen, Ordner-Abgleich, „Von meinem Server holen“ und alles
   Zugehörige entfernen; die Sync-Oberfläche der App im selben PR, damit Android und Windows bauen. Die bisherige
   Sicherung bleibt als Übergang.
3. **Kern: Format 3.** Schreibsiegel, Formatversion 3, Testvektoren, SFTP/FTPS-Fixes (aus
   `fix/review-core-protection` übernommen, ohne Sync-Teile); FORMAT.md.
4. **Kern und App: `.ghlzip`.** Öffnen und Speichern mit allen Prüfungen aus Abschnitt 4, Zähler, Stand-ID,
   Schutz-Epoche, Rückfragen, Gerätedaten löschen; Sicherung/Wiederherstellung entfernen; Dateizuordnung; FORMAT.md;
   Tests auch für Angriffe auf die Datei (fehlende, vertauschte, alte, fremde, abgezweigte Teile; falsches Siegel; Code
   ohne Passwort; ZIP-Tricks).
5. **Kern und App: Drive-Index in den Tresor.** Index je Speicherort im Drive-Tresor, nur Blöcke auf dem Server,
   Löschen nur im Index, „Speicher aufräumen“, eigener Schlüssel für unfertige Uploads, gesammeltes Mail-Archiv;
   FORMAT.md.
6. **App:** Hinweis „nicht gespeicherte Änderungen“, Rückfragen, Emulator-Abläufe für Öffnen/Speichern.
7. **Doku, README, Website:** KONZEPT, AUDIT (Bedrohungsmodell), Testplan.
8. **Restliche Befunde** (Paket 2, M, N) neu sichten.
