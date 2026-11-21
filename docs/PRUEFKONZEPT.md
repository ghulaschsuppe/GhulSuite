<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# Prüfkonzept: mehrere Agenten prüfen, beheben, prüfen nach

Ziel: Fehler früh und zuverlässig finden, ohne dass jede Runde neue Funde an schon geprüften Stellen bringt.
Das Konzept ersetzt keine externe Prüfung (Abschnitt 6), macht sie aber billiger und ergiebiger.

## 1. Warum immer wieder Funde auftauchen

- Jede Runde ändert Code. Behebungen erzeugen neue Stellen, die noch niemand geprüft hat.
- Prüfer ohne festen Auftrag suchen dort, wo es leicht ist. Schwer zugängliche Pfade (Fehlerfälle,
  Abbruch mitten im Schreiben, zweites Gerät) bleiben liegen, bis sie zufällig jemand ansieht.
- Ein Prüfer, der die Behebung selbst geschrieben hat, sieht seine eigenen Annahmen nicht.

Die Zahl der Funde sinkt deshalb nur, wenn (a) die Bereiche fest verteilt sind, (b) Beheben und Prüfen
getrennt sind und (c) jeder Fund einen Test hinterlässt, der ihn für immer festhält.

## 2. Rollen

| Rolle | Auftrag | Darf ändern |
|---|---|---|
| Prüfer (je Bereich einer) | Sucht Fehler in seinem Bereich nach einer festen Liste (Abschnitt 3). Meldet nur, was er im Code belegt hat: Datei und Zeile, Schwere, konkreter Ablauf, der schiefgeht | nein |
| Gegenprüfer | Bekommt jeden Fund ohne die Begründung des Prüfers und versucht, ihn zu widerlegen. Nur bestätigte Funde gehen weiter | nein |
| Beheber | Behebt einen bestätigten Fund, schreibt zuerst einen Test, der ohne Behebung scheitert | ja |
| Nachprüfer (frisch, kennt die Behebung nicht) | Prüft den Diff der Behebung: Ist der Fund weg? Ist etwas Neues entstanden? Läuft der Test wirklich durch den Fehlerpfad? | nein |
| Leitung (Mensch oder Hauptagent) | Verteilt, entscheidet bei „klein“ und bei Streit, führt AUDIT.md und TESTPROTOKOLL.md | ja |

Wichtig: Prüfer, Gegenprüfer und Nachprüfer sind jeweils **neue** Agenten ohne Kenntnis der früheren
Begründungen. Sonst bestätigen sie nur, was man ihnen sagt.

## 3. Bereiche und feste Prüflisten

Jeder Bereich hat eine Liste, die mit jedem Fund wächst. Beispiele:

- **Kryptografie und Schutz:** Welche Faktoren öffnen was? Kann eine Schutzänderung mit weniger als
  beiden Faktoren geschehen? Bleibt nach einer Änderung ein alter Schlüssel irgendwo gültig?
- **Dateien und Absturzfestigkeit:** Jede Schreibstelle: Was liegt auf der Platte, wenn genau hier der
  Strom ausfällt? Gibt es einen Moment ohne gültige Datei?
- **Mehrere Geräte und Stände:** Gerät A und B ändern dieselbe Suite-Datei. Wer überschreibt wen? Was
  passiert mit Fingerabdruck/PIN, Epoche, „gesehenen Ständen“?
- **Sperren und Nebenläufigkeit:** Sperren mitten in jeder lang laufenden Aktion. Was bleibt offen?
- **Plattformen:** Android (Hintergrund, Berechtigungen, Zwischenablage), Windows (Dateidialoge, Update,
  Zwischenablage), Website (PHP 8.1, keine Datenbank).
- **Import/Export und fremde Daten:** Jede Eingabe von außen ist feindlich: Größe, Kodierung, kaputte
  Struktur.
- **Texte und Versprechen:** Sagt jeder Text in App, Website und Doku nur, was der Code wirklich tut?

## 4. Ablauf einer Runde

1. **Einfrieren:** Ein Stand (Commit) wird geprüft, nicht ein bewegliches Ziel.
2. **Prüfen:** Je Bereich ein Prüfer, parallel. Jeder liefert eine Liste nach festem Schema.
3. **Gegenprüfen:** Jeder Fund an einen Gegenprüfer. Widerlegt: verworfen, mit Grund notiert.
4. **Sortieren:** hoch und mittel werden behoben, klein kommt in AUDIT.md 4.4.
5. **Beheben:** je Fund ein Beheber; zuerst der scheiternde Test, dann die Behebung, dann
   `tools/precheck.sh`.
6. **Nachprüfen:** frischer Nachprüfer je Behebung. Neue Funde gehen zurück zu Schritt 3.
7. **Festhalten:** Eintrag im TESTPROTOKOLL mit Testname; Prüfliste des Bereichs um die Frage ergänzt,
   die den Fund gefunden hätte.

## 5. Wann ist es genug?

- **Abbruchregel:** Zwei Runden in Folge ohne bestätigten Fund der Schwere hoch oder mittel, mit
  vollständig abgearbeiteten Prüflisten, dazu alle Tests grün (Kern, Desktop, Android-Emulator).
- Danach prüfen nur noch Änderungen: Jeder neue PR bekommt einen Prüfer je berührtem Bereich und einen
  Nachprüfer.
- Ein Fund nach der Abbruchregel ist kein Scheitern, sondern ein Hinweis auf eine Lücke in der
  Prüfliste: Die Liste wird ergänzt, der Bereich bekommt eine eigene Runde.

## 6. Was Agenten nicht ersetzen

- **Externe Prüfung der Kryptografie und des Dateiformats** durch Fachleute vor der ersten
  öffentlichen Version.
- **Tests auf echten Geräten** mit echten Security Keys (verschiedene Hersteller, NFC und USB) und
  echten USB-Sticks (voll, abgezogen).
- **Fuzzing** der Lesepfade (Suite-Datei, Tresordateien, Import) über längere Zeit.
- **Rückmeldungen echter Nutzer** in einer kleinen, geschlossenen Testphase.
