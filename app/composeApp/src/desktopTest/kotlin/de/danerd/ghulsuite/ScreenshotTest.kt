// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.ImageComposeScene
import androidx.compose.ui.unit.Density
import androidx.compose.ui.use
import de.danerd.ghulsuite.model.EntrySummaryUi
import de.danerd.ghulsuite.model.EntryUi
import de.danerd.ghulsuite.model.FieldType
import de.danerd.ghulsuite.model.FieldUi
import de.danerd.ghulsuite.model.KdfChoice
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.PasswordOptionsUi
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.QuickUnlockReady
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.SlotType
import de.danerd.ghulsuite.model.SlotUi
import de.danerd.ghulsuite.model.TotpCodeUi
import de.danerd.ghulsuite.model.TotpInputUi
import de.danerd.ghulsuite.model.VaultBackend
import de.danerd.ghulsuite.model.ContainerInfoUi
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.OpenVaultUi
import de.danerd.ghulsuite.model.ProtectionUi
import de.danerd.ghulsuite.model.SetupUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.model.AutofillStatus
import de.danerd.ghulsuite.model.ImportPreviewUi
import de.danerd.ghulsuite.model.ImportResultUi
import de.danerd.ghulsuite.model.VaultSourceUi
import de.danerd.ghulsuite.model.FoundVaultUi
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.MailSheet
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.ui.Tab
import java.io.File
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import org.jetbrains.skia.EncodedImageFormat
import kotlin.test.Test
import de.danerd.ghulsuite.model.KeepSkippedUi

