#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# CI only, after the Maestro flows with a real server (app/maestro/server): looks at what the
# test servers hold now. The promise of the app is that a server only ever sees ciphertext.
#   1. GhulDrive (SFTP): the app created its files, and the folder name typed in the app appears
#      nowhere on the server, neither in a file name nor in a file.
#   2. GhulMail: the mail the app sent to itself lies in the mailbox, encrypted (OpenPGP), and its
#      text and subject are not readable there.
# Usage: tools/check-server-flows.sh <servers.env written by tools/test-servers.sh> <output-dir>
set -euo pipefail
env_file="$1"
out="${2:-.}"
# shellcheck disable=SC1090
. "$env_file"
log() { echo "[$(date +%H:%M:%S)] Server-Prüfung: $*"; }
fail() { echo "::error::Server-Prüfung: $*"; exit 1; }
folder_name="Urlaub-Maestro"
mail_text="Geheimtext-Maestro-4711"
mail_subject="Hallo vom Emulator"

# 1 -------------------------------------------------------------------------------------------
drive="$GHUL_TEST_SFTP_PW_ROOT"
sudo find "$drive" -type f -printf '%P\t%s\n' > "$out/server-drive-files.txt" 2> /dev/null || true
files=$(wc -l < "$out/server-drive-files.txt")
[ "$files" -gt 0 ] || fail "GhulDrive hat nichts auf dem SFTP-Server abgelegt (siehe server-drive-files.txt)"
if sudo grep -r -l -a -F -e "$folder_name" "$drive" > "$out/server-drive-leak.txt" 2> /dev/null; then
  fail "der Ordnername „$folder_name“ steht lesbar auf dem Server: $(head -n 3 "$out/server-drive-leak.txt" | tr '\n' ' ')"
fi
if grep -q -F -e "$folder_name" "$out/server-drive-files.txt"; then
  fail "der Ordnername „$folder_name“ steht in einem Dateinamen auf dem Server"
fi
log "GhulDrive: $files Dateien auf dem Server, der Ordnername ist dort nicht lesbar"

# 2 -------------------------------------------------------------------------------------------
box="$GHUL_TEST_DIR/mail"
sudo grep -r -l -a -F -e "BEGIN PGP MESSAGE" "$box" > "$out/server-mail-encrypted.txt" 2> /dev/null || true
[ -s "$out/server-mail-encrypted.txt" ] || fail "im Postfach liegt keine verschlüsselte Mail der App"
for secret in "$mail_text" "$mail_subject"; do
  if sudo grep -r -l -a -F -e "$secret" "$box" > "$out/server-mail-leak.txt" 2> /dev/null; then
    fail "„$secret“ steht lesbar im Postfach auf dem Server: $(head -n 3 "$out/server-mail-leak.txt" | tr '\n' ' ')"
  fi
done
log "GhulMail: $(wc -l < "$out/server-mail-encrypted.txt") verschlüsselte Mail(s) im Postfach, Text und Betreff sind dort nicht lesbar"
