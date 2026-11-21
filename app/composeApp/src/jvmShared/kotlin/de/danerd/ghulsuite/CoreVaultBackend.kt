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
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.withLock
import de.danerd.ghulsuite.model.PendingSingleUi
import de.danerd.ghulsuite.model.SingleSaveUi
import de.danerd.ghulsuite.model.SingleWrite
import de.danerd.ghulsuite.core.SetupState
import de.danerd.ghulsuite.model.SetupStateUi
import de.danerd.ghulsuite.model.SkippedImportUi
import de.danerd.ghulsuite.model.SkipReasonUi
import de.danerd.ghulsuite.model.KeepSkippedUi
import de.danerd.ghulsuite.core.ArchiveBuilt
import de.danerd.ghulsuite.core.ArchiveProgress
import de.danerd.ghulsuite.core.ArchiveSink
import de.danerd.ghulsuite.core.ArchiveSource
import de.danerd.ghulsuite.core.StorageException
import de.danerd.ghulsuite.core.archiveWrittenMatches
import de.danerd.ghulsuite.model.ArchiveCheckUi
import de.danerd.ghulsuite.model.ArchiveRelationUi
import de.danerd.ghulsuite.model.ArchiveTargetUi
import de.danerd.ghulsuite.model.FactorProofUi
import de.danerd.ghulsuite.model.ArchiveTargetInfoUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.core.ArchiveCheck
import de.danerd.ghulsuite.core.ArchiveRelation
import de.danerd.ghulsuite.core.ArchiveTarget
import de.danerd.ghulsuite.core.ShareVaultOptions
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.ShareVaultUi
import de.danerd.ghulsuite.model.SharedVaultUi

