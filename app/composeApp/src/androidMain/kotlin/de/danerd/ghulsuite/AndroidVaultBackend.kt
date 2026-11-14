// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.backupPrefix
import android.app.Activity
import android.app.ActivityManager
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import android.provider.Settings as AndroidSettings
import android.view.autofill.AutofillManager
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.fidoGetInfo
import de.danerd.ghulsuite.core.fidoSetPin
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.AutofillStatus
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.NetBackend
import de.danerd.ghulsuite.model.QuickUnlockReady
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class PrefsStore(context: Context) : KeyValueStore {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
    override fun getInt(key: String, default: Int) = prefs.getInt(key, default)
    override fun putInt(key: String, value: Int) = prefs.edit().putInt(key, value).apply()
    override fun getBoolean(key: String, default: Boolean) = prefs.getBoolean(key, default)
    override fun putBoolean(key: String, value: Boolean) = prefs.edit().putBoolean(key, value).apply()
    override fun getString(key: String, default: String) = prefs.getString(key, default) ?: default
    override fun putString(key: String, value: String) = prefs.edit().putString(key, value).apply()
}

/** Android: storage via SAF, security key via USB, quick unlock via Keystore, autofill. */
/** How long unlocking waits for a security key to be plugged in. */
private const val KEY_WAIT_MS = 60_000L

