// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Shapes
import androidx.compose.material3.Surface
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.Typography
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ThemeMode
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource
import androidx.compose.foundation.combinedClickable

// --- Design system -------------------------------------------------------------------------
// Calm dark-slate surfaces, one violet accent, a colour per area, rounded cards without borders.

private val Violet = Color(0xFF7B61FF)
private val VioletLight = Color(0xFFA996FF)

private val LightColors = lightColorScheme(
    primary = Color(0xFF6A4DF5),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE9E4FF),
    onPrimaryContainer = Color(0xFF26145F),
    secondary = Color(0xFF5B5E78),
    secondaryContainer = Color(0xFFE9E4FF),
    onSecondaryContainer = Color(0xFF26145F),
    tertiary = Color(0xFF0E9F80),
    background = Color(0xFFF4F4F8),
    onBackground = Color(0xFF15161C),
    surface = Color.White,
    onSurface = Color(0xFF15161C),
    surfaceVariant = Color(0xFFF0F0F6),
    onSurfaceVariant = Color(0xFF5F6173),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color.White,
    surfaceContainer = Color.White,
    surfaceContainerHigh = Color.White,
    surfaceContainerHighest = Color(0xFFE7E7F0),
    outline = Color(0xFF8A8CA0),
    outlineVariant = Color(0xFFE3E3EC),
    error = Color(0xFFD03B4C),
)

private val DarkColors = darkColorScheme(
    primary = VioletLight,
    onPrimary = Color(0xFF160D40),
    primaryContainer = Color(0xFF2E2560),
    onPrimaryContainer = Color(0xFFE9E4FF),
    secondary = Color(0xFFB8BAD0),
    secondaryContainer = Color(0xFF34305A),
    onSecondaryContainer = Color(0xFFE9E4FF),
    tertiary = Color(0xFF4FD8B6),
    background = Color(0xFF0F1015),
    onBackground = Color(0xFFE8E8F0),
    surface = Color(0xFF181920),
    onSurface = Color(0xFFE8E8F0),
    surfaceVariant = Color(0xFF1F2029),
    onSurfaceVariant = Color(0xFFA2A4B8),
    surfaceContainerLowest = Color(0xFF181920),
    surfaceContainerLow = Color(0xFF181920),
    surfaceContainer = Color(0xFF181920),
    surfaceContainerHigh = Color(0xFF181920),
    surfaceContainerHighest = Color(0xFF2A2B36),
    outline = Color(0xFF6F7187),
    outlineVariant = Color(0xFF2B2D3B),
    error = Color(0xFFFF7A86),
)

