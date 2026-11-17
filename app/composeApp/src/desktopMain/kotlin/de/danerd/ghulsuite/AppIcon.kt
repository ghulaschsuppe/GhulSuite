// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.toComposeImageBitmap
import java.awt.image.BufferedImage
import javax.imageio.ImageIO

/** The program icon as read from the resources (null if missing). */
internal val baseIcon: BufferedImage? by lazy {
    runCatching {
        Thread.currentThread().contextClassLoader.getResourceAsStream("ghulsuite.png")?.use { ImageIO.read(it) }
    }.getOrNull()
}

/** Window and taskbar icon: the GhulSuite logo. */
internal fun appIcon(): Painter? = baseIcon?.let { BitmapPainter(it.toComposeImageBitmap()) }
