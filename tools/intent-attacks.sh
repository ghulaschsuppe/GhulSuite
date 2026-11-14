#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Intent attacks on the installed app in a running emulator (what drozer would do, with adb only).
#
# Reads the components of the APK's manifest (aapt2) and, as an ordinary caller (adb shell, not
# root), sends every exported component what another app could send: its own actions without
# data, the app's internal actions without or with a wrong token (IntentToken.kt), extras of the
# wrong type, huge strings, file:// and content:// URIs into the app's private data. Components
# that are not exported must refuse the caller.
#
# Fails on: a crash of the app (FATAL EXCEPTION or ANR for the package), a non-exported activity,
# service or provider that answers the caller. Only warns when the screen after an internal
# action without the token differs from the normal start screen (see the screenshots).
#
# Usage: tools/intent-attacks.sh <apk> <output-dir>   (emulator running, APK installed)
set -euo pipefail
apk="$1"; out="$2/intent-attacks"
mkdir -p "$out"
sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
log() { echo "[$(date +%H:%M:%S)] Intents: $*"; }
report="$out/report.txt"
: > "$report"
errors=0
warnings=0
error() { echo "::error::Intents: $*"; echo "FEHLER  $*" >> "$report"; errors=$((errors + 1)); }
warn() { echo "::warning::Intents: $*"; echo "WARNUNG $*" >> "$report"; warnings=$((warnings + 1)); }
# Quotes one argument for the device's shell.
q() { printf "'%s'" "${1//\'/\'\\\'\'}"; }
# Runs a command on the device as the shell user; never fails (the answer is checked instead).
dev() { local cmd=""; for a in "$@"; do cmd+="$(q "$a") "; done; { timeout 60 adb shell "$cmd" < /dev/null 2>&1 | tr -d '\r'; } || true; }

