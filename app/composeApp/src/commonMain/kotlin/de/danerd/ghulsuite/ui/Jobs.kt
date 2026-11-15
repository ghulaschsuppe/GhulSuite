// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Inventory2
import androidx.compose.material.icons.filled.PhotoLibrary
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource

/** Where a background job stands. Waiting: another job of its kind runs first. */
enum class JobState { WAITING, RUNNING, DONE, FAILED, CANCELLED }

/** What a job does (its picture in the strip and the notification). */
enum class JobKind { UPLOAD, DOWNLOAD, ARCHIVE, PHOTOS }

/** A job ends with this text (the message) as failed, e.g. "stopped after 12 of 40 mails: …". */
class JobFailed(val text: String) : Exception(text)

/**
 * Something long that runs in the background (upload, download, mail archive): in the app-wide
 * scope, so leaving a screen does not stop it. The strip in the header shows it with its progress
 * and "Cancel"; when it ends, a message says how it went.
 */
class BgJob internal constructor(val id: Int, val kind: JobKind, val quiet: Boolean, private val changed: () -> Unit) {
    private var titleState by mutableStateOf("")
    private var doneState by mutableStateOf(0)
    private var totalState by mutableStateOf(0)
    private var bytesState by mutableStateOf(0L)
    private var bytesTotalState by mutableStateOf(0L)
    private var itemState by mutableStateOf("")

    var title: String get() = titleState; set(v) { titleState = v; changed() }
    /** Items done (files, mails). */
    var done: Int get() = doneState; set(v) { doneState = v; changed() }
    /** Items in all (0: one thing, or not known yet). */
    var total: Int get() = totalState; set(v) { totalState = v; changed() }
    /** Bytes done of the current item and its size (0: not known). */
    var bytes: Long get() = bytesState; set(v) { bytesState = v; changed() }
    var bytesTotal: Long get() = bytesTotalState; set(v) { bytesTotalState = v; changed() }
    /** Name of the item worked on now. */
    var item: String get() = itemState; set(v) { itemState = v; changed() }

    var state by mutableStateOf(JobState.WAITING)
        internal set
    /** The message it ended with (empty: none). */
    var result by mutableStateOf("")
        internal set
    /** Shown in the strip: a job that ends within half a second (nothing to do) never flashes up. */
    var shown by mutableStateOf(false)
        internal set
    /** Cancelled because the suite was locked (its message waits for the next unlock). */
    var byLock = false
        internal set
    internal var coroutine: kotlinx.coroutines.Job? = null

    val active: Boolean get() = state == JobState.WAITING || state == JobState.RUNNING

    /** 0..1, or null while nothing can be said (the bar moves without an end). */
    val progress: Float?
        get() {
            val part = if (bytesTotal > 0) (bytes.toFloat() / bytesTotal).coerceIn(0f, 1f) else 0f
            return when {
                total > 0 -> ((done + part) / total).coerceIn(0f, 1f)
                bytesTotal > 0 -> part
                else -> null
            }
        }

    fun info() = JobInfo(title, done, total, bytes, bytesTotal, item, progress)
}

/** A job as the platform shows it outside the app's window (Android notification). */
data class JobInfo(
    val title: String, val done: Int, val total: Int, val bytes: Long, val bytesTotal: Long, val item: String, val progress: Float?,
)

/**
 * Background jobs as seen from outside the screens: the Android notification shows [current]
 * with "Cancel" ([cancelAll]); the auto-lock of Android waits while [current] is not null; a job
 * that ended is announced in [finished] (a system notification while the app is not in front).
 */
object JobStatus {
    /** The first running job; null while none runs. */
    val current = MutableStateFlow<JobInfo?>(null)

    /** The messages of jobs that ended (not those stopped by locking). */
    val finished = MutableSharedFlow<String>(extraBufferCapacity = 8)

    /** Cancels every running job (the notification's "Cancel"). Set by the app state. */
    var cancelAll: () -> Unit = {}
}

/** The jobs of the app: running ones and the last few that ended. */
class Jobs internal constructor(private val state: AppState) {
    val list = mutableStateListOf<BgJob>()
    private val lanes = mutableMapOf<String, Mutex>()
    private var nextId = 0

    /** Whether a job runs or waits. */
    val running: Boolean get() = list.any { it.active }

    /** An automatic lock came while jobs ran: it follows when the last one ends. */
    var lockWhenDone = false

    /** A job shows in the strip after this long (a job with nothing to do never flashes up). */
    var revealMillis = 500L

