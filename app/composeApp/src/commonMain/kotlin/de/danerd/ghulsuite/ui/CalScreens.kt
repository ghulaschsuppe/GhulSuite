// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Cake
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.SentimentSatisfied
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.CalCalendarUi
import de.danerd.ghulsuite.model.CalOccurrenceUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import androidx.compose.ui.focus.focusRequester
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

/** Views of GhulCal (the segmented control at the top). */
enum class CalView { MONTH, WEEK, DAY, LIST }

/** 0xRRGGBB as a colour. */
fun calColor(rgb: Int): Color = Color(0xFF000000.toInt() or (rgb and 0xFFFFFF))

/** Day names, month names and date formats of the current language. */
class CalNames(
    /** Monday first. */
    val wdShort: List<String>,
    val wdLong: List<String>,
    val months: List<String>,
    val monthsShort: List<String>,
    private val fmtShort: String,
    private val fmtLong: String,
    private val fmtDay: String,
    private val fmtDm: String,
    private val fmtD: String,
    val today: String,
    val tomorrow: String,
    val yesterday: String,
) {
    /** "Do., 12. Nov." */
    fun short(d: Day) = fmt(fmtShort, wdShort[d.weekday], d.d.toString(), monthsShort[d.m - 1])

    /** "Donnerstag, 12. November 2026" */
    fun long(d: Day) = fmt(fmtLong, wdLong[d.weekday], d.d.toString(), months[d.m - 1], d.y.toString())

    /** "Heute · Do., 12. Nov." or "Freitag, 13. November". */
    fun dayLabel(d: Day): String {
        val diff = (d.days - today().days).toInt()
        val rel = when (diff) {
            0 -> today
            1 -> tomorrow
            -1 -> yesterday
            else -> null
        }
        return if (rel != null) "$rel · ${short(d)}" else fmt(fmtDay, wdLong[d.weekday], d.d.toString(), months[d.m - 1])
    }

    fun month(d: Day) = "${months[d.m - 1]} ${d.y}"

    /** "15. Nov." */
    fun dm(d: Day) = fmt(fmtDm, d.d.toString(), monthsShort[d.m - 1])

    /** "9." */
    fun dOnly(d: Day) = fmt(fmtD, d.d.toString())

    private fun fmt(f: String, vararg a: String): String {
        var s = f
        a.forEachIndexed { i, v -> s = s.replace("%${i + 1}\$s", v) }
        return s
    }
}

@Composable
fun rememberCalNames(): CalNames {
    val wd = stringResource(Res.string.cal_wd_short)
    val wdl = stringResource(Res.string.cal_wd_long)
    val mo = stringResource(Res.string.cal_months)
    val mos = stringResource(Res.string.cal_months_short)
    val f1 = stringResource(Res.string.cal_fmt_short)
    val f2 = stringResource(Res.string.cal_fmt_long)
    val f3 = stringResource(Res.string.cal_fmt_day)
    val f4 = stringResource(Res.string.cal_fmt_dm)
    val f5 = stringResource(Res.string.cal_fmt_d)
    val t = stringResource(Res.string.cal_today)
    val tm = stringResource(Res.string.cal_tomorrow)
    val y = stringResource(Res.string.cal_yesterday)
    return remember(wd, mo, f1) {
        CalNames(wd.split(','), wdl.split(','), mo.split(','), mos.split(','), f1, f2, f3, f4, f5, t, tm, y)
    }
}

/** The same outside of composition (notifications). */
suspend fun loadCalNames(): CalNames = CalNames(
    getString(Res.string.cal_wd_short).split(','), getString(Res.string.cal_wd_long).split(','),
    getString(Res.string.cal_months).split(','), getString(Res.string.cal_months_short).split(','),
    getString(Res.string.cal_fmt_short), getString(Res.string.cal_fmt_long), getString(Res.string.cal_fmt_day),
    getString(Res.string.cal_fmt_dm), getString(Res.string.cal_fmt_d),
    getString(Res.string.cal_today), getString(Res.string.cal_tomorrow), getString(Res.string.cal_yesterday),
)

/**
 * Notification texts of a reminder: full while unlocked, on the lock screen per [detail]
 * (0 only "event in 15 min", 1 title and time, 2 title, time and place).
 */
