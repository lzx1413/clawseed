# Android 离线 Linux、Node.js 和 Python

ClawSeed 可以将 arm64 Linux runtime 随 APK 分发，首次在设置页启用时离线安装。
Rust Gateway 仍直接运行在 Android 上，`shell` 和 `background_run` 选择 `backend=linux` 时进入 PRoot。
打包布局参考 [DSH Go](https://github.com/ccch1mneyyy/dsh-Go)，构建脚本与业务引擎由 ClawSeed 独立维护。

## 构建 APK

从仓库根目录执行（需要 curl、GNU tar、ar、xz、Python 3.11+、Rust 和 Android NDK）：

```bash
./tools/build-proot-runtime.sh arm64-v8a build/proot-runtime-arm64
./tools/package-proot-bootstrap.sh
./tools/build-clawseed-android.sh aarch64 build
cd clients/android
./gradlew :sdk:embedded:testDebugUnitTest :app:assembleDebug \
  -PruntimeBundle=../../build/proot-runtime-arm64/clawseed-proot-runtime-arm64-v8a.tar.xz
```

Python 的 `python`/`python3` 入口使用脚本 exec 实际 `/opt/python/bin/python3` 路径，
保证复制模式的 venv 能找到标准库；不能将入口改回 `/usr/local/bin` 下的符号链接。
安装时同时检查复制模式和符号链接模式的 venv/pip。

第一步下载固定版本并核对 SHA-256：Ubuntu Base 24.04.5、Termux PRoot 5.1.107.102、
Node.js 24.22.1 和 CPython 3.12.15（python-build-standalone 20261021）。
构建机不执行 arm64 解释器。缓存、staging、压缩包和 APK 输入位于忽略提交的
`build/proot-runtime-arm64/`，无须将大型二进制文件放入 Git。

第二步生成 `packaged/assets/clawseed-runtime/{manifest.json,runtime.tar.xz}` 和
`packaged/jniLibs/arm64-v8a/lib{proot,proot-loader,talloc,android-shmem}.so`。
它会将原生依赖名 `libtalloc.so.2` 修补为 APK 可打包的 `libtalloc.so`。
Gradle 自动读取该目录；缺少生成物的普通开发构建仍可构建，但不会携带离线环境。
XZ 资源不再经过 ZIP 二次压缩，原生组件由 Android 解压到 `nativeLibraryDir`。

本次构建的 runtime 压缩体积约 68.82 MiB，解压后普通文件合计约 366.24 MiB。
实际安装还需要目录、文件系统块和 staging 预留空间；安装器按解压大小至少预留 20%。
APK 自身继续保留压缩资源，安装器直接读取 assets，不额外保存一份压缩包。
不同构建的字节数与哈希以生成的 manifest 为准。

## 启用与使用

进入设置页的 Linux 运行环境卡片，选择安装。安装器校验内置包哈希，安全解压到
staging，并实际运行 Node、npm、Python、venv/pip 和子进程检查，通过后才切换
`active.json`。安装在后台 IO 线程进行，不阻塞普通网关启动；安装期间持有有时限的部分唤醒锁，
并在结束时释放，避免屏幕休眠中断解压及解释器自检。
内置 manifest 的信任来自 APK 签名；远程安装仍要求 HTTPS 与发布者签名。

```json
{"name":"shell","arguments":{"backend":"linux","command":"node --version && npm --version && python3 --version && python3 -m pip --version"}}
```

Linux 后端每次执行重新解析 active generation，所以首次安装无需重启网关。
`runtime_status` 返回 generation 路径、runtime 版本及 manifest 声明的 Node/Python 版本。
工作区映射为 `/workspace`，每条命令有独立 `/tmp`；宿主临时文件使用 App 私有目录，
guest 环境通过 `env -i` 重建，避免 Android 动态库变量污染 Linux 解释器。

推荐每个项目独立管理依赖：

```bash
mkdir -p /workspace/demo
cd /workspace/demo
python3 -m venv .venv
.venv/bin/python -m pip install requests
npm init -y
npm install lodash
```

离线可使用解释器、标准库和已安装依赖；下载新 npm/pip 包需要联网。
需要本地编译的扩展还需要适配的编译器、系统头文件和开发库，本包不包含完整工具链。
CA 文件来自固定 Python 包所带的 certifi；DNS 默认使用 1.1.1.1 和 8.8.8.8，
受限网络可能需要调整 guest 的 `/etc/resolv.conf`。

重试和移除时会恢复 PRoot 创建的 mode-000 挂载占位目录的拥有者权限，
再以不跟随符号链接的方式清理。

安装、更新、检查、回滚和移除与命令执行使用共享/排他 POSIX 文件锁互斥。
有运行中的 Linux 命令时拒绝这些操作；操作期间新 Linux 命令会返回运行时忙碌错误。
移除会删除 rootfs 中手工安装的工具，保留 Android workspace。

已有 READY 环境不会被 APK 更新自动覆盖。设置中的“更新或修复环境”会从当前安装源
重新安装、自检新的 generation，通过后原子切换，保留当前 generation 作为上一版本。
内置资源优先于远程安装源；更新 APK 后可用此按钮启用新版内置环境。
失败时继续保留当前可用版本。只保留当前及上一版本，更早版本在成功更新后清理。
“恢复上一版本环境”会先自检再切换，不会重装或删除其中的依赖。
工作区文件保留；rootfs 内手工安装的依赖留在旧 generation，不自动迁移到新版本。
更新期间需预留新环境的完整空间；不要把项目的唯一副本放在 guest `/root` 下。

“检查 Python 和 Node.js”实际执行 Node/npm、Python 模块、子进程和两种 venv/pip 探针，
保存检查时间及失败原因。状态查询和每次 Linux 命令还会检查关键解释器文件与已保存的
健康记录，发现损坏时要求检查或修复。

## 后台任务与重启

`background_run` 使用与同步命令相同的 Linux 后端。Android 前台服务通过已认证的
`GET /api/background-jobs` 独立查询活动任务，聊天结束后继续持有部分唤醒锁并保留服务，
任务结束后释放。保活逻辑忽略聊天状态变化前发出的旧查询；短暂查询失败保留租约，
最长按一小时任务上限加五分钟余量处理。系统强制停止、杀进程和厂商限制仍可能中断任务。

通过文件加载配置时，任务快照保存在配置文件旁的 `clawseed.background-jobs/`。
网关重启后保留结果，原运行或排队任务标记为 `failed/interrupted_by_restart`，不自动重跑。
完成记录按最近 64 条、24 小时、合计 16 MiB 输出清理；清理在启动、查询和状态变化时执行。
`background_status` 不传 `job_id` 可列出任务元数据，传入 ID 可读取保留的输出。
存储不可写或被另一网关/CLI 占用时拒绝新后台任务，避免悄悄丢失结果。
以纯内存配置构造工具（没有 `config_path`）时仍使用内存记录。

Linux/Android 命令还设置父进程退出信号，PRoot 使用 SIGQUIT 请求清理被跟踪的子进程。
输出管道的关闭也受时间限制，避免 shell 已退出、后代仍持有输出时让任务一直占用队列。

## 验证边界

本地 JVM 测试覆盖 GNU/PAX 长路径、链接、路径穿越、损坏包、解压上限、完整实际资源包，
以及完整 manifest 签名验证。Rust 测试覆盖动态激活和命令策略。

ClawSeed 保持 `targetSdk = 37`，DSH Go 为 28；PRoot 原生库能够启动不代表任意 guest
程序都能执行。必须在实际 Android 设备上通过安装时探针及以下命令验收：

```bash
pwd && cat /etc/os-release
node -e 'console.log(require("node:child_process").execFileSync("/bin/sh",["-c","printf child"]).toString())'
python3 -c 'import ssl, sqlite3, subprocess; print(subprocess.check_output(["/bin/sh","-c","printf child"]).decode())'
sleep 5 # 通过工具参数 timeout_secs=1 验证超时
```

还需验证 pip/npm HTTPS 下载、工作区写入持久化、运行时移除保护和 App 重启恢复。
PRoot 是 Linux 用户态兼容层，仍使用 App UID 和 Android 内核，不是内核级安全沙箱。

### Release 真机回归

`LinuxRuntimeDeviceTest.installsBundledRuntimeAndCreatesBothVenvKinds` 会使用当前 APK
的内置包完成安装，然后分别创建复制式和符号链接式 venv，验证其中的 pip。
它保留安装成功的 runtime，不清空用户工作区。运行前应先将对应 Release APK 覆盖安装。
构建 Release AndroidTest 时，如 Gradle 9.3.1 遇到 Binary store 依赖序列化错误，
可使用已安装的 Gradle 9.8.1 构建同一测试目标。

```bash
cd clients/android
./gradlew :app:assembleReleaseAndroidTest -PattachmentTestBuildType=release
adb install -r -t app/build/outputs/apk/androidTest/release/app-release-androidTest.apk
adb shell am instrument -w \
  -e class dev.clawseed.demo.runtime.LinuxRuntimeDeviceTest \
  dev.clawseed.demo.test/androidx.test.runner.AndroidJUnitRunner
```

### 统一命令执行的真机验证

```bash
./tools/build-command-backends-probe-android.sh
cd clients/android
./gradlew :app:assembleReleaseAndroidTest -PattachmentTestBuildType=release
adb install -r -t app/build/outputs/apk/androidTest/release/app-release-androidTest.apk
adb shell am instrument -w \
  -e class dev.clawseed.demo.runtime.CommandBackendsDeviceTest \
  dev.clawseed.demo.test/androidx.test.runner.AndroidJUnitRunner
```

探针只进入测试 APK，不进入正式 APK。它直接调用实际 Rust 工具，验证宿主/Linux 同步执行、
Python/Node.js、共享工作区、兼容入口、后台状态和通知、超时、排队及运行中取消，以及子进程清理。
正式 APK 与测试 APK 需使用匹配的签名。测试使用独立缓存工作区，保留用户已安装的 runtime。

`RuntimeLifecycleDeviceTest` 使用隔离的文件和缓存目录，验证新环境安装、损坏识别、完整健康检查、
更新保留旧依赖、回滚及失败更新保留当前版本，不切换用户实际安装的 runtime。
