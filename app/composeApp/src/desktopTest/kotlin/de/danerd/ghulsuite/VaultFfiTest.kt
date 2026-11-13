// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.coretest.CalCalendar
import de.danerd.ghulsuite.coretest.EntryData
import de.danerd.ghulsuite.coretest.FidoListener
import de.danerd.ghulsuite.coretest.FidoStatus
import de.danerd.ghulsuite.coretest.FieldData
import de.danerd.ghulsuite.coretest.FieldValue
import de.danerd.ghulsuite.coretest.GhulException
import de.danerd.ghulsuite.coretest.GhulSuite
import de.danerd.ghulsuite.coretest.NewVault
import de.danerd.ghulsuite.coretest.OtpAlgo
import de.danerd.ghulsuite.coretest.SetupOptions
import de.danerd.ghulsuite.coretest.SlotKind
import de.danerd.ghulsuite.coretest.TimeZones
import de.danerd.ghulsuite.coretest.TotpInput
import de.danerd.ghulsuite.coretest.VaultKind
import de.danerd.ghulsuite.coretest.generateKeyfile
import de.danerd.ghulsuite.coretest.localStorage
import de.danerd.ghulsuite.coretest.mockAuthenticator
import de.danerd.ghulsuite.coretest.parseOtpauthUri
import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/** The suite API exactly as the app uses it, against the software authenticator. */
class VaultFfiTest {
    private val listener = object : FidoListener {
        override fun onStatus(status: FidoStatus) {}
    }

    private fun suite(dir: File) =
        GhulSuite(localStorage(File(dir, "suite").absolutePath), File(dir, "local").absolutePath)

    private fun options(keyfile: ByteArray?) = SetupOptions(
        password = "Master Passwort!",
        keyfile = keyfile,
        keyLabel = "Key",
        keyfileLabel = "USB-Stick",
        recoveryLabel = "Code",
        memoryKib = 65536u,
        iterations = 3u,
        vaults = listOf(NewVault(VaultKind.PASS, "Privat"), NewVault(VaultKind.PASS, "Arbeit"), NewVault(VaultKind.TOTP, "2FA")),
    )

    private fun login(title: String) = EntryData(
        ByteArray(0), "login",
        listOf(
            FieldData("title", FieldValue.Text(title)),
            FieldData("password", FieldValue.Secret("geheim")),
            FieldData("urls", FieldValue.Items(listOf("https://example.org"))),
        ),
        emptyList(), null, 0, 0uL,
    )

    @Test
    fun securityKeySuiteWithSeveralVaults() {
        val dir = Files.createTempDirectory("ghul-suite").toFile()
        try {
            val key = mockAuthenticator("1234")
            val s = suite(dir)
            val setup = s.create(options(null), key, listener, "1234")
            assertEquals(11, setup.recoveryCode!!.split('-').size)
            val vaults = s.listVaults()
            assertEquals(3, vaults.size)
            val arbeit = vaults.first { it.name == "Arbeit" }.id
            val totp = vaults.first { it.kind == VaultKind.TOTP }.id
            val id = s.saveEntry(arbeit, login("Jira"))
            val t = parseOtpauthUri("otpauth://totp/Example:a?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8")
            s.addTotp(totp, TotpInput(t.issuer, t.account, t.secretBase32, OtpAlgo.SHA1, t.digits, t.period))
            assertEquals("94287082", s.totpCodes(59uL).single().code)

            s.lock()
            assertFailsWith<GhulException.Locked> { s.listVaults() }
            assertFailsWith<GhulException.SecurityKeyRequired> { s.unlock("Master Passwort!", null, null, null, null) }
            assertFailsWith<GhulException.WrongPassword> { s.unlock("falsch", key, listener, "1234", null) }
            s.unlock("Master Passwort!", key, listener, "1234", null)
            assertTrue(s.getEntry(arbeit, id).fields.contains(FieldData("password", FieldValue.Secret("geheim"))))
            assertEquals("Arbeit", s.listEntries(VaultKind.PASS, null).single().vaultName)

            // A key file as additional second factor; removing it later changes nothing else.
            val kf = generateKeyfile()
            s.addKeyfile(kf, "USB-Stick")
            s.lock()
            s.unlock("Master Passwort!", null, null, null, kf)
            val kfSlot = s.slots().first { it.kind == SlotKind.KEYFILE }
            s.removeSlot(kfSlot.slotId)
            val keySlot = s.slots().first { it.kind == SlotKind.SECURITY_KEY }
            assertFailsWith<GhulException.LastFactor> { s.removeSlot(keySlot.slotId) }
            s.lock()
            s.unlockWithRecoveryCode(setup.recoveryCode!!, "Neues Passwort!")
            assertEquals(1, s.totpCodes(59uL).size)
        } finally {
            dir.deleteRecursively()
        }
    }

