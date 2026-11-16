// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.DevChecks
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * AppState.run, the one way the screens start an action, on a dispatcher like the real one (the
 * work is posted, not run at once; the UI tests use Unconfined, where these mistakes do not show).
 */
class AppStateRunTest {
    private fun state(prepare: ScreenshotTest.Fake.() -> Unit = {}): AppState {
        val ui = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        // Texts AppState looks up in the background need a display, which the tests do not have:
        // only that failure is let through (as in the other UI tests). Without this handler it
        // reached the next test class and failed it there.
        val headlessOnly = kotlinx.coroutines.CoroutineExceptionHandler { _, e -> if (e !is java.awt.HeadlessException) throw e }
        return AppState(ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare), CoroutineScope(SupervisorJob() + ui + headlessOnly))
    }

    /** Save tapped twice in a row: the action runs once (it ran twice: busy was set too late). */
    @Test
    fun aSecondTapWhileTheFirstStillRunsIsDropped() = runBlocking {
        DevChecks.enabled = true
        DevChecks.alarms.clear()
        val s = state()
        val runs = AtomicInteger()
        val release = CompletableDeferred<Unit>()
        val done = CompletableDeferred<Unit>()
        // Both taps arrive before the posted work had a turn.
        s.run(block = { runs.incrementAndGet(); release.await() }) { done.complete(Unit) }
        s.run(block = { runs.incrementAndGet() })
        release.complete(Unit)
        withTimeout(5_000) { done.await() }
        assertEquals(1, runs.get())
        // A second tap is no mistake of the app: no alarm.
        assertTrue(DevChecks.alarms.isEmpty(), DevChecks.alarms.toList().toString())
    }

    /**
     * Two calls of sync before the posted work had a turn (choosing the sync server did that):
     * one run covers both, never two runs at the same time ("syncing" was set only once the
     * coroutine ran, so the second call started a run of its own).
     */
    @Test
    fun twoSyncCallsNeverRunAtTheSameTime() = runBlocking {
        // No automatic run after unlocking in between: only the two calls below count.
        val s = state { settings.autoSync = false }
        val net = s.backend.net as FakeNet
        net.syncDelayMs = 300
        s.sync(quiet = true)
        s.sync(quiet = true)
        withTimeout(5_000) { while (net.syncRuns < 1 || s.syncing) kotlinx.coroutines.delay(20) }
        // Time for a second run, if one had been started.
        kotlinx.coroutines.delay(500)
        assertEquals(1, net.syncMaxAtOnce)
        assertEquals(1, net.syncRuns)
        assertFalse(s.syncing)
    }

    /**
     * A request that came while the run found no sync server yet (the server was chosen just then)
     * is not lost: the run looks again (it ended at once and dropped the request).
     */
    @Test
    fun aRequestDuringARunWithoutServerIsNotLost() = runBlocking {
        val s = state { settings.autoSync = false }
        val net = s.backend.net as FakeNet
        net.syncServer = null
        val gate = java.util.concurrent.CountDownLatch(1)
        net.syncStatusGate = gate
        s.sync(quiet = true)
        // The run has read "no server" and waits; now a server is chosen and a sync asked for.
        withTimeout(5_000) { while (net.syncStatusWaiting < 1) kotlinx.coroutines.delay(10) }
        net.syncServer = "s1"
        s.sync(quiet = true)
        net.syncStatusGate = null
        gate.countDown()
        withTimeout(5_000) { while (net.syncRuns < 1 || s.syncing) kotlinx.coroutines.delay(20) }
        assertEquals(1, net.syncRuns)
    }

    /**
     * Locked and unlocked again while a synchronisation runs: the run after unlocking stays
     * "syncing" until it ends (the old run's end said "not syncing" in the middle of it).
     */
    @Test
    fun anOldRunDoesNotEndTheNewOne() = runBlocking {
        val s = state { settings.autoSync = false }
        val net = s.backend.net as FakeNet
        val unlocked = (s.backend as ScreenshotTest.Fake).unlocked
        net.syncDelayMs = 1_500
        s.sync(quiet = true)
        withTimeout(5_000) { while (net.syncRuns < 1) kotlinx.coroutines.delay(10) }
        unlocked.value = false
        withTimeout(5_000) { while (s.syncing) kotlinx.coroutines.delay(10) }
        // Unlocked again a while later: the new run ends well after the old one.
        kotlinx.coroutines.delay(700)
        unlocked.value = true
        s.sync(quiet = true)
        withTimeout(5_000) { while (net.syncRuns < 2) kotlinx.coroutines.delay(10) }
        // The old run ends; the new one still runs.
        withTimeout(5_000) { while (net.syncDone < 1) kotlinx.coroutines.delay(10) }
        kotlinx.coroutines.delay(100)
        assertEquals(1, net.syncDone)
        assertTrue(s.syncing, "the new run is still running")
        withTimeout(5_000) { while (s.syncing) kotlinx.coroutines.delay(20) }
        assertEquals(2, net.syncDone)
    }

    /** A follow-up action started when the first one is done runs (F49: it was dropped). */
    @Test
    fun aFollowUpActionRuns() = runBlocking {
        DevChecks.enabled = true
        DevChecks.alarms.clear()
        val s = state()
        val second = CompletableDeferred<String>()
        s.run(block = { "first" }) { s.run(block = { "second" }) { second.complete(it) } }
        assertEquals("second", withTimeout(5_000) { second.await() })
        assertTrue(DevChecks.alarms.isEmpty(), DevChecks.alarms.toList().toString())
    }

    /** A programming error that reaches the screen is an alarm of the development build. */
    @Test
    fun anUnexpectedErrorIsAnAlarm() = runBlocking {
        DevChecks.enabled = true
        DevChecks.alarms.clear()
        val s = state()
        // (Without a display the error text itself cannot be loaded here; the alarm comes first.)
        s.run(onError = {}, block = { error("bug") })
        withTimeout(5_000) { while (DevChecks.alarms.isEmpty()) kotlinx.coroutines.delay(20) }
        assertTrue(DevChecks.alarms.any { "IllegalStateException" in it }, DevChecks.alarms.toList().toString())
        DevChecks.alarms.clear()
    }

    /** Only Compose's "outdated text layout" crash is dropped as a key; anything else stays a crash. */
    @Test
    fun staleTextLayoutIsRecognisedExactly() {
        assertTrue(de.danerd.ghulsuite.ui.isStaleTextLayoutError(IllegalArgumentException("offset(14) is out of bounds [0, 11]")))
        assertTrue(de.danerd.ghulsuite.ui.isStaleTextLayoutError(IllegalArgumentException("offset(14) is out of bounds [0, 11)")))
        assertFalse(de.danerd.ghulsuite.ui.isStaleTextLayoutError(IllegalArgumentException("index out of bounds")))
        assertFalse(de.danerd.ghulsuite.ui.isStaleTextLayoutError(IllegalStateException("offset(14) is out of bounds [0, 11]")))
        assertFalse(de.danerd.ghulsuite.ui.isStaleTextLayoutError(IllegalArgumentException("wrong offset(14) is out of bounds [0, 11]")))
    }
}
