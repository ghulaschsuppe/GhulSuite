// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.draw.shadow
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.a_back
import de.danerd.ghulsuite.resources.a_lock_now
import de.danerd.ghulsuite.resources.s_busy
import de.danerd.ghulsuite.resources.s_touch_key
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// --- Tokens of the design preview (public/design/design.css) ---------------------------------

/** The colour tokens of the design preview, light and dark. */
class GhulTokens(
    val bg: Color,
    val surface: Color,
    val surface2: Color,
    val surface3: Color,
    val line: Color,
    val fg: Color,
    val muted: Color,
    val faint: Color,
    val accent: Color,
    val accentSoft: Color,
    val onAccent: Color,
    val ok: Color,
    val warn: Color,
    val danger: Color,
)

val LightTokens = GhulTokens(
    bg = Color(0xFFF4F4F8), surface = Color.White, surface2 = Color(0xFFF0F0F6), surface3 = Color(0xFFE7E7F0),
    line = Color(0xFFE3E3EC), fg = Color(0xFF15161C), muted = Color(0xFF5F6173), faint = Color(0xFF8A8CA0),
    accent = Color(0xFF6A4DF5), accentSoft = Color(0xFFE9E4FF), onAccent = Color.White,
    ok = Color(0xFF14874F), warn = Color(0xFFB36200), danger = Color(0xFFD03B4C),
)

val DarkTokens = GhulTokens(
    bg = Color(0xFF0F1015), surface = Color(0xFF181920), surface2 = Color(0xFF1F2029), surface3 = Color(0xFF2A2B36),
    line = Color(0xFF2B2D3B), fg = Color(0xFFE8E8F0), muted = Color(0xFFA2A4B8), faint = Color(0xFF6F7187),
    accent = Color(0xFFA996FF), accentSoft = Color(0xFF2E2560), onAccent = Color(0xFF160D40),
    ok = Color(0xFF4FD8A0), warn = Color(0xFFFFB85C), danger = Color(0xFFFF7A86),
)

/** The accent on a bar in the text colour (snackbar): the light accent on dark, the dark one on light. */
val GhulTokens.accentOnFg: Color
    get() = if (fg.luminance() < 0.5f) DarkTokens.accent else LightTokens.accent

val LocalTokens = staticCompositionLocalOf { LightTokens }

/** Current design tokens. */
val g: GhulTokens @Composable get() = LocalTokens.current

/** True inside a card or sheet: fields then use the second surface colour so they stay visible. */
val LocalOnCard = compositionLocalOf { false }

/** How a [TextButton] looks: plain text, or as a button of a sheet's button row. */
enum class ButtonLook { TEXT, OUTLINE, PRIMARY, DANGER }

val LocalButtonLook = compositionLocalOf { ButtonLook.TEXT }

private val ButtonText @Composable get() = TextStyle(fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
private val ButtonPadding = PaddingValues(horizontal = 20.dp)

// --- Buttons (.btn: 46 dp high, pill, semibold) -------------------------------------------------

@Composable
fun Button(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: ButtonColors? = null,
    contentPadding: PaddingValues = ButtonPadding,
    content: @Composable RowScope.() -> Unit,
) {
    androidx.compose.material3.Button(
        onClick, modifier.heightIn(min = 46.dp), enabled = enabled, shape = CircleShape,
        colors = colors ?: ButtonDefaults.buttonColors(
            containerColor = g.accent, contentColor = g.onAccent,
            disabledContainerColor = g.surface3, disabledContentColor = g.faint,
        ),
        contentPadding = contentPadding,
    ) { ProvideTextStyle(ButtonText) { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), content = content) } }
}

@Composable
fun FilledTonalButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: ButtonColors? = null,
    contentPadding: PaddingValues = ButtonPadding,
    content: @Composable RowScope.() -> Unit,
) {
    androidx.compose.material3.Button(
        onClick, modifier.heightIn(min = 46.dp), enabled = enabled, shape = CircleShape,
        colors = colors ?: ButtonDefaults.buttonColors(
            containerColor = g.accentSoft, contentColor = g.accent,
            disabledContainerColor = g.surface3, disabledContentColor = g.faint,
        ),
        contentPadding = contentPadding,
    ) { ProvideTextStyle(ButtonText) { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), content = content) } }
}

@Composable
fun OutlinedButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: ButtonColors? = null,
    contentPadding: PaddingValues = ButtonPadding,
    content: @Composable RowScope.() -> Unit,
) {
    androidx.compose.material3.OutlinedButton(
        onClick, modifier.heightIn(min = 46.dp), enabled = enabled, shape = CircleShape,
        colors = colors ?: ButtonDefaults.outlinedButtonColors(
            containerColor = g.surface, contentColor = g.fg, disabledContainerColor = g.surface, disabledContentColor = g.faint,
        ),
        border = BorderStroke(1.dp, g.line),
        contentPadding = contentPadding,
    ) { ProvideTextStyle(ButtonText) { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), content = content) } }
}

