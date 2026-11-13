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
  timeout 900 maestro test "$(dirname "$0")/../app/maestro/server" --format junit --output "$out/report-server.xml" --test-output-dir "$out"
fi