private val GhulShapes = Shapes(
    extraSmall = RoundedCornerShape(10.dp),
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(20.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

private fun ghulTypography(): Typography {
    val t = Typography()
    return t.copy(
        headlineMedium = t.headlineMedium.copy(fontWeight = FontWeight.Bold, letterSpacing = (-0.5).sp),
        headlineSmall = t.headlineSmall.copy(fontWeight = FontWeight.SemiBold, letterSpacing = (-0.25).sp),
        titleLarge = t.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = t.titleMedium.copy(fontWeight = FontWeight.SemiBold),
        titleSmall = t.titleSmall.copy(fontWeight = FontWeight.SemiBold),
        labelLarge = t.labelLarge.copy(fontWeight = FontWeight.SemiBold),
        bodyLarge = t.bodyLarge.copy(fontSize = 15.sp, lineHeight = 22.sp, letterSpacing = 0.sp),
        bodyMedium = t.bodyMedium.copy(fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp),
        bodySmall = t.bodySmall.copy(fontSize = 13.sp, lineHeight = 18.sp, letterSpacing = 0.sp),
        labelMedium = t.labelMedium.copy(fontSize = 12.sp, letterSpacing = 0.sp),
    )
}

val LocalDark = androidx.compose.runtime.staticCompositionLocalOf { false }

@Composable
fun GhulTheme(mode: ThemeMode = ThemeMode.SYSTEM, content: @Composable () -> Unit) {
    val dark = when (mode) {
        ThemeMode.SYSTEM -> isSystemInDarkTheme()
        ThemeMode.LIGHT -> false
        ThemeMode.DARK -> true
    }
    androidx.compose.runtime.CompositionLocalProvider(LocalDark provides dark, LocalTokens provides if (dark) DarkTokens else LightTokens) {
        MaterialTheme(
            colorScheme = if (dark) DarkColors else LightColors,
            shapes = GhulShapes,
            typography = ghulTypography(),
            content = content,
        )
    }
}

@Composable
fun okColor(): Color = g.ok

@Composable
fun warnColor(): Color = g.warn

@Composable
fun dangerColor(): Color = g.danger

/** Colour of each area, used for its icon, avatars and accents. */
@Composable
fun areaColor(tab: Tab): Color {
    val dark = LocalDark.current
    return when (tab) {
        Tab.PASS -> if (dark) VioletLight else Violet
        Tab.AUTH -> if (dark) Color(0xFF4FD8B6) else Color(0xFF0E9F80)
        Tab.DRIVE -> if (dark) Color(0xFF6FA8FF) else Color(0xFF2F74E8)
        Tab.MAIL -> if (dark) Color(0xFFFFA25C) else Color(0xFFE0700F)
        Tab.CAL -> if (dark) Color(0xFFFF7AA8) else Color(0xFFD6336C)
        Tab.SETTINGS -> g.accent
    }
}

private val AvatarColors = listOf(
    0xFF7B61FF, 0xFF2F74E8, 0xFF0E9F80, 0xFFE0700F, 0xFFD03B7E, 0xFF8E5BD8, 0xFF1E9BB8, 0xFFC0533A, 0xFF4E8A2E, 0xFFB58600,
)

/** Colour n of the avatar palette (a0–a9 of the design preview). */
fun paletteColor(i: Int): Color = Color(AvatarColors[i % AvatarColors.size])

/** Stable colour for a name (same name → same colour). */
fun avatarColor(key: String): Color {
    var h = 0
    for (c in key.lowercase()) h = h * 31 + c.code
    return Color(AvatarColors[(h and 0x7fffffff) % AvatarColors.size])
}

/** Avatars are 35 % brighter in the dark theme (as in the design preview). */
fun lighten(c: Color): Color = Color((c.red * 1.35f).coerceAtMost(1f), (c.green * 1.35f).coerceAtMost(1f), (c.blue * 1.35f).coerceAtMost(1f), c.alpha)

/** Rounded square with the first letter of [text], tinted by its colour. */
@Composable
fun Avatar(text: String, modifier: Modifier = Modifier, color: Color = avatarColor(text), size: Dp = 42.dp) {
    val letter = text.trim().firstOrNull { it.isLetterOrDigit() }?.uppercase() ?: "•"
    val tint = if (LocalDark.current) lighten(color) else color
    Box(
        modifier.size(size).clip(RoundedCornerShape(size * 0.31f)).background(tint.copy(alpha = 0.14f)),
        contentAlignment = Alignment.Center,
    ) {
        Text(letter, color = tint, fontWeight = FontWeight.Bold, fontSize = (size.value * 0.405f).sp)
    }
}

/** Rounded square with an icon, tinted like [Avatar]. */
@Composable
fun IconTile(icon: ImageVector, color: Color, size: Dp = 42.dp, solid: Boolean = false) {
    val tint = if (LocalDark.current && !solid) lighten(color) else color
    Box(
        Modifier.size(size).clip(RoundedCornerShape(size * 0.31f))
            .background(if (solid) color else tint.copy(alpha = 0.14f)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, null, tint = if (solid) Color.White else tint, modifier = Modifier.size(size * 0.53f))
    }
}

/** Colours of password characters: digits and symbols stand out from letters. */
@Composable
fun passwordColors(): Pair<Color, Color> =
    if (LocalDark.current) Color(0xFF7FB2FF) to Color(0xFFFF8FA3) else Color(0xFF1F66D6) to Color(0xFFC92A5A)

fun colorizePassword(value: String, digit: Color, symbol: Color): AnnotatedString = buildAnnotatedString {
    for (c in value) {
        when {
            c.isDigit() -> withStyle(SpanStyle(color = digit)) { append(c) }
            !c.isLetter() -> withStyle(SpanStyle(color = symbol)) { append(c) }
            else -> append(c)
        }
    }
}

/** A password shown in monospace with coloured digits and symbols. */
@Composable
fun PasswordText(value: String, modifier: Modifier = Modifier, style: TextStyle = MaterialTheme.typography.bodyLarge) {
    val (digit, symbol) = passwordColors()
    Text(colorizePassword(value, digit, symbol), modifier = modifier, style = style, fontFamily = FontFamily.Monospace)
}

/** Colours a visible password field like [PasswordText]. */
private class PasswordColoring(private val digit: Color, private val symbol: Color) : VisualTransformation {
    override fun filter(text: AnnotatedString) =
        TransformedText(colorizePassword(text.text, digit, symbol), OffsetMapping.Identity)
}

@Composable
fun SecretTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier.fillMaxWidth(),
    numeric: Boolean = false,
    trailing: @Composable (() -> Unit)? = null,
) {
    var visible by remember { mutableStateOf(false) }
    val (digit, symbol) = passwordColors()
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        singleLine = true,
        textStyle = if (visible) LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace) else LocalTextStyle.current,
        visualTransformation = if (visible) PasswordColoring(digit, symbol) else PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(
            keyboardType = if (numeric) KeyboardType.NumberPassword else KeyboardType.Password,
            autoCorrectEnabled = false,
        ),
        trailingIcon = {
            Row {
                trailing?.invoke()
                IconButton(onClick = { visible = !visible }) {
                    Icon(
                        if (visible) Icons.Filled.VisibilityOff else Icons.Filled.Visibility,
                        contentDescription = stringResource(if (visible) Res.string.a_hide else Res.string.a_show),
                    )
                }
            }
        },
        modifier = modifier,
    )
}

