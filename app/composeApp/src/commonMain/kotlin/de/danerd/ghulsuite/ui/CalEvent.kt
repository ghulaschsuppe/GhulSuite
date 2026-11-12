// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Bolt
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.CalCalendarUi
import de.danerd.ghulsuite.model.CalDeleteModeUi
import de.danerd.ghulsuite.model.CalEventUi
import de.danerd.ghulsuite.model.CalLinkUi
import de.danerd.ghulsuite.model.CalOccurrenceUi
import de.danerd.ghulsuite.model.CalRepeatUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

val REPEAT_LABELS: List<Pair<CalRepeatUi, StringResource>> = listOf(
    CalRepeatUi.NONE to Res.string.cal_rep_none,
    CalRepeatUi.DAILY to Res.string.cal_rep_daily,
    CalRepeatUi.WEEKDAYS to Res.string.cal_rep_weekdays,
    CalRepeatUi.WEEKLY to Res.string.cal_rep_weekly,
    CalRepeatUi.BIWEEKLY to Res.string.cal_rep_biweekly,
    CalRepeatUi.MONTHLY to Res.string.cal_rep_monthly,
    CalRepeatUi.YEARLY to Res.string.cal_rep_yearly,
)

/** "15 Min. vorher", "Am Vortag um 9:00" … */
suspend fun reminderLabel(m: Int, allDay: Boolean): String = when {
    allDay && m == 0 -> getString(Res.string.cal_rem_day_9)
    allDay && m == 1440 -> getString(Res.string.cal_rem_daybefore_9)
    allDay && m == 10080 -> getString(Res.string.cal_rem_week)
    allDay -> getString(Res.string.cal_rem_days, (m / 1440).toString())
    m == 0 -> getString(Res.string.cal_rem_start)
    m < 60 -> getString(Res.string.cal_rem_min, m.toString())
    m < 1440 -> getString(Res.string.cal_rem_hours, (m / 60).toString())
    m == 1440 -> getString(Res.string.cal_rem_day)
    m == 10080 -> getString(Res.string.cal_rem_week)
    else -> getString(Res.string.cal_rem_days, (m / 1440).toString())
}

@Composable
fun reminderText(m: Int, allDay: Boolean): String = when {
    allDay && m == 0 -> stringResource(Res.string.cal_rem_day_9)
    allDay && m == 1440 -> stringResource(Res.string.cal_rem_daybefore_9)
    allDay && m == 10080 -> stringResource(Res.string.cal_rem_week)
    allDay -> stringResource(Res.string.cal_rem_days, (m / 1440).toString())
    m == 0 -> stringResource(Res.string.cal_rem_start)
    m < 60 -> stringResource(Res.string.cal_rem_min, m.toString())
    m < 1440 -> stringResource(Res.string.cal_rem_hours, (m / 60).toString())
    m == 1440 -> stringResource(Res.string.cal_rem_day)
    m == 10080 -> stringResource(Res.string.cal_rem_week)
    else -> stringResource(Res.string.cal_rem_days, (m / 1440).toString())
}

/** Loads the calendars if a screen opens before the tab did (e.g. from a reminder). */
@Composable
private fun EnsureCalendars(state: AppState) {
    LaunchedEffect(Unit) {
        if (state.calendars.isEmpty()) state.calendars = runCatching { state.backend.cal.calendars() }.getOrDefault(emptyList())
    }
}

/** Small filled button of the header (.btn.primary, 36 dp). */
@Composable
private fun HeaderButton(text: String, onClick: () -> Unit) {
    Box(
        Modifier.height(36.dp).clip(RoundedCornerShape(18.dp)).background(g.accent).clickable(onClick = onClick).padding(horizontal = 16.dp),
        contentAlignment = Alignment.Center,
    ) { Text(text, color = g.onAccent, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1) }
}

/** A new event on [day] (at [hour], or the next full hour today, else 10:00). */
fun newEventDraft(state: AppState, day: Day, hour: Int?): CalEventUi {
    val nowMin = minutesOf(localNow())
    val h = hour ?: if (day == today()) minOf(nowMin / 60 + 1, 22) else 10
    val cal = state.calDefaultCalendar
    val rem = state.calendars.firstOrNull { it.id == cal }?.defaultReminder ?: state.backend.settings.calDefaultReminder
    return CalEventUi(
        calendarId = cal, start = dateTime(day, h * 60), end = dateTime(day, minOf(h + 1, 23) * 60 + if (h >= 23) 59 else 0),
        reminders = if (rem >= 0) listOf(rem) else emptyList(),
    )
}

// --- detail ------------------------------------------------------------------------------------

