// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.autofill

/**
 * Known browsers. In a browser the website counts, never the browser app itself: without a
 * recognised website GhulSuite only offers the picker and never links a login to the browser.
 * The Chromium-based ones are also listed as compatibility packages in `autofill_service.xml`.
 */
object Browsers {
    private val PACKAGES = setOf(
        "com.android.chrome",
        "com.chrome.beta",
        "com.chrome.dev",
        "com.chrome.canary",
        "org.chromium.chrome",
        "com.vivaldi.browser",
        "com.vivaldi.browser.snapshot",
        "com.brave.browser",
        "com.brave.browser_beta",
        "com.brave.browser_nightly",
        "com.microsoft.emmx",
        "com.microsoft.emmx.beta",
        "com.microsoft.emmx.dev",
        "com.microsoft.emmx.canary",
        "com.opera.browser",
        "com.opera.browser.beta",
        "com.opera.mini.native",
        "com.opera.mini.native.beta",
        "com.opera.gx",
        "com.sec.android.app.sbrowser",
        "com.sec.android.app.sbrowser.beta",
        "com.duckduckgo.mobile.android",
        "com.kiwibrowser.browser",
        "org.bromite.bromite",
        "org.cromite.cromite",
        "com.ecosia.android",
        "com.yandex.browser",
        "com.mi.globalbrowser",
        "com.heytap.browser",
        "com.huawei.browser",
        "com.cloudmosa.puffinFree",
        "mark.via.gp",
        "com.stoutner.privacybrowser.standard",
        "us.spotco.mulch",
        "app.vanadium.browser",
        "org.mozilla.firefox",
        "org.mozilla.firefox_beta",
        "org.mozilla.fenix",
        "org.mozilla.fennec_fdroid",
        "org.mozilla.focus",
        "org.mozilla.klar",
        "io.github.forkmaintainers.iceraven",
        "org.torproject.torbrowser",
        "us.spotco.fennec_dos",
        "org.ironfoxoss.ironfox",
        "com.mullvad.browser",
    )

    fun isBrowser(packageName: String): Boolean = packageName in PACKAGES

    /** App stores whose browsers are trusted to report the website they show. */
    private val TRUSTED_INSTALLERS = setOf(
        "com.android.vending", // Google Play
        "com.sec.android.app.samsungapps", // Galaxy Store
        "com.huawei.appmarket",
        "com.xiaomi.market",
        "com.heytap.market",
        "com.oppo.market",
        "org.fdroid.fdroid",
        "org.fdroid.basic",
        "com.aurora.store",
        "com.amazon.venezia",
    )

    /**
     * A known browser that really is that browser: preinstalled or installed from an app store.
     * Any app can carry a browser's package name if that browser is not installed (sideloaded
     * fake); only a trusted one may say "this page is bank.de".
     */
    fun isTrustedBrowser(context: android.content.Context, packageName: String): Boolean {
        if (!isBrowser(packageName)) return false
        return runCatching {
            val pm = context.packageManager
            val info = pm.getApplicationInfo(packageName, 0)
            if (info.flags and (android.content.pm.ApplicationInfo.FLAG_SYSTEM or android.content.pm.ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) != 0) {
                return@runCatching true
            }
            val installer = if (android.os.Build.VERSION.SDK_INT >= 30) {
                pm.getInstallSourceInfo(packageName).installingPackageName
            } else {
                @Suppress("DEPRECATION")
                pm.getInstallerPackageName(packageName)
            }
            installer in TRUSTED_INSTALLERS
        }.getOrDefault(false)
    }
}
