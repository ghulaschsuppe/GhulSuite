// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

/**
 * What a new area's file holds from the start, written right when the area is made: this device's
 * values of the area's settings and, for GhulCal, the usual calendars. Done later (on the first
 * look at the area), these writes came after the suite file of a new suite and made the area
 * "unsaved" without the user changing anything.
 *
 * [calDefaults]: the names and colours of the usual calendars (in the language of the app).
 */
suspend fun prepareNewArea(backend: VaultBackend, kind: VaultKindUi, calDefaults: List<Pair<String, Int>>) {
    val values = backend.settings.deviceValues(kind)
    if (values.isNotEmpty()) backend.setAreaOptions(kind, values)
    if (kind == VaultKindUi.CAL) {
        val cal = backend.cal
        val vault = backend.listVaults().firstOrNull { it.kind == VaultKindUi.CAL && it.available } ?: return
        if (cal.calendars().isEmpty()) {
            for ((name, color) in calDefaults) cal.saveCalendar(CalCalendarUi("", vault.id, name, color, 15, 0))
        }
    }
}
