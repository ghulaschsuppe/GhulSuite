// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.core.platformSecurityKey
import de.danerd.ghulsuite.core.platformSecurityKeyAvailable
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.QuickUnlockReady
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import java.awt.Toolkit
import java.awt.datatransfer.StringSelection
import java.io.File
import java.util.Properties
import java.util.Timer
import kotlin.concurrent.schedule
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** A vault file opened on its own is read as a whole: at most as large as the core reads one. */
private const val MAX_VAULT_FILE_BYTES = 256L * 1024 * 1024

/** Version shown in the app (the MSI version is purely numeric). */
const val DESKTOP_VERSION = BuildInfo.VERSION

/**
 * Where GhulSuite keeps its files on this computer.
 *
 * Portable (a file named `portable` next to GhulSuite.exe, as in the ZIP): everything stays in
 * the folder `GhulSuite-Daten` next to the program, e.g. on the USB stick, nothing on the PC.
 * Installed: `%APPDATA%\GhulSuite` (elsewhere `~/.local/share/GhulSuite`).
 */
object DesktopPaths {
    /** Folder of GhulSuite.exe (set by the launcher), or null when started from Gradle. */
    val programDir: File? = System.getProperty("jpackage.app-path")?.let { File(it).absoluteFile.parentFile }

    val portable: Boolean = programDir?.let { File(it, "portable").exists() } == true

    val dataDir: File = when {
        portable -> File(programDir, "GhulSuite-Daten")
        System.getenv("APPDATA") != null -> File(System.getenv("APPDATA"), "GhulSuite")
        else -> File(System.getProperty("user.home"), ".local/share/GhulSuite")
    }.also { it.mkdirs() }

    /**
     * Decrypted copies handed to other programs; emptied when locking and on start. Always in the
     * temp folder of this PC, also in the portable version (M16): a copy on the stick would go
     * with the stick, and flash memory keeps old blocks of a deleted file. The portable version
     * says so next to "Open with app".
     */
    val shareDir: File = File(System.getProperty("java.io.tmpdir"), "GhulSuite-Weitergabe")

    /** Where older portable versions put those copies (on the stick); emptied like [shareDir]. */
    val oldShareDir: File? = if (portable) File(dataDir, "Weitergabe") else null
}

/** The text of a Zone.Identifier stream: "from the internet" (zone 3), the Mark of the Web. */
internal const val ZONE_INTERNET = "[ZoneTransfer]\r\nZoneId=3\r\n"

/**
 * H4: marks a copy handed to another program as coming from the internet, so Windows
 * (SmartScreen, Office's protected view) treats it like a download and warns before anything in
 * it runs. The mark is an alternate data stream (NTFS); where there is none, nothing happens.
 */
internal fun markFromInternet(f: File): Boolean = runCatching {
    java.io.FileOutputStream(f.path + ":Zone.Identifier").use { it.write(ZONE_INTERNET.encodeToByteArray()) }
}.isSuccess

/** The endings Windows runs without asking (PATHEXT, e.g. ".COM;.EXE;.BAT;.CMD;.VBS;.JS"). */
internal fun pathExtEndings(pathExt: String?): List<String> =
    pathExt.orEmpty().split(';').map { it.trim().trimStart('.').lowercase() }.filter { it.isNotEmpty() }

/**
 * B10: removes the decrypted copies handed to other programs. Each file is overwritten with
 * zeros first, then deleted. Windows refuses both while a program still holds the file open:
 * then it is tried again every 30 seconds, when the app closes and on the next start, until the
 * folder is empty.
 */
object ShareFiles {
    private val timer = Timer("share-wipe", true)
    private var retrying = false
    /**
     * The copies that were there when [wipe] was called: only these are tried again (a copy made
     * after the next unlock is not touched while it is written or open in another program).
     */
    private val pending = mutableSetOf<File>()

    init {
        Runtime.getRuntime().addShutdownHook(Thread { wipeOnce() })
    }