/** A text action; inside a sheet's button row it becomes an outline or primary button. */
@Composable
fun TextButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: ButtonColors? = null,
    content: @Composable RowScope.() -> Unit,
) {
    when (LocalButtonLook.current) {
        ButtonLook.PRIMARY -> CompositionLocalProvider(LocalButtonLook provides ButtonLook.TEXT) {
            Button(onClick, modifier.fillMaxWidth(), enabled, content = content)
        }
        ButtonLook.DANGER -> CompositionLocalProvider(LocalButtonLook provides ButtonLook.TEXT) {
            Button(
                onClick, modifier.fillMaxWidth(), enabled,
                colors = ButtonDefaults.buttonColors(
                    containerColor = g.danger, contentColor = if (LocalDark.current) g.bg else Color.White,
                    disabledContainerColor = g.surface3, disabledContentColor = g.faint,
                ),
                content = content,
            )
        }
        ButtonLook.OUTLINE -> CompositionLocalProvider(LocalButtonLook provides ButtonLook.TEXT) {
            OutlinedButton(onClick, modifier.fillMaxWidth(), enabled, content = content)
        }
        ButtonLook.TEXT -> androidx.compose.material3.TextButton(
            onClick, modifier, enabled = enabled,
            colors = colors ?: ButtonDefaults.textButtonColors(contentColor = g.accent, disabledContentColor = g.faint),
        ) { ProvideTextStyle(ButtonText) { content() } }
    }
}

// --- Switch (.toggle) ------------------------------------------------------------------------------

@Composable
fun Switch(checked: Boolean, onCheckedChange: ((Boolean) -> Unit)?, modifier: Modifier = Modifier, enabled: Boolean = true) {
    androidx.compose.material3.Switch(
        checked, onCheckedChange, modifier, enabled = enabled,
        colors = SwitchDefaults.colors(
            checkedTrackColor = g.accent, checkedThumbColor = g.onAccent, checkedBorderColor = Color.Transparent,
            uncheckedTrackColor = g.surface3, uncheckedThumbColor = g.faint, uncheckedBorderColor = Color.Transparent,
        ),
    )
}

// --- Fields (.field: card with the label inside, no outline) ----------------------------------

@Composable
fun OutlinedTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    textStyle: TextStyle? = null,
    label: @Composable (() -> Unit)? = null,
    placeholder: @Composable (() -> Unit)? = null,
    leadingIcon: @Composable (() -> Unit)? = null,
    trailingIcon: @Composable (() -> Unit)? = null,
    supportingText: @Composable (() -> Unit)? = null,
    isError: Boolean = false,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    singleLine: Boolean = false,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    minLines: Int = 1,
) {
    val t = g
    val base = (textStyle ?: MaterialTheme.typography.bodyLarge).copy(color = t.fg, fontSize = 16.sp)
    // A tap anywhere on the card (label included) puts the cursor into the field.
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    Column(modifier.alpha(if (enabled) 1f else 0.6f)) {
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp))
                .background(if (LocalOnCard.current) t.surface2 else t.surface)
                .clickable(
                    enabled = enabled && !readOnly,
                    interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() },
                    indication = null,
                ) { catching { focus.requestFocus() } }
                .then(if (isError) Modifier.background(t.danger.copy(alpha = 0.06f)) else Modifier)
                .padding(start = 16.dp, end = if (trailingIcon != null) 4.dp else 16.dp, top = 12.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (leadingIcon != null) {
                CompositionLocalProvider(androidx.compose.material3.LocalContentColor provides t.muted) { leadingIcon() }
            }
            Column(Modifier.weight(1f)) {
                if (label != null) {
                    ProvideTextStyle(TextStyle(fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = if (isError) t.danger else t.accent)) {
                        Box(Modifier.padding(bottom = 2.dp)) { label() }
                    }
                }
                BasicTextField(
                    value, onValueChange, Modifier.fillMaxWidth().focusRequester(focus), enabled = enabled, readOnly = readOnly, textStyle = base,
                    keyboardOptions = keyboardOptions, keyboardActions = keyboardActions, singleLine = singleLine,
                    maxLines = maxLines, minLines = minLines, visualTransformation = visualTransformation,
                    cursorBrush = SolidColor(t.accent),
                    decorationBox = { inner ->
                        Box {
                            if (value.isEmpty() && placeholder != null) {
                                ProvideTextStyle(base.copy(color = t.faint)) { placeholder() }
                            }
                            inner()
                        }
                    },
                )
            }
            if (trailingIcon != null) {
                CompositionLocalProvider(androidx.compose.material3.LocalContentColor provides t.muted) { trailingIcon() }
            }
        }
        if (supportingText != null) {
            ProvideTextStyle(TextStyle(fontSize = 13.sp, color = if (isError) t.danger else t.muted)) {
                Box(Modifier.padding(start = 4.dp, top = 4.dp, end = 4.dp)) { supportingText() }
            }
        }
    }
}

// --- Busy: in front of everything -----------------------------------------------------------------

/** Something is running (state.busy), and whether the security key waits for a touch. */
class BusyState(val busy: Boolean, val touch: Boolean)

val LocalBusy = androidx.compose.runtime.compositionLocalOf { BusyState(false, false) }

/**
 * While an action runs: nothing behind can be tapped twice, and after a moment a card in the
 * middle says that something happens (or that the key waits for a touch). Drawn on top of the
 * screens and inside every sheet and dialog (they are windows of their own).
 */
