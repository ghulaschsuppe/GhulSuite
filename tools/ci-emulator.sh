#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# CI only: installs an Android 14 emulator, starts it headless, waits for the boot and runs
# the Maestro flows (tools/run-maestro.sh). Usage: tools/ci-emulator.sh <apk> <output-dir>
# Every step has a time limit, so a hanging emulator ends the job with a log instead of a timeout.
set -euo pipefail
apk="$1"; out="$2"
mkdir -p "$out"
sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
sdkm="$sdk/cmdline-tools/latest/bin/sdkmanager"
avdm="$sdk/cmdline-tools/latest/bin/avdmanager"
image="system-images;android-34;google_apis;x86_64"
# One place for the virtual device, used by avdmanager and the emulator alike.
export ANDROID_USER_HOME="$HOME/.android"
export ANDROID_AVD_HOME="$HOME/.android/avd"
mkdir -p "$ANDROID_AVD_HOME"
log() { echo "[$(date +%H:%M:%S)] $*"; }
finish() {
  status=$?
  log "Ende (Status $status), letzte Zeilen des Emulators:"
  tail -n 40 "$out/emulator.log" 2>/dev/null || true
  timeout 20 adb exec-out screencap -p > "$out/last-screen.png" 2>/dev/null || true
  timeout 30 adb logcat -d > "$out/logcat.txt" 2>/dev/null || true
  timeout 10 adb emu kill > /dev/null 2>&1 || true
  [ -z "${logcat_pid:-}" ] || kill "$logcat_pid" 2> /dev/null || true
}
trap finish EXIT

if [ -e /dev/kvm ]; then log "KVM vorhanden"; ls -la /dev/kvm; else log "::warning::/dev/kvm fehlt"; fi
log "SDK-Pakete installieren"
(yes 2>/dev/null || true) | timeout 300 "$sdkm" --licenses > /dev/null 2>&1 || true
# Downloads from Google's mirror occasionally arrive broken ("unknown archive"): up to three tries.
for try in 1 2 3; do
  if timeout 900 "$sdkm" --install "platform-tools" "emulator" "$image" > "$out/sdk-install.log" 2>&1 \
    && [ -d "$sdk/system-images/android-34/google_apis/x86_64" ]; then
    break
  fi
  log "SDK-Download fehlgeschlagen (Versuch $try)"; tail -n 5 "$out/sdk-install.log" || true
  rm -rf "$sdk/system-images/android-34/google_apis/x86_64" "$HOME/.android/cache" 2>/dev/null || true
  [ "$try" = 3 ] && { log "::error::SDK-Pakete nicht installierbar"; exit 1; }
  sleep 15
done
export PATH="$sdk/platform-tools:$sdk/emulator:$PATH"
log "AVD anlegen"
echo no | timeout 120 "$avdm" create avd --force -n ghul -k "$image" -d pixel_6 -p "$ANDROID_AVD_HOME/ghul.avd" > /dev/null
ls "$ANDROID_AVD_HOME"
log "Emulator starten"
nohup emulator -avd ghul -no-window -gpu swiftshader_indirect -noaudio -no-boot-anim -no-snapshot \
  -no-metrics -camera-back none -camera-front none -accel auto > "$out/emulator.log" 2>&1 &