    /** Wipes the folder now and keeps retrying in the background for files still in use. */
    @Synchronized
    fun wipe() {
        for (dir in listOfNotNull(DesktopPaths.shareDir, DesktopPaths.oldShareDir)) {
            if (dir.exists()) pending += dir.walkBottomUp().filter { it.isFile }
        }
        if (wipePending() || retrying) return
        retrying = true
        timer.schedule(30_000L, 30_000L) {
            val done = synchronized(this@ShareFiles) { wipePending().also { if (it) retrying = false } }
            if (done) cancel()
        }
    }

    /** Wipes what was noted; true when none of it is left. */
    @Synchronized
    private fun wipePending(): Boolean {
        pending.removeAll { f ->
            if (f.exists()) wipeFile(f)
            !f.exists()
        }
        // Empty folders of earlier copies go as well.
        for (dir in listOfNotNull(DesktopPaths.shareDir, DesktopPaths.oldShareDir)) {
            dir.takeIf { it.exists() }?.walkBottomUp()?.filter { it.isDirectory }?.forEach { it.delete() }
        }
        return pending.isEmpty()
    }

    /** Overwrites one file with zeros and deletes it (a copy that was not finished). */
    fun wipeFile(f: File) {
        runCatching {
            java.io.RandomAccessFile(f, "rw").use { raf ->
                val zeros = ByteArray(64 * 1024)
                var left = raf.length()
                while (left > 0) {
                    val n = minOf(left, zeros.size.toLong()).toInt()
                    raf.write(zeros, 0, n)
                    left -= n
                }
                raf.fd.sync()
            }
        }
        f.delete()
    }

    /** True when nothing is left. */
    private fun wipeOnce(): Boolean {
        val dir = DesktopPaths.shareDir
        if (!dir.exists()) return true
        dir.walkBottomUp().forEach { f ->
            if (f.isFile) {
                runCatching {
                    java.io.RandomAccessFile(f, "rw").use { raf ->
                        val zeros = ByteArray(64 * 1024)
                        var left = raf.length()
                        while (left > 0) {
                            val n = minOf(left, zeros.size.toLong()).toInt()
                            raf.write(zeros, 0, n)
                            left -= n
                        }
                        raf.fd.sync()
                    }
                }
            }
            f.delete()
        }
        return !dir.exists()
    }
}

/** Suite files on a file system (Windows; also the desktop tests). */
object SuiteFiles {
    fun input(f: File): SuiteInput? = runCatching { resolve(f).takeIf { it.isFile }?.let { SuiteInput(it.inputStream(), it.length()) } }.getOrNull()

    /**
     * Deletes a suite file (an old place), [wipe]: overwritten with zeros first. Review M-b: a
     * folder or drive that is not there (a stick pulled) is UNREACHABLE, not "deleted".
     */
    fun delete(f: File, wipe: Boolean = false): SuiteDocDelete {
        val dir = f.absoluteFile.parentFile
        if (dir == null || !dir.isDirectory) return SuiteDocDelete.UNREACHABLE
        if (!f.exists()) return SuiteDocDelete.GONE
        if (wipe) ShareFiles.wipeFile(f) else f.delete()
        return if (f.exists()) SuiteDocDelete.FAILED else SuiteDocDelete.DELETED
    }

    /** "THERE", "GONE" (its folder is there) or "UNREACHABLE" (its folder or drive is not). */
    fun presence(f: File): String = when {
        f.absoluteFile.parentFile?.isDirectory != true -> "UNREACHABLE"
        f.isFile -> "THERE"
        else -> "GONE"
    }

    /** Whether [a] and [b] are one file: the same file where both are there, else the same path ignoring case. */
    fun sameFile(a: File, b: File): Boolean {
        if (a.exists() && b.exists()) return java.nio.file.Files.isSameFile(a.toPath(), b.toPath())
        return a.canonicalPath.equals(b.canonicalPath, ignoreCase = true) || a.absolutePath.equals(b.absolutePath, ignoreCase = true)
    }

    /** What a cut off save left next to [f]: the moved aside old file and the new one not moved in. */
    fun leftovers(f: File): List<de.danerd.ghulsuite.model.SuiteFileRef> =
        listOf(OLD_EXT, ".ghltmp").map { File(f.absoluteFile.parentFile, f.name + it) }.filter { it.isFile }
            .map { de.danerd.ghulsuite.model.SuiteFileRef(it.absolutePath, it.name) }

