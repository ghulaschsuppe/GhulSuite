<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# GhulSuite: Prüf-Chronik

Diese Datei hält **jede Prüfung** von GhulSuite fest, in der Reihenfolge, in der sie stattfand, und dazu, **wie und
warum sich das Konzept geändert hat**. Sie ist bewusst offen: Wer GhulSuite nutzen oder prüfen will, soll sehen
können, wer was wann geprüft hat, was dabei herauskam und was noch fehlt.

- **Wer prüft:** bisher der Inhaber (Entscheidungen, eigene Tests) und KI-Agenten (Claude, jeweils neue, voneinander
  unabhängige Agenten je Durchsicht). **Eine Prüfung durch unabhängige Menschen oder eine Firma gab es noch nicht.**
- **Was hier steht:** Datum, wer, was geprüft wurde, das Ergebnis und wo die Einzelheiten stehen. Die einzelnen Funde
  mit Test und Behebung stehen im [Testprotokoll](TESTPROTOKOLL.md) (Nummern F…, MV…, PR…), die offenen kleinen
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

## 3. Gutachten vom 21.11.2026 und was daraus wurde

| Punkt des Gutachtens | Stand |
|---|---|
| Keine Zusage zum Dateiformat | **Erledigt:** Format 3 eingefroren, echte Dateien als dauerhafte Testfälle (PR-19) |
| Ein übernommenes GitHub-Konto reicht für signierte Updates; Signieren ohne vorgeschaltete Tests | Schlüssel bleiben (Entscheidung des Inhabers, Konto mit Hardware-Key). **Erledigt:** Veröffentlichung nur nach grüner CI für genau diesen Commit, Workflow vom Standardzweig; die falsche Aussage im AUDIT richtiggestellt (K-7) |
| Keine Prüfung durch Dritte, keine festgehaltenen Gerätetests | **Offen.** Gerätetests durch den Inhaber kommen mit Datum in Abschnitt 2; eine externe Prüfung (z. B. gefördert über NLnet/NGI) ist vor einer öffentlichen 1.0 vorgesehen |
| Speichern unter Android leert die einzige Suite-Datei zuerst | **Umgesetzt, auf einem Gerät noch nicht geprüft:** mit einmal freigegebenem Ordner (Stick, SD-Karte, Gerätespeicher) neue Datei daneben, prüfen, dann tauschen; ohne Freigabe oder bei Cloud-Apps ein Hinweis (PR-20, Testplan E19e) |
| Übernehmen einer Suite-Datei ersetzt Datei für Datei | **Erledigt:** Journal, nach einem Absturz kommt der Stand davor vollständig zurück (PR-19) |
| Windows-Installer ohne Authenticode-Signatur | **Offen**, braucht ein Zertifikat des Inhabers (z. B. SignPath für Open Source) |
| Rust-Version im Release nicht festgelegt | **Erledigt:** `rust-toolchain.toml` |
| Altes Paket unter neuem Tag (K-6) | **Erledigt:** die signierte Prüfsummenliste nennt ihre Version, die App prüft sie |
| Präparierte Datei kann über den Speicherbedarf abstürzen (K-19) | **Erledigt:** abgeleitet wird mit höchstens 512 MiB |
| Wiederherstellungscode öffnet alles allein, deutlich machen | War schon so: Einrichtung und Notfallblatt sagen, dass der Code alles allein öffnet |
| Doku widerspricht sich (Testzahlen, doppelte Abschnittsnummer) | **Erledigt** |
| Weg zur Veröffentlichung | Funktionsstopp (seit 21.11.2026), Gerätetests, geschlossene Beta, externe Prüfung, öffentliche Beta 0.9, 1.0 erst nach einigen Monaten ohne schweren Fund |

## 4. Vorlage für neue Einträge

Unter 2. einen neuen Abschnitt mit dem Datum anlegen, dann je Prüfung eine Zeile:

| Wer | Prüfung | Ergebnis | Einzelheiten |
|---|---|---|---|
| Inhaber / KI (Zahl der Agenten) / extern (Name) / CI | Was geprüft wurde (Bereiche, Geräte und Versionen, Testplan-Abschnitte) | Funde nach Schwere und was daraus wurde, oder „ohne Fund“ | Nummern im Testprotokoll |

Für einen Gerätetest des Inhabers: Gerät und Android- bzw. Windows-Version, App-Version, welche Abschnitte des
[Testplans](TESTPLAN.md) bzw. [Testplans Windows](TESTPLAN-WINDOWS.md), Security Keys und Speicherorte (Stick,
SD-Karte, Cloud-App), was nicht ging.
