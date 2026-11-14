#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# CI only, after the emulator flows: reads the whole system log of the run (tools/ci-emulator.sh
# records it from the start) and fails on what no screenshot shows:
#   - a crash of the app (FATAL EXCEPTION) or "app isn't responding" (ANR),
#   - an alarm of the development build (GHUL-DEV-ALARM: a dropped follow-up action, an unexpected
#     error on the screen).
# Reported, but not (yet) failing: disk or network access on the main thread in our own code
# (StrictMode), which makes the app stutter.
# Usage: tools/check-logcat.sh <logcat file> <output-dir>
set -euo pipefail
log="$1"
out="${2:-.}"
pkg="de.danerd.ghulsuite.dev"
fail=0
[ -s "$log" ] || { echo "::warning::Kein Systemprotokoll ($log)"; exit 0; }

crashes=$(grep -n -A 25 "FATAL EXCEPTION" "$log" | grep -B 2 -A 25 "Process: $pkg" || true)
if [ -n "$crashes" ]; then
  printf '%s\n' "$crashes" > "$out/logcat-crashes.txt"
  echo "::error::Die App ist abgestürzt (siehe logcat-crashes.txt): $(printf '%s\n' "$crashes" | grep -m 1 -oE '[A-Za-z.]+(Exception|Error)[^ ]*.*' || true)"
  fail=1
fi
if grep -q "ANR in $pkg" "$log"; then
  grep -A 12 "ANR in $pkg" "$log" > "$out/logcat-anr.txt" || true
  echo "::error::Die App hat nicht mehr reagiert (ANR, siehe logcat-anr.txt)"
  fail=1
fi
if grep -q "GHUL-DEV-ALARM" "$log"; then
  grep "GHUL-DEV-ALARM" "$log" | sed 's/.*GHUL-DEV-ALARM: //' | sort | uniq -c > "$out/logcat-dev-alarms.txt"
  while read -r line; do echo "::error::Alarm der Entwicklerversion: $line"; done < "$out/logcat-dev-alarms.txt"
  fail=1
fi
# StrictMode: only violations with a frame of our own code.
awk '/StrictMode policy violation/ { buf=$0; keep=0; n=0; inv=1; next }
     inv && n < 40 { buf = buf "\n" $0; n++; if ($0 ~ /de\.danerd\.ghulsuite/) keep=1; if (n == 40) { if (keep) print buf "\n---"; inv=0 } }' "$log" \
  > "$out/logcat-strictmode.txt" || true
count=$(grep -c -- '^---$' "$out/logcat-strictmode.txt" || true)
if [ "${count:-0}" -gt 0 ]; then
  echo "::warning::$count Zugriffe auf Datei oder Netz im Hauptfaden aus eigenem Code (logcat-strictmode.txt)"
else
  rm -f "$out/logcat-strictmode.txt"
fi
[ "$fail" = 0 ] && echo "Systemprotokoll: kein Absturz, kein Hänger, kein Alarm der Entwicklerversion"
exit "$fail"
