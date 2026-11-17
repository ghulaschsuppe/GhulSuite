// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.EntryData
import de.danerd.ghulsuite.core.FidoListener
import de.danerd.ghulsuite.core.FidoStatus
import de.danerd.ghulsuite.core.GhulException
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.NewVault
import de.danerd.ghulsuite.core.OtpAlgo
import de.danerd.ghulsuite.core.PasswordOptions
import de.danerd.ghulsuite.core.SetupOptions
import de.danerd.ghulsuite.core.SlotKind
import de.danerd.ghulsuite.core.TotpInput
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.calibrateKdf as coreCalibrateKdf
import de.danerd.ghulsuite.core.coreVersion as coreVersionString
import de.danerd.ghulsuite.core.generateKeyfile as coreGenerateKeyfile
import de.danerd.ghulsuite.core.generatePassword as coreGeneratePassword
import de.danerd.ghulsuite.model.AttachmentUi
import de.danerd.ghulsuite.model.EntryExtrasUi
import de.danerd.ghulsuite.model.PasswordHistoryUi
import de.danerd.ghulsuite.model.SecurityIssueKindUi
import de.danerd.ghulsuite.model.SecurityIssueUi
import de.danerd.ghulsuite.model.SecurityReportUi
import de.danerd.ghulsuite.model.TotpNowUi
import de.danerd.ghulsuite.core.inspectContainer
import de.danerd.ghulsuite.core.parseOtpauthUri
import de.danerd.ghulsuite.core.passwordStrength
import de.danerd.ghulsuite.model.ContainerInfoUi
import de.danerd.ghulsuite.model.EntrySummaryUi
import de.danerd.ghulsuite.model.EntryUi
import de.danerd.ghulsuite.model.ImportPreviewUi
import de.danerd.ghulsuite.model.ImportResultUi
import de.danerd.ghulsuite.model.KdfChoice
import de.danerd.ghulsuite.model.OpenVaultUi
import de.danerd.ghulsuite.model.OtpAlgorithmUi
import de.danerd.ghulsuite.model.PasswordOptionsUi
import de.danerd.ghulsuite.model.ProtectionUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.SetupUi
import de.danerd.ghulsuite.model.SlotType
import de.danerd.ghulsuite.model.SlotUi
import de.danerd.ghulsuite.model.TotpCodeUi
import de.danerd.ghulsuite.model.TotpInputUi
import de.danerd.ghulsuite.model.VaultBackend
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.withLock
import de.danerd.ghulsuite.model.PendingSingleUi
import de.danerd.ghulsuite.core.SetupState
import de.danerd.ghulsuite.model.SetupStateUi
import de.danerd.ghulsuite.model.SkippedImportUi
import de.danerd.ghulsuite.model.SkipReasonUi
import de.danerd.ghulsuite.model.KeepSkippedUi
import de.danerd.ghulsuite.model.ArchiveBuiltUi
import de.danerd.ghulsuite.model.ArchiveCheckUi
import de.danerd.ghulsuite.model.ArchiveRelationUi
import de.danerd.ghulsuite.model.ArchiveTargetUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.core.ArchiveCheck
import de.danerd.ghulsuite.core.ArchiveRelation
import de.danerd.ghulsuite.core.ArchiveTarget
import de.danerd.ghulsuite.core.ShareVaultOptions
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.ShareVaultUi
import de.danerd.ghulsuite.model.SharedVaultUi

/**
 * Everything of [VaultBackend] that is the same on Android and Windows: the open suite, vaults,
 * entries, codes, protection, import and export. Platforms add storage, key access, quick
 * unlock, clipboard and printing.
 */
abstract class CoreVaultBackend(private val localDir: String) : VaultBackend {
    protected val unlockedState = MutableStateFlow(false)
    override val unlocked: StateFlow<Boolean> = unlockedState.asStateFlow()
    override val coreVersion: String = coreVersionString()

    private var current: GhulSuite? = null

    /** The open suite object (created on first use, after the subclass is initialised). */
    protected val suite: GhulSuite
        get() = current ?: newSuite().also { current = it }

    /** Where the working copy lives: the app's own storage. */
    protected abstract fun storage(): VaultStorage

    /** Opens the suite again, e.g. after another storage location was chosen. */
    protected fun reopen() {
        current = newSuite()
    }

