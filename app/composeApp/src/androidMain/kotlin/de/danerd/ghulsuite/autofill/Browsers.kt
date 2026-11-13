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
}
