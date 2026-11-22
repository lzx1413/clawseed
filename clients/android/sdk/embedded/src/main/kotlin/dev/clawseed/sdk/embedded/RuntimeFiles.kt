package dev.clawseed.sdk.embedded

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.attribute.BasicFileAttributes

internal object RuntimeFiles {
    /** Guest links may point outside this tree. Never traverse them during cleanup. */
    fun deleteTree(file: File): Boolean = runCatching {
        deleteTreeOrThrow(file)
        true
    }.getOrDefault(false)

    fun deleteTreeOrThrow(file: File) = deletePath(file.toPath())

    private fun deletePath(path: Path) {
        val attributes = try {
            Files.readAttributes(path, BasicFileAttributes::class.java, NOFOLLOW_LINKS)
        } catch (_: NoSuchFileException) {
            return
        }
        if (attributes.isDirectory && !attributes.isSymbolicLink) {
            // PRoot creates mode-000 placeholders for bindings such as /system.
            // Restore owner access before enumerating our own directory, never a link.
            path.toFile().apply {
                setReadable(true, true)
                setWritable(true, true)
                setExecutable(true, true)
            }
            Files.newDirectoryStream(path).use { children ->
                for (child in children) deletePath(child)
            }
        }
        Files.delete(path)
    }
}