    /** The core carries no time zone database: it gets the one of this device. */
    private fun newSuite() = GhulSuite(storage(), localDir).also { it.setTimeZones(DeviceTimeZones) }

    /** Runs [block] with a connection to the security key (USB on Android, system API on Windows). */
    protected abstract suspend fun <T> withKey(block: (HidTransport) -> T): T

    /** Small strings stored per device (not in the suite), e.g. the device SSH key. */
    protected abstract fun devicePref(key: String): String?
    protected abstract fun setDevicePref(key: String, value: String)

    /** Largest memory for Argon2 in MiB that this device can afford. */
    protected abstract fun kdfMaxMib(): Int

    /** After a full unlock (password plus second factor). */
    protected open fun onFullUnlock() {}

    /** Protection changed: shortcuts like fingerprint unlock must be set up again. */
    protected open fun onProtectionChanged() {}

    protected suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { mapped(block) }

    /** Runs [block] with the key if [pin] is given, otherwise without. */
    private suspend fun <T> maybeWithKey(pin: String?, block: (HidTransport?) -> T): T =
        if (pin != null) withKey { t -> block(t) } else io { block(null) }

    protected class Touch(private val onTouchNeeded: () -> Unit) : FidoListener {
        override fun onStatus(status: FidoStatus) {
            if (status == FidoStatus.TOUCH_NEEDED) onTouchNeeded()
        }
    }

    // --- suite -----------------------------------------------------------------------------------

    override fun exists(): Boolean = runCatching { suite.exists() }.getOrDefault(false)

    // --- the suite file ----------------------------------------------------------------------------

    override suspend fun archiveProtection(data: ByteArray): ProtectionUi = io { suite.archiveProtection(data).toUi() }

    override suspend fun archiveOpen(
        data: ByteArray,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): ArchiveCheckUi = maybeWithKey(pin) { t ->
        suite.archiveOpen(data, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile).toUi()
    }

    override suspend fun archiveOpenWithCode(data: ByteArray, code: String, newPassword: String): ArchiveCheckUi =
        io { suite.archiveOpenWithCode(data, code, newPassword).toUi() }

    override fun archiveCancel() = suite.archiveCancel()

    override suspend fun archiveAccept() {
        try {
            io { suite.archiveAccept() }
        } catch (e: Throwable) {
            // The core locked before replacing: the app must not show an open suite.
            unlockedState.value = runCatching { suite.isUnlocked() }.getOrDefault(false)
            throw e
        }
        // The device's quick unlock belonged to the state before.
        onProtectionChanged()
        unlockedState.value = true
    }

    override fun archiveUnsaved(): Boolean = runCatching { suite.archiveUnsaved() }.getOrDefault(false)

    override suspend fun archiveTarget(data: ByteArray): ArchiveTargetUi = io {
        when (suite.archiveTarget(data)) {
            ArchiveTarget.NEW -> ArchiveTargetUi.NEW
            ArchiveTarget.SAME -> ArchiveTargetUi.SAME
            ArchiveTarget.CHANGED_ELSEWHERE -> ArchiveTargetUi.CHANGED_ELSEWHERE
            ArchiveTarget.OTHER_SUITE -> ArchiveTargetUi.OTHER_SUITE
        }
    }

    override suspend fun archiveBuild(device: String): ArchiveBuiltUi = io {
        val b = suite.archiveBuild(device)
        ArchiveBuiltUi(b.data, b.state, b.change.toLong())
    }

    override suspend fun archiveSaved(state: String, change: Long) = io { suite.archiveSaved(state, change.toULong()) }

    override fun suiteFile(): SuiteFileRef? =
        devicePref("suite_file")?.takeIf { it.contains('|') }?.let { SuiteFileRef(it.substringBefore('|'), it.substringAfter('|')) }

    override fun setSuiteFile(ref: SuiteFileRef?) = setDevicePref("suite_file", ref?.let { it.id + "|" + it.name } ?: "")

    // --- one vault on its own ----------------------------------------------------------------

    /** The file of the vault open on its own (then [suite] is the temporary suite around it). */
    private var single: SuiteFileRef? = null

    /** The folder of the vault open on its own (one per opening; only encrypted files). */
    private var singleDir: java.io.File? = null