    /**
     * N3: the save before was cut off between its two moves (see [writeReplacing]): only
     * "<name>.ghlold" is there, complete and not wiped yet. It takes its name back, so the file
     * opens here and is found on another PC; where it cannot be renamed (read only) it is read
     * as it is. A picked "<name>.ghlold" without "<name>" is the same case.
     */
    fun resolve(f: File): File {
        val base = if (f.name.endsWith(OLD_EXT) && f.name.length > OLD_EXT.length) File(f.absoluteFile.parentFile, f.name.removeSuffix(OLD_EXT)) else f
        val old = File(base.absoluteFile.parentFile, base.name + OLD_EXT)
        if (base.exists() || !old.isFile) return if (f == base || base.exists()) base else f
        return if (old.renameTo(base)) base else old
    }

    private const val OLD_EXT = ".ghlold"

    /**
     * Written next to the file first (`<name>.ghltmp`), read back from there, and only then moved
     * over the old file in one step: a pulled stick or a full disk leaves the old file as it was.
     *
     * [wipeOld] (G2-M2, after a change of the protection): the old file holds the state under
     * the protection before; deleted, it stays in the free space of a stick and can be carved
     * back. It is moved aside (`<name>.ghlold`) instead, the new one takes the name, and only
     * then the old one is overwritten with zeros and deleted. Is "<name>" missing next to a
     * "<name>.ghlold" (the power went between the two moves), the old one takes its name back
     * first: the new file is only ever the checked `.ghltmp`, the old one only goes once the new
     * one is in place. N3: a "<name>.ghlold" left next to "<name>" (a wipe that was cut off) is
     * overwritten with zeros and deleted on the next save there, with [wipeOld] or not.
     * [onWipe] is called before the zeros are written (that can take minutes on a stick).
     */
    fun writeReplacing(
        target: File,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
        wipeOld: Boolean = false,
        onWipe: () -> Unit = {},
    ): Boolean {
        val tmp = File(target.absoluteFile.parentFile, target.name + ".ghltmp")
        val old = File(target.absoluteFile.parentFile, target.name + ".ghlold")
        if (!target.exists() && old.exists()) old.renameTo(target)
        var done = false
        try {
            java.io.FileOutputStream(tmp).use { out ->
                val buffered = java.io.BufferedOutputStream(out, 1 shl 20)
                write(buffered)
                buffered.flush()
                out.fd.sync()
            }
            if (!java.io.FileInputStream(tmp).use { check(SuiteInput(it, tmp.length())) }) return false
            if (wipeOld && target.exists()) {
                java.nio.file.Files.move(target.toPath(), old.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
                try {
                    java.nio.file.Files.move(tmp.toPath(), target.toPath())
                } catch (e: java.io.IOException) {
                    // The old one back in its place, nothing lost.
                    runCatching { java.nio.file.Files.move(old.toPath(), target.toPath()) }
                    throw e
                }
                done = true
                onWipe()
                ShareFiles.wipeFile(old)
                return true
            }
            try {
                java.nio.file.Files.move(tmp.toPath(), target.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE)
            } catch (e: java.nio.file.AtomicMoveNotSupportedException) {
                java.nio.file.Files.move(tmp.toPath(), target.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
            }
            done = true
            // A half wiped old file of an earlier save (the new one is in place by now).
            if (old.exists()) {
                onWipe()
                ShareFiles.wipeFile(old)
            }
            return true
        } finally {
            // No half-written temporary file next to the user's file.
            if (!done) tmp.delete()
        }
    }
}

/** Settings in a small properties file (written atomically). */
class PropertiesStore(private val file: File) : KeyValueStore {
    private val props = Properties().apply { if (file.exists()) runCatching { file.inputStream().use { load(it) } } }

    @Synchronized
    private fun save() {
        val tmp = File(file.parentFile, file.name + ".tmp")
        java.io.FileOutputStream(tmp).use { out ->
            props.store(out, "GhulSuite")
            out.fd.sync()
        }
        // One step (also on Windows, where renameTo fails over an existing file): the file is
        // never missing, also not when the power goes or a portable stick is pulled.
        try {
            java.nio.file.Files.move(tmp.toPath(), file.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE)
        } catch (e: java.nio.file.AtomicMoveNotSupportedException) {
            java.nio.file.Files.move(tmp.toPath(), file.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
        }
    }

    fun getString(key: String): String? = props.getProperty(key)

    override fun getString(key: String, default: String): String = props.getProperty(key) ?: default

    override fun putString(key: String, value: String) {
        props.setProperty(key, value)
        save()
    }

    fun remove(key: String) {
        props.remove(key)
        save()
    }

    override fun getInt(key: String, default: Int) = props.getProperty(key)?.toIntOrNull() ?: default
    override fun putInt(key: String, value: Int) = putString(key, value.toString())
    override fun getBoolean(key: String, default: Boolean) = props.getProperty(key)?.toBooleanStrictOrNull() ?: default
    override fun putBoolean(key: String, value: Boolean) = putString(key, value.toString())
}

/** Windows (and other desktops): the working copy in the app's data folder, security key through the system API. */
class DesktopVaultBackend(private val window: () -> java.awt.Window?) :
    CoreVaultBackend(File(DesktopPaths.dataDir, "local").absolutePath) {
    private val device = PropertiesStore(File(DesktopPaths.dataDir, "device.properties"))
    override val settings: Settings = Settings(PropertiesStore(File(DesktopPaths.dataDir, "settings.properties")))
    override val cal: de.danerd.ghulsuite.model.CalBackend = DesktopCalBackend(window) { suite }
    override val updater: de.danerd.ghulsuite.model.AppUpdater = DesktopUpdater()
    override val appVersion: String = DESKTOP_VERSION + if (DesktopPaths.portable) " (portabel)" else ""
    override val keyPinInApp: Boolean = false
    override val isDesktop: Boolean = true
    private val timer = Timer("clipboard", true)

    init {
        ShareFiles.wipe()
    }

    override fun storage(): VaultStorage = localStorage(File(DesktopPaths.dataDir, "suite").absolutePath)

    override suspend fun readSuiteFile(ref: SuiteFileRef): ByteArray? = withContext(Dispatchers.IO) {
        runCatching { File(ref.id).takeIf { it.isFile && it.length() <= MAX_VAULT_FILE_BYTES }?.readBytes() }.getOrNull()
    }

    override fun openSuiteInput(ref: SuiteFileRef): SuiteInput? = SuiteFiles.input(File(ref.id))

    override fun deleteSuiteDocument(ref: SuiteFileRef, wipe: Boolean): SuiteDocDelete = SuiteFiles.delete(File(ref.id), wipe)

    override fun samePlace(a: SuiteFileRef, b: SuiteFileRef): Boolean = SuiteFiles.sameFile(File(a.id), File(b.id))

    override fun placeReachable(ref: SuiteFileRef): Boolean = File(ref.id).absoluteFile.parentFile?.isDirectory == true

    override fun presence(ref: SuiteFileRef): Presence = SuiteFiles.presence(File(ref.id)).let { Presence.valueOf(it) }

    override fun leftoversOf(ref: SuiteFileRef): List<SuiteFileRef> = SuiteFiles.leftovers(File(ref.id))

    override fun whereIs(ref: SuiteFileRef): String = File(ref.id).absolutePath

    /** The room on the drive of the file's folder (0 from Java means: the folder is not there). */
    override fun freeSpaceAt(ref: SuiteFileRef): Long? =
        File(ref.id).absoluteFile.parentFile?.takeIf { it.isDirectory }?.usableSpace?.takeIf { it > 0 }

    override fun writeSuiteStreamed(
        ref: SuiteFileRef,
        created: Boolean,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
        wipeOld: Boolean,
        onWipe: () -> Unit,
    ): Boolean = SuiteFiles.writeReplacing(File(ref.id), write, check, wipeOld, onWipe)

    /** Only where no file is: moved without replacing (a file that came back meanwhile stays). */
    override suspend fun createSuiteFile(ref: SuiteFileRef, data: ByteArray): Boolean = withContext(Dispatchers.IO) {
        val target = File(ref.id)
        val tmp = File(target.parentFile, target.name + ".ghltmp")
        runCatching {
            java.io.FileOutputStream(tmp).use { it.write(data); it.fd.sync() }
            java.nio.file.Files.move(tmp.toPath(), target.toPath())
            target.readBytes().contentEquals(data)
        }.getOrElse {
            tmp.delete()
            false
        }
    }

    override fun devicePref(key: String): String? = device.getString(key)

    override fun setDevicePref(key: String, value: String) = device.putString(key, value)

    override fun kdfMaxMib(): Int {
        val os = java.lang.management.ManagementFactory.getOperatingSystemMXBean()
        val total = (os as? com.sun.management.OperatingSystemMXBean)?.totalMemorySize ?: (4L shl 30)
        return (total / (16L * 1024 * 1024)).toInt()
    }

    override fun lock() {
        ShareFiles.wipe()
        // A copied password does not outlive the session (also not the app: see the hook below).
        clearOwnClip()
        super.lock()
    }

    /** What GhulSuite put on the clipboard last (cleared when locking or quitting, if still there). */
    @Volatile private var copiedSequence: Int? = null
    @Volatile private var copiedText: String? = null

    private fun clearOwnClip() {
        copiedSequence?.let { WinNative.clearIfUnchanged(it) }
        copiedSequence = null
        copiedText?.let { text ->
            runCatching {
                val clipboard = Toolkit.getDefaultToolkit().systemClipboard
                if (clipboard.getData(java.awt.datatransfer.DataFlavor.stringFlavor) as? String == text) {
                    clipboard.setContents(StringSelection(""), null)
                }
            }
        }
        copiedText = null
    }

    init {
        Runtime.getRuntime().addShutdownHook(Thread { runCatching { clearOwnClip() } })
    }

    override fun crashReport(): String? = File(DesktopPaths.dataDir, "absturz.txt").takeIf { it.exists() }?.readText()

    override fun clearCrashReport() {
        File(DesktopPaths.dataDir, "absturz.txt").delete()
    }

    override fun panicLock() {
        // Nothing is written back first (a GhulDocs vault open on its own locks at once).
        panicking = true
        try {
            lock()
        } finally {
            panicking = false
        }
        cal.clearReminders()
    }

    // --- security key: Windows WebAuthn API with its own PIN and touch dialog --------------------

    override suspend fun <T> withKey(block: (HidTransport) -> T): T {
        if (!platformSecurityKeyAvailable()) {
            throw VaultException(VaultErrorKind.TRANSPORT, "Windows 11 or a current Windows 10 is needed for security keys.")
        }
        val transport = platformSecurityKey(WinNative.hwnd(window()).toULong())
        return withContext(Dispatchers.IO) { mapped { block(transport) } }
    }

    override suspend fun keyInfo(): KeyStatus = KeyStatus("Windows", pinSet = true, supportsHmacSecret = true, minPinLength = 4)

    override suspend fun setKeyPin(newPin: String) {
        // Windows sets the PIN itself (Settings → Accounts → Sign-in options → Security key).
        throw VaultException(VaultErrorKind.INVALID_INPUT, "Set the PIN in the Windows settings (Sign-in options → Security key).")
    }

    // --- not on the desktop (yet): quick unlock -----------------------------------------------

    override fun biometricAvailable(): Boolean = false
    override fun quickUnlockReady(): QuickUnlockReady = QuickUnlockReady(false, false)
    override suspend fun enableBiometricUnlock(proof: de.danerd.ghulsuite.model.FactorProofUi, hours: Int, onTouchNeeded: () -> Unit) =
        throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override suspend fun enablePinUnlock(proof: de.danerd.ghulsuite.model.FactorProofUi, pin: String, hours: Int, onTouchNeeded: () -> Unit) =
        throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override fun disableQuickUnlock() {}
    override fun disableQuickUnlockMethod(bio: Boolean) {}
    override suspend fun unlockWithBiometric() = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override suspend fun unlockWithPin(pin: String) = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)

    // --- tools --------------------------------------------------------------------------------

    override val openCopiesOnThisPc: Boolean get() = DesktopPaths.portable

    override val dataFolder: String get() = DesktopPaths.dataDir.absolutePath

    /** Programs, scripts and shortcuts (H4), and whatever this PC's PATHEXT lists. */
    override fun refusesToOpen(name: String): Boolean =
        de.danerd.ghulsuite.ui.isRunnableFile(name, pathExtEndings(System.getenv("PATHEXT")))

    /** Windows has no share sheet: the file opens in the program registered for it. */
    override suspend fun shareToApps(name: String, mime: String, data: ByteArray): Boolean =
        shareStreamToApps(name, mime) { it.write(data) }

    /**
     * The same, written piece by piece into the copy (a document of GhulDocs). A write that fails
     * or is cancelled wipes the half copy at once and opens nothing.
     */
    override suspend fun shareStreamToApps(name: String, mime: String, write: suspend (de.danerd.ghulsuite.model.ByteSink) -> Unit): Boolean {
        // Checked here as well, whoever calls: nothing that runs is handed to Windows to start.
        if (refusesToOpen(name)) throw VaultException(VaultErrorKind.INVALID_INPUT, "This kind of file is not opened with another program.")
        val f = withContext(Dispatchers.IO) {
            val dir = DesktopPaths.shareDir.apply { mkdirs() }
            File(dir, safeFileName(name, mutableSetOf()))
        }
        if (refusesToOpen(f.name)) throw VaultException(VaultErrorKind.INVALID_INPUT, "This kind of file is not opened with another program.")
        val out = withContext(Dispatchers.IO) { java.io.BufferedOutputStream(java.io.FileOutputStream(f), 1 shl 20) }
        var complete = false
        try {
            write(object : de.danerd.ghulsuite.model.ByteSink {
                override fun write(data: ByteArray) = out.write(data)
                override fun close() = out.close()
            })
            withContext(Dispatchers.IO) { out.close() }
            complete = true
        } finally {
            if (!complete) withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) {
                runCatching { out.close() }
                ShareFiles.wipeFile(f)
            }
        }
        // Without the Mark of the Web, Windows would not warn about anything in the copy: then it
        // is not opened (e.g. a temp folder on a drive without data streams).
        if (WinNative.isWindows && !withContext(Dispatchers.IO) { markFromInternet(f) }) {
            withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) { ShareFiles.wipeFile(f) }
            throw VaultException(VaultErrorKind.INVALID_INPUT, "the copy could not be marked as downloaded")
        }
        return runCatching { java.awt.Desktop.getDesktop().open(f) }.isSuccess
    }

