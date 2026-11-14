// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.runtime.getValue
import androidx.compose.ui.unit.dp
import androidx.compose.animation.core.animateFloat
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Fingerprint
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource
import androidx.compose.material.icons.filled.Info

private enum class UnlockMode { QUICK, FULL, RECOVERY }

@Composable
fun UnlockScreen(state: AppState) {
    val backend = state.backend
    val protection = remember { backend.protection() }
    val hasKey = (protection?.securityKeys ?: 1) > 0
    val hasKeyfile = (protection?.keyfiles ?: 0) > 0
    var withKey by remember { mutableStateOf(hasKey) }
    val needsKey = withKey && hasKey
    var ready by remember { mutableStateOf(backend.quickUnlockReady()) }
    var mode by remember { mutableStateOf(if (ready.any) UnlockMode.QUICK else UnlockMode.FULL) }
    var password by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var appPin by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    fun clear() {
        password = ""; repeat = ""; pin = ""; appPin = ""; code = ""; keyfile = null; error = null
    }

    /** Quick unlock failed for good: fall back to the full unlock. */
    suspend fun quickFailed(e: Throwable) {
        error = describe(e)
        ready = backend.quickUnlockReady()
        val kind = (e as? VaultException)?.kind
        if (!ready.any || kind == VaultErrorKind.QUICK_EXPIRED || kind == VaultErrorKind.BIOMETRIC_CHANGED) {
            mode = UnlockMode.FULL
        }
    }

    // When the last fingerprint prompt ended: the prompt itself can pause and resume the app,
    // which must not open it again right away (a cancelled prompt would come back forever).
    var promptEnded by remember { mutableStateOf<kotlin.time.TimeSource.Monotonic.ValueTimeMark?>(null) }

    fun biometric() {
        state.run(onError = { error = it; promptEnded = kotlin.time.TimeSource.Monotonic.markNow() }, block = {
            try {
                backend.unlockWithBiometric()
                true
            } catch (e: VaultException) {
                if (e.kind != VaultErrorKind.BIOMETRIC_CANCELLED) quickFailed(e) else error = null
                false
            }
        }) { ok ->
            promptEnded = kotlin.time.TimeSource.Monotonic.markNow()
            if (ok) clear()
        }
    }

    // Ask for the fingerprint at once: when the screen appears and every time the app comes back
    // to the front while locked, without tapping "Unlock with fingerprint" first.
    // Never while in the background (Android would refuse the prompt).
    val resumed by AppLaunch.resumed.collectAsState()
    val inFront by AppLaunch.inFront.collectAsState()
    LaunchedEffect(resumed, inFront) {
        val recent = promptEnded?.let { it.elapsedNow() < kotlin.time.Duration.parse("1.5s") } == true
        if (inFront && mode == UnlockMode.QUICK && ready.biometric && !state.busy && !recent) biometric()
    }

    val t = g
    @Composable
    fun Links(vararg rows: Pair<String, () -> Unit>) {
        ListCard {
            rows.forEachIndexed { i, (label, action) ->
                ListRow(label, divider = i > 0, trailing = { ChevronEnd() }, onClick = action)
            }
        }
    }

    ScreenColumn {
        BrandHeader()
        BusyBanner(state)
        // Vault files without the suite file: the setup stopped halfway; unlocking completes it.
        val incomplete = remember { backend.setupState() == de.danerd.ghulsuite.model.SetupStateUi.INCOMPLETE }
        if (incomplete) Banner(androidx.compose.material.icons.Icons.Filled.Info, stringResource(Res.string.setup_incomplete))
        when (mode) {
            UnlockMode.QUICK -> {
                if (ready.pin) {
                    SecretTextField(appPin, { appPin = it; error = null }, stringResource(Res.string.l_app_pin), numeric = true)
                    Button(onClick = {
                        state.run(onError = { error = it; appPin = "" }, block = {
                            try {
                                backend.unlockWithPin(appPin)
                                true
                            } catch (e: VaultException) {
                                if (e.kind == VaultErrorKind.QUICK_INVALID && (e.retries ?: 0) > 0) throw e
                                quickFailed(e)
                                appPin = ""
                                false
                            }
                        }) { ok -> if (ok) clear() }
                    }, enabled = !state.busy && appPin.length >= 4, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.Lock, null, Modifier.size(18.dp))
                        Text(stringResource(Res.string.a_unlock_pin))
                    }
                }
                if (ready.biometric) {
                    FilledTonalButton(onClick = { biometric() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Filled.Fingerprint, null, Modifier.size(18.dp))
                        Text(stringResource(Res.string.a_unlock_fingerprint))
                    }
                }
                ErrorText(error)
                Spacer(Modifier.height(6.dp))
                Links(
                    stringResource(if (hasKey) Res.string.unlock_use_full else Res.string.unlock_use_password) to { mode = UnlockMode.FULL; error = null },
                    stringResource(Res.string.unlock_other_suite) to { clear(); state.showStart = true },
                )
            }

            UnlockMode.FULL -> {
                if (hasKey && hasKeyfile) FactorChoice(withKey) { withKey = it; error = null }
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
                if (needsKey) {
                    if (backend.keyPinInApp) SecretTextField(pin, { pin = it; error = null }, stringResource(Res.string.l_key_pin))
                    // Key hint (.keyhint): soft accent, pulsing key while it waits for a touch.
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(t.accentSoft).padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        val pulse = androidx.compose.animation.core.rememberInfiniteTransition()
                        val a by pulse.animateFloat(1f, 0.45f, androidx.compose.animation.core.infiniteRepeatable(
                            androidx.compose.animation.core.tween(800), androidx.compose.animation.core.RepeatMode.Reverse))
                        Icon(Icons.Filled.Usb, null, Modifier.size(28.dp).alpha(if (state.touchNeeded) a else 1f), tint = t.accent)
                        Text(
                            stringResource(if (backend.keyPinInApp) Res.string.unlock_key_hint else Res.string.key_system_dialog),
                            color = t.accent, fontWeight = FontWeight.SemiBold,
                        )
                    }
                } else {
                    KeyfilePicker(keyfile) { keyfile = it; error = null }
                }
                ErrorText(error)
                Button(
                    onClick = {
                        state.run(onError = { error = it }, block = {
                            if (needsKey) {
                                backend.unlock(password, pin, null, state.onTouchNeeded)
                            } else {
                                backend.unlock(password, null, keyfile?.bytes, state.onTouchNeeded)
                            }
                        }) { clear() }
                    },
                    enabled = !state.busy && password.isNotEmpty() && (if (needsKey) keyPinOk(state, pin) else keyfile != null),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(Icons.Outlined.Lock, null, Modifier.size(18.dp))
                    Text(stringResource(Res.string.a_unlock))
                }
                if (ready.biometric) {
                    FilledTonalButton(onClick = { mode = UnlockMode.QUICK; biometric() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Filled.Fingerprint, null, Modifier.size(18.dp))
                        Text(stringResource(Res.string.a_unlock_fingerprint))
                    }
                } else if (ready.any) {
                    FilledTonalButton(onClick = { mode = UnlockMode.QUICK; error = null }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.unlock_use_quick))
                    }
                }
                Spacer(Modifier.height(6.dp))
                Links(
                    stringResource(Res.string.unlock_other_suite) to { clear(); state.showStart = true },
                    stringResource(Res.string.unlock_use_code) to { clear(); mode = UnlockMode.RECOVERY },
                )
                HintText(stringResource(Res.string.storage_current, storageLabel(backend.storageName())))
            }

            UnlockMode.RECOVERY -> {
                SectionLabel(stringResource(Res.string.unlock_code_title))
                HintText(stringResource(Res.string.unlock_code_text))
                OutlinedTextField(
                    code, { code = it; error = null }, label = { Text(stringResource(Res.string.l_recovery_code)) },
                    modifier = Modifier.fillMaxWidth(),
                )
                Hint(stringResource(Res.string.unlock_code_new_password))
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_new_password))
                StrengthMeter(backend.passwordScore(password))
                SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
                ErrorText(error)
                ButtonRow {
                OutlinedButton(onClick = { clear(); mode = UnlockMode.FULL }, modifier = Modifier.weight(1f)) {
                    Text(stringResource(Res.string.a_back))
                }
                Button(
                    onClick = {
                        scope.launch {
                            error = PasswordPolicy.check(backend, password, repeat)
                            if (error != null) return@launch
                            state.run(onError = { error = it }, block = { backend.unlockWithRecoveryCode(code, password) }) {
                                clear()
                                // U11: what to do now is shown right after unlocking, not only as a toast.
                                state.recoveredGuide = true
                            }
                        }
                    },
                    enabled = !state.busy && code.isNotBlank() && password.isNotEmpty(),
                    modifier = Modifier.weight(1f),
                ) { Text(stringResource(Res.string.a_unlock)) }
                }
            }
        }
    }
}