    /** Folders of vaults being closed right now (their last write still runs). */
    private val closing: MutableSet<String> = java.util.Collections.synchronizedSet(mutableSetOf())

    /** One write at a time into the file of a vault open on its own. */
    private val singleWrite = kotlinx.coroutines.sync.Mutex()

    /** Where vaults opened on their own are kept meanwhile, each in a folder of its own. */
    private fun singleBase() = java.io.File(localDir).absoluteFile.resolveSibling("single-vault")

    private fun tempSuite(dir: java.io.File) =
        GhulSuite(localStorage(java.io.File(dir, "vault").path), java.io.File(dir, "local").path)

    override fun singleFile(): SuiteFileRef? = single

    override suspend fun openSingle(
        file: SuiteFileRef,
        data: ByteArray,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): VaultKindUi {
        if (single != null || unlockedState.value) lock()
        val dir = java.io.File(singleBase(), java.util.UUID.randomUUID().toString())
        val temp = tempSuite(dir).also { it.setTimeZones(DeviceTimeZones) }
        val kind = try {
            maybeWithKey(pin) { t -> temp.openSingleVault(data, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile) }
        } catch (e: Throwable) {
            io { dir.deleteRecursively() }
            throw e
        }
        // Where its changes go, also when the app is ended before they are written.
        io {
            java.io.File(dir, "target").writeText(file.id + "\n" + file.name + "\n" + temp.singleVaultName())
        }
        current = temp
        single = file
        singleDir = dir
        unlockedState.value = true
        return kind.toUi()
    }

    override suspend fun saveSingle(): Boolean {
        val ref = single ?: return false
        val s = suite
        return singleWrite.withLock {
            val f = io { s.singleVaultFile() }
            val ok = writeSuiteFile(ref, f.data)
            // Only what is in the file counts as saved (a change meanwhile stays unsaved).
            if (ok) io { s.singleSaved(f.change) }
            ok
        }
    }

    override suspend fun singleVaultBytes(): ByteArray = io { suite.singleVaultFile().data }

    /**
     * Ends the vault opened on its own at once (main thread). In the background its last changes
     * go into its file; only when that worked (or nothing was left) is its folder removed, else
     * [pendingSingleWrites] offers them again.
     */
    private fun closeSingle(ref: SuiteFileRef) {
        val s = suite
        val dir = singleDir
        s.lock()
        single = null
        singleDir = null
        current = null
        unlockedState.value = false
        dir?.let { closing += it.name }
        Thread({
            runCatching { s.finishLock() }
            if (dir != null && runCatching { writePending(dir, ref, s) }.getOrDefault(false)) dir.deleteRecursively()
            dir?.let { closing -= it.name }
        }, "ghul-single-close").apply { isDaemon = true }.start()
    }

    /** Writes the unsaved changes of the vault in [dir] into [ref]; true when nothing is left. */
    private fun writePending(dir: java.io.File, ref: SuiteFileRef, s: GhulSuite = tempSuite(dir)): Boolean {
        if (!s.archiveUnsaved()) return true
        val name = java.io.File(dir, "target").readLines().getOrNull(2) ?: return false
        val data = java.io.File(java.io.File(dir, "vault"), name).readBytes()
        return kotlinx.coroutines.runBlocking { singleWrite.withLock { writeSuiteFile(ref, data) } }
    }

    private fun targetOf(dir: java.io.File): SuiteFileRef? =
        runCatching { java.io.File(dir, "target").readLines() }.getOrNull()
            ?.takeIf { it.size >= 3 }?.let { SuiteFileRef(it[0], it[1]) }

    override fun pendingSingleWrites(): List<PendingSingleUi> {
        val open = singleDir
        return singleBase().listFiles().orEmpty().filter { it.isDirectory && it != open && it.name !in closing }.mapNotNull { dir ->
            val ref = targetOf(dir)
            if (ref == null || !tempSuite(dir).archiveUnsaved()) {
                // Nothing left to write (or never opened): gone.
                dir.deleteRecursively()
                null
            } else {
                PendingSingleUi(dir.name, ref)
            }
        }
    }

    override suspend fun retrySingleWrite(key: String): Boolean = io {
        val dir = java.io.File(singleBase(), key)
        val ref = targetOf(dir) ?: return@io true
        writePending(dir, ref).also { if (it) dir.deleteRecursively() }
    }

