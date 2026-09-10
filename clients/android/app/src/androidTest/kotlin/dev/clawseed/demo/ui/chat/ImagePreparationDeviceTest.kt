package dev.clawseed.demo.ui.chat

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.File

class ImagePreparationDeviceTest {
    @Test fun largePhotoHasDurablePreviewBeforeEncodingAndUsesJpeg() = runBlocking {
        val target = InstrumentationRegistry.getInstrumentation().targetContext
        val directory = File(target.cacheDir, "image-preparation-${System.nanoTime()}").apply { mkdirs() }
        val preferences = "image-preparation-${System.nanoTime()}"
        val context = object : ContextWrapper(target) {
            override fun getFilesDir() = directory
            override fun getSharedPreferences(name: String, mode: Int) = target.getSharedPreferences(preferences, mode)
        }
        try {
            val source = File(directory, "camera.jpg")
            val bitmap = Bitmap.createBitmap(4000, 3000, Bitmap.Config.ARGB_8888)
            val random = java.util.Random(42)
            val row = IntArray(4000)
            for (y in 0 until 3000) {
                for (x in row.indices) row[x] = android.graphics.Color.rgb(random.nextInt(256), random.nextInt(256), random.nextInt(256))
                bitmap.setPixels(row, 0, 4000, 0, y, 4000, 1)
            }
            source.outputStream().use { assertTrue(bitmap.compress(Bitmap.CompressFormat.JPEG, 95, it)) }
            bitmap.recycle()
            val store = ChatImageDrafts(context).also { it.load() }
            val start = SystemClock.elapsedRealtime()
            val draft = store.stageImage(Uri.fromFile(source))
            store.update("test-session", listOf(draft))
            val previewMs = SystemClock.elapsedRealtime() - start
            assertTrue(store.drafts.value.getValue("test-session").single().preparing)
            assertEquals("source", store.file(draft.id).extension)
            assertEquals(source.length(), store.file(draft.id).length())
            source.delete()
            val restored = ChatImageDrafts(context).also { it.load() }
            assertFalse(restored.drafts.value.getValue("test-session").single().preparing)
            assertNotNull(restored.drafts.value.getValue("test-session").single().error)

            // Measure the previous PNG loop using exactly the same persisted photo.
            var legacy = BitmapFactory.decodeFile(store.file(draft.id).path)
            val oldStart = SystemClock.elapsedRealtime()
            while (true) {
                val bytes = ByteArrayOutputStream().use { legacy.compress(Bitmap.CompressFormat.PNG, 100, it); it.size() }
                if (bytes <= 5 * 1024 * 1024) break
                val small = Bitmap.createScaledBitmap(legacy, legacy.width * 3 / 4, legacy.height * 3 / 4, true)
                legacy.recycle()
                legacy = small
            }
            legacy.recycle()
            val legacyMs = SystemClock.elapsedRealtime() - oldStart
            val prepareStart = SystemClock.elapsedRealtime()
            val ready = restored.prepareImage(draft)
            val prepareMs = SystemClock.elapsedRealtime() - prepareStart
            assertFalse(ready.preparing)
            assertFalse(restored.needsPreparation(draft.id))
            val output = restored.file(draft.id)
            assertEquals("jpg", output.extension)
            assertTrue(output.length() in 1..5L * 1024 * 1024)
            assertEquals(0xff, output.inputStream().use { it.read() })
            val decoded = BitmapFactory.decodeFile(output.path)
            assertNotNull(decoded)
            decoded.recycle()
            println("12MP photo: preview=${previewMs}ms, JPEG preparation=${prepareMs}ms, previous PNG loop=${legacyMs}ms, output=${output.length()} bytes")
            restored.remove("test-session", draft.id)
            assertFalse(output.exists())

            val screenshot = File(directory, "screenshot.png")
            val pixels = Bitmap.createBitmap(128, 64, Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.GREEN) }
            screenshot.outputStream().use { pixels.compress(Bitmap.CompressFormat.PNG, 100, it) }
            pixels.recycle()
            val png = restored.prepareImage(restored.stageImage(Uri.fromFile(screenshot)))
            assertEquals("png", restored.file(png.id).extension)
            BitmapFactory.decodeFile(restored.file(png.id).path).let { decodedPng ->
                assertEquals(android.graphics.Color.GREEN, decodedPng.getPixel(0, 0))
                decodedPng.recycle()
            }
        } finally {
            directory.deleteRecursively()
            target.deleteSharedPreferences(preferences)
        }
    }
}
