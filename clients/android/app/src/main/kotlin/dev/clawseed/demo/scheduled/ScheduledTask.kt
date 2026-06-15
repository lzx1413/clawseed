package dev.clawseed.demo.scheduled

import androidx.annotation.StringRes
import dev.clawseed.demo.R
import kotlinx.serialization.Serializable
import java.util.UUID

@Serializable
data class ScheduledTask(
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val message: String,
    val hour: Int,
    val minute: Int,
    val repeat: TaskRepeat = TaskRepeat.DAILY,
    val enabled: Boolean = true,
    val sessionId: String? = null,
    val lastRunAt: Long? = null,
    val lastStatus: TaskStatus? = null,
    val lastResult: String? = null,
    val lastError: String? = null,
    val isAlarm: Boolean = false,
)

@Serializable
enum class TaskRepeat(@StringRes val labelRes: Int) {
    ONCE(R.string.enum_task_repeat_once),
    DAILY(R.string.enum_task_repeat_daily),
    WEEKDAY(R.string.enum_task_repeat_weekday),
}

@Serializable
enum class TaskStatus { RUNNING, SUCCESS, FAILED }
