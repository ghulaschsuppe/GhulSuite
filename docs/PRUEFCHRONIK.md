<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# GhulSuite: Prüf-Chronik

Diese Datei hält **jede Prüfung** von GhulSuite fest, in der Reihenfolge, in der sie stattfand, und dazu, **wie und
warum sich das Konzept geändert hat**. Sie ist bewusst offen: Wer GhulSuite nutzen oder prüfen will, soll sehen
können, wer was wann geprüft hat, was dabei herauskam und was noch fehlt.

- **Wer prüft:** der Inhaber (Entscheidungen, eigene Tests) und KI-Agenten (Claude, jeweils neue, voneinander
  unabhängige Agenten je Durchsicht). Eine Prüfung durch Dritte ist nicht vorgesehen (Entscheidung des Inhabers vom
  21.11.2026, Abschnitt 1).
- **Was hier steht:** Datum, wer, was geprüft wurde, das Ergebnis und wo die Einzelheiten stehen. Die einzelnen Funde
  mit Test und Behebung stehen im [Testprotokoll](TESTPROTOKOLL.md) (Nummern F…, MV…, PR…, GP-…, R7-… bis R14-…, oben dort der Gesamtstand), die offenen kleinen
  Punkte und die Sicherheitslage im [Sicherheitsbericht](AUDIT.md), das Vorgehen im [Prüfkonzept](PRUEFKONZEPT.md).
- **Regel:** Jede neue Prüfung, auch ein Durchgang des Testplans auf einem Gerät durch den Inhaber, bekommt hier einen
  Eintrag (Vorlage in Abschnitt 4). Einträge werden nicht nachträglich geschönt; ein Irrtum wird mit Datum
  richtiggestellt, nicht gelöscht.

## 1. Wie sich das Konzept geändert hat

GhulSuite wurde mehrmals grundlegend umgebaut. Jeder Umbau folgte aus Prüfungen: Was sich nicht sicher genug machen
ließ, wurde entfernt statt weiter geflickt.

