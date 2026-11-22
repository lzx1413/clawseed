package dev.clawseed.demo.runtime

import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.sdk.embedded.ProotRuntimeManager
import dev.clawseed.sdk.embedded.ProotRuntimeState
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.junit.Assert.*
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.FileVisitResult
import java.nio.file.attribute.BasicFileAttributes
import java.io.IOException
import java.util.concurrent.TimeUnit

/** Run explicitly against the signed release; keeps the successfully installed runtime. */
class LinuxRuntimeDeviceTest {
    @Test fun installsBundledRuntimeAndCreatesBothVenvKinds() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val wakeLock = context.getSystemService(android.os.PowerManager::class.java)
            .newWakeLock(android.os.PowerManager.PARTIAL_WAKE_LOCK, "ClawSeed:runtime-device-test")
        wakeLock.acquire(3 * 60 * 1000L)
        try {
        val status = runBlocking { ProotRuntimeManager(context).ensureReady() }
        assertEquals(status.error, ProotRuntimeState.READY, status.state)
        val rootfs = File(status.path!!, "rootfs")
        val temporary = File(context.cacheDir, "venv-test-${java.util.UUID.randomUUID()}").apply { mkdirs() }
        val native = context.applicationInfo.nativeLibraryDir
        val script = """
            import sys, subprocess, venv
            print('PYTHON', sys.version, sys.executable, sys.prefix, flush=True)
            for links in (False, True):
                path = '/tmp/venv-' + str(links)
                try:
                    venv.create(path, with_pip=True, symlinks=links)
                except subprocess.CalledProcessError as error:
                    print(error.output.decode(errors='replace'), flush=True)
                    raise
                subprocess.run([path+'/bin/python','-m','pip','--version'],check=True)
                print('VENV_OK', links, flush=True)
        """.trimIndent()
        try {
            val builder = ProcessBuilder(listOf("$native/libproot.so", "--kill-on-exit", "--link2symlink", "--sysvipc", "-0",
                "-r", rootfs.absolutePath, "-b", "/dev", "-b", "/proc", "-b", "/system", "-b", "/apex",
                "-b", "${temporary.absolutePath}:/tmp", "-w", "/root", "/usr/bin/env", "-i",
                "HOME=/root", "PATH=/usr/local/bin:/usr/bin:/bin", "TMPDIR=/tmp", "/usr/local/bin/python3", "-c", script))
                .redirectErrorStream(true).redirectOutput(File(temporary, "output.txt"))
            builder.environment().apply {
                clear()
                put("PATH", "/system/bin")
                put("PROOT_LOADER", "$native/libproot-loader.so")
                put("LD_LIBRARY_PATH", native)
                put("PROOT_TMP_DIR", temporary.absolutePath)
                put("PROOT_L2S_DIR", File(rootfs, "root/.l2s").absolutePath)
            }
            val process = builder.start()
            val completed = process.waitFor(90, TimeUnit.SECONDS)
            if (!completed) process.destroyForcibly()
            val output = File(temporary, "output.txt").readText()
            android.util.Log.i("LinuxRuntimeDeviceTest", output)
            assertTrue("venv test timed out", completed)
            assertEquals(output, 0, process.exitValue())
            assertTrue(output, output.contains("VENV_OK False") && output.contains("VENV_OK True"))
        } finally {
            for (directory in listOf(temporary, File(context.cacheDir, "venv-diagnostic"))) {
                if (!Files.exists(directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)) continue
                Files.walkFileTree(directory.toPath(), object : SimpleFileVisitor<Path>() {
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
        } finally {
            if (wakeLock.isHeld) wakeLock.release()
        }
    }
}