/** Search field (.search): 48 dp pill on the third surface, icon, no outline. */
@Composable
fun SearchField(
    query: String,
    onQuery: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    onSearch: (() -> Unit)? = null,
    trailing: @Composable (() -> Unit)? = null,
) {
    val t = g
    Row(
        modifier.fillMaxWidth().height(48.dp).clip(RoundedCornerShape(24.dp)).background(t.surface3).padding(start = 16.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(Icons.Filled.Search, null, Modifier.size(20.dp), tint = t.muted)
        val style = MaterialTheme.typography.bodyLarge.copy(color = t.fg)
        androidx.compose.foundation.text.BasicTextField(
            query, onQuery, Modifier.weight(1f), singleLine = true, textStyle = style,
            keyboardOptions = KeyboardOptions(imeAction = if (onSearch != null) androidx.compose.ui.text.input.ImeAction.Search else androidx.compose.ui.text.input.ImeAction.Done),
            keyboardActions = androidx.compose.foundation.text.KeyboardActions(onSearch = { onSearch?.invoke() }),
            cursorBrush = androidx.compose.ui.graphics.SolidColor(t.accent),
            decorationBox = { inner ->
                Box {
                    if (query.isEmpty()) Text(placeholder ?: stringResource(Res.string.l_search), style = style.copy(color = t.muted))
                    inner()
                }
            },
        )
        when {
            trailing != null -> trailing()
            query.isNotEmpty() -> IconButton(onClick = { onQuery("") }) { Icon(Icons.Filled.Close, stringResource(Res.string.a_cancel), tint = t.muted) }
            else -> Box(Modifier.width(10.dp))
        }
    }
}

/** A rounded card that groups rows; rows inside have no own background. */
@Composable
fun ListCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surface,
    ) {
        androidx.compose.runtime.CompositionLocalProvider(LocalOnCard provides true) {
            Column(Modifier.padding(vertical = 4.dp), content = content)
        }
    }
}