@Composable
fun CalEventScreen(state: AppState, id: String, day: String, padding: PaddingValues) {
    val cal = state.backend.cal
    var ev by remember { mutableStateOf<CalEventUi?>(null) }
    var next by remember { mutableStateOf<List<CalOccurrenceUi>>(emptyList()) }
    var gone by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    val names = rememberCalNames()
    val lang = stringResource(Res.string.cal_lang)
    LaunchedEffect(id, state.liveRevision()) {
        runCatching { cal.event(id) }.onSuccess { e ->
            ev = e
            next = if (e.repeat != CalRepeatUi.NONE) {
                val from = (Day.parse(day) ?: today()).plus(1)
                runCatching { cal.occurrences(from.toString(), from.plus(400).toString(), "", lang) }.getOrDefault(emptyList())
                    .filter { it.eventId == id }.take(3)
            } else emptyList()
        }.onFailure { gone = true }
    }
    EnsureCalendars(state)
    val t = g
    val shareIcs = rememberSaveFile { }
    val uri = androidx.compose.ui.platform.LocalUriHandler.current
    androidx.compose.foundation.layout.Column(Modifier.padding(padding)) {
        GhulTopBar(
            "", onLock = { state.backend.lock() }, onBack = { state.pop() },
            actions = {
                if (ev != null) {
                    BarIcon(Icons.Filled.Edit, stringResource(Res.string.a_edit)) { state.push(Screen.CalEdit(ev!!)) }
                    BarIcon(Icons.Filled.MoreVert, stringResource(Res.string.entry_more)) { menu = true }
                }
            },
        )
        ScreenColumn {
            val e = ev
            if (e == null) {
                if (gone) Text(stringResource(Res.string.cal_deleted), color = t.muted, modifier = Modifier.padding(40.dp))
                return@ScreenColumn
            }
            val c = state.calendars.firstOrNull { it.id == e.calendarId }
            val color = calColor(c?.color ?: 0x2F74E8)
            val d0 = Day.parse(day) ?: Day.parse(e.start) ?: today()
            val (start, end) = occurrenceTimes(e, d0)
            // Hero (.ev-hero): colour stripe, calendar tag, title, date and time, repetition.
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min).clip(RoundedCornerShape(20.dp)).background(t.surface)) {
                Box(Modifier.width(6.dp).fillMaxHeight().background(color))
                Column(Modifier.padding(18.dp)) {
                    CalTag(c?.name ?: "", color)
                    Text(if (e.birthYear > 0) "${e.title} (${d0.y - e.birthYear})" else e.title,
                        Modifier.padding(top = 10.dp, bottom = 6.dp), fontSize = 24.sp, fontWeight = FontWeight.Bold, lineHeight = 29.sp, color = t.fg)
                    val days = ((Day.parse(end)?.days ?: d0.days) - d0.days + 1).toInt()
                    if (e.allDay) {
                        if (days > 1) {
                            Text("${names.short(d0)} – ${names.short(Day.parse(end)!!)} ${Day.parse(end)!!.y}", fontWeight = FontWeight.SemiBold, color = t.fg)
                            Text(stringResource(Res.string.cal_all_day_days, days.toString()), color = t.muted, modifier = Modifier.padding(top = 2.dp))
                        } else {
                            Text(names.long(d0), fontWeight = FontWeight.SemiBold, color = t.fg)
                            Text(stringResource(Res.string.cal_all_day), color = t.muted, modifier = Modifier.padding(top = 2.dp))
                        }
                    } else {
                        Text(names.long(d0), fontWeight = FontWeight.SemiBold, color = t.fg)
                        Text("${hm(start)} – ${hm(end)} · ${durationText(absMinutes(end) - absMinutes(start))}", color = t.muted, modifier = Modifier.padding(top = 2.dp))
                    }
                    if (e.repeat != CalRepeatUi.NONE) {
                        Row(Modifier.padding(top = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Icon(Icons.Filled.Repeat, null, Modifier.size(16.dp), tint = t.muted)
                            val until = Day.parse(e.repeatUntil)
                            Text(stringResource(REPEAT_LABELS.first { it.first == e.repeat }.second) + if (until != null) " · " + stringResource(Res.string.cal_until, names.short(until)) else "", color = t.muted)
                        }
                    }
                }
            }
            CalCard {
                var first = true
                if (e.location.isNotEmpty()) {
                    SheetRow(e.location, stringResource(Res.string.cal_open_map), leading = { IconTile(Icons.Filled.Place, paletteColor(7)) },
                        end = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = t.faint) }) {
                        val q = urlEncode(e.location)
                        runCatching { uri.openUri(if (state.backend.isDesktop) "https://www.openstreetmap.org/search?query=$q" else "geo:0,0?q=$q") }
                    }
                    first = false
                }
                val rem = e.reminders.sortedDescending().map { reminderText(it, e.allDay) }.joinToString(", ")
                SheetRow(
                    if (e.reminders.isEmpty()) stringResource(Res.string.cal_no_reminder) else rem,
                    stringResource(if (e.reminders.isEmpty()) Res.string.cal_tap_to_add else Res.string.cal_reminder_sub),
                    leading = { IconTile(Icons.Outlined.Notifications, paletteColor(4)) }, divider = !first,
                    end = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = t.faint) },
                ) { state.push(Screen.CalEdit(e)) }
                if (e.notes.isNotEmpty()) {
                    Box(Modifier.fillMaxWidth().padding(start = 70.dp, end = 16.dp).height(1.dp).background(t.line))
                    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                        IconTile(Icons.Outlined.Description, paletteColor(9))
                        Text(e.notes, fontSize = 14.sp, color = t.fg, modifier = Modifier.weight(1f))
                    }
                }
            }
            if (e.links.isNotEmpty()) {
                SectionLabel(stringResource(Res.string.cal_linked))
                CalCard {
                    e.links.forEachIndexed { i, l ->
                        val (icon, ci, area) = linkLook(l.kind)
                        SheetRow(l.label, stringResource(area), leading = { IconTile(icon, paletteColor(ci)) }, divider = i > 0,
                            end = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = t.faint) }) { openLink(state, l) }
                    }
                }
            }
            if (next.isNotEmpty()) {
                SectionLabel(stringResource(Res.string.cal_next_in_series))
                CalCard { next.forEachIndexed { i, o -> EvRow(state, o, null, divider = i > 0) } }
            }
            ButtonRow {
                FilledTonalButton(onClick = {
                    state.run(block = { cal.eventIcs(id) }) { ics ->
                        state.run(block = { state.backend.net.shareBytesToApps("${safeName(e.title)}.ics", "text/calendar", ics.encodeToByteArray()) }) { ok ->
                            if (!ok) shareIcs("${safeName(e.title)}.ics", ics.encodeToByteArray())
                        }
                    }
                }, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Filled.Upload, null, Modifier.size(18.dp)); Text(stringResource(Res.string.cal_share_ics), Modifier.padding(start = 6.dp), maxLines = 1, softWrap = false)
                }
                OutlinedButton(onClick = {
                    state.run(block = { cal.eventIcs(id) }) { ics ->
                        state.push(Screen.MailCompose(MailDraft(state.mailAccount, subject = e.title,
                            attachments = listOf(de.danerd.ghulsuite.model.OutgoingAttachmentUi("${safeName(e.title)}.ics", "text/calendar", ics.encodeToByteArray())))))
                    }
                }, modifier = Modifier.weight(1f)) {
                    Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Text(stringResource(Res.string.cal_by_mail), Modifier.padding(start = 8.dp), maxLines = 1, softWrap = false)
                }
            }
            HintText(stringResource(Res.string.cal_invite_hint))
        }
    }
    val e = ev ?: return
    if (menu) {
        val close = { menu = false }
        GhulSheet(close, e.title) {
            SheetList {
                SheetRow(stringResource(Res.string.cal_duplicate)) {
                    close()
                    state.run(block = { cal.saveEvent(e.copy(id = "", title = e.title + " " + getString(Res.string.cal_copy_suffix), exceptions = emptyList())) }) {
                        state.changed(); state.pop(); state.messageRes(Res.string.cal_copied)
                    }
                }
                SheetRow(stringResource(Res.string.cal_move), divider = true) { close(); moving = true }
                SheetRow(stringResource(Res.string.cal_save_ics), divider = true) {
                    close(); state.run(block = { cal.eventIcs(id) }) { shareIcs("${safeName(e.title)}.ics", it.encodeToByteArray()) }
                }
                SheetRow(stringResource(Res.string.a_delete), color = t.danger, divider = true) {
                    close()
                    if (e.repeat != CalRepeatUi.NONE) deleting = true
                    else state.run(block = { cal.deleteEvent(id, day, CalDeleteModeUi.ALL) }) { state.changed(); state.pop(); state.messageRes(Res.string.cal_deleted_msg) }
                }
            }
        }
    }
    if (deleting) {
        val close = { deleting = false }
        GhulSheet(close, stringResource(Res.string.cal_delete_series)) {
            SheetList {
                fun del(mode: CalDeleteModeUi) {
                    close()
                    state.run(block = { cal.deleteEvent(id, day, mode) }) {
                        state.changed(); state.pop()
                        state.messageRes(if (mode == CalDeleteModeUi.ONE) Res.string.cal_deleted_one else Res.string.cal_deleted_series)
                    }
                }
                SheetRow(stringResource(Res.string.cal_delete_one), Day.parse(day)?.let { names.short(it) }) { del(CalDeleteModeUi.ONE) }
                SheetRow(stringResource(Res.string.cal_delete_following), divider = true) { del(CalDeleteModeUi.FOLLOWING) }
                SheetRow(stringResource(Res.string.cal_delete_all), color = t.danger, divider = true) { del(CalDeleteModeUi.ALL) }
            }
        }
    }
    if (moving) {
        val close = { moving = false }
        GhulSheet(close, stringResource(Res.string.cal_move_to)) {
            SheetList {
                state.calendars.forEachIndexed { i, c ->
                    SheetRow(c.name, leading = { ColorDot(calColor(c.color)) }, divider = i > 0,
                        end = { if (c.id == e.calendarId) Icon(Icons.Filled.Check, null, tint = t.muted) }) {
                        close()
                        state.run(block = { cal.saveEvent(e.copy(calendarId = c.id)) }) {
                            state.changed()
                            state.scope.launch { state.message(getString(Res.string.cal_moved_to, c.name)) }
                        }
                    }
                }
            }
        }
    }
}