@Composable
fun BusyOverlay(modifier: Modifier = Modifier) {
    val b = LocalBusy.current
    if (!b.busy) return
    var show by remember { mutableStateOf(false) }
    androidx.compose.runtime.LaunchedEffect(Unit) {
        kotlinx.coroutines.delay(250)
        show = true
    }
    val t = g
    Box(
        modifier.fillMaxSize()
            // Takes every touch: nothing behind starts a second time.
            .pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } } }
            .then(if (show || b.touch) Modifier.background(Color.Black.copy(alpha = 0.45f)) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        if (show || b.touch) {
            Row(
                Modifier.padding(24.dp).shadow(16.dp, RoundedCornerShape(20.dp)).clip(RoundedCornerShape(20.dp))
                    .background(if (b.touch) t.accentSoft else t.surface2).padding(horizontal = 22.dp, vertical = 18.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                androidx.compose.material3.CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 3.dp, color = t.accent)
                Text(
                    org.jetbrains.compose.resources.stringResource(if (b.touch) Res.string.s_touch_key else Res.string.s_busy),
                    color = t.fg, fontWeight = FontWeight.SemiBold, fontSize = 16.sp,
                )
            }
        }
    }
}

// --- Sheets (.sheet: from the bottom, handle, 28 dp corners) -------------------------------------

/** Bottom sheet in the look of the design preview: handle, title, content. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GhulSheet(onDismissRequest: () -> Unit, title: String? = null, content: @Composable ColumnScope.() -> Unit) {
    if (LocalWide.current) {
        // Wide windows (Windows, tablets): the same sheet as a dialog in the middle.
        val t = g
        androidx.compose.ui.window.Dialog(
            onDismissRequest = onDismissRequest,
            properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false),
        ) {
            Box(
                Modifier.padding(24.dp).widthIn(max = 520.dp).fillMaxWidth().heightIn(max = 760.dp)
                    .shadow(24.dp, RoundedCornerShape(28.dp)).clip(RoundedCornerShape(28.dp)).background(t.surface),
            ) {
                CompositionLocalProvider(LocalOnCard provides true) {
                    Column(
                        Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(start = 24.dp, end = 24.dp, top = 24.dp, bottom = 24.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        if (title != null) SheetTitle(title)
                        content()
                    }
                }
                BusyOverlay(Modifier.matchParentSize())
            }
        }
        return
    }
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val t = g
    ModalBottomSheet(
        onDismissRequest = onDismissRequest,
        sheetState = state,
        containerColor = t.surface,
        shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
        dragHandle = {
            Box(Modifier.padding(top = 10.dp, bottom = 14.dp).size(36.dp, 4.dp).clip(RoundedCornerShape(2.dp)).background(t.line))
        },
    ) {
        Box {
            CompositionLocalProvider(LocalOnCard provides true) {
                Column(
                    Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, bottom = 28.dp).navigationBarsPadding(),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    if (title != null) SheetTitle(title)
                    content()
                }
            }
            BusyOverlay(Modifier.matchParentSize())
        }
    }
}

@Composable
fun SheetTitle(text: String) {
    Text(text, fontSize = 20.sp, fontWeight = FontWeight.Bold, color = g.fg, modifier = Modifier.padding(bottom = 2.dp))
}

/** A list inside a sheet (.card.flush on the second surface). */
@Composable
fun SheetList(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(g.surface2).padding(vertical = 4.dp), content = content)
}

/**
 * Dialogs open as bottom sheets, like everywhere in the design preview: title, content, then the
 * actions as a row of buttons (cancel outlined, confirm filled). With [danger] the confirm button
 * is red: for everything that deletes for good.
 */
@Composable
fun AlertDialog(
    onDismissRequest: () -> Unit,
    confirmButton: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    dismissButton: @Composable (() -> Unit)? = null,
    icon: @Composable (() -> Unit)? = null,
    title: @Composable (() -> Unit)? = null,
    text: @Composable (() -> Unit)? = null,
    danger: Boolean = false,
) {
    GhulSheet(onDismissRequest) {
        if (icon != null) Box(Modifier.padding(bottom = 4.dp)) { icon() }
        if (title != null) {
            ProvideTextStyle(TextStyle(fontSize = 20.sp, fontWeight = FontWeight.Bold)) { title() }
        }
        if (text != null) {
            CompositionLocalProvider(androidx.compose.material3.LocalContentColor provides g.muted) {
                ProvideTextStyle(MaterialTheme.typography.bodyLarge) { Box(modifier) { text() } }
            }
        }
        Spacer(Modifier.height(6.dp))
        // Buttons share the row; an empty slot takes no room.
        Layout(
            contents = listOf(
                { CompositionLocalProvider(LocalButtonLook provides ButtonLook.OUTLINE) { dismissButton?.invoke() } },
                { CompositionLocalProvider(LocalButtonLook provides if (danger) ButtonLook.DANGER else ButtonLook.PRIMARY) { confirmButton() } },
            ),
            modifier = Modifier.fillMaxWidth(),
        ) { (dismiss, confirm), constraints ->
            val used = (dismiss + confirm).filter { it.maxIntrinsicHeight(constraints.maxWidth) > 0 }
            val gap = 8.dp.roundToPx()
            val n = used.size.coerceAtLeast(1)
            val w = (constraints.maxWidth - gap * (n - 1)) / n
            val placeables = used.map { it.measure(constraints.copy(minWidth = w, maxWidth = w, minHeight = 0)) }
            val h = placeables.maxOfOrNull { it.height } ?: 0
            layout(constraints.maxWidth, h) {
                var x = 0
                for (p in placeables) {
                    p.place(x, 0)
                    x += w + gap
                }
            }
        }
    }
}