    override fun discardSingleWrite(key: String) {
        java.io.File(singleBase(), key).takeIf { it.name == key && key.isNotBlank() }?.deleteRecursively()
    }

    override suspend fun shareAsVault(share: ShareVaultUi, pin: String?, onTouchNeeded: () -> Unit): SharedVaultUi {
        val options = ShareVaultOptions(
            vaultId = share.vaultId,
            ids = share.ids.map { it.hexToBytes() },
            name = share.name,
            masterPassword = share.masterPassword,
            password = share.password,
            keyfile = share.keyfile,
            keyLabel = share.keyLabel,
            keyfileLabel = share.keyfileLabel,
            recoveryLabel = share.recoveryLabel,
        )
        val r = maybeWithKey(pin) { t -> suite.shareAsVault(options, t, t?.let { Touch(onTouchNeeded) }, pin) }
        return SharedVaultUi(r.kind.toUi(), r.data, r.recoveryCode)
    }

    override fun protection(): ProtectionUi? = runCatching { suite.protection().toUi() }.getOrNull()

    override suspend fun inspectFile(data: ByteArray): ContainerInfoUi = io {
        val i = inspectContainer(data)
        ContainerInfoUi(i.kind, i.protection.toUi())
    }

    override fun lock() {
        single?.let { closeSingle(it); return }
        // F92: returns at once, also while a slow server still holds the open suite (the main
        // thread must not wait for it); the rest is dropped in the background when that ends.
        val s = suite
        s.lock()
        unlockedState.value = false
        Thread({ runCatching { s.finishLock() } }, "ghul-lock").apply { isDaemon = true }.start()
    }

    // --- setup and unlock -------------------------------------------------------------------

    override suspend fun calibrateKdf(): KdfChoice = withContext(Dispatchers.Default) {
        val c = mapped { coreCalibrateKdf(1000u, kdfMaxMib().coerceIn(64, 256).toUInt()) }
        KdfChoice(c.memoryKib.toInt(), c.iterations.toInt(), c.estimatedMs.toLong())
    }

    override fun generateKeyfile(): ByteArray = coreGenerateKeyfile()

    override suspend fun create(setup: SetupUi, onTouchNeeded: () -> Unit): String {
        val options = SetupOptions(
            password = setup.password,
            keyfile = setup.keyfile,
            keyLabel = setup.keyLabel,
            keyfileLabel = setup.keyfileLabel,
            recoveryLabel = setup.recoveryLabel,
            memoryKib = setup.kdf.memoryKib.toUInt(),
            iterations = setup.kdf.iterations.toUInt(),
            vaults = setup.vaults.map { NewVault(it.kind.toCore(), it.name) },
        )
        val r = maybeWithKey(setup.keyPin) { t ->
            suite.create(options, t, t?.let { Touch(onTouchNeeded) }, setup.keyPin)
        }
        onFullUnlock()
        unlockedState.value = true
        return r.recoveryCode ?: ""
    }

    override suspend fun createFromVault(open: OpenVaultUi, onTouchNeeded: () -> Unit): String? {
        val r = maybeWithKey(open.keyPin) { t ->
            suite.createFromVault(
                open.file, open.name, open.password, t, t?.let { Touch(onTouchNeeded) }, open.keyPin, open.keyfile,
                open.recoveryLabel,
            )
        }
        onFullUnlock()
        unlockedState.value = true
        return r.recoveryCode
    }

    override suspend fun unlock(password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit) {
        maybeWithKey(pin) { t -> suite.unlock(password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile) }
        onFullUnlock()
        unlockedState.value = true
    }

    override suspend fun unlockWithRecoveryCode(code: String, newPassword: String) {
        io { suite.unlockWithRecoveryCode(code, newPassword) }
        onProtectionChanged()
        unlockedState.value = true
    }

    // --- vaults -------------------------------------------------------------------------------

    override suspend fun listVaults(): List<VaultUi> = io {
        suite.listVaults().map {
            VaultUi(it.id, it.kind.toUi(), it.name, it.file, it.available, it.entries.toInt())
        }
    }

    override suspend fun createVault(kind: VaultKindUi, name: String): String = io { suite.createVault(kind.toCore(), name) }

    override suspend fun renameVault(id: String, name: String) = io { suite.renameVault(id, name) }

