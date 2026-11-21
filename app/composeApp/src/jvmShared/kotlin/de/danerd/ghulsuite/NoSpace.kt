// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

/**
 * H12: whether a failed write ran out of room. Not by the words of a message (they depend on the
 * language of the system), but by what is the same everywhere: the errno name ENOSPC or EDQUOT
 * in the message of Android and Linux ("write failed: ENOSPC (No space left on device)"), an
 * "(os error N)" code (28, 122; Windows 112, 39), or, where Java names no code at all (Windows),
 * the room left at the place written to ([freeBytes], null: not known) being nearly none.
 */
fun isNoSpace(e: Throwable, freeBytes: () -> Long? = { null }): Boolean {
    var x: Throwable? = e
    val seen = mutableSetOf<Throwable>()
    while (x != null && seen.add(x)) {
        val m = x.message.orEmpty()
        if (Regex("""\b(ENOSPC|EDQUOT)\b""").containsMatchIn(m)) return true
        val code = Regex("""\(os error (\d+)\)""").find(m)?.groupValues?.get(1)?.toIntOrNull()
        if (code != null && code in noSpaceCodes()) return true
        x = x.cause
    }
    val free = runCatching { freeBytes() }.getOrNull()
    return free != null && free < NEARLY_FULL
}

/**
 * The codes of a full storage on this system: ERROR_DISK_FULL and ERROR_HANDLE_DISK_FULL on
 * Windows, errno ENOSPC and EDQUOT elsewhere (on Linux 39 and 112 mean other things).
 */
private fun noSpaceCodes(): Set<Int> =
    if (System.getProperty("os.name").orEmpty().lowercase().contains("win")) setOf(112, 39) else setOf(28, 122)

/** Less room than this after a failed write: it failed because the storage is full. */
private const val NEARLY_FULL = 1L shl 20

/**
 * What a callback tells the core when a write ran out of room: the core recognises "ENOSPC" and
 * reports NoSpace (whatever the language of the system).
 */
internal const val NO_SPACE_IO = "the file cannot be written: ENOSPC"
