package dev.clawseed.demo

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.navigation.compose.rememberNavController
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import dev.clawseed.demo.ui.drawer.SessionDrawer
import dev.clawseed.demo.ui.navigation.ClawseedNavHost
import dev.clawseed.demo.ui.navigation.Routes
import kotlinx.coroutines.launch
import androidx.compose.ui.platform.LocalContext

@Composable
fun ClawseedApp(localStore: LocalStore, notificationSessionId: androidx.compose.runtime.MutableState<String?> = remember { mutableStateOf(null) }) {
    val drawerState = androidx.compose.material3.rememberDrawerState(androidx.compose.material3.DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val navController = rememberNavController()
    var currentSessionId by rememberSaveable { mutableStateOf<String?>(null) }
    var sessionVersion by rememberSaveable { mutableStateOf(0) }
    var pendingNewSessionPersona by rememberSaveable { mutableStateOf<String?>(null) }
    var hasPendingNewSessionPersona by rememberSaveable { mutableStateOf(false) }
    var refreshKey by remember { mutableStateOf(0) }

    // Auto-send message state for "Run Now" from scheduled tasks
    var pendingAutoMessage by remember { mutableStateOf<String?>(null) }
    var pendingAutoTaskId by remember { mutableStateOf<String?>(null) }
    val context = LocalContext.current

    // Navigate to session from notification tap
    val notifSessionId = notificationSessionId.value
    if (notifSessionId != null) {
        val target = notifSessionId
        notificationSessionId.value = null
        currentSessionId = target
        pendingNewSessionPersona = null
        hasPendingNewSessionPersona = false
        sessionVersion++
        refreshKey++
        scope.launch {
            localStore.setActiveSessionId(target)
            drawerState.close()
            navController.navigate(Routes.CHAT) {
                popUpTo(Routes.CHAT) { inclusive = true }
                launchSingleTop = true
            }
        }
    }

    // Back key: close drawer if open
    BackHandler(enabled = drawerState.isOpen) {
        scope.launch { drawerState.close() }
    }

    fun switchSession(sessionId: String?, persona: String? = null, hasPersona: Boolean = false) {
        currentSessionId = sessionId
        pendingNewSessionPersona = persona
        hasPendingNewSessionPersona = hasPersona
        sessionVersion++
        refreshKey++
        scope.launch {
            localStore.setActiveSessionId(sessionId)
            drawerState.close()
            navController.navigate(Routes.CHAT) {
                popUpTo(Routes.CHAT) { inclusive = true }
                launchSingleTop = true
            }
        }
    }

    fun onRunTask(task: ScheduledTask) {
        pendingAutoMessage = task.message
        pendingAutoTaskId = task.id
        switchSession(task.sessionId)
    }

    fun onAutoMessageSent() {
        pendingAutoMessage = null
    }

    androidx.compose.material3.ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            SessionDrawer(
                currentSessionId = currentSessionId,
                onSelectSession = { sessionId -> switchSession(sessionId) },
                onDeleteCurrentSession = { switchSession(null) },
                onSettings = {
                    scope.launch { drawerState.close() }
                    navController.navigate(Routes.SETTINGS)
                },
                onScheduledTasks = {
                    scope.launch { drawerState.close() }
                    navController.navigate(Routes.SCHEDULED_TASKS)
                },
                onPersonas = {
                    scope.launch { drawerState.close() }
                    navController.navigate(Routes.PERSONAS)
                },
                isDrawerOpen = drawerState.isOpen,
                refreshKey = refreshKey,
            )
        },
    ) {
        ClawseedNavHost(
            navController = navController,
            onToggleDrawer = { scope.launch { drawerState.open() } },
            onNewSession = { persona -> switchSession(null, persona, true) },
            currentSessionId = currentSessionId,
            onSessionIdChanged = { id ->
                if (hasPendingNewSessionPersona && id != null) {
                    return@ClawseedNavHost
                }
                currentSessionId = id
                refreshKey++
                // Update scheduled task sessionId if auto-run is pending
                val taskId = pendingAutoTaskId
                if (taskId != null && id != null) {
                    pendingAutoTaskId = null
                    scope.launch {
                        ScheduledTaskStore(context).updateTaskById(taskId) { it.copy(sessionId = id) }
                    }
                }
                scope.launch { localStore.setActiveSessionId(id) }
            },
            onSessionEstablished = {
                refreshKey++
            },
            sessionVersion = sessionVersion,
            newSessionPersona = pendingNewSessionPersona,
            hasNewSessionPersona = hasPendingNewSessionPersona,
            onNewSessionPersonaConsumed = {
                pendingNewSessionPersona = null
                hasPendingNewSessionPersona = false
            },
            localStore = localStore,
            pendingAutoMessage = pendingAutoMessage,
            onAutoMessageSent = { onAutoMessageSent() },
            onRunTask = { task -> onRunTask(task) },
        )
    }
}