/** One row of a list: leading tile, title, subtitle, trailing content. */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
fun ListRow(
    title: String,
    subtitle: String? = null,
    leading: @Composable (() -> Unit)? = null,
    trailing: @Composable (() -> Unit)? = null,
    bold: Boolean = false,
    divider: Boolean = false,
    dividerStart: Dp? = null,
    selected: Boolean = false,
    onLongClick: (() -> Unit)? = null,
    onClick: (() -> Unit)? = null,
) {
    if (divider) RowDivider(dividerStart ?: if (leading != null) 70.dp else 16.dp)
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp)
            .then(if (selected) Modifier.background(g.surface2) else Modifier)
            .then(
                when {
                    onLongClick != null -> Modifier.combinedClickable(onClick = onClick ?: {}, onLongClick = onLongClick)
                    onClick != null -> Modifier.clickable(onClick = onClick)
                    else -> Modifier
                },
            )
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        leading?.invoke()
        Column(Modifier.weight(1f)) {
            Text(
                title, maxLines = 1, overflow = TextOverflow.Ellipsis,
                fontSize = 15.sp,
                fontWeight = if (bold) FontWeight.Bold else FontWeight.W500,
            )
            if (!subtitle.isNullOrBlank()) {
                Text(
                    subtitle, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    fontSize = 14.sp, color = g.muted,
                    fontWeight = if (bold) FontWeight.SemiBold else FontWeight.Normal,
                )
            }
        }
        trailing?.invoke()
    }
}

/** Thin line between the rows of a card, starting under the text (not under the icon). */
@Composable
fun RowDivider(start: Dp = 70.dp) {
    Box(
        Modifier.fillMaxWidth().padding(start = start, end = 16.dp).height(1.dp)
            .background(g.line),
    )
}

/** Small heading above a card ("11 EINTRÄGE", "HEUTE"), optionally with an action on the right. */
@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier, trailing: @Composable (() -> Unit)? = null) {
    Row(
        modifier.fillMaxWidth().padding(start = 4.dp, end = 4.dp, top = 6.dp).heightIn(min = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text.uppercase(), Modifier.weight(1f),
            fontSize = 13.sp, letterSpacing = 0.65.sp, fontWeight = FontWeight.SemiBold,
            color = g.muted,
        )
        if (trailing != null) {
            androidx.compose.material3.ProvideTextStyle(androidx.compose.ui.text.TextStyle(fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = g.accent)) {
                trailing()
            }
        }
    }
}

/** Filter chips (one selected); scroll sideways when they do not fit. */
@Composable
fun FilterChipRow(
    options: List<Pair<String, String>>,
    selected: String,
    color: Color,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    icons: Map<String, ImageVector> = emptyMap(),
) {
    Row(
        modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        for ((id, label) in options) {
            val on = id == selected
            val t = g
            Surface(
                onClick = { onSelect(id) },
                shape = CircleShape,
                color = if (on) t.accentSoft else t.surface,
                border = if (on) null else BorderStroke(1.dp, t.line),
            ) {
                Row(
                    Modifier.height(34.dp).padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    icons[id]?.let { Icon(it, null, Modifier.size(16.dp), tint = if (on) t.accent else t.fg) }
                    Text(label, fontSize = 14.sp, color = if (on) t.accent else t.fg, fontWeight = FontWeight.W500)
                }
            }
        }
    }
}

/**
 * One item of a lazy list drawn as part of a single card: the first item gets the top corners,
 * the last one the bottom corners.
 */
@Composable
fun Modifier.cardItem(index: Int, count: Int): Modifier {
    val r = 20.dp
    val shape = RoundedCornerShape(
        topStart = if (index == 0) r else 0.dp, topEnd = if (index == 0) r else 0.dp,
        bottomStart = if (index == count - 1) r else 0.dp, bottomEnd = if (index == count - 1) r else 0.dp,
    )
    return this.fillMaxWidth().padding(horizontal = 16.dp).clip(shape).background(MaterialTheme.colorScheme.surface)
        .padding(top = if (index == 0) 4.dp else 0.dp, bottom = if (index == count - 1) 4.dp else 0.dp)
}

