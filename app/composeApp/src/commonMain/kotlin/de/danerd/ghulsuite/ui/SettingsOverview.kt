// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material3.Surface
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Apps
import androidx.compose.material.icons.filled.Save
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Fingerprint
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Password
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.SlotType
import de.danerd.ghulsuite.model.SlotUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource
import de.danerd.ghulsuite.model.VaultKindUi

private val Violet = Color(0xFF7B61FF)
private val Red = Color(0xFFD03B4C)
private val Gray = Color(0xFF6F7187)
private val Blue = Color(0xFF2F74E8)
private val Teal = Color(0xFF0E9F80)
private val Cyan = Color(0xFF1E9BB8)
private val Pink = Color(0xFFD03B7E)

/** Title of a settings page (top bar). */
fun SetPage.title(): StringResource = when (this) {
    SetPage.UNLOCK -> Res.string.set_unlock
    SetPage.LOCK -> Res.string.set_lock_panic
    SetPage.PRIVACY -> Res.string.set_privacy
    SetPage.FACTORS -> Res.string.set_factors
    SetPage.VAULTS -> Res.string.set_vaults
    SetPage.FILE -> Res.string.file_title
    SetPage.IMPORT -> Res.string.set_import_export
    SetPage.MODULES -> Res.string.set_modules
    SetPage.PASS -> Res.string.tab_pass
    SetPage.AUTH -> Res.string.tab_auth
    SetPage.DOCS -> Res.string.tab_docs
    SetPage.CAL -> Res.string.tab_cal
    SetPage.LOOK -> Res.string.set_view
    SetPage.ABOUT -> Res.string.set_about
}

@Composable
private fun SetRow(icon: ImageVector, color: Color, title: String, subtitle: String, first: Boolean = false, onClick: () -> Unit) {
    ListRow(
        title = title,
        subtitle = subtitle,
        leading = { IconTile(icon, color, solid = true) },
        divider = !first,
        trailing = { ChevronEnd() },
        onClick = onClick,
    )
}

@Composable
private fun Group(title: String, content: @Composable () -> Unit) {
    SectionLabel(title)
    ListCard { content() }
}

