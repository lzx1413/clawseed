package dev.clawseed.demo.datatransfer

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.tomlj.Toml
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/** Plugin IDs are portable. Absolute source paths never enter the plugin archive. */
internal class PluginArchive(private val root: File, private val config: File) {
    private val idPattern = Regex("[a-z][a-z0-9_.-]{0,63}")

    private fun manifestId(dir: File): String {
        val manifest = dir.resolve("manifest.json")
        require(manifest.isFile && !Files.isSymbolicLink(manifest.toPath())) { "Missing plugin manifest in ${dir.name}" }
        val id = Json.parseToJsonElement(manifest.readText()).jsonObject.getValue("plugin_id").jsonPrimitive.content
        require(idPattern.matches(id)) { "Invalid plugin ID in ${dir.name}" }
        return id
    }

    private class Policy(text: String) {
        val toml: org.tomlj.TomlParseResult = Toml.parse(text).also { require(!it.hasErrors()) { it.errors().joinToString() } }
        val enabled: Boolean = toml.getBoolean("plugins.enabled") ?: false
        val allowed = strings("allowed").toMutableSet()
        val denied = strings("denied").toMutableSet()
        val extraRoots = strings("extra_roots").map(::File)
        private fun strings(key: String) = toml.getArrayOrEmpty("plugins.$key").toList().map { it as String }
        fun active(id: String) = enabled && (allowed.isEmpty() || id in allowed) && id !in denied
    }

    private fun inventory(policy: Policy): Map<String, File> = buildMap {
        for (parent in listOf(root) + policy.extraRoots) {
            if (!parent.exists()) continue
            val directories = requireNotNull(parent.listFiles()) { "Cannot read plugin directory $parent" }
            for (dir in directories.sortedBy { it.name }) {
                if (!dir.isDirectory || Files.isSymbolicLink(dir.toPath()) || !dir.resolve("manifest.json").exists()) continue
                putIfAbsent(manifestId(dir), dir)
            }
        }
    }

    fun states(): Map<String, Boolean> {
        val policy = Policy(config.readText())
        return inventory(policy).mapValues { policy.active(it.key) }
    }

    fun export(zip: ZipOutputStream) {
        val policy = Policy(config.readText())
        val plugins = inventory(policy)
        zip.putNextEntry(ZipEntry("plugins/state.json"))
        zip.write(Json.encodeToString(plugins.mapValues { policy.active(it.key) }).toByteArray())
        zip.closeEntry()
        val executables = mutableMapOf<String, MutableList<String>>()
        for ((id, dir) in plugins) {
            val tree = dir.walkTopDown().onEnter { !Files.isSymbolicLink(it.toPath()) }
                .onFail { _, error -> throw error }
            for (file in tree) {
                if (Files.isSymbolicLink(file.toPath())) continue
                val suffix = file.relativeTo(dir).invariantSeparatorsPath
                val path = "plugins/files/$id" + if (suffix.isEmpty()) "" else "/$suffix"
                if (file.isDirectory) zip.putNextEntry(ZipEntry("$path/"))
                else {
                    require(file.isFile) { "Unsupported plugin file: $suffix" }
                    if (file.canExecute()) executables.getOrPut(id) { mutableListOf() }.add(suffix)
                    zip.putNextEntry(ZipEntry(path))
                    file.inputStream().use { it.copyTo(zip) }
                }
                zip.closeEntry()
            }
        }
        zip.putNextEntry(ZipEntry("plugins/executables.json"))
        zip.write(Json.encodeToString(executables).toByteArray())
        zip.closeEntry()
    }

    data class Restored(val written: Int, val skipped: Int)