suspend fun calAlarm(r: de.danerd.ghulsuite.model.CalReminderUi, detail: Int, names: CalNames): de.danerd.ghulsuite.model.CalAlarmUi {
    val day = Day.parse(r.start) ?: today()
    val time = if (r.allDay) names.short(day) + " · " + getString(Res.string.cal_all_day) else names.short(day) + ", " + hm(r.start) + " - " + hm(r.end)
    val full = time + if (r.location.isNotEmpty()) " · " + r.location else ""
    val soon = when {
        r.allDay && r.minutesBefore <= 0 -> getString(Res.string.cal_notify_today)
        r.allDay && r.minutesBefore <= 1440 -> getString(Res.string.cal_notify_tomorrow)
        r.allDay -> getString(Res.string.cal_notify_on, names.short(day))
        r.minutesBefore == 0 -> getString(Res.string.cal_notify_soon)
        else -> getString(Res.string.cal_notify_min, durationLabel(r.minutesBefore))
    }
    val publicTitle = if (detail == 0) soon else r.title
    val publicText = when (detail) {
        0 -> getString(Res.string.cal_notify_locked)
        1 -> if (r.allDay) time else getString(Res.string.cal_notify_in, durationLabel(r.minutesBefore), names.short(day) + ", " + hm(r.start))
        else -> full
    }
    return de.danerd.ghulsuite.model.CalAlarmUi("${r.eventId}|$day", epochMillisOf(r.at), r.title, full, publicTitle, publicText, r.at)
}

private suspend fun durationLabel(m: Int): String = when {
    m < 60 -> getString(Res.string.cal_dur_min, m.toString())
    m < 1440 -> getString(Res.string.cal_dur_hours, (m / 60).toString())
    m < 10080 -> getString(Res.string.cal_dur_days, (m / 1440).toString())
    else -> getString(Res.string.cal_dur_weeks, (m / 10080).toString())
}

/** First day of the week containing [d] (setting: Monday or Sunday). */
fun weekStartOf(d: Day, sundayFirst: Boolean): Day = d.plus(-((d.weekday + (if (sundayFirst) 1 else 0)) % 7))

/** Day of an occurrence's start. */
val CalOccurrenceUi.startDay: Day get() = Day.parse(start) ?: today()

/** Last day the occurrence touches (timed events ending at midnight end the day before). */
val CalOccurrenceUi.lastDay: Day
    get() {
        val e = Day.parse(end) ?: return startDay
        return if (!allDay && end.length >= 16 && minutesOf(end) == 0) e.plus(-1) else e
    }

fun List<CalOccurrenceUi>.on(d: Day): List<CalOccurrenceUi> = filter { it.startDay <= d && it.lastDay >= d }

private val HH = 44.dp
private const val H0 = 6
private const val H1 = 24

/** Calendars and occurrences shared by the tab and the sheets. */
class CalData(val calendars: List<CalCalendarUi>, val occ: List<CalOccurrenceUi>)

/** Loads what the current view shows; creates the first calendar vault on first use. */
@Composable
private fun rememberCalData(state: AppState, from: Day, to: Day, extraQuery: String = ""): CalData {
    val cal = state.backend.cal
    val settings = state.backend.settings
    var data by remember { mutableStateOf(CalData(emptyList(), emptyList())) }
    val lang = stringResource(Res.string.cal_lang)
    val defaults = listOf(
        stringResource(Res.string.cal_def_private) to 0xD6336C,
        stringResource(Res.string.cal_def_work) to 0x2F74E8,
        stringResource(Res.string.cal_def_family) to 0x0E9F80,
        stringResource(Res.string.cal_def_sport) to 0xE0700F,
        stringResource(Res.string.cal_def_birthdays) to 0x8E5BD8,
    )
    val rev = state.liveRevision()
    LaunchedEffect(rev, from, to, settings.calHidden, settings.calHolidays, settings.calRegion, extraQuery) {
        val cals = runCatching { cal.calendars() }.getOrDefault(emptyList())
        // A new GhulCal file (chosen in the setup or under Areas) gets the usual calendars once.
        // Without a GhulCal file nothing is created: the area is set up in Options → Areas.
        val calVault = state.vaultsOf(VaultKindUi.CAL).firstOrNull { it.available }
        if (cals.isEmpty() && calVault != null && !state.calSetupDone) {
            state.calSetupDone = true
            runCatching { for ((n, c) in defaults) cal.saveCalendar(CalCalendarUi("", calVault.id, n, c, 15, 0)) }
            state.changed()
            return@LaunchedEffect
        }
        val hidden = settings.calHiddenSet()
        val region = if (settings.calHolidays && CalOccurrenceUi.HOLIDAYS !in hidden) settings.calRegion else ""
        val occ = runCatching { cal.occurrences(from.toString(), to.toString(), region, lang) }.getOrDefault(emptyList())
            .filter { it.calendarId !in hidden }
        data = CalData(cals, occ)
        if (cals != state.calendars) state.calendars = cals
    }
    return data
}

