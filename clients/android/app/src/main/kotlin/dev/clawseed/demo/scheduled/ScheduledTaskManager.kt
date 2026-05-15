package dev.clawseed.demo.scheduled

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import dev.clawseed.demo.ClawseedService
import java.util.Calendar

object ScheduledTaskManager {

    private const val TAG = "ScheduledTaskManager"

    fun scheduleAlarm(context: Context, task: ScheduledTask) {
        if (!task.enabled) {
            cancelAlarm(context, task.id)
            return
        }

        val triggerMillis = nextTriggerMillis(task)
        if (triggerMillis <= System.currentTimeMillis()) {
            Log.w(TAG, "Task ${task.id} next trigger is in the past, skipping")
            return
        }

        val alarmManager = context.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        val pendingIntent = createPendingIntent(context, task.id)

        // Android 12+: check exact alarm permission
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            if (alarmManager.canScheduleExactAlarms()) {
                alarmManager.setExactAndAllowWhileIdle(
                    AlarmManager.RTC_WAKEUP,
                    triggerMillis,
                    pendingIntent,
                )
            } else {
                alarmManager.setAndAllowWhileIdle(
                    AlarmManager.RTC_WAKEUP,
                    triggerMillis,
                    pendingIntent,
                )
            }
        } else {
            alarmManager.setExactAndAllowWhileIdle(
                AlarmManager.RTC_WAKEUP,
                triggerMillis,
                pendingIntent,
            )
        }

        Log.i(TAG, "Scheduled alarm for task ${task.id} at $triggerMillis")
    }

    fun cancelAlarm(context: Context, taskId: String) {
        val alarmManager = context.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        alarmManager.cancel(createPendingIntent(context, taskId))
    }

    suspend fun rescheduleAll(context: Context) {
        val store = ScheduledTaskStore(context)
        val tasks = store.tasksAsList().filter { it.enabled }
        for (task in tasks) {
            scheduleAlarm(context, task)
        }
        Log.i(TAG, "Rescheduled ${tasks.size} enabled tasks")
    }

    suspend fun onTaskFired(context: Context, taskId: String) {
        val store = ScheduledTaskStore(context)
        var shouldReschedule = false

        store.updateTaskById(taskId) { current ->
            val updated = current.copy(lastRunAt = System.currentTimeMillis())
            if (current.repeat == TaskRepeat.ONCE) {
                shouldReschedule = false
                updated.copy(enabled = false)
            } else {
                shouldReschedule = true
                updated
            }
        }

        if (shouldReschedule) {
            val task = store.tasksAsList().find { it.id == taskId }
            if (task != null && task.enabled) {
                scheduleAlarm(context, task)
            } else {
                cancelAlarm(context, taskId)
            }
        } else {
            cancelAlarm(context, taskId)
        }
    }

    fun nextTriggerMillis(task: ScheduledTask): Long {
        val now = Calendar.getInstance()
        val target = Calendar.getInstance().apply {
            set(Calendar.HOUR_OF_DAY, task.hour)
            set(Calendar.MINUTE, task.minute)
            set(Calendar.SECOND, 0)
            set(Calendar.MILLISECOND, 0)
        }

        when (task.repeat) {
            TaskRepeat.ONCE, TaskRepeat.DAILY -> {
                if (target.before(now) || target == now) {
                    target.add(Calendar.DAY_OF_YEAR, 1)
                }
            }
            TaskRepeat.WEEKDAY -> {
                while (target.before(now) || target == now || isWeekend(target)) {
                    target.add(Calendar.DAY_OF_YEAR, 1)
                }
            }
        }

        return target.timeInMillis
    }

    private fun isWeekend(cal: Calendar): Boolean {
        val day = cal.get(Calendar.DAY_OF_WEEK)
        return day == Calendar.SATURDAY || day == Calendar.SUNDAY
    }

    private fun createPendingIntent(context: Context, taskId: String): PendingIntent {
        val intent = Intent(context, ClawseedService::class.java).apply {
            putExtra(ClawseedService.EXTRA_TASK_ID, taskId)
        }
        return PendingIntent.getService(
            context,
            taskId.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }
}