    /** Times of an .ics file land on the right hour, with this device's own time zone database. */
    @Test
    fun calendarImportUsesTheTimeZonesOfTheDevice() {
        val dir = Files.createTempDirectory("ghul-zones").toFile()
        val before = java.util.TimeZone.getDefault()
        try {
            java.util.TimeZone.setDefault(java.util.TimeZone.getTimeZone("Europe/Berlin"))
            val zones = object : TimeZones {
                override fun offsetAtUtc(zone: String, utcMinutes: Long) = DeviceTimeZones.offsetAtUtc(zone, utcMinutes)
                override fun offsetAtLocal(zone: String, localMinutes: Long) = DeviceTimeZones.offsetAtLocal(zone, localMinutes)
            }
            assertEquals(120, zones.offsetAtUtc("", 29_727_360L)) // 2026-07-10T00:00Z
            assertEquals(60, zones.offsetAtUtc("Europe/Berlin", 29_979_360L)) // 2027-01-01T00:00Z
            assertEquals(-240, zones.offsetAtLocal("America/New_York", 29_727_360L))
            assertEquals(null, zones.offsetAtLocal("Mars/Olympus_Mons", 0L))
            assertEquals(null, zones.offsetAtLocal("../../etc/passwd", 0L))
            assertEquals(null, zones.offsetAtUtc("Europe/Berlin", Long.MAX_VALUE))

            val s = suite(dir)
            s.create(
                options(generateKeyfile()).copy(vaults = listOf(NewVault(VaultKind.CAL, "Kalender"))), null, null, null,
            )
            val vault = s.listVaults().first { it.name == "Kalender" }.id
            val cal = s.calSaveCalendar(CalCalendar("", vault, "Import", 1u, -1, 0u))
            fun event(n: Int, start: String) = "BEGIN:VEVENT\r\nUID:$n@x\r\nSUMMARY:T$n\r\n$start\r\nEND:VEVENT\r\n"
            val ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n" +
                event(1, "DTSTART:20260714T080000Z") +
                event(2, "DTSTART:20261214T080000Z") +
                event(3, "DTSTART;TZID=America/New_York:20260320T100000") +
                event(4, "DTSTART;TZID=Europe/Berlin:20260714T100000") +
                event(5, "DTSTART;TZID=Asia/Kolkata:20260714T100000") +
                event(6, "DTSTART;TZID=Nirgendwo/Nie:20260714T100000") +
                "END:VCALENDAR\r\n"
            assertEquals(6u, s.calImportIcs(cal, ics.toByteArray(), zones))
            val starts = s.calOccurrences("2026-01-01", "2027-01-01", "", "de")
                .filter { Regex("T\\d").matches(it.title) }.associate { it.title to it.start }
            assertEquals(
                mapOf(
                    "T1" to "2026-07-14T10:00", "T2" to "2026-12-14T09:00", "T3" to "2026-03-20T15:00",
                    "T4" to "2026-07-14T10:00", "T5" to "2026-07-14T06:30", "T6" to "2026-07-14T10:00",
                ),
                starts,
            )
        } finally {
            java.util.TimeZone.setDefault(before)
            dir.deleteRecursively()
        }
    }

    @Test
    fun keyfileSuiteThenKey() {
        val dir = Files.createTempDirectory("ghul-keyfile").toFile()
        try {
            val kf = generateKeyfile()
            val s = suite(dir)
            val setup = s.create(options(kf), null, null, null)
            assertEquals(0u, s.protection().securityKeys)
            assertEquals(1u, s.protection().keyfiles)
            s.lock()
            assertFailsWith<GhulException.KeyfileRequired> { s.unlock("Master Passwort!", null, null, null, null) }
            assertFailsWith<GhulException.WrongKeyfile> {
                s.unlock("Master Passwort!", null, null, null, generateKeyfile())
            }
            s.unlock("Master Passwort!", null, null, null, kf)
            val key = mockAuthenticator("1234")
            s.addSecurityKey(key, listener, "1234", "Key")
            s.lock()
            s.unlock("Master Passwort!", key, listener, "1234", null)
            s.lock()
            s.unlock("Master Passwort!", null, null, null, kf)
            s.lock()
            s.unlockWithRecoveryCode(setup.recoveryCode!!, "Noch ein Passwort")
        } finally {
            dir.deleteRecursively()
        }
    }
}
