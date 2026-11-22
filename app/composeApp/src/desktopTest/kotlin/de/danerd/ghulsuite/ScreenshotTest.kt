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
import de.danerd.ghulsuite.model.ImportPreviewUi
import de.danerd.ghulsuite.model.ImportResultUi
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.GhulSuiteApp
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
        override var updater: de.danerd.ghulsuite.model.AppUpdater = de.danerd.ghulsuite.model.NoUpdater
        private val existsFlag = exists
        var protection = ProtectionUi(securityKeys = 1, keyfiles = 0, recoveryCodes = 1)
        var vaults = listOf(
            VaultUi("p1", VaultKindUi.PASS, "Privat", "privat-p1.ghlpss", true, 4),
            VaultUi("p2", VaultKindUi.PASS, "Arbeit", "arbeit-p2.ghlpss", true, 1),
            VaultUi("t1", VaultKindUi.TOTP, "Persönlich", "personlich-t1.ghltotp", true, 2),
            VaultUi("d1", VaultKindUi.DOCS, "Dokumente", "d1.ghldoc", true, 8),
            VaultUi("c1", VaultKindUi.CAL, "Kalender", "kalender-c1.ghlcal", true, 21),
            VaultUi("f1", VaultKindUi.PASS, "Verein", "verein.ghlpss", true, 3),
        )
        override val settings = Settings(object : KeyValueStore {
            private val map = mutableMapOf<String, Any>()
            override fun getInt(key: String, default: Int) = map[key] as? Int ?: default
            override fun putInt(key: String, value: Int) { map[key] = value }
            override fun getBoolean(key: String, default: Boolean) = map[key] as? Boolean ?: default
            override fun putBoolean(key: String, value: Boolean) { map[key] = value }
        })
        var quickReady = false
        /** Whether a suite is on this device (deleting it or setting one up changes it). */
        var existsNow = existsFlag
        override fun exists() = existsNow
        /** Unlocking opens the suite (off: the screens stay as they are, as most tests expect). */
        var unlockOpens = false
        /** What saving the suite file or a GhulDocs package answers (false: not written completely). */
        var saveOk = true
        /** Saving fails with this (e.g. the stick is full). */
        var saveError: de.danerd.ghulsuite.model.VaultException? = null
        /** The place of the suite file this device remembers (null: none yet). */
        var suiteRef: de.danerd.ghulsuite.model.SuiteFileRef? = null
        override fun suiteFile() = suiteRef
        override fun setSuiteFile(ref: de.danerd.ghulsuite.model.SuiteFileRef?) { suiteRef = ref }
        /** r9 M-2: the places whose access was kept; [keepOk] false: the other app does not allow it. */
        val kept = mutableListOf<de.danerd.ghulsuite.model.SuiteFileRef>()
        var keepOk = true
        override fun keepAccess(ref: de.danerd.ghulsuite.model.SuiteFileRef): Boolean { kept += ref; return keepOk }
        /** What is at the place of the suite file (null: it cannot be reached). */
        var target: de.danerd.ghulsuite.model.ArchiveTargetUi? = de.danerd.ghulsuite.model.ArchiveTargetUi.NEW
        /** Who saved the file there, and when (shown before overwriting it). */
        var targetDevice = ""
        var targetTime = 0L
        override suspend fun archiveTargetInfo(file: de.danerd.ghulsuite.model.SuiteFileRef) =
            target?.let { de.danerd.ghulsuite.model.ArchiveTargetInfoUi(it, targetDevice, targetTime) }
        /** Since when changes are unsaved, when it was saved last (ms). */
        var unsavedSinceMs = 0L
        var savedAtMs = 0L
        override fun archiveUnsavedSince() = if (unsavedChanges) unsavedSinceMs.takeIf { it > 0 } ?: 1L else 0L
        override fun archiveSavedAt() = savedAtMs
        /** Saving waits here halfway (to lock meanwhile); null: it does not wait. */
        var saveGate: kotlinx.coroutines.CompletableDeferred<Unit>? = null
        override suspend fun archiveSave(target: de.danerd.ghulsuite.model.SuiteFileRef, device: String, created: Boolean, onProgress: (Long, Long) -> Unit): Boolean {
            kotlinx.coroutines.delay(30)
            calls += "archiveSave:${target.name}:$created"
            onProgress(50, 100)
            saveGate?.await()
            onProgress(100, 100)
            saveError?.let { throw it }
            // As the core: saved, nothing is unsaved any more.
            if (saveOk) unsavedChanges = false
            return saveOk
        }
        /** The next deletion fails with this (a key's second touch broke off), after locking. */
        var deleteFailsOnce: de.danerd.ghulsuite.model.VaultException? = null
        /** The recovery code of this suite (deleting or replacing with it, H5). */
        var recoveryCode = "AAAAA-BBBBB-CCCCC-DDDDD-EEEEE-FFFFF-GGGGG-HHHHH-JJJJJ-KKKKK-MMMMM"
        /** As the core: the proof of the suite here, both factors (the master password counts here) or its code. */
        private fun checkExisting(proof: de.danerd.ghulsuite.model.FactorProofUi?, code: String?) {
            if (code != null) {
                if (code.filter { it.isLetterOrDigit() }.uppercase() != recoveryCode.filter { it.isLetterOrDigit() }) {
                    throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.INVALID_RECOVERY_CODE)
                }
            } else if (proof?.pin?.isEmpty() == true) {
                // As a key: an empty PIN is a wrong one (and costs one of its tries).
                throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.PIN_INVALID, "", 7)
            } else if (proof == null || proof.password != masterPassword) {
                throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.WRONG_PASSWORD)
            } else if (proof.pin == null && proof.keyfile == null && !recovered) {
                throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.INVALID_INPUT, "a security key or key file is required")
            }
        }
        override suspend fun deleteSuite(proof: de.danerd.ghulsuite.model.FactorProofUi?, code: String?, onTouchNeeded: () -> Unit) {
            // As the real backend: locked first, then the proof, then the files go.
            kotlinx.coroutines.delay(30)
            unlocked.value = false
            deleteFailsOnce?.let { deleteFailsOnce = null; throw it }
            checkExisting(proof, code)
            calls += "deleteSuite"
            existsNow = false
        }
        override suspend fun archiveAcceptReplacing(proof: de.danerd.ghulsuite.model.FactorProofUi?, code: String?, onTouchNeeded: () -> Unit) {
            kotlinx.coroutines.delay(30)
            checkExisting(proof, code)
            calls += "archiveAcceptReplacing:" + (if (code != null) "code" else "proof")
        }
        /** Opened with the recovery code (H3): a change of the protection takes the password alone. */
        var recovered = false
        override fun openedWithRecoveryCode() = recovered
        /** The proofs quick unlock was set up with, and its validity (H6). */
        val quickSetups = java.util.Collections.synchronizedList(mutableListOf<String>())
        override fun protection() = protection
        override suspend fun totpUri(vaultId: String, id: String, password: String): String {
            kotlinx.coroutines.delay(30)
            if (password != masterPassword) throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.WRONG_PASSWORD)
            calls += "totpUri:$id"
            return "otpauth://totp/Codeberg:ghul?secret=JBSWY3DPEHPK3PXP&issuer=Codeberg"
        }
        /** How saving replaces the file at a place (M1). */
        var safe = de.danerd.ghulsuite.model.SafeSaveUi.SAFE
        override fun safeSave(ref: de.danerd.ghulsuite.model.SuiteFileRef) = safe
        /** The master password [verifyPassword] accepts (as the core: it takes a moment). */
        var masterPassword = "Sicheres-Passwort-1"
        override suspend fun verifyPassword(password: String): Boolean {
            kotlinx.coroutines.delay(30)
            calls += "verifyPassword"
            return password == masterPassword
        }
        override suspend fun inspectFile(data: ByteArray) = ContainerInfoUi("pass", protection)
        override suspend fun calibrateKdf() = KdfChoice(256 * 1024, 3, 800)
        override suspend fun keyInfo() = KeyStatus("Yubico Security Key", true, true, 4)
        override suspend fun setKeyPin(newPin: String) {}
        override fun generateKeyfile() = ByteArray(46)
        override suspend fun create(setup: SetupUi, onTouchNeeded: () -> Unit): String {
            kotlinx.coroutines.delay(30)
            // As the core: never over an existing suite.
            if (existsNow) throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.ALREADY_EXISTS)
            calls += "create"
            existsNow = true
            // As the core: the new suite is open.
            unlocked.value = true
            return ""
        }
        override suspend fun createFromVault(open: OpenVaultUi, onTouchNeeded: () -> Unit): String? = null
        override suspend fun unlock(password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit) {
            if (!unlockOpens) return
            kotlinx.coroutines.delay(30)
            calls += "unlock"
            unlocked.value = true
        }
        override suspend fun unlockWithRecoveryCode(code: String, newPassword: String) {}
        var locks = 0
        /** The working copy has changes that are not in the suite file (as the core says it). */
        var unsavedChanges = false
        override fun archiveUnsaved() = unsavedChanges
        /** Windows (true) or a phone (false): what only one of them has (keyboard keys …). */
        var desktop = false
        override val isDesktop get() = desktop
        override fun lock() {
            locks++
            // A vault open on its own ends with locking.
            if (single != null) {
                single = null
                unlocked.value = false
            }
        }
        /** The file of a vault open on its own. */
        var single: de.danerd.ghulsuite.model.SuiteFileRef? = null
        override fun singleFile() = single
        override suspend fun openSingle(
            file: de.danerd.ghulsuite.model.SuiteFileRef, data: ByteArray, password: String, pin: String?,
            keyfile: ByteArray?, onTouchNeeded: () -> Unit,
        ): VaultKindUi {
            calls += "openSingle:${file.name}"
            single = file
            unlocked.value = true
            return VaultKindUi.PASS
        }
        override suspend fun saveSingle(mode: de.danerd.ghulsuite.model.SingleWrite) = de.danerd.ghulsuite.model.SingleSaveUi.SAVED.also { calls += "saveSingle" }
        override suspend fun shareAsVault(
            share: de.danerd.ghulsuite.model.ShareVaultUi, pin: String?, onTouchNeeded: () -> Unit,
        ): de.danerd.ghulsuite.model.SharedVaultUi {
            calls += "shareAsVault:${share.vaultId}:${share.ids.joinToString(",")}:${share.name}:${share.keyfile != null}:${share.proof.password}"
            return de.danerd.ghulsuite.model.SharedVaultUi(VaultKindUi.PASS, ByteArray(10), "AAAAA-BBBBB-CCCCC-DDDDD-EEEEE-FFFFF-GGGGG-HHHHH-JJJJJ-KKKKK-MMMMM")
        }
        override fun panicLock() {}
        override fun biometricAvailable() = true
        /** Quick unlock of an older version, to set up again. */
        var quickNeedsSetup = false
        /** What unlocking with the app PIN answers (null: it opens). */
        var quickError: de.danerd.ghulsuite.model.VaultException? = null
        override fun quickUnlockReady() = QuickUnlockReady(biometric = quickReady, pin = quickReady, needsSetup = quickNeedsSetup)
        override suspend fun enableBiometricUnlock(proof: de.danerd.ghulsuite.model.FactorProofUi, hours: Int, onTouchNeeded: () -> Unit) {
            kotlinx.coroutines.delay(30)
            checkExisting(proof, null)
            quickSetups += "bio:$hours:${proof.pin}:${proof.keyfile != null}"
            settings.quickUnlockHours = hours
            settings.quickHoursBio = hours
            settings.quickUnlockBiometric = true
        }
        override suspend fun enablePinUnlock(proof: de.danerd.ghulsuite.model.FactorProofUi, pin: String, hours: Int, onTouchNeeded: () -> Unit) {
            kotlinx.coroutines.delay(30)
            checkExisting(proof, null)
            quickSetups += "pin:$pin:$hours"
            settings.quickUnlockHours = hours
            settings.quickHoursPin = hours
            settings.quickUnlockPin = true
        }
        override fun disableQuickUnlock() {}
        override fun disableQuickUnlockMethod(bio: Boolean) {}
        override suspend fun unlockWithBiometric() {}
        override suspend fun unlockWithPin(pin: String) {
            kotlinx.coroutines.delay(30)
            quickError?.let { e ->
                // As the platform: an outdated one is no longer ready, the setting stays.
                if (e.kind == de.danerd.ghulsuite.model.VaultErrorKind.QUICK_OUTDATED) { quickReady = false; quickNeedsSetup = true }
                throw e
            }
        }
        override val cal: de.danerd.ghulsuite.model.CalBackend = FakeCal()
        override val docs = FakeDocs()
        /** What [archiveInspect] says about a picked `.ghlzip` or `.ghldocs` (null: none). */
        var archiveInfo: de.danerd.ghulsuite.model.ArchiveInfoUi? = null
        override suspend fun archiveInspect(file: de.danerd.ghulsuite.model.SuiteFileRef) = archiveInfo
        /** Copies handed to other apps: name and content (the UI tests check them). */
        val shared = java.util.Collections.synchronizedList(mutableListOf<Pair<String, ByteArray>>())
        override suspend fun shareStreamToApps(name: String, mime: String, write: suspend (de.danerd.ghulsuite.model.ByteSink) -> Unit): Boolean {
            // As the platforms do it: a failed write leaves no copy and hands nothing over.
            val sink = de.danerd.ghulsuite.model.MemorySink()
            try {
                write(sink)
            } catch (e: Throwable) {
                sink.discard()
                throw e
            }
            shared += name to sink.bytes()
            return true
        }
        override suspend fun openSingleDocs(
            file: de.danerd.ghulsuite.model.SuiteFileRef, password: String, pin: String?, keyfile: ByteArray?,
            onTouchNeeded: () -> Unit, onProgress: (Long, Long) -> Unit,
        ): VaultKindUi {
            calls += "openSingleDocs:${file.name}"
            single = file
            unlocked.value = true
            return VaultKindUi.DOCS
        }
        override suspend fun listVaults() = vaults
        override suspend fun createVault(kind: VaultKindUi, name: String) = "new"
        override suspend fun renameVault(id: String, name: String) {}
        override suspend fun removeVault(id: String) { calls += "removeVault:$id" }
        override suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean) { calls += "setFavorite:$vaultId:$id:$favorite" }
        override suspend fun exportVault(id: String) = ByteArray(0)
        override suspend fun addVaultFile(data: ByteArray, name: String) = "x"
        override suspend fun adoptVault(
            data: ByteArray, name: String, password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit,
        ) = "x"
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
        /** The security check; nothing checked by default, so the list shows no security card. */
        var report = de.danerd.ghulsuite.model.SecurityReportUi(100, 0, emptyList())
        override suspend fun securityReport(): de.danerd.ghulsuite.model.SecurityReportUi {
            calls += "securityReport"
            return report
        }
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
        override suspend fun addSecurityKey(proof: de.danerd.ghulsuite.model.FactorProofUi, pin: String, label: String, onTouchNeeded: () -> Unit) {}
        override suspend fun addKeyfile(proof: de.danerd.ghulsuite.model.FactorProofUi, keyfile: ByteArray, label: String, onTouchNeeded: () -> Unit) {}
        override suspend fun newRecoveryCode(proof: de.danerd.ghulsuite.model.FactorProofUi, label: String, onTouchNeeded: () -> Unit) = ""
        override suspend fun removeSlot(proof: de.danerd.ghulsuite.model.FactorProofUi, id: String, onTouchNeeded: () -> Unit) {}
        override suspend fun changePassword(proof: de.danerd.ghulsuite.model.FactorProofUi, new: String, onTouchNeeded: () -> Unit) {}
        override fun generatePassword(options: PasswordOptionsUi) = "x7#Kq!9mVw2\$Lp4@Zr8&"
        override fun passwordScore(password: String) = if (password.length > 12) 4 else 1
        override fun copySensitive(label: String, text: String) {}
        override suspend fun previewImport(data: ByteArray, fileName: String) = ImportPreviewUi("Proton Pass", 42, 0, 5, 1)
        override suspend fun importData(
            data: ByteArray, fileName: String, passVault: String?, totpVault: String?, totpsFromLogins: Boolean, keep: KeepSkippedUi?,
        ) = ImportResultUi(42, 5, 0, 1)
        override suspend fun exportPlain(vaultId: String, password: String, spreadsheetSafe: Boolean) = ByteArray(0)
        override fun printEmergencySheet(code: String, text: EmergencySheetText) {}
        override val appVersion = "0.5.0-dev"
        override val coreVersion = "ghul-core 0.5.0 (RP-ID ghulsuite.invalid)"
    }

    private fun render(name: String, w: Int = 412, h: Int = 892, settleMs: Long = 0, content: @Composable () -> Unit) {
        val out = File("build/screenshots").apply { mkdirs() }
        ImageComposeScene(width = w * 2, height = h * 2, density = Density(2f), content = content).use { scene ->
            repeat(5) {
                scene.render(it * 100_000_000L) // let LaunchedEffects run
                // Work on other threads (a document decrypted and drawn) gets its time.
                if (settleMs > 0) Thread.sleep(settleMs)
            }
            val image = scene.render(600_000_000L)
            File(out, "$name.png").writeBytes(image.encodeToData(EncodedImageFormat.PNG)!!.bytes)
        }
    }

    /** Scopes of the states shown here, ended after the test (they listen to AppLaunch, one for all tests). */
    private val scopes = java.util.Collections.synchronizedList(mutableListOf<CoroutineScope>())

    @kotlin.test.AfterTest
    fun endScopes() {
        scopes.forEach { it.coroutineContext[kotlinx.coroutines.Job]?.cancel() }
        scopes.clear()
    }

    private fun state(exists: Boolean, unlocked: Boolean, setup: AppState.() -> Unit = {}): AppState =
        AppState(
            Fake(exists, unlocked).apply { docs.delayMs = 0; docs.sectionDelayMs = 0 },
            CoroutineScope(kotlinx.coroutines.SupervisorJob() + Dispatchers.Unconfined).also { scopes += it },
        ).apply(setup)

    @Test
    fun screens() {
        Locale.setDefault(Locale.GERMAN)
        // The start screen as on a device: with the buttons that open a file.
        System.setProperty("ghul.pickers", "true")
        try {
            render("1-setup") { GhulSuiteApp(state(exists = false, unlocked = false)) }
        } finally {
            System.clearProperty("ghul.pickers")
        }
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
        render("70-docs", settleMs = 150) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS }) }
        render("71-docs-view-image", settleMs = 300) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS; push(Screen.DocView("d1", "a2")) }) }
        render("72-docs-view-pdf", settleMs = 600) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS; push(Screen.DocView("d1", "a1")) }) }
        render("73-docs-view-text", settleMs = 300) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS; push(Screen.DocView("d1", "a3")) }) }
        render("74-docs-view-none", settleMs = 300) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS; push(Screen.DocView("d1", "a5")) }) }
        render("75-docs-dark", settleMs = 150) {
            GhulSuiteApp(state(true, true) { tab = Tab.DOCS; backend.settings.theme = de.danerd.ghulsuite.model.ThemeMode.DARK })
        }
        render("76-docs-incoming", settleMs = 150) {
            GhulSuiteApp(state(true, true) {
                tab = Tab.DOCS
                docsIncoming = listOf(object : de.danerd.ghulsuite.model.ByteSource {
                    override val name = "Rechnung Mai.pdf"
                    override val size = 120_000L
                    override fun read(buffer: ByteArray, offset: Int, length: Int) = -1
                    override fun close() {}
                })
            })
        }
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

    /** Windows window (1280 × 820) and tablets in landscape: the sidebar and one column in the middle. */
    @Test
    fun wideScreens() {
        Locale.setDefault(Locale.GERMAN)
        fun wide(name: String, content: @Composable () -> Unit) = render("w-$name", 1280, 820, content = content)
        wide("1-unlock") { GhulSuiteApp(state(exists = true, unlocked = false)) }
        wide("2-pass") { GhulSuiteApp(state(true, true)) }
        wide("3-pass-entry") { GhulSuiteApp(state(true, true) { push(Screen.EntryDetail("p1", "1")) }) }
        wide("4-pass-edit") { GhulSuiteApp(state(true, true) { push(Screen.EntryDetail("p1", "1")); push(Screen.EntryEdit("p1", "1", "login")) }) }
        wide("5-auth") { GhulSuiteApp(state(true, true) { tab = Tab.AUTH }) }
        wide("6-auth-grid") { GhulSuiteApp(state(true, true) { tab = Tab.AUTH; authGrid = true }) }
        wide("12-settings") { GhulSuiteApp(state(true, true) { tab = Tab.SETTINGS }) }
        wide("13-settings-page") { GhulSuiteApp(state(true, true) { tab = Tab.SETTINGS; push(Screen.SettingsPage(de.danerd.ghulsuite.ui.SetPage.CAL)) }) }
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
        render("w-17-docs", 1280, 820, settleMs = 150) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS }) }
        render("w-18-docs-view-pdf", 1280, 820, settleMs = 600) { GhulSuiteApp(state(true, true) { tab = Tab.DOCS; push(Screen.DocView("d1", "a1")) }) }
    }
}
