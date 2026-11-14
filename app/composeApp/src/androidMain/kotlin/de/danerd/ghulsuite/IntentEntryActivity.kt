// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import de.danerd.ghulsuite.model.CalEventUi
import de.danerd.ghulsuite.model.OutgoingAttachmentUi
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.MailDraft
import java.time.Instant
import java.time.ZoneId

/**
 * GhulSuite as the mail and calendar app of the phone. Other apps hand over:
 * - `mailto:` links and "send by e-mail" (with attachments) → a new mail in GhulMail,
 * - `.ics` files → imported into the default calendar of GhulCal,
 * - "add to calendar" (title, time, place) → a new appointment in GhulCal.
 *
 * Invisible: reads what was handed over (attachments while their read permission lasts), then
 * opens [MainActivity] in GhulSuite's own task. After unlocking the mail or appointment is ready.
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
            data?.scheme.equals("mailto", ignoreCase = true) || intent.action == Intent.ACTION_SEND || intent.action == Intent.ACTION_SEND_MULTIPLE ->
                AppLaunch.compose.value = mailDraft(intent)
            intent.action == Intent.ACTION_INSERT || intent.action == Intent.ACTION_EDIT ->
                AppLaunch.newEvent.value = event(intent)
            intent.action == Intent.ACTION_VIEW && data != null ->
                read(data, MAX_ICS_BYTES)?.let { AppLaunch.icsFile.value = it }
        }
    }

    /** `mailto:a@b.de?cc=..&subject=..&body=..` plus the usual extras of "send". */
    private fun mailDraft(intent: Intent): MailDraft {
        val to = mutableListOf<String>()
        val cc = mutableListOf<String>()
        val bcc = mutableListOf<String>()
        var subject = intent.getStringExtra(Intent.EXTRA_SUBJECT).orEmpty()
        var body = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString().orEmpty()
        intent.data?.takeIf { it.scheme.equals("mailto", ignoreCase = true) }?.let { uri ->
            val raw = uri.toString().substringAfter(':')
            to += Uri.decode(raw.substringBefore('?')).split(',').map { it.trim() }.filter { it.isNotEmpty() }
            val query = raw.substringAfter('?', "")
            for (pair in query.split('&').filter { it.contains('=') }) {
                val value = Uri.decode(pair.substringAfter('='))
                when (pair.substringBefore('=').lowercase()) {
                    "to" -> to += value.split(',').map { it.trim() }.filter { it.isNotEmpty() }
                    "cc" -> cc += value.split(',').map { it.trim() }.filter { it.isNotEmpty() }
                    "bcc" -> bcc += value.split(',').map { it.trim() }.filter { it.isNotEmpty() }
                    "subject" -> subject = value
                    "body" -> body = value
                }
            }
        }
        intent.getStringArrayExtra(Intent.EXTRA_EMAIL)?.let { to += it }
        intent.getStringArrayExtra(Intent.EXTRA_CC)?.let { cc += it }
        intent.getStringArrayExtra(Intent.EXTRA_BCC)?.let { bcc += it }
        val streams = buildList {
            if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
                @Suppress("DEPRECATION")
                intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)?.let { addAll(it) }
            } else {
                @Suppress("DEPRECATION")
                (intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))?.let { add(it) }
            }
        }
        var left = MAX_ATTACHMENT_BYTES
        val attachments = streams.mapNotNull { uri ->
            val bytes = read(uri, left) ?: return@mapNotNull null
            left -= bytes.size
            OutgoingAttachmentUi(name(uri), contentResolver.getType(uri) ?: "application/octet-stream", bytes)
        }
        return MailDraft(
            accountId = null,
            to = to.distinct().joinToString(", "),
            cc = cc.distinct().joinToString(", "),
            bcc = bcc.distinct().joinToString(", "),
            subject = subject,
            text = body,
            attachments = attachments,
        )
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
        }.getOrNull() ?: uri.lastPathSegment?.substringAfterLast('/') ?: "Anhang"

    /**
     * Only files another app really hands over: `content://` of a provider that does not belong
     * to GhulSuite. A `file://` path or GhulSuite's own provider would let any app put GhulSuite's
     * private files (or decrypted Drive copies) into a ready-to-send mail ("confused deputy").
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
        private const val MAX_ATTACHMENT_BYTES = 25 * 1024 * 1024
        private const val MAX_ICS_BYTES = 5 * 1024 * 1024
    }
}
