// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.runtime.Composable

/** Handles the system back gesture (Android); no-op on desktop. */
@Composable
expect fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit)

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

/** Returns a launcher that lets the user choose a folder. */
@Composable
expect fun rememberPickFolder(onPicked: (de.danerd.ghulsuite.model.FolderRef) -> Unit): () -> Unit

/** Returns a launcher that opens the system file picker for a file of any size (read as a stream). */
@Composable
expect fun rememberOpenStream(onPicked: (de.danerd.ghulsuite.model.ByteSource) -> Unit): () -> Unit

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

/** Asks for access to the photos (photo backup); the callback gets whether it was granted. */
@androidx.compose.runtime.Composable
expect fun rememberPhotoPermission(onResult: (Boolean) -> Unit): () -> Unit

/** Takes a photo with the camera (document scan); null if cancelled or not available. */
expect val cameraPhotoAvailable: Boolean

@androidx.compose.runtime.Composable
expect fun rememberTakePhoto(onResult: (ByteArray?) -> Unit): () -> Unit

/** Operating system and device for error reports, e.g. "Android 15 (API 35) · Google Pixel 8". */
expect fun deviceInfo(): String
