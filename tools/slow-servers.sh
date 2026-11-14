#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# CI only: makes the test servers of tools/test-servers.sh answer slowly, like a server reached
# over a mobile network. Every packet the servers send (SFTP, FTPS, IMAP, SMTP on this computer's
# 127.0.0.1) waits the given milliseconds; all other traffic on the machine,
# adb and the emulator's own included, stays fast.
# Usage: sudo tools/slow-servers.sh <milliseconds>   (0 = remove the delay again)
set -euo pipefail
ms="${1:-150}"
dev=lo
tc qdisc del dev "$dev" root 2> /dev/null || true
[ "$ms" -gt 0 ] || { echo "Testserver: keine Verzögerung"; exit 0; }
# Three bands; all ordinary traffic goes to the second one, only the servers' packets to the third
# one with the delay. No jitter: it would reorder packets, and TCP would slow down far more than
# a real network does.
tc qdisc add dev "$dev" root handle 1: prio bands 3 priomap 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1
tc qdisc add dev "$dev" parent 1:3 handle 30: netem delay "${ms}ms"
for port in 2222 2223 10021 10993 10587; do
  tc filter add dev "$dev" protocol ip parent 1:0 prio 1 u32 match ip sport "$port" 0xffff flowid 1:3
done
echo "Testserver: jede Antwort wartet ${ms} ms"
tc -s qdisc show dev "$dev" | head -n 4
