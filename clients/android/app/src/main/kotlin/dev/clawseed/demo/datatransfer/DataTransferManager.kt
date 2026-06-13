package dev.clawseed.demo.datatransfer

import android.content.Context
import android.util.Log
import dev.clawseed.demo.BuildConfig
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.embedded.GatewayConfigManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.BufferedInputStream
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

/** Core orchestrator for data export/import operations. */
class DataTransferManager(private val context: Context) {

    private val configManager = GatewayConfigManager(context)
    private val localStore = LocalStore(context)
    private val taskStore = ScheduledTaskStore(context)
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }

    // --- Export ----------------------------------------------------------

    suspend fun exportData(
        categories: Set<DataCategory>,
        excludeSensitive: Boolean,
        outputStream: OutputStream,
    ): Result<Unit> = withContext(Dispatchers.IO) {
        try {
            val zipOut = ZipOutputStream(outputStream)

            // Write manifest
            val manifest = ExportManifest(
                timestamp = System.currentTimeMillis(),
                appVersion = BuildConfig.VERSION_NAME,
                categories = categories.map { it.name },
                excludeSensitive = excludeSensitive,
            )
            zipOut.putNextEntry(ZipEntry("manifest.json"))
            zipOut.write(json.encodeToString(ExportManifest.serializer(), manifest).toByteArray())
            zipOut.closeEntry()

            // Export each category
            val prefs = localStore.exportAllPreferences()
            val tasks = taskStore.tasksAsList()

            for (category in categories) {
                when (category) {
                    DataCategory.CONFIG -> exportConfig(zipOut, excludeSensitive, prefs, tasks)
                    DataCategory.MEMORY -> exportMemory(zipOut)
                    DataCategory.SESSIONS -> exportSessions(zipOut)
                    DataCategory.SKILLS -> exportSkills(zipOut)
                    DataCategory.PERSONALITY -> exportPersonality(zipOut)
                }
            }

            zipOut.finish()
            Log.i(TAG, "Export completed: ${categories.map { it.name }}")
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    private fun exportConfig(
        zipOut: ZipOutputStream,
        excludeSensitive: Boolean,
        prefs: Map<String, Map<String, Any?>>,
        tasks: List<ScheduledTask>,
    ) {
        // clawseed.toml
        val configFile = configManager.configFile()
        if (configFile.exists()) {
            var tomlContent = configFile.readText()
            if (excludeSensitive) {
                tomlContent = stripSensitiveToml(tomlContent)
            }
            zipOut.putNextEntry(ZipEntry("config/clawseed.toml"))
            zipOut.write(tomlContent.toByteArray())
            zipOut.closeEntry()
        }

        // preferences.json — serialize as JSON string map manually since Map<String, Any?> isn't @Serializable
        val prefsToExport = if (excludeSensitive) {
            prefs.filterKeys { it !in localStore.sensitiveKeys() }
        } else {
            prefs
        }
        val prefsJson = serializePreferencesMap(prefsToExport)
        zipOut.putNextEntry(ZipEntry("config/preferences.json"))
        zipOut.write(prefsJson.toByteArray())
        zipOut.closeEntry()

        // scheduled_tasks.json
        zipOut.putNextEntry(ZipEntry("config/scheduled_tasks.json"))
        zipOut.write(json.encodeToString(tasks).toByteArray())
        zipOut.closeEntry()
    }

    private fun exportMemory(zipOut: ZipOutputStream) {
        val dbFile = configManager.memoryDbFile()
        if (dbFile.exists()) {
            configManager.checkpointWal(dbFile)
            addFileToZip(zipOut, dbFile, "memory/brain.db")
        }
        val snapshot = File(configManager.workspaceDir(), "MEMORY_SNAPSHOT.md")
        if (snapshot.exists()) {
            addFileToZip(zipOut, snapshot, "memory/MEMORY_SNAPSHOT.md")
        }
    }

    private fun exportSessions(zipOut: ZipOutputStream) {
        val dbFile = configManager.sessionsDbFile()
        if (dbFile.exists()) {
            configManager.checkpointWal(dbFile)
            addFileToZip(zipOut, dbFile, "sessions/sessions.db")
        }
    }

    private fun exportSkills(zipOut: ZipOutputStream) {
        val skillsDir = configManager.skillsDir()
        if (!skillsDir.exists()) return
        for (skillDir in skillsDir.listFiles()?.filter { it.isDirectory } ?: emptyList()) {
            addDirectoryToZip(zipOut, skillDir, "skills/${skillDir.name}")
        }
    }

    private fun exportPersonality(zipOut: ZipOutputStream) {
        val personalityDir = configManager.personalityDir()
        if (!personalityDir.exists()) return
        for (file in personalityDir.listFiles()?.filter { it.isFile } ?: emptyList()) {
            addFileToZip(zipOut, file, "personality/${file.name}")
        }
        // Also check workspace root for SOUL.md (fallback location)
        val workspaceDir = configManager.workspaceDir()
        val PERSONALITY_FILES = listOf("SOUL.md", "IDENTITY.md", "USER.md", "AGENTS.md", "TOOLS.md", "HEARTBEAT.md", "BOOTSTRAP.md", "MEMORY.md")
        for (name in PERSONALITY_FILES) {
            val rootFile = File(workspaceDir, name)
            if (rootFile.exists() && !File(personalityDir, name).exists()) {
                addFileToZip(zipOut, rootFile, "personality/${name}")
            }
        }
    }

    // --- Import ---------------------------------------------------------

    suspend fun importData(
        inputStream: InputStream,
        categories: Set<DataCategory>,
        strategies: Map<DataCategory, ImportStrategy>,
    ): Result<ImportResult> = withContext(Dispatchers.IO) {
        try {
            val tempDir = File(context.cacheDir, "clawseed-import")
            if (tempDir.exists()) tempDir.deleteRecursively()
            tempDir.mkdirs()

            // Extract ZIP to temp directory
            val zipIn = ZipInputStream(inputStream)
            var entry = zipIn.nextEntry
            var manifest: ExportManifest? = null

            while (entry != null) {
                val targetFile = File(tempDir, entry.name)
                if (entry.isDirectory) {
                    targetFile.mkdirs()
                } else {
                    targetFile.parentFile?.mkdirs()
                    FileOutputStream(targetFile).use { out ->
                        zipIn.copyTo(out)
                    }
                }
                if (entry.name == "manifest.json") {
                    manifest = json.decodeFromString<ExportManifest>(targetFile.readText())
                }
                entry = zipIn.nextEntry
            }
            zipIn.close()

            if (manifest == null) {
                tempDir.deleteRecursively()
                throw IllegalArgumentException("导出文件缺少 manifest.json")
            }
            if (manifest.version > ExportManifest.CURRENT_VERSION) {
                tempDir.deleteRecursively()
                throw IllegalArgumentException("导出文件格式版本不兼容 (v${manifest.version} > v${ExportManifest.CURRENT_VERSION})")
            }

            // Check which categories are available in the ZIP
            val availableInZip = manifest.categories.mapNotNull { name ->
                DataCategory.entries.find { it.name == name }
            }.toSet()

            val warnings = mutableListOf<String>()
            val errors = mutableListOf<String>()
            var importedSkills = 0
            var importedMemories = 0
            var importedSessions = 0
            var importedMessages = 0
            var importedPersonalityFiles = 0
            var importedConfig = false
            var importedTasks = 0

            for (category in categories) {
                if (category !in availableInZip) {
                    warnings.add("导出文件中不包含 ${category.label} 数据")
                    continue
                }
                try {
                    when (category) {
                        DataCategory.CONFIG -> {
                            importedConfig = importConfig(tempDir)
                            importedTasks = importScheduledTasks(tempDir)
                        }
                        DataCategory.MEMORY -> {
                            val strategy = strategies[category] ?: ImportStrategy.MERGE
                            importedMemories = importMemory(tempDir, strategy)
                        }
                        DataCategory.SESSIONS -> {
                            val strategy = strategies[category] ?: ImportStrategy.APPEND
                            val result = importSessions(tempDir, strategy)
                            importedSessions = result.first
                            importedMessages = result.second
                        }
                        DataCategory.SKILLS -> {
                            val strategy = strategies[category] ?: ImportStrategy.MERGE
                            importedSkills = importSkills(tempDir, strategy)
                        }
                        DataCategory.PERSONALITY -> {
                            importedPersonalityFiles = importPersonality(tempDir)
                        }
                    }
                } catch (e: Exception) {
                    errors.add("${category.label} 导入失败: ${e.message}")
                    Log.e(TAG, "Import failed for ${category.name}", e)
                }
            }

            // Restart gateway if config or personality was imported
            if (importedConfig || importedPersonalityFiles > 0) {
                try {
                    ClawSeedAndroid.restartGateway()
                } catch (e: Exception) {
                    warnings.add("Gateway 重启失败，请手动重启应用")
                }
            }

            // Cleanup
            tempDir.deleteRecursively()

            Result.success(ImportResult(
                importedSkills = importedSkills,
                importedMemories = importedMemories,
                importedSessions = importedSessions,
                importedMessages = importedMessages,
                importedPersonalityFiles = importedPersonalityFiles,
                importedConfig = importedConfig,
                importedTasks = importedTasks,
                warnings = warnings,
                errors = errors,
            ))
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    private suspend fun importConfig(tempDir: File): Boolean {
        val importedToml = File(tempDir, "config/clawseed.toml")
        if (!importedToml.exists()) return false

        val configFile = configManager.configFile()
        importedToml.copyTo(configFile, overwrite = true)

        val importedPrefs = File(tempDir, "config/preferences.json")
        if (importedPrefs.exists()) {
            val prefsData = deserializePreferencesMap(importedPrefs.readText())
            localStore.importPreferences(prefsData)
        }

        return true
    }

    private suspend fun importScheduledTasks(tempDir: File): Int {
        val importedTasksFile = File(tempDir, "config/scheduled_tasks.json")
        if (!importedTasksFile.exists()) return 0
        val tasks = json.decodeFromString<List<ScheduledTask>>(importedTasksFile.readText())
        if (tasks.isNotEmpty()) {
            taskStore.importTasks(tasks)
        }
        return tasks.size
    }

    private fun importMemory(tempDir: File, strategy: ImportStrategy): Int {
        val importedDb = File(tempDir, "memory/brain.db")
        if (!importedDb.exists()) return 0

        val liveDb = configManager.memoryDbFile()

        if (strategy == ImportStrategy.REPLACE) {
            // Replace: overwrite the live DB file
            if (liveDb.exists()) {
                liveDb.delete()
                // Also delete WAL/SHM files
                File(liveDb.absolutePath + "-wal").delete()
                File(liveDb.absolutePath + "-shm").delete()
            }
            liveDb.parentFile?.mkdirs()
            importedDb.copyTo(liveDb)
            return countMemories(liveDb)
        }

        // MERGE: insert new memories, keep existing
        if (!liveDb.exists()) {
            // No local DB — just copy the imported one
            liveDb.parentFile?.mkdirs()
            importedDb.copyTo(liveDb)
            return countMemories(liveDb)
        }

        return mergeMemories(importedDb, liveDb)
    }

    private fun mergeMemories(importedDb: File, liveDb: File): Int {
        val importedConn = android.database.sqlite.SQLiteDatabase.openDatabase(
            importedDb.absolutePath, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
        )
        val liveConn = android.database.sqlite.SQLiteDatabase.openDatabase(
            liveDb.absolutePath, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READWRITE,
        )

        var inserted = 0
        liveConn.beginTransaction()
        try {
            val cursor = importedConn.rawQuery(
                "SELECT id, key, content, category, created_at, updated_at, session_id, namespace, importance FROM memories",
                null,
            )
            cursor.use {
                while (it.moveToNext()) {
                    val key = it.getString(1)
                    // Check if key already exists
                    val exists = liveConn.rawQuery("SELECT 1 FROM memories WHERE key = ?", arrayOf(key)).use { c ->
                        c.moveToFirst()
                    }
                    if (!exists) {
                        liveConn.execSQL(
                            "INSERT INTO memories (id, key, content, category, created_at, updated_at, session_id, namespace, importance) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                            arrayOf(
                                it.getString(0), key, it.getString(2), it.getString(3),
                                it.getString(4), it.getString(5), it.getString(6),
                                it.getString(7), it.getString(8),
                            ),
                        )
                        inserted++
                    }
                }
            }
            liveConn.setTransactionSuccessful()
        } finally {
            liveConn.endTransaction()
            importedConn.close()
            liveConn.close()
        }
        return inserted
    }

    private fun countMemories(dbFile: File): Int {
        if (!dbFile.exists()) return 0
        val db = android.database.sqlite.SQLiteDatabase.openDatabase(
            dbFile.absolutePath, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
        )
        val count = db.rawQuery("SELECT COUNT(*) FROM memories", null).use { c ->
            if (c.moveToFirst()) c.getInt(0) else 0
        }
        db.close()
        return count
    }

    private fun importSessions(tempDir: File, strategy: ImportStrategy): Pair<Int, Int> {
        val importedDb = File(tempDir, "sessions/sessions.db")
        if (!importedDb.exists()) return Pair(0, 0)

        val liveDb = configManager.sessionsDbFile()

        if (strategy == ImportStrategy.REPLACE) {
            if (liveDb.exists()) {
                liveDb.delete()
                File(liveDb.absolutePath + "-wal").delete()
                File(liveDb.absolutePath + "-shm").delete()
            }
            liveDb.parentFile?.mkdirs()
            importedDb.copyTo(liveDb)
            return countSessions(liveDb)
        }

        // APPEND: insert new sessions + their messages
        if (!liveDb.exists()) {
            liveDb.parentFile?.mkdirs()
            importedDb.copyTo(liveDb)
            return countSessions(liveDb)
        }

        return appendSessions(importedDb, liveDb)
    }

    private fun appendSessions(importedDb: File, liveDb: File): Pair<Int, Int> {
        val importedConn = android.database.sqlite.SQLiteDatabase.openDatabase(
            importedDb.absolutePath, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
        )
        val liveConn = android.database.sqlite.SQLiteDatabase.openDatabase(
            liveDb.absolutePath, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READWRITE,
        )

        var insertedSessions = 0
        var insertedMessages = 0
        liveConn.beginTransaction()
        try {
            // Insert sessions (skip existing by session_key)
            val sessionCursor = importedConn.rawQuery(
                "SELECT session_key, name, state, turn_id, turn_started_at, created_at, last_activity FROM sessions",
                null,
            )
            sessionCursor.use {
                while (it.moveToNext()) {
                    val sessionKey = it.getString(0)
                    val exists = liveConn.rawQuery("SELECT 1 FROM sessions WHERE session_key = ?", arrayOf(sessionKey)).use { c ->
                        c.moveToFirst()
                    }
                    if (!exists) {
                        liveConn.execSQL(
                            "INSERT INTO sessions (session_key, name, state, turn_id, turn_started_at, created_at, last_activity) VALUES (?, ?, ?, ?, ?, ?, ?)",
                            arrayOf(
                                sessionKey, it.getString(1), it.getString(2), it.getString(3),
                                it.getString(4), it.getString(5), it.getString(6),
                            ),
                        )
                        insertedSessions++
                    }
                }
            }

            // Insert messages for sessions that exist in both DBs
            val msgCursor = importedConn.rawQuery(
                "SELECT session_key, role, content, created_at FROM messages",
                null,
            )
            msgCursor.use {
                while (it.moveToNext()) {
                    val sessionKey = it.getString(0)
                    // Only insert messages for sessions that are now present in the live DB
                    val sessionExists = liveConn.rawQuery("SELECT 1 FROM sessions WHERE session_key = ?", arrayOf(sessionKey)).use { c ->
                        c.moveToFirst()
                    }
                    if (sessionExists) {
                        liveConn.execSQL(
                            "INSERT INTO messages (session_key, role, content, created_at) VALUES (?, ?, ?, ?)",
                            arrayOf(sessionKey, it.getString(1), it.getString(2), it.getString(3)),
                        )
                        insertedMessages++
                    }
                }
            }
            liveConn.setTransactionSuccessful()
        } finally {
            liveConn.endTransaction()
            importedConn.close()
            liveConn.close()
        }
        return Pair(insertedSessions, insertedMessages)
    }

    private fun countSessions(dbFile: File): Pair<Int, Int> {
        if (!dbFile.exists()) return Pair(0, 0)
        val db = android.database.sqlite.SQLiteDatabase.openDatabase(
            dbFile.absolutePath, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
        )
        val sessions = db.rawQuery("SELECT COUNT(*) FROM sessions", null).use { c ->
            if (c.moveToFirst()) c.getInt(0) else 0
        }
        val messages = db.rawQuery("SELECT COUNT(*) FROM messages", null).use { c ->
            if (c.moveToFirst()) c.getInt(0) else 0
        }
        db.close()
        return Pair(sessions, messages)
    }

    private suspend fun importSkills(tempDir: File, strategy: ImportStrategy): Int {
        val importedSkillsDir = File(tempDir, "skills")
        if (!importedSkillsDir.exists()) return 0

        val localSkillsDir = configManager.skillsDir()

        if (strategy == ImportStrategy.REPLACE) {
            // Delete all local skills
            if (localSkillsDir.exists()) {
                localSkillsDir.deleteRecursively()
            }
            localSkillsDir.mkdirs()
        } else {
            localSkillsDir.mkdirs()
        }

        var count = 0
        for (skillDir in importedSkillsDir.listFiles()?.filter { it.isDirectory } ?: emptyList()) {
            val target = File(localSkillsDir, skillDir.name)
            skillDir.copyRecursively(target, overwrite = true)
            count++
        }

        // Reload skills in gateway if running
        try {
            if (ClawSeedAndroid.isInitialized) {
                ClawSeedAndroid.gatewayClient().reloadSkills()
            }
        } catch (_: Exception) {
            // Gateway may not be running, ignore
        }

        return count
    }

    private suspend fun importPersonality(tempDir: File): Int {
        val importedDir = File(tempDir, "personality")
        if (!importedDir.exists()) return 0

        val localDir = configManager.personalityDir()
        localDir.mkdirs()

        val files = importedDir.listFiles()?.filter { it.isFile } ?: emptyList()
        for (file in files) {
            val target = File(localDir, file.name)
            file.copyTo(target, overwrite = true)
        }

        // If gateway is running, try to update via API for immediate effect
        try {
            if (ClawSeedAndroid.isInitialized) {
                val client = ClawSeedAndroid.gatewayClient()
                val filesMap = files.associate { it.name to it.readText() }
                client.updatePersonality(filesMap)
            }
        } catch (_: Exception) {
            // Gateway may not be running, files written directly will work on restart
        }

        return files.size
    }

    // --- Helpers --------------------------------------------------------

    private fun addFileToZip(zipOut: ZipOutputStream, file: File, entryName: String) {
        zipOut.putNextEntry(ZipEntry(entryName))
        BufferedInputStream(FileInputStream(file)).use { input ->
            input.copyTo(zipOut, bufferSize = 8192)
        }
        zipOut.closeEntry()
    }

    private fun addDirectoryToZip(zipOut: ZipOutputStream, dir: File, prefix: String) {
        val files = dir.listFiles()
        if (files != null) {
            for (file in files) {
                if (file.isDirectory) {
                    addDirectoryToZip(zipOut, file, "$prefix/${file.name}")
                } else {
                    addFileToZip(zipOut, file, "$prefix/${file.name}")
                }
            }
        }
    }

    /** Strips API keys and sensitive values from TOML content. */
    private fun stripSensitiveToml(toml: String): String {
        return toml.lines().map { line ->
            val trimmed = line.trimStart()
            if (trimmed.startsWith("api_key") || trimmed.startsWith("tavily_api_key")) {
                val key = line.substringBefore("=").trim()
                "$key = \"***\""
            } else line
        }.joinToString("\n")
    }

    /** Serializes a preferences map with type discriminators to JSON. */
    private fun serializePreferencesMap(prefs: Map<String, Map<String, Any?>>): String {
        val sb = StringBuilder("{\n")
        for ((key, entry) in prefs) {
            val type = entry["type"] as? String ?: "string"
            val value = entry["value"]
            sb.append("  \"$key\": {\"type\": \"$type\", \"value\": ")
            when (type) {
                "string" -> sb.append("\"${escapeJsonString(value as? String ?: "")}\"")
                "boolean" -> sb.append(value as? Boolean ?: false)
                "long", "int" -> sb.append(value as? Number ?: 0)
                "float" -> sb.append(value as? Number ?: 0.0)
                else -> sb.append("\"${escapeJsonString(value?.toString() ?: "")}\"")
            }
            sb.append("},\n")
        }
        sb.append("}")
        return sb.toString()
    }

    /** Deserializes a preferences map from JSON. */
    private fun deserializePreferencesMap(jsonStr: String): Map<String, Map<String, Any?>> {
        val result = mutableMapOf<String, Map<String, Any?>>()
        val obj = org.json.JSONObject(jsonStr)
        for (key in obj.keys()) {
            val entry = obj.getJSONObject(key)
            val type = entry.getString("type")
            val value = when (type) {
                "string" -> entry.getString("value")
                "boolean" -> entry.getBoolean("value")
                "long" -> entry.getLong("value")
                "int" -> entry.getInt("value")
                "float" -> entry.getDouble("value")
                else -> entry.getString("value")
            }
            result[key] = mapOf("type" to type, "value" to value)
        }
        return result
    }

    private fun escapeJsonString(s: String): String {
        return s.replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\t", "\\t")
    }

    companion object {
        private const val TAG = "DataTransferManager"
    }
}
