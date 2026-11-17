// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import de.danerd.ghulsuite.model.CalEventUi
import de.danerd.ghulsuite.ui.AppLaunch
import java.time.Instant
import java.time.ZoneId

/**
 * GhulSuite as the calendar app of the phone, and the place suite files open. Other apps hand over:
 * - `.ics` files → imported into the default calendar of GhulCal,
 * - "add to calendar" (title, time, place) → a new appointment in GhulCal,
 * - a suite file (`.ghlzip`) → opened like "Open suite file" (checked completely first).
 *
 * Invisible: reads what was handed over (while its read permission lasts), then opens
 * [MainActivity] in GhulSuite's own task. After unlocking the appointment is ready.
 */
class IntentEntryActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val intent = intent ?: return finish()
        Thread {
            runCatching { handle(intent) }
            runOnUiThread {
                startActivity(
                    Intent(this, MainActivity::class.java)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
                )
                finish()
            }
        }.start()
    }

    private fun handle(intent: Intent) {
        val data = intent.data
        when {
            intent.action == Intent.ACTION_INSERT || intent.action == Intent.ACTION_EDIT ->
                AppLaunch.newEvent.value = event(intent)
            // A suite file only from a content provider (a file:// path could point at any file of the app).
            intent.action == Intent.ACTION_VIEW && data != null && name(data).endsWith(".ghlzip", ignoreCase = true) ->
                if (data.scheme == "content") read(data, MAX_SUITE_BYTES)?.let { AppLaunch.suiteFile.value = suiteFile(data, it) }
            intent.action == Intent.ACTION_VIEW && data != null ->
                read(data, MAX_ICS_BYTES)?.let { AppLaunch.icsFile.value = it }
        }
    }

    /**
     * A suite file handed over. It is remembered for saving only where Android lets GhulSuite
     * keep the right to write it; otherwise "Save as" chooses the place (empty id).
     */
    private fun suiteFile(uri: Uri, data: ByteArray): de.danerd.ghulsuite.ui.PickedSuiteFile {
        val writable = runCatching {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        }.isSuccess
        val ref = de.danerd.ghulsuite.model.SuiteFileRef(if (writable) uri.toString() else "", name(uri))
        return de.danerd.ghulsuite.ui.PickedSuiteFile(ref, data)
    }

    /** "Add to calendar" from other apps (CalendarContract extras). */
    private fun event(intent: Intent): CalEventUi {
        val zone = ZoneId.systemDefault()
        fun at(ms: Long) = Instant.ofEpochMilli(ms).atZone(zone).toLocalDateTime().toString().take(16)
        val allDay = intent.getBooleanExtra("allDay", false)
        val begin = intent.getLongExtra("beginTime", -1L).takeIf { it > 0 } ?: (System.currentTimeMillis() / 3_600_000 + 1) * 3_600_000
        val end = intent.getLongExtra("endTime", -1L).takeIf { it > begin } ?: (begin + 3_600_000)
        return CalEventUi(
            title = intent.getStringExtra("title").orEmpty(),
            allDay = allDay,
            start = if (allDay) at(begin).take(10) else at(begin),
            end = if (allDay) at(end - 1).take(10) else at(end),
            location = intent.getStringExtra("eventLocation").orEmpty(),
            notes = intent.getStringExtra("description").orEmpty(),
        )
    }

    private fun name(uri: Uri): String =
        runCatching {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        }.getOrNull() ?: uri.lastPathSegment?.substringAfterLast('/') ?: "Datei"

    /**
     * Only files another app really hands over: `content://` of a provider that does not belong
     * to GhulSuite. A `file://` path or GhulSuite's own provider would let any app make GhulSuite
     * read its own private files ("confused deputy").
     */
    private fun foreign(uri: Uri): Boolean {
        if (uri.scheme != "content") return false
        val authority = uri.authority ?: return false
        if (authority == "$packageName.files") return false
        val owner = runCatching { packageManager.resolveContentProvider(authority, 0)?.packageName }.getOrNull()
        return owner != null && owner != packageName
    }

    /** The content of [uri], or null if it cannot be read, is not foreign or is larger than [limit]. */
    private fun read(uri: Uri, limit: Int): ByteArray? = if (!foreign(uri)) null else runCatching {
        contentResolver.openInputStream(uri)?.use { input ->
            val out = java.io.ByteArrayOutputStream()
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                out.write(buf, 0, n)
                if (out.size() > limit) return null
            }
            out.toByteArray()
        }
    }.getOrNull()

    companion object {
        private const val MAX_ICS_BYTES = 5 * 1024 * 1024
        /** A suite with attachments can be large, but not larger than memory allows. */
        private const val MAX_SUITE_BYTES = 512 * 1024 * 1024
    }
}
