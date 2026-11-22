# Offline Linux, Node.js and Python on Android

ClawSeed can bundle an arm64 Linux runtime in the APK. Users install it offline
from Settings. The Rust gateway stays on Android; `shell` and `background_run` use PRoot when `backend=linux`. Packaging follows the layout used by
[DSH Go](https://github.com/ccch1mneyyy/dsh-Go), with an independent ClawSeed recipe.

From the repository root:

```bash
./tools/build-proot-runtime.sh arm64-v8a build/proot-runtime-arm64
./tools/package-proot-bootstrap.sh
./tools/build-clawseed-android.sh aarch64 build
cd clients/android
./gradlew :sdk:embedded:testDebugUnitTest :app:assembleDebug \
  -PruntimeBundle=../../build/proot-runtime-arm64/clawseed-proot-runtime-arm64-v8a.tar.xz
```

The recipe requires curl, GNU tar, ar, xz and Python 3.11+. Inputs are pinned by
version and SHA-256: Ubuntu Base 24.04.5, Termux PRoot 5.1.107.102, Node.js 24.22.1
and CPython 3.12.15 from python-build-standalone 20261021. No arm64 code runs on
the build host. Generated files are ignored under `build/proot-runtime-arm64/`.
Gradle reads its `packaged/assets` and `packaged/jniLibs` directories. A normal
developer build without these generated inputs does not contain an offline runtime.

The measured archive is about 68.82 MiB; extracted regular files total 366.24 MiB.
Consult the generated manifest for exact bytes and hashes. Installation reserves
20% additional space, plus filesystem overhead. Assets stream directly from the
APK without retaining a second compressed copy. Native libraries use legacy
extraction and the talloc dependency/SONAME is patched to `libtalloc.so`.

Installation verifies the archive, extracts to staging and probes Node, npm,
Python, venv/pip and guest subprocess execution before activation. The APK
signature authenticates bundled manifests; remote manifests still need HTTPS and
a trusted publisher signature. Each tool call reads the active pointer, so first
installation needs no gateway restart. `runtime_status` includes declared component
versions. Failed probes leave the old active pointer unchanged.

The `python`/`python3` entry scripts exec `/opt/python/bin/python3` directly.
Do not replace them with `/usr/local/bin` symlinks: copied venv interpreters may
then fall back to the standalone build prefix `/install` and fail to import
`encodings`. The readiness probe checks both copied and symlinked environments.

Projects live under `/workspace`; each command gets a private `/tmp`. Use project
`node_modules` and Python `.venv` directories. Interpreters and installed packages
work offline; fetching new dependencies requires networking. Native extensions
may need compilers and development libraries that are not included. The CA bundle
comes from the pinned Python distribution's vendored certifi. Guest DNS defaults
to 1.1.1.1 and 8.8.8.8; restricted networks may require editing `/etc/resolv.conf`.

Cleanup restores owner access to mode-000 PRoot binding placeholders without
following guest symlinks. Installation holds a bounded partial wake lock.
Install, update, health check, rollback and removal hold an exclusive POSIX lock;
running Linux commands hold shared locks. Wait for commands to finish before these
operations. New Linux commands receive a busy error during maintenance.
Removal deletes guest tools but preserves the Android workspace.

APK updates do not automatically overwrite a READY runtime. Settings offers
**Update or repair environment**: prepare and check a new generation, atomically
activate it, and retain the current one for rollback. Bundled assets take precedence
over remote installation sources. A failed update keeps the current usable version.
Only the active and previous generations are retained; earlier generations are
pruned after a successful update. Rollback checks the previous generation before
switching. Guest-installed dependencies remain in their original generation and
are not migrated. Workspace files remain available. Allow enough free space for
the complete new generation during an update.

**Check Python and Node.js** executes interpreter, subprocess, and both venv/pip
probes and records the check time and result. Status and Linux command dispatch
also check essential executables and the saved health result before reporting ready.

### Background work and restarts

The Android foreground service polls authenticated `GET /api/background-jobs`
independently of chat generation, retaining its partial wake lock and service while
jobs remain active. Stale idle polls cannot release a newer execution lease.
Transient poll failures preserve the lease for up to the one-hour command ceiling
plus five minutes. System force-stop, process termination, and vendor restrictions
can still interrupt work.

File-backed configurations store atomic job snapshots beside the configuration in
`clawseed.background-jobs/`. Restart retains results and marks former queued/running
jobs `failed` with `interrupted_by_restart`; commands are never replayed.
On startup, queries, and job transitions, completed history is pruned to 64 jobs,
24 hours, and 16 MiB total captured output. Omit `job_id` from `background_status`
to list metadata without copying output. Unwritable or concurrently owned storage
rejects new background jobs. Configurations without `config_path` retain memory-only
behavior. Linux/Android commands set a parent-death signal, using SIGQUIT for PRoot
to request tracee cleanup. Output-pipe draining is bounded even after the shell exits.

`RuntimeLifecycleDeviceTest` uses isolated storage to verify installation, damaged
interpreters, health checks, preservation of the previous environment, rollback,
and failed updates. It does not switch the user's installed runtime.

JVM tests cover long GNU/PAX names, links, traversal rejection, corrupt archives,
size limits, the actual complete bundle and manifest signature verification.
Rust tests cover activation changes and command policy. ClawSeed retains target
SDK 37, whereas DSH Go uses 28: installation must pass the execution probe on an
actual Android device. Also verify HTTPS pip/npm installs, workspace persistence,
timeouts, removal protection and recovery after restarting the app. PRoot runs
with the app UID and is not a kernel security sandbox.

The Android `CommandBackendsDeviceTest` runs the actual Rust tool implementations.
Build its native probe with `tools/build-command-backends-probe-android.sh`, build
and install the matching Release AndroidTest APK, then run the test class through
AndroidJUnitRunner. The probe is packaged only in the test APK. It covers both
backends, Python/Node.js, workspace bindings, legacy calls, background completion
and events, timeout, queue cancellation and tracee cleanup.
