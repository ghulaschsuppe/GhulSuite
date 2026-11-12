// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.core.platformSecurityKey
import de.danerd.ghulsuite.core.platformSecurityKeyAvailable
import de.danerd.ghulsuite.model.AutofillStatus
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.NetBackend
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

/** Version shown in the app (the MSI version is purely numeric). */
const val DESKTOP_VERSION = "0.5.0-dev"

/**
 * Where GhulSuite keeps its files on this computer.
 *
 * Portable (a file named `portable` next to GhulSuite.exe, as in the ZIP): everything stays in
 * the folder `GhulSuite-Daten` next to the program, e.g. on the USB stick – nothing on the PC.
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

    /** Decrypted copies handed to other programs; emptied when locking and on start. */
    val shareDir: File = File(System.getProperty("java.io.tmpdir"), "GhulSuite-Weitergabe")
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

/** Windows (and other desktops): suite folder on disk, security key through the system API. */
class DesktopVaultBackend(private val window: () -> java.awt.Window?) :
    CoreVaultBackend(File(DesktopPaths.dataDir, "local").absolutePath) {
    private val device = PropertiesStore(File(DesktopPaths.dataDir, "device.properties"))
    override val settings: Settings = Settings(PropertiesStore(File(DesktopPaths.dataDir, "settings.properties")))
    override val net: NetBackend = DesktopNetBackend { suite }
    override val appVersion: String = DESKTOP_VERSION + if (DesktopPaths.portable) " (portabel)" else ""
    override val keyPinInApp: Boolean = false
    override val isDesktop: Boolean = true
    private val timer = Timer("clipboard", true)

    init {
        DesktopPaths.shareDir.deleteRecursively()
    }

    override fun storage(): VaultStorage {
        val folder = device.getString("folder") ?: File(DesktopPaths.dataDir, "suite").absolutePath
        return localStorage(folder)
    }

    override fun devicePref(key: String): String? = device.getString(key)

    override fun setDevicePref(key: String, value: String) = device.putString(key, value)

    override fun kdfMaxMib(): Int {
        val os = java.lang.management.ManagementFactory.getOperatingSystemMXBean()
        val total = (os as? com.sun.management.OperatingSystemMXBean)?.totalMemorySize ?: (4L shl 30)
        return (total / (16L * 1024 * 1024)).toInt()
    }

    override fun useStorage(folder: FolderRef?) {
        lock()
        if (folder == null) device.remove("folder") else device.putString("folder", folder.uri)
        reopen()
    }

    override fun lock() {
        DesktopPaths.shareDir.deleteRecursively()
        super.lock()
    }

    override suspend fun writeToFolder(folder: FolderRef, name: String, data: ByteArray, keep: Int) = withContext(Dispatchers.IO) {
        val dir = File(folder.uri)
        dir.mkdirs()
        File(dir, name).writeBytes(data)
        dir.listFiles { f -> f.name.startsWith(name.substringBefore('-') + "-") && f.name.endsWith(".zip") }
            ?.sortedByDescending { it.name }?.drop(keep)?.forEach { it.delete() }
        Unit
    }

    override fun crashReport(): String? = File(DesktopPaths.dataDir, "absturz.txt").takeIf { it.exists() }?.readText()

    override fun panicLock() = lock()

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

    // --- not on the desktop (yet): quick unlock, autofill ---------------------------------------

    override fun biometricAvailable(): Boolean = false
    override fun quickUnlockReady(): QuickUnlockReady = QuickUnlockReady(false, false)
    override suspend fun enableBiometricUnlock() = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override suspend fun enablePinUnlock(pin: String) = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override fun disableQuickUnlock() {}
    override fun disableQuickUnlockMethod(bio: Boolean) {}
    override suspend fun unlockWithBiometric() = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override suspend fun unlockWithPin(pin: String) = throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
    override fun autofillStatus(): AutofillStatus = AutofillStatus.UNSUPPORTED
    override fun openAutofillSettings() {}

    // --- tools --------------------------------------------------------------------------------

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

/** Desktop: decrypted files are opened with the program Windows uses for them. */
class DesktopNetBackend(suite: () -> GhulSuite) : CoreNetBackend(suite) {
    override suspend fun shareDriveToApps(files: List<DriveItemUi>) {
        val written = withContext(Dispatchers.IO) {
            val dir = DesktopPaths.shareDir.apply { mkdirs() }
            val used = mutableSetOf<String>()
            files.filter { !it.isDir }.map { f ->
                File(dir, safeFileName(f.name, used)).also { target -> writeDriveFile(f.id, target) }
            }
        }
        if (written.isEmpty()) return
        val desktop = java.awt.Desktop.getDesktop()
        // One file: open it; several: show the folder.
        if (written.size == 1) desktop.open(written[0]) else desktop.open(DesktopPaths.shareDir)
    }

    // Mail notifications while locked need a background service; on the desktop not yet.
    override fun mailNotificationsOn(accountId: String): Boolean = false

    override suspend fun setMailNotifications(accountId: String, on: Boolean) {}
}
