// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

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
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.fidoGetInfo
import de.danerd.ghulsuite.core.fidoSetPin
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.KeyValueStore
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

/** A vault file opened on its own is read as a whole: at most as large as the core reads one. */
private const val MAX_VAULT_FILE_BYTES = 256 * 1024 * 1024

/** Android: the working copy in the app's own storage, security key via USB, quick unlock via Keystore. */
/** How long unlocking waits for a security key to be plugged in. */
private const val KEY_WAIT_MS = 60_000L

/** The provider of the device's storage, SD cards and USB sticks (document ids are paths). */
private const val EXTERNAL_STORAGE = "com.android.externalstorage.documents"

/** The document id of a suite file whose swap put the new file in place (see AndroidVaultBackend.recover). */
private const val SWAP_DONE = "suite_swap_done"

class AndroidVaultBackend(private val context: Context, private val activity: () -> Activity?) :
    CoreVaultBackend(File(context.filesDir, "local").absolutePath) {
    private val keys = UsbSecurityKeys(context)
    private val prefs = context.getSharedPreferences("storage", Context.MODE_PRIVATE)
    private val main = Handler(Looper.getMainLooper())

    override val settings: Settings = Settings(PrefsStore(context))
    // Quick unlock belongs to the own suite (not to a vault opened on its own).
    private val quick = AndroidQuickUnlock(context, { ownSuite }, settings, activity)
    override val cal: de.danerd.ghulsuite.model.CalBackend = AndroidCalBackend(context) { suite }
    override val updater: de.danerd.ghulsuite.model.AppUpdater = AndroidUpdater(context)
    override val appVersion: String =
        runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull() ?: "?"

    init {
        // Decrypted copies handed to other apps do not outlive the app (also after a crash).
        File(context.cacheDir, "shared").deleteRecursively()
    }

    override fun storage(): VaultStorage = localStorage(File(context.filesDir, "suite").absolutePath)

    override suspend fun readSuiteFile(ref: SuiteFileRef): ByteArray? = withContext(Dispatchers.IO) {
        runCatching {
            context.contentResolver.openInputStream(io(ref))?.use { input ->
                // A vault file is read as a whole: at most as large as the core reads one.
                val out = java.io.ByteArrayOutputStream()
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    if (out.size() > MAX_VAULT_FILE_BYTES) return@runCatching null
                }
                out.toByteArray()
            }
        }.getOrNull()
    }

    override fun openSuiteInput(ref: SuiteFileRef): SuiteInput? = openInputAt(io(ref))

    private fun openInputAt(uri: Uri): SuiteInput? = runCatching {
        val size = runCatching {
            context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.SIZE), null, null, null)?.use { c ->
                if (c.moveToFirst() && !c.isNull(0)) c.getLong(0) else 0L
            }
        }.getOrNull() ?: 0L
        context.contentResolver.openInputStream(uri)?.let { SuiteInput(it, size) }
    }.getOrNull()

    /**
     * Saving never leaves the place without a complete suite file when its folder is shared
     * (USB stick, SD card, the phone's storage; see [safeWrite]): the new file is written next to
     * the old one and checked, then the two are swapped.
     *
     * Without the folder the Storage Access Framework hands over the chosen document only, no
     * folder to write a temporary file into: the document is truncated and written ("wt"; the
     * provider, a USB stick or a cloud app, keeps its name and place), then read back. Until then the working copy is
     * the only complete state and stays marked as unsaved. The document is opened only when the
     * first byte comes (everything is sealed by then): a save that fails before, e.g. because the
     * suite was locked, leaves the file there as it was. A document made just for this that
     * could not be written completely is removed.
     */
    override fun writeSuiteStreamed(
        ref: SuiteFileRef,
        created: Boolean,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
    ): Boolean {
        safePlace(ref)?.let { return safeWrite(it, created, write, check) }
        val uri = Uri.parse(ref.id)
        var ok = false
        try {
            val lazy = LazyOutput { context.contentResolver.openOutputStream(uri, "wt") ?: throw java.io.IOException("not writable") }
            lazy.use {
                java.io.BufferedOutputStream(lazy, 1 shl 20).use { buffered ->
                    write(buffered)
                    buffered.flush()
                    (lazy.opened as? java.io.FileOutputStream)?.let { raw -> runCatching { raw.fd.sync() } }
                }
            }
            ok = openSuiteInput(ref)?.let { input -> input.stream.use { check(input) } } == true
            return ok
        } finally {
            if (!ok && created) runCatching { android.provider.DocumentsContract.deleteDocument(context.contentResolver, uri) }
        }
    }

    override suspend fun writeSuiteFile(ref: SuiteFileRef, data: ByteArray): Boolean = withContext(Dispatchers.IO) {
        safePlace(ref)?.let { place ->
            return@withContext runCatching {
                safeWrite(place, false, { it.write(data) }) { input -> input.stream.readBytes().contentEquals(data) }
            }.getOrDefault(false)
        }
        // "wt": the document is truncated and written; the provider (a USB stick, a cloud app)
        // keeps its name and place.
        runCatching {
            context.contentResolver.openOutputStream(Uri.parse(ref.id), "wt")?.use { it.write(data); it.flush() } != null
        }.getOrDefault(false) && readSuiteFile(ref)?.contentEquals(data) == true
    }

    override fun safeSave(ref: SuiteFileRef): de.danerd.ghulsuite.model.SafeSaveUi = when {
        ref.id.isEmpty() || safePlace(ref) != null -> de.danerd.ghulsuite.model.SafeSaveUi.SAFE
        runCatching { Uri.parse(ref.id).authority == EXTERNAL_STORAGE }.getOrDefault(false) -> de.danerd.ghulsuite.model.SafeSaveUi.FOLDER_NEEDED
        else -> de.danerd.ghulsuite.model.SafeSaveUi.OVERWRITES
    }

    /** A file on the device's own storage, an SD card or a USB stick, in a folder shared with the app. */
    private class Place(val tree: Uri, val parentId: String, val name: String) {
        fun id(fileName: String): String = if (parentId.endsWith(":")) parentId + fileName else "$parentId/$fileName"
        fun doc(fileName: String): Uri = android.provider.DocumentsContract.buildDocumentUriUsingTree(tree, id(fileName))
    }

    /**
     * Where [ref] can be replaced safely: a document of the device's storage (USB sticks and SD
     * cards too), whose document ids are paths ("1234-ABCD:Folder/GhulSuite.ghlzip"), in a
     * folder the user shared with the app (kept access). Anything else: null.
     */
    private fun safePlace(ref: SuiteFileRef): Place? = runCatching {
        val uri = Uri.parse(ref.id)
        if (uri.authority != EXTERNAL_STORAGE) return@runCatching null
        val docId = android.provider.DocumentsContract.getDocumentId(uri)
        val colon = docId.indexOf(':')
        if (colon < 0) return@runCatching null
        val slash = docId.lastIndexOf('/')
        val parentId = if (slash > colon) docId.substring(0, slash) else docId.substring(0, colon + 1)
        val name = docId.substring(maxOf(slash, colon) + 1)
        if (name.isEmpty()) return@runCatching null
        val tree = context.contentResolver.persistedUriPermissions.firstOrNull { p ->
            p.isReadPermission && p.isWritePermission && p.uri.authority == EXTERNAL_STORAGE &&
                android.provider.DocumentsContract.isTreeUri(p.uri) &&
                android.provider.DocumentsContract.getTreeDocumentId(p.uri).let { t ->
                    parentId == t || parentId.startsWith(if (t.endsWith(":")) t else "$t/")
                }
        }?.uri ?: return@runCatching null
        Place(tree, parentId, name)
    }.getOrNull()

    /**
     * The URI to read and write [ref] with. In a shared folder it is the URI through the folder:
     * the provider takes the access to a document away when it is renamed, so the URI kept for
     * the document itself stops working after the first swap ([safeWrite]); the one through the
     * folder keeps working. A swap that stopped half way is put right first ([recover]), before
     * anything reads the file.
     */
    private fun io(ref: SuiteFileRef): Uri = safePlace(ref)?.let { place ->
        // Not while a save swaps the files (it repairs by itself, under the same lock).
        if (swapLock.tryLock()) {
            try {
                recover(place)
            } finally {
                swapLock.unlock()
            }
        }
        place.doc(place.name)
    } ?: Uri.parse(ref.id)

    /** Held while [safeWrite] swaps and while [recover] repairs: never both at once. */
    private val swapLock = java.util.concurrent.locks.ReentrantLock()

    private fun exists(uri: Uri): Boolean = runCatching {
        context.contentResolver.query(uri, arrayOf(android.provider.DocumentsContract.Document.COLUMN_DOCUMENT_ID), null, null, null)
            ?.use { it.moveToFirst() } == true
    }.getOrDefault(false)

    private fun idOf(uri: Uri?): String? = uri?.let { runCatching { android.provider.DocumentsContract.getDocumentId(it) }.getOrNull() }

    /**
     * A swap stopped half way (the device went off between the two renames): "<name>" is
     * missing and "<name>.alt" is the complete file before; it takes its name back. A
     * "<name>.alt" next to "<name>" goes only when this app noted that its swap put the new
     * file in place (only the tidying was missing). Any other one is kept under its own name
     * ("<name>.alt-<time>"), never deleted: "<name>" might be a half file written over directly
     * (a place that was not shared then). A rename that does not end at the expected name is
     * undone.
     */
    private fun recover(place: Place) {
        val r = context.contentResolver
        val target = place.doc(place.name)
        val alt = place.doc(place.name + ".alt")
        runCatching {
            if (!exists(alt)) return@runCatching
            if (!exists(target)) {
                val back = android.provider.DocumentsContract.renameDocument(r, alt, place.name)
                if (back != null && idOf(back) != place.id(place.name)) {
                    // The provider chose another name ("… (1)"): back to ".alt", nothing lost.
                    runCatching { android.provider.DocumentsContract.renameDocument(r, back, place.name + ".alt") }
                }
            } else if (devicePref(SWAP_DONE) == place.id(place.name)) {
                android.provider.DocumentsContract.deleteDocument(r, alt)
                setDevicePref(SWAP_DONE, "")
            } else {
                android.provider.DocumentsContract.renameDocument(r, alt, place.name + ".alt-" + System.currentTimeMillis())
            }
        }
    }

    /**
     * Writes "<name>.neu" next to the file, reads it back ([check]), then swaps: the old file
     * becomes "<name>.alt", the new one takes the name, the old one goes. At every moment a
     * complete file is there: until the new one is checked the old one is not touched; every
     * rename is checked to end at the expected name (the provider would silently pick
     * "… (1)" for a name in use) and undone otherwise; if the device goes off between the two
     * renames, the next access puts "<name>.alt" back ([recover]). Fails (false) without
     * losing anything.
     */
    private fun safeWrite(
        place: Place,
        created: Boolean,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
    ): Boolean {
        swapLock.lock()
        try {
            return safeWriteLocked(place, created, write, check)
        } finally {
            swapLock.unlock()
        }
    }

    private fun safeWriteLocked(
        place: Place,
        created: Boolean,
        write: (java.io.OutputStream) -> Unit,
        check: (SuiteInput) -> Boolean,
    ): Boolean {
        val r = context.contentResolver
        val target = place.doc(place.name)
        recover(place)
        // An old file next to it that could not be put away: no swap now (its name is taken).
        if (exists(place.doc(place.name + ".alt"))) return false
        val neu = place.doc(place.name + ".neu")
        if (exists(neu)) runCatching { android.provider.DocumentsContract.deleteDocument(r, neu) }
        val parent = android.provider.DocumentsContract.buildDocumentUriUsingTree(place.tree, place.parentId)
        val tmp = android.provider.DocumentsContract.createDocument(r, parent, "application/octet-stream", place.name + ".neu")
            ?: return false
        // Whether the checked new file must stay (it is the only complete one left).
        var keepTmp = false
        var ok = false
        try {
            val raw = r.openOutputStream(tmp, "w") ?: return false
            java.io.BufferedOutputStream(raw, 1 shl 20).use { buffered ->
                write(buffered)
                buffered.flush()
                (raw as? java.io.FileOutputStream)?.let { f -> runCatching { f.fd.sync() } }
            }
            val written = openInputAt(tmp)?.let { input -> input.stream.use { check(input) } } == true
            if (!written) return false
            // The swap. Until here the old file was not touched.
            val old: Uri? = if (exists(target)) {
                val moved = android.provider.DocumentsContract.renameDocument(r, target, place.name + ".alt") ?: return false
                if (idOf(moved) != place.id(place.name + ".alt")) {
                    // Back to its name; where that fails too, the checked new file stays next to it.
                    val back = runCatching { android.provider.DocumentsContract.renameDocument(r, moved, place.name) }.getOrNull()
                    keepTmp = idOf(back) != place.id(place.name)
                    return false
                }
                moved
            } else {
                null
            }
            val placed = runCatching { android.provider.DocumentsContract.renameDocument(r, tmp, place.name) }.getOrNull()
            if (placed == null || idOf(placed) != place.id(place.name)) {
                // Not at its name: back as it was. Where the old file cannot take its name back,
                // the new one stays (complete and checked) next to it.
                val restored = old == null || runCatching {
                    idOf(android.provider.DocumentsContract.renameDocument(r, old, place.name)) == place.id(place.name)
                }.getOrDefault(false)
                if (placed != null) {
                    if (restored) runCatching { android.provider.DocumentsContract.deleteDocument(r, placed) } else keepTmp = true
                }
                keepTmp = keepTmp || (placed == null && !restored)
                return false
            }
            keepTmp = true
            // Noted before the old file goes: if the device goes off now, the next access knows
            // that "<name>" is the new, complete file and ".alt" may go.
            setDevicePref(SWAP_DONE, place.id(place.name))
            old?.let { runCatching { android.provider.DocumentsContract.deleteDocument(r, it) } }
            setDevicePref(SWAP_DONE, "")
            ok = true
            return true
        } finally {
            if (!keepTmp) runCatching { android.provider.DocumentsContract.deleteDocument(r, tmp) }
            // A document made just for this that could not be written stays no empty file.
            if (!ok && created && exists(target) && !exists(place.doc(place.name + ".alt"))) {
                runCatching { if (fileSize(target) == 0L) android.provider.DocumentsContract.deleteDocument(r, target) }
            }
        }
    }

    private fun fileSize(uri: Uri): Long? = runCatching {
        context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst() && !c.isNull(0)) c.getLong(0) else null
        }
    }.getOrNull()

    /** A deleted document cannot be opened for writing again: "save as" instead. */
    override val canWriteFilesAnew: Boolean = false

    override suspend fun canWriteSuiteFile(ref: SuiteFileRef): Boolean = withContext(Dispatchers.IO) {
        runCatching { androidx.documentfile.provider.DocumentFile.fromSingleUri(context, io(ref))?.canWrite() == true }
            .getOrDefault(false)
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

    override fun lock() {
        // Decrypted copies handed to other apps and a copied password do not outlive the session.
        File(context.cacheDir, "shared").deleteRecursively()
        // An "open with" choice that waited would point at a deleted copy, above the lock screen.
        pendingChooser = null
        clearOwnClip()
        super.lock()
    }

    /**
     * Panic lock: locks the app and removes fingerprint and app PIN, so only the master password
     * opens it again. Calendar reminders keep working: the panic lock closes the app.
     */
    override fun panicLock() {
        // Nothing is written back first (a GhulDocs vault open on its own locks at once).
        panicking = true
        try {
            lock()
        } finally {
            panicking = false
        }
        quick.disableAll()
    }

    override val canShareToApps: Boolean = true

    /** Android's share sheet: a copy in the app's cache (deleted on lock) for the chosen app. */
    override suspend fun shareToApps(name: String, mime: String, data: ByteArray): Boolean =
        shareStreamToApps(name, mime) { it.write(data) }

    /**
     * The same, written piece by piece into the copy (a document of GhulDocs). A write that fails
     * or is cancelled removes the half copy at once; nothing is handed over then.
     */
    override suspend fun shareStreamToApps(name: String, mime: String, write: suspend (de.danerd.ghulsuite.model.ByteSink) -> Unit): Boolean {
        val target = withContext(Dispatchers.IO) {
            val dir = File(context.cacheDir, "shared").apply { deleteRecursively(); mkdirs() }
            File(dir, safeFileName(name, mutableSetOf()))
        }
        val out = withContext(Dispatchers.IO) { java.io.BufferedOutputStream(java.io.FileOutputStream(target), 1 shl 20) }
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
                target.delete()
            }
        }
        val uri = androidx.core.content.FileProvider.getUriForFile(context, context.packageName + ".files", target)
        // "Open with": the apps that show this type (not the share targets).
        val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, mime)
        intent.clipData = ClipData.newRawUri("", uri)
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        val chooser = Intent.createChooser(intent, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        // Decrypting took a while and the user went elsewhere: Android does not start it from the
        // background, so it waits until GhulSuite is in front again.
        if (de.danerd.ghulsuite.ui.AppLaunch.inFront.value) {
            ExternalActivity.started()
            context.startActivity(chooser)
        } else {
            pendingChooser = chooser
        }
        return true
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

    override suspend fun enableBiometricUnlock(password: String) {
        val (t, s, c) = bioTexts()
        quick.enableBiometric(password, t, s, c)
    }

    override suspend fun enablePinUnlock(password: String, pin: String) = quick.enablePin(password, pin)

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

    // --- tools --------------------------------------------------------------------------------

    override fun printEmergencySheet(code: String, text: EmergencySheetText) {
        val act = activity() ?: return
        // The print preview is an activity of the system: the automatic lock waits (as for a picker).
        ExternalActivity.started()
        EmergencySheet.print(act, code, text)
    }

    override fun notificationsAllowed(): Boolean =
        runCatching { context.getSystemService(android.app.NotificationManager::class.java).areNotificationsEnabled() }.getOrDefault(true)

    override fun openNotificationSettings() {
        ExternalActivity.started()
        runCatching {
            context.startActivity(
                Intent(android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                    .putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, context.packageName)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            )
        }
    }

    /** "Open with" that waits for the app to come to the front (see [onFront]); dropped when locking. */
    @Volatile private var pendingChooser: Intent? = null

    /** Back in front: the "open with" choice that waited, and a clipboard that is due to be cleared. */
    fun onFront() {
        pendingChooser?.let { c ->
            pendingChooser = null
            ExternalActivity.started()
            runCatching { context.startActivity(c) }
        }
        if (clipDueAt > 0 && android.os.SystemClock.elapsedRealtime() >= clipDueAt) clearOwnClip()
    }

    /** When the copied secret is to be cleared (elapsed realtime, counts in deep sleep); 0: none. */
    @Volatile private var clipDueAt = 0L

    /** Label of the last copied secret, until it was cleared. */
    @Volatile private var copiedLabel: String? = null

    /** Clears the clipboard if it still holds what GhulSuite copied (not what the user copied since). */
    private fun clearOwnClip() {
        val label = copiedLabel ?: return
        copiedLabel = null
        clipDueAt = 0
        val cm = context.getSystemService(ClipboardManager::class.java)
        // In the background Android may hide the clipboard: then clear to be safe.
        val current = runCatching { cm.primaryClipDescription?.label?.toString() }.getOrNull()
        if (current == null || current == label) runCatching { cm.clearPrimaryClip() }
    }

    override fun copySensitive(label: String, text: String) {
        val cm = context.getSystemService(ClipboardManager::class.java)
        // Neutral: the label is visible to keyboards and clipboard previews (no account names).
        val tagged = "GhulSuite ${System.nanoTime() % 100000}"
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
            // The handler's clock stops in deep sleep: the due time is checked again when the
            // app comes to the front (see onFront).
            clipDueAt = android.os.SystemClock.elapsedRealtime() + seconds * 1000L
            main.postDelayed({ if (copiedLabel == tagged) clearOwnClip() }, seconds * 1000L)
        }
    }

    override fun crashReport(): String? = CrashLog.pending(context)

    override fun clearCrashReport() = CrashLog.clear(context)
}

/** Opens its stream only when the first byte is written (nothing is truncated before). */
private class LazyOutput(private val open: () -> java.io.OutputStream) : java.io.OutputStream() {
    var opened: java.io.OutputStream? = null
        private set

    private fun out(): java.io.OutputStream = opened ?: open().also { opened = it }

    override fun write(b: Int) = out().write(b)

    override fun write(b: ByteArray, off: Int, len: Int) = out().write(b, off, len)

    override fun flush() {
        opened?.flush()
    }

    override fun close() {
        opened?.close()
    }
}
