// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import kotlin.coroutines.cancellation.CancellationException

/**
 * Like [runCatching], but a cancelled coroutine stays cancelled.
 *
 * When a screen is left, or an effect starts again with a new key (another folder, a new search
 * text), Compose cancels the running work. [runCatching] would catch that cancellation like an
 * error, and the screen would show "The coroutine scope left the composition". Here it is passed
 * on, so only real errors end up in [Result.failure].
 */
inline fun <R> catching(block: () -> R): Result<R> =
    try {
        Result.success(block())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Throwable) {
        Result.failure(e)
    }

/** The same for calls on a receiver (`net.catching { ... }`). */
inline fun <T, R> T.catching(block: T.() -> R): Result<R> =
    try {
        Result.success(block())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Throwable) {
        Result.failure(e)
    }

/**
 * The crash Compose throws for a key that reaches a text field before its layout caught up with
 * the text: "offset(14) is out of bounds [0, 11]". It happens when keys come faster than a frame
 * (a hardware keyboard, the monkey test). The activity drops that one key instead of ending the
 * app; every other error still ends it. The message is a literal in Compose, so it survives code
 * shrinking, unlike the class names.
 */
fun isStaleTextLayoutError(e: Throwable): Boolean =
    e is IllegalArgumentException && e.message?.let { STALE_TEXT_LAYOUT.containsMatchIn(it) } == true

private val STALE_TEXT_LAYOUT = Regex("""^offset\(-?\d+\) is out of bounds \[0, \d+]""")
