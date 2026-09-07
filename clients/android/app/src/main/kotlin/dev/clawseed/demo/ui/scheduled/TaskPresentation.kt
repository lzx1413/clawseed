package dev.clawseed.demo.ui.scheduled

import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskManager

internal fun nextScheduledRun(task: ScheduledTask, nowMillis: Long): Long? {
    if (!task.enabled) return null
    return runCatching { ScheduledTaskManager.nextTriggerMillis(task, nowMillis) }.getOrNull()
}
