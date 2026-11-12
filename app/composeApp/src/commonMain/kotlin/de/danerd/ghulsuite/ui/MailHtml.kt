// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.ClickableText
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Image
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.MailBlockKindUi
import de.danerd.ghulsuite.model.MailBlockUi
import de.danerd.ghulsuite.model.MailMessageUi
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.decodeToImageBitmap
import org.jetbrains.compose.resources.stringResource

/**
 * An HTML mail drawn from the safe building blocks of the core – never as HTML. Images inside the
 * mail are shown; images from the internet only after the user asks (once, or always for this sender).
 * Links show their target before they open.
 */
@Composable
fun MailHtmlView(state: AppState, m: MailMessageUi) {
    val t = g
    val settings = state.backend.settings
    val net = state.backend.net
    val sender = m.fromAddress.lowercase()
    var loadRemote by remember(m.uid) { mutableStateOf(sender.isNotEmpty() && sender in settings.mailImageSenders.lines()) }
    val remote = remember(m.uid) { mutableStateMapOf<String, ImageBitmap?>() }
    var linkToOpen by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(loadRemote, m.uid) {
        if (!loadRemote) return@LaunchedEffect
        for (b in m.blocks) {
            val src = b.imageSrc
            if (b.kind != MailBlockKindUi.IMAGE || src.startsWith("cid:", ignoreCase = true) || src in remote) continue
            remote[src] = runCatching { net.mailFetchImage(src).decodeToImageBitmap() }.getOrNull()
        }
    }
    Column(Modifier.padding(top = 14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (m.remoteImages > 0 && !loadRemote) {
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(t.surface2).padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(stringResource(Res.string.mail_images_blocked, m.remoteImages.toString()), fontSize = 13.sp, color = t.muted)
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(stringResource(Res.string.mail_images_load), color = t.accent, fontWeight = FontWeight.SemiBold, fontSize = 14.sp,
                        modifier = Modifier.clickable { loadRemote = true })
                    if (sender.isNotEmpty()) {
                        Text(stringResource(Res.string.mail_images_always), color = t.accent, fontWeight = FontWeight.SemiBold, fontSize = 14.sp,
                            modifier = Modifier.clickable {
                                settings.mailImageSenders = (settings.mailImageSenders.lines() + sender).filter { it.isNotBlank() }.distinct().joinToString("\n")
                                loadRemote = true
                            })
                    }
                }
            }
        }
        for (b in m.blocks) HtmlBlock(b, m, remote, loadRemote) { linkToOpen = it }
    }
    linkToOpen?.let { link ->
        val uri = LocalUriHandler.current
        val target = link.removePrefix("mailto:").substringAfter("://").substringBefore('/').ifBlank { link }
        AlertDialog(
            onDismissRequest = { linkToOpen = null },
            title = { Text(stringResource(Res.string.mail_link_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(target, fontWeight = FontWeight.Bold, color = t.fg)
                    Text(link, fontSize = 13.sp, color = t.muted)
                }
            },
            confirmButton = { TextButton(onClick = { linkToOpen = null; runCatching { uri.openUri(link) } }) { Text(stringResource(Res.string.mail_link_open)) } },
            dismissButton = { TextButton(onClick = { linkToOpen = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

@Composable
private fun HtmlBlock(b: MailBlockUi, m: MailMessageUi, remote: Map<String, ImageBitmap?>, loadRemote: Boolean, onLink: (String) -> Unit) {
    val t = g
    when (b.kind) {
        MailBlockKindUi.RULE -> Box(Modifier.fillMaxWidth().padding(vertical = 4.dp).height(1.dp).background(t.line))
        MailBlockKindUi.IMAGE -> {
            val cid = b.imageSrc.takeIf { it.startsWith("cid:", ignoreCase = true) }?.substring(4)
            val bitmap = if (cid != null) {
                remember(m.uid, cid) { m.inlineImages[cid]?.let { runCatching { it.decodeToImageBitmap() }.getOrNull() } }
            } else {
                remote[b.imageSrc]
            }
            if (bitmap != null) {
                Image(bitmap, b.imageAlt, Modifier.fillMaxWidth().heightIn(max = 480.dp).clip(RoundedCornerShape(10.dp)), contentScale = ContentScale.FillWidth)
            } else if (cid == null && !loadRemote || b.imageAlt.isNotBlank()) {
                Row(
                    Modifier.clip(RoundedCornerShape(10.dp)).background(t.surface2).padding(horizontal = 10.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    androidx.compose.material3.Icon(Icons.Filled.Image, null, Modifier.width(16.dp), tint = t.faint)
                    Text(b.imageAlt.ifBlank { stringResource(Res.string.mail_image_placeholder) }, fontSize = 12.sp, color = t.faint)
                }
            }
        }
        else -> {
            val quote = b.kind == MailBlockKindUi.QUOTE
            val text = annotated(b, t.accent)
            val style = when (b.kind) {
                MailBlockKindUi.HEADING -> TextStyle(fontSize = (22 - 2 * (b.level.coerceIn(1, 4) - 1)).sp, fontWeight = FontWeight.Bold, color = t.fg, lineHeight = 28.sp)
                else -> TextStyle(fontSize = 15.sp, lineHeight = 22.sp, color = if (quote) t.muted else t.fg)
            }
            Row(Modifier.padding(start = if (b.kind == MailBlockKindUi.LIST_ITEM) ((b.level - 1).coerceAtLeast(0) * 16).dp else 0.dp)) {
                if (quote) Box(Modifier.width(3.dp).height(22.dp).background(t.line))
                if (b.kind == MailBlockKindUi.LIST_ITEM) Text("•  ", style = style)
                ClickableText(text, Modifier.padding(start = if (quote) 10.dp else 0.dp), style = style) { offset ->
                    text.getStringAnnotations("link", offset, offset).firstOrNull()?.let { onLink(it.item) }
                }
            }
        }
    }
}

private fun annotated(b: MailBlockUi, linkColor: androidx.compose.ui.graphics.Color): AnnotatedString = buildAnnotatedString {
    for (s in b.spans) {
        val style = SpanStyle(
            fontWeight = if (s.bold) FontWeight.Bold else null,
            fontStyle = if (s.italic) FontStyle.Italic else null,
            color = if (s.link.isNotEmpty()) linkColor else androidx.compose.ui.graphics.Color.Unspecified,
            textDecoration = if (s.link.isNotEmpty()) TextDecoration.Underline else null,
        )
        if (s.link.isNotEmpty()) pushStringAnnotation("link", s.link)
        withStyle(style) { append(s.text) }
        if (s.link.isNotEmpty()) pop()
    }
}
