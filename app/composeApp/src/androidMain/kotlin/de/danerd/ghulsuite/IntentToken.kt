// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context
import android.content.Intent
import java.security.SecureRandom

/**
 * B7: the internal actions of MainActivity ("open mail", "open event") come only from the app's
 * own notifications. They carry a random token known only to this installation; MainActivity is
 * exported (launcher), so another app could otherwise send these actions as well.
 */
object IntentToken {
    private const val PREFS = "intent_token"
    private const val KEY = "token"
    const val EXTRA = "de.danerd.ghulsuite.TOKEN"

    fun get(ctx: Context): String {
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        prefs.getString(KEY, null)?.let { return it }
        val bytes = ByteArray(16).also { SecureRandom().nextBytes(it) }
        val token = bytes.joinToString("") { "%02x".format(it) }
        prefs.edit().putString(KEY, token).apply()
        return token
    }

    /** Whether the intent came from the app itself. */
    fun valid(ctx: Context, intent: Intent): Boolean {
        val given = intent.getStringExtra(EXTRA) ?: return false
        return java.security.MessageDigest.isEqual(given.toByteArray(), get(ctx).toByteArray())
    }
}
