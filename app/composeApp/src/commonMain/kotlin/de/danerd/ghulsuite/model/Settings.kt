// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlin.properties.ReadWriteProperty
import kotlin.reflect.KProperty

/** Persistent key-value storage of the platform (Android: SharedPreferences). Not for secrets. */
interface KeyValueStore {
    fun getInt(key: String, default: Int): Int
    fun putInt(key: String, value: Int)
    fun getBoolean(key: String, default: Boolean): Boolean
    fun putBoolean(key: String, value: Boolean)
    fun getString(key: String, default: String): String = default
    fun putString(key: String, value: String) {}
}

enum class ThemeMode { SYSTEM, LIGHT, DARK }

enum class QuickUnlockMode { OFF, BIOMETRIC, PIN }

/**
 * All user settings. Reading is observable by Compose; writing persists immediately.
 * Defaults favour security; the user can relax them deliberately.
 */
class Settings(private val store: KeyValueStore) {
    /**
     * Settings that belong to an area travel in the area's file (#24): every change goes to
     * [onAreaChange], values from the file come back through [applyArea]. The device keeps a copy
     * so the app shows them before the suite is unlocked.
     */
    var onAreaChange: ((VaultKindUi, String, String) -> Unit)? = null
    private val appliers = mutableMapOf<String, (String) -> Unit>()
    private val readers = mutableMapOf<String, () -> String>()

    private fun changed(key: String, value: String) {
        AREA_KEYS[key]?.let { kind -> onAreaChange?.invoke(kind, key, value) }
    }

    /** Takes over values from an area's file (without writing them back). */
    fun applyArea(values: Map<String, String>) {
        for ((k, v) in values) appliers[k]?.invoke(v)
    }

    /** This device's values of an area's settings (to put into the file once). */
    fun deviceValues(kind: VaultKindUi): Map<String, String> =
        AREA_KEYS.filterValues { it == kind }.keys.mapNotNull { k -> readers[k]?.let { k to it() } }.toMap()

    private fun int(key: String, default: Int) = object : ReadWriteProperty<Any?, Int> {
        private var state by mutableStateOf(store.getInt(key, default))
        init {
            appliers[key] = { v -> v.toIntOrNull()?.let { state = it; store.putInt(key, it) } }
            readers[key] = { state.toString() }
        }
        override fun getValue(thisRef: Any?, property: KProperty<*>) = state
        override fun setValue(thisRef: Any?, property: KProperty<*>, value: Int) {
            state = value
            store.putInt(key, value)
            changed(key, value.toString())
        }
    }

    private fun bool(key: String, default: Boolean) = object : ReadWriteProperty<Any?, Boolean> {
        private var state by mutableStateOf(store.getBoolean(key, default))
        init {
            appliers[key] = { v -> v.toBooleanStrictOrNull()?.let { state = it; store.putBoolean(key, it) } }
            readers[key] = { state.toString() }
        }
        override fun getValue(thisRef: Any?, property: KProperty<*>) = state
        override fun setValue(thisRef: Any?, property: KProperty<*>, value: Boolean) {
            state = value
            store.putBoolean(key, value)
            changed(key, value.toString())
        }
    }

    private fun str(key: String, default: String) = object : ReadWriteProperty<Any?, String> {
        private var state by mutableStateOf(store.getString(key, default))
        init {
            appliers[key] = { v -> state = v; store.putString(key, v) }
            readers[key] = { state }
        }
        override fun getValue(thisRef: Any?, property: KProperty<*>) = state
        override fun setValue(thisRef: Any?, property: KProperty<*>, value: String) {
            state = value
            store.putString(key, value)
            changed(key, value)
        }
    }

    companion object {
        /** Settings stored in an area's file (synchronised), by key. Everything else stays on the device. */
        val AREA_KEYS: Map<String, VaultKindUi> = mapOf(
            "generator_mode" to VaultKindUi.PASS,
            "generator_length" to VaultKindUi.PASS,
            "cal_holidays" to VaultKindUi.CAL,
            "cal_region" to VaultKindUi.CAL,
            "cal_default_reminder" to VaultKindUi.CAL,
            "cal_allday_reminder" to VaultKindUi.CAL,
            "cal_default" to VaultKindUi.CAL,
            "cal_week_start" to VaultKindUi.CAL,
        )
    }

    // --- Updates (only builds that update themselves, see AppUpdater)
    var updateAuto by bool("update_auto", true)
    /** Also development versions ("Vorab"). Starts on for a development version. */
    var updateDev by bool("update_dev", de.danerd.ghulsuite.BuildInfo.VERSION.contains('-'))
    /** Last automatic check (seconds since 1970, as text). */
    var updateLastCheck by str("update_last_check", "0")

