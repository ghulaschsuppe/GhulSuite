// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

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