| Datum | Änderung | Grund | Einzelheiten |
|---|---|---|---|
| 12.11.2026 | **Start:** selbst gehostete Suite mit Ende-zu-Ende-Verschlüsselung: Passwörter, 2FA, Dateien (GhulDrive), Mail (GhulMail), Kalender, Synchronisation über eigene Server (SFTP, FTPS, PHP-Modul), Freigabelinks, Autofill | | [KONZEPT](KONZEPT.md), Geschichte im [AUDIT](AUDIT.md), Abschnitt 1 |
| 16.11.2026 | **Autofill und Foto-Sicherung gestrichen** | Autofill war eine eigene Angriffsfläche (Funde F18, F19, F25, F26: zu grobe Domain-Zuordnung, Apps konnten Webadressen behaupten) | [AUDIT](AUDIT.md), Abschnitt 1 und 3 |
| 17.11.2026 | **Keine Synchronisation mehr.** Die Suite ist eine Datei (`.ghlzip`), die man öffnet, bearbeitet und speichert; abgeglichen wird nichts | Die Synchronisation war die Quelle der meisten Sicherheitsfunde (fremde Dateien übernehmen, Schutzstände zusammenführen, ersetzte Codes, gleichzeitige Änderungen, gefälschte Server-Dateien) | [UMBAU-OHNE-SYNC](UMBAU-OHNE-SYNC.md) |
| 17. bis 19.11.2026 | **Reine Offline-App:** GhulMail, GhulDrive, alle Server, Freigabelinks, das Hochladen von Fehlerberichten und das Lesen älterer Formate entfernt; GhulDocs (verschlüsselte Dokumente auf dem Gerät) ersetzt GhulDrive | Rund 70 der 146 offenen Befunde der letzten Durchsichten betrafen Mail, Drive, Server, Freigaben und den Schlüsselwechsel; ohne Server und Konten fällt diese Angriffsfläche ganz weg | [UMBAU-GHULDOCS](UMBAU-GHULDOCS.md) |
| 20.11.2026 | Mehrere GhulDocs-Tresore (höchstens 3 je 1 GB), ganzen Tresor weitergeben; **Formatversion 3** mit Schreibsiegel | Einfacher als Einsortieren und Ersetzen (weniger Code) | [AUDIT](AUDIT.md), Abschnitt 1; [FORMAT](FORMAT.md) |
| 21.11.2026 | **Speichern mit Knopf statt automatisch** | Entscheidung des Inhabers: Automatisches Speichern passt nicht zu einem Tresor auf einem Stick (große GhulDocs-Tresore schrieben bei jeder Änderung Gigabytes) | Testprotokoll MV-18 |
| 21.11.2026 | **Schutzänderungen nur mit beiden Faktoren, Pflicht-Backup für Security Keys** | Wunsch des Inhabers nach Prüfrunde 4: Wer die entsperrte App kurz in der Hand hat, darf keinen dauerhaften Zugang bekommen; der Zugang darf nicht an einem einzelnen Key hängen, der verloren gehen kann | Testprotokoll PR-11, PR-14 |
| 21.11.2026 | **Format 3 eingefroren**, Kompatibilitätszusage | Gutachten: Für echte Daten darf ein Update nie den Zugang kosten | [FORMAT](FORMAT.md), Testprotokoll PR-19 |
| 21.11.2026 | **Funktionsstopp:** keine neuen Funktionen, nur Fehlerbehebungen und kleinere Anpassungen an Design und Dialogen | Entscheidung des Inhabers, Ziel: eine Version, die man der Öffentlichkeit sicher geben kann | [PROJEKT](../PROJEKT.md) |
| 21.11.2026 | **Signaturschlüssel bleiben wie sie sind** (ein Schlüssel für APK und Updates, als Secret in GitHub) | Entscheidung des Inhabers nach dem Gutachten; sein GitHub-Konto ist mit einem Hardware-Key gesichert, veröffentlicht wird nur nach grüner CI | [AUDIT](AUDIT.md), Abschnitt 4.1 und 4.2 |
| 21.11.2026 | **Schlüssel bleiben, aber nur noch in GitHub-Environments** (`signing` für die Testversionen von `main` und `dev`, `release` für Veröffentlichungen); alle anderen Läufe signieren mit einem Wegwerf-Schlüssel; kein Release von Hand | Großprüfung, Befund H1 und H2: Der Schlüssel stand jedem Push auf jeden Branch zur Verfügung. Ein Wechsel der Schlüssel war dafür nicht nötig, der Inhaber behält sie | [PROJEKT](../PROJEKT.md), „Signatur“; [AUDIT](AUDIT.md), Abschnitt 4.1 |
| 21.11.2026 | **Kein Testserver mehr** (dev.danerd.de): Pakete von Branches und Pull Requests gibt es nur als GitHub-Artefakte, die Vorschau der Website auf dem Server entfällt | Entscheidung des Inhabers: Die Zugangsdaten des Testservers mussten jedem Branch offenstehen (Großprüfung, Runde 5) | [PROJEKT](../PROJEKT.md), „CI und Signatur“ |
| 21.11.2026 | **Keine externe oder bezahlte Prüfung** | Entscheidung des Inhabers: Hobbyprojekt, getragen von der Community; geprüft wird mit KI-Agenten, eigenen Gerätetests und einer Beta | Abschnitt 3 |
| 22.11.2026 | **App-PIN neu:** 8 bis 12 Ziffern, zwei Einmal-Schlüssel im Android-Keystore, an die PIN gebunden; nach 2 falschen nur noch Passwort und zweiter Faktor. Alte App-PINs sind neu einzurichten | Gutachten zu M19: Die alte App-PIN war mit Root außerhalb des Geräts durchprobierbar; Entscheidung des Inhabers zu Länge und Versuchen | [KONZEPT](KONZEPT.md), Abschnitt 6.5; Abschnitt 2.4 |
| 22.11.2026 | **Android 12 als Mindestversion** (`minSdk` 31, vorher Android 10) | Erst ab API 31 gibt es die Einmal-Schlüssel; Entscheidung des Inhabers, direkt umzustellen. Direkt installierte Apps auf Android 10 und 11 bekommen keine Updates mehr | [KONZEPT](KONZEPT.md), Abschnitt 16; [AUDIT](AUDIT.md), Abschnitt 7 |
| 22.11.2026 | **Release aus dem CI-Lauf** (`workflow_call` statt `workflow_run`) | Seit PR #50 entstand keine Vorabversion mehr (Abschnitt 2.4); Entscheidung des Inhabers, den Auslöser zu reparieren | [PROJEKT](../PROJEKT.md), „Versionen und Veröffentlichung“ |
| 22.11.2026 | **Härtung nach der Angreifer-Runde** (Funktionsstopp gilt weiter, es sind Fehlerbehebungen): Schutzänderungen werden ganz versiegelt und über ein Journal geschrieben; Sperre bei Untätigkeit im Vordergrund (5 bis 15 Minuten) und nach dem Ruhezustand unter Windows; Master-Passwort vor „Papierkorb leeren“ und vor dem Löschen von 10 oder mehr Einträgen; der ganze Wiederherstellungscode wird bestätigt; neue Keys brauchen `credProtect`; einfache App-PINs abgelehnt; eine Datei aus einer anderen App wird erst nach Rückfrage Speicherort | Runde 9 (fünf Angreifer) und die Runden 10 bis 14 (Abschnitt 2.5) | [KONZEPT](KONZEPT.md); Testprotokoll, Runden 9 bis 14 |
| 22.11.2026 | **Vorschläge zur Lieferkette abgelehnt:** „Immutable releases“, Pflicht-Review durch Code-Owner (die Datei `.github/CODEOWNERS` gibt es), `IMPRESSUM_*` ins Environment `website`, Rulesets für Tags | Entscheidung des Inhabers nach Runde 9; die Lücken dahinter brauchen Schreibrecht am Repository, das nur der Inhaber hat | Abschnitt 2.5; [AUDIT](AUDIT.md), Abschnitt 4.6 |

