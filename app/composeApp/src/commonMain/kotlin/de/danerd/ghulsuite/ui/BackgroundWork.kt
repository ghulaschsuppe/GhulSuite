// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.MailArchivedUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// The long work of the app, each as a background job (see [Jobs]): transfers of GhulDrive and the
// mail archive. Started from a screen, they go on when the screen is left.

/**
 * Uploads files into a GhulDrive folder, one after the other. A failed file does not stop the
 * others (the message names it); "Cancel" stops the rest. Every file is closed in the end.
 */
fun AppState.uploadToDrive(folder: String?, sources: List<ByteSource>) {
    if (sources.isEmpty()) return
    val net = backend.net
    val one = sources.singleOrNull()
    jobs.start(
        JobKind.UPLOAD,
        title = { if (one != null) str(Res.string.drive_uploading, one.name) else plural(Res.plurals.job_uploading_n, sources.size, sources.size.toString()) },
        cancelled = { j ->
            if (one != null) str(Res.string.drive_upload_cancelled_one, one.name)
            else plural(Res.plurals.drive_upload_cancelled_n, sources.size, j.done.toString(), sources.size.toString())
        },
        onEnd = { sources.forEach { runCatching { it.close() } } },
    ) { job ->
        job.total = sources.size
        var ok = 0
        val failed = mutableListOf<String>()
        var failure: String? = null
        for (s in sources) {
            job.item = s.name
            job.bytes = 0
            job.bytesTotal = s.size.coerceAtLeast(0)
            try {
                net.driveUpload(folder, s) { job.bytes = it }
                ok++
                // The list and the number of files follow after each file.
                driveTick++
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // One file alone: its error is the message.
                if (one != null) throw e
                failed += s.name
                failure = describe(e)
            } finally {
                runCatching { s.close() }
            }
            job.done++
        }
        if (one != null) return@start str(Res.string.drive_uploaded, one.name)
        // Every file that failed is named (with the reason of the last one).
        listOfNotNull(
            if (ok > 0) plural(Res.plurals.drive_uploaded_n, ok, ok.toString()) else null,
            failure?.let { failed.joinToString(", ") + ": " + it },
        ).joinToString(" ")
    }
}

/** Downloads a GhulDrive file into [sink] (a file the user chose). */
fun AppState.downloadFromDrive(item: DriveItemUi, sink: ByteSink) {
    jobs.start(
        JobKind.DOWNLOAD,
        title = { str(Res.string.drive_downloading, item.name) },
        cancelled = { str(Res.string.drive_download_cancelled, item.name) },
        onEnd = { runCatching { sink.close() } },
    ) { job ->
        job.item = item.name
        job.bytesTotal = item.size
        backend.net.driveDownload(item.id, sink) { job.bytes = it }
        str(Res.string.drive_downloaded, item.name)
    }
}

/** Counts of an archive run. */
private class ArchiveCount {
    var saved = 0
    var skipped = 0
    var gone = 0

    fun add(r: MailArchivedUi) {
        when (r) {
            MailArchivedUi.SAVED -> saved++
            MailArchivedUi.SKIPPED -> skipped++
            MailArchivedUi.GONE -> gone++
        }
    }
}

/** "40 mails archived in GhulDrive. 3 were already in the archive." */
private suspend fun AppState.archiveSummary(c: ArchiveCount): String = listOfNotNull(
    plural(Res.plurals.mail_archive_done, c.saved, c.saved.toString()),
    if (c.skipped > 0) plural(Res.plurals.mail_archive_skipped, c.skipped, c.skipped.toString()) else null,
    if (c.gone > 0) plural(Res.plurals.mail_archive_gone, c.gone, c.gone.toString()) else null,
).joinToString(" ")

/**
 * Copies mails into the mail archive in GhulDrive (`Mail-Archiv/<account>/<year>/`), one by one.
 * Nothing changes at the provider. [mails] finds the mails (account and UID), e.g. all of a folder;
 * mails already in the archive are skipped and counted.
 */
