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
import androidx.compose.material.icons.filled.Save
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
enum class JobKind { UPLOAD, DOWNLOAD, ADD, EXPORT, SAVE }

/** A job ends with this text (the message) as failed, e.g. "stopped after 12 of 40 files: …". */
class JobFailed(val text: String) : Exception(text)

/**
 * Something long that runs in the background (adding or saving files): in the app-wide
 * scope, so leaving a screen does not stop it. The strip in the header shows it with its progress
 * and "Cancel"; when it ends, a message says how it went.
 */
/**
 * [cancellable] false: saving the suite file. It has no "Cancel" and goes on when the suite is
 * locked (stopping midway would leave an incomplete file on a stick, see AppState.saveSuiteFile).
 */
class BgJob internal constructor(
    val id: Int,
    val kind: JobKind,
    val quiet: Boolean,
    val cancellable: Boolean = true,
    private val onChange: (BgJob) -> Unit,
) {
    /** When it last reported progress (the clock of [Jobs]); a job that stops reporting counts as stalled. */
    internal var lastProgress = 0L

    private fun changed() = onChange(this)

    private var titleState by mutableStateOf("")
    private var doneState by mutableStateOf(0)
    private var totalState by mutableStateOf(0)
    private var bytesState by mutableStateOf(0L)
    private var bytesTotalState by mutableStateOf(0L)
    private var itemState by mutableStateOf("")

    var title: String get() = titleState; set(v) { titleState = v; changed() }
    /** What kind of job it is, without names of files (M18: Android's notification of the running job). */
    internal var publicTitle = ""
    /** Items done (files). */
    var done: Int get() = doneState; set(v) { doneState = v; changed() }
    /** Items in all (0: one thing, or not known yet). */
    var total: Int get() = totalState; set(v) { totalState = v; changed() }
    /** Bytes done of the current item and its size (0: not known). */
    var bytes: Long get() = bytesState; set(v) { bytesState = v; changed() }
    var bytesTotal: Long get() = bytesTotalState; set(v) { bytesTotalState = v; changed() }
    /** Name of the item worked on now. */
    var item: String get() = itemState; set(v) { itemState = v; changed() }

    /** Saving (N3): the new file is in place, the one it replaced is overwritten with zeros now. */
    var wipingOld by mutableStateOf(false)

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

    fun info() = JobInfo(title, done, total, bytes, bytesTotal, item, progress, cancellable, publicTitle)
}

/** A job as the platform shows it outside the app's window (Android notification). */
data class JobInfo(
    val title: String, val done: Int, val total: Int, val bytes: Long, val bytesTotal: Long, val item: String, val progress: Float?,
    /** False: saving the suite file (no "Cancel"). */
    val cancellable: Boolean = true,
    /** The kind of job without any name ([title] may name a file), for notifications. */
    val publicTitle: String = "",
)

/**
 * How a job ended, for a system notification: [text] is the full message (it may name files),
 * [publicText] says only what kind of job ended and the counts (Windows Action Center, Android
 * lock screen).
 */
data class JobEnd(val text: String, val publicText: String)

/**
 * Background jobs as seen from outside the screens: the Android notification shows [current]
 * with "Cancel" ([cancelAll]); the automatic lock of Android goes through [autoLock] while
 * [current] is not null; a job that ended is announced in [finished] (a system notification while
 * the app is not in front).
 */
/**
 * Whether "back" on the first screen sends the app to the background instead of closing it: while
 * a job runs on Android 11 (API 30) or older, where closing the window would end the job. Newer
 * Android versions do that by themselves.
 */
fun backKeepsJobs(jobRunning: Boolean, sdk: Int): Boolean = jobRunning && sdk < 31

object JobStatus {
    /** The first running job; null while none runs. */
    val current = MutableStateFlow<JobInfo?>(null)

    /** Jobs that ended (not those stopped by locking). */
    val finished = MutableSharedFlow<JobEnd>(extraBufferCapacity = 8)

    /** Cancels every running job (the notification's "Cancel"). Set by the app state. */
    var cancelAll: () -> Unit = {}

    /** An automatic lock while jobs run (see AppState.autoLock); null: lock directly. */
    var autoLock: (() -> Unit)? = null

    /**
     * The suite file is being written (0 to 1; -1: how far is not known yet; null: not now). It
     * goes on after locking: Android keeps the app alive meanwhile (a notification without names).
     */
    val saving = MutableStateFlow<Float?>(null)

