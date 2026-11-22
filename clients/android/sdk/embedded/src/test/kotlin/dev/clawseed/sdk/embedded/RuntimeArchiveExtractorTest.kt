package dev.clawseed.sdk.embedded

import org.apache.commons.compress.archivers.tar.TarArchiveEntry
import org.apache.commons.compress.archivers.tar.TarArchiveOutputStream
import org.apache.commons.compress.archivers.tar.TarConstants
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.tukaani.xz.LZMA2Options
import org.tukaani.xz.XZOutputStream
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.file.Files
import java.util.zip.GZIPOutputStream

class RuntimeArchiveExtractorTest {
    @get:Rule val temporary = TemporaryFolder()

    private fun tar(pax: Boolean = false, entries: TarArchiveOutputStream.() -> Unit): ByteArray {
        val output = ByteArrayOutputStream()
        TarArchiveOutputStream(output).use { stream ->
            stream.setLongFileMode(if (pax) TarArchiveOutputStream.LONGFILE_POSIX else TarArchiveOutputStream.LONGFILE_GNU)
            stream.entries()
        }
        return output.toByteArray()
    }

    private fun TarArchiveOutputStream.file(name: String, content: String, executable: Boolean = false) {
        val bytes = content.toByteArray()
        putArchiveEntry(TarArchiveEntry(name).apply { size = bytes.size.toLong(); mode = if (executable) 493 else 420 })
        write(bytes)
        closeArchiveEntry()
    }

    private fun TarArchiveOutputStream.link(name: String, target: String, hard: Boolean = false) {
        putArchiveEntry(TarArchiveEntry(name, if (hard) TarConstants.LF_LINK else TarConstants.LF_SYMLINK).apply { linkName = target })
        closeArchiveEntry()
    }

    private fun extract(bytes: ByteArray, xz: Boolean = false, limit: Long = 1024 * 1024): File {
        val compressed = ByteArrayOutputStream()
        (if (xz) XZOutputStream(compressed, LZMA2Options(1)) else GZIPOutputStream(compressed)).use { it.write(bytes) }
        val destination = temporary.newFolder()
        RuntimeArchiveExtractor.extract(ByteArrayInputStream(compressed.toByteArray()), destination,
            if (xz) "tar.xz" else "tar.gz", limit)
        return destination
    }

    @Test fun preservesGnuAndPaxLongNamesAndExecutableBits() {
        for (pax in listOf(false, true)) {
            val name = "rootfs/opt/" + "long name ".repeat(15) + "/tool"
            val root = extract(tar(pax) { file(name, "ready", true) }, xz = pax)
            assertEquals("ready", File(root, name).readText())
            assertTrue(File(root, name).canExecute())
        }
    }

    @Test fun preservesGuestSymlinksAndMaterializesForwardHardlinks() {
        val root = extract(tar {
            link("rootfs/usr/local/bin/python3", "../../../opt/python/bin/python3")
            link("rootfs/alias", "rootfs/payload", hard = true)
            file("rootfs/payload", "payload", true)
            file("rootfs/opt/python/bin/python3", "python", true)
        })
        assertTrue(Files.isSymbolicLink(File(root, "rootfs/usr/local/bin/python3").toPath()))
        assertEquals("python", File(root, "rootfs/usr/local/bin/python3").readText())
        assertEquals("payload", File(root, "rootfs/alias").readText())
    }

    @Test fun rejectsTraversalAndWritesThroughGuestSymlinks() {
        for (bytes in listOf(tar { file("../escaped", "bad") }, tar {
            link("rootfs/escape", temporary.root.absolutePath)
            file("rootfs/escape/escaped", "bad")
        })) {
            assertThrows(IllegalArgumentException::class.java) { extract(bytes) }
        }
        assertFalse(File(temporary.root, "escaped").exists())
    }

    @Test fun rejectsTruncatedEntriesAndOversizedOutput() {
        val bytes = tar { file("payload", "payload") }
        assertThrows(Exception::class.java) { extract(bytes.copyOf(514)) }
        assertThrows(IllegalStateException::class.java) { extract(bytes, limit = 3) }
    }

    /** Optional full bundle test, also run by the documented runtime build validation. */
    @Test fun extractsBuiltRuntime() {
        val path = System.getProperty("clawseed.runtime.bundle")
        assumeTrue(path != null)
        val bundle = File(path!!)
        val manifest = JSONObject(File(bundle.parentFile, "manifest.json").readText())
        val destination = temporary.newFolder()
        bundle.inputStream().use {
            RuntimeArchiveExtractor.extract(it, destination, "tar.xz", manifest.getLong("unpacked_size"))
        }
        val launcher = File(destination, "rootfs/usr/local/bin/python3")
        assertFalse("Python entry must preserve the real installation path for copied venvs", Files.isSymbolicLink(launcher.toPath()))
        assertTrue(launcher.readText().contains("exec /opt/python/bin/python3"))
        for (name in listOf("bin/sh", "usr/bin/env", "opt/node/bin/node", "opt/python/bin/python3")) {
            assertTrue("missing $name", File(destination, "rootfs/$name").canExecute())
        }
    }
}