fun AppState.archiveMails(folder: String, mails: suspend () -> List<Pair<String, Long>>) {
    val net = backend.net
    val count = ArchiveCount()
    jobs.start(
        JobKind.ARCHIVE,
        title = { str(Res.string.mail_archiving) },
        cancelled = { j ->
            if (j.total == 0) str(Res.string.mail_archive_cancelled_early)
            else plural(Res.plurals.mail_archive_cancelled, j.total, j.done.toString(), j.total.toString())
        },
    ) { job ->
        val list = mails()
        job.total = list.size
        job.title = plural(Res.plurals.mail_archiving_n, list.size, list.size.toString())
        for ((account, uid) in list) {
            try {
                count.add(net.archiveToDrive(account, folder, uid))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Nothing done yet: the error alone; else how far it got.
                if (job.done == 0) throw e
                throw JobFailed(plural(Res.plurals.mail_archive_stopped, list.size, job.done.toString(), list.size.toString(), describe(e)))
            }
            job.done++
        }
        archiveTick++
        archiveSummary(count)
    }
}

/**
 * Automatic archiving after unlocking, at most once a day on this device: the mails of every
 * account's inbox and sent folder older than the chosen days. A quiet job: a message only when
 * something was archived. After each folder the core notes how far it got, so a cancelled run
 * continues there next time (mails already archived would be skipped anyway).
 */
fun AppState.runAutoArchive() {
    val st = backend.settings
    val net = backend.net
    if (!st.moduleGhulMail || jobs.list.any { it.active && it.kind == JobKind.ARCHIVE && it.quiet }) return
    if (nowSeconds() - (st.archiveAutoLast.toLongOrNull() ?: 0L) < 86_400) return
    scope.launch {
        val days = catching { net.archiveAutoDays() }.getOrDefault(0)
        if (days <= 0) return@launch
        // Without GhulDrive there is nowhere to archive to (the options say so).
        if (catching { net.driveStatus().serverId }.getOrNull() == null) return@launch
        val count = ArchiveCount()
        jobs.start(JobKind.ARCHIVE, title = { str(Res.string.mail_archive_auto_running) }, quiet = true) { job ->
            class Batch(val account: String, val folder: String, val validity: Long, val uids: List<Long>)
            val work = mutableListOf<Batch>()
            var trouble = false
            for (a in catching { net.mailAccounts() }.getOrDefault(emptyList())) {
                val folders = catching { net.mailFolders(a.id) }.getOrNull() ?: run { trouble = true; emptyList() }
                for (f in folders.filter { it.role == "inbox" || it.role == "sent" }) {
                    val b = catching { net.archiveAutoUids(a.id, f.name) }.getOrNull()
                    if (b == null) trouble = true else if (b.uids.isNotEmpty()) work += Batch(a.id, f.name, b.validity, b.uids)
                }
            }
            job.total = work.sumOf { it.uids.size }
            for (b in work) {
                var last = 0L
                try {
                    for (uid in b.uids) {
                        count.add(net.archiveToDrive(b.account, b.folder, uid))
                        last = uid
                        job.done++
                    }
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Offline or the folder changed: the next run tries again from here.
                    trouble = true
                } finally {
                    // Only up to the last mail handled (the UIDs come oldest first).
                    if (last > 0) withContext(NonCancellable) { catching { net.archiveAutoDone(b.account, b.folder, b.validity, last) } }
                }
            }
            if (!trouble) st.archiveAutoLast = nowSeconds().toString()
            if (count.saved > 0) {
                archiveTick++
                plural(Res.plurals.mail_archive_auto_done, count.saved, count.saved.toString())
            } else null
        }
    }
}

/** Collects a download in memory (an archived mail read from GhulDrive). */
class MemorySink : ByteSink {
    private val parts = mutableListOf<ByteArray>()

    override fun write(data: ByteArray) {
        parts += data.copyOf()
    }

    override fun close() {}

    fun bytes(): ByteArray {
        val out = ByteArray(parts.sumOf { it.size })
        var pos = 0
        for (p in parts) {
            p.copyInto(out, pos)
            pos += p.size
        }
        return out
    }
}