    /** The user is back: an automatic lock that waits for jobs is dropped. */
    var keepUnlocked: (() -> Unit)? = null
}

/** The jobs of the app: running ones and the last few that ended. */
class Jobs internal constructor(private val state: AppState) {
    val list = mutableStateListOf<BgJob>()
    private val lanes = mutableMapOf<String, Mutex>()
    private var nextId = 0

    /** Whether a job runs or waits. */
    val running: Boolean get() = list.any { it.active }

    /** An automatic lock came while jobs ran: it follows when the last one the user started ends. */
    var lockWhenDone = false

    /** A job the user started that reported no progress for this long counts as stalled. */
    var stallMillis = 120_000L

    private val started = kotlin.time.TimeSource.Monotonic.markNow()

    /** Milliseconds of a monotonic clock (tests may replace it). */
    var clock: () -> Long = { started.elapsedNow().inWholeMilliseconds }

    private var watchdog: kotlinx.coroutines.Job? = null

    /** A job the user started is running and reported progress lately. */
    private fun progressing(): Boolean = list.any { it.active && !it.quiet && clock() - it.lastProgress < stallMillis }

    /**
     * An automatic lock (time away, idle, screen off). Quiet jobs stop at once: they continue
     * from their marks next time and never hold the lock back. Jobs the user
     * started are waited for while they make progress; once none reported progress for
     * [stallMillis], the suite locks and cancels them.
     */
    fun autoLock() {
        for (j in list.filter { it.active && it.quiet }) {
            j.byLock = true
            j.coroutine?.cancel()
        }
        if (!progressing()) {
            lockWhenDone = false
            state.lockNow()
            return
        }
        lockWhenDone = true
        if (watchdog?.isActive == true) return
        watchdog = state.scope.launch {
            while (lockWhenDone) {
                kotlinx.coroutines.delay((stallMillis / 4).coerceIn(50L, 1_000L))
                if (lockWhenDone && !progressing()) {
                    lockWhenDone = false
                    state.lockNow()
                }
            }
        }
    }

    /** A job shows in the strip after this long (a job with nothing to do never flashes up). */
    var revealMillis = 500L