## 2. Prüfläufe

### 2.1 Bis 20.11.2026

Die Runden vor dem Funktionsstopp stehen ausführlich im [Sicherheitsbericht](AUDIT.md), Abschnitt 1. Kurz:

| Datum | Wer | Prüfung | Ergebnis |
|---|---|---|---|
| 11.11.2026 | KI | Audit 0.5.0 (Freigabelinks, Server-Modul, FTPS, HTTPS-Client, OpenPGP, Abhängigkeiten) | 8 Befunde, behoben bzw. begründet |
| 13.11.2026 | KI | Audit des ganzen Codes in drei unabhängigen Durchsichten; Testrunden 1 bis 3 (Chaos-Sync, Fuzzing, absurde Eingaben, Angriffe, öffentliche Testsammlungen, Emulator) | 6 dringende Punkte und 39 Funde, fast alle behoben |
| 14.11.2026 | KI | Runden 4 bis 8 (Schlüsselerneuerung, Prüfwerkzeuge im Nachtlauf, Bedienung, Mail mit echten Servern, Updates, Website) | 9 Funde, behoben |
| 15. und 16.11.2026 | KI | Runden 9 und 10 (Teilen ohne Kopie, Mail-Archiv, mehrere Speicherorte) | 44 Funde (F106 bis F149), erledigt |
| 17. bis 20.11.2026 | KI | Durchsichten der Umbauten (ohne Synchronisation, GhulDocs, mehrere GhulDocs-Tresore) | 14 und 9 Befunde, behoben; viele frühere Befunde **obsolet** (Funktion entfernt) |

### 2.2 21.11.2026

