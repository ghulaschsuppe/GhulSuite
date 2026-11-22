// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context

/**
 * Review N-a: the folder grants the app took itself for a safe place of the suite file. Only
 * these are ever given back (after the old file was deleted, or when a chosen folder is not taken
 * after all); grants the user gave for other things (a vault open on its own) stay.
 */
object SafePlaceGrants {
    private const val PREFS = "safe_place_grants"
    private const val KEY = "trees"

    fun all(context: Context): Set<String> =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getStringSet(KEY, emptySet()).orEmpty()

    fun add(context: Context, tree: String) {
        val p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        p.edit().putStringSet(KEY, all(context) + tree).commit()
    }

    fun forget(context: Context, tree: String) {
        val p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        p.edit().putStringSet(KEY, all(context) - tree).commit()
    }
}
