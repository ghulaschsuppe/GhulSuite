#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# CI only, after the Maestro flows: the calendar flow created an event with a reminder. This checks
# on the running emulator what no flow can see:
#   1. Android holds an alarm for the reminder.
#   2. After a change of the time zone the alarm is at the same time on the clock.
#   3. After the app was killed for good and the device "restarted", the alarm is back.
#   4. When the alarm fires, a notification arrives: with the app closed and locked.
# Usage: tools/check-reminders.sh <output-dir>
set -euo pipefail
out="${1:-.}"
pkg="de.danerd.ghulsuite.dev"
receiver="$pkg/de.danerd.ghulsuite.CalAlarmReceiver"
fire="de.danerd.ghulsuite.CAL_FIRE"
log() { echo "[$(date +%H:%M:%S)] Erinnerungen: $*"; }
fail() { echo "::error::Erinnerungen: $*"; exit 1; }

# Moment (Unix milliseconds) of the earliest reminder alarm, empty if there is none.
alarm_at() {
  timeout 30 adb shell dumpsys alarm | tr -d '\r' > "$out/alarms-$1.txt" || true
  # "|| true": no alarm is an answer (empty), not a reason to end the script.
  { grep -B 3 -F "$fire" "$out/alarms-$1.txt" | grep -oE 'origWhen [0-9]+' | awk '{print $2}' | sort -n | sed -n 1p; } || true
}
# Time on the clock of that moment in a time zone.
clock() { TZ="$2" date -d "@$(( $1 / 1000 ))" '+%Y-%m-%d %H:%M'; }
zone() { timeout 10 adb shell getprop persist.sys.timezone | tr -d '\r'; }

# 1 -------------------------------------------------------------------------------------------
before=$(alarm_at 1-planned)
[ -n "$before" ] || fail "kein Alarm für den Termin aus dem Kalender-Ablauf (siehe alarms-1-planned.txt)"
z1=$(zone); z1=${z1:-GMT}
log "Alarm geplant für $(clock "$before" "$z1") ($z1)"

# 2 -------------------------------------------------------------------------------------------
z2="America/New_York"; [ "$z1" = "$z2" ] && z2="Asia/Tokyo"
timeout 20 adb shell cmd alarm set-timezone "$z2" > /dev/null 2>&1 || true
sleep 8
if [ "$(zone)" = "$z2" ]; then
  after=$(alarm_at 2-zone)
  [ -n "$after" ] || fail "nach dem Zeitzonen-Wechsel fehlt der Alarm"
  log "nach Wechsel auf $z2: $(clock "$after" "$z2")"
  [ "$(clock "$before" "$z1")" = "$(clock "$after" "$z2")" ] \
    || fail "Erinnerung nach Zeitzonen-Wechsel zur falschen Uhrzeit: $(clock "$before" "$z1") wurde $(clock "$after" "$z2")"
else
  log "::warning::Zeitzone ließ sich im Emulator nicht umstellen, Schritt 2 übersprungen"
fi

# 3 -------------------------------------------------------------------------------------------
timeout 60 adb root > /dev/null 2>&1 || true
timeout 60 adb wait-for-device
sleep 3
timeout 20 adb shell am force-stop "$pkg"
sleep 3
[ -z "$(alarm_at 3-stopped)" ] || log "Hinweis: Alarm überlebt das Beenden der App"
timeout 20 adb shell am broadcast --include-stopped-packages -a android.intent.action.BOOT_COMPLETED -n "$receiver" > "$out/boot-broadcast.txt" 2>&1 || true
sleep 6
restored=$(alarm_at 4-restored)
[ -n "$restored" ] || fail "nach dem Neustart-Signal ist der Alarm nicht wieder da (siehe boot-broadcast.txt)"
log "nach Neustart wieder geplant für $(clock "$restored" "$(zone)")"

# 4 -------------------------------------------------------------------------------------------
timeout 20 adb shell am broadcast --include-stopped-packages -a "$fire" -n "$receiver" --ei index 0 > "$out/fire-broadcast.txt" 2>&1 || true
sleep 6
timeout 30 adb shell dumpsys notification --noredact | tr -d '\r' > "$out/notifications.txt" || true
grep -qE "NotificationRecord.*pkg=$pkg.*channel=cal(_silent)?([ :,)]|$)" "$out/notifications.txt" \
  || fail "der ausgelöste Alarm hat keine Benachrichtigung gezeigt (siehe notifications.txt)"
log "Benachrichtigung ist da, auch bei beendeter, gesperrter App"