/** Empty state: a tinted icon and a short text. */
@Composable
fun EmptyState(icon: ImageVector, text: String, color: Color = MaterialTheme.colorScheme.primary) {
    Column(
        Modifier.fillMaxWidth().padding(vertical = 48.dp, horizontal = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        IconTile(icon, color, size = 64.dp)
        Text(text, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
    }
}

@Composable
fun SectionCard(title: String? = null, content: @Composable ColumnScope.() -> Unit) {
    // As in the design preview: the title stands above the card (.section-title), not inside it.
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (title != null && !title.equals(LocalPageTitle.current, ignoreCase = true)) SectionLabel(title)
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = MaterialTheme.shapes.large,
            color = MaterialTheme.colorScheme.surface,
        ) {
            androidx.compose.runtime.CompositionLocalProvider(LocalOnCard provides true) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    content()
                }
            }
        }
    }
}

/** Busy indicator, with "touch your key" when the key is waiting. */
@Composable
fun BusyBanner(state: AppState) {
    if (!state.busy) return
    val color = if (state.touchNeeded) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        color = if (state.touchNeeded) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = color)
            Spacer(Modifier.width(12.dp))
            Text(
                stringResource(if (state.touchNeeded) Res.string.s_touch_key else Res.string.s_busy),
                color = color,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

@Composable
fun ErrorText(text: String?) {
    if (text.isNullOrEmpty()) return
    Text(text, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
}

/** zxcvbn score 0..4 as a bar with a label. */
@Composable
fun StrengthMeter(score: Int) {
    val t = g
    val sc = score.coerceIn(0, 4)
    val color = if (sc < 2) t.danger else if (sc < 3) t.warn else t.ok
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(t.surface3)) {
            Box(Modifier.fillMaxWidth((sc + 1) / 5f).height(6.dp).clip(RoundedCornerShape(3.dp)).background(color))
        }
        Text(stringResource(strengthLabel(sc)), color = t.muted, fontSize = 13.sp)
    }
}

@Composable
fun Hint(text: String) {
    Text(text, fontSize = 13.sp, lineHeight = 18.sp, color = g.muted)
}

/** App name with a shield tile and the four areas, at the top of start and unlock. */
@Composable
fun BrandHeader(subtitle: String? = null) {
    Column(Modifier.fillMaxWidth().padding(top = 40.dp, bottom = 24.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(
            Modifier.shadow(18.dp, RoundedCornerShape(26.dp), ambientColor = Color(0xFF6A4DF5), spotColor = Color(0xFF6A4DF5))
                .size(84.dp).clip(RoundedCornerShape(26.dp))
                .background(androidx.compose.ui.graphics.Brush.linearGradient(listOf(Color(0xFF8A72FF), Color(0xFF5B3DF0)))),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Outlined.Shield, null, tint = Color.White, modifier = Modifier.size(46.dp))
        }
        Text(stringResource(Res.string.app_name), fontSize = 28.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.84).sp, modifier = Modifier.padding(top = 4.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (t in listOf(Tab.PASS, Tab.AUTH, Tab.DRIVE, Tab.MAIL)) {
                Box(Modifier.size(width = 24.dp, height = 4.dp).clip(RoundedCornerShape(2.dp)).background(areaColor(t)))
            }
        }
        if (subtitle != null) Text(subtitle, color = g.muted, textAlign = TextAlign.Center)
    }
}

/**
 * PIN of the security key. Where the system asks for it in its own dialog (Windows), only a hint
 * is shown instead.
 */
@Composable
fun KeyPinField(state: AppState, pin: String, onPin: (String) -> Unit) {
    if (state.backend.keyPinInApp) {
        SecretTextField(pin, onPin, stringResource(Res.string.l_key_pin))
    } else {
        Hint(stringResource(Res.string.key_system_dialog))
    }
}

/** A PIN is entered, or the system asks for it itself. */
fun keyPinOk(state: AppState, pin: String): Boolean = !state.backend.keyPinInApp || pin.isNotEmpty()

/** Display name of the storage location: the app's own storage gets a readable name. */
@Composable
fun storageLabel(raw: String): String = if (raw == "app") stringResource(Res.string.storage_app) else raw