    // --- Security
    /** Seconds in the background before locking; 0 = immediately. */
    var autoLockSeconds by int("auto_lock_seconds", 60)
    var lockOnScreenOff by bool("lock_on_screen_off", true)
    var shakeToLock by bool("shake_to_lock", true)
    /** 0 = low, 1 = medium, 2 = high sensitivity. */
    var shakeSensitivity by int("shake_sensitivity", 1)
    /** M12: the panic lock also deletes the prepared calendar reminders (they come back with the next unlock). */
    var panicClearReminders by bool("panic_clear_reminders", false)
    /** 0 = never clear. */
    var clipboardClearSeconds by int("clipboard_clear_seconds", 45)
    var allowScreenshots by bool("allow_screenshots", false)

    // --- Unlock
    var quickUnlockBiometric by bool("quick_bio", false)
    var quickUnlockPin by bool("quick_pin", false)
    /** How long a quick unlock stays valid after the last full unlock. */
    var quickUnlockHours by int("quick_hours", 24)
    /**
     * H6: the validity each method was set up with (it is fixed in its quick unlock file); -1:
     * not known (set up by an older version), then [quickUnlockHours].
     */
    var quickHoursBio by int("quick_hours_bio", -1)
    var quickHoursPin by int("quick_hours_pin", -1)

    // --- View
    var themeOrdinal by int("theme", ThemeMode.SYSTEM.ordinal)
    var theme: ThemeMode
        get() = ThemeMode.entries.getOrElse(themeOrdinal) { ThemeMode.SYSTEM }
        set(v) { themeOrdinal = v.ordinal }
    var moduleGhulPass by bool("module_pass", true)
    var moduleGhulAuth by bool("module_auth", true)
    var moduleGhulDocs by bool("module_docs", true)
    var moduleGhulCal by bool("module_cal", true)

    // --- GhulCal
    /** Start view: 0 month, 1 week, 2 day, 3 list. */
    var calStartView by int("cal_start_view", 0)
    /** 0 = weeks start on Monday, 6 = on Sunday. */
    var calWeekStart by int("cal_week_start", 0)
    var calWeekNumbers by bool("cal_week_numbers", true)
    var calHolidays by bool("cal_holidays", true)
    /** Region of the public holidays (e.g. DE-NW, AT, CH; empty = none). */
    var calRegion by str("cal_region", "DE-NW")
    /** Hidden calendars (ids, comma separated; `holidays` for the public holidays). */
    var calHidden by str("cal_hidden", "")
    /** Minutes before timed events (-1 none). */
    var calDefaultReminder by int("cal_default_reminder", 15)
    /** All-day events: minutes before 9:00 of the day (-1 none; 1440 = day before at 9:00). */
    var calAllDayReminder by int("cal_allday_reminder", 1440)
    /** Lock screen: 0 only "event in x min", 1 with title, 2 title, time and place. */
    var calNotifyDetail by int("cal_notify_detail", 1)
    var calSound by bool("cal_sound", true)
    /** Calendar of new events (empty = first). */
    var calDefault by str("cal_default", "")

    fun calHiddenSet(): Set<String> = calHidden.split(',').filter { it.isNotBlank() }.toSet()

    fun toggleCalHidden(id: String) {
        val s = calHiddenSet()
        calHidden = (if (id in s) s - id else s + id).joinToString(",")
    }

    // --- Lists
    var showEntrySubtitles by bool("show_subtitles", true)
    var totpHideCodes by bool("totp_hide", false)
    var totpShowNext by bool("totp_next", true)

    // --- GhulPass extras (device-local)
    /** Recently opened entries "vaultId/entryId", newest first (IDs only, no content). */
    var recentEntries by str("recent_entries", "")
    /** Breach check of passwords (k-anonymity, network); off by default. */
    var pwnedCheck by bool("pwned_check", false)
    /** Generator: 0 = characters, 1 = passphrase. */
    var generatorMode by int("generator_mode", 0)
    var generatorLength by int("generator_length", 20)

    // --- Start
    /** Area opened after unlocking: -1 = the one used last, else the tab index. */
    var startTab by int("start_tab", -1)
    /**
     * The second factor chosen last on this device (key or key file): the unlock and open screens
     * start with it, and always offer both (they do not tell what a file is protected with).
     */
    var factorKey by bool("factor_key", true)

    /** The area used last ([de.danerd.ghulsuite.ui.Tab] name; the old index "last_tab" is not read any more). */
    var lastTabName by str("last_tab_name", "")

    /** App language: "" follows the system, otherwise a language tag ("de", "en"). */
    var appLanguage by str("app_language", "")

    // --- GhulAuth
    /** 0 = by name, 1 = newest first, 2 = favourites first. */
    var authSort by int("auth_sort", 0)

    /**
     * The suite on this device was deleted: what names its vaults, entries or calendars, and its
     * quick unlock, goes. Settings of the device itself (look, locking, updates) stay.
     */
    fun forgetSuite() {
        recentEntries = ""
        calHidden = ""
        calDefault = ""
        quickUnlockBiometric = false
        quickUnlockPin = false
    }

    fun markRecent(vaultId: String, id: String) {
        val key = "$vaultId/$id"
        recentEntries = (listOf(key) + recentEntries.split(',').filter { it.isNotBlank() && it != key }).take(8).joinToString(",")
    }
}
