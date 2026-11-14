// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

/** Repetition of an event (same choices as the design preview). */
enum class CalRepeatUi { NONE, DAILY, WEEKDAYS, WEEKLY, BIWEEKLY, MONTHLY, YEARLY }

enum class CalDeleteModeUi { ONE, FOLLOWING, ALL }

data class CalLinkUi(val kind: String, val label: String, val target: String)

data class CalCalendarUi(
    val id: String,
    val vaultId: String,
    val name: String,
    /** 0xRRGGBB. */
    val color: Int,
    /** Minutes, -1 = none. */
    val defaultReminder: Int,
    val events: Int,
)

/**
 * An event. Times are floating local wall-clock times: timed `YYYY-MM-DDTHH:MM` with exclusive
 * end; all-day `YYYY-MM-DD` with the last day as end. All-day reminders count back from 9:00.
 */
data class CalEventUi(
    val id: String = "",
    val calendarId: String = "",
    val title: String = "",
    val allDay: Boolean = false,
    val start: String = "",
    val end: String = "",
    val repeat: CalRepeatUi = CalRepeatUi.NONE,
    val repeatUntil: String = "",
    val exceptions: List<String> = emptyList(),
    val reminders: List<Int> = emptyList(),
    val location: String = "",
    val notes: String = "",
    val links: List<CalLinkUi> = emptyList(),
    val birthYear: Int = 0,
    /**
     * Time zone of a series that came from another zone (e.g. `America/New_York`), empty: the
     * clock of this device. With a zone, [start] and [end] are on the clock of that zone; the
     * calendar shows every occurrence converted.
     */
    val zone: String = "",
)

data class CalOccurrenceUi(
    val eventId: String,
    /** `holidays` for public holidays. */
    val calendarId: String,
    val title: String,
    val allDay: Boolean,
    val start: String,
    val end: String,
    val location: String,
    val repeating: Boolean,
    val hasReminders: Boolean,
    val hasLinks: Boolean,
    val age: Int,
    val color: Int,
) {
    val holiday: Boolean get() = calendarId == HOLIDAYS
    /** Title with the age for birthdays ("Oma Hilde (78)"). */
    val label: String get() = if (age > 0) "$title ($age)" else title

    companion object {
        const val HOLIDAYS = "holidays"
    }
}

data class CalReminderUi(
    val eventId: String,
    val title: String,
    val location: String,
    val allDay: Boolean,
    val start: String,
    val end: String,
    /** When to notify, `YYYY-MM-DDTHH:MM` local. */
    val at: String,
    val minutesBefore: Int,
)

/**
 * A prepared notification: [title]/[text] while the device is unlocked, [publicTitle]/[publicText]
 * on the lock screen (detail level chosen in the options). [key] is `eventId|YYYY-MM-DD`.
 */
data class CalAlarmUi(
    val key: String,
    val atMillis: Long,
    val title: String,
    val text: String,
    val publicTitle: String,
    val publicText: String,
    /** Wall-clock time `YYYY-MM-DDTHH:MM` of the reminder: it stays, whatever time zone the device moves to. */
    val atLocal: String = "",
)

/** Result of the quick-add parser. */
data class CalQuickUi(
    val title: String,
    val date: String,
    val start: String,
    val end: String,
    val allDay: Boolean,
    val repeat: CalRepeatUi,
    val calendarHint: String,
)

data class CalRegionUi(val code: String, val name: String)

/** GhulCal: encrypted calendar vaults (`.ghlcal`), no CalDAV. */
interface CalBackend {
    suspend fun calendars(): List<CalCalendarUi>
    suspend fun saveCalendar(cal: CalCalendarUi): String
    suspend fun deleteCalendar(id: String)
    suspend fun event(id: String): CalEventUi
    suspend fun saveEvent(event: CalEventUi): String
    suspend fun deleteEvent(id: String, date: String, mode: CalDeleteModeUi)
    suspend fun occurrences(from: String, to: String, holidayRegion: String, lang: String): List<CalOccurrenceUi>
    suspend fun reminders(now: String, days: Int): List<CalReminderUi>
    suspend fun exportIcs(calendarIds: List<String>): String
    suspend fun eventIcs(id: String): String
    /** Times in UTC or another time zone become local time of this device. */
    suspend fun importIcs(calendarId: String, data: ByteArray): Int
    fun quickParse(text: String, today: String, nowMinutes: Int): CalQuickUi
    fun regions(lang: String): List<CalRegionUi>

    /**
     * Hands the reminders of the next days to the system so they arrive while the suite is locked
     * (Android: exact alarms; Windows: while the program runs). Replaces earlier ones.
     */
    fun scheduleReminders(alarms: List<CalAlarmUi>, sound: Boolean) {}

    /** Panic lock / module off: removes every scheduled reminder and its stored copy. */
    fun clearReminders() {}

    /** Shows a reminder right now (test in the options). */
    fun showReminder(alarm: CalAlarmUi, sound: Boolean) {}
}

/** Backends without a calendar (tests): empty lists, saving fails. */
object NoCalBackend : CalBackend {
    override suspend fun calendars() = emptyList<CalCalendarUi>()
    override suspend fun saveCalendar(cal: CalCalendarUi): String = throw UnsupportedOperationException()
    override suspend fun deleteCalendar(id: String) = throw UnsupportedOperationException()
    override suspend fun event(id: String): CalEventUi = throw UnsupportedOperationException()
    override suspend fun saveEvent(event: CalEventUi): String = throw UnsupportedOperationException()
    override suspend fun deleteEvent(id: String, date: String, mode: CalDeleteModeUi) = throw UnsupportedOperationException()
    override suspend fun occurrences(from: String, to: String, holidayRegion: String, lang: String) = emptyList<CalOccurrenceUi>()
    override suspend fun reminders(now: String, days: Int) = emptyList<CalReminderUi>()
    override suspend fun exportIcs(calendarIds: List<String>) = ""
    override suspend fun eventIcs(id: String) = ""
    override suspend fun importIcs(calendarId: String, data: ByteArray) = 0
    override fun quickParse(text: String, today: String, nowMinutes: Int) = CalQuickUi(text, today, "", "", true, CalRepeatUi.NONE, "")
    override fun regions(lang: String) = emptyList<CalRegionUi>()
}
