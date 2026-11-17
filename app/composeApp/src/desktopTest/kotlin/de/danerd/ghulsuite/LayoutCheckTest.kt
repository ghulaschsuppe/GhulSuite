// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.isRoot
import androidx.compose.ui.test.runDesktopComposeUiTest
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.Density
import androidx.compose.ui.platform.LocalDensity
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.CalView
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.ui.SetPage
import de.danerd.ghulsuite.ui.Tab
import de.danerd.ghulsuite.ui.JobKind
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.jetbrains.skia.EncodedImageFormat
import org.jetbrains.skia.Image
import java.io.File
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertTrue

/**
 * Looks at every main screen on several phones (narrow 320 dp, usual 360 and 412 dp) with the
 * normal and the large system font (130 %), in German and English, and reports what a user would
 * see as a broken layout, without anyone looking at a picture:
 *   - a word broken in the middle (the checklist of the PHP module, bug F53),
 *   - a cut-off label of a button,
 *   - content reaching past the edge of the screen (outside of horizontal scrolling),
 *   - a tap target smaller than 40 × 40 dp (calendar grids: only listed),
 *   - texts lying on top of each other.
 * Every finding names screen, phone and text; the list is in build/layout-check/report-<language>.txt,
 * texts shortened with "…" (often wanted) in notes-<language>.txt, and a picture of every screen on
 * every phone in build/screenshots/l-<language>-<phone>-<screen>.png.
 */
@OptIn(ExperimentalTestApi::class)
class LayoutCheckTest {
    private val out = File("build/layout-check").apply { mkdirs() }

    private class Phone(val name: String, val widthDp: Int, val heightDp: Int, val fontScale: Float)

    private val phones = listOf(
        Phone("412dp", 412, 892, 1f),
        Phone("360dp-schrift130", 360, 780, 1.3f),
        Phone("320dp", 320, 640, 1f),
    )

    /** Texts AppState looks up outside of the screen need a display; only that failure is let through. */
    private val errors = java.util.Collections.synchronizedList(mutableListOf<Throwable>())
    /** Scopes of the screens shown so far; each one is ended when the next screen comes. */
    private val scopes = mutableListOf<CoroutineScope>()
    private fun scope() = CoroutineScope(
        kotlinx.coroutines.SupervisorJob() + Dispatchers.Unconfined +
            kotlinx.coroutines.CoroutineExceptionHandler { _, e -> if (e !is java.awt.HeadlessException) errors += e },
    ).also { scopes += it }

    private fun endOldScopes() {
        while (scopes.size > 1) scopes.removeAt(0).coroutineContext[kotlinx.coroutines.Job]?.cancel()
    }

    private fun state(setup: AppState.() -> Unit = {}): AppState =
        AppState(ScreenshotTest.Fake(exists = true, unlocked = true), scope()).apply(setup)

    /** The screens, each made fresh (name to state). */
    private val screens: List<Pair<String, () -> AppState>> = buildList {
        add("einrichtung" to { AppState(ScreenshotTest.Fake(exists = false, unlocked = false), scope()) })
        add("entsperren" to { AppState(ScreenshotTest.Fake(exists = true, unlocked = false), scope()) })
        add("pass" to { state() })
        add("pass-eintrag" to { state { push(Screen.EntryDetail("p1", "1")) } })
        add("pass-bearbeiten" to { state { push(Screen.EntryEdit("p1", "1", "login")) } })
        add("auth" to { state { tab = Tab.AUTH } })
        add("auth-hinzufuegen" to { state { push(Screen.AddTotp("t1")) } })
        // The strip of a running background job under the header, with its progress and "Cancel".
        add("hintergrund-auftrag" to {
            state {
                jobs.revealMillis = 0
                jobs.start(JobKind.UPLOAD, title = { "Steuerunterlagen 2025 vollständig.pdf" }) { j ->
                    j.total = 2
                    j.item = "Steuerunterlagen 2025 vollständig.pdf"
                    j.bytesTotal = 2_400_000
                    j.bytes = 800_000
                    kotlinx.coroutines.awaitCancellation()
                }
            }
        })
        add("kalender-monat" to { state { tab = Tab.CAL } })
        add("kalender-woche" to { state { tab = Tab.CAL; calView = CalView.WEEK } })
        add("kalender-tag" to { state { tab = Tab.CAL; calView = CalView.DAY } })
        add("kalender-liste" to { state { tab = Tab.CAL; calView = CalView.LIST } })
        add("optionen" to { state { tab = Tab.SETTINGS } })
        for (p in SetPage.entries) add("optionen-${p.name.lowercase()}" to { state { push(Screen.SettingsPage(p)) } })
        add("import" to { state { push(Screen.Import) } })
        add("export" to { state { push(Screen.Export) } })
    }

