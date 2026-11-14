#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Installs the dev APK on the running emulator/device and runs the Maestro flows.
# Usage: tools/run-maestro.sh path/to/ghulsuite-dev.apk [output-dir]
#
# MAESTRO_SHARD chooses which flows (CI runs the three side by side, each on its own emulator):
#   all     (default) everything below one after the other
#   base    the flows in app/maestro (setup, GhulPass, GhulAuth, GhulCal, app PIN, options,
#           GhulDrive and GhulMail with a server that does not exist)
#   server  setup and app PIN, then with a real server: GhulDrive (server/1-drive.yaml), servers,
#           synchronisation, sharing, Drive card, selection (3-sync-share.yaml), GhulDrive in
#           depth (4-drive-deep.yaml)
#   mail    setup, app PIN and GhulMail without account, then a real mail account (server/2-mail.yaml)
#           and GhulMail in depth (5-mail-deep.yaml)
# "server" and "mail" need the test servers next to the emulator (GHUL_TEST_SERVERS=1, see
# tools/test-servers.sh); without them only "base" runs.
set -euo pipefail
apk="$1"
out="${2:-maestro-out}"
shard="${MAESTRO_SHARD:-all}"
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
  timeout "$limit" maestro test "$@" "$file" --format junit --output "$out/report-$name.xml" --test-output-dir "$out"
  echo "::endgroup::"
}

# Which address does the device reach the test servers at? The SSH server greets with "SSH-": the
# device's own 127.0.0.1 (adb reverse) or the emulator's alias of the host. The servers only
# listen on this computer's 127.0.0.1, never on its other interfaces.
server_address() {
  echo "Testserver auf diesem Rechner:" >&2
  for p in 2223 10993 10587; do
    if (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "  Port $p lauscht" >&2; else echo "::error::Testserver-Port $p lauscht nicht" >&2; exit 1; fi
    adb reverse "tcp:$p" "tcp:$p" > /dev/null 2>&1 || true
  done
  adb reverse --list >&2 || true
  local server="" cand greeting
  for cand in 127.0.0.1 10.0.2.2; do
    greeting=$(timeout 15 adb shell "echo | nc -w 3 $cand 2223 2>&1 | head -c 40" 2>&1 | tr -d '\r' || true)
    echo "  Gerät -> $cand:2223: ${greeting:-keine Antwort}" >&2
    if [ -z "$server" ] && echo "$greeting" | grep -q "SSH-"; then server="$cand"; fi
  done
  [ -n "$server" ] || { echo "::error::Das Gerät erreicht die Testserver auf keinem Weg." >&2; exit 1; }
  echo "Abläufe mit echtem Server über $server" >&2
  echo "$server"
}

servers="${GHUL_TEST_SERVERS:-0}"
case "$shard" in
  base)
    timeout 2400 maestro test "$flows" --format junit --output "$out/report.xml" --test-output-dir "$out"
    ;;
  server)
    [ "$servers" = 1 ] || { echo "::error::Teil „server“ braucht die Testserver"; exit 1; }
    run "$flows/01-setup.yaml" 300
    run "$flows/04-app-pin-lock-unlock.yaml" 300
    addr=$(server_address)
    run "$flows/server/1-drive.yaml" 900 -e GHUL_SERVER="$addr"
    run "$flows/server/3-sync-share.yaml" 1200 -e GHUL_SERVER="$addr"
    run "$flows/server/4-drive-deep.yaml" 900 -e GHUL_SERVER="$addr"
    ;;
  mail)
    [ "$servers" = 1 ] || { echo "::error::Teil „mail“ braucht die Testserver"; exit 1; }
    run "$flows/01-setup.yaml" 300
    run "$flows/04-app-pin-lock-unlock.yaml" 300
    run "$flows/07-areas-without-server.yaml" 600
    addr=$(server_address)
    run "$flows/server/2-mail.yaml" 900 -e GHUL_SERVER="$addr"
    run "$flows/server/5-mail-deep.yaml" 900 -e GHUL_SERVER="$addr"
    ;;
  all)
    timeout 2400 maestro test "$flows" --format junit --output "$out/report.xml" --test-output-dir "$out"
    # With test servers: GhulDrive, GhulMail, servers and sharing against a real server. They
    # continue where the flows above ended.
    if [ "$servers" = 1 ]; then
      addr=$(server_address)
      run "$flows/server/1-drive.yaml" 900 -e GHUL_SERVER="$addr"
      run "$flows/server/2-mail.yaml" 900 -e GHUL_SERVER="$addr"
      run "$flows/server/3-sync-share.yaml" 1200 -e GHUL_SERVER="$addr"
      run "$flows/server/4-drive-deep.yaml" 900 -e GHUL_SERVER="$addr"
      run "$flows/server/5-mail-deep.yaml" 900 -e GHUL_SERVER="$addr"
    fi
    ;;
  *)
    echo "::error::Unbekannter Teil: $shard"; exit 1
    ;;
esac
