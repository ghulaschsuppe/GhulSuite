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