/** Start and end of the occurrence on [day] (series keep their time and length). */
fun occurrenceTimes(e: CalEventUi, day: Day): Pair<String, String> {
    if (e.allDay) {
        val len = (Day.parse(e.end)?.days ?: 0L) - (Day.parse(e.start)?.days ?: 0L)
        return day.toString() to day.plus(len.toInt().coerceAtLeast(0)).toString()
    }
    val s = dateTime(day, minutesOf(e.start))
    return s to fromAbsMinutes(absMinutes(s) + (absMinutes(e.end) - absMinutes(e.start)))
}

@Composable
private fun durationText(min: Long): String =
    if (min < 60) stringResource(Res.string.cal_dur_min, min.toString())
    else stringResource(Res.string.cal_dur_hours, ((min / 6) / 10.0).toString().removeSuffix(".0").replace('.', ','))

private fun linkLook(kind: String): Triple<androidx.compose.ui.graphics.vector.ImageVector, Int, StringResource> = when (kind) {
    "drive" -> Triple(Icons.Filled.Cloud, 1, Res.string.tab_drive)
    "mail" -> Triple(Icons.Filled.Email, 3, Res.string.tab_mail)
    else -> Triple(Icons.Filled.Key, 0, Res.string.tab_pass)
}

/** Percent-encoding for a search in the map app. */
fun urlEncode(s: String): String = s.encodeToByteArray().joinToString("") { b ->
    val c = b.toInt() and 0xff
    if (c in 'a'.code..'z'.code || c in 'A'.code..'Z'.code || c in '0'.code..'9'.code || c == '-'.code || c == '.'.code || c == '_'.code) c.toChar().toString()
    else "%" + c.toString(16).uppercase().padStart(2, '0')
}

/** Opens what a link points to: the mail, the entry, the file. */
fun openLink(state: AppState, l: CalLinkUi) {
    when (l.kind) {
        "mail" -> {
            val p = l.target.split('|')
            if (p.size == 3) {
                state.stack.clear(); state.selectTab(Tab.MAIL)
                state.push(Screen.MailRead(p[0], p[1], p[2].toLongOrNull() ?: 0))
            }
        }
        "pass" -> {
            val p = l.target.split('|')
            if (p.size == 2) {
                state.stack.clear(); state.selectTab(Tab.PASS)
                state.push(Screen.EntryDetail(p[0], p[1]))
            }
        }
        else -> {
            state.stack.clear(); state.selectTab(Tab.DRIVE)
            state.scope.launch { state.message(getString(Res.string.cal_link_in_drive, l.label)) }
        }
    }
}

fun safeName(s: String): String = s.map { if (it.isLetterOrDigit() || it in " -_") it else '_' }.joinToString("").trim().ifEmpty { "termin" }

@Composable
fun ColorDot(c: Color, size: androidx.compose.ui.unit.Dp = 10.dp) {
    Box(Modifier.size(size).clip(CircleShape).background(c))
}

