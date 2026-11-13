// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.TimeZones
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.zone.ZoneRules

/**
 * The time zone database of this device for the core, which carries none of its own. Zone names
 * come from files of strangers: anything unknown or odd answers `null`.
 */
object DeviceTimeZones : TimeZones {
    private fun rules(zone: String): ZoneRules? =
        runCatching { (if (zone.isEmpty()) ZoneId.systemDefault() else ZoneId.of(zone)).rules }.getOrNull()

    override fun offsetAtUtc(zone: String, utcMinutes: Long): Int? = runCatching {
        rules(zone)?.getOffset(Instant.ofEpochSecond(Math.multiplyExact(utcMinutes, 60L)))?.totalSeconds?.div(60)
    }.getOrNull()

    override fun offsetAtLocal(zone: String, localMinutes: Long): Int? = runCatching {
        val local = LocalDateTime.ofEpochSecond(Math.multiplyExact(localMinutes, 60L), 0, ZoneOffset.UTC)
        rules(zone)?.getOffset(local)?.totalSeconds?.div(60)
    }.getOrNull()
}
