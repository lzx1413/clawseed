package dev.clawseed.demo.ui.scheduled

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

class ScheduledTasksViewModel(application: Application) : AndroidViewModel(application) {

    private val store = ScheduledTaskStore(application)

    val tasks: StateFlow<List<ScheduledTask>> = store.tasks
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyList())

    private val _canScheduleExactAlarms = kotlinx.coroutines.flow.MutableStateFlow(true)
    val canScheduleExactAlarms: StateFlow<Boolean> = _canScheduleExactAlarms.asStateFlow()

    fun checkExactAlarmPermission() {
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
            val alarmManager = getApplication<Application>().getSystemService(android.content.Context.ALARM_SERVICE) as android.app.AlarmManager
            _canScheduleExactAlarms.value = alarmManager.canScheduleExactAlarms()
        }
    }

    fun addTask(task: ScheduledTask) {
        viewModelScope.launch {
            store.addTask(task)
            ScheduledTaskManager.scheduleAlarm(getApplication(), task)
        }
    }

    fun deleteTask(taskId: String) {
        viewModelScope.launch {
            ScheduledTaskManager.cancelAlarm(getApplication(), taskId)
            store.deleteTask(taskId)
        }
    }

    fun updateTask(taskId: String, updated: ScheduledTask) {
        viewModelScope.launch {
            ScheduledTaskManager.cancelAlarm(getApplication(), taskId)
            store.updateTaskById(taskId) { updated.copy(id = taskId) }
            val task = store.tasksAsList().find { it.id == taskId }
            if (task != null && task.enabled) {
                ScheduledTaskManager.scheduleAlarm(getApplication(), task)
            }
        }
    }

    fun toggleTask(taskId: String, enabled: Boolean) {
        viewModelScope.launch {
            store.updateTaskById(taskId) { it.copy(enabled = enabled) }
            val task = store.tasksAsList().find { it.id == taskId }
            if (task != null) {
                if (task.enabled) {
                    ScheduledTaskManager.scheduleAlarm(getApplication(), task)
                } else {
                    ScheduledTaskManager.cancelAlarm(getApplication(), taskId)
                }
            }
        }
    }
}