/** Renders the main screens with a fake backend to PNG files (build/screenshots) for review. */
class ScreenshotTest {
    internal class Fake(exists: Boolean, unlocked: Boolean) : VaultBackend {
        /** What the UI asked the backend to change (the UI tests check these). */
        val calls = java.util.Collections.synchronizedList(mutableListOf<String>())
        override val unlocked = MutableStateFlow(unlocked)
        override var net: de.danerd.ghulsuite.model.NetBackend = FakeNet()
        override fun deviceSshKey() = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDeviceKeyExample ghulsuite-device"
        override suspend fun fetchFromServer(input: de.danerd.ghulsuite.model.ServerInputUi, hostKey: String?) = ""
        private val existsFlag = exists
        var protection = ProtectionUi(securityKeys = 1, keyfiles = 0, recoveryCodes = 1)
        var vaults = listOf(
            VaultUi("p1", VaultKindUi.PASS, "Privat", "privat-p1.ghlpss", true, 4),
            VaultUi("p2", VaultKindUi.PASS, "Arbeit", "arbeit-p2.ghlpss", true, 1),
            VaultUi("t1", VaultKindUi.TOTP, "Persönlich", "personlich-t1.ghltotp", true, 2),
            VaultUi("c1", VaultKindUi.CAL, "Kalender", "kalender-c1.ghlcal", true, 21),
            // The areas with one file each: without them their tabs would not exist.
            VaultUi("d1", VaultKindUi.DRIVE, "Drive", "Drive.ghldrv", true, 0),
            VaultUi("m1", VaultKindUi.MAIL, "Mail", "Mail.ghlmail", true, 0),
            VaultUi("f1", VaultKindUi.PASS, "Verein", "verein.ghlpss", true, 3, outdated = true),
        )
        override val settings = Settings(object : KeyValueStore {
            private val map = mutableMapOf<String, Any>()
            override fun getInt(key: String, default: Int) = map[key] as? Int ?: default
            override fun putInt(key: String, value: Int) { map[key] = value }
            override fun getBoolean(key: String, default: Boolean) = map[key] as? Boolean ?: default
            override fun putBoolean(key: String, value: Boolean) { map[key] = value }
        })
        var quickReady = false
        override fun exists() = existsFlag
        override fun storageName() = "App-Speicher"
        override fun useStorage(folder: FolderRef?) {}
        override fun protection() = protection
        override suspend fun inspectFile(data: ByteArray) = ContainerInfoUi("pass", protection)
        override suspend fun calibrateKdf() = KdfChoice(256 * 1024, 3, 800)
        override suspend fun keyInfo() = KeyStatus("Yubico Security Key", true, true, 4)
        override suspend fun setKeyPin(newPin: String) {}
        override fun generateKeyfile() = ByteArray(46)
        override suspend fun create(setup: SetupUi, onTouchNeeded: () -> Unit) = ""
        override suspend fun createFromVault(open: OpenVaultUi, onTouchNeeded: () -> Unit): String? = null
        override suspend fun unlock(password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit) {}
        override suspend fun unlockWithRecoveryCode(code: String, newPassword: String) {}
        override fun lock() {}
        override fun panicLock() {}
        override fun biometricAvailable() = true
        override fun quickUnlockReady() = QuickUnlockReady(biometric = quickReady, pin = quickReady)
        override suspend fun enableBiometricUnlock() {}
        override suspend fun enablePinUnlock(pin: String) {}
        override fun disableQuickUnlock() {}
        override fun disableQuickUnlockMethod(bio: Boolean) {}
        override suspend fun unlockWithBiometric() {}
        override suspend fun unlockWithPin(pin: String) {}
        override val cal: de.danerd.ghulsuite.model.CalBackend = FakeCal()
        override suspend fun listVaults() = vaults
        override suspend fun createVault(kind: VaultKindUi, name: String) = "new"
        override suspend fun renameVault(id: String, name: String) {}
        override suspend fun removeVault(id: String) { calls += "removeVault:$id" }
        override suspend fun exportVault(id: String) = ByteArray(0)
        override suspend fun addVaultFile(data: ByteArray, name: String) = "x"
        override suspend fun adoptVault(
            source: VaultSourceUi, name: String, password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit,
        ) = "x"
        var found = emptyList<FoundVaultUi>()
        override suspend fun findNewVaults() = found
        override suspend fun ignoreFoundVault(file: String) {}
        override suspend fun acceptVaultState(id: String) {}
        override suspend fun listEntries(vaultId: String?) = listOf(
            EntrySummaryUi("p1", "Privat", "1", "login", "Codeberg", "ghul"),
            EntrySummaryUi("p2", "Arbeit", "2", "login", "Jira", "g.hul@firma.de"),
            EntrySummaryUi("p1", "Privat", "3", "card", "Visa", "G. Hul"),
            EntrySummaryUi("p1", "Privat", "4", "wifi", "WLAN Zuhause", "FritzBox 7590"),
            EntrySummaryUi("p1", "Privat", "5", "note", "Router-Notizen", ""),
        ).filter { vaultId == null || it.vaultId == vaultId }
        override suspend fun getEntry(vaultId: String, id: String) = EntryUi(
            "p1", "1", "login",
            listOf(
                FieldUi("title", FieldType.TEXT, "Codeberg"),
                FieldUi("username", FieldType.TEXT, "ghul"),
                FieldUi("password", FieldType.SECRET, "Kater-Ufer-Mondlicht-73"),
                FieldUi("urls", FieldType.LIST, "https://codeberg.org"),
            ),
        )
        override suspend fun saveEntry(entry: EntryUi) = "1"
        override suspend fun deleteEntry(vaultId: String, id: String) { calls += "deleteEntry:$vaultId:$id" }
        override suspend fun totpCodes(nowSeconds: Long) = listOf(
            TotpCodeUi("t1", "Persönlich", "a", "Codeberg", "ghul", "492039", "118273", 30, 17),
            TotpCodeUi("t1", "Persönlich", "b", "GitHub", "ghulaschsuppe", "771204", "009812", 30, 4),
        )
        override suspend fun addTotp(vaultId: String, input: TotpInputUi) {}
        override suspend fun parseOtpauth(uri: String) = TotpInputUi("", "", "")
        override suspend fun renameTotp(vaultId: String, id: String, issuer: String, account: String) {}
        override suspend fun deleteTotp(vaultId: String, id: String) { calls += "deleteTotp:$vaultId:$id" }
        override suspend fun slots() = listOf(
            SlotUi("k", SlotType.SECURITY_KEY, "Key am Schlüsselbund", 1_762_800_000_000),
            SlotUi("f", SlotType.KEYFILE, "USB-Stick", 1_762_800_000_000),
            SlotUi("c", SlotType.RECOVERY_CODE, "Wiederherstellungscode", 1_762_800_000_000),
        )
        override suspend fun addSecurityKey(pin: String, label: String, onTouchNeeded: () -> Unit) {}
        override suspend fun addKeyfile(keyfile: ByteArray, label: String) {}
        override suspend fun newRecoveryCode(label: String) = ""
        override suspend fun removeSlot(id: String) {}
        override suspend fun adoptRemoteProtection(password: String?, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit) {}
        override suspend fun changePassword(current: String, new: String) {}
        override fun generatePassword(options: PasswordOptionsUi) = "x7#Kq!9mVw2\$Lp4@Zr8&"
        override fun passwordScore(password: String) = if (password.length > 12) 4 else 1
        override fun copySensitive(label: String, text: String) {}
        override suspend fun previewImport(data: ByteArray, fileName: String) = ImportPreviewUi("Proton Pass", 42, 0, 5, 1)
        override suspend fun importData(
            data: ByteArray, fileName: String, passVault: String?, totpVault: String?, totpsFromLogins: Boolean, keep: KeepSkippedUi?,
        ) = ImportResultUi(42, 5, 0, 1)
        override suspend fun exportPlain(vaultId: String, password: String) = ByteArray(0)
        override fun autofillStatus() = AutofillStatus.OFF
        override fun openAutofillSettings() {}
        override fun printEmergencySheet(code: String, text: EmergencySheetText) {}
        override val appVersion = "0.5.0-dev"
        override val coreVersion = "ghul-core 0.5.0 (RP-ID ghulsuite.invalid)"
    }