timeout 300 adb wait-for-device
log "Gerät da, warte auf Systemstart"
booted=""
for i in $(seq 1 90); do
  if [ "$(timeout 10 adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
  sleep 5
done
[ -n "$booted" ] || { log "::error::Emulator nicht hochgefahren"; exit 1; }
log "System bereit"
# The whole system log of the run (the device keeps only the last few hundred kilobytes), for
# tools/check-logcat.sh: crashes, hangs and alarms of the development build fail the run.
adb logcat -c || true
nohup adb logcat -v threadtime > "$out/logcat-full.txt" 2> /dev/null &
logcat_pid=$!
for k in window_animation_scale transition_animation_scale animator_duration_scale; do
  timeout 10 adb shell settings put global "$k" 0 || true
done
# The development build may be photographed on this device (otherwise every screenshot of the
# flows is black: the app forbids screenshots by default; TestDevice.kt).
timeout 10 adb shell setprop debug.ghulsuite.screenshots 1 || true
# A freshly booted emulator is busy for a while; system "isn't responding" dialogs would cover the app.
timeout 10 adb shell settings put global hide_error_dialogs 1 || true
sleep 30
timeout 10 adb shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS > /dev/null || true
timeout 10 adb shell input keyevent 82 || true
if [ "${SKIP_MAESTRO:-0}" != 1 ]; then
  log "Maestro-Abläufe starten"
  maestro_rc=0
  timeout 3600 "$(dirname "$0")/run-maestro.sh" "$apk" "$out" || maestro_rc=$?
  log "Maestro fertig (Status $maestro_rc)"
  # Also after a failed flow: a crash in the log is most likely why it failed, and says so.
  # (The log keeps recording for the monkey below and is checked again after it.)
  logcat_rc=0
  "$(dirname "$0")/check-logcat.sh" "$out/logcat-full.txt" "$out" || logcat_rc=$?
  # Every step leaves a picture to be looked at. A picture of a black screen (the app forbids
  # screenshots, see TestDevice.kt) is a few kilobytes as PNG, a real screen many times that.
  python3 -c "import PIL" 2> /dev/null || pip install --quiet pillow > /dev/null 2>&1 || true
  if ! python3 "$(dirname "$0")/check-black-screenshots.py" "$out"; then
    logcat_rc=1
  fi
  [ "$maestro_rc" = 0 ] || exit "$maestro_rc"
  [ "$logcat_rc" = 0 ] || exit "$logcat_rc"
  shard="${MAESTRO_SHARD:-all}"
  # What no flow can see: the reminder of the calendar flow as an alarm of the system.
  if [ "$shard" = all ] || [ "$shard" = base ]; then
    timeout 300 "$(dirname "$0")/check-reminders.sh" "$out"
  fi
  # And what the servers of the flows with a real server hold now: only ciphertext.
  if [ "${GHUL_TEST_SERVERS:-0}" = 1 ] && [ -f servers.env ]; then
    case "$shard" in
      all) parts="drive mail" ;;
      server) parts="drive" ;;
      mail) parts="mail" ;;
      *) parts="" ;;
    esac
    if [ -n "$parts" ]; then
      CHECK_PARTS="$parts" timeout 120 "$(dirname "$0")/check-server-flows.sh" servers.env "$out"
    fi
  fi
else
  timeout 120 adb install -r "$apk" > /dev/null
fi
# Extra checks on the emulator with the app installed (nightly: tools/intent-attacks.sh). The
# script in EXTRA_CHECKS is called as `<script> <apk> <output-dir>`; its failure fails the job,
# but only after the monkey below has run as well.
extra_failed=""
if [ -n "${EXTRA_CHECKS:-}" ]; then
  log "Zusatzprüfung: $EXTRA_CHECKS"
  timeout "${EXTRA_CHECKS_TIMEOUT:-2400}" "$EXTRA_CHECKS" "$apk" "$out" || extra_failed=1
  timeout 20 adb shell am force-stop de.danerd.ghulsuite.dev || true
fi
# App fuzzing (nightly): tens of thousands of random taps, swipes and keys inside the app.
# Every crash or "app isn't responding" fails the job; the log names the seed to repeat it.
if [ "${MONKEY_EVENTS:-0}" -gt 0 ]; then
  pkg="de.danerd.ghulsuite.dev"
  failed=""
  for seed in ${MONKEY_SEEDS:-11 4242 90210}; do
    log "Monkey: $MONKEY_EVENTS Ereignisse, Seed $seed"
    timeout 2400 adb shell monkey -p "$pkg" -s "$seed" --throttle 20 --pct-syskeys 0 --pct-appswitch 2 \
      --ignore-security-exceptions -v "$MONKEY_EVENTS" > "$out/monkey-$seed.txt" 2>&1 || true
    if grep -qE "// CRASH|// NOT RESPONDING|ANR in $pkg" "$out/monkey-$seed.txt"; then
      log "::error::Monkey: Absturz oder Hänger mit Seed $seed (siehe monkey-$seed.txt)"
      grep -A 25 -E "// CRASH|// NOT RESPONDING" "$out/monkey-$seed.txt" | head -60 || true
      # The threads of the hanging app (main thread first): the system keeps them in its dropbox.
      timeout 60 adb shell dumpsys dropbox --print data_app_anr > "$out/anr-traces-$seed.txt" 2>&1 || true
      failed=1
    fi
    timeout 20 adb shell am force-stop "$pkg" || true
  done
  # Alarms of the development build during the monkey count as well.
  "$(dirname "$0")/check-logcat.sh" "$out/logcat-full.txt" "$out" || failed=1
  [ -z "$failed" ] || exit 1
  log "Monkey ohne Absturz"
fi
if [ -n "$extra_failed" ]; then
  log "::error::Zusatzprüfung $EXTRA_CHECKS fehlgeschlagen"
  exit 1
fi
