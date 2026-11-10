package dev.clawseed.demo.ui.scheduled

import androidx.compose.foundation.selection.selectableGroup
import dev.clawseed.demo.ui.components.ClawSeedButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import dev.clawseed.demo.ui.components.ClawSeedOutlinedButton
import dev.clawseed.demo.ui.components.ClawSeedTextButton
import dev.clawseed.demo.ui.components.ClawSeedTextField
import dev.clawseed.demo.ui.components.ClawSeedChoiceChip
import dev.clawseed.demo.ui.components.ClawSeedIconButton
import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Warning
import dev.clawseed.demo.ui.components.ClawSeedFormDialog
import dev.clawseed.demo.ui.components.ClawSeedAddAction
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import dev.clawseed.demo.ui.components.ClawSeedTopAppBar
import dev.clawseed.demo.ui.components.ClawSeedSwitch
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.produceState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.theme.success
import dev.clawseed.demo.i18n.label
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.TaskRepeat
import dev.clawseed.demo.scheduled.TaskStatus
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.delay

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScheduledTasksScreen(
    onBack: () -> Unit,
    onRunTask: (ScheduledTask) -> Unit = {},
    onSettings: () -> Unit = {},
    onOpenSession: (String) -> Unit = {},
    viewModel: ScheduledTasksViewModel = viewModel(),
) {
    val tasks by viewModel.tasks.collectAsState()
    val canScheduleExactAlarms by viewModel.canScheduleExactAlarms.collectAsState()
    val context = LocalContext.current
    var showAddDialog by remember { mutableStateOf(false) }
    var editingTask by remember { mutableStateOf<ScheduledTask?>(null) }
    var detailsTaskId by remember { mutableStateOf<String?>(null) }
    val nowMillis by produceState(System.currentTimeMillis()) {
        while (true) {
            value = System.currentTimeMillis()
            delay(30_000)
        }
    }

    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { /* no-op: permission result reflected in next composition */ }

    LaunchedEffect(Unit) {
        viewModel.checkExactAlarmPermission()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val granted = ContextCompat.checkSelfPermission(
                context, Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
            if (!granted) {
                notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
    }

    if (showAddDialog) {
        TaskDialog(
            onDismiss = { showAddDialog = false },
            onConfirm = { task ->
                viewModel.addTask(task)
                showAddDialog = false
            },
        )
    }

    editingTask?.let { task ->
        TaskDialog(
            initialTask = task,
            onDismiss = { editingTask = null },
            onConfirm = { updated ->
                viewModel.updateTask(task.id, updated)
                editingTask = null
            },
        )
    }

    tasks.find { it.id == detailsTaskId }?.let { task ->
        TaskDetailsDialog(task, onDismiss = { detailsTaskId = null }, onOpenSession = {
            detailsTaskId = null
            onOpenSession(it)
        }, onSettings = {
            detailsTaskId = null
            onSettings()
        })
    }

    Scaffold(
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
        topBar = {
            ClawSeedTopAppBar(
                title = { Text(stringResource(R.string.task_title)) },
                navigationIcon = {
                    ClawSeedIconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                    }
                },
            )
        },
        floatingActionButton = {
            ClawSeedAddAction(
                onClick = { showAddDialog = true },
                contentDescription = stringResource(R.string.task_add),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            if (!canScheduleExactAlarms && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                dev.clawseed.demo.ui.components.ClawSeedCard(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    containerColor = MaterialTheme.colorScheme.errorContainer,
                ) {
                    Column(
                        modifier = Modifier.padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Icon(
                            Icons.Default.Warning,
                            contentDescription = null,
                            modifier = Modifier.size(20.dp),
                            tint = MaterialTheme.colorScheme.onErrorContainer,
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            stringResource(R.string.task_alarm_warning),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onErrorContainer,
                        )
                        ClawSeedTextButton(onClick = {
                            val intent = Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM).apply {
                                putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                            }
                            context.startActivity(intent)
                        }) {
                            Text(stringResource(R.string.task_go_settings))
                        }
                    }
                }
            }

            if (tasks.isEmpty()) {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        stringResource(R.string.task_empty),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    contentPadding = PaddingValues(start = 16.dp, top = 16.dp, end = 16.dp, bottom = 96.dp),
                ) {
                    items(tasks, key = { it.id }) { task ->
                        TaskCard(
                            task = task,
                            onToggle = { enabled -> viewModel.toggleTask(task.id, enabled) },
                            onEdit = { editingTask = task },
                            onDelete = { viewModel.deleteTask(task.id) },
                            onRunNow = { onRunTask(task) },
                            onDetails = { detailsTaskId = task.id },
                            nowMillis = nowMillis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun TaskCard(
    task: ScheduledTask,
    onToggle: (Boolean) -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
    onRunNow: () -> Unit,
    onDetails: () -> Unit,
    nowMillis: Long,
) {
    dev.clawseed.demo.ui.components.ClawSeedCard(
        onClick = onDetails,
        modifier = Modifier.fillMaxWidth(),
        muted = !task.enabled,
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        task.name,
                        style = MaterialTheme.typography.titleSmall,
                    )
                    Text(
                        String.format("%02d:%02d", task.hour, task.minute),
                        style = MaterialTheme.typography.headlineSmall,
                    )
                }
                ClawSeedSwitch(
                    checked = task.enabled,
                    onCheckedChange = onToggle,
                )

            }

            Text(
                task.message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )

            val next = nextScheduledRun(task, nowMillis)
            Text(
                when {
                    !task.enabled -> stringResource(R.string.task_paused)
                    next == null -> stringResource(R.string.task_invalid_schedule)
                    else -> stringResource(R.string.task_next_run, formatTaskTime(next))
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 8.dp),
            )

            Spacer(modifier = Modifier.height(6.dp))

            // "Run Now" button (or "正在执行..." text when alarm-triggered RUNNING)
            androidx.compose.foundation.layout.FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (task.lastStatus == TaskStatus.RUNNING) {
                    Text(
                        stringResource(R.string.task_executing),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                } else {
                    ClawSeedOutlinedButton(
                        onClick = onRunNow,
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
                    ) {
                        Icon(
                            Icons.Default.PlayArrow,
                            contentDescription = stringResource(R.string.task_run_now),
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(modifier = Modifier.width(4.dp))
                        Text(stringResource(R.string.task_run_now), style = MaterialTheme.typography.labelSmall)
                    }
                }
                ClawSeedIconButton(onClick = onEdit) {
                    Icon(
                        Icons.Default.Edit,
                        contentDescription = stringResource(R.string.task_edit),
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
                ClawSeedIconButton(onClick = onDelete, dangerous = true) {
                    Icon(
                        Icons.Default.Delete,
                        contentDescription = stringResource(R.string.common_delete),
                        tint = MaterialTheme.colorScheme.error,
                    )
                }
            }

            Spacer(modifier = Modifier.height(4.dp))

            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                val repeatLabel = if (task.repeat == TaskRepeat.CUSTOM) {
                    task.repeatDays.sorted().joinToString(" ") { weekdayLabel(it) }
                } else task.repeat.label()
                Text(repeatLabel, style = MaterialTheme.typography.labelSmall)

                if (task.lastRunAt != null && task.lastStatus != TaskStatus.RUNNING) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            when (task.lastStatus) {
                                TaskStatus.SUCCESS -> Icons.Default.Check
                                TaskStatus.FAILED -> Icons.Default.Close
                                TaskStatus.RUNNING -> Icons.Default.Check
                                null -> Icons.Default.Close
                            },
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = when (task.lastStatus) {
                                TaskStatus.SUCCESS -> MaterialTheme.colorScheme.success
                                TaskStatus.FAILED -> MaterialTheme.colorScheme.error
                                TaskStatus.RUNNING -> MaterialTheme.colorScheme.primary
                                null -> MaterialTheme.colorScheme.onSurfaceVariant
                            },
                        )
                        Spacer(modifier = Modifier.width(4.dp))
                        Text(
                            stringResource(R.string.task_last_run, formatTaskTime(task.lastRunAt)),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }

            if (task.lastError != null) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    task.lastError,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            } else if (task.lastResult != null && task.lastStatus != TaskStatus.RUNNING) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    task.lastResult,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

private fun formatTaskTime(time: Long): String {
    val sdf = SimpleDateFormat("MM/dd E HH:mm", Locale.getDefault())
    return sdf.format(Date(time))
}

@Composable
private fun TaskDetailsDialog(
    task: ScheduledTask,
    onDismiss: () -> Unit,
    onSettings: () -> Unit,
    onOpenSession: (String) -> Unit,
) {
    ClawSeedFormDialog(
        onDismissRequest = onDismiss,
        title = { Text(task.name) },
        text = {
            SelectionContainer {
                Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.task_message_label), style = MaterialTheme.typography.labelMedium)
                    Text(task.message)
                    Text(stringResource(when (task.lastStatus) {
                        TaskStatus.RUNNING -> R.string.task_executing
                        TaskStatus.SUCCESS -> R.string.task_succeeded
                        TaskStatus.FAILED -> R.string.task_failed
                        null -> R.string.task_not_run
                    }), style = MaterialTheme.typography.titleSmall,
                        color = when (task.lastStatus) {
                            TaskStatus.FAILED -> MaterialTheme.colorScheme.error
                            TaskStatus.SUCCESS -> MaterialTheme.colorScheme.success
                            else -> MaterialTheme.colorScheme.primary
                        })
                    task.lastRunAt?.let { Text(stringResource(R.string.task_last_run, formatTaskTime(it))) }
                    if (task.lastStatus == TaskStatus.FAILED) {
                        task.lastError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                    }
                    if (task.lastStatus == TaskStatus.SUCCESS) {
                        task.lastResult?.let {
                            Text(stringResource(R.string.task_result_summary), style = MaterialTheme.typography.labelMedium)
                            Text(it)
                        }
                    }
                    task.sessionId?.takeIf { it.isNotBlank() }?.let { sessionId ->
                        ClawSeedTextButton(onClick = { onOpenSession(sessionId) }) {
                            Text(stringResource(R.string.task_open_session))
                        }
                    }
                    if (task.lastStatus == TaskStatus.FAILED) {
                        Text(stringResource(R.string.task_failure_recovery), style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        },
        confirmButton = {
            ClawSeedButton(onClick = onDismiss) { Text(stringResource(R.string.common_close)) }
        },
        dismissButton = {
            if (task.lastStatus == TaskStatus.FAILED) {
                ClawSeedTextButton(onClick = onSettings) { Text(stringResource(R.string.drawer_settings)) }
            }
        },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TaskDialog(
    onDismiss: () -> Unit,
    onConfirm: (ScheduledTask) -> Unit,
    initialTask: ScheduledTask? = null,
) {
    var name by remember { mutableStateOf(initialTask?.name ?: "") }
    var message by remember { mutableStateOf(initialTask?.message ?: "") }
    var repeat by remember { mutableStateOf(initialTask?.repeat ?: TaskRepeat.DAILY) }
    var repeatDays by remember { mutableStateOf(initialTask?.repeatDays.orEmpty().toSet()) }
    var hour by remember { mutableStateOf(initialTask?.hour ?: 8) }
    var minute by remember { mutableStateOf(initialTask?.minute ?: 0) }
    var showTimePicker by remember { mutableStateOf(false) }
    if (showTimePicker) {
        val timeState = androidx.compose.material3.rememberTimePickerState(hour, minute, is24Hour = true)
        ClawSeedFormDialog(
            onDismissRequest = { showTimePicker = false },
            title = { Text(stringResource(R.string.task_time_label)) },
            text = { androidx.compose.material3.TimeInput(state = timeState) },
            confirmButton = { ClawSeedButton(onClick = {
                hour = timeState.hour
                minute = timeState.minute
                showTimePicker = false
            }) { Text(stringResource(R.string.common_confirm)) } },
            dismissButton = { ClawSeedTextButton(onClick = { showTimePicker = false }) { Text(stringResource(R.string.common_cancel)) } },
        )
        return
    }

    dev.clawseed.demo.ui.components.ClawSeedFormDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (initialTask != null) stringResource(R.string.task_edit_dialog_title) else stringResource(R.string.task_add_dialog_title)) },
        text = {
            Column(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                ClawSeedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text(stringResource(R.string.task_name_label)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                ClawSeedTextField(
                    value = message,
                    onValueChange = { message = it },
                    label = { Text(stringResource(R.string.task_message_label)) },
                    minLines = 2,
                    maxLines = 4,
                    modifier = Modifier.fillMaxWidth(),
                )

                Text(stringResource(R.string.task_time_label), style = MaterialTheme.typography.labelMedium)
                ClawSeedOutlinedButton(onClick = { showTimePicker = true }, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Default.Edit, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.task_clock_time, hour, minute))
                }

                Text(stringResource(R.string.task_repeat_label), style = MaterialTheme.typography.labelMedium)
                FlowRow(modifier = Modifier.selectableGroup(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    TaskRepeat.entries.forEach { mode ->
                        ClawSeedChoiceChip(
                            selected = repeat == mode,
                            onClick = { repeat = mode },
                            label = { Text(mode.label()) },
                        )
                    }
                }
                if (repeat == TaskRepeat.CUSTOM) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        (1..7).forEach { day ->
                            ClawSeedChoiceChip(
                                selected = day in repeatDays,
                                choiceType = dev.clawseed.demo.ui.components.ChoiceType.Multiple,
                                onClick = { repeatDays = if (day in repeatDays) repeatDays - day else repeatDays + day },
                                label = { Text(weekdayLabel(day)) },
                            )
                        }
                    }
                }
            }
        },
        confirmButton = {
            ClawSeedButton(
                onClick = {
                    if (name.isNotBlank() && message.isNotBlank()) {
                        onConfirm(
                            (initialTask ?: ScheduledTask(name = name, message = message, hour = 8, minute = 0)).copy(
                                name = name,
                                message = message,
                                hour = hour,
                                minute = minute,
                                repeat = repeat,
                                repeatDays = if (repeat == TaskRepeat.CUSTOM) repeatDays.sorted() else emptyList(),
                            ),
                        )
                    }
                },
                enabled = name.isNotBlank() && message.isNotBlank() && (repeat != TaskRepeat.CUSTOM || repeatDays.isNotEmpty()),
            ) {
                Text(stringResource(R.string.common_confirm))
            }
        },
        dismissButton = {
            ClawSeedTextButton(onClick = onDismiss) { Text(stringResource(R.string.common_cancel)) }
        },
    )
}

private fun weekdayLabel(day: Int): String = java.time.DayOfWeek.of(day)
    .getDisplayName(java.time.format.TextStyle.SHORT, Locale.getDefault())
