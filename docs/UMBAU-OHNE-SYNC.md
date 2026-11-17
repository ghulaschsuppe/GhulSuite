# Umbau: GhulSuite ohne Synchronisation

**Stand:** 17.11.2026 · **Status:** beschlossen, Umsetzung in Paketen (Abschnitt 8)

Die Synchronisation zwischen mehreren Geräten wird vollständig entfernt. Sie war die Quelle der meisten
Sicherheitsfunde der letzten Durchsichten (fremde Dateien übernehmen, Schutzstände zusammenführen, ersetzte Codes,
gleichzeitige Änderungen, gefälschte Server-Dateien). Statt eines Abgleichs gibt es ein einfaches, sicheres Modell:
**Die Suite liegt auf einem Gerät und wird als Datei mitgenommen.**

---

## 1. Grundidee

- Eine Suite lebt auf genau einem Gerät: die Suite-Datei und die geschützten Tresore der Bereiche (GhulPass, GhulAuth,
  GhulCal, GhulDrive, GhulMail), wie bisher mit Passwort und Key bzw. Schlüsseldatei.
- Wer auf einem anderen Gerät weiterarbeiten will, **speichert die Suite als eine Datei** (`.ghlzip`), trägt sie
  hinüber (z. B. USB-Stick) und **lädt** sie dort.
- **Kein Abgleich, kein Zusammenführen.** Beim Laden ersetzt die Datei den Stand auf dem Gerät vollständig.
  Aktuell ist immer nur die zuletzt gespeicherte Datei.

Beispiel: Handy → Suite speichern → USB-Stick → Windows: Suite laden, arbeiten, speichern → Stick → Handy: laden.

## 2. Die Datei `.ghlzip`

- Eigene Endung, damit Android und Windows sie der App zuordnen.
- Ein ZIP-Archiv mit allen Container-Dateien der Suite (Suite-Datei und Tresore, Formatversion 3, jede mit
  Schreibsiegel) und einer Kopfdatei `export.ghlexp`.
- **Kein zweites Passwort.** Die Inhalte sind die verschlüsselten, versiegelten Tresore; die ZIP selbst fügt nichts
  hinzu und verrät nichts außer Anzahl und Größe der Dateien (Namen sind neutral, F23).
- `export.ghlexp` ist ein eigener kleiner Container (verschlüsselt und versiegelt wie die Tresore) mit:
  Suite-ID, Exportzähler (wächst bei jedem Speichern), Zeitpunkt, Gerätename, Liste der enthaltenen Dateien mit
  SHA-256. So fällt auf, wenn eine Datei fehlt, vertauscht oder aus einem anderen Export stammt.

## 3. Speichern

1. „Suite speichern“ schreibt alle Container frisch (neue Revision) und packt sie mit `export.ghlexp` in die ZIP.
2. Der Exportzähler wird auf dem Gerät gemerkt.
3. Die App merkt sich, ob es seit dem letzten Speichern Änderungen gab, und zeigt das an
   („Ungespeicherte Änderungen seit dem letzten Export“).

## 4. Laden

1. Die App liest `export.ghlexp` und fragt nach Passwort und Key bzw. Schlüsseldatei (oder dem Wiederherstellungscode,
   dann wird danach ein neues Passwort gesetzt, wie bisher).
2. Geprüft wird **alles, bevor etwas ersetzt wird**:
   - Schreibsiegel jeder Datei (Formatversion 3) und von `export.ghlexp`;
   - jede Datei steht in der Liste und hat den genannten Hash; keine fehlt, keine ist zu viel;
   - alle Dateien gehören zu derselben Suite.
   Schlägt etwas fehl, wird nichts geladen und die App sagt, was nicht stimmt.
3. **Schutz vor Versehen:**
   - Andere Suite als die auf dem Gerät: deutliche Rückfrage („Diese Datei enthält eine andere Suite. Sie ersetzt
     alles auf diesem Gerät.“).
   - Kleinerer Exportzähler als der zuletzt hier gespeicherte oder geladene: Rückfrage („Diese Datei ist älter als
     dein Stand hier.“).
   - Ungespeicherte Änderungen auf dem Gerät: Rückfrage („Sie gehen verloren, wenn du jetzt lädst.“).
4. Der bisherige Stand wird vor dem Ersetzen als Sicherung aufgehoben (einmal rückgängig machen).

## 5. Was bleibt

- **Formatversion 3 mit Schreibsiegel.** Es schützt jetzt die Datei auf dem Stick: Wer den Stick und einen alten
  Code hat, kann lesen, was damals galt, aber nichts unterschieben, was ein Gerät lädt.
