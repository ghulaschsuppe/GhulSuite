#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Installs the dev APK on the running emulator/device and runs the Maestro flows.
# Usage: tools/run-maestro.sh path/to/ghulsuite-dev.apk [output-dir]
#
# The flows in app/maestro (setup, GhulPass, GhulAuth, GhulCal, app PIN, options, the areas in
# depth, dark design, landscape, the suite file), then the app's process ended in the background
# the way Android does it (app/maestro/extra/killed-*), then the German tour with normal and with
# large font. The app makes no connections the flows depend on: no test servers are needed.
set -euo pipefail
apk="$1"
out="${2:-maestro-out}"
flows="$(dirname "$0")/../app/maestro"
mkdir -p "$out"
timeout 300 adb wait-for-device
adb uninstall de.danerd.ghulsuite.dev > /dev/null 2>&1 || true
timeout 300 adb install -r "$apk"
# English texts, as the flows expect them.
adb shell "settings put system system_locales en-US" > /dev/null 2>&1 || true

# One flow file; the report is named after it.
run() {
  local file="$1" limit="$2"
  shift 2
  local name
  name=$(basename "$file" .yaml)
  echo "::group::Ablauf $name"
  # The status of maestro itself: called as `run … || rc=$?`, set -e does not apply in here.
  local rc=0
  timeout "$limit" maestro test "$@" "$file" --format junit --output "$out/report-$name.xml" --test-output-dir "$out" < /dev/null || rc=$?
  # Maestro's own helper on the device can die (it is installed anew for every flow). That is
  # not the app: the flow runs once more, and the log says so. Any other failure stays a failure.
  if [ "$rc" != 0 ] && grep -q "DeviceServerDiedException\|Device server died" "$out/report-$name.xml" 2>/dev/null; then
    echo "::warning::Ablauf $name: Maestros Gerätedienst ist abgestürzt (nicht die App), der Ablauf läuft noch einmal"
    mv "$out/report-$name.xml" "$out/report-$name-device-server-died.xml"
    rc=0
    timeout "$limit" maestro test "$@" "$file" --format junit --output "$out/report-$name.xml" --test-output-dir "$out" < /dev/null || rc=$?
  fi
  echo "::endgroup::"
  return "$rc"
}

# The flows of app/maestro one by one, in the order of its config.yaml (each depends on the ones
# before it, so the first failure ends the part). One by one, so that a flow whose run Maestro's
# own device helper ended is run once more (see run).
base_flows() {
  local name file
  while read -r name; do
    name="${name%\"}"; name="${name#\"}"
    file=$(grep -l -F -x "name: $name" "$flows"/[01][0-9]-*.yaml | head -n 1)
    [ -n "$file" ] || { echo "::error::Kein Ablauf mit dem Namen „$name“ (app/maestro/config.yaml)"; return 1; }
    run "$file" 900 || return $?
  done < <(sed -n '/flowsOrder:/,$p' "$flows/config.yaml" | sed -n 's/^ *- //p')
}

# The German tour (app/maestro/extra/tour-de.yaml): with the normal font, then with the large
# system font (130 %) and the large display size, as many people set their phones.
tour_de() {
  echo "::group::Rundgang auf Deutsch"
  timeout 900 maestro test -e PREFIX=de "$flows/extra/tour-de.yaml" --format junit --output "$out/report-tour-de.xml" --test-output-dir "$out"
  echo "::endgroup::"
  echo "::group::Rundgang auf Deutsch, große Schrift und Anzeige"
  local dens
  dens=$(adb shell wm density | tr -d '\r' | grep -oE '[0-9]+' | head -n 1)
  adb shell settings put system font_scale 1.3
  [ -n "$dens" ] && adb shell wm density $((dens * 115 / 100))
  local rc=0
  timeout 900 maestro test -e PREFIX=de-gross "$flows/extra/tour-de.yaml" --format junit --output "$out/report-tour-de-gross.xml" --test-output-dir "$out" || rc=$?
  adb shell settings put system font_scale 1.0
  adb shell wm density reset
  echo "::endgroup::"
  return $rc
}

# The app's process ended in the background the way Android does it when memory runs short; back
# in the app it asks to unlock and shows its area again.
killed_in_background() {
  run "$flows/extra/killed-1-before.yaml" 600
  local pkg=de.danerd.ghulsuite.dev pid
  pid=$(adb shell pidof "$pkg" | tr -d '\r' || true)
  [ -n "$pid" ] || { echo "::error::Die App läuft nicht, es gibt keinen Prozess zu beenden"; return 1; }
  echo "Prozess $pkg ($pid) wird beendet wie bei Speichermangel"
  # kill -9 is what Android does. A debuggable build may end its own process (run-as); the
  # test version signed with the real key is not debuggable, so then as root (the emulator's
  # google_apis image has su). "am kill" last: it spares a process with a foreground service.
  adb shell run-as "$pkg" kill -9 "$pid" 2>/dev/null || adb shell su 0 kill -9 "$pid" || adb shell am kill "$pkg"
  sleep 3
  if [ -n "$(adb shell pidof "$pkg" | tr -d '\r' || true)" ]; then echo "::error::Der Prozess der App läuft noch"; return 1; fi
  run "$flows/extra/killed-2-after.yaml" 600
}

base_flows
killed_in_background
tour_de
