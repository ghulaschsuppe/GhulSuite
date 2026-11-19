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
     * Decrypted copies handed to other programs; emptied when locking and on start. Portable:
     * inside the data folder, so nothing lands on the PC (also not in its temp folder).
     */
    val shareDir: File =
        if (portable) File(dataDir, "Weitergabe") else File(System.getProperty("java.io.tmpdir"), "GhulSuite-Weitergabe")
}

/**
 * B10: removes the decrypted copies handed to other programs. Each file is overwritten with
 * zeros first, then deleted. Windows refuses both while a program still holds the file open:
 * then it is tried again every 30 seconds, when the app closes and on the next start, until the
 * folder is empty.
 */
object ShareFiles {
    private val timer = Timer("share-wipe", true)
    private var retrying = false

    init {
        Runtime.getRuntime().addShutdownHook(Thread { wipeOnce() })
    }

    /** Wipes the folder now and keeps retrying in the background for files still in use. */
    @Synchronized
    fun wipe() {
        if (wipeOnce() || retrying) return
        retrying = true
        timer.schedule(30_000L, 30_000L) {
            if (wipeOnce()) {
                synchronized(this@ShareFiles) { retrying = false }
                cancel()
            }
        }
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
    fun input(f: File): SuiteInput? = runCatching { f.takeIf { it.isFile }?.let { SuiteInput(it.inputStream(), it.length()) } }.getOrNull()

    /**
     * Written next to the file first (`<name>.ghltmp`), read back from there, and only then moved
     * over the old file in one step: a pulled stick or a full disk leaves the old file as it was.
     */
    fun writeReplacing(target: File, write: (java.io.OutputStream) -> Unit, check: (SuiteInput) -> Boolean): Boolean {
        val tmp = File(target.absoluteFile.parentFile, target.name + ".ghltmp")
        var done = false
        try {
            java.io.FileOutputStream(tmp).use { out ->
                val buffered = java.io.BufferedOutputStream(out, 1 shl 20)
                write(buffered)
                buffered.flush()
                out.fd.sync()
            }
            if (!java.io.FileInputStream(tmp).use { check(SuiteInput(it, tmp.length())) }) return false
            try {
                java.nio.file.Files.move(tmp.toPath(), target.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE)
            } catch (e: java.nio.file.AtomicMoveNotSupportedException) {
                java.nio.file.Files.move(tmp.toPath(), target.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
            }
            done = true
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
        tmp.outputStream().use { props.store(it, "GhulSuite") }
        if (!tmp.renameTo(file)) {
            file.delete()
            tmp.renameTo(file)
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

    override fun writeSuiteStreamed(
        ref: SuiteFileRef,
        created: Boolean,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
    ): Boolean = SuiteFiles.writeReplacing(File(ref.id), write, check)

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
        super.lock()
    }

    override fun crashReport(): String? = File(DesktopPaths.dataDir, "absturz.txt").takeIf { it.exists() }?.readText()

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
    override suspend fun enableBiometricUnlock() = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override suspend fun enablePinUnlock(pin: String) = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override fun disableQuickUnlock() {}
    override fun disableQuickUnlockMethod(bio: Boolean) {}
    override suspend fun unlockWithBiometric() = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override suspend fun unlockWithPin(pin: String) = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)

    // --- tools --------------------------------------------------------------------------------

    /** Windows has no share sheet: the file opens in the program registered for it. */
    override suspend fun shareToApps(name: String, mime: String, data: ByteArray): Boolean =
        shareStreamToApps(name, mime) { it.write(data) }

    /**
     * The same, written piece by piece into the copy (a document of GhulDocs). A write that fails
     * or is cancelled wipes the half copy at once and opens nothing.
     */
    override suspend fun shareStreamToApps(name: String, mime: String, write: suspend (de.danerd.ghulsuite.model.ByteSink) -> Unit): Boolean {
        val f = withContext(Dispatchers.IO) {
            val dir = DesktopPaths.shareDir.apply { mkdirs() }
            File(dir, safeFileName(name, mutableSetOf()))
        }
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
        return runCatching { java.awt.Desktop.getDesktop().open(f) }.isSuccess
    }

    override fun printEmergencySheet(code: String, text: EmergencySheetText) = DesktopEmergencySheet.print(code, text)

    override fun copySensitive(label: String, text: String) {
        val seconds = settings.clipboardClearSeconds
        val sequence = WinNative.copyPrivate(text)
        if (sequence != null) {
            if (seconds > 0) timer.schedule(seconds * 1000L) { WinNative.clearIfUnchanged(sequence) }
            return
        }
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