class AndroidVaultBackend(private val context: Context, private val activity: () -> Activity?) :
    CoreVaultBackend(File(context.filesDir, "local").absolutePath) {
    private val keys = UsbSecurityKeys(context)
    private val prefs = context.getSharedPreferences("storage", Context.MODE_PRIVATE)
    private val main = Handler(Looper.getMainLooper())

    override val settings: Settings = Settings(PrefsStore(context))
    private val quick = AndroidQuickUnlock(context, { suite }, settings, activity)
    override val net: NetBackend = AndroidNetBackend(context) { suite }
    override val cal: de.danerd.ghulsuite.model.CalBackend = AndroidCalBackend(context) { suite }
    override val appVersion: String =
        runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull() ?: "?"

    init {
        // Test data of versions before 0.3 used an older format; it is not migrated.
        File(context.filesDir, "vault").deleteRecursively()
        File(context.cacheDir, "shared").deleteRecursively()
    }

    override fun storage(): VaultStorage {
        val uri = prefs.getString("folder_uri", null)
        return if (uri == null) {
            localStorage(File(context.filesDir, "suite").absolutePath)
        } else {
            SafStorage(context, Uri.parse(uri))
        }
    }

    override fun devicePref(key: String): String? = prefs.getString(key, null)

    override fun setDevicePref(key: String, value: String) {
        prefs.edit().putString(key, value).apply()
    }

    override fun kdfMaxMib(): Int {
        val am = context.getSystemService(ActivityManager::class.java)
        val info = ActivityManager.MemoryInfo().also { am.getMemoryInfo(it) }
        return (info.totalMem / (16L * 1024 * 1024)).toInt()
    }

    override fun onFullUnlock() = quick.onFullUnlock()

    override fun onProtectionChanged() = quick.disableAll()

    override fun useStorage(folder: FolderRef?) {
        lock()
        if (folder == null) {
            prefs.edit().remove("folder_uri").apply()
        } else {
            val uri = Uri.parse(folder.uri)
            runCatching {
                context.contentResolver.takePersistableUriPermission(
                    uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
                )
            }
            prefs.edit().putString("folder_uri", folder.uri).apply()
        }
        reopen()
    }

    override fun lock() {
        // Decrypted copies handed to other apps and a copied password do not outlive the session.
        File(context.cacheDir, "shared").deleteRecursively()
        clearOwnClip()
        super.lock()
    }

    /**
     * Panic lock: locks the app and removes fingerprint and app PIN, so only the master password
     * opens it again. Mail notifications and calendar reminders keep working: the panic lock
     * closes the app, it does not switch off its mail reception.
     */
    override fun panicLock() {
        lock()
        quick.disableAll()
    }

    // --- key access -----------------------------------------------------------------------

    private val keyWaiting = kotlinx.coroutines.flow.MutableStateFlow(false)
    override val waitingForKey: kotlinx.coroutines.flow.StateFlow<Boolean> = keyWaiting

    override suspend fun <T> withKey(block: (HidTransport) -> T): T = withUsbKey { t -> block(t) }

    /** Finds a FIDO key, asks for USB permission and runs [block] on an IO thread. */
    private suspend fun <T> withUsbKey(block: (UsbHidTransport) -> T): T {
        var devices = keys.candidates()
        if (devices.isEmpty()) {
            // No key plugged in yet: wait for it (up to a minute) instead of failing at once.
            keyWaiting.value = true
            try {
                devices = kotlinx.coroutines.withTimeoutOrNull(KEY_WAIT_MS) {
                    var found = keys.candidates()
                    while (found.isEmpty()) {
                        kotlinx.coroutines.delay(400)
                        found = keys.candidates()
                    }
                    kotlinx.coroutines.delay(300) // let the key finish starting up
                    found
                } ?: emptyList()
            } finally {
                keyWaiting.value = false
            }
            if (devices.isEmpty()) throw VaultException(VaultErrorKind.NO_KEY)
        }
        var last: VaultException? = null
        for (device in devices) {
            if (!keys.ensurePermission(device)) {
                last = VaultException(VaultErrorKind.PERMISSION_DENIED, UsbSecurityKeys.describe(device))
                continue
            }
            try {
                return withContext(Dispatchers.IO) {
                    val transport = try {
                        keys.open(device)
                    } catch (e: KeyException) {
                        throw VaultException(
                            if (e.kind == KeyErrorKind.NOT_A_FIDO_KEY) VaultErrorKind.NOT_A_FIDO_KEY else VaultErrorKind.TRANSPORT,
                            e.detail,
                        )
                    }
                    transport.use { t -> mapped { block(t) } }
                }
            } catch (e: VaultException) {
                if (e.kind != VaultErrorKind.NOT_A_FIDO_KEY) throw e
                last = e
            }
        }
        throw last ?: VaultException(VaultErrorKind.NO_KEY)
    }

    override suspend fun keyInfo(): KeyStatus = withUsbKey { t ->
        val i = fidoGetInfo(t)
        KeyStatus(UsbSecurityKeys.describe(t.device), i.pinSet, i.supportsHmacSecret, (i.minPinLength ?: 4u).toInt())
    }

    override suspend fun setKeyPin(newPin: String) = withUsbKey { t -> fidoSetPin(t, newPin) }

    // --- quick unlock -------------------------------------------------------------------------

    override fun biometricAvailable(): Boolean = quick.biometricAvailable()

    override fun quickUnlockReady(): QuickUnlockReady = if (exists()) quick.ready() else QuickUnlockReady(false, false)

    private fun bioTexts() = Triple(
        context.getString(R.string.bio_title),
        context.getString(R.string.bio_subtitle),
        context.getString(R.string.bio_cancel),
    )

    override suspend fun enableBiometricUnlock() {
        val (t, s, c) = bioTexts()
        quick.enableBiometric(t, s, c)
    }

    override suspend fun enablePinUnlock(pin: String) = quick.enablePin(pin)

    override fun disableQuickUnlock() = quick.disableAll()

    override fun disableQuickUnlockMethod(bio: Boolean) =
        quick.disable(if (bio) AndroidQuickUnlock.BIO else AndroidQuickUnlock.PIN)

    override suspend fun unlockWithBiometric() {
        val (t, s, c) = bioTexts()
        quick.unlockBiometric(t, s, c)
        unlockedState.value = true
    }

    override suspend fun unlockWithPin(pin: String) {
        quick.unlockPin(pin)
        unlockedState.value = true
    }

    // --- autofill ------------------------------------------------------------------------------

    override fun autofillStatus(): AutofillStatus {
        val am = context.getSystemService(AutofillManager::class.java) ?: return AutofillStatus.UNSUPPORTED
        if (!am.isAutofillSupported) return AutofillStatus.UNSUPPORTED
        return if (am.hasEnabledAutofillServices()) AutofillStatus.ON else AutofillStatus.OFF
    }

    override fun openAutofillSettings() {
        val intent = Intent(AndroidSettings.ACTION_REQUEST_SET_AUTOFILL_SERVICE)
            .setData(Uri.parse("package:${context.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ExternalActivity.started()
        runCatching { (activity() ?: context).startActivity(intent) }.onFailure { ExternalActivity.finished() }
    }

    /** Logins for a website or app (with its signing certificate) for the autofill service. */
    suspend fun autofillLogins(domain: String?, pkg: String?, cert: String?) = io { suite.autofillLogins(domain, pkg, cert) }

    suspend fun allLogins() = io { suite.autofillLogins(null, null, null) }

    suspend fun linkAppToLogin(vaultId: String, id: ByteArray, pkg: String, cert: String?) =
        io { suite.linkAppToLogin(vaultId, id, pkg, cert) }

    suspend fun autofillSave(domain: String?, pkg: String?, cert: String?, username: String, password: String) =
        io { suite.autofillSave(domain, pkg, cert, username, password) }

    // --- tools --------------------------------------------------------------------------------

    override fun printEmergencySheet(code: String, text: EmergencySheetText) {
        val act = activity() ?: return
        EmergencySheet.print(act, code, text)
    }

    /** Label of the last copied secret, until it was cleared. */
    @Volatile private var copiedLabel: String? = null

    /** Clears the clipboard if it still holds what GhulSuite copied (not what the user copied since). */
    private fun clearOwnClip() {
        val label = copiedLabel ?: return
        copiedLabel = null
        val cm = context.getSystemService(ClipboardManager::class.java)
        // In the background Android may hide the clipboard: then clear to be safe.
        val current = runCatching { cm.primaryClipDescription?.label?.toString() }.getOrNull()
        if (current == null || current == label) runCatching { cm.clearPrimaryClip() }
    }

    override fun copySensitive(label: String, text: String) {
        val cm = context.getSystemService(ClipboardManager::class.java)
        val tagged = "$label \u2022 GhulSuite ${System.nanoTime() % 100000}"
        val clip = ClipData.newPlainText(tagged, text)
        clip.description.extras = PersistableBundle().apply {
            if (Build.VERSION.SDK_INT >= 33) {
                putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
            } else {
                putBoolean("android.content.extra.IS_SENSITIVE", true)
            }
        }
        cm.setPrimaryClip(clip)
        copiedLabel = tagged
        val seconds = settings.clipboardClearSeconds
        if (seconds > 0) {
            main.postDelayed({ if (copiedLabel == tagged) clearOwnClip() }, seconds * 1000L)
        }
    }

    override suspend fun writeToFolder(folder: FolderRef, name: String, data: ByteArray, keep: Int) = withContext(Dispatchers.IO) {
        val store = SafStorage(context, android.net.Uri.parse(folder.uri))
        store.write(name, data)
        // Only the newest backups stay.
        val old = store.list().filter { it.startsWith(backupPrefix(name)) && it.endsWith(".zip") }.sortedDescending().drop(keep)
        for (o in old) runCatching { store.delete(o) }
    }

    override fun crashReport(): String? = CrashLog.pending(context)
}
