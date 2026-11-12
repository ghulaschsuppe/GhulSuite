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
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Email
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
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.SlotType
import de.danerd.ghulsuite.model.SlotUi
import de.danerd.ghulsuite.model.SyncStatusUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource

private val Violet = Color(0xFF7B61FF)
private val Red = Color(0xFFD03B4C)
private val Gray = Color(0xFF6F7187)
private val Blue = Color(0xFF2F74E8)
private val Teal = Color(0xFF0E9F80)
private val Cyan = Color(0xFF1E9BB8)
private val Orange = Color(0xFFE0700F)
private val Pink = Color(0xFFD03B7E)

/** Title of a settings page (top bar). */
fun SetPage.title(): StringResource = when (this) {
    SetPage.UNLOCK -> Res.string.set_unlock
    SetPage.LOCK -> Res.string.set_lock_panic
    SetPage.PRIVACY -> Res.string.set_privacy
    SetPage.FACTORS -> Res.string.set_factors
    SetPage.VAULTS -> Res.string.set_vaults
    SetPage.IMPORT -> Res.string.set_import_export
    SetPage.SERVERS -> Res.string.set_servers_sync_share
    SetPage.MODULES -> Res.string.set_modules
    SetPage.PASS -> Res.string.tab_pass
    SetPage.AUTH -> Res.string.tab_auth
    SetPage.DRIVE -> Res.string.tab_drive
    SetPage.MAIL -> Res.string.tab_mail
    SetPage.LOOK -> Res.string.set_view
    SetPage.ABOUT -> Res.string.set_about
}

@Composable
private fun SetRow(icon: ImageVector, color: Color, title: String, subtitle: String, first: Boolean = false, onClick: () -> Unit) {
    ListRow(
        title = title,
        subtitle = subtitle,
        leading = { IconTile(icon, color, size = 36.dp, solid = true) },
        divider = !first,
        dividerStart = 66.dp,
        trailing = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        onClick = onClick,
    )
}

@Composable
private fun Group(title: String, content: @Composable () -> Unit) {
    Text(
        title.uppercase(),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(start = 4.dp, top = 6.dp),
    )
    ListCard { content() }
}

@Composable
fun SettingsScreen(state: AppState, padding: PaddingValues) {
    val backend = state.backend
    val settings = backend.settings
    var slots by remember { mutableStateOf<List<SlotUi>>(emptyList()) }
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    var sync by remember { mutableStateOf(SyncStatusUi(null, 0)) }
    LaunchedEffect(state.liveRevision()) {
        slots = runCatching { backend.slots() }.getOrDefault(emptyList())
        servers = runCatching { backend.net.servers() }.getOrDefault(emptyList())
        sync = runCatching { backend.net.syncStatus() }.getOrDefault(SyncStatusUi(null, 0))
    }
    val open = { p: SetPage -> state.push(Screen.SettingsPage(p)) }
    val keys = slots.count { it.type == SlotType.SECURITY_KEY }
    val files = slots.count { it.type == SlotType.KEYFILE }

    Column(Modifier.padding(padding)) {
        ScreenColumn {
            SectionCard {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    IconTile(Icons.Outlined.Shield, paletteColor(0), size = 64.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(Res.string.set_my_suite), style = MaterialTheme.typography.titleMedium)
                        Hint(stringResource(Res.string.set_suite_summary, state.vaults.size.toString(), storageLabel(backend.storageName())))
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
                        Hint(stringResource(if (backend.isDesktop) Res.string.set_lock_hint_short else Res.string.lock_now_shake))
                    }
                    FilledTonalButton(onClick = { backend.lock() }) {
                        Icon(Icons.Filled.Lock, null)
                        Text(" " + stringResource(Res.string.a_lock))
                    }
                }
            }

            Group(stringResource(Res.string.set_g_security)) {
                if (!backend.isDesktop) {
                    SetRow(Icons.Filled.Fingerprint, Violet, stringResource(Res.string.set_unlock),
                        stringResource(if (settings.quickUnlockBiometric || settings.quickUnlockPin) Res.string.set_unlock_sub_on else Res.string.set_unlock_sub_off), first = true) { open(SetPage.UNLOCK) }
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
                SetRow(Icons.Filled.Layers, Teal, stringResource(Res.string.set_vaults),
                    stringResource(Res.string.set_vaults_sub, state.vaults.size.toString()), first = true) { open(SetPage.VAULTS) }
                SetRow(Icons.Filled.Download, Cyan, stringResource(Res.string.set_import_export), stringResource(Res.string.set_import_sub)) { open(SetPage.IMPORT) }
            }
            Group(stringResource(Res.string.set_g_servers)) {
                val sub = when {
                    servers.isEmpty() -> stringResource(Res.string.set_servers_none)
                    else -> stringResource(
                        Res.string.set_servers_sub,
                        servers.size.toString(),
                        stringResource(if (sync.serverId != null) Res.string.set_sync_on else Res.string.set_sync_off_short),
                        stringResource(if (servers.any { it.canShare }) Res.string.set_share_on else Res.string.set_share_off),
                    )
                }
                SetRow(Icons.Filled.Dns, Blue, stringResource(Res.string.set_servers_sync_share), sub, first = true) { open(SetPage.SERVERS) }
            }
            Group(stringResource(Res.string.set_g_areas)) {
                SetRow(Icons.Filled.Apps, Gray, stringResource(Res.string.set_modules), stringResource(Res.string.set_modules_sub), first = true) { open(SetPage.MODULES) }
                if (settings.moduleGhulPass) {
                    SetRow(Icons.Filled.Key, areaColor(Tab.PASS), stringResource(Res.string.tab_pass),
                        stringResource(if (backend.isDesktop) Res.string.set_pass_sub_desktop else Res.string.set_pass_sub)) { open(SetPage.PASS) }
                }
                if (settings.moduleGhulAuth) {
                    SetRow(Icons.Filled.VerifiedUser, areaColor(Tab.AUTH), stringResource(Res.string.tab_auth), stringResource(Res.string.set_auth_sub)) { open(SetPage.AUTH) }
                }
                if (settings.moduleGhulDrive) {
                    SetRow(Icons.Filled.Cloud, areaColor(Tab.DRIVE), stringResource(Res.string.tab_drive), stringResource(Res.string.set_drive_sub)) { open(SetPage.DRIVE) }
                }
                if (settings.moduleGhulMail) {
                    SetRow(Icons.Filled.Email, areaColor(Tab.MAIL), stringResource(Res.string.tab_mail), stringResource(Res.string.set_mail_sub)) { open(SetPage.MAIL) }
                }
            }
            Group(stringResource(Res.string.set_g_general)) {
                SetRow(Icons.Filled.Palette, Pink, stringResource(Res.string.set_view), stringResource(Res.string.set_view_sub), first = true) { open(SetPage.LOOK) }
                SetRow(Icons.Filled.Info, Gray, stringResource(Res.string.set_about), "GhulSuite ${backend.appVersion}") { open(SetPage.ABOUT) }
            }
        }
    }
}
