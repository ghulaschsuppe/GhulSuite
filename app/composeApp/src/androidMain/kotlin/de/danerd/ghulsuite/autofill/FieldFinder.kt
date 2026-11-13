// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.autofill

import android.app.assist.AssistStructure
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId

/** Login fields found in a screen (app or web page) plus where it comes from. */
class LoginFields(
    val packageName: String,
    val webDomain: String?,
    val username: List<AutofillId>,
    val password: List<AutofillId>,
    val usernameValue: String?,
    val passwordValue: String?,
) {
    val ids: List<AutofillId> get() = username + password
    val isEmpty: Boolean get() = username.isEmpty() && password.isEmpty()

    /** A browser whose website was not recognised: never treat the browser itself as the app. */
    val unknownSite: Boolean get() = webDomain == null && Browsers.isBrowser(packageName)

    /** The app to match logins against: null for websites and for browsers. */
    val appPackage: String? get() = if (webDomain == null && !unknownSite) packageName else null
}

/** Finds user name and password fields by autofill hints, HTML attributes, input type and names. */
object FieldFinder {
    private val USER_WORDS = listOf("user", "email", "e-mail", "login", "account", "benutzer", "anmelde")
    private val PASS_WORDS = listOf("pass", "pwd", "kennwort")

    fun find(structures: List<AssistStructure>): LoginFields {
        val last = structures.last()
        var domain: String? = null
        val user = mutableListOf<AutofillId>()
        val pass = mutableListOf<AutofillId>()
        var userValue: String? = null
        var passValue: String? = null
        // Several screens (e.g. user name first, password on the next page) count for saving.
        for ((index, structure) in structures.withIndex()) {
            val texts = mutableListOf<AssistStructure.ViewNode>()
            for (i in 0 until structure.windowNodeCount) collect(structure.getWindowNodeAt(i).rootViewNode, texts)
            val isLast = index == structures.lastIndex
            var firstPassword = -1
            for ((i, node) in texts.withIndex()) {
                if (domain == null && !node.webDomain.isNullOrBlank() && structure === last) domain = node.webDomain
                val value = node.autofillValue?.takeIf { it.isText }?.textValue?.toString()
                when (kind(node)) {
                    Kind.PASSWORD -> {
                        if (firstPassword < 0) firstPassword = i
                        if (isLast) node.autofillId?.let(pass::add)
                        if (!value.isNullOrEmpty()) passValue = value
                    }
                    Kind.USERNAME -> {
                        if (isLast) node.autofillId?.let(user::add)
                        if (!value.isNullOrEmpty()) userValue = value
                    }
                    Kind.OTHER -> {}
                }
            }
            // No recognisable user name field: take the text field right before the password.
            if (isLast && user.isEmpty() && firstPassword > 0) {
                val candidate = texts[firstPassword - 1]
                if (kind(candidate) == Kind.OTHER) {
                    candidate.autofillId?.let(user::add)
                    candidate.autofillValue?.takeIf { it.isText }?.textValue?.toString()?.let { if (it.isNotEmpty()) userValue = it }
                }
            }
        }
        return LoginFields(last.activityComponent.packageName, domain, user, pass, userValue, passValue)
    }

    private fun collect(node: AssistStructure.ViewNode, out: MutableList<AssistStructure.ViewNode>) {
        if (node.visibility == View.VISIBLE && node.autofillType == View.AUTOFILL_TYPE_TEXT && node.autofillId != null) {
            out.add(node)
        }
        for (i in 0 until node.childCount) collect(node.getChildAt(i), out)
    }

    private enum class Kind { USERNAME, PASSWORD, OTHER }

    private fun kind(node: AssistStructure.ViewNode): Kind {
        val hints = node.autofillHints?.map { it.lowercase() }.orEmpty()
        if (hints.any { "password" in it }) return Kind.PASSWORD
        if (hints.any { it == "username" || it.contains("email") || it == "phone" }) return Kind.USERNAME
        val html = node.htmlInfo?.attributes?.associate { it.first.lowercase() to it.second.lowercase() }.orEmpty()
        if (html["type"] == "password" || html["autocomplete"]?.contains("password") == true) return Kind.PASSWORD
        if (html["type"] == "email" || html["autocomplete"] == "username" || html["autocomplete"] == "email") {
            return Kind.USERNAME
        }
        val type = node.inputType
        if (type and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_TEXT) {
            when (type and InputType.TYPE_MASK_VARIATION) {
                InputType.TYPE_TEXT_VARIATION_PASSWORD,
                InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD,
                InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,
                -> return Kind.PASSWORD
                InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
                InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS,
                -> return Kind.USERNAME
            }
        }
        if (type and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_NUMBER &&
            type and InputType.TYPE_MASK_VARIATION == InputType.TYPE_NUMBER_VARIATION_PASSWORD
        ) {
            return Kind.PASSWORD
        }
        val words = listOfNotNull(node.idEntry, node.hint, html["name"], html["id"]).joinToString(" ").lowercase()
        if (PASS_WORDS.any { it in words }) return Kind.PASSWORD
        if (USER_WORDS.any { it in words }) return Kind.USERNAME
        return Kind.OTHER
    }
}
