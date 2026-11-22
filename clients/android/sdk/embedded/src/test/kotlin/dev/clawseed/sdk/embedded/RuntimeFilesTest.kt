package dev.clawseed.sdk.embedded

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.nio.file.Files

class RuntimeFilesTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun removesUnreadableProotBindingPlaceholders() {
        val runtime = temporary.newFolder("runtime-unreadable")
        val mount = File(runtime, "system").apply { mkdir() }
        File(mount, "payload").writeText("temporary")
        mount.setReadable(false, false)
        mount.setWritable(false, false)
        mount.setExecutable(false, false)
        assertTrue(RuntimeFiles.deleteTree(runtime))
        assertFalse(runtime.exists())
    }

    @Test fun cleanupNeverTraversesGuestSymlinks() {
        val external = temporary.newFolder("external")
        val sentinel = File(external, "keep").apply { writeText("keep") }
        val runtime = temporary.newFolder("runtime")
        Files.createSymbolicLink(File(runtime, "absolute").toPath(), external.toPath())
        Files.createSymbolicLink(File(runtime, "relative").toPath(), java.nio.file.Paths.get("../external"))
        Files.createSymbolicLink(File(runtime, "dangling").toPath(), java.nio.file.Paths.get("missing"))
        assertTrue(RuntimeFiles.deleteTree(runtime))
        assertFalse(runtime.exists())
        assertEquals("keep", sentinel.readText())
        val link = File(temporary.root, "top-level-link")
        Files.createSymbolicLink(link.toPath(), external.toPath())
        assertTrue(RuntimeFiles.deleteTree(link))
        assertEquals("keep", sentinel.readText())
    }
}
