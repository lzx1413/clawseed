package dev.clawseed.demo.ui.chat

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Session-scoped keys and a byte limit keep scrolling from rereading full attachments. */
internal class AttachmentImageCache(private val maxBytes: Int = 24 * 1024 * 1024) {
    private val mutex = Mutex()
    private val entries = LinkedHashMap<String, ByteArray>(16, 0.75f, true)
    private var bytes = 0

    suspend fun load(key: String, read: suspend () -> Result<ByteArray>): Result<ByteArray> = mutex.withLock {
        entries[key]?.let { return@withLock Result.success(it) }
        val result = read()
        result.getOrNull()?.let { data ->
            if (data.size <= maxBytes) {
                entries[key] = data
                bytes += data.size
                while (bytes > maxBytes) {
                    val oldest = entries.entries.iterator()
                    bytes -= oldest.next().value.size
                    oldest.remove()
                }
            }
        }
        result
    }
}
