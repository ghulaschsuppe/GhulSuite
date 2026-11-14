# Sicherheit

GhulSuite schützt Passwörter, 2FA-Schlüssel, Dateien und Mails. Sicherheitslücken nehme ich sehr ernst.

## Eine Lücke melden

**Bitte nicht öffentlich als Issue melden.** Nutze stattdessen die private Meldung in diesem Repository:
**Security → Report a vulnerability**. Nur der Inhaber sieht die Meldung.

Hilfreich sind:
- betroffene Version (Optionen → Über) und Plattform (Android-Version, Windows-Version),
- eine Beschreibung, wie sich das Problem auslösen lässt,
- was ein Angreifer dadurch erreichen kann.

Bitte keine echten Passwörter, Tresordateien oder Schlüssel mitschicken.

## Was danach passiert

- Eingangsbestätigung innerhalb von **7 Tagen**.
- Einschätzung und Plan innerhalb von **30 Tagen**.
- Die Korrektur erscheint in einer neuen Version; danach wird die Lücke im Sicherheitsbericht
  ([`docs/AUDIT.md`](docs/AUDIT.md)) beschrieben, auf Wunsch mit Nennung der meldenden Person.
- Bitte gib uns Zeit für die Korrektur, bevor du Details veröffentlichst (in der Regel 90 Tage).

## Unterstützte Versionen

GhulSuite ist in Entwicklung. Korrekturen gibt es für den aktuellen Stand von `main` und die jeweils
neueste veröffentlichte Version.

## Was als Sicherheitslücke zählt

Zum Beispiel: Zugriff auf Tresorinhalte ohne Master-Passwort **und** zweiten Faktor, Schwächen im Dateiformat
oder in der Verschlüsselung, Datenabfluss an Server oder Logs, Umgehen der Sperre, Lücken im PHP-Server-Modul
oder auf der Empfängerseite für Freigabelinks.

Kein Sicherheitsproblem im engeren Sinn sind Angriffe, die ein bereits vollständig kompromittiertes, entsperrtes
Gerät voraussetzen.
