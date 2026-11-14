#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Installs the dev APK on the running emulator/device and runs the Maestro flows.
# Usage: tools/run-maestro.sh path/to/ghulsuite-dev.apk [output-dir]
set -euo pipefail
apk="$1"
out="${2:-maestro-out}"
mkdir -p "$out"
timeout 300 adb wait-for-device
adb uninstall de.danerd.ghulsuite.dev > /dev/null 2>&1 || true
timeout 300 adb install -r "$apk"
# English texts, as the flows expect them.
adb shell "settings put system system_locales en-US" > /dev/null 2>&1 || true
timeout 1400 maestro test "$(dirname "$0")/../app/maestro" --format junit --output "$out/report.xml" --test-output-dir "$out"
# With test servers next to the emulator (CI: GHUL_TEST_SERVERS=1, see tools/test-servers.sh):
# GhulDrive and GhulMail against a real server. They continue where the flows above ended.
if [ "${GHUL_TEST_SERVERS:-0}" = 1 ]; then
  echo "Testserver auf diesem Rechner:"
  ports="2223 10993 10587"
  for p in $ports; do
    if (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "  Port $p lauscht"; else echo "::error::Testserver-Port $p lauscht nicht"; exit 1; fi
    adb reverse "tcp:$p" "tcp:$p" > /dev/null 2>&1 || true
  done
  adb reverse --list || true
  # Which address does the device reach the servers at? The SSH server greets with "SSH-": the
  # device's own 127.0.0.1 (adb reverse) or the emulator's alias of the host. The servers only
  # listen on this computer's 127.0.0.1, never on its other interfaces.
  server=""
  for cand in 127.0.0.1 10.0.2.2; do
    greeting=$(timeout 15 adb shell "echo | nc -w 3 $cand 2223 2>&1 | head -c 40" 2>&1 | tr -d '\r' || true)
    echo "  Gerät -> $cand:2223: ${greeting:-keine Antwort}"
    if [ -z "$server" ] && echo "$greeting" | grep -q "SSH-"; then server="$cand"; fi
  done
  [ -n "$server" ] || { echo "::error::Das Gerät erreicht die Testserver auf keinem Weg."; exit 1; }
  echo "Abläufe mit echtem Server über $server"
  flows="$(dirname "$0")/../app/maestro/server"
  timeout 600 maestro test -e GHUL_SERVER="$server" "$flows/1-drive.yaml" --format junit --output "$out/report-server-drive.xml" --test-output-dir "$out"
  # The mail flow is not yet required (GHUL_MAIL_FLOW_REQUIRED=1 makes it so): its failure is a
  # warning, and tools/check-server-flows.sh then skips the mailbox.
  rm -f "$out/mail-flow-failed"
  if ! timeout 600 maestro test -e GHUL_SERVER="$server" "$flows/2-mail.yaml" --format junit --output "$out/report-server-mail.xml" --test-output-dir "$out"; then
    if [ "${GHUL_MAIL_FLOW_REQUIRED:-0}" = 1 ]; then exit 1; fi
    echo "::warning::Ablauf „GhulMail on a real server“ fehlgeschlagen (noch nicht verpflichtend)"
    touch "$out/mail-flow-failed"
  fi
fi