| Wer | Prüfung | Ergebnis | Einzelheiten |
|---|---|---|---|
| Inhaber | Durchsprache des automatischen Speicherns | Ausgebaut, Speichern-Knopf mit Dialog | MV-18 |
| KI, 2 Agenten | Durchsicht von Speichern-Knopf und Suite-Datei im Assistenten | 1 Befund hoch (mehrere Teile), behoben | MV-19 |
| KI, 2 Agenten | **Prüfrunde 1:** Kern, App | 4 mittel, 2 klein, behoben; kleine Punkte gelistet | PR-1 bis PR-6 |
| KI, 2 Agenten | **Prüfrunde 2:** Android, Windows und CI | 2 mittel, behoben | PR-7, PR-8 |
| KI, 2 Agenten | **Prüfrunde 3:** GhulDocs, GhulPass und GhulCal | 2 mittel, behoben | PR-9, PR-10 |
| KI, 2 Agenten | **Prüfrunde 4:** Sicherheit, Website und Texte | 2 hoch, 1 mittel, behoben; danach auf Wunsch des Inhabers beide Faktoren und Pflicht-Backup | PR-11 bis PR-13 |
| KI, 2 Agenten | **Prüfrunde 5:** Kern, App | 2 mittel, behoben | PR-14, PR-15 |
| KI, 1 Agent | Unabhängige Durchsicht der Behebungen aus Runde 5 | 3 mittel, 3 klein, behoben | PR-16 |
| KI, 1 Agent | Nachprüfung dieser Behebungen (neuer Agent, kannte sie nicht) | 1 hoch (ein entfernter Key hätte durch Überschreiben zurückkommen können), 1 klein, behoben | PR-16 |
| CI | Läufe von Android, Windows und Emulator zum Pull Request | Android-Build seit Runde 4 rot (lokal ohne Android-SDK nicht sichtbar), Oberflächentests und zwei Emulator-Abläufe nicht nachgezogen; behoben, dazu eine Prüfung der Emulator-Abläufe vor dem Push | PR-17, PR-18 |
| KI, 1 Agent | **Gutachten: Kann GhulSuite an die Öffentlichkeit?** Kryptografie, Dateiformat, Datenverlust, Plattformen, Updates, Bedienung, Build, Rechtliches; Dokumente als Behauptungen geprüft | **Noch nicht reif für die Öffentlichkeit, eine begrenzte, klar gekennzeichnete Beta mit Testdaten ist möglich.** Blocker: keine Formatzusage, Signieren ohne Freigabe, keine Prüfung durch Dritte und keine festgehaltenen Gerätetests. Die beiden schwersten Code-Funde wurden nachgeprüft und bestätigt | Abschnitt 3 |
| KI, 1 Agent | Durchsicht des sicheren Speicherns unter Android vor dem Push (ohne Android-SDK, gegen das Verhalten von AOSP gelesen) | 2 hoch (nach dem ersten Tausch „nicht erreichbar“, Wiederherstellung nie erreicht), 1 mittel (stille Namenskollision meldete „gespeichert“), 3 klein; behoben | PR-20 |
| KI, 1 Agent | Nachprüfung dieser Korrektur (neuer Agent; Kontrollfluss mit dem Kotlin-Compiler gegen Attrappen gebaut) | 1 mittel und 2 klein, nur bei Doppel-Störungen oder gleichzeitigem Zugriff; behoben (Sperre, `.alt` nur mit eigener Notiz löschen, sonst aufheben); 1 klein offen (K-37) | PR-20 |
| KI, 1 Agent | Dritte Prüfung des sicheren Speicherns unter Android (Kontrollfluss mit dem Kotlin-Compiler gebaut) | kein Fund hoch oder mittel; 3 klein: die Notiz zum fertigen Tausch wird jetzt sofort geschrieben (`commit`), 2 kommen auf die Liste (K-38) | PR-20 |
| Lokal (neu: Android-SDK in der Sitzung) | Vollständiger Precheck mit signierter APK, Lint, Desktop-Tests und Paketierung | grün, erstmals mit Android-Build vor dem Push | |

### 2.3 21. und 22.11.2026: Großprüfung

Eine Prüfung in mehreren Stufen, jeder Prüfer ein neuer KI-Agent (Claude) ohne Kenntnis der anderen Berichte. Die
Rohberichte und die Auswertung liegen nicht im Repository; die Funde mit Nummer, Test und Stand stehen im
[Testprotokoll](TESTPROTOKOLL.md), Abschnitt 12.

