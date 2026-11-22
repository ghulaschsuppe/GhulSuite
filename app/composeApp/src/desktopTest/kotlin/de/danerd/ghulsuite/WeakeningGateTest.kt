// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.WEAKENING_VALID_MS
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Device test: one confirmation of the master password covers further weakening steps for two
 * minutes; a lock (and a restart: nothing is stored) asks again.
 */
class WeakeningGateTest {
    @Test
    fun oneConfirmationCoversTwoMinutesUntilALock() {
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true)
        val headlessOnly = CoroutineExceptionHandler { _, e -> if (e !is java.awt.HeadlessException) throw e }
        val s = AppState(fake, CoroutineScope(SupervisorJob() + Dispatchers.Unconfined + headlessOnly))
        var now = 1_000_000L
        s.gateClock = { now }
        assertFalse(s.weakeningStillConfirmed(), "nothing confirmed after the start")
        s.noteWeakeningConfirmed()
        now += WEAKENING_VALID_MS - 1
        assertTrue(s.weakeningStillConfirmed(), "a second step within two minutes does not ask")
        now += 1
        assertFalse(s.weakeningStillConfirmed(), "after two minutes it asks again")
        s.noteWeakeningConfirmed()
        assertTrue(s.weakeningStillConfirmed())
        // Review N5: a lock by hand ends it at once (not only through the flow).
        s.lockNow()
        assertFalse(s.weakeningStillConfirmed(), "after the lock button")
        s.noteWeakeningConfirmed()
        // Review M1: away from the app (screen off, another app) ends it too.
        de.danerd.ghulsuite.ui.AppLaunch.inFront.value = false
        assertFalse(s.weakeningStillConfirmed(), "after going to the background")
        de.danerd.ghulsuite.ui.AppLaunch.inFront.value = true
        s.noteWeakeningConfirmed()
        assertTrue(s.weakeningStillConfirmed())
        // Locked by the backend directly (auto-lock without jobs): at once, by its lock count.
        fake.lock()
        assertFalse(s.weakeningStillConfirmed(), "after a lock of the backend")
        s.noteWeakeningConfirmed()
        // Locked from anywhere (the backend says so).
        fake.unlocked.value = false
        val end = System.currentTimeMillis() + 5_000
        while (s.weakeningStillConfirmed() && System.currentTimeMillis() < end) Thread.sleep(20)
        assertFalse(s.weakeningStillConfirmed(), "after a lock it asks again")
    }
}
