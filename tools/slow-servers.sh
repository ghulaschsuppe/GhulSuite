#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# CI only: makes the test servers of tools/test-servers.sh behave like servers reached over a
# mobile network. Every packet to and from the servers (SFTP, IMAP, SMTP on this computer's
# 127.0.0.1) waits the given milliseconds and is lost with the given chance; all other traffic on
# the machine, adb and the emulator's own included, stays fast and complete.
# Usage: sudo tools/slow-servers.sh <milliseconds> [loss in percent]
#   sudo tools/slow-servers.sh 150 2     slow and a little lossy (the flows)
#   sudo tools/slow-servers.sh 150 100   the servers are gone (dead network, dead server)
#   sudo tools/slow-servers.sh 0         everything as normal again
set -euo pipefail
ms="${1:-150}"
loss="${2:-0}"
dev=lo
tc qdisc del dev "$dev" root 2> /dev/null || true
if [ "$ms" -le 0 ] && [ "$loss" -le 0 ]; then echo "Testserver: normal"; exit 0; fi
# Three bands; all ordinary traffic goes to the second one, only the servers' packets to the third
# one with delay and loss. No jitter: it would reorder packets, and TCP would slow down far more
# than a real network does.
tc qdisc add dev "$dev" root handle 1: prio bands 3 priomap 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1
tc qdisc add dev "$dev" parent 1:3 handle 30: netem delay "${ms}ms" loss "${loss}%"
for port in 2222 2223 10993 10587; do
  tc filter add dev "$dev" protocol ip parent 1:0 prio 1 u32 match ip sport "$port" 0xffff flowid 1:3
  # Also what goes to the servers: with 100 % loss not even a connection is made.
  tc filter add dev "$dev" protocol ip parent 1:0 prio 1 u32 match ip dport "$port" 0xffff flowid 1:3
done
echo "Testserver: jedes Paket hin und zurück wartet ${ms} ms, ${loss} % gehen verloren"