| Wer | Prüfung | Ergebnis | Einzelheiten |
|---|---|---|---|
| KI, 10 Agenten | **Welle 1:** ganzheitlich, erst das Gesamtbild, dann dort in die Tiefe, wo der Prüfer selbst das größte Risiko sieht | zusammen mit den Wellen 2 bis 4 ausgewertet (unten) | Testprotokoll 12 |
| KI, 5 Agenten | **Welle 2:** als Angreifer (Dieb mit gesperrtem Gerät, kurz entsperrte App, präparierte Dateien, Lieferkette und Netz, andere Apps auf dem Gerät) | wie oben | Testprotokoll 12 |
| KI, 5 Agenten | **Welle 3:** Nutzerwege über Monate (Key verloren, zwei Geräte mit Stick, Gerätewechsel und Updates, Erben mit Notfallblatt, Vielnutzer an den Grenzen) | wie oben | Testprotokoll 12 |
| KI, 5 Agenten | **Welle 4:** Methoden (Testqualität mit Mutationsprobe, Fehlerpfade, Nebenläufigkeit, Versprechen gegen Code, Wartbarkeit) | wie oben | Testprotokoll 12 |
| KI (Auswertung) | Zusammenführen der 25 Berichte, doppelte Funde vereint, die wichtigsten im Code nachgeprüft | **Einstimmig: noch nicht reif für die breite Öffentlichkeit, eine geschlossene, klar gekennzeichnete Beta mit Testdaten ist vertretbar.** Kein Prüfer kam ohne Master-Passwort plus zweiten Faktor (oder den Wiederherstellungscode) an die Daten einer Suite-Datei. **13 hoch** (H1 bis H13), **44 mittel** (M1 bis M44), dazu kleine Punkte. Die Probleme lagen an den Rändern: Signaturschlüssel in der CI, Notfallweg, „Mit App öffnen“ unter Windows, fehlende Abfragen, seltene Datenverluste, Texte, die mehr versprachen als der Code | Testprotokoll 12 |
| KI, 2 Agenten (Entwickler A: Kern, Workflows, Werkzeuge; Entwickler B: App, README, Website) | Behebung der Funde, danach gegenseitige Durchsicht (A über B, B über A) | A über B: 5 mittel, 7 klein; B über A: 1 hoch (ein Aufräumfehler ließ eine schon geschriebene Schutzänderung als gescheitert erscheinen), 6 mittel, 6 klein; behoben | Testprotokoll 12 |
| KI, 5 Agenten | **Runde 5:** zwei prüfen die behobenen Schwächen (Kern und CI, App), drei das ganze Projekt (Nutzer über Monate, Angreifer, Robustheit und Wartbarkeit) | 6 hoch (u. a. Schnell-Entsperren im Assistenten scheiterte immer und verbrauchte PIN-Versuche des Keys; der Schutz des Schlüssels hing an Einstellungen außerhalb des Repositorys; nach dem Merge nach `main` entstand keine stabile Version), 21 mittel, 38 klein (teils doppelt); behoben bis auf die unten genannten | Testprotokoll 12 |
| KI, 1 Agent | **Schlussprüfung (r6)** nur der Änderungen aus Runde 5 | kein Fund hoch; 4 mittel (Löschen ließ ein Ersetzungs-Journal liegen, das eine neue Suite hätte überschreiben können; Marke mit absolutem Pfad; Prüfung des Gradle-Wrappers verhinderte den Gradle-Lauf nicht; Ausweich-Anker beim Entsperren), 7 klein; alle behoben, der Punkt „Dokumentation veraltet“ mit diesem Nachtrag | Testprotokoll 12 |
| KI, 1 Agent | **Runde 7** (nachgetragen am 22.11.2026): Behebung von M8 (Tresor entfernen, Exporte melden Fehler) und M21 (Windows nimmt nur USB oder NFC) | kein Fund hoch; 2 mittel (Android ließ nach einem gescheiterten Speichern eine leere oder halbe Datei liegen, auch nach einer Neuerzeugung der App), 7 klein; behoben | Testprotokoll, Runde 7 |
| Inhaber | Entscheidungen | Schlüssel bleiben (nur in Environments), keine externe Prüfung, kein Testserver | Abschnitt 1 |
| Lokal | Vollständiger Precheck (22.11.2026) | grün: Kern 305 Tests bestanden, 7 ignoriert; Desktop 159 Tests und 20 in `desktopAppBindingTest`. Windows (Clippy, Build, Tests) und der Emulator laufen nur in der CI | Testprotokoll 1 |

**Behoben** (Einzelheiten im Testprotokoll, Abschnitt 12): 12 der 13 hohen Funde ganz, H11 zum Teil (ehrlicher Dialog
und ein Weg über „Als Tresor weitergeben“, kein Zusammenführen); von den mittleren alle bis auf die unten genannten
(einige mit Restrisiko).

**Offen nach der Großprüfung:**
- Die Environments und Secrets richtet der Inhaber in GitHub ein (Environments `signing`, `release`, `website`,
  Repository-Variable `GHUL_UPDATE_KEY`, danach die Secrets auf Repository-Ebene löschen; seit 22.11.2026 braucht
  auch `release` die Deployment-Branches `main` und `dev`, Abschnitt 2.4). Bis dahin meldet der Job
  `secrets-check` auf `main` und `dev` einen Fehler, und der Schutz aus H1 wirkt noch nicht.
- M44 (Android-Code ohne Unit-Tests) und die übrigen kleinen Punkte, soweit nicht im Testprotokoll als behoben
  genannt. M8 und M21 wurden am 22.11.2026 nachträglich behoben; M21 (nur USB oder NFC unter Windows) ist erst
  in der Windows-CI gebaut und muss auf einem echten Windows-11-Gerät mit Handy nachgeprüft werden.
- Restrisiken (AUDIT 4.6): das MSI-Update ist nur als Text des Skripts getestet (M24); Windows und Emulator laufen
  nur in der CI; die App-PIN ist mit Root offline angreifbar (M19; am 22.11.2026 neu gebaut, das Restrisiko steht
  in Abschnitt 2.4); Erinnerungen mit „Alles“ (M12, M18).
- Festgehaltene Tests auf echten Geräten (Testplan, besonders E19e und Abschnitt 40) und eine Beta mit Leuten aus der
  Community.

### 2.4 22.11.2026: App-PIN (M19), Android 12, Release

