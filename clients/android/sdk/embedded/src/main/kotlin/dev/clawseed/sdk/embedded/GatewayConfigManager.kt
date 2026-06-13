package dev.clawseed.sdk.embedded

import android.content.Context
import android.util.Log
import java.io.File

/** Ensures the embedded gateway has a usable config directory and TOML file. */
class GatewayConfigManager(private val context: Context) {

    /** Creates or patches the gateway config file and returns its path. */
    fun ensureConfig(): File {
        val configDir = File(context.filesDir, ".clawseed")
        configDir.mkdirs()

        val workspaceDir = File(configDir, "workspace")
        if (!workspaceDir.exists()) {
            workspaceDir.mkdirs()
            Log.i(TAG, "Created workspace directory: ${workspaceDir.absolutePath}")
        }

        val configFile = File(configDir, "clawseed.toml")
        if (configFile.exists()) {
            var content = configFile.readText()
            var changed = false

            if (!content.contains("workspace_dir")) {
                content = "workspace_dir = \"${workspaceDir.absolutePath}\"\n$content"
                changed = true
                Log.i(TAG, "Added workspace_dir to config")
            }

            for ((section, patch) in WEB_FEATURE_PATCHES) {
                val patched = enableSectionIfPresent(content, section, patch)
                if (patched != content) {
                    content = patched
                    changed = true
                }
            }

            for ((header, body) in REQUIRED_SECTIONS) {
                if (!content.contains(header)) {
                    content = content.trimEnd() + "\n\n$header\n$body\n"
                    changed = true
                    Log.i(TAG, "Added missing section: $header")
                }
            }

            // Ensure web_search has provider set
            val wsIdx = content.indexOf("[web_search]")
            if (wsIdx != -1) {
                val nextSectionIdx = content.indexOf("\n[", wsIdx + 1).let { if (it == -1) content.length else it }
                val wsSection = content.substring(wsIdx, nextSectionIdx)
                if (!wsSection.contains("provider")) {
                    content = content.substring(0, wsIdx) +
                        wsSection.replace("[web_search]\n", "[web_search]\nprovider = \"bing\"\n") +
                        content.substring(nextSectionIdx)
                    changed = true
                    Log.i(TAG, "Added provider = bing to [web_search]")
                }
            }

            // Ensure [gateway] has embedded defaults
            val gwIdx = content.indexOf("[gateway]")
            if (gwIdx != -1) {
                val nextGwSection = content.indexOf("\n[", gwIdx + 1).let { if (it == -1) content.length else it }
                val gwSection = content.substring(gwIdx, nextGwSection)
                var gwPatch = ""
                if (!gwSection.contains("require_pairing")) {
                    gwPatch += "\nrequire_pairing = false"
                }
                // Default TTL=0 for embedded use (never auto-delete sessions).
                // Only add if the key is absent; if the user set a value via Settings UI, leave it.
                if (!gwSection.contains("session_ttl_hours")) {
                    gwPatch += "\nsession_ttl_hours = 0"
                }
                if (gwPatch.isNotEmpty()) {
                    content = content.substring(0, nextGwSection) +
                        gwPatch +
                        content.substring(nextGwSection)
                    changed = true
                    Log.i(TAG, "Patched [gateway] section: $gwPatch")
                }
            }

            if (changed) configFile.writeText(content)
        } else {
            configFile.writeText(INITIAL_CONFIG.replace("{WORKSPACE_DIR}", workspaceDir.absolutePath))
            Log.i(TAG, "Created initial config")
        }

        return configFile
    }

    /** Returns the root `.clawseed` config directory inside app storage. */
    fun configDir(): File = File(context.filesDir, ".clawseed")

    /** Returns the workspace directory exposed to file tools. */
    fun workspaceDir(): File = File(configDir(), "workspace")

    /** Copies bundled embedding model files from APK assets to the workspace model directory.
     *  Only copies if the target files don't already exist, so this is safe to call on every startup.
     *  Copies ALL files found in the assets model directory, not just a hardcoded list. */
    fun ensureBundledModelFiles() {
        val modelName = "gte-multilingual-base"
        val modelDir = File(workspaceDir(), "models/$modelName")
        modelDir.mkdirs()

        val assetBase = "models/$modelName"
        val assetFiles = try {
            context.assets.list(assetBase)?.filterNotNull() ?: emptyList()
        } catch (_: Exception) { emptyList() }

        for (filename in assetFiles) {
            val target = File(modelDir, filename)
            if (target.exists() && target.length() > 0) continue

            val assetPath = "$assetBase/$filename"
            try {
                context.assets.open(assetPath).use { input ->
                    target.outputStream().use { output ->
                        input.copyTo(output)
                    }
                }
                Log.i(TAG, "Copied bundled model file: $assetPath → ${target.absolutePath} (${target.length()} bytes)")
            } catch (e: Exception) {
                Log.w(TAG, "Bundled model file $assetPath not found in assets — will be downloaded at runtime")
            }
        }
    }

