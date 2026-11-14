// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.autofill

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import android.view.autofill.AutofillId
import android.view.autofill.AutofillManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.AndroidVaultBackend
import de.danerd.ghulsuite.core.AutofillLogin
import de.danerd.ghulsuite.ghul
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.GhulTheme
import de.danerd.ghulsuite.ui.Hint
import de.danerd.ghulsuite.ui.UnlockScreen
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.stringResource

/**
 * Opened by the autofill service: unlocks GhulSuite if needed and lets the user pick the login to
 * fill. Returns the chosen login as autofill dataset. A login picked by hand for an app is linked
 * to that app (`androidapp://` URL), so it is offered directly next time.
 */
class AutofillAuthActivity : ComponentActivity() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val backend = ghul.backend
        // L10: no taps while another app draws over the login choice (tapjacking).
        window.decorView.filterTouchesWhenObscured = true
        if (!backend.settings.allowScreenshots) {
            window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        }
        val domain = intent.getStringExtra(EXTRA_DOMAIN)
        val pkg = intent.getStringExtra(EXTRA_PACKAGE)
        val userIds = ids(EXTRA_USER_IDS)
        val passIds = ids(EXTRA_PASS_IDS)
        val fields = LoginFields(pkg.orEmpty(), domain, userIds, passIds, null, null)
        val state = AppState(backend, scope)
        setResult(Activity.RESULT_CANCELED)
        setContent {
            GhulTheme(backend.settings.theme) {
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    val unlocked by backend.unlocked.collectAsState()
                    Scaffold(snackbarHost = { SnackbarHost(state.snackbar) }) { _ ->
                        if (!unlocked) {
                            UnlockScreen(state)
                        } else {
                            var confirm by remember { mutableStateOf<AutofillLogin?>(null) }
                            Picker(backend, domain, pkg, onPick = { login, _ ->
                                // B9: an app is only linked to a login after the user confirmed it.
                                if (!login.direct && pkg != null) confirm = login else finishWith(backend, fields, login, link = false)
                            }) {
                                finish()
                            }
                            confirm?.let { login ->
                                val appName = remember(pkg) {
                                    runCatching {
                                        packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg!!, 0)).toString()
                                    }.getOrDefault(pkg.orEmpty())
                                }
                                de.danerd.ghulsuite.ui.AlertDialog(
                                    onDismissRequest = { confirm = null },
                                    title = { Text(stringResource(Res.string.autofill_link_title)) },
                                    text = {
                                        Text(stringResource(Res.string.autofill_link_text, appName, pkg.orEmpty(),
                                            login.title.ifBlank { stringResource(Res.string.untitled) }))
                                    },
                                    confirmButton = {
                                        TextButton(onClick = { confirm = null; finishWith(backend, fields, login, link = true) }) {
                                            Text(stringResource(Res.string.autofill_link_yes))
                                        }
                                    },
                                    dismissButton = {
                                        TextButton(onClick = { confirm = null; finishWith(backend, fields, login, link = false) }) {
                                            Text(stringResource(Res.string.autofill_link_once))
                                        }
                                    },
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private fun ids(key: String): List<AutofillId> =
        if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableArrayListExtra(key, AutofillId::class.java).orEmpty()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra<AutofillId>(key).orEmpty()
        }

    private fun finishWith(backend: AndroidVaultBackend, fields: LoginFields, login: AutofillLogin, link: Boolean) {
        val pkg = intent.getStringExtra(EXTRA_PACKAGE)
        scope.launch {
            if (link && !login.direct && pkg != null) {
                val cert = AppCert.sha256(this@AutofillAuthActivity, pkg)
                if (cert != null) runCatching { backend.linkAppToLogin(login.vaultId, login.id, pkg, cert) }
            }
            val dataset = GhulAutofillService.dataset(this@AutofillAuthActivity, fields, login)
            setResult(Activity.RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, dataset))
            finish()
        }
    }

    companion object {
        const val EXTRA_DOMAIN = "de.danerd.ghulsuite.autofill.DOMAIN"
        const val EXTRA_PACKAGE = "de.danerd.ghulsuite.autofill.PACKAGE"
        const val EXTRA_USER_IDS = "de.danerd.ghulsuite.autofill.USER_IDS"
        const val EXTRA_PASS_IDS = "de.danerd.ghulsuite.autofill.PASS_IDS"
        private val requestCodes = AtomicInteger(1)

        fun nextRequestCode() = requestCodes.getAndIncrement()
    }
}

@Composable
private fun Picker(
    backend: AndroidVaultBackend,
    domain: String?,
    pkg: String?,
    onPick: (AutofillLogin, Boolean) -> Unit,
    onCancel: () -> Unit,
) {
    var matched by remember { mutableStateOf<List<AutofillLogin>>(emptyList()) }
    var all by remember { mutableStateOf<List<AutofillLogin>>(emptyList()) }
    var query by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val context = androidx.compose.ui.platform.LocalContext.current
    LaunchedEffect(Unit) {
        scope.launch {
            val cert = pkg?.let { AppCert.sha256(context, it) }
            matched = runCatching { backend.autofillLogins(domain, pkg, cert) }.getOrDefault(emptyList())
            all = runCatching { backend.allLogins() }.getOrDefault(emptyList())
        }
    }
    val q = query.trim().lowercase()
    val others = all.filter { a -> matched.none { it.id.contentEquals(a.id) } }
        .filter { q.isEmpty() || it.title.lowercase().contains(q) || it.username.lowercase().contains(q) }
    Column(Modifier.fillMaxSize().safeDrawingPadding().padding(16.dp)) {
        Text(stringResource(Res.string.autofill_pick_title), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
        Hint(stringResource(Res.string.autofill_for, domain ?: pkg ?: "?"))
        OutlinedTextField(
            query, { query = it },
            placeholder = { Text(stringResource(Res.string.l_search)) },
            leadingIcon = { Icon(Icons.Filled.Search, null) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
        )
        LazyColumn(Modifier.weight(1f)) {
            if (matched.isNotEmpty() && q.isEmpty()) {
                item { Text(stringResource(Res.string.autofill_matching), style = MaterialTheme.typography.labelLarge) }
                items(matched) { l -> LoginRow(l) { onPick(l, true) } }
                item { HorizontalDivider() }
            }
            items(others) { l -> LoginRow(l) { onPick(l, false) } }
        }
        if (pkg != null) Hint(stringResource(Res.string.autofill_link_hint))
        TextButton(onClick = onCancel) { Text(stringResource(Res.string.a_cancel)) }
    }
}

@Composable
private fun LoginRow(login: AutofillLogin, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(login.title.ifBlank { stringResource(Res.string.untitled) }) },
        supportingContent = { if (login.username.isNotBlank()) Text(login.username) },
        modifier = Modifier.clickable(onClick = onClick),
    )
}
