// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

/** A calendar day (proleptic Gregorian), as used by GhulCal; text form `YYYY-MM-DD`. */
data class Day(val y: Int, val m: Int, val d: Int) : Comparable<Day> {
    /** Days since 1970-01-01. */
    val days: Long
        get() {
            val yy = if (m <= 2) y - 1L else y.toLong()
            val era = (if (yy >= 0) yy else yy - 399) / 400
            val yoe = yy - era * 400
            val mp = if (m > 2) m - 3 else m + 9
            val doy = (153 * mp + 2) / 5 + d - 1
            val doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
            return era * 146097 + doe - 719468
        }

    fun plus(n: Int): Day = ofDays(days + n)

    fun plusMonths(n: Int): Day {
        val total = y * 12 + (m - 1) + n
        val ny = total.floorDiv(12)
        val nm = total.mod(12) + 1
        return Day(ny, nm, minOf(d, monthLength(ny, nm)))
    }

    /** 0 = Monday … 6 = Sunday. */
    val weekday: Int get() = (days + 3).mod(7L).toInt()

    val isoWeek: Int
        get() {
            val thursday = plus(3 - weekday)
            return ((thursday.days - Day(thursday.y, 1, 1).days) / 7 + 1).toInt()
        }

    val firstOfMonth: Day get() = Day(y, m, 1)

    override fun compareTo(other: Day): Int = days.compareTo(other.days)

    override fun toString(): String = "$y-${pad(m)}-${pad(d)}"

    companion object {
        fun ofDays(z0: Long): Day {
            val z = z0 + 719468
            val era = (if (z >= 0) z else z - 146096) / 146097
            val doe = z - era * 146097
            val yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
            val doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
            val mp = (5 * doy + 2) / 153
            val d = (doy - (153 * mp + 2) / 5 + 1).toInt()
            val m = (if (mp < 10) mp + 3 else mp - 9).toInt()
            val y = (yoe + era * 400 + if (m <= 2) 1 else 0).toInt()
            return Day(y, m, d)
        }

        fun parse(s: String): Day? {
            if (s.length < 10 || s[4] != '-' || s[7] != '-') return null
            val y = s.substring(0, 4).toIntOrNull() ?: return null
            val m = s.substring(5, 7).toIntOrNull() ?: return null
            val d = s.substring(8, 10).toIntOrNull() ?: return null
            return if (m in 1..12 && d in 1..monthLength(y, m)) Day(y, m, d) else null
        }

        fun monthLength(y: Int, m: Int): Int = when (m) {
            2 -> if ((y % 4 == 0 && y % 100 != 0) || y % 400 == 0) 29 else 28
            4, 6, 9, 11 -> 30
            else -> 31
        }
    }
}

fun pad(n: Int): String = n.toString().padStart(2, '0')

/** Minutes since midnight of `YYYY-MM-DDTHH:MM` (0 for a plain date). */
fun minutesOf(s: String): Int {
    if (s.length < 16) return 0
    return (s.substring(11, 13).toIntOrNull() ?: 0) * 60 + (s.substring(14, 16).toIntOrNull() ?: 0)
}

/** `HH:MM` of a date-time string. */
fun hm(s: String): String = if (s.length >= 16) s.substring(11, 16) else ""

fun hm(minutes: Int): String = "${pad(minutes / 60)}:${pad(minutes % 60)}"

fun dateTime(day: Day, minutes: Int): String = "${day}T${hm(minutes)}"

/** Minutes since 1970 (floating) of a date or date-time string. */
fun absMinutes(s: String): Long = (Day.parse(s)?.days ?: 0L) * 1440 + minutesOf(s)

fun fromAbsMinutes(m: Long): String = dateTime(Day.ofDays(m.floorDiv(1440L)), m.mod(1440L).toInt())

fun today(): Day = Day.parse(localNow())!!
