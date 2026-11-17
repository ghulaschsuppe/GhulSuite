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
   - **Älterer Schutz** (kleinere Schutz-Epoche als hier zuletzt gesehen): **wird nicht geöffnet.** „Diese Datei ist
     durch eine neuere mit geändertem Schutz ersetzt und ungültig.“ Auf einem Gerät, das den neueren Stand nie gesehen
     hat, öffnet sie sich zwar, kommt aber nach dem Schlüsselwechsel nicht mehr an den Server (Abschnitt 6.4).
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
- **Freigabelinks** über das PHP-Modul bleiben (das Modul liest nur, es schreibt nichts übers Web). Die
  Freigabebeschreibungen liegen auf dem Server (`shares/*.json`); jedes Gerät mit der aktuellen Datei sieht und beendet
  alle Freigaben.
- **Server nur noch per SFTP** (Abschnitt 6.1). Ohne SSH-Zugang beim Hoster gibt es kein GhulDrive und keine
  Freigabelinks; GhulPass, GhulAuth, GhulCal und GhulMail brauchen keinen Server.

## 6. Server und GhulDrive

### 6.1 Nur SFTP

- FTP/FTPS entfällt vollständig. In der Datei steht als Server-Zugang nur ein **SSH-Schlüssel**, nie ein Passwort.
  Nur so kann die App den Zugang bei einer Schutzänderung selbst tauschen (6.4).
- Das PHP-Modul für Freigabelinks lädt die App ebenfalls per SFTP hoch.
- Die Server-Kennung (Host-Key) wird wie bisher beim ersten Verbinden bestätigt und danach geprüft.

### 6.2 Was wo liegt

- **Auf dem Server, im Drive-Ordner:**
  - `index.ghldrv`: der eine Index (Ordner, Namen, Größen, Dateischlüssel, Blocklisten, Papierkorb, Versionen),
    verschlüsselt und authentisiert mit dem Drive-Schlüssel, an die Suite gebunden (ein eigenes Siegel mit dem
    Passwort braucht er nicht: der Drive-Schlüssel steht nur im Drive-Tresor, gegen alte Kopien hilft 6.4);
  - `index.bak`: der vorige Stand (falls der Index beschädigt wird);
  - die **Blöcke**: Dateiinhalte, je Dateifassung mit eigenem, zufälligem Schlüssel; Blöcke werden nie überschrieben.
- **In der Suite-Datei** (Drive-Tresor): Drive-Schlüssel, SSH-Schlüssel, Serveradresse, Host-Key und die höchste
  gesehene Index-Revision. Kein Index, keine Dateiliste.
- Drive ist damit nicht Teil des Speicherns: Änderungen liegen sofort auf dem Server. **Jede Kopie der Suite-Datei seit
  der letzten Schutzänderung** sieht denselben, aktuellen Drive-Stand, egal auf welchem Gerät sie geöffnet wird (wer den
  ganzen Tag am Telefon hoch- und herunterlädt, sieht am PC alles, auch mit einer älteren Kopie). Nur Passwörter, 2FA,
  Kalender und Mail-Einstellungen stehen in der Datei selbst; für sie gilt die zuletzt gespeicherte.

### 6.3 Im Alltag

- **Lesen:** Index laden, entschlüsseln und prüfen. Ist die Revision kleiner als die gemerkte, hat der Server
  einen alten Stand zurückgespielt: Die App warnt und schreibt nichts.
- **Ändern** (Hochladen, Umbenennen, Verschieben, Löschen) unter einer Sperre:
  1. `index.lock` exklusiv anlegen (SFTP legt sie nur an, wenn es sie noch nicht gibt); eine Sperre, die älter als
     zwei Minuten ist, gilt als verwaist;
  2. Index neu laden, ändern, als neue Datei schreiben und in einem Schritt ersetzen; der bisherige wird `index.bak`;
  3. Sperre löschen.
  So ändert immer nur ein Gerät zur Zeit, nichts geht verloren, auch wenn zwei Geräte gleichzeitig arbeiten.
