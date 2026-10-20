package dev.clawseed.demo.datatransfer

import android.content.Context
import dev.clawseed.demo.R
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import dev.clawseed.demo.scheduled.TaskRepeat

enum class WebDavAutoBackupFrequency { DAILY, WEEKLY }

data class WebDavAutoBackupSettings(
    val enabled: Boolean = false,
    val frequency: WebDavAutoBackupFrequency = WebDavAutoBackupFrequency.DAILY,
    val wifiOnly: Boolean = true,
    val retainCount: Int = 5,
    val lastSuccessAt: Long? = null,
    val lastFailure: String? = null,
    val retryCount: Int = 0,
) {
    fun normalized(): WebDavAutoBackupSettings = copy(retainCount = retainCount.coerceIn(1, 100))
}

class WebDavAutoBackupStore(context: Context) {
    private val preferences = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    fun load(): WebDavAutoBackupSettings = WebDavAutoBackupSettings(
        enabled = preferences.getBoolean(KEY_ENABLED, false),
        frequency = runCatching {
            WebDavAutoBackupFrequency.valueOf(preferences.getString(KEY_FREQUENCY, "DAILY") ?: "DAILY")
        }.getOrDefault(WebDavAutoBackupFrequency.DAILY),
        wifiOnly = preferences.getBoolean(KEY_WIFI_ONLY, true),
        retainCount = preferences.getInt(KEY_RETAIN_COUNT, 5).coerceIn(1, 100),
        lastSuccessAt = preferences.getLong(KEY_LAST_SUCCESS, 0L).takeIf { it > 0 },
        lastFailure = preferences.getString(KEY_LAST_FAILURE, null),
        retryCount = preferences.getInt(KEY_RETRY_COUNT, 0),
    )

    fun save(settings: WebDavAutoBackupSettings) {
        val value = settings.normalized()
        preferences.edit()
            .putBoolean(KEY_ENABLED, value.enabled)
            .putString(KEY_FREQUENCY, value.frequency.name)
            .putBoolean(KEY_WIFI_ONLY, value.wifiOnly)
            .putInt(KEY_RETAIN_COUNT, value.retainCount)
            .apply()
    }

    fun recordSuccess() = preferences.edit()
        .putLong(KEY_LAST_SUCCESS, System.currentTimeMillis())
        .remove(KEY_LAST_FAILURE)
        .putInt(KEY_RETRY_COUNT, 0)
        .apply()

    fun recordFailure(message: String) = preferences.edit()
        .putString(KEY_LAST_FAILURE, message.take(300))
        .putInt(KEY_RETRY_COUNT, preferences.getInt(KEY_RETRY_COUNT, 0) + 1)
        .apply()

    companion object {
        private const val NAME = "webdav_auto_backup"
        private const val KEY_ENABLED = "enabled"
        private const val KEY_FREQUENCY = "frequency"
        private const val KEY_WIFI_ONLY = "wifi_only"
        private const val KEY_RETAIN_COUNT = "retain_count"
        private const val KEY_LAST_SUCCESS = "last_success"
        private const val KEY_LAST_FAILURE = "last_failure"
        private const val KEY_RETRY_COUNT = "retry_count"
    }
}

object WebDavAutoBackupScheduler {
    const val TASK_ID = "webdav_auto_backup"
    private const val HOUR = 3
    private const val MINUTE = 0

    suspend fun apply(context: Context, settings: WebDavAutoBackupSettings) {
        val store = ScheduledTaskStore(context)
        val current = store.tasksAsList().find { it.id == TASK_ID }
        if (!settings.enabled) {
            ScheduledTaskManager.cancelAlarm(context, TASK_ID)
            if (current != null) store.deleteTask(TASK_ID)
            return
        }
        val task = (current ?: ScheduledTask(
            id = TASK_ID,
            name = context.getString(R.string.data_webdav_auto_task_name),
            message = "",
            hour = HOUR,
            minute = MINUTE,
        )).copy(
            hour = HOUR,
            minute = MINUTE,
            repeat = if (settings.frequency == WebDavAutoBackupFrequency.DAILY) TaskRepeat.DAILY else TaskRepeat.CUSTOM,
            repeatDays = if (settings.frequency == WebDavAutoBackupFrequency.WEEKLY) {
                listOf(java.util.Calendar.getInstance().get(java.util.Calendar.DAY_OF_WEEK).let { (it + 5) % 7 + 1 })
            } else emptyList(),
            enabled = true,
            isWebDavBackup = true,
        )
        if (current == null) store.addTask(task) else store.updateTaskById(TASK_ID) { task }
        ScheduledTaskManager.scheduleAlarm(context, task)
    }
}

fun webDavBackupFileName(categories: Set<DataCategory>, now: java.util.Date = java.util.Date()): String {
    val date = java.text.SimpleDateFormat("yyyyMMdd", java.util.Locale.getDefault()).format(now)
    val names = categories.joinToString("-") { it.name.lowercase() }
    return "clawseed-$date-$names.zip"
}

fun webDavRemoteBackupPath(
    directory: String,
    categories: Set<DataCategory>,
    now: java.util.Date = java.util.Date(),
): String = listOf(WebDavSettingsStore.normalizeDirectory(directory), webDavBackupFileName(categories, now)).joinToString("/")
