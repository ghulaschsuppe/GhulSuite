// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.JobKind
import de.danerd.ghulsuite.ui.JobState
import de.danerd.ghulsuite.ui.JobStatus
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.awaitCancellation
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotSame
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** The background jobs without screens: automatic locks, a job cancelled at once, the hooks. */
class JobsTest {
    private fun state(
        dispatcher: CoroutineDispatcher = Dispatchers.Unconfined,
        prepare: ScreenshotTest.Fake.() -> Unit = {},
    ): Pair<AppState, ScreenshotTest.Fake> {
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare)
        // Texts AppState looks up need a display, which the tests do not have: only that failure is let through.
        val headlessOnly = CoroutineExceptionHandler { _, e -> if (e !is java.awt.HeadlessException) throw e }
        return AppState(fake, CoroutineScope(SupervisorJob() + dispatcher + headlessOnly)) to fake
    }

    private fun waitUntil(millis: Long = 5_000, what: String = "", cond: () -> Boolean) {
        val end = System.currentTimeMillis() + millis
        while (!cond()) {
            check(System.currentTimeMillis() < end) { "timed out: $what" }
            Thread.sleep(20)
        }
    }

    /** "Back" on the first screen keeps a running job alive only where Android would end it. */
    @Test
    fun backOnTheFirstScreenKeepsJobsOnOldAndroid() {
        assertTrue(de.danerd.ghulsuite.ui.backKeepsJobs(jobRunning = true, sdk = 30))
        assertTrue(!de.danerd.ghulsuite.ui.backKeepsJobs(jobRunning = false, sdk = 30), "no job: closes as usual")
        assertTrue(!de.danerd.ghulsuite.ui.backKeepsJobs(jobRunning = true, sdk = 31), "Android 12+ keeps it anyway")
    }

    /** A quiet job (automatic archive) never holds the automatic lock back: it stops. */
    @Test
    fun quietJobDoesNotDelayTheAutoLock() {
        val (s, fake) = state()
        val job = s.jobs.start(JobKind.ARCHIVE, title = { "archive" }, quiet = true) { awaitCancellation() }
        assertTrue(job.active)
        s.autoLock()
        waitUntil(2_000, "locked at once") { fake.locks == 1 }
        waitUntil(what = "the quiet job stopped") { job.state == JobState.CANCELLED }
    }

    /**
     * A job the user started holds the automatic lock back while it reports progress; one that
     * stopped reporting (a stalled connection) is cancelled and the suite locks after the timeout.
     */
    @Test
    fun stalledJobLocksAfterTheTimeout() {
        val (s, fake) = state()
        s.jobs.stallMillis = 400
        // Progress every 100 ms for 1.5 s: the lock waits, then follows its end.
        val busy = s.jobs.start(JobKind.UPLOAD, title = { "busy" }) { j ->
            repeat(15) { kotlinx.coroutines.delay(100); j.done++ }
            null
        }
        s.autoLock()
        Thread.sleep(1_000)
        assertEquals(0, fake.locks, "not while it makes progress")
        waitUntil(what = "locked after the end") { fake.locks == 1 }
        assertEquals(JobState.DONE, busy.state)
        // No progress at all: locked (and cancelled) after the timeout, not before.
        val stalled = s.jobs.start(JobKind.UPLOAD, title = { "stalled" }) { awaitCancellation() }
        s.autoLock()
        Thread.sleep(150)
        assertEquals(1, fake.locks, "not at once")
        waitUntil(3_000, "locked after the timeout") { fake.locks == 2 }
        waitUntil(what = "the stalled job stopped") { stalled.state == JobState.CANCELLED }
    }

    /**
     * A job cancelled before its coroutine had a turn (desktop: Dispatchers.Main, not immediate)
     * still ends: it does not stay "waiting" and hold every later automatic lock back.
     */
    @Test
    fun jobCancelledBeforeItStartsEnds() {
        val exec = Executors.newSingleThreadExecutor()
        try {
            val (s, fake) = state(exec.asCoroutineDispatcher())
            // Let the work of unlocking settle, then keep the thread busy while the job is started.
            Thread.sleep(300)
            val gate = CountDownLatch(1)
            exec.submit { gate.await() }
            val job = s.jobs.start(JobKind.UPLOAD, title = { "upload" }) { awaitCancellation() }
            s.jobs.cancel(job)
            gate.countDown()
            waitUntil(what = "the job ended") { !s.jobs.running }
            assertEquals(JobState.CANCELLED, job.state)
            assertNull(JobStatus.current.value)
            s.autoLock()
            waitUntil(what = "the automatic lock") { fake.locks == 1 }
        } finally {
            exec.shutdownNow()
        }
    }

    /** Unlocking starts the automatic archive and takes the hooks of the notification. */
    @Test
    fun unlockingStartsTheArchiveAndTakesTheHooks() {
        val cancel = {}
        JobStatus.cancelAll = cancel
        val (app, _) = state(prepare = { (net as FakeNet).archiveAutoDaysValue = 1 })
        waitUntil(what = "archive job") { app.jobs.list.any { it.kind == JobKind.ARCHIVE } }
        assertNotSame(cancel, JobStatus.cancelAll)
        app.jobs.cancelAll()
    }
}
