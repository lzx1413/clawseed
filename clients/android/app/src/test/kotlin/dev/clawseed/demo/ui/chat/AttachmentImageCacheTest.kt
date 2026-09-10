package dev.clawseed.demo.ui.chat

import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test

class AttachmentImageCacheTest {
    @Test fun concurrentReadersShareBytesAndDifferentSessionsRemainIsolated() = runTest {
        val cache = AttachmentImageCache(8)
        var reads = 0
        suspend fun read(): Result<ByteArray> { reads++; delay(10); return Result.success(byteArrayOf(1, 2)) }
        val first = async { cache.load("session-a:image", ::read).getOrThrow() }
        val second = async { cache.load("session-a:image", ::read).getOrThrow() }
        assertSame(first.await(), second.await())
        assertEquals(1, reads)
        cache.load("session-b:image", ::read)
        assertEquals(2, reads)
    }

    @Test fun budgetEvictsLeastRecentlyUsedBytesAndFailuresCanRetry() = runTest {
        val cache = AttachmentImageCache(4)
        suspend fun load(key: String) = cache.load(key) { Result.success(ByteArray(2)) }.getOrThrow()
        val first = load("first")
        val second = load("second")
        assertSame(first, load("first"))
        load("third")
        assertNotSame(second, load("second"))
        assertTrue(cache.load("retry") { Result.failure(Exception("offline")) }.isFailure)
        assertTrue(cache.load("retry") { Result.success(byteArrayOf(7)) }.isSuccess)
    }
}