- **Hochladen:** erst alle Blöcke, dann der Eintrag im Index. Bricht es ab, fehlt nur der Eintrag, nie ein Block.
- **Löschen:** erst in den Papierkorb (Markierung im Index). Endgültig löschen: erst den Index schreiben, dann die
  Blöcke löschen.
- **Aufräumen** (automatisch): Der Index ist der vollständige Stand. Blöcke, die in keinem Eintrag und keinem
  Freigabelink vorkommen und älter als einen Tag sind (Schutz für laufende Uploads), werden gelöscht. Es bleiben
  keine Leichen.

### 6.4 Schlüsselwechsel

Bei jeder Schutzänderung (neues Passwort, neuer Code, Key oder Schlüsseldatei entfernt):

1. **Lokal vormerken:** Die Arbeitskopie bekommt einen neuen Drive-Schlüssel und einen neuen SSH-Schlüssel, zusätzlich
   zu den alten, mit dem Vermerk „Wechsel läuft“. Bricht etwas ab, setzt die App beim nächsten Start dort fort.
2. **Index umschlüsseln** (unter der Sperre): mit dem alten Schlüssel lesen, mit dem neuen Schlüssel schreiben,
   `index.bak` (alter Schlüssel) löschen. Ein Schreibvorgang; die Blöcke bleiben unverändert, denn ihre Schlüssel
   stehen nur im Index. Ist der Index älter als gesehen, beschädigt oder fehlt er, wird nichts still übernommen: der
   Nutzer entscheidet wie beim normalen Lesen (6.3).
3. **SSH-Zugang tauschen:** neuen öffentlichen Schlüssel auf dem Server eintragen, damit anmelden und prüfen, dann den
   alten entfernen und prüfen, dass er abgewiesen wird (je Anbieter, siehe unten).
4. **Abschließen:** alte SSH- und Drive-Schlüssel aus der Arbeitskopie entfernen; die App fordert auf, die
   Suite-Datei zu speichern. Eine Sicherung des Servers von vor der Schutzänderung ist danach nicht mehr lesbar (6.5).

Danach:
- Eine **alte Datei** (z. B. auf einem verlorenen Stick) kann sich nicht mehr am Server anmelden und den Index nicht
  öffnen. Selbst mit einer früher gezogenen Kopie des Index sieht sie nur die Dateien von damals.
- Ein **anderes Gerät mit der vorigen Datei** wird abgewiesen („Der Zugang wurde geändert, öffne die aktuelle
  Suite-Datei.“). Es kann nichts zerstören; seine Drive-Änderungen liegen bereits auf dem Server.
- Regel: Nach einer Schutzänderung arbeitet nur das Gerät weiter, das sie gemacht hat; alle anderen öffnen zuerst die
  neue Datei.

**Je Anbieter.** Wie Schritt 3 abläuft, hängt davon ab, wo der Anbieter die erlaubten Schlüssel hält. Die App erkennt
das selbst, eine Einstellung braucht es nicht:

- **Der Server liest `~/.ssh/authorized_keys`** (eigene Server, VPS, NAS, die meisten Webhoster mit SSH-Zugang und
  Speicherboxen mit SFTP): Die App meldet sich mit dem alten Schlüssel an, hängt den neuen an die Datei an, meldet sich
  mit dem neuen an, entfernt die Zeilen der alten Schlüssel (alle anderen Zeilen bleiben) und prüft zum Schluss, dass
  der alte Schlüssel abgewiesen wird. Ohne Zutun des Nutzers.
- **Der Anbieter verwaltet Schlüssel nur in seinem Webpanel** (die Datei wird angelegt, aber nicht gelesen, oder ist
  nicht beschreibbar): Die Anmeldung mit dem neuen Schlüssel scheitert. Die App zeigt den neuen öffentlichen Schlüssel
  und bittet, ihn im Webpanel einzutragen; „Prüfen“ meldet sich damit an. Danach bittet sie, den alten im Webpanel zu
  entfernen; „Prüfen“ stellt sicher, dass er abgewiesen wird. Bis dahin meldet sich die App mit dem alten Schlüssel an,
  der Server bleibt also nutzbar.