    /** Copies bundled skills from APK assets to the workspace skills directory.
     *  Only copies if the target skill directory doesn't already exist, so user-modified
     *  skills are never overwritten. Scans all skill directories found under assets/skills/.
     *  This follows the same pattern as ensureBundledModelFiles(). */
    fun ensureBundledSkills() {
        val skillsDir = File(workspaceDir(), ".clawseed/skills")
        skillsDir.mkdirs()

        val assetBase = "skills"
        val skillNames = try {
            context.assets.list(assetBase)?.filterNotNull() ?: emptyList()
        } catch (_: Exception) { emptyList() }

        for (skillName in skillNames) {
            val targetDir = File(skillsDir, skillName)
            if (targetDir.exists()) continue // Don't overwrite user-modified skills

            val skillAssetBase = "$assetBase/$skillName"
            val skillFiles = try {
                context.assets.list(skillAssetBase)?.filterNotNull() ?: emptyList()
            } catch (_: Exception) { emptyList() }

            targetDir.mkdirs()
            for (filename in skillFiles) {
                val assetPath = "$skillAssetBase/$filename"
                try {
                    context.assets.open(assetPath).use { input ->
                        File(targetDir, filename).outputStream().use { output ->
                            input.copyTo(output)
                        }
                    }
                    Log.i(TAG, "Copied bundled skill file: $assetPath → ${File(targetDir, filename).absolutePath}")
                } catch (e: Exception) {
                    Log.w(TAG, "Failed to copy bundled skill file: $assetPath", e)
                }
            }
            Log.i(TAG, "Seeded bundled skill: $skillName")
        }
    }

    /** Ensures skills directory exists under the workspace and migrates any skills
     *  from the legacy location ({configDir}/skills/) to the workspace-level path
     *  ({workspaceDir}/.clawseed/skills/) so the agent can manage skills via file tools. */
    fun ensureSkillsDir() {
        val targetDir = File(workspaceDir(), ".clawseed/skills")
        if (!targetDir.exists()) {
            targetDir.mkdirs()
            Log.i(TAG, "Created workspace skills directory: ${targetDir.absolutePath}")
        }

        // Migrate skills from legacy location ({configDir}/skills/) if it exists.
        // Uses copy+delete instead of renameTo() because cross-filesystem renames fail on Android.
        val legacyDir = File(configDir(), "skills")
        if (legacyDir.exists() && legacyDir.isDirectory) {
            val legacySkills = legacyDir.listFiles()?.filter { it.isDirectory } ?: emptyList()
            for (skillDir in legacySkills) {
                val dest = File(targetDir, skillDir.name)
                if (dest.exists()) continue // Don't overwrite workspace-level skills
                try {
                    skillDir.copyRecursively(dest)
                    skillDir.deleteRecursively()
                    Log.i(TAG, "Migrated skill: ${skillDir.name} → ${dest.absolutePath}")
                } catch (e: Exception) {
                    Log.w(TAG, "Failed to migrate skill: ${skillDir.name}", e)
                }
            }
            // Remove empty legacy directory
            val remaining = legacyDir.listFiles()
            if (remaining == null || remaining.isEmpty()) {
                legacyDir.delete()
                Log.i(TAG, "Removed empty legacy skills directory")
            }
        }
    }

    private fun enableSectionIfPresent(content: String, sectionHeader: String, patch: Pair<String, String>): String {
        val sectionIdx = content.indexOf("\n$sectionHeader\n")
        if (sectionIdx == -1) return content
        val nextSection = content.indexOf("\n[", sectionIdx + 1).let { if (it == -1) content.length else it }
        val before = content.substring(0, sectionIdx)
        var section = content.substring(sectionIdx, nextSection)
        val after = content.substring(nextSection)
        if (section.contains(patch.first)) {
            section = section.replace(patch.first, patch.second)
            Log.i(TAG, "Patched config: $sectionHeader ${patch.second}")
        }
        if (sectionHeader in listOf("[http_request]", "[web_fetch]") && !section.contains("allowed_domains")) {
            section = section.trimEnd() + "\nallowed_domains = [\"*\"]\n"
            Log.i(TAG, "Added allowed_domains to $sectionHeader")
        }
        return before + section + after
    }

    companion object {
        private const val TAG = "GatewayConfigManager"

        private val WEB_FEATURE_PATCHES = listOf(
            "[web_fetch]"    to ("enabled = false" to "enabled = true"),
            "[http_request]" to ("enabled = false" to "enabled = true"),
            "[web_search]"   to ("enabled = false" to "enabled = true"),
        )

        private val REQUIRED_SECTIONS = listOf(
            "[gateway]" to "session_persistence = true\nsession_ttl_hours = 0\nrequire_pairing = false",
            "[web_fetch]" to "enabled = true\nallowed_domains = [\"*\"]",
            "[http_request]" to "enabled = true\nallowed_domains = [\"*\"]",
            "[web_search]" to "enabled = true\nprovider = \"bing\"",
            "[memory]" to """embedding_provider = "local"
embedding_model = "gte-multilingual-base"
merge_strategy = "rrf"
defer_embedding = true
stable_memory_in_system_prompt = true
conflict_mode = "combined"
min_retention_floor = 30
backfill_on_startup = true""",
        )

        private val INITIAL_CONFIG = """
workspace_dir = "{WORKSPACE_DIR}"

[gateway]
session_persistence = true
session_ttl_hours = 0
require_pairing = false

[memory]
embedding_provider = "local"
embedding_model = "gte-multilingual-base"
merge_strategy = "rrf"
defer_embedding = true
stable_memory_in_system_prompt = true
conflict_mode = "combined"
min_retention_floor = 30
backfill_on_startup = true

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "bing"
""".trimIndent() + "\n"
    }
}