/** The GhulCal tab. */
@Composable
fun CalScreen(state: AppState, padding: PaddingValues) {
    val settings = state.backend.settings
    val names = rememberCalNames()
    val sel = state.calSel
    val sundayFirst = settings.calWeekStart == 6
    var query by remember { mutableStateOf("") }
    val (from, to) = when (state.calView) {
        CalView.MONTH -> weekStartOf(sel.firstOfMonth, sundayFirst).let { it to it.plus(41) }
        CalView.WEEK, CalView.DAY -> weekStartOf(sel, sundayFirst).let { it to it.plus(6) }
        CalView.LIST -> if (query.isBlank()) today() to today().plus(60) else today().plus(-365) to today().plus(366)
    }
    val data = rememberCalData(state, from, to, query)
    val swipe = Modifier.pointerInput(state.calView) {
        var dx = 0f
        detectHorizontalDragGestures(onDragStart = { dx = 0f }, onDragEnd = {
            if (kotlin.math.abs(dx) > 60.dp.toPx()) state.calNav(if (dx < 0) 1 else -1)
        }) { _, d -> dx += d }
    }
    Column(
        Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 100.dp),
    ) {
        SingleChoiceSegmentedButtonRow(Modifier.padding(bottom = 10.dp)) {
            listOf(CalView.MONTH to Res.string.cal_view_month, CalView.WEEK to Res.string.cal_view_week, CalView.DAY to Res.string.cal_view_day, CalView.LIST to Res.string.cal_view_list)
                .forEach { (v, label) -> SegmentedButton(state.calView == v, { state.calView = v }) { Text(stringResource(label)) } }
        }
        when (state.calView) {
            CalView.MONTH -> {
                CalHead(state, names.month(sel))
                MonthGrid(state, data, names, sundayFirst, swipe)
                DayAgenda(state, data.occ, sel, names)
            }
            CalView.WEEK -> {
                val days = (0..6).map { from.plus(it) }
                val d6 = days[6]
                val label = stringResource(Res.string.cal_week_label, days[0].isoWeek.toString(),
                    if (days[0].m != d6.m) names.dm(days[0]) else names.dOnly(days[0]), names.dm(d6))
                CalHead(state, label)
                DayStrip(state, days, data.occ, names, pick = false, modifier = swipe)
                AllDayRow(state, days, data.occ)
                TimeGrid(state, days, data.occ, swipe)
            }
            CalView.DAY -> {
                val days = (0..6).map { from.plus(it) }
                CalHead(state, names.short(sel) + if (sel.y != today().y) " ${sel.y}" else "")
                DayStrip(state, days, data.occ, names, pick = true, modifier = swipe)
                val n = data.occ.on(sel).size
                Text(
                    when {
                        n == 0 -> stringResource(Res.string.cal_none)
                        sel == today() -> stringResource(if (n == 1) Res.string.cal_one_today else Res.string.cal_n_today, n.toString())
                        else -> stringResource(if (n == 1) Res.string.cal_one else Res.string.cal_n, n.toString())
                    },
                    Modifier.padding(start = 4.dp, end = 4.dp, bottom = 8.dp), fontSize = 13.sp, color = g.muted, fontWeight = FontWeight.SemiBold,
                )
                AllDayRow(state, listOf(sel), data.occ)
                TimeGrid(state, listOf(sel), data.occ, swipe)
            }
            CalView.LIST -> CalList(state, data.occ, query, { query = it }, names)
        }
    }
}

