// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

/**
 * Whether Android lets the user share the folder of the document [docId] of the device's storage
 * ("primary:Download/GhulSuite.ghlzip", "1234-ABCD:GhulSuite.ghlzip") with an app: not the top
 * of a volume and not "Download" itself (the system's folder picker refuses both). A folder
 * inside Download is fine.
 */
fun folderShareable(docId: String): Boolean {
    val colon = docId.indexOf(':')
    if (colon < 0) return true
    val path = docId.substring(colon + 1).trim('/')
    val parent = path.substringBeforeLast('/', "")
    return parent.isNotEmpty() && !parent.equals("Download", ignoreCase = true)
}

/**
 * Whether a folder chosen in the system's folder picker ([treeDocId], e.g. "primary:GhulSuite")
 * can take the suite file safely: not the top of a volume ("primary:", "1234-ABCD:") and not
 * "Download" itself (Android lets no app keep these); a folder inside them is fine.
 */
fun treeFolderAccepted(treeDocId: String): Boolean {
    val colon = treeDocId.indexOf(':')
    if (colon < 0) return false
    val path = treeDocId.substring(colon + 1).trim('/')
    return path.isNotEmpty() && !path.equals("Download", ignoreCase = true)
}