// --- Rows used in sheets and cards ---------------------------------------------------------------

/** Row of a sheet list: optional icon tile, title, optional subtitle, optional end content. */
@Composable
fun SheetRow(
    title: String,
    subtitle: String? = null,
    leading: @Composable (() -> Unit)? = null,
    end: @Composable (() -> Unit)? = null,
    color: Color = Color.Unspecified,
    divider: Boolean = false,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
) {
    val t = g
    if (divider) {
        Box(Modifier.fillMaxWidth().padding(start = if (leading != null) 70.dp else 16.dp, end = 16.dp).height(1.dp).background(t.line))
    }
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp)
            .then(if (onClick != null && enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .alpha(if (enabled) 1f else 0.55f)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        leading?.invoke()
        Column(Modifier.weight(1f)) {
            Text(title, fontWeight = FontWeight.W500, fontSize = 15.sp, color = if (color != Color.Unspecified) color else t.fg)
            if (!subtitle.isNullOrBlank()) Text(subtitle, fontSize = 14.sp, color = t.muted)
        }
        end?.invoke()
    }
}

/** Small rounded label (.tag), optionally coloured (ok, warn, danger). */
@Composable
fun Tag(text: String, color: Color? = null, icon: androidx.compose.ui.graphics.vector.ImageVector? = null) {
    val t = g
    val fg = color ?: t.muted
    Row(
        Modifier.height(24.dp).clip(RoundedCornerShape(8.dp)).background(color?.copy(alpha = 0.14f) ?: t.surface2).padding(horizontal = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (icon != null) androidx.compose.material3.Icon(icon, null, Modifier.size(14.dp), tint = fg)
        Text(text, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = fg, maxLines = 1)
    }
}

/** Info line on the second surface (.banner). */
@Composable
fun Banner(icon: androidx.compose.ui.graphics.vector.ImageVector, text: String, end: @Composable (() -> Unit)? = null) {
    val t = g
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(t.surface2).padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        androidx.compose.material3.Icon(icon, null, Modifier.size(18.dp), tint = t.muted)
        Text(text, fontSize = 13.sp, color = t.muted, modifier = Modifier.weight(1f))
        end?.invoke()
    }
}

/**
 * Row of buttons that share the width (.btn-row). If a label would have to break inside a word
 * (a narrow phone, the large system font, a long German word), the buttons stand one below the
 * other at full width instead (bug F57: "Abbreche n", "Hinzufüg en").
 * Buttons with Modifier.weight share the rest of the width equally, others keep their own width.
 */
@Composable
fun ButtonRow(content: @Composable RowScope.() -> Unit) {
    val gap = 8.dp
    Layout(content = { ButtonRowScope.content() }, modifier = Modifier.fillMaxWidth()) { items, constraints ->
        val space = gap.roundToPx()
        val weighted = items.filter { it.layoutId == WEIGHTED }
        val fixed = items.filter { it.layoutId != WEIGHTED }
        val fixedWidths = fixed.map { it.maxIntrinsicWidth(constraints.maxHeight) }
        // Without a width limit (inside horizontal scrolling) every button takes its own width.
        val width = if (constraints.hasBoundedWidth) constraints.maxWidth
        else items.sumOf { it.maxIntrinsicWidth(constraints.maxHeight) } + space * (items.size - 1).coerceAtLeast(0)
        val share = if (weighted.isEmpty()) 0 else (width - fixedWidths.sum() - space * (items.size - 1)) / weighted.size
        val fits = fixedWidths.sum() + space * (items.size - 1) <= width &&
            weighted.all { it.minIntrinsicWidth(constraints.maxHeight) <= share }
        if (fits) {
            val placeables = items.map { m ->
                val w = if (m.layoutId == WEIGHTED) share else fixedWidths[fixed.indexOf(m)]
                m.measure(androidx.compose.ui.unit.Constraints.fixedWidth(w.coerceAtLeast(0)))
            }
            val h = placeables.maxOfOrNull { it.height } ?: 0
            layout(width, h) {
                var x = 0
                for (p in placeables) { p.place(x, (h - p.height) / 2); x += p.width + space }
            }
        } else {
            val placeables = items.map { it.measure(androidx.compose.ui.unit.Constraints.fixedWidth(width)) }
            val h = placeables.sumOf { it.height } + space * (placeables.size - 1).coerceAtLeast(0)
            layout(width, h) {
                var y = 0
                for (p in placeables) { p.place(0, y); y += p.height + space }
            }
        }
    }
}

private const val WEIGHTED = "ghul-button-row-weighted"

/** The scope of [ButtonRow]: weight marks a button that shares the free width. */
private object ButtonRowScope : RowScope {
    override fun Modifier.weight(weight: Float, fill: Boolean): Modifier = this.layoutId(WEIGHTED)
    override fun Modifier.align(alignment: Alignment.Vertical): Modifier = this
    override fun Modifier.alignBy(alignmentLine: androidx.compose.ui.layout.HorizontalAlignmentLine): Modifier = this
    override fun Modifier.alignByBaseline(): Modifier = this
    override fun Modifier.alignBy(alignmentLineBlock: (androidx.compose.ui.layout.Measured) -> Int): Modifier = this
}