- **Wiederherstellungscode:** öffnet alles allein, auch eine einzelne Tresordatei oder eine `.ghlzip`.
- **Schutzänderungen** (Passwort, Key hinzufügen/entfernen, neuer Code) wirken auf dem Gerät und reisen mit der
  nächsten gespeicherten Datei mit. Bei jeder erneuert die App die inneren Schlüssel wie bisher.
- **Keys und Schlüsseldateien** funktionieren auf jedem Gerät; Schnell-Entsperren bleibt je Gerät.
- **GhulMail:** Der Mail-Server ist die Wahrheit für Mails; Konten, Schlüssel und Entwürfe stehen im Mail-Tresor und
  reisen mit der Datei.
- **Freigabelinks** über das PHP-Modul bleiben.
- **SFTP/FTPS** bleibt, nur noch als Speicher für GhulDrive-Blöcke und Freigaben.

## 6. GhulDrive

- Der **Index** (Ordner, Namen, Größen, Dateischlüssel, Blocklisten) liegt **im Drive-Tresor**, nicht mehr auf dem
  Server. Er reist mit der `.ghlzip`.
- Auf dem Server liegen nur noch die verschlüsselten **Blöcke** (je Datei eigener Schlüssel, wie bisher).
- Wer eine ältere Datei lädt, sieht deren Index: Dateien, die erst in einer neueren Datei hochgeladen wurden, fehlen
  dort. Das ist so gewollt (nicht bequem, aber sicher): Es gibt keinen gemeinsamen Index mehr, den jemand fälschen
  oder zwei Geräte gleichzeitig ändern könnten.
- **Löschen:** Ein Gerät löscht nur Blöcke, die in seinem eigenen Index stehen und nirgends mehr gebraucht werden.
  Blöcke, die es nicht kennt (aus einem neueren Export), rührt es nie an. Unbenutzte Blöcke räumt nur ein
  ausdrückliches „Speicher aufräumen“ auf, mit Hinweis, dass es dabei Dateien aus anderen Ständen löschen kann.
- Der Drive-Schlüssel des Index entfällt damit; Erneuern bei Schutzänderungen und das Index-Siegel werden nicht mehr
  gebraucht.

## 7. Was wegfällt

- **Kern:** die gesamte Synchronisation (`app/sync.rs`): Zusammenführen von Suite und Tresoren, Übernahme fremder
  Schutzstände, frühere Schutzstände, ersetzte Codes, frühere Passwort-Schlüssel, Löschmarken für Tresore,
  Änderungszeit des Schutzes, Gleichstandsregel, Verwerfen veränderter Server-Dateien, der Sync-Server.
- **„Ordner ist die Suite“** mit Konfliktkopien fremder Sync-Programme (Syncthing, Nextcloud): Der Ordner ist nur noch
  der interne Speicher der App; Kopien darin werden nicht zusammengeführt.
- **Drive-Index auf dem Server**, Drive-Schlüssel des Index, Index-Siegel, „älterer Index“-Prüfung.
- **App:** Sync-Einstellungen, „Jetzt synchronisieren“, die Dialoge „Passwort / Schutz auf anderem Gerät geändert“,
  die Warnung über veränderte Server-Dateien.
- **Befunde:** Was nur die Synchronisation betraf (u. a. H1, S1, M18 und Teile vieler M- und N-Punkte), erledigt sich.
- Ältere Dateien und Formate werden nicht übernommen (Entwicklungsphase).

## 8. Umsetzung in Paketen (je ein Pull Request)

1. **Konzept** (diese Datei).
2. **Kern, Format 3:** Schreibsiegel, Formatversion 3, Testvektoren (aus `fix/review-core-protection` übernommen,
   ohne die Sync-Teile); SFTP/FTPS-Fixes.
3. **Kern, Sync raus:** `sync.rs`, Ordner-Abgleich und alles Zugehörige entfernen; Tests aufräumen.
4. **Kern, Drive-Index in den Tresor:** Index im Drive-Tresor, nur Blöcke auf dem Server, sicheres Löschen.
5. **Kern, `.ghlzip`:** Speichern und Laden mit allen Prüfungen aus Abschnitt 4; Tests auch für Angriffe auf die
   Datei (fehlende, vertauschte, alte, fremde Teile; falsches Siegel; Code ohne Passwort).
6. **App (Android und Windows):** Sync-Oberfläche raus, „Suite speichern / laden“ rein, Hinweis auf ungespeicherte
   Änderungen, Dateizuordnung `.ghlzip`, Emulator-Abläufe.
7. **Doku, README, Website:** KONZEPT, FORMAT, AUDIT (Bedrohungsmodell), Testplan neu.
8. **Restliche Befunde** (Paket 2, M, N) neu sichten.
