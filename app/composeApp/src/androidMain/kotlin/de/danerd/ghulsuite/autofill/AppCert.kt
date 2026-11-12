// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.autofill

import android.content.Context
import android.content.pm.PackageManager
import java.security.MessageDigest

/** SHA-256 of an app's signing certificate (lower-case hex), or null if it cannot be read. */
object AppCert {
    fun sha256(context: Context, packageName: String): String? = runCatching {
        val info = context.packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        val signing = info.signingInfo ?: return null
        // Current signer (after key rotation the newest one); with several signers the first.
        val certs = if (signing.hasMultipleSigners()) signing.apkContentsSigners else signing.signingCertificateHistory
        val sig = certs?.firstOrNull() ?: return null
        MessageDigest.getInstance("SHA-256").digest(sig.toByteArray()).joinToString("") { "%02x".format(it) }
    }.getOrNull()
}
