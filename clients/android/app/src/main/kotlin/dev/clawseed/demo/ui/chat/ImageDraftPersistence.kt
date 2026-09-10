package dev.clawseed.demo.ui.chat

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

internal data class ImageDraftSnapshot(
    val images: Map<String, List<ChatImageDraft>> = emptyMap(),
    val texts: Map<String, String> = emptyMap(),
)

/** Serializes durable mutations off the UI thread, publishing only successful writes. */
internal class ImageDraftPersistence(
    private val read: () -> ImageDraftSnapshot,
    private val write: (ImageDraftSnapshot) -> Unit,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val mutex = Mutex()
    @Volatile private var snapshot = ImageDraftSnapshot()
    private val mutableDrafts = MutableStateFlow(snapshot.images)
    val drafts = mutableDrafts.asStateFlow()
    private val mutableReady = MutableStateFlow(false)
    val ready = mutableReady.asStateFlow()

    fun savedText(key: String): String = snapshot.texts[key].orEmpty()

    suspend fun load() = withContext(dispatcher) { mutex.withLock { loadLocked() } }

    private fun loadLocked() {
        if (mutableReady.value) return
        val stored = read()
        snapshot = stored.copy(images = stored.images.mapValues { (_, images) ->
            images.map { it.copy(uploading = false, awaitingReply = false) }
        })
        mutableDrafts.value = snapshot.images
        mutableReady.value = true
    }

    suspend fun update(transform: (ImageDraftSnapshot) -> ImageDraftSnapshot) = withContext(dispatcher) {
        mutex.withLock {
            loadLocked()
            val updated = transform(snapshot)
            write(updated)
            snapshot = updated
            mutableDrafts.value = updated.images
        }
    }
}
