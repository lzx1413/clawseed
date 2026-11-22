package dev.clawseed.demo.runtime

import android.os.PowerManager
import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.sdk.embedded.ProotRuntimeManager
import dev.clawseed.sdk.embedded.ProotRuntimeState
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.io.IOException
import java.nio.file.FileVisitResult
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.attribute.BasicFileAttributes
import java.util.concurrent.TimeUnit

/** Executes the actual Rust tools, including the PRoot cancellation path, on a device. */
class CommandBackendsDeviceTest {
    @Test fun synchronousAndBackgroundBackends() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val probe = File(instrumentation.context.applicationInfo.nativeLibraryDir, "libcommand-backends-probe.so")
        assertTrue("Run tools/build-command-backends-probe-android.sh first", probe.canExecute())
        val wakeLock = context.getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "ClawSeed:command-backends-test")
        val workspace = File(context.cacheDir, "command-backends-${java.util.UUID.randomUUID()}").apply { mkdirs() }
        wakeLock.acquire(3 * 60 * 1000L)
        try {
            // Keep native children foreground without waiting for UI idleness.
            context.startActivity(android.content.Intent(context, dev.clawseed.demo.MainActivity::class.java)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
            val status = runBlocking { ProotRuntimeManager(context).ensureReady() }
            assertEquals(status.error, ProotRuntimeState.READY, status.state)
            val native = context.applicationInfo.nativeLibraryDir
            val output = File(workspace, "probe.log")
            val builder = ProcessBuilder(probe.absolutePath, workspace.absolutePath)
                .redirectErrorStream(true).redirectOutput(output)
            builder.environment().apply {
                put("HOME", context.filesDir.absolutePath)
                put("TMPDIR", context.cacheDir.absolutePath)
                put("CLAWSEED_PROOT_RUNTIME_DIR", File(context.filesDir, "proot-runtime").absolutePath)
                put("CLAWSEED_PROOT_BINARY", "$native/libproot.so")
                put("CLAWSEED_PROOT_LOADER", "$native/libproot-loader.so")
                put("CLAWSEED_PROOT_LIBRARY_DIR", native)
            }
            val process = builder.start()
            var completed = false
            repeat(18) {
                if (!completed) {
                    completed = process.waitFor(5, TimeUnit.SECONDS)
                    android.util.Log.i("CommandBackendsDeviceTest", output.readText().takeLast(4096))
                }
            }
            if (!completed) process.destroyForcibly()
            val log = output.readText()
            android.util.Log.i("CommandBackendsDeviceTest", log)
            assertTrue("Command probe timed out: $log", completed)
            assertEquals(log, 0, process.exitValue())
            assertTrue(log, log.contains("COMMAND_BACKENDS_OK"))
        } finally {
            if (wakeLock.isHeld) wakeLock.release()
            Files.walkFileTree(workspace.toPath(), object : SimpleFileVisitor<Path>() {
                override fun visitFile(path: Path, attrs: BasicFileAttributes): FileVisitResult {
                    Files.delete(path)
                    return FileVisitResult.CONTINUE
                }
                override fun postVisitDirectory(path: Path, error: IOException?): FileVisitResult {
                    if (error != null) throw error
                    Files.delete(path)
                    return FileVisitResult.CONTINUE
                }
            })
        }
    }
}
