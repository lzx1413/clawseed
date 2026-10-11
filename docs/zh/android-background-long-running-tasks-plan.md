# Android 后台长程任务开发计划

ClawSeed 需要解决两个不同的问题，分别实施和验收：

- **Plan A：当前对话后台继续执行**
- **Plan B：可靠的定时后台任务**

不在本计划中引入 Workspace 后台进程、Sub-agent 或统一 AgentRun 体系。

## Plan A：当前对话后台继续执行

### 目标

用户发送消息后，即使切到后台或锁屏，当前这轮 Agent 对话仍能完成、失败、取消或超时。

本 Plan 只处理已经开始的对话，不负责进程被杀后的任务重放。

### 实现

复用 [`ClawseedService`](../../clients/android/app/src/main/kotlin/dev/clawseed/demo/ClawseedService.kt) 作为 Gateway 和执行宿主：

1. 生成开始前确认 Foreground Service 已就绪；
2. 获取 `Partial WakeLock`，防止设备休眠中断网络流；
3. 在后台继续收集 WebSocket 事件并保存消息；
4. 生成完成、失败、取消或超时时统一清理 Gateway、会话、协程和 WakeLock；
5. App 在后台时发送完成或失败通知。

WakeLock 必须按活动任务计数管理，不能因一个并发任务结束而提前释放其他任务的保活资源。

保留当前连接和生成超时，并增加整轮对话的总超时上限。

### 验收

- 发送后立即切到后台，对话仍能完成；
- 锁屏期间生成不会因为设备休眠而立即中断；
- 返回 App 后能看到完整回复；
- 超时、取消和网络错误都会释放连接和 WakeLock；
- 后台完成时通知能打开对应会话。

## Plan B：可靠的定时后台任务

### 目标

任务即使没有打开当前对话，也能按计划触发；设备重启、App 更新或进程异常退出后，状态可判断，不重复执行，也不会静默丢失。

现有相关代码：

- [`ScheduledTask.kt`](../../clients/android/app/src/main/kotlin/dev/clawseed/demo/scheduled/ScheduledTask.kt)：任务模型；
- [`ScheduledTaskStore.kt`](../../clients/android/app/src/main/kotlin/dev/clawseed/demo/scheduled/ScheduledTaskStore.kt)：任务持久化；
- [`ScheduledTaskManager.kt`](../../clients/android/app/src/main/kotlin/dev/clawseed/demo/scheduled/ScheduledTaskManager.kt)：AlarmManager 调度；
- [`BootReceiver.kt`](../../clients/android/app/src/main/kotlin/dev/clawseed/demo/scheduled/BootReceiver.kt)：重启恢复。

### 实现

1. **状态和幂等领取**

   为任务增加 `RUNNING`、`INTERRUPTED` 等状态，以及 `runId`、`scheduledAt`、`startedAt`、`retryCount` 和错误信息。

   每次触发先以 `taskId + scheduledAt` 领取任务：已成功、或仍在租约内的相同触发直接跳过；只有领取成功的调用可以执行。

   第一版沿用现有 DataStore，不立即迁移 Room。

2. **调度和恢复**

   保留 AlarmManager 负责单次、每日、工作日和自定义星期触发。

   补充 `BOOT_COMPLETED`、`MY_PACKAGE_REPLACED`、时区/时间变化和 App 启动校验。WorkManager 只做低频恢复检查：重新注册丢失的 Alarm、清理超时的 `RUNNING` 任务和触发有限重试。

3. **执行和保活**

   定时任务触发后复用 `ClawseedService`，执行期间使用 Foreground Service 和 `Partial WakeLock`。完成、失败、取消和超时都必须保存最终状态、释放资源并发送通知。

   默认保留现有 WebSocket 10 秒连接超时和 5 分钟生成超时，增加整个任务 15 分钟上限。单次任务中断后最多自动重试一次。

4. **进程异常恢复**

   App 启动或服务重新创建时检查 `RUNNING` 任务：超过租约则标记 `INTERRUPTED`，重复任务保留下一次 Alarm，单次任务按重试次数决定是否重试。

### 验收

- 设备重启后启用的任务会重新注册；
- App 更新后启用的任务会重新注册；
- 同一任务的同一 `scheduledAt` 只执行一次；
- 进程在 `RUNNING` 阶段被杀后，下一次启动会标记 `INTERRUPTED`；
- 超时后会停止会话、释放 WakeLock、保存失败状态并通知用户；
- 任务完成、失败、取消和异常退出都不会遗留连接或 WakeLock。

## 交付顺序

1. Plan A：Foreground Service、WakeLock、后台事件保存和超时收尾；
2. Plan A：后台完成通知和 Android 验收；
3. Plan B：状态、租约、幂等领取和 Alarm 恢复；
4. Plan B：WorkManager 检查、中断重试、通知和测试。
