package dev.clawseed.demo.ui.chat

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import coil3.ImageLoader
import coil3.disk.DiskCache
import okio.Path.Companion.toOkioPath
import java.io.File

/** App-owned cache for agent media previews. It never stores chat transcripts. */
object RichMediaCache {
    private const val DIRECTORY = "rich-media"
    private const val IMAGE_DIRECTORY = "images"
    const val MAX_IMAGE_BYTES: Long = 100L * 1024L * 1024L

    fun imageDirectory(context: Context): File = File(cacheDirectory(context), IMAGE_DIRECTORY)

    fun cacheDirectory(context: Context): File = File(context.cacheDir, DIRECTORY)

    fun sizeBytes(context: Context): Long = cacheDirectory(context)
        .walkTopDown()
        .filter { it.isFile }
        .sumOf { it.length() }

    fun clear(context: Context): Boolean {
        val directory = cacheDirectory(context)
        return !directory.exists() || directory.deleteRecursively()
    }
}

@Composable
fun rememberRichMediaImageLoader(): ImageLoader {
    val appContext = LocalContext.current.applicationContext
    return remember(appContext) {
        ImageLoader.Builder(appContext)
            .diskCache {
                DiskCache.Builder()
                    .directory(RichMediaCache.imageDirectory(appContext).toOkioPath())
                    .maxSizeBytes(RichMediaCache.MAX_IMAGE_BYTES)
                    .build()
            }
            .build()
    }
}