- **Nur Passwort-Anmeldung, keine Schlüssel:** Solche Server nimmt die App nicht auf (6.1), es gibt nichts zu tauschen.

Ist ein Server gerade nicht erreichbar, bleibt er auf „wartet“ und wird nach dem nächsten Entsperren wieder versucht.
Nur eine abgewiesene Anmeldung zählt als „alter Schlüssel abgewiesen“; bricht die Verbindung ab (Zeitlimit,
Verbindungssperren nach vielen Anmeldungen), wartet der Server. Neuere SSH-Server sperren eine Adresse nach
abgewiesenen Anmeldungen kurz (bei OpenSSH ab 9.8 einige Sekunden); auch dann wartet der Server und wird beim nächsten
Mal fertig. Nach dem Entfernen der alten Zeilen prüft die App, dass
der neue Schlüssel weiter funktioniert, sonst schreibt sie die Datei zurück. Kommt kein Schlüssel mehr hinein (der
alte wurde im Webpanel schon entfernt), bittet sie, den neuen dort einzutragen.
Der Wechsel gilt erst als abgeschlossen, wenn alle Server und alle Drive-Speicherorte durch sind; erst dann werden die
alten Schlüssel aus der Suite gelöscht.

### 6.5 Grenzen

- Verliert der Server Daten (Hoster setzt zurück, Datenträger kaputt), merkt die App es (Revision, fehlende Blöcke),
  retten kann sie es nicht. Dagegen hilft nur eine Sicherung beim Hoster, und zwar eine von nach der letzten
  Schutzänderung: Ältere sind mit einem Drive-Schlüssel verschlüsselt, den die Suite nicht mehr hat.
- Wer die aktuelle Datei mit Passwort und Key öffnen kann, hat Zugriff auf Drive. So ist es gedacht.
- Unfertige Uploads werden auf dem Gerät mit einem eigenen Geräteschlüssel versiegelt.

## 7. Was wegfällt

- **Kern:** die gesamte Synchronisation (`app/sync.rs`): Zusammenführen von Suite und Tresoren, Übernahme fremder
  Schutzstände, frühere Schutzstände und Passwort-Schlüssel, ersetzte Codes, `protection_changed`, Gleichstandsregel,
  Verwerfen veränderter Server-Dateien, der Sync-Server, `SyncReport`.
- **Zusammenführen in den Einstellungen:** `merge_settings`, Zeitstempel (`Stamped`) und Löschmarken (`mark_deleted`)
  in Servern, Freigaben und Bereichen; `REMOVED_SLOTS`, `revisions`, `ignored_files`.
- **„Ordner ist die Suite“:** Konfliktkopien fremder Sync-Programme, das Übernehmen gefundener Tresore aus dem Ordner
  und das Zusammenführen doppelter Kopien. Der Ordner ist nur noch die Arbeitskopie.
- **FTP/FTPS:** Adapter, Zertifikatsprüfung für FTP, die Sonderfälle beim Ersetzen (`.ghlold`, `SIZE`/`MDTM`).
- **Drive:** `use_sync` und das Umziehen vom Sync-Server; mehrere Schlüssel-Generationen des Index (es gibt nur den
  aktuellen, beim Wechsel wird umgeschlüsselt).
- **Ungeprüfte Ladewege:** die bisherige Sicherung/Wiederherstellung (`backup.rs`, ZIP ohne Kopfdatei und Prüfung) und
  „Von meinem Server holen“ (`fetch_from_server`, Server-Ordner `suite/`). Beide werden durch Öffnen/Speichern einer
  `.ghlzip` ersetzt; die Sicherung bleibt nur bis Paket 5 (Übergang).
- **App:** Sync-Einstellungen, „Jetzt synchronisieren“, die Dialoge „Passwort / Schutz auf anderem Gerät geändert“,
  die Warnung über veränderte Server-Dateien.