/** Calendar name with its colour dot (.tag.ev). */
@Composable
fun CalTag(name: String, color: Color) {
    Row(
        Modifier.height(24.dp).clip(RoundedCornerShape(8.dp)).background(color.copy(alpha = 0.15f)).padding(horizontal = 9.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        ColorDot(color)
        Text(name, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = g.fg, maxLines = 1)
    }
}

// --- edit --------------------------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
@Composable
fun CalEditScreen(state: AppState, start: CalEventUi, padding: PaddingValues) {
    val t = g
    val cal = state.backend.cal
    var d by remember { mutableStateOf(start) }
    var repSheet by remember { mutableStateOf(false) }
    var remSheet by remember { mutableStateOf(false) }
    var linkSheet by remember { mutableStateOf(false) }
    var datePick by remember { mutableStateOf<Int?>(null) }
    var timePick by remember { mutableStateOf<Int?>(null) }
    EnsureCalendars(state)
    val needTitle = stringResource(Res.string.cal_need_title)
    fun save() {
        if (d.title.isBlank()) {
            state.message(needTitle); return
        }
        val e = d
        state.run(block = { cal.saveEvent(e) }) {
            state.changed()
            state.calSel = Day.parse(e.start) ?: state.calSel
            state.pop()
            state.scope.launch {
                val r = e.reminders.minOrNull()
                state.message(if (r == null) getString(Res.string.cal_saved) else getString(Res.string.cal_saved_rem, reminderLabel(r, e.allDay)))
            }
        }
    }
    androidx.compose.foundation.layout.Column(Modifier.padding(padding)) {
        GhulTopBar(
            stringResource(if (start.id.isEmpty()) Res.string.cal_new_event else Res.string.cal_edit_event),
            onLock = { state.backend.lock() }, onBack = { state.pop() },
            actions = {
                HeaderButton(stringResource(Res.string.a_save), ::save)
            },
        )
        ScreenColumn {
            // Title (.field.big)
            Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface).padding(horizontal = 16.dp, vertical = 12.dp)) {
                val style = TextStyle(fontSize = 22.sp, fontWeight = FontWeight.Bold, color = t.fg)
                BasicTextField(d.title, { d = d.copy(title = it) }, Modifier.fillMaxWidth(), textStyle = style, singleLine = true, cursorBrush = SolidColor(t.accent),
                    decorationBox = { inner -> Box { if (d.title.isEmpty()) Text(stringResource(Res.string.cal_title), style = style.copy(color = t.faint)); inner() } })
            }
            // Calendars (.chip.cal-c)
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (c in state.calendars) {
                    val col = calColor(c.color)
                    val on = d.calendarId == c.id
                    Row(
                        Modifier.height(34.dp).clip(CircleShape).background(if (on) col.copy(alpha = 0.16f) else t.surface)
                            .border(BorderStroke(1.dp, if (on) col else t.line), CircleShape)
                            .clickable { d = d.copy(calendarId = c.id) }.padding(horizontal = 14.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        ColorDot(col)
                        Text(c.name, fontSize = 14.sp, fontWeight = FontWeight.W500, color = t.fg)
                    }
                }
            }
            CalCard {
                EditRow(stringResource(Res.string.cal_all_day)) {
                    Switch(d.allDay, { on ->
                        val day = Day.parse(d.start) ?: today()
                        val endDay = Day.parse(d.end) ?: day
                        d = if (on) d.copy(allDay = true, start = day.toString(), end = endDay.toString(), reminders = listOf(state.backend.settings.calAllDayReminder).filter { it >= 0 })
                        else d.copy(allDay = false, start = dateTime(day, 10 * 60), end = dateTime(day, 11 * 60), reminders = listOf(state.backend.settings.calDefaultReminder).filter { it >= 0 })
                    })
                }
                Divider16()
                EditRow(stringResource(Res.string.cal_begin)) { DateTimeBoxes(d.start, d.allDay, { datePick = 0 }, { timePick = 0 }) }
                Divider16()
                EditRow(stringResource(Res.string.cal_end)) { DateTimeBoxes(d.end, d.allDay, { datePick = 1 }, { timePick = 1 }) }
                if (!d.allDay) {
                    Divider16()
                    val tz = stringResource(Res.string.cal_tz_hint)
                    EditRow(stringResource(Res.string.cal_time_zone), onClick = { state.message(tz) }) {
                        Text(stringResource(Res.string.cal_tz_device), fontSize = 13.sp, color = t.faint)
                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = t.faint)
                    }
                }
            }
            CalCard {
                EditRow(stringResource(Res.string.cal_repeat), sub = if (d.id.isNotEmpty() && d.repeat != CalRepeatUi.NONE) stringResource(Res.string.cal_series_hint) else null, onClick = { repSheet = true }) {
                    Text(stringResource(REPEAT_LABELS.first { it.first == d.repeat }.second), fontSize = 13.sp, color = t.faint)
                    Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = t.faint)
                }
                Divider16()
                Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp)) {
                    Text(stringResource(Res.string.cal_reminders), fontWeight = FontWeight.W500, fontSize = 15.sp)
                    FlowRow(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        d.reminders.sortedDescending().forEach { m ->
                            SmallChip(reminderText(m, d.allDay), Icons.Outlined.Notifications) { d = d.copy(reminders = d.reminders - m) }
                        }
                        val cc = areaColor(Tab.CAL)
                        Row(
                            Modifier.height(30.dp).clip(CircleShape).background(cc.copy(alpha = 0.1f)).clickable { remSheet = true }.padding(horizontal = 12.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Icon(Icons.Filled.Add, null, Modifier.size(14.dp), tint = cc)
                            Text(stringResource(Res.string.cal_reminder), fontSize = 13.sp, fontWeight = FontWeight.W500, color = cc)
                        }
                    }
                }
            }
            CalCard {
                InlineField(Icons.Filled.Place, d.location, stringResource(Res.string.cal_add_place)) { d = d.copy(location = it) }
                Divider16()
                InlineField(Icons.Outlined.Description, d.notes, stringResource(Res.string.cal_note), lines = 3) { d = d.copy(notes = it) }
                Divider16()
                EditRow(stringResource(Res.string.cal_link_title), icon = Icons.Filled.Link, onClick = { linkSheet = true }) {
                    Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = t.faint)
                }
                if (d.links.isNotEmpty()) {
                    FlowRow(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        d.links.forEach { l -> SmallChip(l.label, linkLook(l.kind).first) { d = d.copy(links = d.links - l) } }
                    }
                }
            }
            HintText(stringResource(Res.string.cal_edit_hint))
        }
    }
    if (repSheet) {
        val close = { repSheet = false }
        GhulSheet(close, stringResource(Res.string.cal_repeat)) {
            SheetList {
                REPEAT_LABELS.forEachIndexed { i, (r, label) ->
                    SheetRow(stringResource(label), divider = i > 0, end = { if (d.repeat == r) Icon(Icons.Filled.Check, null, tint = t.muted) }) {
                        d = d.copy(repeat = r); close()
                    }
                }
            }
            HintText(stringResource(Res.string.cal_repeat_hint))
        }
    }
    if (remSheet) {
        val close = { remSheet = false }
        val opts = if (d.allDay) listOf(0, 1440, 2880, 10080) else listOf(0, 5, 10, 15, 30, 60, 120, 1440, 2880, 10080)
        GhulSheet(close, stringResource(Res.string.cal_reminder_add)) {
            SheetList {
                opts.forEachIndexed { i, m ->
                    SheetRow(reminderText(m, d.allDay), divider = i > 0, end = { if (m in d.reminders) Icon(Icons.Filled.Check, null, tint = t.muted) }) {
                        if (m !in d.reminders) d = d.copy(reminders = d.reminders + m)
                        close()
                    }
                }
            }
        }
    }
    if (linkSheet) LinkSheet(state, { linkSheet = false }) { l -> if (d.links.none { it.target == l.target && it.kind == l.kind }) d = d.copy(links = d.links + l) }
    datePick?.let { which ->
        val cur = Day.parse(if (which == 0) d.start else d.end) ?: today()
        val st = rememberDatePickerState(initialSelectedDateMillis = cur.days * 86_400_000L)
        DatePickerDialog(
            onDismissRequest = { datePick = null },
            confirmButton = {
                TextButton(onClick = {
                    st.selectedDateMillis?.let { ms -> d = withDate(d, which, Day.ofDays(ms / 86_400_000L)) }
                    datePick = null
                }) { Text(stringResource(Res.string.a_ok)) }
            },
            dismissButton = { TextButton(onClick = { datePick = null }) { Text(stringResource(Res.string.a_cancel)) } },
        ) { DatePicker(st) }
    }
    timePick?.let { which ->
        val cur = minutesOf(if (which == 0) d.start else d.end)
        val st = rememberTimePickerState(cur / 60, cur % 60, is24Hour = true)
        AlertDialog(
            onDismissRequest = { timePick = null },
            confirmButton = { TextButton(onClick = { d = withTime(d, which, st.hour * 60 + st.minute); timePick = null }) { Text(stringResource(Res.string.a_ok)) } },
            dismissButton = { TextButton(onClick = { timePick = null }) { Text(stringResource(Res.string.a_cancel)) } },
            text = { TimePicker(st) },
        )
    }
}

