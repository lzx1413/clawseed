package dev.clawseed.demo.ui.navigation

import androidx.compose.runtime.Composable
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import dev.clawseed.demo.ui.chat.ChatScreen
import dev.clawseed.demo.ui.settings.SettingsScreen

object Routes {
    const val CHAT = "chat"
    const val SETTINGS = "settings"
}

@Composable
fun ClawseedNavHost(
    navController: NavHostController,
    onToggleDrawer: () -> Unit,
    currentSessionId: String? = null,
    onSessionIdChanged: (String?) -> Unit = {},
    onSessionEstablished: () -> Unit = {},
    sessionVersion: Int = 0,
) {
    NavHost(
        navController = navController,
        startDestination = Routes.CHAT,
    ) {
        composable(Routes.CHAT) {
            ChatScreen(
                onToggleDrawer = onToggleDrawer,
                sessionId = currentSessionId,
                onSessionIdChanged = onSessionIdChanged,
                onSessionEstablished = onSessionEstablished,
                sessionVersion = sessionVersion,
            )
        }
        composable(Routes.SETTINGS) {
            SettingsScreen(onBack = { navController.popBackStack() })
        }
    }
}
