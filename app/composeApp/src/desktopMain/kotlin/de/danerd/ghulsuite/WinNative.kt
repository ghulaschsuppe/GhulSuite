// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import java.awt.Window

/** Small Windows APIs used through JNA. Every call is a no-op elsewhere. */
object WinNative {
    val isWindows: Boolean = System.getProperty("os.name").lowercase().contains("win")

    @Suppress("FunctionName")
    private interface User32 : Library {
        fun SetWindowDisplayAffinity(hwnd: Pointer, affinity: Int): Boolean
        fun OpenClipboard(hwnd: Pointer?): Boolean
        fun EmptyClipboard(): Boolean
        fun SetClipboardData(format: Int, mem: Pointer?): Pointer?
        fun CloseClipboard(): Boolean
        fun RegisterClipboardFormatW(name: com.sun.jna.WString): Int
        fun GetClipboardSequenceNumber(): Int
        fun OpenInputDesktop(flags: Int, inherit: Boolean, access: Int): Pointer?
        fun SwitchDesktop(desktop: Pointer): Boolean
        fun CloseDesktop(desktop: Pointer): Boolean
    }

    @Suppress("FunctionName")
    private interface Kernel32 : Library {
        fun GlobalAlloc(flags: Int, bytes: Long): Pointer?
        fun GlobalLock(mem: Pointer): Pointer?
        fun GlobalUnlock(mem: Pointer): Boolean
        fun GlobalFree(mem: Pointer): Pointer?
    }

    private val user32: User32? by lazy { if (isWindows) runCatching { Native.load("user32", User32::class.java) }.getOrNull() else null }
    private val kernel32: Kernel32? by lazy { if (isWindows) runCatching { Native.load("kernel32", Kernel32::class.java) }.getOrNull() else null }

    private const val WDA_NONE = 0
    private const val WDA_EXCLUDEFROMCAPTURE = 0x11
    private const val CF_UNICODETEXT = 13
    private const val GMEM_MOVEABLE = 0x0002
    private const val DESKTOP_SWITCHDESKTOP = 0x0100

    /** Native window handle (HWND) of an AWT window, 0 if unknown. */
    fun hwnd(window: Window?): Long =
        if (!isWindows || window == null) 0 else runCatching { Pointer.nativeValue(Native.getWindowPointer(window)) }.getOrDefault(0)

    /** Keeps the window out of screenshots, screen recordings and screen sharing (Windows 10 2004+). */
    fun setCaptureProtection(window: Window, protect: Boolean) {
        val u = user32 ?: return
        val p = runCatching { Native.getWindowPointer(window) }.getOrNull() ?: return
        u.SetWindowDisplayAffinity(p, if (protect) WDA_EXCLUDEFROMCAPTURE else WDA_NONE)
    }

    /**
     * Puts text on the clipboard so that Windows neither keeps it in the clipboard history (Win+V)
     * nor syncs it to the cloud. Returns the clipboard sequence number to check later, or null if
     * the native way failed (the caller then uses the normal clipboard).
     */
    fun copyPrivate(text: String): Int? {
        val u = user32 ?: return null
        val k = kernel32 ?: return null
        if (!u.OpenClipboard(null)) return null
        try {
            u.EmptyClipboard()
            val chars = (text + "\u0000").toCharArray()
            val mem = k.GlobalAlloc(GMEM_MOVEABLE, chars.size * 2L) ?: return null
            val ptr = k.GlobalLock(mem) ?: run { k.GlobalFree(mem); return null }
            ptr.write(0, chars, 0, chars.size)
            k.GlobalUnlock(mem)
            if (u.SetClipboardData(CF_UNICODETEXT, mem) == null) {
                k.GlobalFree(mem)
                return null
            }
            // Documented markers for clipboard history and cloud clipboard.
            u.SetClipboardData(u.RegisterClipboardFormatW(com.sun.jna.WString("ExcludeClipboardContentFromMonitorProcessing")), null)
            setDword(u, k, "CanIncludeInClipboardHistory", 0)
            setDword(u, k, "CanUploadToCloudClipboard", 0)
        } finally {
            u.CloseClipboard()
        }
        return u.GetClipboardSequenceNumber()
    }

    private fun setDword(u: User32, k: Kernel32, format: String, value: Int) {
        val mem = k.GlobalAlloc(GMEM_MOVEABLE, 4) ?: return
        val ptr = k.GlobalLock(mem) ?: run { k.GlobalFree(mem); return }
        ptr.setInt(0, value)
        k.GlobalUnlock(mem)
        if (u.SetClipboardData(u.RegisterClipboardFormatW(com.sun.jna.WString(format)), mem) == null) k.GlobalFree(mem)
    }

    /** Empties the clipboard if it still holds what we put there ([sequence] from [copyPrivate]). */
    fun clearIfUnchanged(sequence: Int) {
        val u = user32 ?: return
        if (u.GetClipboardSequenceNumber() != sequence) return
        if (u.OpenClipboard(null)) {
            u.EmptyClipboard()
            u.CloseClipboard()
        }
    }

    /** True while Windows shows the lock screen (the input desktop cannot be switched to). */
    fun workstationLocked(): Boolean {
        val u = user32 ?: return false
        val desk = u.OpenInputDesktop(0, false, DESKTOP_SWITCHDESKTOP) ?: return true
        return try {
            !u.SwitchDesktop(desk)
        } finally {
            u.CloseDesktop(desk)
        }
    }
}