| Wer | Prüfung | Ergebnis | Einzelheiten |
|---|---|---|---|
| KI, 2 Agenten | **Gutachten zu M19:** Wie lässt sich die App-PIN gegen ein gerootetes Gerät halten, was bringt Android 12 als Mindestversion? | Die App-PIN war mit Root **offline** angreifbar: Der Keystore-Schlüssel gab den Schnellschlüssel ohne PIN frei, die PIN steckte nur im Argon2id, der Zähler in den Einstellungen half nicht. KONZEPT 6.5 („außerhalb des Geräts nicht möglich“) und AUDIT GP-M19 („auf dem Gerät“) waren falsch, beide richtiggestellt. Empfehlung: zwei an die PIN gebundene, importierte Einmal-Schlüssel (`setMaxUsageCount(1)`), mehr Ziffern, Android 12 | [KONZEPT](KONZEPT.md) 6.5, [AUDIT](AUDIT.md) GP-M19 |
| Inhaber | Entscheidungen | App-PIN mit mindestens 8 Ziffern und **2 Versuchen**; **Android 12** (`minSdk` 31) direkt, ohne eine letzte Version für Android 10 und 11; der Auslöser des Release wird repariert | Abschnitt 1 |
| KI, 2 Agenten (Entwickler A: Kern, Workflows; Entwickler B: App) | Umsetzung: App-PIN neu (Schnell-Datei Version 4 im Kern, Einmal-Schlüssel in der App), `minSdk` 31, Release aus dem CI-Lauf | Dabei gefunden: `workflow_run` läuft nur mit der Fassung des Standardzweigs `main`, und dort gab es `release.yml` nicht: Seit PR #50 entstand **keine Vorabversion** mehr. Jetzt ruft der letzte Job von `android.yml` den Release auf (`workflow_call`); das Environment `release` muss dafür auch `dev` zulassen (stellt der Inhaber ein) | [PROJEKT](../PROJEKT.md), „Versionen und Veröffentlichung“; [AUDIT](AUDIT.md) 4.2, K-7 |
| KI, 1 Agent | **Durchsicht r8** der Umsetzung (App-PIN, `minSdk` 31, Release) | kein Fund hoch; 4 mittel (die Website verlinkte eine Vorabversion, die später gelöscht wird; ein gescheitertes Neu-Einrichten löschte die vorhandene App-PIN, auch bei falschem Passwort; das Environment `release` ließ nur `main` zu, die Doku beschrieb noch `workflow_run`; ein erneuter Lauf eines älteren Commits hätte alten Code als neue Version veröffentlicht), 9 klein; behoben. Ob der Keystore im Emulator die Schlüssel als sichere Hardware meldet (sonst lässt sich die App-PIN dort nicht einrichten), zeigt erst der Emulator-Lauf des Pull Requests | Testplan Abschnitt 41 |

**Restrisiko** (AUDIT GP-M19): Auf Geräten ohne rollback-festen Keystore (die meisten ohne StrongBox) könnte jemand mit
Root die Keystore-Datenbank zurückspielen und weiter raten, aber nur auf dem Gerät und langsam (bei 8 Ziffern Jahre).
**Richtiggestellt am 22.11.2026 (Runde 9, Krypto N1):** „Jahre“ ist zu günstig. Mit Root lässt sich Argon2id für
alle 10^8 PINs auf fremder Hardware vorrechnen (grob ein Tag auf einem großen Server); auf dem Gerät bleibt je
Versuch nur ein HMAC im TEE und das Zurückspielen, zusammen **Tage bis wenige Wochen**.
Auf Geräten mit StrongBox erzwingt das die Hardware; das beruht auf der AOSP-Dokumentation und ist auf einem Gerät
noch nicht nachgeprüft. Die App-PIN ist im echten Keystore noch nicht getestet (Testplan Abschnitt 41).

### 2.5 22.11.2026: Angreifer-Runde und Behebungsrunden 10 bis 14

Jeder Prüfer ein neuer KI-Agent (Claude) ohne Kenntnis der anderen Berichte; behoben haben die Entwickler-Agenten A
(Kern, Workflows, Werkzeuge) und B (App, README, Website). Einzelheiten mit Test je Fund im
[Testprotokoll](TESTPROTOKOLL.md), Runden 9 bis 14.

