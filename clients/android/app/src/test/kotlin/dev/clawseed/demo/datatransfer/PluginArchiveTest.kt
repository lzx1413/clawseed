package dev.clawseed.demo.datatransfer

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

class PluginArchiveTest {
    @get:Rule val temp = TemporaryFolder()

    private fun plugin(root: File, dir: String, id: String, content: String) {
        val folder = root.resolve(dir).apply { mkdirs() }
        folder.resolve("manifest.json").writeText("""{"plugin_id":"$id"}""")
        folder.resolve("plugin.py").writeText(content)
    }

    @Test fun standaloneRestoreAndConflictsUseManifestIdsAndPreserveOtherPlugins() {
        val source = temp.newFolder("source")
        val srcConfig = temp.newFile("source.toml").apply { writeText("[plugins]\nenabled=true\ndenied=['sleeping']\n") }
        plugin(source, "custom-dir", "echo", "new echo")
        plugin(source, "sleeping", "sleeping", "new sleeping")
        assertTrue(source.resolve("custom-dir/plugin.py").setExecutable(true, true))
        val zip = temp.newFile("backup.zip")
        ZipOutputStream(zip.outputStream()).use { PluginArchive(source, srcConfig).export(it) }
        val extracted = temp.newFolder("extracted")
        ZipInputStream(zip.inputStream()).use { input ->
            while (true) {
                val entry = input.nextEntry ?: break
                val target = extracted.resolve(entry.name)
                if (entry.isDirectory) target.mkdirs() else {
                    requireNotNull(target.parentFile).mkdirs()
                    target.outputStream().use { input.copyTo(it) }
                }
            }
        }
        val dest = temp.newFolder("dest")
        plugin(dest, "other", "other", "untouched")
        val config = temp.newFile("dest.toml").apply { writeText("[plugins]\nenabled=false\nallowed=[\n 'other'\n]\n[agent]\nmax_iterations=3\n") }
        val archive = PluginArchive(dest, config)
        val packed = extracted.resolve("plugins")
        assertEquals(PluginArchive.Restored(2, 0), archive.restore(packed, false, archive.states()))
        assertEquals(mapOf("echo" to true, "sleeping" to false, "other" to false), archive.states())
        assertTrue(dest.resolve("echo/plugin.py").canExecute())
        assertTrue(config.readText().contains("max_iterations=3"))

        dest.resolve("echo").renameTo(dest.resolve("renamed"))
        dest.resolve("renamed/plugin.py").writeText("local")
        dest.resolve("renamed/obsolete").writeText("old file")
        assertEquals(PluginArchive.Restored(0, 2), archive.restore(packed, false, archive.states()))
        assertEquals("local", dest.resolve("renamed/plugin.py").readText())
        assertEquals(PluginArchive.Restored(2, 0), archive.restore(packed, true, archive.states()))
        assertEquals("new echo", dest.resolve("renamed/plugin.py").readText())
        assertFalse(dest.resolve("renamed/obsolete").exists())
        assertEquals("untouched", dest.resolve("other/plugin.py").readText())
    }

    @Test fun versionOneManifestBeforePluginsStillSelectsItsOriginalCategories() {
        val manifest = Json.decodeFromString<ExportManifest>(
            """{"version":1,"timestamp":1,"appVersion":"2","categories":["CONFIG","SKILLS"]}""")
        assertTrue(manifest.version <= ExportManifest.CURRENT_VERSION)
        assertEquals(setOf(DataCategory.CONFIG, DataCategory.SKILLS), manifest.categories.map(DataCategory::valueOf).toSet())
    }
}
