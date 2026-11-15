// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.JobKind
import de.danerd.ghulsuite.ui.JobState
import de.danerd.ghulsuite.ui.JobStatus
import de.danerd.ghulsuite.ui.runAutoArchive
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
import kotlin.test.assertSame
import kotlin.test.assertTrue

/** The background jobs without screens: automatic locks, a job cancelled at once, the autofill state. */
class JobsTest {
    private fun state(
        dispatcher: CoroutineDispatcher = Dispatchers.Unconfined,
        main: Boolean = true,
        prepare: ScreenshotTest.Fake.() -> Unit = {},
    ): Pair<AppState, ScreenshotTest.Fake> {
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare)
        // Texts AppState looks up need a display, which the tests do not have: only that failure is let through.
        val headlessOnly = CoroutineExceptionHandler { _, e -> if (e !is java.awt.HeadlessException) throw e }
        return AppState(fake, CoroutineScope(SupervisorJob() + dispatcher + headlessOnly), main) to fake
    }

    private fun waitUntil(millis: Long = 5_000, what: String = "", cond: () -> Boolean) {
        val end = System.currentTimeMillis() + millis
        while (!cond()) {
            check(System.currentTimeMillis() < end) { "timed out: $what" }
            Thread.sleep(20)
        }
    }

    /** A quiet job (automatic archive, photo backup) never holds the automatic lock back: it stops. */
    @Test
    fun quietJobDoesNotDelayTheAutoLock() {
        val (s, fake) = state()
        val job = s.jobs.start(JobKind.PHOTOS, title = { "photos" }, quiet = true) { awaitCancellation() }
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

    /**
     * A second state (the autofill window) starts no background work after unlocking and leaves
     * the hooks of the notification to the app's own state.
     */
    @Test
    fun secondStateStartsNoBackgroundWork() {
        val prepare: ScreenshotTest.Fake.() -> Unit = {
            (net as FakeNet).archiveAutoDaysValue = 1
            (net as FakeNet).photos = listOf("a.jpg" to 100L)
            settings.photoBackup = true
        }
        val cancel = {}
        JobStatus.cancelAll = cancel
        JobStatus.autoLock = null
        val (other, _) = state(main = false, prepare = prepare)
        other.runAutoArchive()
        other.runPhotoBackup()
        Thread.sleep(500)
        assertTrue(other.jobs.list.isEmpty(), other.jobs.list.map { it.kind }.toString())
        assertSame(cancel, JobStatus.cancelAll)
        assertNull(JobStatus.autoLock)
        // The app's own state does both and takes the hooks.
        val (app, _) = state(prepare = prepare)
        waitUntil(what = "archive and photo jobs") {
            app.jobs.list.any { it.kind == JobKind.ARCHIVE } && app.jobs.list.any { it.kind == JobKind.PHOTOS }
        }
        assertNotSame(cancel, JobStatus.cancelAll)
        app.jobs.cancelAll()
    }
}
