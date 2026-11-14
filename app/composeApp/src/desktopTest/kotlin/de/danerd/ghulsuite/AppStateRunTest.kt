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
import kotlin.test.assertTrue

/**
 * AppState.run, the one way the screens start an action, on a dispatcher like the real one (the
 * work is posted, not run at once; the UI tests use Unconfined, where these mistakes do not show).
 */
class AppStateRunTest {
    private fun state(): AppState {
        val ui = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        return AppState(ScreenshotTest.Fake(exists = true, unlocked = true), CoroutineScope(SupervisorJob() + ui))
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
}
