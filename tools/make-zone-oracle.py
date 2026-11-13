#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Writes the expected UTC offsets for the time zone test of the core.

For every zone file in core/ghul-core/tests/data/zoneinfo the offsets of the years 2000 to 2023
are taken from the time zone database of this computer (Python `zoneinfo`) and written to
core/ghul-core/tests/data/zone_offsets.tsv:

    Europe/Berlin<TAB>60<TAB>15900540:120 16213020:60 ...

The first number is the offset in minutes at 2000-01-01T00:00Z, then every change as
`minutes since 1970 (UTC):new offset`. The Rust test `world_time_zones_by_the_rules_of_a_file`
reads the same zones from the rules inside the .ics files and must get the same result.

Usage: python3 -I tools/make-zone-oracle.py
"""
import datetime as dt
import pathlib
import zoneinfo

ROOT = pathlib.Path(__file__).resolve().parent.parent / "core" / "ghul-core" / "tests" / "data"
UTC = dt.timezone.utc
START = dt.datetime(2000, 1, 1, tzinfo=UTC)
END = dt.datetime(2024, 1, 1, tzinfo=UTC)
EPOCH = dt.datetime(1970, 1, 1, tzinfo=UTC)


def offset(zone, at):
    return int(at.astimezone(zone).utcoffset().total_seconds() // 60)


def main():
    lines = []
    for path in sorted((ROOT / "zoneinfo").rglob("*.ics")):
        name = path.relative_to(ROOT / "zoneinfo").with_suffix("").as_posix()
        try:
            zone = zoneinfo.ZoneInfo(name)
        except Exception:
            continue
        at, current = START, offset(zone, START)
        first, changes = current, []
        while at < END:
            nxt = at + dt.timedelta(days=1)
            if offset(zone, nxt) != current:
                # Changes happen on full quarter hours at most: find the quarter hour.
                step = at
                while offset(zone, step + dt.timedelta(minutes=15)) == current:
                    step += dt.timedelta(minutes=15)
                step += dt.timedelta(minutes=15)
                current = offset(zone, step)
                changes.append(f"{int((step - EPOCH).total_seconds() // 60)}:{current}")
                # A second change within the same day is found on the next round.
                at = step
                continue
            at = nxt
        lines.append(f"{name}\t{first}\t{' '.join(changes)}")
    out = ROOT / "zone_offsets.tsv"
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(lines)} zones -> {out}")


if __name__ == "__main__":
    main()