/** ‹ title ˅ › above the month, week and day views. */
@Composable
private fun CalHead(state: AppState, label: String) {
    var pick by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        SmallIcon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, stringResource(Res.string.cal_prev)) { state.calNav(-1) }
        Row(
            Modifier.weight(1f).height(36.dp).clip(RoundedCornerShape(12.dp)).clickable { pick = true },
            horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(label, fontSize = 16.sp, fontWeight = FontWeight.Bold, color = g.fg, maxLines = 1)
            Icon(Icons.Filled.KeyboardArrowDown, null, Modifier.padding(start = 4.dp).size(16.dp), tint = g.muted)
        }
        SmallIcon(Icons.AutoMirrored.Filled.KeyboardArrowRight, stringResource(Res.string.cal_next)) { state.calNav(1) }
    }
    if (pick) MonthPickSheet(state) { pick = false }
}

@Composable
private fun SmallIcon(icon: androidx.compose.ui.graphics.vector.ImageVector, description: String, onClick: () -> Unit) {
    Box(Modifier.size(36.dp).clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
        Icon(icon, description, Modifier.size(22.dp), tint = g.fg)
    }
}

/** Year with the twelve months and "today" (.mgrid). */
@Composable
private fun MonthPickSheet(state: AppState, close: () -> Unit) {
    val names = rememberCalNames()
    var year by remember { mutableStateOf(state.calSel.y) }
    val cal = areaColor(Tab.CAL)
    GhulSheet(close, stringResource(Res.string.cal_jump_to)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            SmallIcon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, stringResource(Res.string.cal_prev)) { year-- }
            Text(year.toString(), Modifier.weight(1f), textAlign = TextAlign.Center, fontSize = 16.sp, fontWeight = FontWeight.Bold)
            SmallIcon(Icons.AutoMirrored.Filled.KeyboardArrowRight, stringResource(Res.string.cal_next)) { year++ }
        }
        for (row in 0..2) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (col in 0..3) {
                    val m = row * 4 + col + 1
                    val on = state.calSel.y == year && state.calSel.m == m
                    val now = today().y == year && today().m == m
                    Box(
                        Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(14.dp)).background(if (on) cal else g.surface2)
                            .clickable {
                                state.calSel = if (now) today() else Day(year, m, 1)
                                if (state.calView == CalView.LIST) state.calView = CalView.MONTH
                                close()
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(names.months[m - 1].take(3), fontWeight = FontWeight.SemiBold, color = if (on) Color.White else if (now) cal else g.fg)
                    }
                }
            }
        }
        FilledTonalButton(onClick = { state.calSel = today(); close() }, modifier = Modifier.fillMaxWidth().padding(top = 14.dp)) {
            Text(stringResource(Res.string.cal_today_button))
        }
    }
}

// --- month -------------------------------------------------------------------------------------