    fun restore(archive: File, replace: Boolean, previous: Map<String, Boolean>): Restored {
        val states = Json.decodeFromString<Map<String, Boolean>>(archive.resolve("state.json").readText())
        val executables = archive.resolve("executables.json").takeIf { it.exists() }?.let {
            Json.decodeFromString<Map<String, List<String>>>(it.readText())
        }.orEmpty()
        val original = config.readText()
        val policy = Policy(original)
        val existing = inventory(policy)
        val targets = mutableMapOf<String, File>()
        for (id in states.keys) {
            require(idPattern.matches(id) && manifestId(archive.resolve("files/$id")) == id) { "Plugin manifest mismatch: $id" }
            if (!replace && id in existing) continue
            // External plugins are restored to the managed root, which has lookup priority.
            val target = existing[id]?.takeIf { it.parentFile == root } ?: root.resolve(id)
            require(target.canonicalFile.parentFile == root.canonicalFile && !Files.isSymbolicLink(target.toPath()))
            require(!target.exists() || target == existing[id]) { "Plugin directory already in use: ${target.name}" }
            targets[id] = target
        }
        val desired = states.mapValues { (id, active) ->
            if (id in targets) active else previous[id] ?: policy.active(id)
        }
        val updated = mergePolicy(original, policy, existing.keys, desired)
        if (targets.isEmpty() && updated == original) return Restored(0, states.size)

        root.mkdirs()
        val staging = Files.createTempDirectory(requireNotNull(root.parentFile).toPath(), ".plugin-import-").toFile()
        val replaced = mutableMapOf<File, File>()
        val installed = mutableListOf<File>()
        try {
            for ((id, target) in targets) {
                val prepared = staging.resolve("new-$id")
                archive.resolve("files/$id").copyRecursively(prepared)
                for (relative in executables[id].orEmpty()) {
                    val file = prepared.resolve(relative).canonicalFile
                    require(file.toPath().startsWith(prepared.canonicalFile.toPath()) && file.isFile)
                    check(file.setExecutable(true, true)) { "Cannot restore executable: $relative" }
                }
                if (target.exists()) {
                    val old = staging.resolve("old-$id")
                    Files.move(target.toPath(), old.toPath(), StandardCopyOption.ATOMIC_MOVE)
                    replaced[target] = old
                }
                Files.move(prepared.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE)
                installed += target
            }
            if (updated != original) {
                val temp = File.createTempFile("plugins-", ".toml", config.parentFile)
                try {
                    temp.writeText(updated)
                    Files.move(temp.toPath(), config.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
                } finally { temp.delete() }
            }
        } catch (error: Exception) {
            installed.forEach { it.deleteRecursively() }
            replaced.forEach { (target, old) -> Files.move(old.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE) }
            throw error
        } finally { staging.deleteRecursively() }
        return Restored(targets.size, states.size - targets.size)
    }

    private fun mergePolicy(text: String, policy: Policy, existing: Set<String>, states: Map<String, Boolean>): String {
        if (states.all { (id, enabled) -> policy.active(id) == enabled }) return text
        if (policy.toml.contains("plugins") && text.lineSequence().none { it.substringBefore('#').trim() == "[plugins]" }) {
            val normalized = policy.toml.toToml()
            require(normalized != text) { "Unsupported plugins table format" }
            return mergePolicy(normalized, Policy(normalized), existing, states)
        }
        val enabled = policy.enabled || states.values.any { it }
        // Enabling the subsystem for one restored plugin must preserve the local plugins' off state.
        val desired = existing.associateWith { policy.active(it) } + states
        for ((id, active) in desired) {
            if (active) {
                policy.denied.remove(id)
                if (policy.allowed.isNotEmpty()) policy.allowed.add(id)
            } else policy.denied.add(id)
        }
        val values = mapOf("enabled" to enabled.toString(), "allowed" to Json.encodeToString(policy.allowed.sorted()),
            "denied" to Json.encodeToString(policy.denied.sorted()))
        // Use parser locations to remove complete assignments, including multiline arrays.
        val lines = text.lines().toMutableList()
        val positions = values.keys.mapNotNull { policy.toml.inputPositionOf("plugins.$it")?.line() }.sortedDescending()
        for (line in positions) {
            val start = line - 1
            var end = start + 1
            while (end < lines.size && Toml.parse(lines.subList(start, end).joinToString("\n")).hasErrors()) end++
            lines.subList(start, end).clear()
        }
        val header = lines.indexOfFirst { it.substringBefore('#').trim() == "[plugins]" }
        if (header < 0) lines.add("[plugins]")
        lines.addAll(if (header < 0) lines.size else header + 1, values.map { (key, value) -> "$key = $value" })
        val updated = lines.joinToString("\n")
        val checked = Policy(updated)
        require(desired.all { (id, active) -> checked.active(id) == active }) { "Could not restore plugin settings" }
        return updated
    }
}