/** New start date keeps the length; a new end date before the start moves the start along. */
private fun withDate(e: CalEventUi, which: Int, day: Day): CalEventUi {
    if (which == 0) {
        val len = absMinutes(e.end) - absMinutes(e.start)
        val s = if (e.allDay) day.toString() else dateTime(day, minutesOf(e.start))
        val end = if (e.allDay) day.plus((len / 1440).toInt()).toString() else fromAbsMinutes(absMinutes(s) + len)
        return e.copy(start = s, end = end)
    }
    val end = if (e.allDay) day.toString() else dateTime(day, minutesOf(e.end))
    return if (absMinutes(end) < absMinutes(e.start)) withDate(e, 0, day) else e.copy(end = end)
}

private fun withTime(e: CalEventUi, which: Int, min: Int): CalEventUi {
    val day = Day.parse(if (which == 0) e.start else e.end) ?: today()
    if (which == 0) {
        val len = (absMinutes(e.end) - absMinutes(e.start)).coerceAtLeast(15)
        val s = dateTime(day, min)
        return e.copy(start = s, end = fromAbsMinutes(absMinutes(s) + len))
    }
    val end = dateTime(day, min)
    return if (absMinutes(end) <= absMinutes(e.start)) e.copy(end = fromAbsMinutes(absMinutes(e.start) + 60)) else e.copy(end = end)
}

@Composable
private fun Divider16() = Box(Modifier.fillMaxWidth().padding(horizontal = 16.dp).height(1.dp).background(g.line))

@Composable
private fun EditRow(
    label: String,
    sub: String? = null,
    icon: androidx.compose.ui.graphics.vector.ImageVector? = null,
    onClick: (() -> Unit)? = null,
    end: @Composable () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (icon != null) Icon(icon, null, Modifier.size(20.dp), tint = g.muted)
        Column(Modifier.weight(1f)) {
            Text(label, fontWeight = FontWeight.W500, fontSize = 15.sp, color = g.fg)
            if (sub != null) Text(sub, fontSize = 14.sp, color = g.muted)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) { end() }
    }
}

/** Date and time "inputs" (.dt input) that open the pickers. */
@Composable
private fun DateTimeBoxes(value: String, allDay: Boolean, onDate: () -> Unit, onTime: () -> Unit) {
    val day = Day.parse(value) ?: today()
    DtBox("${pad(day.d)}.${pad(day.m)}.${day.y}", onDate)
    if (!allDay) DtBox(hm(value), onTime)
}

@Composable
private fun DtBox(text: String, onClick: () -> Unit) {
    Box(
        Modifier.clip(RoundedCornerShape(10.dp)).background(g.surface2).border(BorderStroke(1.dp, g.line), RoundedCornerShape(10.dp))
            .clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 6.dp),
    ) { Text(text, fontSize = 14.sp, color = g.fg) }
}

@Composable
private fun InlineField(icon: androidx.compose.ui.graphics.vector.ImageVector, value: String, placeholder: String, lines: Int = 1, onChange: (String) -> Unit) {
    val t = g
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = if (lines > 1) Alignment.Top else Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(icon, null, Modifier.size(20.dp), tint = t.muted)
        val style = TextStyle(fontSize = 15.sp, color = t.fg)
        BasicTextField(value, onChange, Modifier.weight(1f).heightIn(min = if (lines > 1) 66.dp else 0.dp), textStyle = style, singleLine = lines == 1,
            cursorBrush = SolidColor(t.accent),
            decorationBox = { inner -> Box { if (value.isEmpty()) Text(placeholder, style = style.copy(color = t.faint)); inner() } })
    }
}

