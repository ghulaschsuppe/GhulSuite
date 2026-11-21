// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import java.io.File
import kotlin.test.Test
import kotlin.test.assertTrue

/**
 * M17: every window of its own (a dialog, a sheet, a popup) calls GuardWindow() right in its
 * content, so the protection of the main window (no taps under an overlay, hidden from
 * accessibility services that are not tools) reaches it too. Dialogs of the app are GhulSheets
 * (AlertDialog in Design.kt); any other window maker must guard itself.
 */
class GuardedWindowsTest {
    private val windowMakers = Regex(
        """(?<![\w.])(androidx\.compose\.ui\.window\.Dialog|Dialog|Popup|ModalBottomSheet|DatePickerDialog|DropdownMenu|ExposedDropdownMenu)\(""",
    )

    @Test
    fun everyWindowIsGuarded() {
        val src = File("src/commonMain/kotlin")
        assertTrue(src.isDirectory, "run from app/composeApp")
        val unguarded = mutableListOf<String>()
        src.walk().filter { it.extension == "kt" }.forEach { f ->
            val lines = f.readLines()
            lines.forEachIndexed { i, line ->
                val code = line.substringBefore("//")
                if (code.trimStart().startsWith("*") || code.contains("fun ") || code.contains("import ")) return@forEachIndexed
                if (windowMakers.containsMatchIn(code)) {
                    val next = lines.subList(i, minOf(lines.size, i + 25)).joinToString("\n")
                    if (!next.contains("GuardWindow()")) unguarded += "${f.name}:${i + 1}: ${line.trim()}"
                }
            }
        }
        assertTrue(unguarded.isEmpty(), "windows without GuardWindow():\n" + unguarded.joinToString("\n"))
    }
}
