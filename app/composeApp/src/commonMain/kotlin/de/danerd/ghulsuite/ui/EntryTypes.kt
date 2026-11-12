// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import de.danerd.ghulsuite.model.FieldType
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource

/** How a field is edited. */
enum class EditKind { TEXT, SECRET, MULTILINE, LIST }

data class FieldDef(val name: String, val label: StringResource, val edit: EditKind) {
    val storageType: FieldType
        get() = when (edit) {
            EditKind.TEXT, EditKind.MULTILINE -> FieldType.TEXT
            EditKind.SECRET -> FieldType.SECRET
            EditKind.LIST -> FieldType.LIST
        }
}

data class EntryTypeDef(val type: String, val label: StringResource, val fields: List<FieldDef>)

private val title = FieldDef("title", Res.string.f_title, EditKind.TEXT)
private val notes = FieldDef("notes", Res.string.f_notes, EditKind.MULTILINE)

/** Entry types of GhulPass, see docs/FORMAT.md, section 5.4. */
val ENTRY_TYPES: List<EntryTypeDef> = listOf(
    EntryTypeDef(
        "login", Res.string.t_login,
        listOf(
            title,
            FieldDef("username", Res.string.f_username, EditKind.TEXT),
            FieldDef("password", Res.string.f_password, EditKind.SECRET),
            FieldDef("urls", Res.string.f_urls, EditKind.LIST),
            FieldDef("totp", Res.string.f_totp, EditKind.SECRET),
            notes,
        ),
    ),
    EntryTypeDef("note", Res.string.t_note, listOf(title, notes)),
    EntryTypeDef(
        "card", Res.string.t_card,
        listOf(
            title,
            FieldDef("holder", Res.string.f_holder, EditKind.TEXT),
            FieldDef("number", Res.string.f_number, EditKind.SECRET),
            FieldDef("expiry", Res.string.f_expiry, EditKind.TEXT),
            FieldDef("cvc", Res.string.f_cvc, EditKind.SECRET),
            FieldDef("pin", Res.string.f_pin, EditKind.SECRET),
            notes,
        ),
    ),
    EntryTypeDef(
        "wifi", Res.string.t_wifi,
        listOf(
            title,
            FieldDef("ssid", Res.string.f_ssid, EditKind.TEXT),
            FieldDef("password", Res.string.f_password, EditKind.SECRET),
            FieldDef("security", Res.string.f_security, EditKind.TEXT),
            notes,
        ),
    ),
    EntryTypeDef(
        "identity", Res.string.t_identity,
        listOf(
            title,
            FieldDef("name", Res.string.f_name, EditKind.TEXT),
            FieldDef("email", Res.string.f_email, EditKind.TEXT),
            FieldDef("phone", Res.string.f_phone, EditKind.TEXT),
            FieldDef("address", Res.string.f_address, EditKind.MULTILINE),
            FieldDef("birthday", Res.string.f_birthday, EditKind.TEXT),
            notes,
        ),
    ),
    EntryTypeDef(
        "ssh", Res.string.t_ssh,
        listOf(
            title,
            FieldDef("public_key", Res.string.f_public_key, EditKind.MULTILINE),
            FieldDef("private_key", Res.string.f_private_key, EditKind.SECRET),
            FieldDef("passphrase", Res.string.f_passphrase, EditKind.SECRET),
            notes,
        ),
    ),
)

fun entryType(type: String): EntryTypeDef =
    ENTRY_TYPES.firstOrNull { it.type == type } ?: EntryTypeDef(type, Res.string.t_other, listOf(title, notes))