/** Chip with a remove button (.chip.sm). */
@Composable
private fun SmallChip(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector, onRemove: () -> Unit) {
    val t = g
    Row(
        Modifier.height(30.dp).clip(CircleShape).background(t.surface2).padding(start = 10.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Icon(icon, null, Modifier.size(14.dp), tint = t.fg)
        Text(text, fontSize = 13.sp, color = t.fg)
        Box(Modifier.size(22.dp).clip(CircleShape).clickable(onClick = onRemove), contentAlignment = Alignment.Center) {
            Icon(Icons.Filled.Close, stringResource(Res.string.a_remove), Modifier.size(14.dp), tint = t.muted)
        }
    }
}

/** Link to a GhulPass entry, a GhulDrive file or a GhulMail message. */
@Composable
private fun LinkSheet(state: AppState, close: () -> Unit, onPick: (CalLinkUi) -> Unit) {
    var kind by remember { mutableStateOf<String?>(null) }
    var items by remember { mutableStateOf<List<CalLinkUi>>(emptyList()) }
    var query by remember { mutableStateOf("") }
    LaunchedEffect(kind) {
        items = when (kind) {
            "pass" -> runCatching { state.backend.listEntries(null) }.getOrDefault(emptyList())
                .map { CalLinkUi("pass", it.title, "${it.vaultId}|${it.id}") }
            "drive" -> runCatching { state.backend.net.driveRecent(60) }.getOrDefault(emptyList()).filter { !it.isDir }
                .map { CalLinkUi("drive", it.name, it.id) }
            "mail" -> state.mailLoaded.map { CalLinkUi("mail", it.m.subject.ifBlank { it.m.from }, "${it.accountId}|${state.mailFolder}|${it.m.uid}") }
            else -> emptyList()
        }
    }
    GhulSheet(close, stringResource(Res.string.cal_link_title)) {
        if (kind == null) {
            SheetList {
                SheetRow(stringResource(Res.string.cal_link_drive), stringResource(Res.string.tab_drive), leading = { IconTile(Icons.Filled.Cloud, paletteColor(1)) }) { kind = "drive" }
                SheetRow(stringResource(Res.string.cal_link_mail), stringResource(Res.string.tab_mail), leading = { IconTile(Icons.Filled.Email, paletteColor(3)) }, divider = true) { kind = "mail" }
                SheetRow(stringResource(Res.string.cal_link_pass), stringResource(Res.string.tab_pass), leading = { IconTile(Icons.Filled.Key, paletteColor(0)) }, divider = true) { kind = "pass" }
            }
            HintText(stringResource(Res.string.cal_link_hint))
        } else {
            SearchField(query, { query = it })
            val shown = items.filter { query.isBlank() || it.label.contains(query, ignoreCase = true) }.take(40)
            if (shown.isEmpty()) HintText(stringResource(Res.string.cal_link_none))
            else SheetList {
                shown.forEachIndexed { i, l ->
                    SheetRow(l.label, divider = i > 0, leading = { IconTile(linkLook(l.kind).first, paletteColor(linkLook(l.kind).second)) }) { onPick(l); close() }
                }
            }
        }
    }
}

// --- quick add ---------------------------------------------------------------------------------

/** "+ Termin": type a sentence, see what was understood, save (or open all options). */
@Composable
fun CalQuickSheet(state: AppState, close: () -> Unit) {
    val t = g
    val cal = state.backend.cal
    val names = rememberCalNames()
    var text by remember { mutableStateOf("") }
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    LaunchedEffect(Unit) { kotlinx.coroutines.delay(250); runCatching { focus.requestFocus() } }
    val q = if (text.isBlank()) null else cal.quickParse(text, today().toString(), minutesOf(localNow()))
    val target = q?.let { calendarFor(state, it.calendarHint) }
    val calName = state.calendars.firstOrNull { it.id == target }
    fun draft(): CalEventUi {
        val p = q ?: return newEventDraft(state, state.calSel, null)
        val day = Day.parse(p.date) ?: today()
        val base = newEventDraft(state, day, null)
        val rem = if (p.allDay) state.backend.settings.calAllDayReminder else (calName?.defaultReminder ?: state.backend.settings.calDefaultReminder)
        return base.copy(
            title = p.title, calendarId = target ?: base.calendarId, allDay = p.allDay, repeat = p.repeat,
            start = if (p.allDay) day.toString() else "${day}T${p.start}", end = if (p.allDay) day.toString() else "${day}T${p.end}",
            reminders = if (rem >= 0) listOf(rem) else emptyList(),
        )
    }
    val needTitle = stringResource(Res.string.cal_need_title)
    fun saveNow() {
        val e = draft()
        if (e.title.isBlank()) { state.message(needTitle); return }
        close()
        state.run(block = { cal.saveEvent(e) }) {
            state.changed()
            val day = Day.parse(e.start) ?: today()
            state.calSel = day
            state.scope.launch { state.message(getString(Res.string.cal_added, names.short(day) + if (e.allDay) "" else ", " + hm(e.start))) }
        }
    }
    GhulSheet(close, stringResource(Res.string.cal_new_event)) {
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface2).padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(Res.string.cal_quick), fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = t.accent)
                val style = TextStyle(fontSize = 16.sp, color = t.fg)
                val ph = stringResource(Res.string.cal_quick_ph)
                BasicTextField(text, { text = it }, Modifier.fillMaxWidth().padding(top = 2.dp).focusRequester(focus),
                    textStyle = style, singleLine = true, cursorBrush = SolidColor(t.accent),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = androidx.compose.ui.text.input.ImeAction.Done),
                    keyboardActions = androidx.compose.foundation.text.KeyboardActions(onDone = { saveNow() }),
                    decorationBox = { inner -> Box { if (text.isEmpty()) Text(ph, style = style.copy(color = t.faint)); inner() } })
            }
            Icon(Icons.Filled.Bolt, null, Modifier.size(20.dp), tint = t.muted)
        }
        if (q == null) {
            HintText(stringResource(Res.string.cal_quick_hint))
        } else {
            val col = calColor(calName?.color ?: 0x2F74E8)
            val day = Day.parse(q.date) ?: today()
            Row(
                Modifier.fillMaxWidth().height(IntrinsicSize.Min).clip(RoundedCornerShape(16.dp)).background(t.surface2).padding(horizontal = 14.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Box(Modifier.width(4.dp).fillMaxHeight().clip(RoundedCornerShape(2.dp)).background(col))
                Column(Modifier.weight(1f)) {
                    Text(q.title.ifBlank { stringResource(Res.string.cal_untitled) }, fontWeight = FontWeight.Bold, color = t.fg)
                    Text(names.long(day), fontSize = 13.sp, color = t.muted)
                    Text(
                        (if (q.allDay) stringResource(Res.string.cal_all_day) else "${q.start} – ${q.end}") +
                            if (q.repeat != CalRepeatUi.NONE) " · " + stringResource(REPEAT_LABELS.first { it.first == q.repeat }.second) else "",
                        fontSize = 13.sp, color = t.muted,
                    )
                }
                CalTag(calName?.name ?: "", col)
            }
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (ex in stringResource(Res.string.cal_quick_examples).split('|')) {
                FilterChip(false, { text = ex }, { Text(ex) })
            }
        }
        ButtonRow {
            OutlinedButton(onClick = { close(); state.push(Screen.CalEdit(draft())) }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.cal_more_options)) }
            Button(onClick = { saveNow() }, modifier = Modifier.weight(1f)) {
                Icon(Icons.Filled.Check, null, Modifier.size(18.dp)); Text(stringResource(Res.string.cal_enter), Modifier.padding(start = 8.dp))
            }
        }
    }
}

/** Calendar named by `#hint` (prefix of its name), else the default one. */
fun calendarFor(state: AppState, hint: String): String {
    if (hint.isNotBlank()) state.calendars.firstOrNull { it.name.startsWith(hint, ignoreCase = true) }?.let { return it.id }
    return state.calDefaultCalendar
}

