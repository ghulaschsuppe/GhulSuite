// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

/**
 * A crash as it is kept for the report: the classes of the exception and of its causes with
 * their stack frames, without any message. The report can be seen before the suite is unlocked;
 * a message could carry a name or other text of the user.
 */
fun crashTrace(error: Throwable): String = buildString {
    var e: Throwable? = error
    var first = true
    val seen = mutableSetOf<Throwable>()
    while (e != null && seen.add(e)) {
        if (!first) append("Caused by: ")
        appendLine(e.javaClass.name)
        for (frame in e.stackTrace.take(60)) appendLine("\tat $frame")
        for (s in e.suppressed) appendLine("\tSuppressed: ${s.javaClass.name}")
        first = false
        e = e.cause
    }
}