@Composable
private fun MonthGrid(state: AppState, data: CalData, names: CalNames, sundayFirst: Boolean, swipe: Modifier) {
    val t = g
    val cal = areaColor(Tab.CAL)
    val sel = state.calSel
    val kw = state.backend.settings.calWeekNumbers
    val start = weekStartOf(sel.firstOfMonth, sundayFirst)
    val order = if (sundayFirst) listOf(6, 0, 1, 2, 3, 4, 5) else (0..6).toList()
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).then(swipe).padding(start = 4.dp, end = 4.dp, top = 8.dp, bottom = 4.dp)) {
        Row(Modifier.fillMaxWidth().padding(bottom = 4.dp)) {
            if (kw) Text(stringResource(Res.string.cal_kw), Modifier.width(20.dp), fontSize = 9.5.sp, fontWeight = FontWeight.Bold, color = t.faint, textAlign = TextAlign.Center, maxLines = 1, softWrap = false)
            for (i in order) Text(names.wdShort[i], Modifier.weight(1f), fontSize = 11.sp, fontWeight = FontWeight.Bold, color = t.faint, textAlign = TextAlign.Center)
        }
        for (w in 0 until 6) {
            val ws = start.plus(w * 7)
            if (w > 3 && ws.m != sel.m) break
            Row(Modifier.fillMaxWidth()) {
                if (kw) {
                    Text(ws.plus(if (sundayFirst) 1 else 0).isoWeek.toString(), Modifier.width(20.dp).padding(top = 9.dp),
                        fontSize = 9.5.sp, color = t.faint, textAlign = TextAlign.Center)
                }
                for (i in 0..6) {
                    val d = ws.plus(i)
                    val list = data.occ.on(d)
                    val out = d.m != sel.m
                    val isToday = d == today()
                    val isSel = d == sel
                    val weekend = d.weekday >= 5
                    val holiday = list.any { it.holiday }
                    Column(
                        Modifier.weight(1f).heightIn(min = 70.dp).clip(RoundedCornerShape(12.dp))
                            .then(if (isSel) Modifier.background(cal.copy(alpha = 0.09f)).border(1.5.dp, cal.copy(alpha = 0.5f), RoundedCornerShape(12.dp)) else Modifier)
                            .clickable { state.calSel = d }
                            .padding(horizontal = 2.dp, vertical = 3.dp),
                        verticalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        Box(
                            Modifier.align(Alignment.CenterHorizontally).size(24.dp).clip(CircleShape).background(if (isToday) cal else Color.Transparent),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                d.d.toString(), fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                                color = when {
                                    isToday -> Color.White
                                    out -> t.faint
                                    holiday -> cal
                                    weekend -> t.muted
                                    else -> t.fg
                                },
                            )
                        }
                        for (o in list.take(3)) Pill(o, Modifier.alpha(if (out) 0.45f else 1f))
                        if (list.size > 3) Text("+${list.size - 3}", Modifier.padding(start = 3.dp), fontSize = 10.sp, color = t.muted, fontWeight = FontWeight.Bold)
                    }
                }
            }
        }
    }
}

/** Small event label in the month grid and the all-day row (.pill). */
@Composable
private fun Pill(o: CalOccurrenceUi, modifier: Modifier = Modifier, big: Boolean = false, onClick: (() -> Unit)? = null) {
    val c = calColor(o.color)
    Text(
        o.label,
        modifier.fillMaxWidth().height(if (big) 18.dp else 14.dp).clip(RoundedCornerShape(4.dp))
            .background(if (o.allDay) c else c.copy(alpha = 0.2f))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = if (big) 5.dp else 3.dp),
        fontSize = if (big) 11.sp else 10.sp, lineHeight = if (big) 18.sp else 14.sp, fontWeight = FontWeight.SemiBold,
        color = if (o.allDay) Color.White else g.fg, maxLines = 1, softWrap = false, overflow = TextOverflow.Clip,
    )
}

/** Section with the selected day's events under the month (+ Termin). */
@Composable
private fun DayAgenda(state: AppState, occ: List<CalOccurrenceUi>, d: Day, names: CalNames) {
    val list = occ.on(d)
    SectionLabel(names.dayLabel(d), Modifier.padding(top = 12.dp, bottom = 8.dp)) {
        Row(Modifier.clickable { state.push(Screen.CalEdit(newEventDraft(state, d, null))) }, verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Add, null, Modifier.size(16.dp), tint = g.accent)
            Text(stringResource(Res.string.cal_event_short))
        }
    }
    CalCard {
        if (list.isEmpty()) {
            Row(Modifier.fillMaxWidth().padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Icon(Icons.Outlined.SentimentSatisfied, null, Modifier.size(20.dp), tint = g.muted)
                Text(stringResource(Res.string.cal_free_day), fontSize = 14.sp, color = g.muted)
            }
        }
        list.forEachIndexed { i, o -> EvRow(state, o, d, divider = i > 0) }
    }
}

/** Card without padding (.card.flush). */
@Composable
fun CalCard(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(g.surface).padding(vertical = 4.dp)) { content() }
}