// --- calendars ---------------------------------------------------------------------------------

private val CAL_COLORS = listOf(0xD6336C, 0x2F74E8, 0x0E9F80, 0xE0700F, 0x8E5BD8, 0x1E9BB8, 0x4E8A2E, 0xB58600, 0xC0533A, 0x6F7187)

/** Sheet behind "Alle Kalender": visibility, colours, new calendar, .ics import and export. */
@Composable
fun CalendarsSheet(state: AppState, close: () -> Unit) {
    val t = g
    val cal = state.backend.cal
    val settings = state.backend.settings
    var editing by remember { mutableStateOf<CalCalendarUi?>(null) }
    var importInto by remember { mutableStateOf<String?>(null) }
    val save = rememberSaveFile { }
    val doneMsg = stringResource(Res.string.cal_import_done)
    val pick = rememberOpenFile { f ->
        val target = importInto ?: return@rememberOpenFile
        state.run(block = { cal.importIcs(target, f.bytes, utcOffsetMinutes()) }) { n ->
            state.changed(); state.message(doneMsg.replace("%1\$s", n.toString()))
        }
    }
    val e = editing
    if (e != null) {
        CalendarEditSheet(state, e) { editing = null }
        return
    }
    GhulSheet(close, stringResource(Res.string.cal_calendars)) {
        val hidden = settings.calHiddenSet()
        val year = remember(state.revision) { mutableStateOf<Map<String, Int>>(emptyMap()) }
        val lang = stringResource(Res.string.cal_lang)
        LaunchedEffect(state.revision) {
            year.value = runCatching { cal.occurrences(today().toString(), today().plus(365).toString(), "", lang) }.getOrDefault(emptyList())
                .groupingBy { it.calendarId }.eachCount()
        }
        SheetList {
            val nEvents = stringResource(Res.string.cal_n_events_year)
            state.calendars.forEachIndexed { i, c ->
                CalVisRow(c.name, nEvents.replace("%1\$s", (year.value[c.id] ?: 0).toString()), calColor(c.color), c.id !in hidden, i > 0,
                    onToggle = { settings.toggleCalHidden(c.id) }, onMore = { editing = c })
            }
            if (state.backend.settings.calRegion.isNotEmpty()) {
                CalVisRow(stringResource(Res.string.cal_holidays), stringResource(Res.string.cal_holidays_sub, state.calRegionName), calColor(0x6F7187),
                    settings.calHolidays && CalOccurrenceUi.HOLIDAYS !in hidden, true,
                    onToggle = { if (!settings.calHolidays) settings.calHolidays = true else settings.toggleCalHidden(CalOccurrenceUi.HOLIDAYS) }, onMore = null)
            }
            SheetRow(stringResource(Res.string.cal_new_calendar), leading = { IconTile(Icons.Filled.Add, paletteColor(5)) }, divider = true) {
                val vault = state.vaultsOf(de.danerd.ghulsuite.model.VaultKindUi.CAL).firstOrNull()?.id ?: return@SheetRow
                editing = CalCalendarUi("", vault, "", CAL_COLORS[state.calendars.size % CAL_COLORS.size], 15, 0)
            }
        }
        ButtonRow {
            OutlinedButton(onClick = {
                importInto = state.calDefaultCalendar.ifEmpty { null }
                if (importInto != null) pick()
            }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.cal_import)) }
            OutlinedButton(onClick = {
                val ids = state.calendars.map { it.id }.filter { it !in hidden }
                state.run(block = { cal.exportIcs(ids) }) { save("ghulcal.ics", it.encodeToByteArray()) }
            }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.cal_export)) }
        }
        HintText(stringResource(Res.string.cal_closed_hint))
    }
}

@Composable
private fun CalVisRow(name: String, sub: String, color: Color, on: Boolean, divider: Boolean, onToggle: () -> Unit, onMore: (() -> Unit)?) {
    val t = g
    if (divider) Box(Modifier.fillMaxWidth().padding(start = 56.dp, end = 16.dp).height(1.dp).background(t.line))
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(onClick = onToggle).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Box(
            Modifier.size(26.dp).clip(RoundedCornerShape(8.dp)).background(if (on) color else Color.Transparent).border(2.dp, color, RoundedCornerShape(8.dp)),
            contentAlignment = Alignment.Center,
        ) { if (on) Icon(Icons.Filled.Check, null, Modifier.size(16.dp), tint = Color.White) }
        Column(Modifier.weight(1f)) {
            Text(name, fontWeight = FontWeight.W500, fontSize = 15.sp, color = t.fg)
            Text(sub, fontSize = 14.sp, color = t.muted, maxLines = 1)
        }
        if (onMore != null) BarIcon(Icons.Filled.MoreVert, null, onClick = onMore)
    }
}

