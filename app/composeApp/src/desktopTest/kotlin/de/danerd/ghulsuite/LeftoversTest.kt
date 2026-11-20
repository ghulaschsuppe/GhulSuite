// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** What removed features left goes once; user data stays (review G9 13). */
class LeftoversTest {
    @Test
    fun onlyTheLeftoversOfRemovedFeaturesGo() {
        val root = Files.createTempDirectory("ghul-leftovers").toFile()
        try {
            val local = File(root, "files/local").apply { mkdirs() }
            val cache = File(root, "cache").apply { mkdirs() }
            val noBackup = File(root, "no_backup").apply { mkdirs() }
            val databases = File(root, "databases").apply { mkdirs() }
            fun file(dir: File, name: String) = File(dir, name).apply { parentFile.mkdirs(); writeText("x") }
            val old = listOf(
                file(local, "mail-cache/INBOX/1.eml"), file(local, "drive-offline/files"), file(local, "drive-pending"),
                file(cache, "scan/scan.jpg"), file(noBackup, "androidx.work.workdb"), file(noBackup, "androidx.work.workdb-wal"),
                file(databases, "androidx.work.workdb"),
            )
            val kept = listOf(
                file(root, "files/suite/suite.ghlsuite"), file(root, "files/suite/docs-0123/a.ghlblob"),
                file(local, "archive-seen.json"), file(local, "quick-bio.stamp"), file(cache, "shared/keep.pdf"),
                file(noBackup, "other.db"),
            )
            val removed = Leftovers.removeFiles(local, cache, listOf(noBackup, databases))
            assertEquals(7, removed.size, removed.toString())
            for (f in old) assertFalse(f.exists(), f.path)
            assertFalse(File(local, "mail-cache").exists())
            for (f in kept) assertTrue(f.exists(), f.path)
            // Once more: nothing left to remove, nothing else touched.
            assertEquals(emptyList(), Leftovers.removeFiles(local, cache, listOf(noBackup, databases)))
            for (f in kept) assertTrue(f.exists(), f.path)
        } finally {
            root.deleteRecursively()
        }
    }
}