/** Small hint text below a section (.hint-text). */
@Composable
fun HintText(text: String, modifier: Modifier = Modifier) {
    // Inside a card flush with the labels above and below it (it stood 4 dp further in, F58);
    // outside of cards slightly in, like the group titles.
    val inset = if (LocalOnCard.current) 0.dp else 4.dp
    Text(text, fontSize = 13.sp, lineHeight = 18.sp, color = g.muted, modifier = modifier.padding(horizontal = inset))
}

/** Small filled button of the header (.btn.primary, 36 dp): the main action of a full-screen editor. */
@Composable
fun HeaderButton(
    text: String,
    enabled: Boolean = true,
    icon: androidx.compose.ui.graphics.vector.ImageVector? = null,
    // The colour of the area (GhulMail orange, GhulCal pink …), as its "+" button has it.
    color: Color? = null,
    onClick: () -> Unit,
) {
    Row(
        Modifier.height(36.dp).clip(RoundedCornerShape(18.dp)).background(if (enabled) color ?: g.accent else g.surface3)
            .clickable(enabled = enabled) { HeaderGuard.noteAction(); onClick() }.padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val fg = if (enabled) g.onAccent else g.faint
        if (icon != null) androidx.compose.material3.Icon(icon, null, Modifier.size(18.dp), tint = fg)
        Text(text, color = fg, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1)
    }
}

/** Arrow at the end of a row that leads somewhere (18 dp, faint). */
@Composable
fun ChevronEnd() {
    androidx.compose.material3.Icon(androidx.compose.material.icons.Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = g.faint)
}

/** Check mark of the chosen option in a list or sheet (20 dp, accent). */
@Composable
fun SheetCheck() {
    androidx.compose.material3.Icon(androidx.compose.material.icons.Icons.Filled.Check, null, Modifier.size(20.dp), tint = g.accent)
}

@Suppress("unused")
private val keep = Modifier.width(0.dp)

// --- Top bar (.topbar) and bottom navigation (.nav) ---------------------------------------------

/** Icon button of the top bar: 40 dp, rounded square. */
@Composable
fun BarIcon(icon: androidx.compose.ui.graphics.vector.ImageVector, description: String?, tint: Color = Color.Unspecified, onClick: () -> Unit) {
    Box(
        Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.material3.Icon(icon, description, Modifier.size(22.dp), tint = if (tint == Color.Unspecified) g.fg else tint)
    }
}

/**
 * Header of every screen: back (on sub-pages), title (22 sp bold, 18 sp on sub-pages), the context
 * line in the area colour below it, own actions, and the lock on the right.
 */
@Composable
fun GhulTopBar(
    title: String,
    onLock: () -> Unit,
    onBack: (() -> Unit)? = null,
    backIcon: androidx.compose.ui.graphics.vector.ImageVector = androidx.compose.material.icons.Icons.AutoMirrored.Filled.ArrowBack,
    context: @Composable (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
    showLock: Boolean = !LocalWide.current,
) {
    Row(
        Modifier.fillMaxWidth().background(g.bg).androidx_statusBarsPadding().heightIn(min = 56.dp).padding(start = 12.dp, end = 12.dp, top = 6.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (onBack != null) BarIcon(backIcon, org.jetbrains.compose.resources.stringResource(Res.string.a_back), onClick = onBack)
        Column(Modifier.weight(1f).padding(start = if (onBack == null) 4.dp else 0.dp)) {
            if (title.isNotEmpty()) {
                // A long title gets a little smaller (down to 15 sp) before it is shortened with "…".
                val size = if (onBack != null) 18.sp else 22.sp
                ShrinkText(
                    title,
                    TextStyle(
                        color = g.fg, fontSize = size,
                        fontWeight = if (onBack != null) FontWeight.SemiBold else FontWeight.Bold,
                        letterSpacing = if (onBack != null) 0.sp else (-0.44).sp, lineHeight = if (onBack != null) 22.sp else 26.sp,
                    ),
                    15.sp,
                    Modifier.testTag(TITLE_TAG),
                    fallbackLines = 2,
                )
            }
            context?.invoke()
        }
        actions()
        if (showLock) {
            BarIcon(androidx.compose.material.icons.Icons.Outlined.Lock, org.jetbrains.compose.resources.stringResource(Res.string.a_lock_now)) {
                if (!HeaderGuard.lockBlocked()) onLock()
            }
        }
    }
}

@Composable
private fun Modifier.androidx_statusBarsPadding(): Modifier = this.statusBarsPadding()

/** One tab of the bottom navigation. */
/** [tag]: "tab-pass", "tab-auth" …: the emulator flows tap the bar by it (the area's name is also its title). */
class NavTab(val label: String, val icon: androidx.compose.ui.graphics.vector.ImageVector, val color: Color, val badge: Int = 0, val tag: String = "")

/** Bottom navigation: 80 dp, surface, line on top, pill indicator in the tab colour. */
@Composable
fun GhulNavBar(tabs: List<NavTab>, selected: Int, onSelect: (Int) -> Unit) {
    val t = g
    Column(Modifier.fillMaxWidth().background(t.surface)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(t.line))
        Row(Modifier.fillMaxWidth().navigationBarsPadding().height(79.dp).padding(start = 4.dp, end = 4.dp, top = 8.dp, bottom = 16.dp)) {
            tabs.forEachIndexed { i, tab ->
                val on = i == selected
                Column(
                    Modifier.weight(1f).testTag(tab.tag).clickable(
                        interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() },
                        indication = null,
                    ) { onSelect(i) },
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    val pill = if (tabs.size >= 6) 50.dp else 56.dp
                    // The badge sits on top of the pill, not inside it: the pill clips its content.
                    Box(Modifier.size(pill, 30.dp)) {
                        Box(
                            // Six areas: a little narrower, as in the design preview.
                            Modifier.fillMaxSize().clip(RoundedCornerShape(15.dp)).background(if (on) tab.color.copy(alpha = 0.16f) else Color.Transparent),
                            contentAlignment = Alignment.Center,
                        ) {
                            androidx.compose.material3.Icon(tab.icon, null, Modifier.size(22.dp), tint = if (on) tab.color else t.muted)
                        }
                        if (tab.badge > 0) {
                            Box(
                                Modifier.align(Alignment.TopStart).offset(x = pill / 2 + 4.dp, y = (-5).dp)
                                    .heightIn(min = 16.dp).widthIn(min = 16.dp)
                                    .border(1.5.dp, t.surface, RoundedCornerShape(8.dp))
                                    .clip(RoundedCornerShape(8.dp))
                                    .background(tab.color).padding(horizontal = 4.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    if (tab.badge > 99) "99+" else tab.badge.toString(),
                                    color = Color.White, fontSize = 10.sp, lineHeight = 12.sp, fontWeight = FontWeight.Bold, maxLines = 1, softWrap = false,
                                )
                            }
                        }
                    }
                    Text(tab.label, fontSize = if (tabs.size >= 6) 10.5.sp else 11.sp, fontWeight = FontWeight.SemiBold, color = if (on) tab.color else t.muted, maxLines = 1, softWrap = false)
                }
            }
        }
    }
}