    override suspend fun removeVault(id: String) = io { suite.removeVault(id) }

    override fun setupState(): SetupStateUi = when (runCatching { suite.setupState() }.getOrDefault(SetupState.EMPTY)) {
        SetupState.EMPTY -> SetupStateUi.EMPTY
        SetupState.COMPLETE -> SetupStateUi.COMPLETE
        SetupState.INCOMPLETE -> SetupStateUi.INCOMPLETE
    }

    override suspend fun addArea(kind: VaultKindUi, name: String): String = io { suite.addArea(kind.toCore(), name) }

    override suspend fun removeArea(kind: VaultKindUi) = io { suite.removeArea(kind.toCore()) }

    override suspend fun areaOptions(kind: VaultKindUi): Map<String, String> = io { suite.areaOptions(kind.toCore()) }

    override suspend fun setAreaOptions(kind: VaultKindUi, options: Map<String, String>) =
        io { suite.setAreaOptions(kind.toCore(), options) }

    override suspend fun recoveryConfirmed(): Boolean = io { suite.recoveryConfirmed() }

    override suspend fun confirmRecoveryCode(group: Int, typed: String): Boolean =
        io { suite.confirmRecoveryCode(group.toUInt(), typed) }

    override suspend fun exportVault(id: String): ByteArray = io { suite.exportVault(id) }

    override suspend fun addVaultFile(data: ByteArray, name: String): String = io { suite.addVaultFile(data, name) }

    override suspend fun adoptVault(
        data: ByteArray,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): String = maybeWithKey(pin) { t ->
        suite.adoptVault(data, name, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile)
    }

    // --- GhulPass -----------------------------------------------------------------------------

    override suspend fun listEntries(vaultId: String?): List<EntrySummaryUi> = io {
        suite.listEntries(VaultKind.PASS, vaultId).map {
            EntrySummaryUi(it.vaultId, it.vaultName, it.id.toHex(), it.entryType, it.title, it.subtitle, it.favorite, it.hasTotp)
        }
    }

    override suspend fun getEntry(vaultId: String, id: String): EntryUi = io {
        val e = suite.getEntry(vaultId, id.hexToBytes())
        EntryUi(vaultId, e.id.toHex(), e.entryType, e.fields.map { it.toUi() }, e.modified)
    }

    override suspend fun saveEntry(entry: EntryUi): String = io {
        suite.saveEntry(
            entry.vaultId,
            EntryData(
                id = entry.id?.hexToBytes() ?: ByteArray(0),
                entryType = entry.type,
                fields = entry.fields.map { it.toCore() },
                tags = emptyList(),
                folder = null,
                modified = 0,
                revision = 0uL,
            ),
        ).toHex()
    }

    override suspend fun deleteEntry(vaultId: String, id: String) = io { suite.deleteEntry(vaultId, id.hexToBytes()) }

    // --- GhulAuth -----------------------------------------------------------------------------

    override suspend fun totpCodes(nowSeconds: Long): List<TotpCodeUi> = io {
        suite.totpCodes(nowSeconds.toULong()).map {
            TotpCodeUi(
                it.vaultId, it.vaultName, it.id.toHex(), it.issuer, it.account, it.code, it.nextCode, it.period.toInt(),
                it.remaining.toInt(), it.favorite, it.modified,
            )
        }
    }

    override suspend fun addTotp(vaultId: String, input: TotpInputUi) {
        io {
            suite.addTotp(
                vaultId,
                TotpInput(
                    input.issuer, input.account, input.secret,
                    when (input.algorithm) {
                        OtpAlgorithmUi.SHA1 -> OtpAlgo.SHA1
                        OtpAlgorithmUi.SHA256 -> OtpAlgo.SHA256
                        OtpAlgorithmUi.SHA512 -> OtpAlgo.SHA512
                        OtpAlgorithmUi.STEAM -> OtpAlgo.STEAM
                    },
                    input.digits.toUInt(), input.period.toUInt(),
                ),
            )
        }
    }

