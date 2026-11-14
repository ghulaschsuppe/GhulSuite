// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context

/**
 * The emulator runs of the CI (tools/ci-emulator.sh) photograph every step. The app forbids
 * screenshots by default (FLAG_SECURE), which made every one of those pictures black. Only the
 * development build, and only on a device where the test set the system property
 * `debug.ghulsuite.screenshots` to 1 (adb shell setprop), allows them. The stable app and a
 * development build on a normal phone ignore it.
 */
object TestDevice {
    private const val PROPERTY = "debug.ghulsuite.screenshots"

    fun screenshotsAllowed(context: Context): Boolean =
        context.packageName.endsWith(".dev") && property(PROPERTY) == "1"

    private fun property(key: String): String = runCatching {
        Class.forName("android.os.SystemProperties").getMethod("get", String::class.java).invoke(null, key) as String
    }.getOrDefault("")
}
