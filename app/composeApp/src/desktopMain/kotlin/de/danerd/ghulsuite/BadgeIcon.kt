// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.toComposeImageBitmap
import java.awt.BasicStroke
import java.awt.Color
import java.awt.Font
import java.awt.RenderingHints
import java.awt.geom.Path2D
import java.awt.geom.RoundRectangle2D
import java.awt.image.BufferedImage
import javax.imageio.ImageIO

/** The program icon as read from the resources (null if missing). */
internal val baseIcon: BufferedImage? by lazy {
    runCatching {
        Thread.currentThread().contextClassLoader.getResourceAsStream("ghulsuite.png")?.use { ImageIO.read(it) }
    }.getOrNull()
}

/**
 * Window and taskbar icon: the GhulSuite logo, with a small envelope in the mail colour showing
 * the number of unread messages when there are any (like the badge in the navigation).
 */
internal fun badgeIcon(unread: Int): Painter? {
    val base = baseIcon ?: return null
    if (unread <= 0) return BitmapPainter(base.toComposeImageBitmap())
    val size = 128
    val img = BufferedImage(size, size, BufferedImage.TYPE_INT_ARGB)
    val g = img.createGraphics()
    g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON)
    g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON)
    g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_BICUBIC)
    g.drawImage(base, 0, 0, size, size, null)
    // Envelope in the lower right corner with a white outline so it stands out on any taskbar.
    val w = 84.0
    val h = 62.0
    val x = size - w - 2
    val y = size - h - 2
    val env = RoundRectangle2D.Double(x, y, w, h, 16.0, 16.0)
    g.color = Color.WHITE
    g.stroke = BasicStroke(6f)
    g.draw(env)
    g.color = Color(0xE0, 0x70, 0x0F)
    g.fill(env)
    val flap = Path2D.Double().apply {
        moveTo(x + 8, y + 9)
        lineTo(x + w / 2, y + h * 0.42)
        lineTo(x + w - 8, y + 9)
    }
    g.color = Color(255, 255, 255, 150)
    g.stroke = BasicStroke(4f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND)
    g.draw(flap)
    val text = if (unread > 99) "99+" else unread.toString()
    g.font = Font(Font.SANS_SERIF, Font.BOLD, if (text.length > 2) 26 else 34)
    val fm = g.fontMetrics
    g.color = Color.WHITE
    g.drawString(text, (x + (w - fm.stringWidth(text)) / 2).toFloat(), (y + h - 10).toFloat())
    g.dispose()
    return BitmapPainter(img.toComposeImageBitmap())
}