/** True in wide windows (Windows, tablets in landscape): side navigation and two panes. */
val LocalWide = compositionLocalOf { false }

/** Columns for tile grids (Drive, Auth): 2 on phones, more in wide panes. */
val LocalColumns = compositionLocalOf { 2 }

/** Tag of the title of a screen: the layout check reports it when it is shortened with "…". */
const val TITLE_TAG = "ghul-title"

/**
 * The lock in the header ignores taps for a moment after an editor's "Save" or "Send": the editor
 * closes on the first tap and the screen behind it has its lock where "Save" was, so the second
 * tap of a double tap locked the app. Other taps on the lock (after "Back", on any screen) count.
 */
object HeaderGuard {
    /** How long after "Save" or "Send" the lock takes no tap (tests set it). */
    var grace: kotlin.time.Duration = kotlin.time.Duration.parse("500ms")
    private var lastAction: kotlin.time.TimeSource.Monotonic.ValueTimeMark? = null

    fun noteAction() {
        lastAction = kotlin.time.TimeSource.Monotonic.markNow()
    }

    fun lockBlocked(): Boolean = lastAction?.let { it.elapsedNow() < grace } ?: false
}

/** Tag of [GhulFab]: the layout check knows it floats over the list on purpose. */
const val FAB_TAG = "ghul-fab"

