package dev.clawseed.sdk.embedded

import android.content.Context
import java.io.File
import java.util.concurrent.TimeUnit

/** Installing files is insufficient: verify guest interpreters under the app's actual target SDK. */
internal object RuntimeReadinessProbe {
    fun check(context: Context, runtime: File) {
        val native = File(context.applicationInfo.nativeLibraryDir)
        val nativeTawcroot = File(native, "libtawcroot.so").takeIf { it.canExecute() }
        val nativeProot = File(native, "libproot.so")
        val proot = nativeProot.takeIf { it.canExecute() } ?: File(runtime, "proot")
        val loader = if (proot == nativeProot) File(native, "libproot-loader.so") else File(runtime, "proot-loader")
        val libraries = if (File(native, "libtalloc.so").isFile) native else File(runtime, "proot-libs")
        val rootfs = File(runtime, "rootfs")
        val temporary = File(context.cacheDir, "runtime-probe-${java.util.UUID.randomUUID()}").apply { mkdirs() }
        val log = File(temporary, "probe.log")
        try {
            val backendCommand = if (nativeTawcroot != null) {
                listOf(nativeTawcroot.absolutePath, "-r", rootfs.absolutePath,
                    "-b", "/dev:/dev", "-b", "/proc:/proc", "-b", "/system:/system", "-b", "/apex:/apex",
                    "-b", "${temporary.absolutePath}:/tmp", "--", "/usr/bin/env", "-i")
            } else {
                listOf(proot.absolutePath, "--kill-on-exit", "--link2symlink", "--sysvipc", "-0",
                    "-r", rootfs.absolutePath, "-b", "/dev", "-b", "/proc", "-b", "/system", "-b", "/apex",
                    "-b", "${temporary.absolutePath}:/tmp", "-w", "/root", "/usr/bin/env", "-i")
            }
            val command = backendCommand + listOf(
                "HOME=/root", "PATH=/usr/local/bin:/usr/bin:/bin", "LANG=C.UTF-8", "TMPDIR=/tmp",
                "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt", "/bin/sh", "-ec",
                "node -e 'require(\"node:child_process\").execFileSync(\"/bin/sh\",[\"-c\",\"true\"]); console.log(process.version)'\n" +
                    "npm --version\n" +
                    "python3 -c 'import ssl, sqlite3, subprocess, venv; subprocess.run([\"/bin/sh\",\"-c\",\"true\"],check=True); venv.create(\"/tmp/venv\",with_pip=False)'\n" +
                    "/tmp/venv/bin/python -m ensurepip --upgrade --default-pip\n" +
                    "/tmp/venv/bin/python -m pip --version\n" +
                    "python3 -m venv /tmp/venv-links\n" +
                    "/tmp/venv-links/bin/python -m pip --version\n" +
                    "printf CLAWSEED_RUNTIME_OK")
            val builder = ProcessBuilder(command).directory(context.filesDir)
                .redirectErrorStream(true).redirectOutput(log)
            builder.environment().apply {
                clear()
                put("PATH", "/system/bin")
                put("TMPDIR", temporary.absolutePath)
                put("PROOT_TMP_DIR", temporary.absolutePath)
                put("PROOT_L2S_DIR", File(rootfs, "root/.l2s").apply { mkdirs() }.absolutePath)
                if (nativeTawcroot == null) {
                    put("PROOT_LOADER", loader.absolutePath)
                    put("LD_LIBRARY_PATH", libraries.absolutePath)
                }
            }
            var process = runCatching { builder.start() }.getOrElse { firstError ->
                check(nativeTawcroot != null && proot.canExecute()) { firstError.message ?: "Linux runtime launcher failed" }
                ProcessBuilder(listOf(proot.absolutePath, "--kill-on-exit", "--link2symlink", "--sysvipc", "-0",
                    "-r", rootfs.absolutePath, "-b", "/dev", "-b", "/proc", "-b", "/system", "-b", "/apex",
                    "-b", "${temporary.absolutePath}:/tmp", "-w", "/root", "/usr/bin/env", "-i") + command.drop(backendCommand.size))
                    .directory(context.filesDir).redirectErrorStream(true).redirectOutput(log).apply {
                        environment().apply {
                            clear(); put("PATH", "/system/bin"); put("TMPDIR", temporary.absolutePath)
                            put("PROOT_TMP_DIR", temporary.absolutePath)
                            put("PROOT_L2S_DIR", File(rootfs, "root/.l2s").apply { mkdirs() }.absolutePath)
                            put("PROOT_LOADER", loader.absolutePath); put("LD_LIBRARY_PATH", libraries.absolutePath)
                        }
                    }.start()
            }
            val completed = process.waitFor(90, TimeUnit.SECONDS)
            if (!completed) {
                process.destroyForcibly()
                process.waitFor(5, TimeUnit.SECONDS)
            }
            val output = if (log.isFile) log.inputStream().use { input ->
                val buffer = ByteArray(4096)
                val count = input.read(buffer)
                if (count > 0) String(buffer, 0, count, Charsets.UTF_8) else ""
            } else ""
            check(completed && process.exitValue() == 0 && output.contains("CLAWSEED_RUNTIME_OK")) {
                if (!completed) "Linux runtime execution check timed out after 90 seconds: ${output.take(2000)}"
                else "Linux runtime execution check failed on this device: ${output.take(2000)}"
            }
        } finally {
            RuntimeFiles.deleteTree(temporary)
        }
    }
}
