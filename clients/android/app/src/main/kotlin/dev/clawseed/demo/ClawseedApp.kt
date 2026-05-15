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
import dev.clawseed.demo.ui.drawer.SessionDrawer
import dev.clawseed.demo.ui.navigation.ClawseedNavHost
import dev.clawseed.demo.ui.navigation.Routes
import kotlinx.coroutines.launch

@Composable
fun ClawseedApp(localStore: LocalStore, notificationSessionId: androidx.compose.runtime.MutableState<String?> = remember { mutableStateOf(null) }) {
    val drawerState = androidx.compose.material3.rememberDrawerState(androidx.compose.material3.DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val navController = rememberNavController()
    var currentSessionId by rememberSaveable { mutableStateOf<String?>(null) }
    var sessionVersion by rememberSaveable { mutableStateOf(0) }
    var refreshKey by remember { mutableStateOf(0) }

    // Navigate to session from notification tap
    val notifSessionId = notificationSessionId.value
    androidx.compose.runtime.LaunchedEffect(notifSessionId) {
        if (notifSessionId != null && notifSessionId != currentSessionId) {
            currentSessionId = notifSessionId
            sessionVersion++
            refreshKey++
            localStore.setActiveSessionId(notifSessionId)
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

    fun switchSession(sessionId: String?) {
        currentSessionId = sessionId
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
                isDrawerOpen = drawerState.isOpen,
                refreshKey = refreshKey,
            )
        },
    ) {
        ClawseedNavHost(
            navController = navController,
            onToggleDrawer = { scope.launch { drawerState.open() } },
            onNewSession = { switchSession(null) },
            currentSessionId = currentSessionId,
            onSessionIdChanged = { id ->
                currentSessionId = id
                refreshKey++
                scope.launch { localStore.setActiveSessionId(id) }
            },
            onSessionEstablished = {
                refreshKey++
            },
            sessionVersion = sessionVersion,
            localStore = localStore,
        )
    }
}