| Wer | Prüfung | Ergebnis | Einzelheiten |
|---|---|---|---|
| KI, 5 Agenten | **Runde 9, Angreifer:** Dieb und Finder (Handy, Laptop, Stick, Notfallblatt), präparierte Dateien (mit eigenen Programmen gegen den echten Kern nachgestellt), Schadsoftware auf demselben Gerät, Lieferkette (Fork-PR, Abhängigkeit, Re-Run, Tags, Netz), Kryptografie und Format | Keiner kam ohne Master-Passwort plus zweiten Faktor (oder den Code) an den Inhalt einer Suite-Datei. **4 hoch:** eine Schutzänderung nach einem zu großen Tresor sperrte aus einem anderen Tresor aus; das Windows-Update startete `powershell` ohne Pfad; `publish` nahm jedes Artefakt `release-*` des Laufs; ein neuer Wiederherstellungscode ließ sich von einem zweiten Gerät beim Speichern rückgängig machen. **13 mittel**, **32 klein**. Behoben bis auf die unten genannten | Testprotokoll, Runde 9 |
| Inhaber | Entscheidungen zur Lieferkette | „Immutable releases“, Pflicht-Review durch Code-Owner, `IMPRESSUM_*` ins Environment `website` und Rulesets für Tags: abgelehnt | Abschnitt 1 |
| KI, 1 Agent | **Runde 10:** alle Behebungen der Runde 9 | Alle vier hohen Funde geschlossen. Kein Fund hoch; 5 mittel (der ganze Code in einem Feld, das die Tastatur lernt; Einschalten der Anzeige brach lange Aufträge ab; die Leerlauf-Sperre sah die Bildschirmtastatur nicht; neues Schnell-Entsperren galt sofort als abgelaufen; Verschieben in einen vollen Tresor nahm nichts zurück), 5 klein; behoben, dazu das Journal für Schutzänderungen | Testprotokoll, Runde 10 |
| KI, 1 Agent | **Runde 11:** das neue Journal und die Sperre | 1 hoch (ein liegengebliebenes Journal schrieb nach dem Löschen und Neueinrichten die alte Suite über die neue, mit einer Probe bestätigt), 3 mittel (keine gemeinsame Sperre, ungespeicherte Eingabe schaltete die Leerlauf-Sperre ganz ab, Kopien mit altem Schutz blieben), 4 klein; behoben | Testprotokoll, Runde 11 |
| KI, 1 Agent | **Runde 12:** die Behebungen aus Runde 11 | Kein Fund hoch; 1 mittel (ein Entsperren, das vor einer laufenden Passwortänderung gelesen hatte, öffnete danach mit den alten Schlüsseln und schrieb den alten Schutz zurück, mit einer Probe bestätigt), 5 klein; behoben (Zähler `protection_changes`) | Testprotokoll, Runde 12 |
| KI, 1 Agent | **Runde 13:** die Behebungen aus Runde 12 | Kein Fund hoch; 1 mittel (ein Speicherfehler oder `Locked` schaltete den Fingerabdruck ab), 4 klein; behoben | Testprotokoll, Runde 13 |
| KI, 1 Agent | **Runde 14:** die Behebungen aus Runde 13 | Kein Fund hoch, kein Fund mittel; 2 klein: einer behoben (ein offenes Journal ohne Suite-Datei behielt seine Kopien nicht), einer offen (ein Merker nach einem Wettlauf mit der Übernahme, kein Datenverlust) | Testprotokoll, Runde 14 |
| Lokal | Kern und Desktop | Kern 341 Tests bestanden, 7 ignoriert (348 Testfunktionen); Desktop 168 in `desktopTest` und 28 in `desktopAppBindingTest`. Windows und der Emulator laufen nur in der CI (PR #52) | Testprotokoll, Gesamtstand |

**Gezählt seit dem 21.11.2026:** 28 Prüfdurchgänge mit 63 Prüf-Agenten (Abschnitte 2.2 bis 2.5; die Entwickler-Agenten
und die Agenten für die Dokumentation nicht mitgezählt). Wie gezählt wurde, steht im Gesamtstand des Testprotokolls.

**Offen nach Runde 14:**
- PR #52 grün (Emulator mit App-PIN und ganzem Wiederherstellungscode, Windows).
- Handtests auf echten Geräten: App-PIN auf einem echten Handy samt StrongBox, Fingerabdruck im Assistenten
  (Schritt 6), sicheres Speichern unter Android, die neuen Schritte (Testplan Abschnitt 42, Testplan Windows
  Abschnitt 15), unter Windows 11 das Update über das MSI und die Ablehnung eines Handy-Passkeys.
- Kleine offene Funde: Runde 14 N2; Klartext-Export mit dem Passwort allein (Entscheidung steht aus); das Salz unter
  Windows (nur als Handtest); Fingerabdruck des Signaturzertifikats nicht veröffentlicht.
- Restrisiken ([AUDIT](AUDIT.md), Abschnitt 4.6), neu dazu: App-PIN mit Root auf Geräten ohne rollback-festen
  Keystore (Tage bis Wochen), präparierte fremde Tresore mit sehr viel CBOR, Apps mit Zugriff auf Benachrichtigungen,
  Schadsoftware desselben Benutzers unter Windows, Programmtausch auf dem portablen Stick.

**Einschätzung:** bereit für eine öffentliche Beta, sobald PR #52 grün ist und die Handtests festgehalten sind;
eine stabile Version erst nach einigen Monaten Beta ohne schweren Fund.

## 3. Gutachten vom 21.11.2026 und was daraus wurde

| Punkt des Gutachtens | Stand |
|---|---|
| Keine Zusage zum Dateiformat | **Erledigt:** Format 3 eingefroren, echte Dateien als dauerhafte Testfälle (PR-19) |
| Ein übernommenes GitHub-Konto reicht für signierte Updates; Signieren ohne vorgeschaltete Tests | Schlüssel bleiben (Entscheidung des Inhabers, Konto mit Hardware-Key). **Erledigt:** Veröffentlichung nur nach grüner CI für genau diesen Commit, Workflow vom Standardzweig; die falsche Aussage im AUDIT richtiggestellt (K-7). **Geändert am 22.11.2026:** Der Workflow vom Standardzweig lief nie, weil es `release.yml` auf `main` nicht gab; jetzt ruft der letzte Job der CI den Release auf (Abschnitt 2.4). Seit der Großprüfung liegt der Schlüssel nur noch in den Environments `signing` und `release` (Abschnitt 2.3) |
| Keine Prüfung durch Dritte, keine festgehaltenen Gerätetests | Prüfung durch Dritte: **entfällt** (Entscheidung des Inhabers, Abschnitt 1). Gerätetests: **offen**, sie kommen mit Datum in Abschnitt 2 |
| Speichern unter Android leert die einzige Suite-Datei zuerst | **Umgesetzt, auf einem Gerät noch nicht geprüft:** mit einmal freigegebenem Ordner (Stick, SD-Karte, Gerätespeicher) neue Datei daneben, prüfen, dann tauschen; ohne Freigabe oder bei Cloud-Apps ein Hinweis (PR-20, Testplan E19e) |
| Übernehmen einer Suite-Datei ersetzt Datei für Datei | **Erledigt:** Journal, nach einem Absturz kommt der Stand davor vollständig zurück (PR-19) |
| Windows-Installer ohne Authenticode-Signatur | **Offen**, braucht ein Zertifikat des Inhabers (z. B. SignPath für Open Source) |
| Rust-Version im Release nicht festgelegt | **Erledigt:** `rust-toolchain.toml` |
| Altes Paket unter neuem Tag (K-6) | **Erledigt:** die signierte Prüfsummenliste nennt ihre Version, die App prüft sie |
| Präparierte Datei kann über den Speicherbedarf abstürzen (K-19) | **Erledigt:** abgeleitet wird mit höchstens 512 MiB |
| Wiederherstellungscode öffnet alles allein, deutlich machen | War schon so: Einrichtung und Notfallblatt sagen, dass der Code alles allein öffnet |
| Doku widerspricht sich (Testzahlen, doppelte Abschnittsnummer) | **Erledigt** |
| Weg zur Veröffentlichung | Funktionsstopp (seit 21.11.2026), Behebung der Großprüfung (Abschnitt 2.3), Gerätetests, geschlossene Beta, öffentliche Beta 0.9, 1.0 erst nach einigen Monaten ohne schweren Fund |

## 4. Vorlage für neue Einträge

Unter 2. einen neuen Abschnitt mit dem Datum anlegen, dann je Prüfung eine Zeile:

| Wer | Prüfung | Ergebnis | Einzelheiten |
|---|---|---|---|
| Inhaber / KI (Zahl der Agenten) / extern (Name) / CI | Was geprüft wurde (Bereiche, Geräte und Versionen, Testplan-Abschnitte) | Funde nach Schwere und was daraus wurde, oder „ohne Fund“ | Nummern im Testprotokoll |

Für einen Gerätetest des Inhabers: Gerät und Android- bzw. Windows-Version, App-Version, welche Abschnitte des
[Testplans](TESTPLAN.md) bzw. [Testplans Windows](TESTPLAN-WINDOWS.md), Security Keys und Speicherorte (Stick,
SD-Karte, Cloud-App), was nicht ging.
