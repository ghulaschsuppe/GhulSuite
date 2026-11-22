// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.model.IDLE_MAX_MS
import de.danerd.ghulsuite.model.IDLE_MIN_MS
import de.danerd.ghulsuite.model.idleLockMillis
import de.danerd.ghulsuite.ui.RELEASE_NOTES_MAX
import de.danerd.ghulsuite.ui.releaseNotesShown
import de.danerd.ghulsuite.PinKeyLevel
import de.danerd.ghulsuite.pinKeyLevelAccepted
import de.danerd.ghulsuite.looksLikeEmulator
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** Small rules from round 9 (attackers' review). */
class R9SafetyTest {
    /** dieb N4/N5: idle in front locks after the auto-lock time, at least 5 and at most 15 minutes. */
    @Test
    fun theIdleLockIsBetweenFiveAndFifteenMinutes() {
        assertEquals(IDLE_MIN_MS, idleLockMillis(60))
        assertEquals(10 * 60_000L, idleLockMillis(600))
        assertEquals(IDLE_MAX_MS, idleLockMillis(86_400))
        assertEquals(15 * 60_000L, IDLE_MAX_MS)
    }

    /** dieb M2: the app refuses the PINs the core refuses (repeats, runs, dates) before asking it. */
    @Test
    fun easyAppPinsAreRefusedFirstHere() {
        for (p in listOf("11111111", "12121212", "12341234", "12345678", "87654321", "01234567", "24121990", "12241990", "19901224", "123123123123")) {
            assertTrue(de.danerd.ghulsuite.ui.appPinTooSimple(p), p)
        }
        for (p in listOf("24681357", "13579246", "11223344", "73920518", "32131990", "90876543")) {
            assertFalse(de.danerd.ghulsuite.ui.appPinTooSimple(p), p)
        }
    }

    /** r10 M3, r11 M2: unsaved input in an editor or a running action defer the idle lock, but not for good. */
    @Test
    fun theIdleLockWaitsForAnEditorWithInput() {
        val m = de.danerd.ghulsuite.model.idleLockMillis(60)
        val latest = de.danerd.ghulsuite.model.idleLockLatestMillis(60)
        assertTrue(de.danerd.ghulsuite.model.idleLockDue(m, 60, editorDirty = false, busy = false))
        assertFalse(de.danerd.ghulsuite.model.idleLockDue(IDLE_MIN_MS - 1, 60, editorDirty = false, busy = false))
        // r11 M2: unsaved input or a running action only defer it, with a warning a minute before.
        assertFalse(de.danerd.ghulsuite.model.idleLockDue(m, 60, editorDirty = true, busy = false))
        assertFalse(de.danerd.ghulsuite.model.idleLockDue(m, 60, editorDirty = false, busy = true))
        assertFalse(de.danerd.ghulsuite.model.idleLockWarning(m, 60, editorDirty = true))
        assertTrue(de.danerd.ghulsuite.model.idleLockWarning(latest - 30_000, 60, editorDirty = true))
        assertFalse(de.danerd.ghulsuite.model.idleLockWarning(latest - 30_000, 60, editorDirty = false))
        assertTrue(de.danerd.ghulsuite.model.idleLockDue(latest, 60, editorDirty = true, busy = false))
        assertTrue(de.danerd.ghulsuite.model.idleLockDue(latest, 60, editorDirty = false, busy = true))
        assertEquals(IDLE_MAX_MS, latest)
        assertEquals(2 * IDLE_MAX_MS, de.danerd.ghulsuite.model.idleLockLatestMillis(86_400))
        // Typing in a dialog or on the screen keyboard counts as input.
        de.danerd.ghulsuite.ui.UserActivity.touch()
        assertTrue((de.danerd.ghulsuite.ui.UserActivity.idleMillis() ?: Long.MAX_VALUE) < 1_000)
    }

    /** lieferkette N4: release notes are shown as plain, short text. */
    @Test
    fun releaseNotesArePlainAndShort() {
        val notes = "## What's Changed\n* Fix [login](https://evil.example/x) by @a\u202Ein\n\n\n* \u0007Next"
        val shown = releaseNotesShown(notes)
        assertEquals("## What's Changed\n* Fix login by @ain\n* Next", shown)
        assertTrue(releaseNotesShown("x".repeat(5000)).length <= RELEASE_NOTES_MAX + 2)
    }
}

/** r9 N6: the software keystore only for the development build on an emulator. */
class PinKeyLevelTest {
    @Test
    fun softwareKeysOnlyForTheDevBuildOnAnEmulator() {
        assertFalse(pinKeyLevelAccepted(PinKeyLevel.SOFTWARE, devBuild = false, emulator = true), "release on an emulator")
        assertFalse(pinKeyLevelAccepted(PinKeyLevel.SOFTWARE, devBuild = true, emulator = false), "dev build on a real phone")
        assertFalse(pinKeyLevelAccepted(PinKeyLevel.SOFTWARE, devBuild = false, emulator = false))
        assertTrue(pinKeyLevelAccepted(PinKeyLevel.SOFTWARE, devBuild = true, emulator = true))
        for (dev in listOf(true, false)) for (emu in listOf(true, false)) {
            assertTrue(pinKeyLevelAccepted(PinKeyLevel.TEE, dev, emu))
            assertTrue(pinKeyLevelAccepted(PinKeyLevel.STRONGBOX, dev, emu))
            assertFalse(pinKeyLevelAccepted(PinKeyLevel.OTHER, dev, emu))
        }
        assertTrue(looksLikeEmulator("google/sdk_gphone64_x86_64/emu64xa:14/UE1A/1:userdebug/dev-keys", "sdk_gphone64_x86_64", "", ""))
        assertTrue(looksLikeEmulator("x", "y", "1", ""))
        assertTrue(looksLikeEmulator("generic/sdk/generic:11", "sdk", "", ""))
        assertFalse(looksLikeEmulator("google/husky/husky:14/AP1A/1:user/release-keys", "husky", "", "0"))
    }
}