- **Befunde:** Was nur die Synchronisation betraf (u. a. H1, S1, M18 und Teile vieler M- und N-Punkte), erledigt sich.
- Ältere Dateien und Formate werden nicht übernommen (Entwicklungsphase).

## 8. Bedrohungsmodell (Kurzfassung)

| Wer | Kann | Kann nicht |
|---|---|---|
| Hat den Stick, sonst nichts | Datei kopieren, löschen, durch eine ältere ersetzen | lesen; etwas unterschieben (Siegel); eine ältere Datei unbemerkt öffnen lassen (Zähler, Rückfrage) |
| Hat Stick und einen alten Code | lesen, was im alten Stand galt | etwas unterschieben; den aktuellen Stand lesen; nach dem Schlüsselwechsel an den Server |
| Hat Stick, alten Code und altes Passwort | einen alten Stand mit altem Schutz anbieten | ihn auf einem Gerät mit neuerem Stand öffnen lassen (Schutz-Epoche, Sperre); an den Server |
| Hat eine alte Suite-Datei (vor einer Schutzänderung) | die damaligen Inhalte lesen | sich am Server anmelden; den aktuellen Drive-Index öffnen; neue Drive-Dateien lesen |
| Betreibt den Server | Blöcke löschen, einen alten Index zurückspielen, Freigaben stören | Dateien lesen; einen Index fälschen (authentisiert mit dem Drive-Schlüssel); einen alten Index unbemerkt unterschieben (Revision) |
| Betreibt den Server und hat eine alte Kopie der Suite-Datei | einen Index unter dem alten Drive-Schlüssel ablegen | ihn unterschieben (nach dem Schlüsselwechsel kennt die Suite den alten Schlüssel nicht mehr; während des Wechsels nur nach Bestätigung einer eigenen Warnung) |

## 9. Umsetzung in Paketen (je ein Pull Request, Doku jeweils mit)

1. **Konzept** (diese Datei).
2. **Kern und App: Sync raus.** `sync.rs`, Zusammenführen, Ordner-Abgleich, „Von meinem Server holen“ und alles
   Zugehörige entfernen; die Sync-Oberfläche der App im selben PR, damit Android und Windows bauen. Die bisherige
   Sicherung bleibt als Übergang.
3. **Kern und App: nur SFTP.** FTP/FTPS entfernen (Kern, Server-Formular, Einrichtung, Tests, CI-Testserver).
4. **Kern: Format 3.** Schreibsiegel, Formatversion 3, Testvektoren, SFTP-Fixes (aus `fix/review-core-protection`
   übernommen, ohne Sync-Teile); FORMAT.md.
5. **Kern und App: `.ghlzip`.** Öffnen und Speichern mit allen Prüfungen aus Abschnitt 4, Zähler, Stand-ID,
   Schutz-Epoche, Rückfragen, Gerätedaten löschen; Sicherung/Wiederherstellung entfernen; Dateizuordnung; FORMAT.md;
   Tests auch für Angriffe auf die Datei (fehlende, vertauschte, alte, fremde, abgezweigte Teile; falsches Siegel; Code
   ohne Passwort; ZIP-Tricks).
6. **Kern und App: Drive nach Abschnitt 6.** Versiegelter Index mit Sperre und `index.bak`, Revision, automatisches
   Aufräumen, Schlüsselwechsel mit Umschlüsseln des Index und SSH-Tausch (fortsetzbar), Geräteschlüssel für unfertige
   Uploads; FORMAT.md; Tests für gleichzeitige Änderungen, Abbrüche an jeder Stelle des Wechsels, alte Datei nach dem
   Wechsel, zurückgespielten und beschädigten Index.
7. **App:** Hinweis „nicht gespeicherte Änderungen“, Rückfragen, Aufforderung zum Speichern nach einer Schutzänderung,
   Emulator-Abläufe für Öffnen/Speichern und Drive.
8. **Doku, README, Website:** KONZEPT, AUDIT (Bedrohungsmodell), Testplan, Hinweis „SSH-Zugang nötig für Drive und
   Freigaben“.
9. **Restliche Befunde** (Paket 2, M, N) neu sichten.
