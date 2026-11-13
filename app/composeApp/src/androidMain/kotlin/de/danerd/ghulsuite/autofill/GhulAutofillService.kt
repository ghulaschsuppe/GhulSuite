// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.autofill

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.IntentSender
import android.os.Build
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveInfo
import android.service.autofill.SaveRequest
import android.view.autofill.AutofillId
import android.view.autofill.AutofillValue
import android.widget.RemoteViews
import android.widget.Toast
import de.danerd.ghulsuite.R
import de.danerd.ghulsuite.core.AutofillLogin
import de.danerd.ghulsuite.core.AutofillSaved
import de.danerd.ghulsuite.ghul
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Offers matching logins in other apps and in browsers, below the field and, where the keyboard
 * supports it, in the keyboard's suggestion bar. While GhulSuite is locked, only an
 * "Unlock GhulSuite" entry is offered; it opens [AutofillAuthActivity]. Nothing is revealed
 * to the requesting app before the user picks an entry.
 */
class GhulAutofillService : AutofillService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
        val fields = FieldFinder.find(request.fillContexts.map { it.structure })
        if (fields.packageName == packageName || fields.isEmpty) {
            callback.onSuccess(null)
            return
        }
        val backend = applicationContext.ghul.backend
        val appPackage = fields.appPackage
        val cert = appPackage?.let { AppCert.sha256(this, it) }
        val chips = InlineChips.of(this, request)
        val job = scope.launch {
            val response = FillResponse.Builder()
            var index = 0
            if (backend.unlocked.value) {
                // Only exact matches are offered directly; suggestions need a deliberate pick.
                val logins = if (fields.unknownSite) {
                    emptyList()
                } else {
                    runCatching { backend.autofillLogins(fields.webDomain, appPackage, cert) }
                        .getOrDefault(emptyList())
                        .filter { it.direct }
                }
                for (l in logins.take(MAX_DATASETS)) {
                    response.addDataset(dataset(this@GhulAutofillService, fields, l, chips, index++))
                }
                response.addDataset(pickerDataset(fields, getString(R.string.autofill_pick), chips, index))
            } else {
                response.addDataset(pickerDataset(fields, getString(R.string.autofill_unlock), chips, index))
            }
            // Saving needs to know the website or app; a browser without a recognised site has neither.
            if (fields.password.isNotEmpty() && !fields.unknownSite) {
                var type = SaveInfo.SAVE_DATA_TYPE_PASSWORD
                if (fields.username.isNotEmpty()) type = type or SaveInfo.SAVE_DATA_TYPE_USERNAME
                response.setSaveInfo(SaveInfo.Builder(type, fields.ids.toTypedArray()).build())
            }
            runCatching { callback.onSuccess(response.build()) }
        }
        cancellationSignal.setOnCancelListener { job.cancel() }
    }

    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        val fields = FieldFinder.find(request.fillContexts.map { it.structure })
        val password = fields.passwordValue
        val backend = applicationContext.ghul.backend
        if (password.isNullOrEmpty()) {
            callback.onSuccess()
            return
        }
        if (fields.unknownSite) {
            callback.onSuccess()
            return
        }
        if (!backend.unlocked.value) {
            toast(R.string.autofill_save_locked)
            callback.onSuccess()
            return
        }
        val appPackage = fields.appPackage
        val cert = appPackage?.let { AppCert.sha256(this, it) }
        scope.launch {
            val result = runCatching {
                backend.autofillSave(fields.webDomain, appPackage, cert, fields.usernameValue.orEmpty(), password)
            }
            toast(
                when (result.getOrNull()) {
                    AutofillSaved.CREATED -> R.string.autofill_saved
                    AutofillSaved.UPDATED -> R.string.autofill_updated
                    AutofillSaved.UNCHANGED -> R.string.autofill_unchanged
                    null -> R.string.autofill_save_failed
                },
            )
            callback.onSuccess()
        }
    }

    private fun toast(text: Int) = Toast.makeText(applicationContext, text, Toast.LENGTH_SHORT).show()

    /** Entry that opens GhulSuite (unlock if needed) and lets the user pick a login. */
    @Suppress("DEPRECATION")
    private fun pickerDataset(fields: LoginFields, label: String, chips: InlineChips?, index: Int): Dataset {
        val presentation = RemoteViews(packageName, android.R.layout.simple_list_item_1).apply {
            setTextViewText(android.R.id.text1, label)
        }
        val chip = chips?.chip(index, label, null)
        val builder = Dataset.Builder(presentation)
        for (id in fields.ids) {
            if (chip != null && Build.VERSION.SDK_INT >= 30) builder.setValue(id, null, presentation, chip)
            else builder.setValue(id, null, presentation)
        }
        builder.setAuthentication(AutofillAuthActivity.intentSender(this, fields))
        return builder.build()
    }

    companion object {
        private const val MAX_DATASETS = 8

        /** A dataset that fills [login] into the fields; shown with title and user name. */
        @Suppress("DEPRECATION")
        fun dataset(context: Context, fields: LoginFields, login: AutofillLogin, chips: InlineChips? = null, index: Int = 0): Dataset {
            val title = login.title.ifBlank { login.username }
            val presentation = RemoteViews(context.packageName, android.R.layout.simple_list_item_2).apply {
                setTextViewText(android.R.id.text1, title)
                setTextViewText(android.R.id.text2, login.username)
            }
            val chip = chips?.chip(index, title, login.username.takeIf { it != title })
            val builder = Dataset.Builder(presentation)
            val values = fields.username.map { it to login.username } + fields.password.map { it to login.password }
            for ((id, text) in values) {
                val value = AutofillValue.forText(text)
                if (chip != null && Build.VERSION.SDK_INT >= 30) builder.setValue(id, value, presentation, chip)
                else builder.setValue(id, value, presentation)
            }
            return builder.build()
        }

        internal fun idsExtra(ids: List<AutofillId>) = ArrayList(ids)
    }
}

internal fun AutofillAuthActivity.Companion.intentSender(context: Context, fields: LoginFields): IntentSender {
    val intent = Intent(context, AutofillAuthActivity::class.java)
        .putExtra(AutofillAuthActivity.EXTRA_DOMAIN, fields.webDomain)
        .putExtra(AutofillAuthActivity.EXTRA_PACKAGE, fields.appPackage)
        .putParcelableArrayListExtra(AutofillAuthActivity.EXTRA_USER_IDS, GhulAutofillService.idsExtra(fields.username))
        .putParcelableArrayListExtra(AutofillAuthActivity.EXTRA_PASS_IDS, GhulAutofillService.idsExtra(fields.password))
    val flags = PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_MUTABLE
    return PendingIntent.getActivity(context, AutofillAuthActivity.nextRequestCode(), intent, flags).intentSender
}