    @Test
    fun germanScreensFit() = check(Locale.GERMAN, "de")

    @Test
    fun englishScreensFit() = check(Locale.ENGLISH, "en")

    private fun check(locale: Locale, tag: String) {
        // Phones: measured with the fonts of Linux (DejaVu Sans is wider than Android's Roboto, so
        // this is the strict case). Windows draws with other fonts and is no phone; its narrow
        // windows are an open point of their own (docs/TESTPROTOKOLL.md).
        org.junit.Assume.assumeFalse("Handy-Layout wird unter Linux geprüft", System.getProperty("os.name").startsWith("Windows"))
        Locale.setDefault(locale)
        val findings = mutableListOf<String>()
        val allNotes = mutableListOf<String>()
        for (phone in phones) {
            var current by mutableStateOf(0)
            runDesktopComposeUiTest(phone.widthDp * 2, phone.heightDp * 2) {
                // A screen with an endless animation (a spinner) never becomes idle: the clock is
                // moved on by hand instead of waiting for idle.
                mainClock.autoAdvance = false
                setContent {
                    CompositionLocalProvider(LocalDensity provides Density(2f, phone.fontScale)) {
                        // A new state (and composition) for each screen.
                        key(current) { GhulSuiteApp(screens[current].second()) }
                    }
                }
                for (i in screens.indices) {
                    current = i
                    mainClock.advanceTimeBy(1_500)
                    endOldScopes()
                    val name = screens[i].first
                    println("Layout: $tag/${phone.name}/$name")
                    denseGrid = name in setOf("kalender-monat", "kalender-woche", "kalender-tag")
                    val found = inspect(phone.widthDp * 2f)
                    val where = "$tag/${phone.name}/$name"
                    allNotes += notes.map { "$where: $it" }
                    notes.clear()
                    if (found.isNotEmpty()) findings += found.map { "$where: $it" }
                    // Every screen on every phone, for looking at them (and for the comparison
                    // with dev in each pull request): build/screenshots/l-<language>-<phone>-<screen>.png
                    picture("$tag-${phone.name}-$name")
                }
            }
        }
        // Nothing of these screens may run on into the next test.
        scopes.forEach { it.coroutineContext[kotlinx.coroutines.Job]?.cancel() }
        scopes.clear()
        File(out, "report-$tag.txt").writeText(findings.joinToString("\n", postfix = "\n"))
        File(out, "notes-$tag.txt").writeText(allNotes.joinToString("\n", postfix = "\n"))
        assertTrue(errors.isEmpty(), "errors in the app: $errors")
        assertTrue(findings.isEmpty(), "Layout (${findings.size} Funde, siehe build/layout-check/report-$tag.txt):\n" + findings.take(60).joinToString("\n"))
    }

    private val pictures = File("build/screenshots").apply { mkdirs() }

