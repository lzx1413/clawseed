package dev.clawseed.demo.ui.navigation

import androidx.compose.runtime.Composable
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.ui.chat.ChatScreen
import dev.clawseed.demo.ui.persona.PersonaManagerScreen
import dev.clawseed.demo.ui.scheduled.ScheduledTasksScreen
import dev.clawseed.demo.ui.settings.SettingsScreen

object Routes {
    const val CHAT = "chat"
    const val SETTINGS = "settings"
    const val SCHEDULED_TASKS = "scheduled_tasks"
    const val PERSONAS = "personas"
}

@Composable
fun ClawseedNavHost(
    navController: NavHostController,
    onToggleDrawer: () -> Unit,
    onNewSession: (String?) -> Unit = {},
    currentSessionId: String? = null,
    onSessionIdChanged: (String?) -> Unit = {},
    onSessionEstablished: () -> Unit = {},
    sessionVersion: Int = 0,
    newSessionPersona: String? = null,
    hasNewSessionPersona: Boolean = false,
    onNewSessionPersonaConsumed: () -> Unit = {},
    localStore: LocalStore? = null,
    pendingAutoMessage: String? = null,
    onAutoMessageSent: () -> Unit = {},
    onRunTask: (ScheduledTask) -> Unit = {},
) {
    NavHost(
        navController = navController,
        startDestination = Routes.CHAT,
    ) {
        composable(Routes.CHAT) {
            ChatScreen(
                onToggleDrawer = onToggleDrawer,
                onNewSession = onNewSession,
                sessionId = currentSessionId,
                onSessionIdChanged = onSessionIdChanged,
                onSessionEstablished = onSessionEstablished,
                sessionVersion = sessionVersion,
                newSessionPersona = newSessionPersona,
                hasNewSessionPersona = hasNewSessionPersona,
                onNewSessionPersonaConsumed = onNewSessionPersonaConsumed,
                onManagePersonas = { navController.navigate(Routes.PERSONAS) },
                autoSendMessage = pendingAutoMessage,
                onAutoMessageSent = onAutoMessageSent,
            )
        }
        composable(Routes.SETTINGS) {
            SettingsScreen(onBack = { navController.popBackStack() }, localStore = localStore)
        }
        composable(Routes.SCHEDULED_TASKS) {
            ScheduledTasksScreen(
                onBack = { navController.popBackStack() },
                onRunTask = onRunTask,
            )
        }
        composable(Routes.PERSONAS) {
            PersonaManagerScreen(
                onBack = { navController.popBackStack() },
                onStartChat = { persona ->
                    onNewSession(persona)
                },
            )
        }
    }
}
