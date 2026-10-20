package dev.clawseed.demo.datatransfer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class WebDavAutoBackupTest {
    @Test
    fun normalizedClampsScheduleAndRetentionValues() {
        val normalized = WebDavAutoBackupSettings(
            retainCount = 500,
            hour = -2,
            minute = 99,
            weeklyDay = 12,
        ).normalized()

        assertEquals(100, normalized.retainCount)
        assertEquals(0, normalized.hour)
        assertEquals(59, normalized.minute)
        assertEquals(7, normalized.weeklyDay)
    }

    @Test
    fun generatedPathUsesNormalizedDirectoryAndCategoryName() {
        val path = webDavRemoteBackupPath(
            "/backups/",
            setOf(DataCategory.CONFIG),
            java.text.SimpleDateFormat("yyyyMMdd", java.util.Locale.US).parse("20261021")!!,
        )

        assertTrue(path.startsWith("backups/clawseed-20261021-config.zip"))
    }
}