    private fun render(name: String, w: Int = 412, h: Int = 892, content: @Composable () -> Unit) {
        val out = File("build/screenshots").apply { mkdirs() }
        ImageComposeScene(width = w * 2, height = h * 2, density = Density(2f), content = content).use { scene ->
            repeat(5) { scene.render(it * 100_000_000L) } // let LaunchedEffects run
            val image = scene.render(600_000_000L)
            File(out, "$name.png").writeBytes(image.encodeToData(EncodedImageFormat.PNG)!!.bytes)
        }
    }

    private fun state(exists: Boolean, unlocked: Boolean, setup: AppState.() -> Unit = {}): AppState =
        AppState(Fake(exists, unlocked), CoroutineScope(Dispatchers.Unconfined)).apply(setup)

    @Test
    fun screens() {
        Locale.setDefault(Locale.GERMAN)
        render("1-setup") { GhulSuiteApp(state(exists = false, unlocked = false)) }
        render("2-unlock") { GhulSuiteApp(state(exists = true, unlocked = false)) }
        render("3-pass") { GhulSuiteApp(state(exists = true, unlocked = true)) }
        render("4-entry") { GhulSuiteApp(state(true, true) { push(Screen.EntryDetail("p1", "1")) }) }
        render("5-edit") { GhulSuiteApp(state(true, true) { push(Screen.EntryEdit("p1", "1", "login")) }) }
        render("6-auth") { GhulSuiteApp(state(true, true) { tab = Tab.AUTH }) }
        render("7-settings") { GhulSuiteApp(state(true, true) { tab = Tab.SETTINGS }) }
        render("8-add-totp") { GhulSuiteApp(state(true, true) { push(Screen.AddTotp("t1")) }) }
        render("9-quick-unlock") {
            GhulSuiteApp(AppState(Fake(true, false).apply { quickReady = true }, CoroutineScope(Dispatchers.Unconfined)))
        }
        render("10-settings-dark") {
            val s = state(true, true) { tab = Tab.SETTINGS }
            s.backend.settings.theme = de.danerd.ghulsuite.model.ThemeMode.DARK
            GhulSuiteApp(s)
        }
        render("11-modules-auth-only") {
            val s = state(true, true)
            s.backend.settings.moduleGhulPass = false
            GhulSuiteApp(s)
        }
        render("13-unlock-keyfile") {
            GhulSuiteApp(AppState(Fake(true, false).apply { protection = ProtectionUi(0, 1, 1) }, CoroutineScope(Dispatchers.Unconfined)))
        }
        render("14-unlock-key-or-keyfile") {
            GhulSuiteApp(AppState(Fake(true, false).apply { protection = ProtectionUi(1, 1, 1) }, CoroutineScope(Dispatchers.Unconfined)))
        }
        render("15-pass-one-vault") { GhulSuiteApp(state(true, true) { passVault = "p2" }) }
        render("17-import") { GhulSuiteApp(state(true, true) { push(Screen.Import) }) }
        render("18-export") { GhulSuiteApp(state(true, true) { push(Screen.Export) }) }
        render("19-drive") { GhulSuiteApp(state(true, true) { tab = Tab.DRIVE }) }
        render("20-drive-setup") {
            GhulSuiteApp(AppState(Fake(true, true).apply { net = FakeNet(configured = false) }, CoroutineScope(Dispatchers.Unconfined)).apply { tab = Tab.DRIVE })
        }
        render("21-mail") { GhulSuiteApp(state(true, true) { tab = Tab.MAIL }) }
        render("22-mail-read") { GhulSuiteApp(state(true, true) { push(Screen.MailRead("m1", "INBOX", 2)) }) }
        render("23-mail-compose") {
            GhulSuiteApp(state(true, true) { push(Screen.MailCompose(de.danerd.ghulsuite.ui.MailDraft("m1", to = "erika@example.org", subject = "AW: Treffen am Freitag"))) })
        }
        render("24-mail-account") { GhulSuiteApp(state(true, true) { mailAccountSheet = MailSheet(null) }) }
        render("25-add-server") { GhulSuiteApp(state(true, true) { push(Screen.AddServer) }) }
        render("26-settings-servers") { GhulSuiteApp(state(true, true) { tab = Tab.SETTINGS }) }
        render("27-shares") { GhulSuiteApp(state(true, true) { push(Screen.Shares) }) }
        render("28-share-setup") { GhulSuiteApp(state(true, true) { push(Screen.ShareSetup("s1")) }) }
        render("29-settings-servers") { GhulSuiteApp(state(true, true) { push(Screen.SettingsPage(de.danerd.ghulsuite.ui.SetPage.SERVERS)) }) }
        render("30-settings-drive") { GhulSuiteApp(state(true, true) { push(Screen.SettingsPage(de.danerd.ghulsuite.ui.SetPage.DRIVE)) }) }
        render("31-mail-compose-attachment") {
            GhulSuiteApp(state(true, true) {
                push(Screen.MailCompose(de.danerd.ghulsuite.ui.MailDraft("m1", subject = "Steuer 2025.pdf",
                    attachments = listOf(de.danerd.ghulsuite.model.OutgoingAttachmentUi("Steuer 2025.pdf", "application/pdf", ByteArray(2_400))))))
            })
        }
        val files = listOf(de.danerd.ghulsuite.model.DriveItemUi("f1", null, "Steuer 2025.pdf", false, 2_400_000, 0))
        render("32-share-sheet-mail-on") {
            val s = state(true, true) { tab = Tab.DRIVE }
            GhulSuiteApp(s)
            de.danerd.ghulsuite.ui.ShareSheet(s, files, {}, {})
        }
        render("33-share-sheet-mail-off") {
            val s = state(true, true) { tab = Tab.DRIVE }
            s.backend.settings.moduleGhulMail = false
            GhulSuiteApp(s)
            de.danerd.ghulsuite.ui.ShareSheet(s, files, {}, {})
        }
        render("34-share-result") {
            val s = state(true, true) { tab = Tab.DRIVE }
            GhulSuiteApp(s)
            de.danerd.ghulsuite.ui.ShareResultDialog(s, kotlinx.coroutines.runBlocking { s.backend.net.shareList().first() }, true) {}
        }
        render("35-add-server-php") { GhulSuiteApp(state(true, true) { push(Screen.AddServer) }) }
        render("36-settings-mail") { GhulSuiteApp(state(true, true) { push(Screen.SettingsPage(de.danerd.ghulsuite.ui.SetPage.MAIL)) }) }
        render("37-mail-compose-unknown") {
            GhulSuiteApp(state(true, true) { push(Screen.MailCompose(de.danerd.ghulsuite.ui.MailDraft("m1", to = "neu@example.org", subject = "Hallo"))) })
        }
        render("40-new-entry-sheet") {
            val s = state(true, true)
            GhulSuiteApp(s)
            de.danerd.ghulsuite.ui.TypePickerDialog({}, {}, onGenerate = {})
        }
        render("41-generator") {
            val s = state(true, true)
            GhulSuiteApp(s)
            de.danerd.ghulsuite.ui.GeneratorDialog(s, {}, {})
        }
        render("42-mail-compose-new") { GhulSuiteApp(state(true, true) { push(Screen.MailCompose(de.danerd.ghulsuite.ui.MailDraft("m1"))) }) }
        render("60-cal-month") { GhulSuiteApp(state(true, true) { tab = Tab.CAL }) }
        render("61-cal-week") { GhulSuiteApp(state(true, true) { tab = Tab.CAL; calView = de.danerd.ghulsuite.ui.CalView.WEEK }) }
        render("62-cal-day") { GhulSuiteApp(state(true, true) { tab = Tab.CAL; calView = de.danerd.ghulsuite.ui.CalView.DAY }) }
        render("63-cal-list") { GhulSuiteApp(state(true, true) { tab = Tab.CAL; calView = de.danerd.ghulsuite.ui.CalView.LIST }) }
        render("64-cal-event") { GhulSuiteApp(state(true, true) { tab = Tab.CAL; push(Screen.CalEvent("2", de.danerd.ghulsuite.ui.today().plus(1).toString())) }) }
        render("65-cal-edit") { GhulSuiteApp(state(true, true) { tab = Tab.CAL; push(Screen.CalEdit(de.danerd.ghulsuite.model.CalEventUi("2", "priv", "Zahnarzt Dr. Weber", false,
            de.danerd.ghulsuite.ui.dateTime(de.danerd.ghulsuite.ui.today().plus(1), 600), de.danerd.ghulsuite.ui.dateTime(de.danerd.ghulsuite.ui.today().plus(1), 645),
            reminders = listOf(1440, 60), location = "Praxis am Markt, Hauptstraße 12", notes = "Überweisung und Versichertenkarte mitnehmen."))) }) }
        render("66-cal-quick") { GhulSuiteApp(state(true, true) { tab = Tab.CAL; calQuickOpen = true }) }
        render("67-cal-dark") {
            GhulSuiteApp(state(true, true) { tab = Tab.CAL; backend.settings.theme = de.danerd.ghulsuite.model.ThemeMode.DARK })
        }
        render("46-mail-html") { GhulSuiteApp(state(true, true) { tab = Tab.MAIL; push(Screen.MailRead("m1", "INBOX", 9)) }) }
        render("68-mail-archive-read") { GhulSuiteApp(state(true, true) { tab = Tab.MAIL; push(Screen.MailArchiveRead("m1", "INBOX", "k1")) }) }
        // The whole page, for the picture of the archive options at its end.
        render("69-set-mail-long", h = 2900) { GhulSuiteApp(state(true, true) { push(Screen.SettingsPage(de.danerd.ghulsuite.ui.SetPage.MAIL)) }) }
        render("43-drive-grid") { GhulSuiteApp(state(true, true) { tab = Tab.DRIVE; driveGrid = true }) }
        render("44-auth-grid") { GhulSuiteApp(state(true, true) { tab = Tab.AUTH; authGrid = true }) }
        render("45-pass-dark") {
            val s = state(true, true)
            s.backend.settings.theme = de.danerd.ghulsuite.model.ThemeMode.DARK
            GhulSuiteApp(s)
        }
        for (p in de.danerd.ghulsuite.ui.SetPage.entries) {
            render("50-set-" + p.name.lowercase()) { GhulSuiteApp(state(true, true) { push(Screen.SettingsPage(p)) }) }
        }
        render("12-recovery-code") {
            GhulSuiteApp(state(true, true) { push(Screen.RecoveryCode("D8PDS-YB7RF-3NA3V-N1716-HWNGZ-KEAP8-AGJGS-YH7FG-G6FYM-N98TM-Z5M98")) })
        }
    }