/** Floating action button (.fab): 56 dp, 18 dp corners, white content, optional text. */
@Composable
fun GhulFab(color: Color, icon: androidx.compose.ui.graphics.vector.ImageVector, text: String? = null, description: String? = null, onClick: () -> Unit) {
    androidx.compose.material3.Surface(
        onClick = onClick, shape = RoundedCornerShape(18.dp), color = color, contentColor = Color.White,
        shadowElevation = 6.dp, modifier = Modifier.heightIn(min = 56.dp).widthIn(min = 56.dp).testTag(FAB_TAG),
    ) {
        Row(
            Modifier.height(56.dp).padding(horizontal = if (text != null) 18.dp else 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        ) {
            androidx.compose.material3.Icon(icon, description ?: text, Modifier.size(24.dp))
            if (text != null) Text(text, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
        }
    }
}

// --- Segmented control (.seg) ----------------------------------------------------------------------

/** Scope of [SingleChoiceSegmentedButtonRow]: each [SegmentedButton] takes an equal share. */
class SegScope(private val row: RowScope) : RowScope by row {
    @Composable
    fun SegmentedButton(selected: Boolean, onClick: () -> Unit, shape: Any? = null, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
        val t = g
        androidx.compose.material3.Surface(
            onClick = onClick,
            shape = RoundedCornerShape(10.dp),
            color = if (selected) t.surface else Color.Transparent,
            shadowElevation = if (selected) 1.dp else 0.dp,
            // 40 dp: the smallest height that is still easy to hit with a finger.
            modifier = modifier.weight(1f).height(40.dp),
        ) {
            Box(contentAlignment = Alignment.Center) {
                ProvideTextStyle(TextStyle(fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = if (selected) t.fg else t.muted)) { content() }
            }
        }
    }
}

/** Messages (.toast): dark pill with light text, centred, like in the design preview. */
@Composable
fun GhulSnackbarHost(host: androidx.compose.material3.SnackbarHostState, modifier: Modifier = Modifier) {
    androidx.compose.material3.SnackbarHost(host, modifier) { data ->
        val t = g
        Box(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), contentAlignment = Alignment.Center) {
            Row(
                Modifier.widthIn(max = 560.dp).clip(RoundedCornerShape(14.dp)).background(t.fg)
                    .padding(start = 16.dp, end = if (data.visuals.actionLabel != null) 6.dp else 16.dp, top = 4.dp, bottom = 4.dp)
                    .heightIn(min = 36.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(data.visuals.message, Modifier.weight(1f, fill = false).padding(vertical = 6.dp), color = t.bg, fontSize = 14.sp, fontWeight = FontWeight.W500)
                // "Undo": the light accent reads on the dark bar (light design) and the dark accent
                // on the light bar (dark design).
                data.visuals.actionLabel?.let { label ->
                    Text(
                        label,
                        Modifier.clip(RoundedCornerShape(10.dp)).clickable { data.performAction() }.padding(horizontal = 10.dp, vertical = 8.dp),
                        color = t.accentOnFg, fontSize = 14.sp, fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
    }
}

/** Segmented choice in the look of the design preview: grey track, the chosen part raised. */
@Composable
fun SingleChoiceSegmentedButtonRow(modifier: Modifier = Modifier, content: @Composable SegScope.() -> Unit) {
    // All labels of one bar in the same size: the smallest one any of them needs.
    val sizes = androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateMapOf<String, Float>() }
    androidx.compose.runtime.CompositionLocalProvider(LocalSegSizes provides sizes) { SegRow(modifier, content) }
}

private val LocalSegSizes = compositionLocalOf<androidx.compose.runtime.snapshots.SnapshotStateMap<String, Float>?> { null }

@Composable
private fun SegRow(modifier: Modifier, content: @Composable SegScope.() -> Unit) {
    Row(
        modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(if (LocalOnCard.current) g.surface3 else g.surface2).padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) { SegScope(this).content() }
}

/**
 * One line of text that gets smaller (down to [minSize]) when it does not fit. If it does not fit
 * even then, it takes up to [fallbackLines] lines at that size (wrapped between words), and only
 * after that is it shortened with "…". Measures first, so it works for any font scale and width.
 * [sizes]: segments of one bar share it, so all of them use the same (smallest) size.
 */
@Composable
fun ShrinkText(
    text: String,
    style: TextStyle,
    minSize: androidx.compose.ui.unit.TextUnit,
    modifier: Modifier = Modifier,
    fallbackLines: Int = 1,
    sizes: androidx.compose.runtime.snapshots.SnapshotStateMap<String, Float>? = null,
) {
    val measurer = androidx.compose.ui.text.rememberTextMeasurer()
    androidx.compose.foundation.layout.BoxWithConstraints(modifier) {
        val max = constraints.maxWidth
        var size = style.fontSize.value
        var fits = true
        if (max < androidx.compose.ui.unit.Constraints.Infinity) {
            fun wide(v: Float) = measurer.measure(text, style.copy(fontSize = v.sp), maxLines = 1, softWrap = false).size.width > max
            while (size > minSize.value && wide(size)) size -= 0.5f
            fits = !wide(size)
        }
        if (sizes != null) {
            androidx.compose.runtime.SideEffect { sizes[text] = size }
            size = minOf(size, sizes.values.minOrNull() ?: size)
        }
        val lines = if (fits) 1 else fallbackLines
        Text(text, style = style.copy(fontSize = size.sp), maxLines = lines, softWrap = lines > 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
    }
}

/**
 * A setting: label on the left, its value on the right. When both do not fit side by side (a
 * long value, the large system font, a narrow phone), the value goes below the label instead of
 * squeezing it until a word breaks ("Zwischenablag e leeren", bug F57).
 */
@Composable
fun LabelValueRow(label: String, value: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val t = g
    Layout(
        content = {
            Text(label, fontSize = 15.sp)
            Text(value, fontSize = 13.sp, color = t.faint)
            androidx.compose.material3.Icon(androidx.compose.material.icons.Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.padding(start = 6.dp).size(18.dp), tint = t.faint)
        },
        modifier = modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(onClick = onClick),
    ) { (labelM, valueM, arrowM), c ->
        val width = c.maxWidth
        val gap = 8.dp.roundToPx()
        val arrow = arrowM.measure(androidx.compose.ui.unit.Constraints())
        val valueWidth = valueM.maxIntrinsicWidth(c.maxHeight)
        // Side by side only if the label then keeps every word whole.
        val besides = labelM.minIntrinsicWidth(c.maxHeight) + gap + valueWidth + arrow.width <= width
        if (besides) {
            val v = valueM.measure(androidx.compose.ui.unit.Constraints(maxWidth = valueWidth))
            val l = labelM.measure(androidx.compose.ui.unit.Constraints(maxWidth = (width - v.width - gap - arrow.width).coerceAtLeast(0)))
            val h = maxOf(c.minHeight, l.height, v.height, arrow.height)
            layout(width, h) {
                l.place(0, (h - l.height) / 2)
                v.place(width - arrow.width - v.width, (h - v.height) / 2)
                arrow.place(width - arrow.width, (h - arrow.height) / 2)
            }
        } else {
            // (coerceAtLeast: a pane that animates its width from 0 is narrower than the arrow)
            val l = labelM.measure(androidx.compose.ui.unit.Constraints(maxWidth = (width - arrow.width).coerceAtLeast(0)))
            val v = valueM.measure(androidx.compose.ui.unit.Constraints(maxWidth = (width - arrow.width).coerceAtLeast(0)))
            val pad = 6.dp.roundToPx()
            val h = maxOf(c.minHeight, pad + l.height + 2 + v.height + pad)
            layout(width, h) {
                l.place(0, pad)
                v.place(0, pad + l.height + 2)
                arrow.place(width - arrow.width, (h - arrow.height) / 2)
            }
        }
    }
}

/**
 * Chips side by side that continue in the next line when they do not fit (a narrow phone, the
 * large system font) instead of squeezing their labels until a word breaks ("Passwo rt").
 */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun ChipRow(content: @Composable androidx.compose.foundation.layout.FlowRowScope.() -> Unit) {
    androidx.compose.foundation.layout.FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
        content = content,
    )
}

/**
 * Label of a segment: one line. A long word (German, the large system font, a narrow phone)
 * makes the text a little smaller, down to 10 sp, instead of breaking it ("SHA25 6", bug F57).
 */
@Composable
fun SegLabel(text: String) {
    ShrinkText(text, androidx.compose.material3.LocalTextStyle.current, 10.sp, Modifier.padding(horizontal = 4.dp), sizes = LocalSegSizes.current)
}

object SegmentedButtonDefaults {
    fun itemShape(index: Int, count: Int): Any? = null
}

/** Chip (.chip): pill, outlined; chosen = soft accent. Replaces Material's filter chip. */
@Composable
fun FilterChip(
    selected: Boolean,
    onClick: () -> Unit,
    label: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    leadingIcon: @Composable (() -> Unit)? = null,
) {
    val t = g
    androidx.compose.material3.Surface(
        onClick = onClick, enabled = enabled, shape = CircleShape,
        color = if (selected) t.accentSoft else t.surface,
        border = if (selected) null else BorderStroke(1.dp, t.line),
        modifier = modifier.alpha(if (enabled) 1f else 0.5f),
    ) {
        Row(Modifier.height(34.dp).padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            CompositionLocalProvider(androidx.compose.material3.LocalContentColor provides if (selected) t.accent else t.fg) {
                leadingIcon?.invoke()
                ProvideTextStyle(TextStyle(fontSize = 14.sp, fontWeight = FontWeight.W500, color = if (selected) t.accent else t.fg)) { label() }
            }
        }
    }
}


@Composable
fun RadioButton(selected: Boolean, onClick: (() -> Unit)?, modifier: Modifier = Modifier, enabled: Boolean = true) {
    androidx.compose.material3.RadioButton(selected, onClick, modifier, enabled,
        colors = androidx.compose.material3.RadioButtonDefaults.colors(selectedColor = g.accent, unselectedColor = g.faint))
}

@Composable
fun Checkbox(checked: Boolean, onCheckedChange: ((Boolean) -> Unit)?, modifier: Modifier = Modifier, enabled: Boolean = true) {
    androidx.compose.material3.Checkbox(checked, onCheckedChange, modifier, enabled,
        colors = androidx.compose.material3.CheckboxDefaults.colors(checkedColor = g.accent, uncheckedColor = g.faint, checkmarkColor = g.onAccent))
}

/** Title of the current sub-page: a section with the same title is not repeated below it. */
val LocalPageTitle = compositionLocalOf { "" }

/** Slider like the native range input of the preview: thin track, round thumb. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GhulSlider(value: Float, onValueChange: (Float) -> Unit, valueRange: ClosedFloatingPointRange<Float>, modifier: Modifier = Modifier) {
    val t = g
    androidx.compose.material3.Slider(
        value = value, onValueChange = onValueChange, valueRange = valueRange, modifier = modifier,
        thumb = { Box(Modifier.size(18.dp).clip(CircleShape).background(t.accent)) },
        track = { st ->
            val f = ((st.value - st.valueRange.start) / (st.valueRange.endInclusive - st.valueRange.start)).coerceIn(0f, 1f)
            Box(Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(t.surface3)) {
                Box(Modifier.fillMaxWidth(f).height(6.dp).clip(RoundedCornerShape(3.dp)).background(t.accent))
            }
        },
    )
}

/** A QR code, dark on white with a quiet zone, for scanning from another device. */
@Composable
fun QrImage(text: String, modifier: Modifier = Modifier) {
    val modules = androidx.compose.runtime.remember(text) { catching { qrModules(text) }.getOrDefault(emptyList()) }
    Box(modifier.clip(RoundedCornerShape(16.dp)).background(Color.White).padding(16.dp)) {
        androidx.compose.foundation.Canvas(Modifier.fillMaxWidth().aspectRatio(1f)) {
            val n = modules.size
            if (n == 0) return@Canvas
            val cell = size.width / n
            for (y in 0 until n) for (x in 0 until n) {
                if (modules[y][x]) drawRect(Color.Black, androidx.compose.ui.geometry.Offset(x * cell, y * cell), androidx.compose.ui.geometry.Size(cell + 0.5f, cell + 0.5f))
            }
        }
    }
}