/** One event in lists (.ev-row): times, colour bar, title, place, symbols. */
@Composable
fun EvRow(state: AppState, o: CalOccurrenceUi, day: Day?, divider: Boolean) {
    val t = g
    val c = calColor(o.color)
    val past = absMinutes(if (o.allDay) o.lastDay.plus(1).toString() else o.end) < absMinutes(localNow())
    if (divider) Box(Modifier.fillMaxWidth().padding(horizontal = 16.dp).height(1.dp).background(t.line))
    Row(
        Modifier.fillMaxWidth().clickable { openOccurrence(state, o) }.alpha(if (past) 0.55f else 1f)
            .padding(horizontal = 16.dp, vertical = 10.dp).height(androidx.compose.foundation.layout.IntrinsicSize.Min),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.width(44.dp)) {
            if (o.allDay) {
                Text(stringResource(Res.string.cal_all_day_short), fontSize = 13.sp, fontWeight = FontWeight.Bold, lineHeight = 17.sp)
                val days = (o.lastDay.days - o.startDay.days + 1).toInt()
                if (days > 1) {
                    val n = ((day ?: o.startDay).days - o.startDay.days + 1).toInt()
                    Text(stringResource(Res.string.cal_day_of, n.toString(), days.toString()), fontSize = 13.sp, color = t.faint, lineHeight = 17.sp)
                }
            } else {
                Text(hm(o.start), fontSize = 13.sp, fontWeight = FontWeight.Bold, lineHeight = 17.sp)
                Text(hm(o.end), fontSize = 13.sp, color = t.faint, fontWeight = FontWeight.W500, lineHeight = 17.sp)
            }
        }
        Box(Modifier.width(4.dp).fillMaxHeight().clip(RoundedCornerShape(2.dp)).background(c))
        Column(Modifier.weight(1f)) {
            Text(o.label, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, color = t.fg)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                when {
                    o.location.isNotEmpty() -> {
                        Icon(Icons.Filled.Place, null, Modifier.size(14.dp), tint = t.muted)
                        Text(o.location, fontSize = 13.sp, color = t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    o.age > 0 -> {
                        Icon(Icons.Filled.Cake, null, Modifier.size(14.dp), tint = t.muted)
                        Text(stringResource(Res.string.cal_birthday), fontSize = 13.sp, color = t.muted)
                    }
                    else -> Text(calendarName(state, o), fontSize = 13.sp, color = t.muted, maxLines = 1)
                }
            }
        }
        Row(Modifier.align(Alignment.CenterVertically), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            if (o.repeating) Icon(Icons.Filled.Repeat, null, Modifier.size(15.dp), tint = t.faint)
            if (o.hasReminders) Icon(Icons.Outlined.Notifications, null, Modifier.size(15.dp), tint = t.faint)
            if (o.hasLinks) Icon(Icons.Filled.Link, null, Modifier.size(15.dp), tint = t.faint)
        }
    }
}

@Composable
private fun calendarName(state: AppState, o: CalOccurrenceUi): String =
    if (o.holiday) stringResource(Res.string.cal_holidays) else state.calNames[o.calendarId] ?: ""

fun openOccurrence(state: AppState, o: CalOccurrenceUi) {
    if (o.holiday) {
        state.scope.launch { state.message(getString(Res.string.cal_holiday_in, state.calRegionName)) }
        return
    }
    state.push(Screen.CalEvent(o.eventId, o.startDay.toString()))
}

// --- week and day -------------------------------------------------------------------------------

/** Day headers of the week view, or the day picker of the day view (.cal-strip). */
@Composable
private fun DayStrip(state: AppState, days: List<Day>, occ: List<CalOccurrenceUi>, names: CalNames, pick: Boolean, modifier: Modifier) {
    val t = g
    val cal = areaColor(Tab.CAL)
    Row(
        Modifier.fillMaxWidth().padding(bottom = if (pick) 8.dp else 6.dp)
            .then(if (pick) Modifier.clip(RoundedCornerShape(20.dp)).background(t.surface).padding(horizontal = 4.dp, vertical = 6.dp) else Modifier)
            .then(modifier),
    ) {
        if (!pick) Spacer(Modifier.width(38.dp))
        for (d in days) {
            val isToday = d == today()
            val isSel = d == state.calSel
            val filled = if (pick) isSel else isToday
            Column(
                Modifier.weight(1f).clip(RoundedCornerShape(12.dp)).clickable {
                    state.calSel = d
                    if (!pick) state.calView = CalView.DAY
                }.padding(vertical = 4.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(names.wdShort[d.weekday], fontSize = 11.sp, fontWeight = FontWeight.Bold, color = t.faint)
                Box(Modifier.size(30.dp).clip(CircleShape).background(if (filled) cal else Color.Transparent), contentAlignment = Alignment.Center) {
                    Text(d.d.toString(), fontSize = 15.sp, fontWeight = FontWeight.Bold, color = if (filled) Color.White else if (pick && isToday) cal else t.fg)
                }
                Row(Modifier.height(5.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                    for (o in occ.on(d).take(3)) Box(Modifier.size(5.dp).clip(CircleShape).background(calColor(o.color)))
                }
            }
        }
    }
}

/** All-day events above the time grid ("ganzt."). */
@Composable
private fun AllDayRow(state: AppState, days: List<Day>, occ: List<CalOccurrenceUi>) {
    val cells = days.map { d -> occ.on(d).filter { it.allDay } }
    if (cells.all { it.isEmpty() }) return
    Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(stringResource(Res.string.cal_all_day_short), Modifier.width(38.dp).padding(top = 2.dp, end = 6.dp), fontSize = 9.5.sp, color = g.faint, textAlign = TextAlign.End)
        for (list in cells) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                for (o in list) Pill(o, big = true) { openOccurrence(state, o) }
            }
        }
    }
}