    override fun printEmergencySheet(code: String, text: EmergencySheetText) = DesktopEmergencySheet.print(code, text)

    override fun copySensitive(label: String, text: String) {
        val seconds = settings.clipboardClearSeconds
        // Another program may hold the clipboard open for a moment: tried a few times before the
        // normal clipboard (which Windows keeps in its history) is used.
        var sequence: Int? = null
        for (attempt in 0 until 5) {
            sequence = WinNative.copyPrivate(text, window())
            if (sequence != null) break
            Thread.sleep(40)
        }
        if (sequence != null) {
            copiedSequence = sequence
            copiedText = null
            if (seconds > 0) timer.schedule(seconds * 1000L) { WinNative.clearIfUnchanged(sequence) }
            return
        }
        // r9 dieb N6: on Windows never the normal clipboard instead (its history and the cloud
        // would keep the secret): said, and copied again by the user.
        if (WinNative.isWindows) throw VaultException(VaultErrorKind.CLIPBOARD_BUSY)
        copiedSequence = null
        copiedText = text
        val clipboard = Toolkit.getDefaultToolkit().systemClipboard
        val selection = StringSelection(text)
        clipboard.setContents(selection, selection)
        if (seconds > 0) {
            timer.schedule(seconds * 1000L) {
                runCatching {
                    val now = clipboard.getData(java.awt.datatransfer.DataFlavor.stringFlavor) as? String
                    if (now == text) clipboard.setContents(StringSelection(""), null)
                }
            }
        }
    }
}
