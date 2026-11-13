// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.autofill

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.autofill.FillRequest
import android.service.autofill.InlinePresentation
import androidx.annotation.RequiresApi
import androidx.autofill.inline.UiVersions
import androidx.autofill.inline.v1.InlineSuggestionUi
import de.danerd.ghulsuite.MainActivity
import de.danerd.ghulsuite.R

/**
 * Suggestions shown in the keyboard's suggestion bar (Android 11+, keyboards such as Gboard or
 * Samsung). Many keyboards show only these and no drop-down below the field.
 */
class InlineChips private constructor(private val context: Context, private val request: Any) {

    /** The chip for the [index]-th dataset, or null if the keyboard has no room or no support. */
    fun chip(index: Int, title: String, subtitle: String?): InlinePresentation? =
        if (Build.VERSION.SDK_INT >= 30) build(index, title, subtitle) else null

    @SuppressLint("RestrictedApi")
    @RequiresApi(30)
    private fun build(index: Int, title: String, subtitle: String?): InlinePresentation? {
        val r = request as android.view.inputmethod.InlineSuggestionsRequest
        val specs = r.inlinePresentationSpecs
        if (specs.isEmpty() || index >= r.maxSuggestionCount) return null
        val spec = specs[minOf(index, specs.size - 1)]
        if (UiVersions.INLINE_UI_VERSION_1 !in UiVersions.getVersions(spec.style)) return null
        // Long-pressing a chip opens GhulSuite; it must be immutable and reveal nothing.
        val attribution = PendingIntent.getActivity(
            context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val content = InlineSuggestionUi.newContentBuilder(attribution)
            .setTitle(title)
            .setContentDescription(title)
            .setStartIcon(Icon.createWithResource(context, R.drawable.ic_lock).setTint(0xFF4F7CFF.toInt()))
        if (!subtitle.isNullOrBlank()) content.setSubtitle(subtitle)
        return InlinePresentation(content.build().slice, spec, false)
    }

    companion object {
        /** Chips for this fill request, or null if the keyboard did not ask for any. */
        fun of(context: Context, request: FillRequest): InlineChips? {
            if (Build.VERSION.SDK_INT < 30) return null
            val inline = request.inlineSuggestionsRequest ?: return null
            return InlineChips(context, inline)
        }
    }
}