    override suspend fun parseOtpauth(uri: String): TotpInputUi = io {
        val t = parseOtpauthUri(uri)
        TotpInputUi(
            t.issuer, t.account, t.secretBase32,
            when (t.algorithm) {
                OtpAlgo.SHA1 -> OtpAlgorithmUi.SHA1
                OtpAlgo.SHA256 -> OtpAlgorithmUi.SHA256
                OtpAlgo.SHA512 -> OtpAlgorithmUi.SHA512
                OtpAlgo.STEAM -> OtpAlgorithmUi.STEAM
            },
            t.digits.toInt(), t.period.toInt(),
        )
    }

    override suspend fun renameTotp(vaultId: String, id: String, issuer: String, account: String) =
        io { suite.renameTotp(vaultId, id.hexToBytes(), issuer, account) }

    override suspend fun deleteTotp(vaultId: String, id: String) = io { suite.deleteEntry(vaultId, id.hexToBytes()) }

    // --- protection ---------------------------------------------------------------------------

    override suspend fun slots(): List<SlotUi> = io {
        suite.slots().map {
            SlotUi(
                it.slotId.toHex(),
                when (it.kind) {
                    SlotKind.SECURITY_KEY -> SlotType.SECURITY_KEY
                    SlotKind.KEYFILE -> SlotType.KEYFILE
                    SlotKind.RECOVERY_CODE -> SlotType.RECOVERY_CODE
                },
                it.label,
                it.created,
                it.needsUpdate,
            )
        }
    }

    override suspend fun addSecurityKey(pin: String, label: String, onTouchNeeded: () -> Unit) =
        withKey { t -> suite.addSecurityKey(t, Touch(onTouchNeeded), pin, label) }

    override suspend fun addKeyfile(keyfile: ByteArray, label: String) = io { suite.addKeyfile(keyfile, label) }

    override suspend fun newRecoveryCode(label: String): String = io { suite.newRecoveryCode(label) }

    override suspend fun removeSlot(id: String) {
        io { suite.removeSlot(id.hexToBytes()) }
        onProtectionChanged()
    }

    override suspend fun changePassword(current: String, new: String) {
        io { suite.changePassword(current, new) }
        onProtectionChanged()
    }

    // --- import and export --------------------------------------------------------------------

    override suspend fun previewImport(data: ByteArray, fileName: String): ImportPreviewUi = io {
        val p = de.danerd.ghulsuite.core.previewImport(data, fileName)
        ImportPreviewUi(p.format, p.entries.toInt(), p.totps.toInt(), p.totpsInLogins.toInt(), p.skipped.toInt(), p.skippedItems.map { it.toUi() })
    }

    private fun de.danerd.ghulsuite.core.SkippedImport.toUi() = SkippedImportUi(
        name, kind,
        when (reason) {
            de.danerd.ghulsuite.core.SkipReason.HOTP -> SkipReasonUi.HOTP
            de.danerd.ghulsuite.core.SkipReason.UNSUPPORTED -> SkipReasonUi.UNSUPPORTED
            de.danerd.ghulsuite.core.SkipReason.INVALID_CODE -> SkipReasonUi.INVALID_CODE
        },
        detail, inEntry,
    )

    override suspend fun importData(
        data: ByteArray,
        fileName: String,
        passVault: String?,
        totpVault: String?,
        totpsFromLogins: Boolean,
        keep: KeepSkippedUi?,
    ): ImportResultUi = io {
        val k = keep?.let {
            de.danerd.ghulsuite.core.KeepSkipped(it.folder, it.reasonLabel, it.dataLabel, it.reasonHotp, it.reasonUnsupported, it.reasonInvalid)
        }
        val r = suite.importData(data, fileName, passVault, totpVault, totpsFromLogins, k)
        ImportResultUi(r.entries.toInt(), r.totps.toInt(), r.duplicates.toInt(), r.skipped.toInt(), r.skippedItems.map { it.toUi() }, r.kept.toInt())
    }

    override suspend fun exportPlain(vaultId: String, password: String): ByteArray = io { suite.exportPlain(vaultId, password) }
    override suspend fun exportEntries(vaultId: String, ids: List<String>, password: String): ByteArray =
        io { suite.exportEntries(vaultId, ids.map { it.hexToBytes() }, password) }

    // --- tools --------------------------------------------------------------------------------

    override fun generatePassword(options: PasswordOptionsUi): String = mapped {
        coreGeneratePassword(
            PasswordOptions(
                options.length.toUInt(), options.lowercase, options.uppercase, options.digits, options.symbols,
                options.avoidAmbiguous,
            ),
        )
    }

