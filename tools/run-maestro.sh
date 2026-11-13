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
  # The servers listen on 127.0.0.1 of this computer. "adb reverse" makes the same ports the
  # device's own 127.0.0.1 (the emulator's 10.0.2.2 alias is not reliable on every version).
  echo "Testserver auf diesem Rechner:"
  ports="2223 10993 10587"
  for p in $ports; do
    if (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "  Port $p lauscht"; else echo "::error::Testserver-Port $p lauscht nicht"; exit 1; fi
    adb reverse "tcp:$p" "tcp:$p" > /dev/null
  done
  adb shell "for p in $ports; do if nc -z -w 3 127.0.0.1 \$p; then echo \"  Gerät erreicht Port \$p\"; else echo \"  Gerät erreicht Port \$p NICHT\"; fi; done" || true
  timeout 900 maestro test "$(dirname "$0")/../app/maestro/server" --format junit --output "$out/report-server.xml" --test-output-dir "$out"
fi