    /**
     * Starts [block] as a background job and returns at once. Jobs of the same [lane] run one after
     * the other (one transfer at a time), the others wait. [title] is looked up
     * first; [block] returns the message at the end (null: none), [cancelled] the message after
     * "Cancel" (null: none). [onEnd] runs in every case (e.g. closes the files). A [quiet] job is
     * shown in the strip but only says something when its [block] has something to say.
     */
    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    fun start(
        kind: JobKind,
        title: suspend () -> String,
        quiet: Boolean = false,
        lane: String = "files",
        cancellable: Boolean = true,
        cancelled: suspend (BgJob) -> String? = { null },
        onEnd: () -> Unit = {},
        block: suspend (BgJob) -> String?,
    ): BgJob {
        val job = BgJob(++nextId, kind, quiet, cancellable) { j ->
            j.lastProgress = clock()
            publish()
        }
        job.lastProgress = clock()
        list.add(job)
        // Older jobs that ended leave the list (only the last few are kept).
        list.filter { !it.active }.dropLast(4).forEach { list.remove(it) }
        // ATOMIC: a job cancelled before its coroutine had a turn still runs its end (else it
        // stayed "waiting" for good and held every automatic lock back).
        job.coroutine = state.scope.launch(start = kotlinx.coroutines.CoroutineStart.ATOMIC) {
            val reveal = launch {
                kotlinx.coroutines.delay(revealMillis)
                job.shown = true
                publish()
            }
            var message: String? = null
            try {
                // The name-free kind for notifications, on the side: the texts must not hold the job up.
                state.scope.launch {
                    job.publicTitle = catching { kindName(job.kind) }.getOrDefault("")
                    publish()
                }
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
        if (job.cancellable) job.coroutine?.cancel()
    }

    /**
     * Cancels every running job; [byLock]: the suite is being locked (the message waits for the
     * next unlock). Saving the suite file goes on (see [BgJob.cancellable]).
     */
    fun cancelAll(byLock: Boolean = false) {
        for (j in list.filter { it.active && it.cancellable }) {
            if (byLock) j.byLock = true
            j.coroutine?.cancel()
        }
    }

    private suspend fun ended(job: BgJob, message: String?) = withContext(NonCancellable) {
        publish()
        // The last job the user started ended while an automatic lock waited for it.
        val locking = lockWhenDone && list.none { it.active && !it.quiet }
        if (!message.isNullOrEmpty()) {
            when {
                // A quiet job that was stopped by locking says nothing (it continues next time).
                job.byLock -> if (!job.quiet) state.noteAfterUnlock(message)
                // The suite locks right now: the message (it may name a file) waits for the next
                // unlock instead of standing on the lock screen.
                locking -> state.noteAfterUnlock(message)
                else -> {
                    state.message(message)
                    JobStatus.finished.tryEmit(JobEnd(message, catching { publicText(job) }.getOrDefault("GhulSuite")))
                }
            }
        }
        if (locking) {
            lockWhenDone = false
            state.lockNow()
        }
    }

    private suspend fun kindName(kind: JobKind): String = state.str(
        when (kind) {
            JobKind.UPLOAD -> Res.string.job_kind_upload
            JobKind.DOWNLOAD -> Res.string.job_kind_download
            JobKind.ADD -> Res.string.job_kind_add
            JobKind.EXPORT -> Res.string.job_kind_export
            JobKind.SAVE -> Res.string.job_kind_save
        },
    )

    /** "Upload finished · 3 of 3 done": what ended and the counts, never a file name. */
    private suspend fun publicText(job: BgJob): String {
        val what = kindName(job.kind)
        val end = state.str(
            when (job.state) {
                JobState.CANCELLED -> Res.string.job_end_cancelled
                JobState.FAILED -> Res.string.job_end_failed
                else -> Res.string.job_end_done
            },
            what,
        )
        return if (job.total > 1) end + " · " + state.str(Res.string.job_of, job.done.toString(), job.total.toString()) else end
    }

    /**
     * Hands the first running job to the platform (notification, auto-lock). One not shown yet
     * counts as running, without a title (the notification stays as it is).
     */
    private fun publish() {
        val first = list.firstOrNull { it.active && it.shown } ?: list.firstOrNull { it.active }
        JobStatus.saving.value = list.firstOrNull { it.active && !it.cancellable }?.let { it.progress ?: -1f }
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
    val color = t.accent
    val icon = when (job.kind) {
        JobKind.UPLOAD, JobKind.ADD -> Icons.Filled.Upload
        JobKind.DOWNLOAD, JobKind.EXPORT -> Icons.Filled.Download
        JobKind.SAVE -> Icons.Filled.Save
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
            val p = job.progress.takeUnless { job.wipingOld }
            val bar = Modifier.fillMaxWidth().height(4.dp).clip(RoundedCornerShape(2.dp))
            if (p == null || job.state == JobState.WAITING) {
                LinearProgressIndicator(bar, color = color, trackColor = t.surface3)
            } else {
                Box(bar.background(t.surface3)) { Box(Modifier.fillMaxWidth(p).height(4.dp).clip(RoundedCornerShape(2.dp)).background(color)) }
            }
            Text(jobDetail(job), fontSize = 12.sp, color = t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (job.cancellable) {
            TextButton(onClick = { jobs.cancel(job) }, modifier = Modifier.testTag(JOB_CANCEL_TAG)) { Text(stringResource(Res.string.a_cancel)) }
        }
    }
}

/** "2 of 5 done · b.txt · 1.2 MB / 3 MB", or "Waiting …" behind another job. */
@Composable
private fun jobDetail(job: BgJob): String {
    if (job.state == JobState.WAITING) return stringResource(Res.string.job_waiting)
    // Saving: written and read back in one bar, so a percentage says more than the bytes.
    if (job.kind == JobKind.SAVE) {
        if (job.wipingOld) return stringResource(Res.string.save_wiping_old)
        val p = job.progress ?: return stringResource(Res.string.job_starting)
        return stringResource(Res.string.save_progress, "${(p * 100).toInt()} %")
    }
    return listOfNotNull(
        if (job.total > 1) stringResource(Res.string.job_of, job.done.toString(), job.total.toString()) else null,
        job.item.takeIf { it.isNotEmpty() },
        if (job.bytesTotal > 0) formatSize(job.bytes) + " / " + formatSize(job.bytesTotal) else null,
    ).joinToString(" · ").ifEmpty { stringResource(Res.string.job_starting) }
}
