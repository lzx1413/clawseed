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
    @Volatile private var sharedImageLoader: ImageLoader? = null

    const val MAX_IMAGE_BYTES: Long = 100L * 1024L * 1024L

    fun imageDirectory(context: Context): File = File(cacheDirectory(context), IMAGE_DIRECTORY)

    fun cacheDirectory(context: Context): File = File(context.cacheDir, DIRECTORY)

    fun sizeBytes(context: Context): Long = cacheDirectory(context)
        .walkTopDown()
        .filter { it.isFile }
        .sumOf { it.length() }

    fun imageLoader(context: Context): ImageLoader {
        sharedImageLoader?.let { return it }
        return synchronized(this) {
            sharedImageLoader ?: ImageLoader.Builder(context.applicationContext)
                .diskCache {
                    DiskCache.Builder()
                        .directory(imageDirectory(context.applicationContext).toOkioPath())
                        .maxSizeBytes(MAX_IMAGE_BYTES)
                        .build()
                }
                .build().also { sharedImageLoader = it }
        }
    }

    fun clear(context: Context): Boolean = runCatching {
        // Let the live cache manage its journal instead of deleting files under it.
        imageLoader(context).memoryCache?.clear()
        imageLoader(context).diskCache?.clear()
        true
    }.getOrDefault(false)
}

@Composable
fun rememberRichMediaImageLoader(): ImageLoader {
    val appContext = LocalContext.current.applicationContext
    return remember(appContext) { RichMediaCache.imageLoader(appContext) }
}
