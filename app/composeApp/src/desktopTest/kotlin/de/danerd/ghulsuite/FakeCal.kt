// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

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
import de.danerd.ghulsuite.ui.Day
import de.danerd.ghulsuite.ui.dateTime
import de.danerd.ghulsuite.ui.today

/** The sample week of the design preview, relative to today, for screenshots. */
class FakeCal : CalBackend {
    private val cals = listOf(
        CalCalendarUi("priv", "c1", "Privat", 0xD6336C, 15, 5),
        CalCalendarUi("work", "c1", "Arbeit", 0x2F74E8, 15, 6),
        CalCalendarUi("fam", "c1", "Familie", 0x0E9F80, 15, 3),
        CalCalendarUi("sport", "c1", "Sport", 0xE0700F, 30, 3),
        CalCalendarUi("bday", "c1", "Geburtstage", 0x8E5BD8, 1440, 1),
    )
    private val color = cals.associate { it.id to it.color }
    private val t0 = today()
    private val m0 = t0.plus(-t0.weekday)

    private class Ev(val id: String, val cal: String, val title: String, val day: Day, val s: Int, val e: Int, val loc: String = "",
                     val rep: Boolean = false, val rem: Boolean = true, val link: Boolean = false, val days: Int = 1, val age: Int = 0)

    private val events: List<Ev> = buildList {
        for (w in -6..6) for (d in 0..4) add(Ev("1", "work", "Team-Standup", m0.plus(w * 7 + d), 570, 585, "Video-Raum", rep = true))
        add(Ev("2", "priv", "Zahnarzt Dr. Weber", t0.plus(1), 600, 645, "Praxis am Markt, Hauptstraße 12", link = true))
        add(Ev("3", "work", "Quartalsbericht freigeben", m0.plus(2), 840, 900, link = true))
        add(Ev("4", "work", "Wartungsfenster VPN und Mail", m0.plus(5), 360, 540, rem = false, link = true))
        for (w in -6..6) {
            add(Ev("5", "sport", "Laufen im Stadtpark", m0.plus(w * 7 + 1), 1110, 1170, rep = true))
            add(Ev("6", "sport", "Laufen im Stadtpark", m0.plus(w * 7 + 3), 1110, 1170, rep = true))
            add(Ev("7", "sport", "Yoga", m0.plus(w * 7 + 6), 600, 675, "Studio Lotus", rep = true, rem = false))
        }
        add(Ev("8", "bday", "Oma Hilde", t0.plus(3), -1, -1, rep = true, age = 78))
        add(Ev("9", "priv", "Abendessen mit Jana", t0, 1140, 1260, "Trattoria Da Luca"))
        add(Ev("10", "fam", "Elternabend Klasse 3b", t0.plus(8), 1170, 1260, "Grundschule am Park, Raum 12"))
        add(Ev("11", "fam", "Urlaub an der Ostsee", t0.plus(24), -1, -1, "Zingst", days = 8, link = true))
        add(Ev("12", "priv", "Steuererklärung abgeben", t0.plus(18), -1, -1))
        add(Ev("14", "work", "Kundentermin Müller GmbH", t0, 600, 690, "Besprechungsraum 2"))
        add(Ev("15", "work", "Review Designentwurf", t0, 660, 720))
        add(Ev("16", "work", "Mittag mit dem Team", t0, 735, 795, "Kantine", rem = false))
        add(Ev("17", "priv", "Bürgeramt: Ausweis abholen", t0.plus(5), 560, 580, "Bürgerbüro Mitte", link = true))
    }

    override suspend fun calendars() = cals
    override suspend fun saveCalendar(cal: CalCalendarUi) = "x"
    override suspend fun deleteCalendar(id: String) {}
    override suspend fun event(id: String): CalEventUi {
        val e = events.first { it.id == id }
        return CalEventUi(
            id, e.cal, e.title, e.s < 0, if (e.s < 0) e.day.toString() else dateTime(e.day, e.s),
            if (e.s < 0) e.day.plus(e.days - 1).toString() else dateTime(e.day, e.e),
            if (e.rep) CalRepeatUi.WEEKDAYS else CalRepeatUi.NONE, "", emptyList(), if (e.rem) listOf(1440, 60) else emptyList(), e.loc,
            if (id == "2") "Überweisung und Versichertenkarte mitnehmen." else "",
            if (e.link) listOf(CalLinkUi("drive", "Überweisung.pdf", "f1")) else emptyList(),
        )
    }
    override suspend fun saveEvent(event: CalEventUi) = "x"
    override suspend fun deleteEvent(id: String, date: String, mode: CalDeleteModeUi) {}
    override suspend fun occurrences(from: String, to: String, holidayRegion: String, lang: String): List<CalOccurrenceUi> {
        val f = Day.parse(from)!!
        val t = Day.parse(to)!!
        val out = events.filter { it.day <= t && it.day.plus(it.days - 1) >= f }.map { e ->
            val all = e.s < 0
            CalOccurrenceUi(
                e.id, e.cal, e.title, all, if (all) e.day.toString() else dateTime(e.day, e.s),
                if (all) e.day.plus(e.days - 1).toString() else dateTime(e.day, e.e), e.loc, e.rep, e.rem, e.link, e.age, color[e.cal] ?: 0,
            )
        }.toMutableList()
        if (holidayRegion.isNotEmpty()) {
            val h = Day(t0.y, 11, 1)
            if (h in f..t) out += CalOccurrenceUi("", "holidays", "Allerheiligen", true, h.toString(), h.toString(), "", false, false, false, 0, 0x6F7187)
        }
        return out.sortedWith(compareBy({ it.start.take(10) }, { !it.allDay }, { it.start }))
    }
    override suspend fun reminders(now: String, days: Int) = emptyList<CalReminderUi>()
    override suspend fun exportIcs(calendarIds: List<String>) = ""
    override suspend fun eventIcs(id: String) = ""
    override suspend fun importIcs(calendarId: String, data: ByteArray) = 0
    override fun quickParse(text: String, today: String, nowMinutes: Int) =
        CalQuickUi("Friseur", t0.plus(1).toString(), "14:30", "15:30", false, CalRepeatUi.NONE, "privat")
    override fun regions(lang: String) = listOf(CalRegionUi("DE-NW", "Nordrhein-Westfalen"))
}
