package dev.clawseed.demo.ui.navigation

import androidx.compose.runtime.Composable
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import dev.clawseed.demo.ui.components.GlassBackdropHost
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.ui.chat.ChatScreen
import dev.clawseed.demo.ui.persona.PersonaManagerScreen
import dev.clawseed.demo.ui.profile.UserProfileScreen
import dev.clawseed.demo.ui.scheduled.ScheduledTasksScreen
import dev.clawseed.demo.ui.settings.SettingsScreen

object Routes {
    const val CHAT = "chat"
    const val SETTINGS = "settings"
    const val SCHEDULED_TASKS = "scheduled_tasks"
    const val PERSONAS = "personas"
    const val USER_PROFILE = "user_profile"
    const val PERSONA_NAME_ARG = "name"
    const val PERSONA_DETAIL = "personas/{$PERSONA_NAME_ARG}"

    fun personaDetail(name: String): String =
        "personas/${java.net.URLEncoder.encode(name, "UTF-8")}"
}

@Composable
fun ClawseedNavHost(
    navController: NavHostController,
    onToggleDrawer: () -> Unit,
    onNewSession: (String?) -> Unit = {},
    defaultPersonaName: String? = null,
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
    onOpenTaskSession: (String) -> Unit = {},
) {
    NavHost(
        navController = navController,
        startDestination = Routes.CHAT,
    ) {
        composable(Routes.CHAT) {
            ChatScreen(
                onToggleDrawer = onToggleDrawer,
                onNewSession = onNewSession,
                defaultPersonaName = defaultPersonaName,
                sessionId = currentSessionId,
                onSessionIdChanged = onSessionIdChanged,
                onSessionEstablished = onSessionEstablished,
                sessionVersion = sessionVersion,
                newSessionPersona = newSessionPersona,
                hasNewSessionPersona = hasNewSessionPersona,
                onNewSessionPersonaConsumed = onNewSessionPersonaConsumed,
                onOpenPersona = { persona -> navController.navigate(Routes.personaDetail(persona)) },
                autoSendMessage = pendingAutoMessage,
                onAutoMessageSent = onAutoMessageSent,
            )
        }
        composable(Routes.SETTINGS) {
            AppPageBackdrop { SettingsScreen(
                onBack = { navController.popBackStack() },
                onManageUserProfile = { navController.navigate(Routes.USER_PROFILE) },
                localStore = localStore,
            ) }
        }
        composable(Routes.USER_PROFILE) {
            AppPageBackdrop { UserProfileScreen(onBack = { navController.popBackStack() }) }
        }
        composable(Routes.SCHEDULED_TASKS) {
            AppPageBackdrop { ScheduledTasksScreen(
                onBack = { navController.popBackStack() },
                onRunTask = onRunTask,
                onSettings = { navController.navigate(Routes.SETTINGS) },
                onOpenSession = onOpenTaskSession,
            ) }
        }
        composable(Routes.PERSONAS) {
            AppPageBackdrop { PersonaManagerScreen(
                onBack = { navController.popBackStack() },
                onStartChat = { persona ->
                    onNewSession(persona)
                },
            ) }
        }
        composable(Routes.PERSONA_DETAIL) { backStackEntry ->
            val encoded = backStackEntry.arguments?.getString(Routes.PERSONA_NAME_ARG).orEmpty()
            AppPageBackdrop { PersonaManagerScreen(
                onBack = { navController.popBackStack() },
                onStartChat = { persona ->
                    onNewSession(persona)
                },
                initialPersona = java.net.URLDecoder.decode(encoded, "UTF-8"),
            ) }
        }
    }
}

@Composable
private fun AppPageBackdrop(content: @Composable () -> Unit) {
    val colors = MaterialTheme.colorScheme
    GlassBackdropHost(
        modifier = Modifier.fillMaxSize(),
        background = {
            androidx.compose.foundation.layout.Box(
                Modifier.fillMaxSize().background(colors.background).background(Brush.verticalGradient(listOf(
                    colors.background, colors.primaryContainer.copy(alpha = 0.45f), colors.background,
                )))
            )
        },
        content = { content() },
    )
}
