package dev.clawseed.demo.runtime

import android.content.ContextWrapper
import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.sdk.embedded.ProotRuntimeConfig
import dev.clawseed.sdk.embedded.ProotRuntimeManager
import dev.clawseed.sdk.embedded.ProotRuntimeState
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption

/** Isolated app storage: upgrade/repair tests never switch the user's runtime. */
class RuntimeLifecycleDeviceTest {
    @Test fun checksRepairsAndRollsBackWithoutLosingThePreviousEnvironment() = runBlocking {
        val original = InstrumentationRegistry.getInstrumentation().targetContext
        val temporary = File(original.cacheDir, "runtime-lifecycle-${java.util.UUID.randomUUID()}").apply { mkdirs() }
        val context = object : ContextWrapper(original) {
            override fun getFilesDir() = File(temporary, "files").apply { mkdirs() }
            override fun getCacheDir() = File(temporary, "cache").apply { mkdirs() }
        }
        val wakeLock = original.getSystemService(android.os.PowerManager::class.java)
            .newWakeLock(android.os.PowerManager.PARTIAL_WAKE_LOCK, "ClawSeed:runtime-lifecycle-test")
        wakeLock.acquire(5 * 60 * 1000L)
        original.startActivity(android.content.Intent(original, dev.clawseed.demo.MainActivity::class.java)
            .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            val manager = ProotRuntimeManager(context)
            val first = manager.ensureReady()
            assertEquals(first.error, ProotRuntimeState.READY, first.state)
            assertNotNull(first.checkedAtMs)
            val marker = File(first.path!!, "rootfs/root/lifecycle-dependency")
            marker.writeText("keep-me")
            val workspaceFile = File(context.filesDir, "workspace/project.txt").apply { parentFile!!.mkdirs(); writeText("project") }

            val updated = manager.updateInstalled()
            assertEquals(updated.error, ProotRuntimeState.READY, updated.state)
            assertNotEquals(first.generation, updated.generation)
            assertTrue(updated.canRollback)
            assertEquals("keep-me", marker.readText())
            assertEquals("project", workspaceFile.readText())
            assertFalse(File(updated.path!!, "rootfs/root/lifecycle-dependency").exists())

            val node = File(updated.path!!, "rootfs/opt/node/bin/node")
            val savedNode = File(node.parentFile, "node.saved")
            assertTrue(node.renameTo(savedNode))
            assertEquals(ProotRuntimeState.CORRUPTED, manager.status().state)
            try {
                assertEquals(ProotRuntimeState.CORRUPTED, manager.checkHealth().state)
            } finally { assertTrue(savedNode.renameTo(node)) }
            val healthy = manager.checkHealth()
            assertEquals(healthy.error, ProotRuntimeState.READY, healthy.state)

            val rolledBack = manager.rollback()
            assertEquals(rolledBack.error, ProotRuntimeState.READY, rolledBack.state)
            assertEquals(first.generation, rolledBack.generation)
            assertEquals("keep-me", marker.readText())
            val failure = ProotRuntimeManager(context, ProotRuntimeConfig(expectedAbi = "invalid-test-abi")).updateInstalled()
            assertEquals(ProotRuntimeState.READY, failure.state)
            assertNotNull(failure.error)
            assertEquals(first.generation, manager.status().generation)
            assertTrue(manager.clearInstalled())
            assertEquals(ProotRuntimeState.NOT_INSTALLED, manager.status().state)
            assertEquals("project", workspaceFile.readText())
        } finally {
            if (wakeLock.isHeld) wakeLock.release()
            fun remove(file: File) {
                if (Files.isDirectory(file.toPath(), LinkOption.NOFOLLOW_LINKS)) {
                    file.setReadable(true, true); file.setWritable(true, true); file.setExecutable(true, true)
                    checkNotNull(file.listFiles()) { "Cannot enumerate $file" }.forEach(::remove)
                }
                Files.deleteIfExists(file.toPath())
            }
            remove(temporary)
        }
    }
}
