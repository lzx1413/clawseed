package dev.clawseed.demo.ui.chat

import android.database.sqlite.SQLiteDatabase
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.security.MessageDigest

/** Can run before upgrading the target APK: depends only on platform APIs. */
class AttachmentUpgradeBaselineTest {
    @Test fun recordOrVerifyExistingData() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val before = InstrumentationRegistry.getArguments().getString("phase") == "before"
        val snapshot = JSONObject()
        val root = context.filesDir
        val database = root.walkTopDown().firstOrNull { it.name == "sessions.db" }
        assertNotNull("Existing session database must be present", database)
        SQLiteDatabase.openDatabase(database!!.path, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            for ((table, columns, order) in listOf(
                Triple("sessions", "session_key, name, user_id, created_at", "session_key"),
                Triple("messages", "id, session_key, role, content", "id"),
                Triple("session_personas", "session_key, persona", "session_key"),
            )) {
                db.rawQuery("SELECT $columns FROM $table ORDER BY $order", null).use { cursor ->
                    val rows = JSONObject()
                    while (cursor.moveToNext()) {
                        val values = (0 until cursor.columnCount).map { if (cursor.isNull(it)) "<NULL>" else cursor.getString(it) }
                        rows.put(values.first(), digest(values.joinToString("\u0000").toByteArray()))
                    }
                    snapshot.put(table, rows)
                }
            }
        }
        val configs = JSONObject()
        root.walkTopDown().filter { it.isFile && (it.extension == "toml" || "personas" in it.relativeTo(root).invariantSeparatorsPath.split('/')) }.forEach {
            configs.put(it.relativeTo(root).invariantSeparatorsPath, digest(it.readBytes()))
        }
        snapshot.put("config_files", configs)
        val baseline = File(context.cacheDir, "file-attachment-upgrade-baseline.json")
        if (before) baseline.writeText(snapshot.toString())
        else {
            assertTrue("Capture baseline before upgrading", baseline.isFile)
            val original = JSONObject(baseline.readText())
            for (category in original.keys()) {
                val expected = original.getJSONObject(category)
                val actual = snapshot.getJSONObject(category)
                for (key in expected.keys()) assertEquals("Preserve $category/$key", expected.getString(key), actual.optString(key))
            }
        }
        val summary = android.os.Bundle().apply {
            putString("stream", "\n${if (before) "Recorded" else "Verified"} baseline: ${snapshot.getJSONObject("sessions").length()} sessions, ${snapshot.getJSONObject("messages").length()} messages, ${snapshot.getJSONObject("session_personas").length()} persona bindings, ${configs.length()} config/persona files.\n")
        }
        InstrumentationRegistry.getInstrumentation().sendStatus(0, summary)
    }
    private fun digest(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}
