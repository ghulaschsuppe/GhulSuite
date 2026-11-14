// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.CalCalendar
import de.danerd.ghulsuite.core.CalDeleteMode
import de.danerd.ghulsuite.core.CalEvent
import de.danerd.ghulsuite.core.CalLink
import de.danerd.ghulsuite.core.CalRepeat
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.calQuickParse
import de.danerd.ghulsuite.core.calRegions
import de.danerd.ghulsuite.model.CalBackend
import de.danerd.ghulsuite.model.CalCalendarUi
import de.danerd.ghulsuite.model.CalDeleteModeUi
import de.danerd.ghulsuite.model.CalEventUi
import de.danerd.ghulsuite.model.CalLinkUi
import de.danerd.ghulsuite.model.CalOccurrenceUi
import de.danerd.ghulsuite.model.CalQuickUi
import de.danerd.ghulsuite.model.CalRegionUi
import de.danerd.ghulsuite.model.CalReminderUi
import de.danerd.ghulsuite.model.CalRepeatUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** GhulCal on top of the Rust core, the same on Android and Windows; platforms add reminders. */
open class CoreCalBackend(protected val suite: () -> GhulSuite) : CalBackend {
    protected suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { mapped(block) }

    override suspend fun calendars(): List<CalCalendarUi> = io {
        suite().calCalendars().map { CalCalendarUi(it.id, it.vaultId, it.name, it.color.toInt(), it.defaultReminder, it.events.toInt()) }
    }

    override suspend fun saveCalendar(cal: CalCalendarUi): String = io {
        suite().calSaveCalendar(CalCalendar(cal.id, cal.vaultId, cal.name, cal.color.toUInt(), cal.defaultReminder, 0u))
    }

    override suspend fun deleteCalendar(id: String) = io { suite().calDeleteCalendar(id) }

    override suspend fun event(id: String): CalEventUi = io { suite().calGetEvent(id).toUi() }

    override suspend fun saveEvent(event: CalEventUi): String = io { suite().calSaveEvent(event.toCore()) }

    override suspend fun deleteEvent(id: String, date: String, mode: CalDeleteModeUi) = io {
        suite().calDeleteEvent(
            id, date,
            when (mode) {
                CalDeleteModeUi.ONE -> CalDeleteMode.ONE
                CalDeleteModeUi.FOLLOWING -> CalDeleteMode.FOLLOWING
                CalDeleteModeUi.ALL -> CalDeleteMode.ALL
            },
        )
    }

    override suspend fun occurrences(from: String, to: String, holidayRegion: String, lang: String): List<CalOccurrenceUi> = io {
        suite().calOccurrences(from, to, holidayRegion, lang).map {
            CalOccurrenceUi(
                it.eventId, it.calendarId, it.title, it.allDay, it.start, it.end, it.location,
                it.repeating, it.hasReminders, it.hasLinks, it.age.toInt(), it.color.toInt(),
            )
        }
    }

    override suspend fun reminders(now: String, days: Int): List<CalReminderUi> = io {
        suite().calReminders(now, days.toUInt()).map {
            CalReminderUi(it.eventId, it.title, it.location, it.allDay, it.start, it.end, it.at, it.minutesBefore.toInt())
        }
    }

    override suspend fun exportIcs(calendarIds: List<String>): String = io { suite().calExportIcs(calendarIds) }

    override suspend fun eventIcs(id: String): String = io { suite().calEventIcs(id) }

    override suspend fun importIcs(calendarId: String, data: ByteArray): Int =
        io { suite().calImportIcs(calendarId, data, DeviceTimeZones).toInt() }

    override fun quickParse(text: String, today: String, nowMinutes: Int): CalQuickUi {
        val q = calQuickParse(text, today, nowMinutes.toUInt())
        return CalQuickUi(q.title, q.date, q.start, q.end, q.allDay, q.repeat.toUi(), q.calendarHint)
    }

    override fun regions(lang: String): List<CalRegionUi> = calRegions(lang).map { CalRegionUi(it.code, it.name) }
}

internal fun CalRepeat.toUi() = when (this) {
    CalRepeat.NONE -> CalRepeatUi.NONE
    CalRepeat.DAILY -> CalRepeatUi.DAILY
    CalRepeat.WEEKDAYS -> CalRepeatUi.WEEKDAYS
    CalRepeat.WEEKLY -> CalRepeatUi.WEEKLY
    CalRepeat.BIWEEKLY -> CalRepeatUi.BIWEEKLY
    CalRepeat.MONTHLY -> CalRepeatUi.MONTHLY
    CalRepeat.YEARLY -> CalRepeatUi.YEARLY
}

internal fun CalRepeatUi.toCore() = when (this) {
    CalRepeatUi.NONE -> CalRepeat.NONE
    CalRepeatUi.DAILY -> CalRepeat.DAILY
    CalRepeatUi.WEEKDAYS -> CalRepeat.WEEKDAYS
    CalRepeatUi.WEEKLY -> CalRepeat.WEEKLY
    CalRepeatUi.BIWEEKLY -> CalRepeat.BIWEEKLY
    CalRepeatUi.MONTHLY -> CalRepeat.MONTHLY
    CalRepeatUi.YEARLY -> CalRepeat.YEARLY
}

internal fun CalEvent.toUi() = CalEventUi(
    id, calendarId, title, allDay, start, end, repeat.toUi(), repeatUntil, exceptions, reminders.map { it.toInt() },
    location, notes, links.map { CalLinkUi(it.kind, it.label, it.target) }, birthYear.toInt(), zone,
)

internal fun CalEventUi.toCore() = CalEvent(
    id, calendarId, title, allDay, start, end, repeat.toCore(), repeatUntil, exceptions, reminders.map { it.toUInt() },
    location, notes, links.map { CalLink(it.kind, it.label, it.target) }, birthYear.toUInt(), 0L, zone,
)