@Composable
private fun CalendarEditSheet(state: AppState, start: CalCalendarUi, close: () -> Unit) {
    val t = g
    val cal = state.backend.cal
    var c by remember { mutableStateOf(start) }
    var confirm by remember { mutableStateOf(false) }
    val save = rememberSaveFile { }
    val defaultName = stringResource(Res.string.cal_new_calendar)
    GhulSheet(close, stringResource(if (start.id.isEmpty()) Res.string.cal_new_calendar else Res.string.cal_edit_calendar)) {
        OutlinedTextField(c.name, { c = c.copy(name = it) }, label = { Text(stringResource(Res.string.cal_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        FlowRow12 {
            for (col in CAL_COLORS) {
                val cc = calColor(col)
                val on = c.color == col
                Box(
                    Modifier.size(42.dp).padding(5.dp).then(if (on) Modifier.border(2.dp, cc, CircleShape).padding(4.dp) else Modifier)
                        .clip(CircleShape).background(cc).clickable { c = c.copy(color = col) },
                )
            }
        }
        SheetList {
            ChoicePickRow(stringResource(Res.string.cal_default_reminder), c.defaultReminder) { c = c.copy(defaultReminder = it) }
            if (start.id.isNotEmpty()) {
                SheetRow(stringResource(Res.string.cal_export_one), divider = true) {
                    state.run(block = { cal.exportIcs(listOf(c.id)) }) { save("${safeName(c.name)}.ics", it.encodeToByteArray()) }
                }
                SheetRow(stringResource(Res.string.cal_delete_calendar), color = t.danger, divider = true) { confirm = true }
            }
        }
        Button(onClick = {
            val v = c.copy(name = c.name.ifBlank { defaultName })
            state.run(block = { cal.saveCalendar(v) }) { state.changed(); close() }
        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_done)) }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(stringResource(Res.string.cal_delete_calendar)) },
            text = { Text(stringResource(Res.string.cal_delete_calendar_text, c.name)) },
            confirmButton = {
                TextButton(onClick = {
                    confirm = false
                    state.run(block = { cal.deleteCalendar(c.id) }) { state.changed(); close() }
                }) { Text(stringResource(Res.string.a_delete)) }
            },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun FlowRow12(content: @Composable () -> Unit) {
    FlowRow(Modifier.fillMaxWidth().padding(vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) { content() }
}

/** Default reminder of a calendar: none, at start, 5 … 60 minutes, one day. */
@Composable
private fun ChoicePickRow(label: String, value: Int, onPick: (Int) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val opts = listOf(-1, 0, 5, 10, 15, 30, 60, 1440)
    SheetRow(label, if (value < 0) stringResource(Res.string.cal_no_reminder) else reminderText(value, false),
        end = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = g.faint) }) { open = true }
    if (open) {
        GhulSheet({ open = false }, label) {
            SheetList {
                opts.forEachIndexed { i, m ->
                    SheetRow(if (m < 0) stringResource(Res.string.cal_no_reminder) else reminderText(m, false), divider = i > 0,
                        end = { if (m == value) Icon(Icons.Filled.Check, null, tint = g.muted) }) { onPick(m); open = false }
                }
            }
        }
    }
}

// --- settings page -----------------------------------------------------------------------------

/** Options → GhulCal: view, holidays, reminders and lock-screen detail, data. */
@Composable
fun CalSettingsSection(state: AppState) {
    val settings = state.backend.settings
    val cal = state.backend.cal
    val lang = stringResource(Res.string.cal_lang)
    val regions = remember(lang) { runCatching { cal.regions(lang) }.getOrDefault(emptyList()) }
    var regionSheet by remember { mutableStateOf(false) }
    var calSheet by remember { mutableStateOf(false) }
    val testText = stringResource(Res.string.cal_test_title)
    SectionCard(stringResource(Res.string.set_view)) {
        ChoiceRow(stringResource(Res.string.cal_start_view),
            listOf(0 to Res.string.cal_view_month, 1 to Res.string.cal_view_week, 2 to Res.string.cal_view_day, 3 to Res.string.cal_view_list), settings.calStartView) { settings.calStartView = it }
        ChoiceRow(stringResource(Res.string.cal_week_starts), listOf(0 to Res.string.cal_monday, 6 to Res.string.cal_sunday), settings.calWeekStart) { settings.calWeekStart = it }
        SwitchRow(stringResource(Res.string.cal_show_week_numbers), settings.calWeekNumbers) { settings.calWeekNumbers = it }
        SwitchRow(stringResource(Res.string.cal_show_holidays), settings.calHolidays) { settings.calHolidays = it }
        Hint(stringResource(Res.string.cal_holidays_offline))
        Row(Modifier.fillMaxWidth().heightIn(min = 40.dp).clickable { regionSheet = true }, verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(Res.string.cal_region), Modifier.weight(1f), fontSize = 15.sp)
            Text(regions.firstOrNull { it.code == settings.calRegion }?.name ?: stringResource(Res.string.cal_region_none), fontSize = 14.sp, color = g.muted)
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = g.faint)
        }
    }
    SectionCard(stringResource(Res.string.cal_reminders)) {
        ChoiceRow(stringResource(Res.string.cal_default_timed),
            listOf(-1 to Res.string.cal_no_reminder, 0 to Res.string.cal_rem_start, 5 to Res.string.cal_rem_5, 10 to Res.string.cal_rem_10, 15 to Res.string.cal_rem_15,
                30 to Res.string.cal_rem_30, 60 to Res.string.cal_rem_60, 1440 to Res.string.cal_rem_day), settings.calDefaultReminder) { settings.calDefaultReminder = it }
        ChoiceRow(stringResource(Res.string.cal_default_allday),
            listOf(-1 to Res.string.cal_no_reminder, 0 to Res.string.cal_rem_day_9, 1440 to Res.string.cal_rem_daybefore_9), settings.calAllDayReminder) { settings.calAllDayReminder = it }
        SwitchRow(stringResource(Res.string.cal_sound), settings.calSound) { settings.calSound = it; state.refreshCalendar() }
        Text(stringResource(Res.string.cal_lock_content), fontSize = 15.sp)
        Text(stringResource(listOf(Res.string.cal_lock_min_sub, Res.string.cal_lock_title_sub, Res.string.cal_lock_full_sub)[settings.calNotifyDetail.coerceIn(0, 2)]),
            fontSize = 14.sp, color = g.muted)
        SingleChoiceSegmentedButtonRow {
            listOf(Res.string.cal_lock_min, Res.string.cal_lock_title, Res.string.cal_lock_full).forEachIndexed { i, r ->
                SegmentedButton(settings.calNotifyDetail == i, { settings.calNotifyDetail = i; state.refreshCalendar() }) { Text(stringResource(r)) }
            }
        }
        OutlinedButton(onClick = { state.calTestReminder(testText) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.cal_test))
        }
        Hint(stringResource(Res.string.cal_reminder_info))
    }
    SectionCard(stringResource(Res.string.cal_data)) {
        OutlinedButton(onClick = { calSheet = true }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.cal_manage, state.calendars.size.toString()))
        }
        Hint(stringResource(Res.string.cal_data_hint))
    }
    SectionCard(stringResource(Res.string.cal_closed_title)) {
        Text(stringResource(Res.string.cal_closed_text), fontSize = 14.sp, color = g.muted)
    }
    if (calSheet) CalendarsSheet(state) { calSheet = false }
    if (regionSheet) {
        val close = { regionSheet = false }
        GhulSheet(close, stringResource(Res.string.cal_region)) {
            SheetList {
                (listOf(de.danerd.ghulsuite.model.CalRegionUi("", stringResource(Res.string.cal_region_none))) + regions).forEachIndexed { i, r ->
                    SheetRow(r.name, divider = i > 0, end = { if (r.code == settings.calRegion) Icon(Icons.Filled.Check, null, tint = g.muted) }) {
                        settings.calRegion = r.code
                        settings.calHolidays = r.code.isNotEmpty()
                        state.refreshCalendar(); close()
                    }
                }
            }
        }
    }
}