private class Placed(val o: CalOccurrenceUi, val lane: Int, var lanes: Int)

/** Side by side columns for overlapping events (like the preview). */
private fun lanesOf(list: List<CalOccurrenceUi>): List<Placed> {
    val out = mutableListOf<Placed>()
    var cluster = mutableListOf<Placed>()
    var ends = mutableListOf<Long>()
    var clusterEnd = Long.MIN_VALUE
    fun flush() {
        cluster.forEach { it.lanes = ends.size }
        out += cluster
        cluster = mutableListOf()
        ends = mutableListOf()
    }
    for (o in list.sortedWith(compareBy({ absMinutes(it.start) }, { -absMinutes(it.end) }))) {
        val s = absMinutes(o.start)
        val e = absMinutes(o.end)
        if (cluster.isNotEmpty() && s >= clusterEnd) flush()
        var l = ends.indexOfFirst { it <= s }
        if (l < 0) {
            l = ends.size
            ends += e
        } else {
            ends[l] = e
        }
        clusterEnd = if (cluster.isEmpty()) e else maxOf(clusterEnd, e)
        cluster += Placed(o, l, 1)
    }
    flush()
    return out
}

/** Hours 6 to 24 with the events of one or seven days (.tg). */
@Composable
private fun TimeGrid(state: AppState, days: List<Day>, occ: List<CalOccurrenceUi>, swipe: Modifier) {
    val t = g
    val cal = areaColor(Tab.CAL)
    val one = days.size == 1
    var now by remember { mutableStateOf(localNow()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            now = localNow()
        }
    }
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).then(swipe).padding(top = 12.dp, bottom = 12.dp, end = 6.dp)) {
        Column(Modifier.width(38.dp)) {
            for (h in H0 until H1) {
                Box(Modifier.height(HH).fillMaxWidth()) {
                    if (h > H0) {
                        Text("${pad(h)}:00", Modifier.align(Alignment.TopEnd).offset(y = (-7).dp).padding(end = 6.dp), fontSize = 10.sp, color = t.faint)
                    }
                }
            }
        }
        for (d in days) {
            val isToday = d == today()
            val line = t.line
            BoxWithConstraints(
                Modifier.weight(1f).height(HH * (H1 - H0))
                    .background(if (isToday) cal.copy(alpha = 0.05f) else Color.Transparent)
                    .drawBehind {
                        drawLine(line, Offset(0f, 0f), Offset(0f, size.height), 1.dp.toPx())
                        val step = HH.toPx()
                        var y = 0f
                        while (y < size.height) {
                            drawLine(line, Offset(0f, y), Offset(size.width, y), 1.dp.toPx())
                            y += step
                        }
                    },
            ) {
                val w = maxWidth
                for (h in H0 until H1) {
                    Box(
                        Modifier.offset(y = HH * (h - H0)).fillMaxWidth().height(HH)
                            .clickable { state.push(Screen.CalEdit(newEventDraft(state, d, h))) },
                    )
                }
                val dayStart = d.days * 1440 + H0 * 60L
                val dayEnd = (d.days + 1) * 1440
                val timed = occ.on(d).filter { !it.allDay && absMinutes(it.end) > dayStart }
                for (p in lanesOf(timed)) {
                    val s = maxOf(absMinutes(p.o.start), dayStart)
                    val e = minOf(absMinutes(p.o.end), dayEnd)
                    val top = HH * ((s - dayStart) / 60f)
                    val h = maxOf(HH * ((e - s) / 60f) - 2.dp, 18.dp)
                    val c = calColor(p.o.color)
                    val past = absMinutes(p.o.end) < absMinutes(now)
                    Column(
                        Modifier.offset(x = w * p.lane / p.lanes + 1.dp, y = top).width(w / p.lanes - 2.dp).height(h)
                            .clip(RoundedCornerShape(7.dp)).background(lerp(t.surface, c, 0.2f))
                            .clickable { openOccurrence(state, p.o) }.alpha(if (past) 0.6f else 1f),
                    ) {
                        Row(Modifier.fillMaxSize()) {
                            Box(Modifier.width(3.dp).fillMaxHeight().background(c))
                            Column(Modifier.padding(horizontal = if (one) 9.dp else 4.dp, vertical = if (h < 30.dp) 1.dp else if (one) 5.dp else 2.dp)) {
                                Text(p.o.label, fontSize = if (one) 13.sp else 10.5.sp, lineHeight = if (one) 16.sp else 13.sp, fontWeight = FontWeight.Bold,
                                    color = t.fg, maxLines = 1, softWrap = false, overflow = if (one) TextOverflow.Ellipsis else TextOverflow.Clip)
                                if (h > 34.dp) {
                                    val meta = "${hm(p.o.start)}-${hm(p.o.end)}" + if (one && p.o.location.isNotEmpty()) " · ${p.o.location}" else ""
                                    Text(meta, fontSize = if (one) 13.sp else 10.5.sp, lineHeight = if (one) 16.sp else 13.sp, color = t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                            }
                        }
                    }
                }
                if (isToday && absMinutes(now) >= dayStart) {
                    val y = HH * ((absMinutes(now) - dayStart) / 60f)
                    Box(Modifier.offset(y = y - 1.dp).fillMaxWidth().height(2.dp).background(cal))
                    Box(Modifier.offset(x = (-5).dp, y = y - 5.dp).size(10.dp).clip(CircleShape).background(cal))
                }
            }
        }
    }
}

// --- list ----------------------------------------------------------------------------------------

@Composable
private fun CalList(state: AppState, occ: List<CalOccurrenceUi>, query: String, onQuery: (String) -> Unit, names: CalNames) {
    val q = query.trim().lowercase()
    val from = if (q.isEmpty()) today() else today().plus(-365)
    val list = if (q.isEmpty()) occ else occ.filter {
        listOf(it.title, it.location, state.calNames[it.calendarId] ?: "").joinToString(" ").lowercase().contains(q)
    }
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    LaunchedEffect(state.calSearchTick) { if (state.calSearchTick > 0) runCatching { focus.requestFocus() } }
    SearchField(query, onQuery, Modifier.padding(bottom = 12.dp).focusRequester(focus), placeholder = stringResource(Res.string.cal_search))
    val groups = list.groupBy { if (it.startDay < from) from else it.startDay }.toSortedMap()
    if (groups.isEmpty()) {
        Text(
            if (q.isEmpty()) stringResource(Res.string.cal_list_empty) else stringResource(Res.string.cal_search_empty, query),
            Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 40.dp), textAlign = TextAlign.Center, color = g.muted,
        )
    }
    for ((d, items) in groups) {
        SectionLabel(names.dayLabel(d), Modifier.padding(top = 12.dp, bottom = 8.dp))
        CalCard { items.forEachIndexed { i, o -> EvRow(state, o, d, divider = i > 0) } }
    }
    HintText(stringResource(if (q.isEmpty()) Res.string.cal_list_hint else Res.string.cal_search_hint), Modifier.padding(top = 10.dp))
}

/** The "12" icon in the header: jumps to today. */
@Composable
fun CalTodayIcon(onClick: () -> Unit) {
    Box(Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
        Box(Modifier.size(24.dp).border(BorderStroke(2.dp, g.fg), RoundedCornerShape(7.dp)), contentAlignment = Alignment.Center) {
            Text(today().d.toString(), fontSize = 11.sp, fontWeight = FontWeight.ExtraBold, color = g.fg, lineHeight = 11.sp)
        }
    }
}
