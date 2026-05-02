package dev.clawseed.demo

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.navigation.compose.rememberNavController
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.ui.drawer.SessionDrawer
import dev.clawseed.demo.ui.navigation.ClawseedNavHost
import dev.clawseed.demo.ui.navigation.Routes
import kotlinx.coroutines.launch

@Composable
fun ClawseedApp(localStore: LocalStore) {
    val drawerState = androidx.compose.material3.rememberDrawerState(androidx.compose.material3.DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val navController = rememberNavController()
    var currentSessionId by remember { mutableStateOf<String?>(null) }
    var sessionVersion by remember { mutableStateOf(0) }
    var refreshKey by remember { mutableStateOf(0) }

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
                onNewSession = { switchSession(null) },
                onSelectSession = { sessionId -> switchSession(sessionId) },
                onSettings = {
                    scope.launch { drawerState.close() }
                    navController.navigate(Routes.SETTINGS)
                },
                isDrawerOpen = drawerState.isOpen,
                refreshKey = refreshKey,
            )
        },
    ) {
        ClawseedNavHost(
            navController = navController,
            onToggleDrawer = { scope.launch { drawerState.open() } },
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
        )
    }
}