    private fun DesktopComposeUiTest.picture(name: String) {
        val bitmap = captureToImage().asSkiaBitmap()
        File(pictures, "l-$name.png").writeBytes(Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)!!.bytes)
    }

    /** All findings on what is shown now (all windows: the screen, sheets, dialogs). */
    /** Texts shortened with "…" outside of buttons: not a failure, listed for looking at them. */
    private val notes = linkedSetOf<String>()

    /** The screen now looked at is a calendar grid (month, week, day). */
    private var denseGrid = false

    private fun DesktopComposeUiTest.inspect(screenWidthPx: Float): List<String> {
        val found = linkedSetOf<String>()
        val roots = onAllNodes(isRoot(), useUnmergedTree = true).fetchSemanticsNodes()
        for (root in roots) {
            val all = mutableListOf<SemanticsNode>()
            fun walk(n: SemanticsNode) { all += n; n.children.forEach(::walk) }
            walk(root)
            val texts = mutableListOf<Pair<SemanticsNode, String>>()
            for (n in all) {
                val text = n.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text }
                    ?: n.config.getOrNull(SemanticsProperties.EditableText)?.text
                val layout = textLayout(n)
                if (text != null && layout != null) {
                    texts += n to text
                    brokenWord(text, layout)?.let { found += "Wort mitten im Wort umgebrochen: „$it“ in „${text.take(60)}“" }
                    // Shortened with "…". (didOverflowWidth is no measure: it also reports texts
                    // that are shown whole.)
                    val cut = (0 until layout.lineCount).any { layout.isLineEllipsized(it) }
                    if (cut && insideButton(n)) found += "Beschriftung abgeschnitten: „${text.take(60)}“"
                    // Shortened with "…" elsewhere: often wanted (long subjects), so only a note.
                    if (cut && insideTag(n, de.danerd.ghulsuite.ui.TITLE_TAG)) found += "Titel abgeschnitten: „${text.take(60)}“"
                    else if (cut && !insideButton(n)) notes += "mit … gekürzt: „${text.take(60)}“"
                }
                val b = n.boundsInRoot
                if (b.width > 0 && (b.right > screenWidthPx + 2 || b.left < -2) && !scrollsSideways(n)) {
                    found += "ragt über den Rand: ${describe(n, text)}"
                }
                // Text fields: their whole box is the tap target, not the line of text inside.
                val clickable = n.config.getOrNull(SemanticsActions.OnClick) != null && !n.config.contains(SemanticsProperties.Disabled) &&
                    !n.config.contains(SemanticsProperties.EditableText)
                if (clickable) {
                    // The full size, also of a row cut off at the edge of the screen (its visible
                    // rest would look small), and the touch area around small icons.
                    val t = n.touchBoundsInRoot
                    val w = maxOf(t.width, n.size.width.toFloat())
                    val h = maxOf(t.height, n.size.height.toFloat())
                    val min = 40 * 2f
                    if (t.width > 0 && t.height > 0 && (w < min || h < min)) {
                        val what = "Tippfläche klein (${(w / 2).toInt()} × ${(h / 2).toInt()} dp): ${describe(n, text)}"
                        // The calendar grids show events small on purpose (a month on one screen).
                        if (denseGrid) notes += what else found += what
                    }
                }
            }
            // Texts on top of each other (not one inside the other).
            for (i in texts.indices) for (j in i + 1 until texts.size) {
                val (a, ta) = texts[i]
                val (b, tb) = texts[j]
                if (a.id == b.id || ancestor(a, b) || ancestor(b, a) || ta.isBlank() || tb.isBlank()) continue
                // The floating button ("+ Event") lies over the list on purpose.
                if (insideFab(a) || insideFab(b)) continue
                val overlap = a.boundsInRoot.intersect(b.boundsInRoot)
                if (overlap.width > 4 && overlap.height > 4) {
                    val smaller = minOf(area(a.boundsInRoot), area(b.boundsInRoot))
                    if (smaller > 0 && area(overlap) > smaller * 0.3f) found += "Texte übereinander: „${ta.take(40)}“ und „${tb.take(40)}“"
                }
            }
        }
        return found.toList()
    }

    private fun area(r: Rect) = r.width.coerceAtLeast(0f) * r.height.coerceAtLeast(0f)

    private fun ancestor(a: SemanticsNode, b: SemanticsNode): Boolean {
        var p = b.parent
        while (p != null) { if (p.id == a.id) return true; p = p.parent }
        return false
    }

    private fun insideButton(n: SemanticsNode): Boolean {
        var p: SemanticsNode? = n
        while (p != null) {
            if (p.config.getOrNull(SemanticsProperties.Role) == Role.Button) return true
            p = p.parent
        }
        return false
    }

    private fun insideFab(n: SemanticsNode) = insideTag(n, de.danerd.ghulsuite.ui.FAB_TAG)

    private fun insideTag(n: SemanticsNode, tag: String): Boolean {
        var p: SemanticsNode? = n
        while (p != null) {
            if (p.config.getOrNull(SemanticsProperties.TestTag) == tag) return true
            p = p.parent
        }
        return false
    }

    private fun scrollsSideways(n: SemanticsNode): Boolean {
        var p: SemanticsNode? = n
        while (p != null) {
            if (p.config.contains(SemanticsProperties.HorizontalScrollAxisRange)) return true
            p = p.parent
        }
        return false
    }

    private fun describe(n: SemanticsNode, text: String?): String {
        val label = text ?: n.config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString(" ")
            ?: n.children.firstNotNullOfOrNull { c -> c.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text } }
        return "„${label?.take(60) ?: "(ohne Text)"}“"
    }

    private fun textLayout(n: SemanticsNode): TextLayoutResult? {
        val action = n.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action ?: return null
        val results = mutableListOf<TextLayoutResult>()
        return if (action(results)) results.firstOrNull() else null
    }

    /**
     * The word broken in the middle, if any. Long words without spaces (addresses, keys, paths,
     * fingerprints) may break anywhere; a normal word may not.
     */
    private fun brokenWord(text: String, layout: TextLayoutResult): String? {
        val t = layout.layoutInput.text.text
        for (line in 1 until layout.lineCount) {
            val start = layout.getLineStart(line)
            if (start <= 0 || start >= t.length) continue
            if (!t[start - 1].isLetterOrDigit() || !t[start].isLetterOrDigit()) continue
            var a = start - 1
            while (a > 0 && !t[a - 1].isWhitespace()) a--
            var b = start
            while (b < t.length && !t[b].isWhitespace()) b++
            val word = t.substring(a, b)
            val technical = word.length > 18 || word.any { it in "/:@._\\=+#" } || word.count { it.isDigit() } > 3
            if (!technical) return word
        }
        return null
    }
}
