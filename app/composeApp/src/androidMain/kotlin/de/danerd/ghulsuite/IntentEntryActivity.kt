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
 * - a suite file (`.ghlzip`) → opened like "Open suite file" (checked completely first),
 * - a GhulDocs package (`.ghldocs`) → opened on its own or added to GhulDocs, as the user chooses,
 * - "Save in GhulDocs" (one or several files of any type) → added to GhulDocs, encrypted, after
 *   unlocking and choosing the vault and folder.
 *
 * Invisible: reads what was handed over (while its read permission lasts), then opens
 * [MainActivity] in GhulSuite's own task. After unlocking the appointment is ready. A suite file is
 * not read here (it can be over a gigabyte): MainActivity gets the right to read it and reads it
 * as a stream when it is opened.
 */
class IntentEntryActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val intent = intent ?: return finish()
        // "Save in GhulDocs": the files stay where they are until GhulDocs adds them; the right to
        // read them goes along to GhulSuite's own window (it ends with this one otherwise).
        val toDocs = intent.component?.className?.endsWith(".ShareToDocsActivity") == true &&
            (intent.action == Intent.ACTION_SEND || intent.action == Intent.ACTION_SEND_MULTIPLE)
        // Only files GhulSuite was really allowed to read: passing on a right it does not hold
        // would make Android end GhulSuite (any app could crash it this way).
        val docsUris = if (!toDocs) emptyList() else streams(intent).filter {
            foreign(it) && checkUriPermission(it, android.os.Process.myPid(), android.os.Process.myUid(), Intent.FLAG_GRANT_READ_URI_PERMISSION) ==
                android.content.pm.PackageManager.PERMISSION_GRANTED
        }.distinct().take(MAX_SHARED_FILES)
        Thread {
            val suiteFile = if (toDocs) null else runCatching { handle(intent) }.getOrNull()
            if (docsUris.isNotEmpty()) {
                runCatching { AppLaunch.docsFiles.value = docsUris.map { de.danerd.ghulsuite.ui.contentSource(applicationContext, it) } }
            }
            runOnUiThread {
                val main = Intent(this, MainActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
                val grant = if (suiteFile != null) listOf(suiteFile) else docsUris
                if (grant.isNotEmpty()) {
                    // The read permission of this activity ends with it: handed on to MainActivity,
                    // which reads the files later (where Android kept no lasting permission).
                    val clip = android.content.ClipData.newRawUri("", grant[0])
                    grant.drop(1).forEach { clip.addItem(android.content.ClipData.Item(it)) }
                    main.clipData = clip
                    main.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                try {
                    startActivity(main)
                } catch (e: SecurityException) {
                    // The right could not be passed on after all: GhulSuite opens, the files stay out.
                    if (docsUris.isNotEmpty()) AppLaunch.docsFiles.value = null
                    main.clipData = null
                    main.flags = main.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION.inv()
                    startActivity(main)
                }
                finish()
            }
        }.start()
    }

    /** Takes over what was handed over; the suite file's address if it was one (to pass its read permission on). */
    private fun handle(intent: Intent): Uri? {
        val data = intent.data
        when {
            intent.action == Intent.ACTION_INSERT || intent.action == Intent.ACTION_EDIT ->
                AppLaunch.newEvent.value = event(intent)
            // A suite file or GhulDocs package only from a content provider (a file:// path could
            // point at any file of the app). Which of the two, its name says (see SuiteFileSection).
            intent.action == Intent.ACTION_VIEW && data != null && isSuiteOrDocsPackage(name(data)) ->
                if (foreign(data)) {
                    AppLaunch.suiteFile.value = suiteFile(data)
                    return data
                }
            intent.action == Intent.ACTION_VIEW && data != null ->
                read(data, MAX_ICS_BYTES)?.let { AppLaunch.icsFile.value = it }
        }
        return null
    }

    private fun isSuiteOrDocsPackage(name: String) =
        name.endsWith(".ghlzip", ignoreCase = true) || name.endsWith(".ghldocs", ignoreCase = true)

    /**
     * A suite file or GhulDocs package handed over. It is remembered for saving only where Android lets GhulSuite
     * keep the right to write it; otherwise "Save as" chooses the place (empty id).
     */
    private fun suiteFile(uri: Uri): de.danerd.ghulsuite.ui.PickedSuiteFile {
        val writable = runCatching {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        }.isSuccess
        val name = name(uri)
        val ref = de.danerd.ghulsuite.model.SuiteFileRef(if (writable) uri.toString() else "", name)
        return de.danerd.ghulsuite.ui.PickedSuiteFile(ref, de.danerd.ghulsuite.model.SuiteFileRef(uri.toString(), name))
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

    /**
     * The files handed over with "send" (one or several). Only addresses: anything else another
     * app puts there (a text, a list of something else) is left out instead of ending GhulSuite.
     */
    private fun streams(intent: Intent): List<Uri> = runCatching {
        @Suppress("DEPRECATION")
        when (intent.action) {
            Intent.ACTION_SEND_MULTIPLE -> intent.getParcelableArrayListExtra<android.os.Parcelable>(Intent.EXTRA_STREAM).orEmpty().filterIsInstance<Uri>()
            Intent.ACTION_SEND -> listOfNotNull(intent.getParcelableExtra<android.os.Parcelable>(Intent.EXTRA_STREAM) as? Uri)
            else -> emptyList()
        }
    }.getOrDefault(emptyList())

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
        /** Files handed over at once ("send" with many files): more are left out. */
        private const val MAX_SHARED_FILES = 100
    }
}
