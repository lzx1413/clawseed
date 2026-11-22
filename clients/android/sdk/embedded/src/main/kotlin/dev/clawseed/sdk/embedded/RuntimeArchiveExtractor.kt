package dev.clawseed.sdk.embedded

import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.tukaani.xz.XZInputStream
import java.io.File
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.Path
import java.util.zip.GZIPInputStream
import java.util.zip.ZipInputStream

/** Extracts into a fresh staging tree. Guest symlinks are preserved but never followed by writes. */
internal object RuntimeArchiveExtractor {
    fun extract(raw: InputStream, destination: File, format: String, maxBytes: Long) {
        require(maxBytes > 0)
        destination.mkdirs()
        val root = destination.canonicalFile.toPath()
        var remaining = maxBytes
        fun write(input: InputStream, path: Path) {
            Files.createDirectories(path.parent)
            check(!Files.exists(path, NOFOLLOW_LINKS)) { "duplicate runtime entry: $path" }
            Files.newOutputStream(path).use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    remaining -= count
                    check(remaining >= 0) { "runtime exceeds its unpacked size limit" }
                    output.write(buffer, 0, count)
                }
            }
        }
        if (format == "zip") {
            ZipInputStream(raw).use { zip ->
                while (true) {
                    val entry = zip.nextEntry ?: break
                    val path = safePath(root, entry.name)
                    if (entry.isDirectory) Files.createDirectories(path) else write(zip, path)
                    zip.closeEntry()
                }
            }
            File(destination, "proot").setExecutable(true, true)
            return
        }
        val decoded = when (format) {
            "tar.gz" -> GZIPInputStream(raw.buffered())
            "tar.xz" -> XZInputStream(raw.buffered(), 64 * 1024)
            else -> error("unsupported runtime archive: $format")
        }
        val hardlinks = mutableListOf<Pair<Path, Path>>()
        TarArchiveInputStream(decoded).use { tar ->
            while (true) {
                val entry = tar.nextEntry ?: break
                require(!entry.isSparse && entry.isCheckSumOK) { "invalid runtime tar entry" }
                val path = safePath(root, entry.name)
                when {
                    entry.isDirectory -> Files.createDirectories(path)
                    entry.isSymbolicLink -> {
                        Files.createDirectories(path.parent)
                        Files.createSymbolicLink(path, java.nio.file.Paths.get(entry.linkName))
                    }
                    entry.isLink -> hardlinks += path to safePath(root, entry.linkName)
                    entry.isFile -> {
                        write(tar, path)
                        if (entry.mode and 0b001_001_001 != 0) path.toFile().setExecutable(true, true)
                    }
                    else -> error("unsupported runtime tar entry: ${entry.name}")
                }
            }
        }
        // Materialize hardlinks: Android app sandboxes may reject link(2).
        while (hardlinks.isNotEmpty()) {
            val count = hardlinks.size
            val iterator = hardlinks.iterator()
            while (iterator.hasNext()) {
                val (path, target) = iterator.next()
                safePath(root, root.relativize(path).toString())
                safePath(root, root.relativize(target).toString())
                if (Files.isRegularFile(target, NOFOLLOW_LINKS)) {
                    Files.newInputStream(target).use { write(it, path) }
                    path.toFile().setExecutable(target.toFile().canExecute(), true)
                    iterator.remove()
                }
            }
            check(hardlinks.size < count) { "unresolved runtime hardlinks" }
        }
    }

    private fun safePath(root: Path, name: String): Path {
        require(name.isNotEmpty() && !name.startsWith('/')) { "unsafe runtime path" }
        val path = root.resolve(name).normalize()
        require(path.startsWith(root)) { "unsafe runtime path" }
        var parent: Path? = path
        while (parent != null && parent != root) {
            require(!Files.isSymbolicLink(parent)) { "runtime entry writes through a symlink: $name" }
            parent = parent.parent
        }
        return path
    }
}
