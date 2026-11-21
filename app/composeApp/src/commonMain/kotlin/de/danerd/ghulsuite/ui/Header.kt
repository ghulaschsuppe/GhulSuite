// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.Tune
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.sp
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.rememberResourceEnvironment
import org.jetbrains.compose.resources.stringResource

/** The line under the area title: what is shown right now, in the area's colour; tap to switch. */
@Composable
fun AreaContext(state: AppState) {
    var label by remember { mutableStateOf("") }
    val all = stringResource(Res.string.vault_all)
    val allAccounts = stringResource(Res.string.ctx_all_codes)
    val suite = stringResource(Res.string.set_my_suite)
    val versionWord = stringResource(Res.string.ctx_version)
    val allVaults = stringResource(Res.string.vault_all_vaults)
    val allCalendars = stringResource(Res.string.cal_all_calendars)
    // Texts looked up later (after a count is known) use the language of the screen.
    val env = rememberResourceEnvironment()
    LaunchedEffect(state.tab, state.revision, state.passVault, state.authVault, state.docsVault, state.calendars, state.backend.settings.calHidden, state.hidden) {
        label = when (state.tab) {
            Tab.PASS -> state.vaults.firstOrNull { it.id == state.passVault }?.name ?: allVaults
            Tab.AUTH -> {
                // Accounts deleted with "Undo" still open are not counted (the list hides them too).
                val n = catching { state.backend.totpCodes(nowSeconds()) }.getOrDefault(emptyList())
                    .count { (state.authVault == null || it.vaultId == state.authVault) && hiddenKey("auth", it.vaultId, it.id) !in state.hidden }
                (state.vaults.firstOrNull { it.id == state.authVault }?.name ?: allAccounts) + " · " + countText(env, Res.plurals.auth_count, n, n.toString())
            }
            Tab.DOCS -> state.vaults.firstOrNull { it.id == state.docsVault }?.name
                ?: state.vaultsOf(VaultKindUi.DOCS).filter { it.available }.singleOrNull()?.name ?: allVaults
            Tab.CAL -> {
                val cals = state.calendars
                val hidden = state.backend.settings.calHiddenSet()
                val visible = cals.count { it.id !in hidden }
                if (visible == cals.size) allCalendars else countText(env, Res.plurals.cal_some_calendars, cals.size, visible.toString(), cals.size.toString())
            }
            Tab.SETTINGS -> "$suite · " + versionWord.replace("%1\$s", state.backend.appVersion.substringBefore("-").split('.').take(2).joinToString("."))
        }
    }
    val clickable = state.tab != Tab.SETTINGS
    Row(
        Modifier.then(if (clickable) Modifier.clickable { state.contextSheet = true } else Modifier),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
            color = areaColor(state.tab),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (clickable) Icon(Icons.Filled.KeyboardArrowDown, null, Modifier.padding(start = 2.dp).size(16.dp), tint = areaColor(state.tab))
    }
}

@Composable
private fun CtxRow(title: String, subtitle: String? = null, selected: Boolean = false, divider: Boolean = false, leading: @Composable () -> Unit, onClick: () -> Unit) {
    SheetRow(
        title, subtitle, leading = leading, divider = divider,
        end = { if (selected) SheetCheck() },
        onClick = onClick,
    )
}

/** Bottom sheet behind the context line: vault filter or calendars, the same pattern everywhere. */
@Composable
fun AreaContextSheet(state: AppState) {
    val close = { state.contextSheet = false }
    when (state.tab) {
        Tab.PASS, Tab.AUTH -> VaultSheet(state, if (state.tab == Tab.PASS) VaultKindUi.PASS else VaultKindUi.TOTP, close)
        Tab.DOCS -> VaultSheet(state, VaultKindUi.DOCS, close)
        Tab.CAL -> CalendarsSheet(state, close)
        Tab.SETTINGS -> close()
    }
}

@Composable
private fun VaultSheet(state: AppState, kind: VaultKindUi, close: () -> Unit) {
    val selected = when (kind) {
        VaultKindUi.PASS -> state.passVault
        VaultKindUi.DOCS -> state.docsVault
        else -> state.authVault
    }
    val pick = { id: String? ->
        when (kind) {
            VaultKindUi.PASS -> state.passVault = id
            VaultKindUi.DOCS -> state.docsVault = id
            else -> state.authVault = id
        }
        close()
    }
    GhulSheet(close, stringResource(Res.string.ctx_choose_vault)) {
        SheetList {
            CtxRow(stringResource(Res.string.vault_all_vaults), selected = selected == null, leading = { IconTile(Icons.Filled.Layers, paletteColor(0)) }) { pick(null) }
            for (v in state.vaultsOf(kind)) {
                CtxRow(v.name, pluralStringResource(Res.plurals.ctx_entries, v.entries, v.entries.toString()), selected = selected == v.id, divider = true, leading = { Avatar(v.name) }) { pick(v.id) }
            }
            // A vault open on its own: no vaults of a suite to manage.
            if (state.single == null) {
                CtxRow(stringResource(Res.string.set_vaults), divider = true, leading = { IconTile(Icons.Filled.Tune, paletteColor(5)) }) {
                    close(); state.push(Screen.SettingsPage(SetPage.VAULTS))
                }
            }
        }
    }
}