/** A suite file opened for reading: the stream and its size (0: not known). */
class SuiteInput(val stream: java.io.InputStream, val size: Long)

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

    /** The temporary suite around a vault opened on its own (see [openSingle]). */
    @Volatile private var temp: GhulSuite? = null

    /** The own suite (created on first use, after the subclass is initialised). */
    protected val ownSuite: GhulSuite
        get() = current ?: newSuite().also { current = it }

    /** The suite the screens work with: a vault opened on its own, else the own suite. */
    protected val suite: GhulSuite
        get() = temp ?: ownSuite

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

    /** Calls of the core with the security key when a PIN is given (GhulDocs: hand on, take over). */
    private val keyRunner = object : KeyRunner {
        override suspend fun <T> run(pin: String?, block: (HidTransport?) -> T): T = maybeWithKey(pin, block)
    }

    private val coreDocs by lazy { CoreDocsBackend({ suite }, { openSuiteInput(it) }, keyRunner) }

    override val docs: de.danerd.ghulsuite.model.DocsBackend get() = coreDocs

    override suspend fun archiveInspect(file: SuiteFileRef): de.danerd.ghulsuite.model.ArchiveInfoUi? = coreDocs.inspect(file)

    // --- the suite file ----------------------------------------------------------------------------

    /** Opens a suite file for reading (streamed); null if it cannot be read. */
    protected open fun openSuiteInput(ref: SuiteFileRef): SuiteInput? = null

    /**
     * Writes the suite file [ref] (streamed): [write] puts all bytes into the stream it gets,
     * [check] reads the result back and says whether it is complete. True only when the file is
     * in place and [check] said so; where the platform allows it the old file is replaced only
     * then (see the platforms). [created]: the file was just made for this; a failed write
     * removes it.
     */
    protected open fun writeSuiteStreamed(
        ref: SuiteFileRef,
        created: Boolean,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
    ): Boolean = false

    /** A suite file for the core to read, piece by piece. */
    private class StreamSource(private val input: SuiteInput) : ArchiveSource {
        override fun size(): ULong = input.size.coerceAtLeast(0).toULong()

        override fun read(max: UInt): ByteArray {
            val buf = ByteArray(max.toInt().coerceIn(1, 1 shl 20))
            var n = 0
            try {
                while (n < buf.size) {
                    val r = input.stream.read(buf, n, buf.size - n)
                    if (r < 0) break
                    n += r
                }
            } catch (e: java.io.IOException) {
                throw StorageException.Io("the file cannot be read")
            }
            return if (n == buf.size) buf else buf.copyOf(n)
        }
    }

    private class StreamSink(private val out: java.io.OutputStream) : ArchiveSink {
        override fun write(data: ByteArray) {
            try {
                out.write(data)
            } catch (e: java.io.IOException) {
                throw StorageException.Io("the file cannot be written")
            }
        }
    }

    private class Progress(private val report: (Long, Long) -> Unit) : ArchiveProgress {
        override fun progress(done: ULong, total: ULong) = report(done.toLong(), total.toLong())
    }

    /** Runs [block] with [file] open for reading; a file that cannot be opened is a storage error. */
    private fun <T> reading(file: SuiteFileRef, block: (ArchiveSource) -> T): T {
        val input = openSuiteInput(file) ?: throw VaultException(VaultErrorKind.STORAGE, "the file cannot be read")
        return input.stream.use { block(StreamSource(input)) }
    }

    override suspend fun archiveProtection(file: SuiteFileRef): ProtectionUi =
        io { reading(file) { suite.archiveProtection(it).toUi() } }

    /**
     * One action on the suite file or the working copy at a time: saving (also by itself), opening
     * a suite file, taking it over, deleting the working copy. Otherwise a save could stream
     * documents that are just being replaced, or a file be read while it is written.
     */
    private val fileLock = kotlinx.coroutines.sync.Mutex()

    override suspend fun archiveOpen(
        file: SuiteFileRef,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): ArchiveCheckUi = fileLock.withLock { maybeWithKey(pin) { t ->
        reading(file) { src ->
            suite.archiveOpen(src, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile, Progress(onProgress)).toUi()
        }
    } }

    override suspend fun archiveOpenWithCode(
        file: SuiteFileRef,
        code: String,
        newPassword: String,
        onProgress: (Long, Long) -> Unit,
    ): ArchiveCheckUi = fileLock.withLock { io { reading(file) { suite.archiveOpenWithCode(it, code, newPassword, Progress(onProgress)).toUi() } } }

    override fun archiveCancel() = suite.archiveCancel()

    override suspend fun archiveAccept() = fileLock.withLock { acceptLocked() }

    private suspend fun acceptLocked() {
        val s = suite
        try {
            io { s.archiveAccept() }
        } catch (e: Throwable) {
            // The core may have locked before failing: the app must not show an open suite.
            // The core got as far as removing this device's quick unlock of the old state: the
            // platform's part of it goes too (not when it failed before that).
            if (runCatching { s.archiveDeviceDataWiped() }.getOrDefault(false)) onProtectionChanged()
            unlockedState.value = runCatching { s.isUnlocked() }.getOrDefault(false)
            throw e
        }
        // The device's quick unlock belonged to the state before.
        onProtectionChanged()
        unlockedState.value = true
    }

    override fun archiveUnsaved(): Boolean = runCatching { suite.archiveUnsaved() }.getOrDefault(false)

    override fun archiveUnsavedSince(): Long = runCatching { suite.archiveUnsavedSince() }.getOrDefault(0L)

    override fun archiveSavedAt(): Long = runCatching { suite.archiveSavedAt() }.getOrDefault(0L)

    override suspend fun archiveTarget(file: SuiteFileRef): ArchiveTargetUi? = archiveTargetInfo(file)?.target

    override suspend fun archiveTargetInfo(file: SuiteFileRef): ArchiveTargetInfoUi? = io {
        val input = openSuiteInput(file) ?: return@io null
        val info = input.stream.use { suite.archiveTargetInfo(StreamSource(input)) }
        val target = when (info.target) {
            ArchiveTarget.NEW -> ArchiveTargetUi.NEW
            ArchiveTarget.SAME -> ArchiveTargetUi.SAME
            ArchiveTarget.CHANGED_ELSEWHERE -> ArchiveTargetUi.CHANGED_ELSEWHERE
            ArchiveTarget.OTHER_SUITE -> ArchiveTargetUi.OTHER_SUITE
        }
        ArchiveTargetInfoUi(target, info.device, info.time)
    }

    override suspend fun archiveSave(
        target: SuiteFileRef,
        device: String,
        created: Boolean,
        onProgress: (Long, Long) -> Unit,
    ): Boolean = fileLock.withLock { io {
        val s = suite
        var built: ArchiveBuilt? = null
        // One bar for both steps: writing is the first half, reading back the second.
        val written = writeSuiteStreamed(
            target,
            created,
            write = { out -> built = s.archiveBuild(device, StreamSink(out), Progress { d, t -> onProgress(d, 2 * t) }) },
            check = { input ->
                val b = built ?: return@writeSuiteStreamed false
                val size = b.size.toLong()
                archiveWrittenMatches(StreamSource(input), b.size, b.sha256, Progress { d, _ -> onProgress(size + d, 2 * size) })
            },
        )
        val b = built
        if (written && b != null) {
            s.archiveSaved(b.state, b.change)
            // Here, not after returning: the app may end meanwhile, the place must be known then.
            setSuiteFile(target)
        }
        written && b != null
    } }

    override suspend fun deleteSuite() = fileLock.withLock { deleteLocked() }

    private suspend fun deleteLocked() {
        // A vault open on its own is closed (written back) first; the own suite is deleted.
        lock()
        val s = ownSuite
        try {
            io { s.deleteWorkingCopy() }
        } finally {
            // Also when a file could not be deleted: nothing of the old suite opens quickly or
            // reminds of it, and the app does not save into its suite file.
            onProtectionChanged()
            cal.clearReminders()
            setSuiteFile(null)
            settings.forgetSuite()
            unlockedState.value = false
        }
    }

    override fun suiteFile(): SuiteFileRef? =
        devicePref("suite_file")?.takeIf { it.contains('|') }?.let { SuiteFileRef(it.substringBefore('|'), it.substringAfter('|')) }

    override fun setSuiteFile(ref: SuiteFileRef?) = setDevicePref("suite_file", ref?.let { it.id + "|" + it.name } ?: "")

    // --- one vault on its own ----------------------------------------------------------------

    /** The file of the vault open on its own (then [suite] is the temporary suite around it). */
    @Volatile private var single: SuiteFileRef? = null

    /** The folder of the vault open on its own (one per opening; only encrypted files). */
    @Volatile private var singleDir: java.io.File? = null

    /** Folders being opened or closed right now: not leftovers. */
    private val busyDirs: MutableSet<String> = java.util.Collections.synchronizedSet(mutableSetOf())

    /** One write at a time into the file of a vault open on its own. */
    private val singleWrite = kotlinx.coroutines.sync.Mutex()

    private val pendingTick = MutableStateFlow(0)
    override val singleWritesChanged: StateFlow<Int> = pendingTick.asStateFlow()

    /** File extensions of vault files (a record that cannot be read falls back to them). */
    private val VAULT_EXTENSIONS = setOf("ghlpss", "ghltotp", "ghlcal", "ghldoc")

    /**
     * The vault open on its own is a GhulDocs vault: its file is a `.ghldocs` with the vault and
     * its documents, read and written as a stream (see [openSingleDocs]).
     */
    @Volatile private var singleDocs = false

    /** Set while the panic lock locks: nothing is written back first (see [closeSingle]). */
    @Volatile protected var panicking = false

    /** A folder of a vault opened on its own holds a GhulDocs vault (its record names a `.ghldoc`). */
    private fun isDocsDir(dir: java.io.File, t: Target? = readTarget(dir)): Boolean =
        (t?.vault ?: vaultIn(dir))?.endsWith(".ghldoc") == true

    /** SHA-256 (hex) of the file at [ref], read as a stream; null if it cannot be read. */
    private fun fileHash(ref: SuiteFileRef): String? {
        val input = openSuiteInput(ref) ?: return null
        return runCatching {
            input.stream.use { s ->
                val md = java.security.MessageDigest.getInstance("SHA-256")
                val buf = ByteArray(1 shl 20)
                while (true) {
                    val n = s.read(buf)
                    if (n < 0) break
                    md.update(buf, 0, n)
                }
                md.digest().joinToString("") { b -> "%02x".format(b) }
            }
        }.getOrNull()
    }

    /** Where vaults opened on their own are kept meanwhile, each in a folder of its own. */
    private fun singleBase() = java.io.File(localDir).absoluteFile.resolveSibling("single-vault")

    private fun tempSuite(dir: java.io.File) =
        GhulSuite(localStorage(java.io.File(dir, "vault").path), java.io.File(dir, "local").path)

    /** What a folder of a vault opened on its own records: its file and what is in that file. */
    private class Target(val ref: SuiteFileRef, val vault: String, val hash: String)

    /** The vault file in a folder (when its record cannot be read; the newest if several). */
    private fun vaultIn(dir: java.io.File): String? =
        java.io.File(dir, "vault").listFiles().orEmpty().filter { it.isFile && it.extension in VAULT_EXTENSIONS }
            .maxByOrNull { it.lastModified() }?.name

    private fun readTarget(dir: java.io.File): Target? = runCatching {
        val file = java.io.File(dir, "target")
        val lines = file.readLines()
        // The earlier form: exactly three plain lines (a properties file starts with a comment).
        if (lines.size == 3 && lines.none { it.startsWith("#") || it.startsWith("id=") }) {
            // Written by an earlier development version: three plain lines, no hash.
            Target(SuiteFileRef(lines[0], lines[1]), lines[2], "")
        } else {
            val p = java.util.Properties()
            file.inputStream().use { p.load(it) }
            Target(SuiteFileRef(p.getProperty("id")!!, p.getProperty("name")!!), p.getProperty("vault")!!, p.getProperty("hash") ?: "")
        }
    }.getOrNull()

    private fun writeTarget(dir: java.io.File, t: Target) {
        dir.mkdirs()
        val p = java.util.Properties()
        p.setProperty("id", t.ref.id)
        p.setProperty("name", t.ref.name)
        p.setProperty("vault", t.vault)
        p.setProperty("hash", t.hash)
        val tmp = java.io.File(dir, "target.tmp")
        tmp.outputStream().use { p.store(it, null) }
        java.nio.file.Files.move(tmp.toPath(), java.io.File(dir, "target").toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
    }

    private fun sha(data: ByteArray?): String =
        data?.let { java.security.MessageDigest.getInstance("SHA-256").digest(it).joinToString("") { b -> "%02x".format(b) } } ?: ""

    private fun tick() = pendingTick.update { it + 1 }

    override fun singleFile(): SuiteFileRef? = single

    override fun ownSuiteExists(): Boolean = runCatching { ownSuite.exists() }.getOrDefault(false)

    /**
     * Changes of earlier openings of [ref] that wait (only records read); with [busyToo] also
     * those still being written.
     */
    private fun waitingFor(ref: SuiteFileRef, busyToo: Boolean = true): Boolean {
        val open = singleDir
        return singleBase().listFiles().orEmpty().any { dir ->
            val busy = dir.name in busyDirs
            dir.isDirectory && dir != open && (busyToo || !busy) && readTarget(dir)?.ref?.id == ref.id &&
                (busy || tempSuite(dir).archiveUnsaved())
        }
    }

    override suspend fun hasPendingSingleWrite(ref: SuiteFileRef): Boolean = io { waitingFor(ref) }

    override suspend fun openSingle(
        file: SuiteFileRef,
        data: ByteArray,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): VaultKindUi {
        // Changes of an earlier opening of this file come first: refused before anything is closed.
        if (io { waitingFor(file, busyToo = false) }) throw VaultException(VaultErrorKind.SINGLE_PENDING)
        if (single != null || unlockedState.value) lock()
        // A closing still writing: wait for it (it may be this file), then read the file anew.
        var bytes = data
        if (busyDirs.isNotEmpty()) {
            io {
                for (i in 0 until 300) {
                    if (busyDirs.isEmpty()) break
                    Thread.sleep(100)
                }
            }
            if (busyDirs.isNotEmpty()) throw VaultException(VaultErrorKind.SINGLE_BUSY)
            bytes = readSuiteFile(file) ?: data
        }
        // Changes of an earlier opening of this file come first (else they would be overwritten).
        if (io { waitingFor(file) }) throw VaultException(VaultErrorKind.SINGLE_PENDING)
        val name = java.util.UUID.randomUUID().toString()
        val dir = java.io.File(singleBase(), name)
        busyDirs += name
        val t = tempSuite(dir).also { it.setTimeZones(DeviceTimeZones) }
        try {
            val kind = maybeWithKey(pin) { k -> t.openSingleVault(bytes, password, k, k?.let { Touch(onTouchNeeded) }, pin, keyfile) }
            // Where its changes go, also when the app is ended before they are written.
            io { writeTarget(dir, Target(file, t.singleVaultName(), sha(bytes))) }
            temp = t
            single = file
            singleDir = dir
            unlockedState.value = true
            return kind.toUi()
        } catch (e: Throwable) {
            runCatching { t.lock(); t.finishLock() }
            io { dir.deleteRecursively() }
            throw e
        } finally {
            busyDirs -= name
        }
    }

    /**
     * Writes [data] into the file of [dir] as [mode] allows: [SingleWrite.CHECKED] only when the
     * file is as this vault left it, [SingleWrite.ANEW] also where it is gone (created, never
     * replaced), [SingleWrite.OVERWRITE] whatever it holds. Records what is in the file now. Only
     * while holding [singleWrite].
     */
    private suspend fun writeBackLocked(dir: java.io.File, data: ByteArray, mode: SingleWrite): SingleSaveUi {
        val t = io { readTarget(dir) } ?: return SingleSaveUi.FAILED
        val done = suspend {
            io { writeTarget(dir, Target(t.ref, t.vault, sha(data))) }
            SingleSaveUi.SAVED
        }
        if (mode == SingleWrite.OVERWRITE) return if (writeSuiteFile(t.ref, data)) done() else SingleSaveUi.FAILED
        val now = readSuiteFile(t.ref)
        when {
            // Gone or unreadable (deleted, moved, storage away): only made anew when asked, and
            // then only where nothing is (a file that is there but unreadable stays).
            now == null -> {
                if (mode != SingleWrite.ANEW) return SingleSaveUi.MISSING
                if (createSuiteFile(t.ref, data)) return done()
                // Not made: what is there now decides.
                val there = readSuiteFile(t.ref) ?: return SingleSaveUi.FAILED
                return when {
                    there.contentEquals(data) -> done()
                    t.hash.isNotEmpty() && sha(there) == t.hash -> if (writeSuiteFile(t.ref, data)) done() else SingleSaveUi.FAILED
                    else -> SingleSaveUi.CHANGED_ELSEWHERE
                }
            }
            // It holds exactly this already (an earlier write whose record was not updated):
            // nothing to write, a write would only risk the file.
            now.contentEquals(data) -> return done()
            // Changed elsewhere since this vault read or wrote it.
            t.hash.isNotEmpty() && sha(now) != t.hash -> return SingleSaveUi.CHANGED_ELSEWHERE
        }
        return if (writeSuiteFile(t.ref, data)) done() else SingleSaveUi.FAILED
    }

    override suspend fun saveSingle(mode: SingleWrite): SingleSaveUi = singleWrite.withLock {
        // Read while holding the lock: an older state never goes over a newer one.
        val dir = singleDir ?: return@withLock SingleSaveUi.FAILED
        val s = temp ?: return@withLock SingleSaveUi.FAILED
        if (singleDocs) return@withLock io { writeBackDocs(dir, s, mode) }
        val f = io { s.singleVaultFile() }
        val r = writeBackLocked(dir, f.data, mode)
        // Only what is in the file counts as saved (a change meanwhile stays unsaved).
        if (r == SingleSaveUi.SAVED) io { s.singleSaved(f.change) }
        r
    }

    override suspend fun saveSingleAs(ref: SuiteFileRef): Boolean = singleWrite.withLock {
        val dir = singleDir ?: return@withLock false
        val s = temp ?: return@withLock false
        if (singleDocs) {
            // Written into the new file, which is its file from now on (writeBackDocs records it).
            val r = io { writeBackDocs(dir, s, SingleWrite.OVERWRITE, target = ref, created = true) }
            if (r != SingleSaveUi.SAVED) return@withLock false
            if (singleDir === dir) single = ref
            return@withLock true
        }
        val f = io { s.singleVaultFile() }
        val old = io { readTarget(dir) } ?: return@withLock false
        if (!writeSuiteFile(ref, f.data)) return@withLock false
        // Closed meanwhile (locked): the new file has it, nothing else changes.
        if (singleDir !== dir) return@withLock true
        // From now on this vault goes into the new file (set before anything can suspend, so a
        // lock meanwhile is not undone).
        val before = single
        single = ref
        try {
            io { writeTarget(dir, Target(ref, old.vault, sha(f.data))) }
        } catch (e: Throwable) {
            // The record still names the old file: so does the app.
            if (singleDir === dir) single = before
            throw e
        }
        io { s.singleSaved(f.change) }
        true
    }

    override suspend fun singleVaultBytes(): ByteArray = io { (temp ?: throw VaultException(VaultErrorKind.LOCKED)).singleVaultFile().data }

    override val singleIsDocs: Boolean get() = singleDocs

    /**
     * Ends the vault opened on its own at once (main thread). Its last changes go into its file
     * on a thread the app waits for when it ends; only when that worked (or nothing was left) is
     * its folder removed, else [pendingSingleWrites] offers them again.
     */
    private fun closeSingle() {
        val s = suite
        val dir = singleDir
        val docs = singleDocs
        // A GhulDocs vault can only be written back while it is open (its documents are listed in
        // its index): its last changes go into its file first, then it locks. Not so in a panic:
        // then it locks at once, and what is not in its file yet stays behind (encrypted).
        val writeFirst = docs && !panicking
        // Busy before it stops being the open one: never taken for a leftover in between.
        dir?.let { busyDirs += it.name }
        if (!writeFirst) s.lock()
        single = null
        singleDir = null
        singleDocs = false
        temp = null
        unlockedState.value = false
        Thread({
            val saved = if (writeFirst) {
                val r = dir == null || runCatching {
                    !s.archiveUnsaved() || kotlinx.coroutines.runBlocking { singleWrite.withLock { writeBackDocs(dir, s, SingleWrite.CHECKED) } } == SingleSaveUi.SAVED
                }.getOrDefault(false)
                s.lock()
                r
            } else {
                false
            }
            runCatching { s.finishLock() }
            val done = when {
                dir == null -> false
                writeFirst -> saved
                docs -> runCatching { !s.archiveUnsaved() }.getOrDefault(false)
                else -> runCatching { writePending(dir, s, SingleWrite.CHECKED) == SingleSaveUi.SAVED }.getOrDefault(false)
            }
            if (dir != null && done) dir.deleteRecursively()
            dir?.let { busyDirs -= it.name }
            tick()
        }, "ghul-single-close").apply { isDaemon = false }.start()
    }

    /**
     * Writes the GhulDocs vault open on its own (in [s], folder [dir]) into its file as a stream,
     * as [mode] allows (see [writeBackLocked]); the file is read back and compared before it
     * counts. Only while holding [singleWrite].
     */
    private fun writeBackDocs(dir: java.io.File, s: GhulSuite, mode: SingleWrite, target: SuiteFileRef? = null, created: Boolean = false): SingleSaveUi {
        val t = readTarget(dir) ?: return SingleSaveUi.FAILED
        val ref = target ?: t.ref
        if (target == null && mode != SingleWrite.OVERWRITE) {
            val now = fileHash(ref)
            when {
                // Gone or unreadable: only written anew when asked.
                now == null -> if (mode != SingleWrite.ANEW) return SingleSaveUi.MISSING
                // Changed elsewhere since this vault read or wrote it.
                t.hash.isNotEmpty() && now != t.hash -> return SingleSaveUi.CHANGED_ELSEWHERE
            }
        }
        var written: de.danerd.ghulsuite.core.SingleDocsWritten? = null
        val ok = runCatching {
            writeSuiteStreamed(
                ref, created,
                write = { out -> written = s.singleDocsWrite(StreamSink(out), null) },
                check = { input ->
                    val w = written ?: return@writeSuiteStreamed false
                    archiveWrittenMatches(StreamSource(input), w.size, w.sha256, null)
                },
            )
        }.getOrDefault(false)
        val w = written
        if (!ok || w == null) return SingleSaveUi.FAILED
        writeTarget(dir, Target(ref, t.vault, w.sha256.lowercase()))
        s.singleSaved(w.change)
        return SingleSaveUi.SAVED
    }

    override suspend fun openSingleDocs(
        file: SuiteFileRef,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): VaultKindUi {
        if (io { waitingFor(file, busyToo = false) }) throw VaultException(VaultErrorKind.SINGLE_PENDING)
        if (single != null || unlockedState.value) lock()
        if (busyDirs.isNotEmpty()) {
            io {
                for (i in 0 until 300) {
                    if (busyDirs.isEmpty()) break
                    Thread.sleep(100)
                }
            }
            if (busyDirs.isNotEmpty()) throw VaultException(VaultErrorKind.SINGLE_BUSY)
        }
        if (io { waitingFor(file) }) throw VaultException(VaultErrorKind.SINGLE_PENDING)
        val name = java.util.UUID.randomUUID().toString()
        val dir = java.io.File(singleBase(), name)
        busyDirs += name
        val t = tempSuite(dir).also { it.setTimeZones(DeviceTimeZones) }
        try {
            // The file's hash on the way (what it holds now: written back only over this state).
            val hash = maybeWithKey(pin) { k ->
                val input = openSuiteInput(file) ?: throw VaultException(VaultErrorKind.STORAGE, "the file cannot be read")
                input.stream.use { raw ->
                    val md = java.security.MessageDigest.getInstance("SHA-256")
                    val hashing = java.security.DigestInputStream(raw, md)
                    t.openSingleDocs(StreamSource(SuiteInput(hashing, input.size)), password, k, k?.let { Touch(onTouchNeeded) }, pin, keyfile, Progress(onProgress))
                    // The rest of the file (the core may stop before its end) counts too.
                    val buf = ByteArray(1 shl 16)
                    while (hashing.read(buf) >= 0) { /* hashed */ }
                    md.digest().joinToString("") { b -> "%02x".format(b) }
                }
            }
            io { writeTarget(dir, Target(file, t.singleVaultName(), hash)) }
            temp = t
            single = file
            singleDir = dir
            singleDocs = true
            unlockedState.value = true
            return VaultKindUi.DOCS
        } catch (e: Throwable) {
            runCatching { t.lock(); t.finishLock() }
            io { dir.deleteRecursively() }
            throw e
        } finally {
            busyDirs -= name
        }
    }

    /** Writes the unsaved changes of the vault in [dir] into its file. */
    private fun writePending(dir: java.io.File, s: GhulSuite = tempSuite(dir), mode: SingleWrite): SingleSaveUi {
        if (!s.archiveUnsaved()) return SingleSaveUi.SAVED
        val t = readTarget(dir) ?: return SingleSaveUi.FAILED
        val data = java.io.File(java.io.File(dir, "vault"), t.vault).readBytes()
        return kotlinx.coroutines.runBlocking { singleWrite.withLock { writeBackLocked(dir, data, mode) } }
    }

    /** The changes in [dir] (its own record's vault, else the only vault file there). */
    private fun pendingData(dir: java.io.File): ByteArray? =
        (readTarget(dir)?.vault ?: vaultIn(dir))?.let { java.io.File(java.io.File(dir, "vault"), it).readBytes() }

    override fun pendingSingleWrites(): List<PendingSingleUi> {
        val open = singleDir
        return singleBase().listFiles().orEmpty()
            .filter { it.isDirectory && it != open && it.name !in busyDirs }
            .sortedBy { it.lastModified() }
            .mapNotNull { dir ->
                // Only what holds no unsaved changes goes; an unreadable record does not lose them.
                if (!tempSuite(dir).archiveUnsaved()) {
                    dir.deleteRecursively()
                    return@mapNotNull null
                }
                val t = readTarget(dir)
                // A GhulDocs vault: only written while it is open (see closeSingle); only giving up.
                if (isDocsDir(dir, t)) {
                    return@mapNotNull PendingSingleUi(dir.name, t?.ref ?: SuiteFileRef("", vaultIn(dir) ?: "GhulDocs.ghldocs"), docs = true)
                }
                if (t == null) {
                    // Which file is unknown: only "save as" (named like the vault file) or give up.
                    val v = vaultIn(dir)
                    if (v == null) dir.deleteRecursively()
                    return@mapNotNull v?.let { PendingSingleUi(dir.name, SuiteFileRef("", it), targetKnown = false) }
                }
                val now = kotlinx.coroutines.runBlocking { readSuiteFile(t.ref) }
                // Already in the file (written, but its record was not updated): done.
                if (now != null && pendingData(dir)?.contentEquals(now) == true) {
                    dir.deleteRecursively()
                    return@mapNotNull null
                }
                PendingSingleUi(
                    key = dir.name,
                    file = t.ref,
                    changedElsewhere = now != null && t.hash.isNotEmpty() && sha(now) != t.hash,
                    fileIntact = now != null && runCatching { inspectContainer(now) }.isSuccess,
                    missing = now == null,
                )
            }
    }

    private fun pendingDir(key: String): java.io.File? =
        java.io.File(singleBase(), key).takeIf { key.isNotBlank() && it.name == key && it.isDirectory }

    override suspend fun retrySingleWrite(key: String, mode: SingleWrite): SingleSaveUi {
        val dir = pendingDir(key) ?: return SingleSaveUi.SAVED
        if (io { isDocsDir(dir) }) return SingleSaveUi.FAILED
        val r = io { writePending(dir, mode = mode) }
        if (r == SingleSaveUi.SAVED) io { dir.deleteRecursively() }
        tick()
        return r
    }

    override suspend fun saveSingleWriteAs(key: String, ref: SuiteFileRef): Boolean {
        val dir = pendingDir(key) ?: return false
        if (io { isDocsDir(dir) }) return false
        val data = io { pendingData(dir) } ?: return false
        val ok = singleWrite.withLock { writeSuiteFile(ref, data) }
        if (ok) io { dir.deleteRecursively() }
        tick()
        return ok
    }

    override fun discardSingleWrite(key: String) {
        pendingDir(key)?.deleteRecursively()
        tick()
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
        if (single != null) {
            closeSingle()
            return
        }
        // F92: returns at once, also while a slow call still holds the open suite (the main
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
            )
        }
    }

    private fun FactorProofUi.core() = de.danerd.ghulsuite.core.FactorProof(password, pin, keyfile)

    override suspend fun addSecurityKey(proof: FactorProofUi, pin: String, label: String, onTouchNeeded: () -> Unit) =
        withKey { t -> suite.addSecurityKey(proof.core(), t.takeIf { proof.pin != null }, t, Touch(onTouchNeeded), pin, label) }

    override suspend fun addKeyfile(proof: FactorProofUi, keyfile: ByteArray, label: String, onTouchNeeded: () -> Unit) =
        maybeWithKey(proof.pin) { t -> suite.addKeyfile(proof.core(), t, t?.let { Touch(onTouchNeeded) }, keyfile, label) }

    override suspend fun newRecoveryCode(proof: FactorProofUi, label: String, onTouchNeeded: () -> Unit): String =
        maybeWithKey(proof.pin) { t -> suite.newRecoveryCode(proof.core(), t, t?.let { Touch(onTouchNeeded) }, label) }

    override suspend fun removeSlot(proof: FactorProofUi, id: String, onTouchNeeded: () -> Unit) {
        maybeWithKey(proof.pin) { t -> suite.removeSlot(proof.core(), t, t?.let { Touch(onTouchNeeded) }, id.hexToBytes()) }
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

    override suspend fun pwnedCount(password: String): Int = io { de.danerd.ghulsuite.core.pwnedCount(password).toInt() }
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