    /**
     * Starts [block] as a background job and returns at once. Jobs of the same [lane] run one after
     * the other (GhulDrive takes one transfer at a time), the others wait. [title] is looked up
     * first; [block] returns the message at the end (null: none), [cancelled] the message after
     * "Cancel" (null: none). [onEnd] runs in every case (e.g. closes the files). A [quiet] job is
     * shown in the strip but only says something when its [block] has something to say.
     */
    fun start(
        kind: JobKind,
        title: suspend () -> String,
        quiet: Boolean = false,
        lane: String = "drive",
        cancelled: suspend (BgJob) -> String? = { null },
        onEnd: () -> Unit = {},
        block: suspend (BgJob) -> String?,
    ): BgJob {
        val job = BgJob(++nextId, kind, quiet) { publish() }
        list.add(job)
        // Older jobs that ended leave the list (only the last few are kept).
        list.filter { !it.active }.dropLast(4).forEach { list.remove(it) }
        job.coroutine = state.scope.launch {
            val reveal = launch {
                kotlinx.coroutines.delay(revealMillis)
                job.shown = true
                publish()
            }
            var message: String? = null
            try {
                job.title = title()
                lanes.getOrPut(lane) { Mutex() }.withLock {
                    job.state = JobState.RUNNING
                    publish()
                    message = block(job)
                }
                job.state = JobState.DONE
            } catch (e: CancellationException) {
                job.state = JobState.CANCELLED
                message = withContext(NonCancellable) {
                    try {
                        val text = cancelled(job)
                        // Stopped by locking: the message (after the next unlock) says why.
                        if (job.byLock && text != null) state.str(Res.string.job_locked, text) else text
                    } catch (_: Exception) {
                        null
                    }
                }
            } catch (e: JobFailed) {
                job.state = JobState.FAILED
                message = e.text
            } catch (e: Exception) {
                job.state = JobState.FAILED
                message = withContext(NonCancellable) { describe(e) }
            } finally {
                reveal.cancel()
                runCatching { onEnd() }
                job.result = message.orEmpty()
                ended(job, message)
            }
        }
        publish()
        return job
    }

    fun cancel(job: BgJob) {
        job.coroutine?.cancel()
    }

    /** Cancels every running job; [byLock]: the suite is being locked (the message waits for the next unlock). */
    fun cancelAll(byLock: Boolean = false) {
        for (j in list.filter { it.active }) {
            if (byLock) j.byLock = true
            j.coroutine?.cancel()
        }
    }

    private fun ended(job: BgJob, message: String?) {
        publish()
        if (!message.isNullOrEmpty()) {
            if (job.byLock) {
                // A quiet job that was stopped by locking says nothing (it continues next time).
                if (!job.quiet) state.noteAfterUnlock(message)
            } else {
                state.message(message)
                JobStatus.finished.tryEmit(message)
            }
        }
        if (!running && lockWhenDone) {
            lockWhenDone = false
            state.lockNow()
        }
    }

    /**
     * Hands the first running job to the platform (notification, auto-lock). One not shown yet
     * counts as running, without a title (the notification stays as it is).
     */
    private fun publish() {
        val first = list.firstOrNull { it.active && it.shown } ?: list.firstOrNull { it.active }
        JobStatus.current.value = when {
            first == null -> null
            first.shown -> first.info()
            else -> JobInfo("", 0, 0, 0, 0, "", null)
        }
    }
}

/** Gives the header access to the jobs (the strip below every header). */
val LocalJobs = staticCompositionLocalOf<Jobs?> { null }

/** Test tag of the strip's "Cancel". */
const val JOB_CANCEL_TAG = "job-cancel"

/**
 * The running jobs under the header, wherever the user is: what runs, how far, and "Cancel".
 * Two at most; more are counted below them.
 */
@Composable
fun JobStrip() {
    val jobs = LocalJobs.current ?: return
    val shown = jobs.list.filter { it.active && it.shown }
    if (shown.isEmpty()) return
    Column(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (job in shown.take(2)) JobRow(jobs, job)
        if (shown.size > 2) {
            HintText(pluralStringResource(Res.plurals.job_more, shown.size - 2, (shown.size - 2).toString()))
        }
    }
}

@Composable
private fun JobRow(jobs: Jobs, job: BgJob) {
    val t = g
    val (icon, color) = when (job.kind) {
        JobKind.UPLOAD -> Icons.Filled.Upload to areaColor(Tab.DRIVE)
        JobKind.DOWNLOAD -> Icons.Filled.Download to areaColor(Tab.DRIVE)
        JobKind.ARCHIVE -> Icons.Filled.Inventory2 to areaColor(Tab.MAIL)
        JobKind.PHOTOS -> Icons.Filled.PhotoLibrary to areaColor(Tab.DRIVE)
    }
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface)
            .padding(start = 12.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        IconTile(icon, color, size = 34.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(job.title, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, color = t.fg)
            val p = job.progress
            val bar = Modifier.fillMaxWidth().height(4.dp).clip(RoundedCornerShape(2.dp))
            if (p == null || job.state == JobState.WAITING) {
                LinearProgressIndicator(bar, color = color, trackColor = t.surface3)
            } else {
                Box(bar.background(t.surface3)) { Box(Modifier.fillMaxWidth(p).height(4.dp).clip(RoundedCornerShape(2.dp)).background(color)) }
            }
            Text(jobDetail(job), fontSize = 12.sp, color = t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        TextButton(onClick = { jobs.cancel(job) }, modifier = Modifier.testTag(JOB_CANCEL_TAG)) { Text(stringResource(Res.string.a_cancel)) }
    }
}

/** "2 of 5 done · b.txt · 1.2 MB / 3 MB", or "Waiting …" behind another job. */
@Composable
private fun jobDetail(job: BgJob): String {
    if (job.state == JobState.WAITING) return stringResource(Res.string.job_waiting)
    return listOfNotNull(
        if (job.total > 1) stringResource(Res.string.job_of, job.done.toString(), job.total.toString()) else null,
        job.item.takeIf { it.isNotEmpty() },
        if (job.bytesTotal > 0) formatSize(job.bytes) + " / " + formatSize(job.bytesTotal) else null,
    ).joinToString(" · ").ifEmpty { stringResource(Res.string.job_starting) }
}