    override fun passwordScore(password: String): Int = passwordStrength(password).score.toInt()

    // --- GhulPass extras ------------------------------------------------------------------------

    override suspend fun entryExtras(vaultId: String, id: String): EntryExtrasUi = io {
        val e = suite.entryExtras(vaultId, id.hexToBytes())
        EntryExtrasUi(
            e.favorite, e.created,
            e.history.map { PasswordHistoryUi(it.password, it.changed) },
            e.attachments.map { AttachmentUi(it.name, it.size.toLong()) },
        )
    }

    override suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean) = io { suite.setEntryFavorite(vaultId, id.hexToBytes(), favorite) }

    override suspend fun addAttachment(vaultId: String, id: String, name: String, data: ByteArray) = io {
        suite.addEntryAttachment(vaultId, id.hexToBytes(), name, data)
    }

    override suspend fun attachment(vaultId: String, id: String, name: String): ByteArray = io { suite.entryAttachment(vaultId, id.hexToBytes(), name) }

    override suspend fun removeAttachment(vaultId: String, id: String, name: String) = io { suite.removeEntryAttachment(vaultId, id.hexToBytes(), name) }

    override suspend fun duplicateEntry(vaultId: String, id: String, titleSuffix: String): String = io {
        suite.duplicateEntry(vaultId, id.hexToBytes(), titleSuffix).toHex()
    }

    override suspend fun moveEntry(vaultId: String, id: String, targetVaultId: String): String = io {
        suite.moveEntry(vaultId, id.hexToBytes(), targetVaultId).toHex()
    }

    override suspend fun securityReport(): SecurityReportUi = io {
        val r = suite.securityReport()
        SecurityReportUi(
            r.score.toInt(), r.checked.toInt(),
            r.issues.map {
                SecurityIssueUi(
                    it.vaultId, it.vaultName, it.id.toHex(), it.title,
                    when (it.kind) {
                        de.danerd.ghulsuite.core.SecurityIssueKind.REUSED -> SecurityIssueKindUi.REUSED
                        de.danerd.ghulsuite.core.SecurityIssueKind.WEAK -> SecurityIssueKindUi.WEAK
                        de.danerd.ghulsuite.core.SecurityIssueKind.NO_SECOND_FACTOR -> SecurityIssueKindUi.NO_SECOND_FACTOR
                    },
                )
            },
        )
    }

    override fun entryTotp(secret: String, nowSeconds: Long): TotpNowUi? =
        de.danerd.ghulsuite.core.entryTotp(secret, nowSeconds.toULong())?.let { TotpNowUi(it.code, it.nextCode, it.remaining.toInt(), it.period.toInt()) }

    override fun generatePassphrase(words: Int, separator: String, capitalize: Boolean, number: Boolean): String = mapped {
        de.danerd.ghulsuite.core.generatePassphrase(words.toUInt(), separator, capitalize, number)
    }

    override suspend fun totpUri(vaultId: String, id: String): String = io { suite.totpUri(vaultId, id.hexToBytes()) }

    override suspend fun clockOffset(nowSeconds: Long): Long? = io { suite.clockOffset(nowSeconds) }



    override suspend fun pwnedCount(password: String): Int = io { de.danerd.ghulsuite.core.pwnedCount(password).toInt() }

    override suspend fun sendReport(report: String, note: String, contact: String) = io {
        de.danerd.ghulsuite.core.reportSend(report, note, contact)
    }
}

internal fun ArchiveCheck.toUi() = ArchiveCheckUi(
    relation = when (relation) {
        ArchiveRelation.NO_SUITE -> ArchiveRelationUi.NO_SUITE
        ArchiveRelation.OTHER_SUITE -> ArchiveRelationUi.OTHER_SUITE
        ArchiveRelation.SAME -> ArchiveRelationUi.SAME
        ArchiveRelation.NEWER -> ArchiveRelationUi.NEWER
        ArchiveRelation.OLDER -> ArchiveRelationUi.OLDER
        ArchiveRelation.BRANCH -> ArchiveRelationUi.BRANCH
    },
    unsavedChanges = unsavedChanges,
    unverified = unverified,
    savedAt = savedAt,
    device = device,
    vaults = vaults.toInt(),
)
