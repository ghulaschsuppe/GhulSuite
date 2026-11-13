// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Typeface
import android.os.Build
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.io.File
import java.io.PrintWriter
import java.io.StringWriter

/**
 * Remembers a crash and shows it on the next start, so it can be copied into a bug report.
 * The report holds only the error and device information, no vault content (secrets never
 * appear in exception messages of the app).
 */
object CrashLog {
    private fun file(context: Context) = File(context.filesDir, "last-crash.txt")

    fun install(context: Context) {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        val app = context.applicationContext
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            runCatching { write(app, error, thread.name) }
            previous?.uncaughtException(thread, error)
        }
    }

    fun write(context: Context, error: Throwable, where: String = "start") {
        val trace = StringWriter().also { error.printStackTrace(PrintWriter(it)) }.toString()
        val version = runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull()
        val text = buildString {
            appendLine("GhulSuite $version · Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}) · ${Build.MANUFACTURER} ${Build.MODEL}")
            appendLine("Thread: $where")
            appendLine()
            append(trace)
        }
        file(context).writeText(text)
    }

    fun pending(context: Context): String? = file(context).takeIf { it.exists() }?.readText()

    fun clear(context: Context) {
        file(context).delete()
    }

    /** Plain Android views (no Compose), so the report shows even if the UI itself is broken. */
    fun show(activity: Activity, report: String) {
        val de = activity.resources.configuration.locales[0].language == "de"
        val pad = (16 * activity.resources.displayMetrics.density).toInt()
        val text = TextView(activity).apply {
            setTextIsSelectable(true)
            typeface = Typeface.MONOSPACE
            textSize = 11f
            this.text = report
        }
        val title = TextView(activity).apply {
            textSize = 18f
            setTypeface(typeface, Typeface.BOLD)
            this.text = if (de) "GhulSuite ist abgestürzt" else "GhulSuite crashed"
        }
        val hint = TextView(activity).apply {
            this.text = if (de) {
                "Bitte kopiere diesen Bericht und schicke ihn mit. Er enthält keine Passwörter oder Tresor-Inhalte."
            } else {
                "Please copy this report and send it along. It contains no passwords or vault content."
            }
        }
        val copy = Button(activity).apply {
            this.text = if (de) "Bericht kopieren" else "Copy report"
            setOnClickListener {
                activity.getSystemService(ClipboardManager::class.java)
                    .setPrimaryClip(ClipData.newPlainText("GhulSuite crash", report))
            }
        }
        val restart = Button(activity).apply {
            this.text = if (de) "Schließen und neu starten" else "Close and restart"
            setOnClickListener {
                clear(activity)
                activity.startActivity(
                    Intent(activity, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK),
                )
                Runtime.getRuntime().exit(0)
            }
        }
        val column = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad * 3, pad, pad)
            addView(title)
            addView(hint)
            addView(copy)
            addView(restart)
            addView(text)
        }
        activity.setContentView(ScrollView(activity).apply { addView(column, ViewGroup.LayoutParams(-1, -2)) })
    }
}
