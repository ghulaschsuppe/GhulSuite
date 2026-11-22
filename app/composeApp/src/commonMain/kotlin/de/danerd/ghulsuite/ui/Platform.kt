// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.runtime.Composable

/** Handles the system back gesture (Android); no-op on desktop. */
@Composable
expect fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit)

/**
 * Test tags as ids for the emulator flows (Android). The main window sets this once; a sheet or
 * dialog is a window of its own and needs it again (bug: "ask-confirm" in the delete question
 * was not found). No-op on desktop.
 */
expect fun androidx.compose.ui.Modifier.testTagsAsIds(): androidx.compose.ui.Modifier

/**
 * M17: protects the window it is composed in (a dialog or a sheet is a window of its own): no
 * taps while another app draws over it (tapjacking), and on Android 14+ hidden from accessibility
 * services that are not tools for people who need them. Nothing on Windows.
 */
@Composable
expect fun GuardWindow()

/** Current Unix time in seconds. */
expect fun nowSeconds(): Long

/**
 * Milliseconds of a clock that only goes forward and also runs while the device sleeps (Android:
 * elapsedRealtime; Windows: nanoTime, which counts sleep there too): for time windows.
 */
expect fun monotonicMillis(): Long

/** Formats a Unix time in milliseconds as a local date. */
expect fun formatDate(millis: Long): String

/** Formats a Unix time in milliseconds as a local time of day. */
expect fun formatTime(millis: Long): String

/** Whether this platform can scan QR codes with a camera. */
expect val qrScannerAvailable: Boolean

/** Camera preview that reports the first QR code found. */
@Composable
expect fun QrScanner(onResult: (String) -> Unit)

/** Whether this platform offers system file and folder pickers. */
expect val filePickersAvailable: Boolean

/** Returns a launcher that opens the system file picker; [onPicked] receives the file's content. */
@Composable
expect fun rememberOpenFile(onPicked: (de.danerd.ghulsuite.model.PickedFile) -> Unit): () -> Unit

/**
 * Returns a launcher that asks the user where to save a file (suggested name, content);
 * [onSaved] receives how it ended: saved, cancelled, or chosen but not written (M8: a failed
 * write says so, see [reportSaveFile]).
 */
@Composable
expect fun rememberSaveFile(onSaved: (SaveFileResult) -> Unit): (String, ByteArray) -> Unit

/**
 * Returns a launcher that picks a suite file (`.ghlzip`), a GhulDocs package (`.ghldocs`) or a vault file to open. Nothing is read
 * yet. The app keeps access to it, so it can save into the same file later (Android: a kept
 * document permission; Windows: the path).
 */
@Composable
expect fun rememberOpenSuiteFile(onPicked: (de.danerd.ghulsuite.model.SuiteFileRef) -> Unit): () -> Unit

/** Returns a launcher that asks where to save a new suite file (suggested name); null when cancelled. */
@Composable
expect fun rememberCreateSuiteFile(onCreated: (de.danerd.ghulsuite.model.SuiteFileRef?) -> Unit): (String) -> Unit

/** Returns a launcher that lets the user choose a folder. */
@Composable
expect fun rememberPickFolder(onPicked: (de.danerd.ghulsuite.model.FolderRef) -> Unit): () -> Unit

/** What choosing a safe place for the suite file gave (device test: one step instead of "share a folder"). */
sealed class SafePlaceResult {
    /** The file [ref] in the chosen folder (shared with the app for good); it may not exist yet. */
    class Chosen(
        val ref: de.danerd.ghulsuite.model.SuiteFileRef,
        val exists: Boolean,
        /** Gives the kept right to the folder back (when the folder is not taken after all). */
        val release: () -> Unit = {},
    ) : SafePlaceResult()

    /** A folder Android never shares ("Download", the top of a storage) or one of another app. */
    class Refused(val notShareable: Boolean) : SafePlaceResult()

    object Cancelled : SafePlaceResult()
}

/**
 * Android: returns a launcher that lets the user choose (or create) a folder for the suite file,
 * keeps the right to read and write it, and gives the file of the given name in it: saving there
 * is safe (written next to it, checked, then swapped). Windows has no need for it (null).
 */
@Composable
expect fun rememberChooseSafePlace(onResult: (SafePlaceResult) -> Unit): ((fileName: String) -> Unit)?

/** Returns a launcher that opens the system file picker for a file of any size (read as a stream). */
@Composable
expect fun rememberOpenStream(onPicked: (de.danerd.ghulsuite.model.ByteSource) -> Unit): () -> Unit

/**
 * The same for several files at once. Each file is only opened when it is read, so choosing many
 * files keeps nothing open.
 */
@Composable
expect fun rememberOpenStreams(onPicked: (List<de.danerd.ghulsuite.model.ByteSource>) -> Unit): () -> Unit

/**
 * Returns a launcher that asks the user where to save a file (suggested name and MIME type);
 * [onCreated] receives a stream to write to, or null if cancelled. A place chosen that cannot be
 * opened for writing calls [onFailed] first (then [onCreated] with null).
 */
@Composable
expect fun rememberCreateStream(onFailed: () -> Unit = {}, onCreated: (de.danerd.ghulsuite.model.ByteSink?) -> Unit): (String, String) -> Unit

/** QR code modules for [text] (rows of dark/light), via ZXing on both platforms. */
expect fun qrModules(text: String): List<BooleanArray>

/** Finds a QR code in an image file (PNG, JPEG, …); null if there is none. */
expect fun decodeQrImage(bytes: ByteArray): String?

/** Operating system and device for error reports, e.g. "Android 15 (API 35) · Google Pixel 8". */
expect fun deviceInfo(): String

/** Local wall-clock time now as `YYYY-MM-DDTHH:MM` (GhulCal). */
expect fun localNow(): String

/** Unix milliseconds of a local `YYYY-MM-DDTHH:MM`. */
expect fun epochMillisOf(local: String): Long
/** Switches the app language ("" = follow the system). Android restarts the screen; Windows after a restart. */
expect fun applyAppLanguage(tag: String)

/**
 * Decodes an image (JPEG, PNG, WebP, GIF, on Android also HEIC) from memory, scaled down so its
 * longer side is at most [maxSide] pixels; null if it is no image this platform can read.
 * Nothing is written to the disk.
 */
expect fun decodeImage(bytes: ByteArray, maxSide: Int): androidx.compose.ui.graphics.ImageBitmap?

/** A PDF opened from memory: its pages, rendered when they are shown. */
interface PdfView {
    val pageCount: Int
    /** Page [index] (from 0), [widthPx] wide; null if it cannot be drawn. */
    fun render(index: Int, widthPx: Int): androidx.compose.ui.graphics.ImageBitmap?
    fun close()
}

/**
 * Opens a PDF held in memory, never through a file on the disk (Android: an anonymous file in
 * memory; Windows: read from the bytes). Null if it cannot be shown here (damaged, protected by a
 * password, or an Android version without anonymous files in memory).
 */
expect fun openPdf(bytes: ByteArray): PdfView?

/**
 * Scanning documents into GhulDocs (G7): Android has it (the camera in the app, pages cropped and
 * straightened, saved as one PDF); Windows has no "Scan" in the add menu of GhulDocs.
 */
expect val docScanAvailable: Boolean

/** The camera and pictures for scanning (see [de.danerd.ghulsuite.scan.ScanEngine]); null without scanning. */
expect fun platformScanEngine(): de.danerd.ghulsuite.scan.ScanEngine?
