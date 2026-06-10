package dev.clawseed.demo.scheduled

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

private val Context.taskDataStore: DataStore<Preferences> by preferencesDataStore(name = "scheduled_tasks_prefs")

class ScheduledTaskStore(private val context: Context) {

    private val store get() = context.taskDataStore
    private val mutex = Mutex()
    private val json = Json { ignoreUnknownKeys = true }

    private val KEY_TASKS = stringPreferencesKey("tasks_json")

    val tasks: Flow<List<ScheduledTask>> = store.data.map { prefs ->
        val raw = prefs[KEY_TASKS] ?: "[]"
        runCatching { json.decodeFromString<List<ScheduledTask>>(raw) }.getOrDefault(emptyList())
    }

    suspend fun tasksAsList(): List<ScheduledTask> = tasks.first()

    /** Ensure built-in default tasks exist (idempotent — skips if already seeded). */
    suspend fun ensureDefaultTasks(context: android.content.Context) {
        val current = tasksAsList()
        val curatorExists = current.any { it.id == "memory_curator" }
        if (!curatorExists) {
            val task = ScheduledTask(
                id = "memory_curator",
                name = "记忆整理",
                message = "分析所有记忆，删除不重要的和重复的，合并冲突，每条不超过50字摘要",
                hour = 21,
                minute = 0,
                repeat = TaskRepeat.DAILY,
                enabled = true,
            )
            addTask(task)
            ScheduledTaskManager.scheduleAlarm(context, task)
        }
    }

    suspend fun addTask(task: ScheduledTask) = mutex.withLock {
        val current = tasksAsList().toMutableList()
        current.add(task)
        saveTasks(current)
    }

    suspend fun deleteTask(taskId: String) = mutex.withLock {
        val current = tasksAsList().filter { it.id != taskId }
        saveTasks(current)
    }

    suspend fun updateTaskById(
        taskId: String,
        transform: (ScheduledTask) -> ScheduledTask?,
    ) = mutex.withLock {
        val current = tasksAsList()
        val updated = current.map { task ->
            if (task.id == taskId) transform(task) ?: task else task
        }
        saveTasks(updated)
    }

    private suspend fun saveTasks(tasks: List<ScheduledTask>) {
        store.edit { prefs ->
            prefs[KEY_TASKS] = json.encodeToString(tasks)
        }
    }
}