@Composable
fun SettingsScreen(state: AppState, padding: PaddingValues) {
    val backend = state.backend
    val settings = backend.settings
    var slots by remember { mutableStateOf<List<SlotUi>>(emptyList()) }
    LaunchedEffect(state.liveRevision()) {
        slots = catching { backend.slots() }.getOrDefault(emptyList())
    }
    val open = { p: SetPage -> state.push(Screen.SettingsPage(p)) }
    val keys = slots.count { it.type == SlotType.SECURITY_KEY }
    val files = slots.count { it.type == SlotType.KEYFILE }

    if (state.single != null) {
        // A vault opened on its own: no options of the own suite.
        Column(Modifier.padding(padding)) { ScreenColumn { SingleVaultSection(state) } }
        return
    }
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            SectionCard {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    IconTile(Icons.Outlined.Shield, paletteColor(0), size = 64.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(Res.string.set_my_suite), style = MaterialTheme.typography.titleMedium)
                        Hint(pluralStringResource(Res.plurals.set_suite_summary, state.vaults.size, state.vaults.size.toString()))
                        val how = stringResource(
                            when {
                                keys > 0 -> Res.string.prot_pw_key
                                files > 0 -> Res.string.prot_pw_file
                                else -> Res.string.prot_pw
                            },
                        )
                        Surface(shape = MaterialTheme.shapes.small, color = okColor().copy(alpha = 0.12f)) {
                            Row(Modifier.padding(horizontal = 8.dp, vertical = 3.dp), verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                Icon(Icons.Filled.VerifiedUser, null, Modifier.size(14.dp), tint = okColor())
                                Text(stringResource(Res.string.set_suite_protected, how), style = MaterialTheme.typography.labelMedium,
                                    color = okColor(), fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                }
            }
            Surface(shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface, modifier = Modifier.fillMaxWidth()) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(Res.string.a_lock_now), style = MaterialTheme.typography.titleSmall)
                        // The shake hint only where shaking is switched on.
                        Hint(stringResource(if (backend.isDesktop || !settings.shakeToLock) Res.string.set_lock_hint_short else Res.string.lock_now_shake))
                    }
                    FilledTonalButton(onClick = { state.lockNow() }) {
                        Icon(Icons.Filled.Lock, null, Modifier.size(18.dp))
                        Text(stringResource(Res.string.a_lock))
                    }
                }
            }

            Group(stringResource(Res.string.set_g_security)) {
                if (!backend.isDesktop) {
                    // What is really switched on (and ready on this device), with its validity.
                    val ready = remember(state.revision) { backend.quickUnlockReady() }
                    val bio = settings.quickUnlockBiometric && ready.biometric
                    val pin = settings.quickUnlockPin && ready.pin
                    val method = when {
                        bio && pin -> stringResource(Res.string.set_unlock_sub_both)
                        bio -> stringResource(Res.string.set_quick_bio)
                        pin -> stringResource(Res.string.set_quick_pin)
                        else -> null
                    }
                    val validity = if (settings.quickUnlockHours == 0) stringResource(Res.string.set_unlock_no_expiry)
                    else QUICK_HOURS_OPTIONS.firstOrNull { it.first == settings.quickUnlockHours }?.let { stringResource(it.second) }
                    SetRow(Icons.Filled.Fingerprint, Violet, stringResource(Res.string.set_unlock),
                        if (method == null) stringResource(Res.string.set_unlock_sub_off) else listOfNotNull(method, validity).joinToString(" · "),
                        first = true) { open(SetPage.UNLOCK) }
                }
                SetRow(Icons.Filled.Lock, Red, stringResource(Res.string.set_lock_panic),
                    stringResource(if (backend.isDesktop) Res.string.set_lock_panic_sub_desktop else Res.string.set_lock_panic_sub), first = backend.isDesktop) { open(SetPage.LOCK) }
                SetRow(Icons.Filled.VisibilityOff, Gray, stringResource(Res.string.set_privacy), stringResource(Res.string.set_privacy_sub)) { open(SetPage.PRIVACY) }
            }
            Group(stringResource(Res.string.set_g_protection)) {
                SetRow(Icons.Filled.Usb, Violet, stringResource(Res.string.set_factors),
                    stringResource(Res.string.set_factors_sub, keys.toString(), files.toString()), first = true) { open(SetPage.FACTORS) }
                SetRow(Icons.Filled.Key, Blue, stringResource(Res.string.set_password), stringResource(Res.string.a_change_password)) {
                    state.push(Screen.ChangePassword)
                }
            }
            Group(stringResource(Res.string.set_g_data)) {
                SetRow(Icons.Filled.Save, Blue, stringResource(Res.string.file_title),
                    stringResource(if (state.unsaved) Res.string.file_unsaved_short else Res.string.file_sub), first = true) { open(SetPage.FILE) }
                SetRow(Icons.Filled.Layers, Teal, stringResource(Res.string.set_vaults),
                    pluralStringResource(Res.plurals.set_vaults_sub, state.vaults.size, state.vaults.size.toString())) { open(SetPage.VAULTS) }
                SetRow(Icons.Filled.Download, Cyan, stringResource(Res.string.set_import_export), stringResource(Res.string.set_import_sub)) { open(SetPage.IMPORT) }
            }
            Group(stringResource(Res.string.set_g_areas)) {
                SetRow(Icons.Filled.Apps, Gray, stringResource(Res.string.set_modules), stringResource(Res.string.set_modules_sub), first = true) { open(SetPage.MODULES) }
                if (state.vaults.any { it.kind == VaultKindUi.PASS }) {
                    SetRow(tabIcon(Tab.PASS), areaColor(Tab.PASS), stringResource(Res.string.tab_pass),
                        stringResource(Res.string.set_pass_sub)) { open(SetPage.PASS) }
                }
                if (state.vaults.any { it.kind == VaultKindUi.TOTP }) {
                    SetRow(tabIcon(Tab.AUTH), areaColor(Tab.AUTH), stringResource(Res.string.tab_auth), stringResource(Res.string.set_auth_sub)) { open(SetPage.AUTH) }
                }
                if (state.vaults.any { it.kind == VaultKindUi.DOCS }) {
                    SetRow(tabIcon(Tab.DOCS), areaColor(Tab.DOCS), stringResource(Res.string.tab_docs), stringResource(Res.string.set_docs_sub)) { open(SetPage.DOCS) }
                }
                if (state.vaults.any { it.kind == VaultKindUi.CAL }) {
                    SetRow(tabIcon(Tab.CAL), areaColor(Tab.CAL), stringResource(Res.string.tab_cal),
                        stringResource(if (settings.calHolidays && state.calRegionName.isNotEmpty()) Res.string.set_cal_sub_holidays else Res.string.set_cal_sub, state.calRegionName)) { open(SetPage.CAL) }
                }
            }
            Group(stringResource(Res.string.set_g_general)) {
                SetRow(Icons.Filled.Palette, Pink, stringResource(Res.string.set_view), stringResource(Res.string.set_view_sub), first = true) { open(SetPage.LOOK) }
                SetRow(Icons.Filled.Info, Gray, stringResource(Res.string.set_about), "GhulSuite ${backend.appVersion}") { open(SetPage.ABOUT) }
            }
        }
    }
}