# --- the manifest ----------------------------------------------------------------------------
aapt2=$(ls -d "$sdk"/build-tools/*/aapt2 2>/dev/null | sort -V | tail -n 1 || true)
if [ -z "$aapt2" ]; then
  log "aapt2 fehlt, build-tools installieren"
  timeout 600 "$sdk/cmdline-tools/latest/bin/sdkmanager" --install "build-tools;35.0.0" > /dev/null
  aapt2="$sdk/build-tools/35.0.0/aapt2"
fi
pkg=$("$aapt2" dump packagename "$apk")
"$aapt2" dump xmltree --file AndroidManifest.xml "$apk" > "$out/manifest-tree.txt"
# One line per component: kind|name|exported|permission|actions|schemes|mime types
python3 -I - "$out/manifest-tree.txt" > "$out/components.txt" <<'PY'
import re, sys
stack, comps = [], []
for line in open(sys.argv[1], encoding="utf-8"):
    indent = len(line) - len(line.lstrip())
    s = line.strip()
    m = re.match(r"E: (\S+)", s)
    if m:
        while stack and stack[-1][0] >= indent:
            stack.pop()
        node = {"tag": m.group(1), "attrs": {}, "parent": stack[-1][1] if stack else None}
        stack.append((indent, node))
        if node["tag"] in ("activity", "activity-alias", "service", "receiver", "provider"):
            node.update(actions=[], schemes=[], types=[])
            comps.append(node)
        continue
    m = re.match(r'A: (?:\S+:)?([\w]+)(?:\(0x[0-9a-f]+\))?=(?:\(type 0x[0-9a-f]+\))?"?([^"]*)"?', s)
    if m and stack:
        node = stack[-1][1]
        node["attrs"][m.group(1)] = m.group(2).split(" (Raw")[0]
        comp = node
        while comp is not None and "actions" not in comp:
            comp = comp["parent"]
        if comp is None or comp is node:
            continue
        key, val = m.group(1), node["attrs"][m.group(1)]
        if node["tag"] == "action" and key == "name":
            comp["actions"].append(val)
        elif node["tag"] == "data" and key == "scheme":
            comp["schemes"].append(val)
        elif node["tag"] == "data" and key == "mimeType":
            comp["types"].append(val)
for c in comps:
    a = c["attrs"]
    exported = a.get("exported")
    if exported is None:
        exported = "true" if c["actions"] else "false"
    exported = "true" if exported in ("true", "0xffffffff", "-1") else "false"
    name = a.get("name", "?")
    print("|".join([c["tag"], name, exported, a.get("permission", ""), ",".join(dict.fromkeys(c["actions"])),
                    ",".join(dict.fromkeys(c["schemes"])), ",".join(dict.fromkeys(c["types"])),
                    a.get("authorities", "")]))
PY
log "Paket $pkg, $(wc -l < "$out/components.txt") Komponenten (components.txt)"
[ -s "$out/components.txt" ] || { error "keine Komponenten im Manifest gefunden"; exit 1; }

# --- as an ordinary caller -------------------------------------------------------------------
if [ "$(dev id -u)" = 0 ]; then
  timeout 60 adb unroot > /dev/null 2>&1 || true
  timeout 120 adb wait-for-device
  sleep 3
fi
caller=$(dev id -u)
log "Aufrufer uid $caller"
[ "$caller" != 0 ] || warn "adb läuft als root: Prüfung nicht exportierter Komponenten nicht aussagekräftig"

timeout 20 adb logcat -b all -c || true
crashes() { timeout 30 adb logcat -b crash -d 2>/dev/null | grep -c "Process: $pkg" || true; }
anrs() { timeout 30 adb logcat -b main,system -d 2>/dev/null | grep -c "ANR in $pkg" || true; }
seen_crashes=0
seen_anrs=0
n=0
# Runs one attack and checks the app survived it. $1: label, rest: the am/content command.
attack() {
  local label="$1"; shift
  n=$((n + 1))
  local res
  res=$(dev "$@" | head -c 2000) || true
  printf '%03d %s\n    %s\n    -> %s\n' "$n" "$label" "$(echo "$*" | head -c 300)" "$(echo "$res" | tr '\n' ' ' | head -c 400)" >> "$report"
  sleep "${ATTACK_PAUSE:-1}"
  local c a
  c=$(crashes); a=$(anrs)
  if [ "$c" -gt "$seen_crashes" ]; then
    error "Absturz nach $label (Nr. $n), siehe crash-$n.txt"
    timeout 30 adb logcat -b crash -d > "$out/crash-$n.txt" 2>&1 || true
    seen_crashes=$c
  fi
  if [ "$a" -gt "$seen_anrs" ]; then
    error "App reagiert nicht (ANR) nach $label (Nr. $n)"
    seen_anrs=$a
  fi
  last="$res"
}
denied() { echo "$1" | grep -qiE "Permission Denial|SecurityException|not exported|Requires permission|Error|Exception|not allowed|Unknown"; }

big=$(head -c 20000 /dev/zero | tr '\0' 'A')
weird="%n%s%x'\"<script>alert(1)</script>../../\$(id);\`id\`|&"
priv="/data/data/$pkg"
files_auth="$pkg.files"
internal_actions="de.danerd.ghulsuite.OPEN_MAIL de.danerd.ghulsuite.OPEN_EVENT de.danerd.ghulsuite.LOCK de.danerd.ghulsuite.CAL_FIRE de.danerd.ghulsuite.CAL_SNOOZE de.danerd.ghulsuite.CAL_DONE"
uris=("file://$priv/shared_prefs/intent_token.xml" "file://$priv/files/" "file:///proc/self/environ" "file:///sdcard/../data/data/$pkg/databases"
  "content://$files_auth/drive/x" "content://$files_auth/../../shared_prefs/intent_token.xml" "content://settings/secure"
  "content://media/external/file/999999999" "mailto:$big@example.org" "mailto:?subject=$weird&body=$weird&cc=a@b&bcc=c@d&attachment=file://$priv/shared_prefs/intent_token.xml"
  "content://com.android.contacts/contacts" "javascript:alert(1)" "intent:#Intent;component=$pkg/.MainActivity;end")

# --- screen of a normal start (for the comparison after internal actions) ---------------------
main="$pkg/de.danerd.ghulsuite.MainActivity"
screen() {
  timeout 30 adb shell uiautomator dump "/sdcard/ui-$1.xml" < /dev/null > /dev/null 2>&1 || true
  timeout 30 adb exec-out cat "/sdcard/ui-$1.xml" < /dev/null > "$out/ui-$1.xml" 2> /dev/null || true
  timeout 20 adb exec-out screencap -p < /dev/null > "$out/screen-$1.png" 2> /dev/null || true
  # Only the app's own texts, sorted (the status bar's clock belongs to another package).
  python3 -I - "$out/ui-$1.xml" "$pkg" <<'PY' || true
import re, sys
try:
    xml = open(sys.argv[1], encoding="utf-8", errors="replace").read()
except OSError:
    sys.exit(0)
texts = set()
for node in re.findall(r"<node [^>]*>", xml):
    if f'package="{sys.argv[2]}"' not in node:
        continue
    for attr in ("text", "content-desc"):
        m = re.search(attr + r'="([^"]*)"', node)
        if m and m.group(1):
            texts.add(m.group(1))
print("\n".join(sorted(texts)))
PY
}
dev am force-stop "$pkg" > /dev/null
dev am start -W -n "$main" > /dev/null
sleep 5
screen start > "$out/texts-start.txt"
log "Startbildschirm: $(wc -l < "$out/texts-start.txt") Texte"
[ -s "$out/texts-start.txt" ] || warn "Startbildschirm nicht lesbar (uiautomator), Bildschirmvergleich ohne Aussage"

# --- 1. internal actions of MainActivity without the token: nothing may open -------------------
for act in de.danerd.ghulsuite.OPEN_MAIL de.danerd.ghulsuite.OPEN_EVENT; do
  for variant in none wrong empty big; do
    args=(am start -W -n "$main" -a "$act" --es de.danerd.ghulsuite.MAIL_ACCOUNT acc1 --ei de.danerd.ghulsuite.MAIL_UID 1 --es key "event-1")
    case $variant in
      wrong) args+=(--es de.danerd.ghulsuite.TOKEN 00000000000000000000000000000000) ;;
      empty) args+=(--es de.danerd.ghulsuite.TOKEN "") ;;
      big) args+=(--es de.danerd.ghulsuite.TOKEN "$big") ;;
    esac
    dev am force-stop "$pkg" > /dev/null
    attack "$act ohne gültiges Token ($variant)" "${args[@]}"
    sleep 3
    tag="${act##*.}-$variant"
    screen "$tag" > "$out/texts-$tag.txt"
    if [ -z "$(dev pidof "$pkg")" ]; then
      error "App läuft nach $act ($variant) nicht mehr"
    elif [ -s "$out/texts-start.txt" ] && ! diff -q "$out/texts-start.txt" "$out/texts-$tag.txt" > /dev/null; then
      warn "Bildschirm nach $act ($variant) anders als beim normalen Start, siehe screen-$tag.png"
    fi
  done
done
# The same actions on a running app (onNewIntent), with extras of the wrong type.
dev am start -W -n "$main" > /dev/null
attack "OPEN_MAIL, Extras falscher Typ" am start -n "$main" -a de.danerd.ghulsuite.OPEN_MAIL --ei de.danerd.ghulsuite.MAIL_ACCOUNT 7 --es de.danerd.ghulsuite.MAIL_UID x --ez de.danerd.ghulsuite.TOKEN true
attack "OPEN_MAIL, riesige Extras" am start -n "$main" -a de.danerd.ghulsuite.OPEN_MAIL --es de.danerd.ghulsuite.MAIL_ACCOUNT "$big" --ei de.danerd.ghulsuite.MAIL_UID -1
attack "OPEN_EVENT, Schlüssel mit Sonderzeichen" am start -n "$main" -a de.danerd.ghulsuite.OPEN_EVENT --es key "$weird" --el de.danerd.ghulsuite.TOKEN 1
screen running-new-intent > "$out/texts-running-new-intent.txt"
if [ -s "$out/texts-start.txt" ] && ! diff -q "$out/texts-start.txt" "$out/texts-running-new-intent.txt" > /dev/null; then
  warn "Bildschirm nach internen Aktionen an die laufende App anders als beim Start, siehe screen-running-new-intent.png"
fi

# --- 2. every exported component, generic attacks ----------------------------------------------
while IFS='|' read -r kind name exported perm actions schemes types auth <&3; do
  comp="$pkg/$name"
  [ "$exported" = true ] || continue
  log "exportiert: $kind $name ${perm:+(Berechtigung $perm)}"
  case $kind in
    activity|activity-alias)
      attack "$name ohne Aktion" am start -W -n "$comp"
      for act in ${actions//,/ } $internal_actions; do
        attack "$name $act leer" am start -n "$comp" -a "$act"
        # Extras of the wrong type and huge or odd values for everything the app reads.
        attack "$name $act Extras" am start -n "$comp" -a "$act" --es android.intent.extra.SUBJECT "$big" \
          --es android.intent.extra.TEXT "$weird" --esa android.intent.extra.EMAIL "a@b,$weird" --ei android.intent.extra.STREAM 5 \
          --es title "$big" --el beginTime -1 --el endTime 9223372036854775807 --ez allDay true --es eventLocation "$weird" --es description "$weird"
      done
      # URIs only with the component's own actions (internal actions do not read data).
      for act in ${actions//,/ }; do
        for u in "${uris[@]}"; do
          attack "$name $act $u" am start -n "$comp" -a "$act" -d "$u"
        done
        case $act in
          android.intent.action.SEND|android.intent.action.SEND_MULTIPLE)
            attack "$name $act Stream-Liste falsch" am start -n "$comp" -a "$act" -t "*/*" --esa android.intent.extra.STREAM "file://$priv/shared_prefs/intent_token.xml"
            for u in "${uris[@]}"; do
              attack "$name $act Stream $u" am start -n "$comp" -a "$act" -t "*/*" --eu android.intent.extra.STREAM "$u"
              # Confused deputy: the app must not attach its own private files for the caller.
              case $u in
                *"$priv"*|*"$files_auth"*)
                  sleep 2
                  tag="stream-$n"
                  screen "$tag" > "$out/texts-$tag.txt"
                  if grep -qE "intent_token|databases" "$out/texts-$tag.txt"; then
                    error "private Datei als Anhang übernommen ($act, $u), siehe screen-$tag.png"
                  else
                    rm -f "$out/ui-$tag.xml" "$out/screen-$tag.png" "$out/texts-$tag.txt"
                  fi ;;
              esac
            done ;;
        esac
        for t in ${types//,/ }; do
          for u in "file://$priv/shared_prefs/intent_token.xml" "content://$files_auth/x.ics"; do
            attack "$name $act $t $u" am start -n "$comp" -a "$act" -d "$u" -t "$t"
          done
        done
      done
      ;;
    service)
      for act in "" ${actions//,/ } $internal_actions; do
        attack "$name Dienst ${act:-ohne Aktion}" am startservice -n "$comp" ${act:+-a "$act"} --es x "$big"
        if [ -n "$perm" ] && [ "$caller" != 0 ] && ! denied "$last"; then
          error "Dienst $name (Berechtigung $perm) ließ sich ohne die Berechtigung starten: $last"
        fi
      done
      ;;
    receiver)
      for act in "" ${actions//,/ } $internal_actions; do
        attack "$name Empfänger ${act:-ohne Aktion}" am broadcast -n "$comp" ${act:+-a "$act"} --ei index 999999 --ei notify_id -1 --es x "$big"
      done
      ;;
    provider)
      for a in ${auth//;/ }; do
        attack "$name Provider $a" content query --uri "content://$a/"
        attack "$name Provider $a ../" content query --uri "content://$a/../../shared_prefs/intent_token.xml"
        attack "$name Provider $a lesen" content read --uri "content://$a/x"
      done
      ;;
  esac
done 3< "$out/components.txt"

# --- 3. components that are not exported must refuse the caller --------------------------------
if [ "$caller" != 0 ]; then
  while IFS='|' read -r kind name exported perm actions schemes types auth <&3; do
    [ "$exported" = false ] || continue
    comp="$pkg/$name"
    case $kind in
      activity|activity-alias)
        attack "nicht exportiert: $name" am start -W -n "$comp" --es de.danerd.ghulsuite.autofill.DOMAIN example.org
        denied "$last" || error "nicht exportierte Activity $name ließ sich starten: $last" ;;
      service)
        attack "nicht exportiert: $name" am startservice -n "$comp" -a de.danerd.ghulsuite.LOCK
        denied "$last" || error "nicht exportierter Dienst $name ließ sich starten: $last" ;;
      receiver)
        # The system drops such broadcasts silently; only the app's survival is checked here.
        attack "nicht exportiert: $name" am broadcast -n "$comp" -a de.danerd.ghulsuite.CAL_FIRE --ei index 0 ;;
      provider)
        for a in ${auth//;/ }; do
          attack "nicht exportiert: Provider $a" content query --uri "content://$a/x"
          denied "$last" || error "nicht exportierter Provider $a antwortet: $last"
        done ;;
    esac
  done 3< "$out/components.txt"
fi

# --- result ------------------------------------------------------------------------------------
dev am start -W -n "$main" > /dev/null
sleep 3
timeout 20 adb exec-out screencap -p > "$out/screen-end.png" 2> /dev/null || true
[ -n "$(dev pidof "$pkg")" ] || error "App läuft am Ende nicht"
timeout 30 adb logcat -b crash -d > "$out/logcat-crash.txt" 2>&1 || true
timeout 30 adb logcat -d > "$out/logcat.txt" 2>&1 || true
dev dumpsys activity activities > "$out/activities.txt" || true
log "$n Angriffe, $errors Fehler, $warnings Warnungen (report.txt)"
echo "$n Angriffe, $errors Fehler, $warnings Warnungen" >> "$report"
dev am force-stop "$pkg" > /dev/null || true
[ "$errors" = 0 ]