    /** Windows window (1280 × 820) and tablets in landscape: side navigation and two panes. */
    @Test
    fun wideScreens() {
        Locale.setDefault(Locale.GERMAN)
        fun wide(name: String, content: @Composable () -> Unit) = render("w-$name", 1280, 820, content)
        wide("1-unlock") { GhulSuiteApp(state(exists = true, unlocked = false)) }
        wide("2-pass") { GhulSuiteApp(state(true, true)) }
        wide("3-pass-entry") { GhulSuiteApp(state(true, true) { push(Screen.EntryDetail("p1", "1")) }) }
        wide("4-pass-edit") { GhulSuiteApp(state(true, true) { push(Screen.EntryDetail("p1", "1")); push(Screen.EntryEdit("p1", "1", "login")) }) }
        wide("5-auth") { GhulSuiteApp(state(true, true) { tab = Tab.AUTH }) }
        wide("6-auth-grid") { GhulSuiteApp(state(true, true) { tab = Tab.AUTH; authGrid = true }) }
        wide("7-drive") { GhulSuiteApp(state(true, true) { tab = Tab.DRIVE }) }
        wide("8-drive-grid") { GhulSuiteApp(state(true, true) { tab = Tab.DRIVE; driveGrid = true }) }
        wide("9-mail") { GhulSuiteApp(state(true, true) { tab = Tab.MAIL }) }
        wide("10-mail-read") { GhulSuiteApp(state(true, true) { tab = Tab.MAIL; push(Screen.MailRead("m1", "INBOX", 2)) }) }
        wide("11-mail-compose") { GhulSuiteApp(state(true, true) { tab = Tab.MAIL; push(Screen.MailCompose(de.danerd.ghulsuite.ui.MailDraft("m1"))) }) }
        wide("12-settings") { GhulSuiteApp(state(true, true) { tab = Tab.SETTINGS }) }
        wide("13-settings-page") { GhulSuiteApp(state(true, true) { tab = Tab.SETTINGS; push(Screen.SettingsPage(de.danerd.ghulsuite.ui.SetPage.MAIL)) }) }
        wide("14-new-entry-dialog") {
            GhulSuiteApp(state(true, true))
            CompositionLocalProvider(de.danerd.ghulsuite.ui.LocalWide provides true) {
                de.danerd.ghulsuite.ui.GhulTheme(de.danerd.ghulsuite.model.ThemeMode.LIGHT) {
                    de.danerd.ghulsuite.ui.TypePickerDialog({}, {}, onGenerate = {})
                }
            }
        }
        wide("15-pass-dark") {
            val s = state(true, true) { push(Screen.EntryDetail("p1", "1")) }
            s.backend.settings.theme = de.danerd.ghulsuite.model.ThemeMode.DARK
            GhulSuiteApp(s)
        }
        wide("16-setup") { GhulSuiteApp(state(exists = false, unlocked = false)) }
    }
}
