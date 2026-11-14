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
import de.danerd.ghulsuite.core.generateSshKey
import de.danerd.ghulsuite.core.inspectContainer
import de.danerd.ghulsuite.core.parseOtpauthUri
import de.danerd.ghulsuite.core.passwordStrength
import de.danerd.ghulsuite.model.ContainerInfoUi
import de.danerd.ghulsuite.model.EntrySummaryUi
import de.danerd.ghulsuite.model.EntryUi
import de.danerd.ghulsuite.model.FoundVaultUi
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
import de.danerd.ghulsuite.model.VaultSourceUi
import de.danerd.ghulsuite.model.VaultUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import de.danerd.ghulsuite.core.SetupState
import de.danerd.ghulsuite.model.SetupStateUi
import de.danerd.ghulsuite.model.SkippedImportUi
import de.danerd.ghulsuite.model.SkipReasonUi
import de.danerd.ghulsuite.model.KeepSkippedUi

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

    /** Where the suite lives (app folder, chosen folder, USB stick …). */
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

    override fun storageName(): String = suite.storageName()

    override fun protection(): ProtectionUi? = runCatching { suite.protection().toUi() }.getOrNull()

    override suspend fun inspectFile(data: ByteArray): ContainerInfoUi = io {
        val i = inspectContainer(data)
        ContainerInfoUi(i.kind, i.protection.toUi())
    }

    override fun lock() {
        suite.lock()
        unlockedState.value = false
    }

    override fun deviceSshKey(): String {
        devicePref("device_ssh_public")?.let { return it }
        val pair = mapped { generateSshKey() }
        setDevicePref("device_ssh_private", pair.privateKey)
        setDevicePref("device_ssh_public", pair.publicKey)
        return pair.publicKey
    }

    override suspend fun fetchFromServer(input: ServerInputUi, hostKey: String?): String = io {
        val key = if (input.password.isNullOrEmpty()) {
            deviceSshKey()
            devicePref("device_ssh_private")
        } else {
            null
        }
        try {
            suite.fetchFromServer(input.toCore(), hostKey, key)
        } catch (e: GhulException.NoVault) {
            throw VaultException(VaultErrorKind.NOT_FOUND)
        }
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
            VaultUi(it.id, it.kind.toUi(), it.name, it.file, it.available, it.entries.toInt(), it.outdated)
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
        source: VaultSourceUi,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): String = maybeWithKey(pin) { t ->
        suite.adoptVault(source.toCore(), name, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile)
    }

    override suspend fun findNewVaults(): List<FoundVaultUi> = io {
        suite.findNewVaults().map { FoundVaultUi(it.file, it.kind.toUi(), it.protection.toUi()) }
    }

    override suspend fun ignoreFoundVault(file: String) = io { suite.ignoreFoundVault(file) }

    override suspend fun acceptVaultState(id: String) = io { suite.acceptVaultState(id) }

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

    override suspend fun adoptRemoteProtection(password: String?, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit) {
        maybeWithKey(pin) { t -> suite.adoptRemoteProtection(password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile) }
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

    override suspend fun suiteBackup(): ByteArray = io { suite.suiteBackup() }

    override suspend fun restoreBackup(data: ByteArray): Int = io { suite.restoreBackup(data).toInt() }

    override suspend fun pwnedCount(password: String): Int = io { de.danerd.ghulsuite.core.pwnedCount(password).toInt() }
}
