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

/** Current Unix time in seconds. */
expect fun nowSeconds(): Long

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
 * [onSaved] receives whether the file was written.
 */
@Composable
expect fun rememberSaveFile(onSaved: (Boolean) -> Unit): (String, ByteArray) -> Unit

/**
 * Returns a launcher that picks a suite file (`.ghlzip`) to open. The app keeps access to it, so
 * it can save into the same file later (Android: a kept document permission; Windows: the path).
 */
@Composable
expect fun rememberOpenSuiteFile(onPicked: (de.danerd.ghulsuite.model.SuiteFileRef, ByteArray) -> Unit): () -> Unit

/** Returns a launcher that asks where to save a new suite file (suggested name); null when cancelled. */
@Composable
expect fun rememberCreateSuiteFile(onCreated: (de.danerd.ghulsuite.model.SuiteFileRef?) -> Unit): (String) -> Unit

/** Returns a launcher that lets the user choose a folder. */
@Composable
expect fun rememberPickFolder(onPicked: (de.danerd.ghulsuite.model.FolderRef) -> Unit): () -> Unit

/** Returns a launcher that opens the system file picker for a file of any size (read as a stream). */
@Composable
expect fun rememberOpenStream(onPicked: (de.danerd.ghulsuite.model.ByteSource) -> Unit): () -> Unit

/**
 * The same for several files at once (GhulDrive). Each file is only opened when it is read, so
 * choosing many files keeps nothing open.
 */
@Composable
expect fun rememberOpenStreams(onPicked: (List<de.danerd.ghulsuite.model.ByteSource>) -> Unit): () -> Unit

/**
 * Returns a launcher that asks the user where to save a file (suggested name and MIME type);
 * [onCreated] receives a stream to write to, or null if cancelled.
 */
@Composable
expect fun rememberCreateStream(onCreated: (de.danerd.ghulsuite.model.ByteSink?) -> Unit): (String, String) -> Unit

/** QR code modules for [text] (rows of dark/light), via ZXing on both platforms. */
expect fun qrModules(text: String): List<BooleanArray>

/** Finds a QR code in an image file (PNG, JPEG, …); null if there is none. */
expect fun decodeQrImage(bytes: ByteArray): String?

/** Takes a photo with the camera (document scan); null if cancelled or not available. */
expect val cameraPhotoAvailable: Boolean

@androidx.compose.runtime.Composable
expect fun rememberTakePhoto(onResult: (ByteArray?) -> Unit): () -> Unit

/** Operating system and device for error reports, e.g. "Android 15 (API 35) · Google Pixel 8". */
expect fun deviceInfo(): String

/** Local wall-clock time now as `YYYY-MM-DDTHH:MM` (GhulCal). */
expect fun localNow(): String

/** Unix milliseconds of a local `YYYY-MM-DDTHH:MM`. */
expect fun epochMillisOf(local: String): Long
/** Switches the app language ("" = follow the system). Android restarts the screen; Windows after a restart. */
expect fun applyAppLanguage(tag: String)
