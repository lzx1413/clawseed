package dev.clawseed.demo.ui.chat

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Snackbar
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.ui.chat.components.ChatBottomBar
import dev.clawseed.demo.ui.chat.components.MessageBubble

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(
    onToggleDrawer: () -> Unit,
    onNewSession: () -> Unit = {},
    sessionId: String? = null,
    onSessionIdChanged: (String?) -> Unit = {},
    onSessionEstablished: () -> Unit = {},
    sessionVersion: Int = 0,
) {
    val viewModel: ChatViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    var input by remember { mutableStateOf("") }
    val density = LocalDensity.current
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    var bottomBarHeightPx by remember { mutableStateOf(0) }

    fun dismissInput() {
        focusManager.clearFocus(force = true)
        keyboardController?.hide()
    }

    val locationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { /* granted or denied — tool handler checks at call time */ }

    LaunchedEffect(Unit) {
        locationPermissionLauncher.launch(
            arrayOf(
                Manifest.permission.ACCESS_FINE_LOCATION,
                Manifest.permission.ACCESS_COARSE_LOCATION,
            )
        )
    }

    val listState = rememberLazyListState()
    val isStreaming = uiState.streamingContent.isNotEmpty() || uiState.thinkingContent.isNotEmpty()
    val imeBottom = WindowInsets.ime.getBottom(density)
    val bottomContentPadding = with(density) { bottomBarHeightPx.toDp() + 8.dp }

    // Only auto-scroll if user is near the bottom
    val isNearBottom by remember {
        derivedStateOf {
            val lastVisible = listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
            val totalItems = listState.layoutInfo.totalItemsCount
            totalItems == 0 || lastVisible >= totalItems - 2
        }
    }

    // Switch session only on explicit user action (version bump)
    LaunchedEffect(sessionVersion) {
        viewModel.switchToSession(sessionId)
    }

    // Propagate session ID changes
    LaunchedEffect(uiState.currentSessionId) {
        if (uiState.currentSessionId != null && uiState.currentSessionId != sessionId) {
            onSessionIdChanged(uiState.currentSessionId)
            onSessionEstablished()
        }
    }

    // Auto-scroll to bottom on new messages (only when user is near bottom)
    LaunchedEffect(uiState.messages.size, uiState.streamingContent, uiState.thinkingContent) {
        if (isNearBottom && (uiState.messages.isNotEmpty() || uiState.streamingContent.isNotEmpty() || uiState.thinkingContent.isNotEmpty())) {
            val totalItems = uiState.messages.size +
                (if (uiState.thinkingContent.isNotEmpty()) 1 else 0) +
                (if (uiState.streamingContent.isNotEmpty()) 1 else 0)
            if (totalItems > 0) {
                listState.animateScrollToItem(totalItems - 1)
            }
        }
    }

    LaunchedEffect(imeBottom) {
        if (!isNearBottom || imeBottom <= 0) {
            return@LaunchedEffect
        }

        val totalItems = uiState.messages.size +
            (if (uiState.thinkingContent.isNotEmpty()) 1 else 0) +
            (if (uiState.streamingContent.isNotEmpty()) 1 else 0)
        if (totalItems > 0) {
            listState.animateScrollToItem(totalItems - 1)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(uiState.sessionName ?: "新对话") },
                navigationIcon = {
                    IconButton(onClick = {
                        dismissInput()
                        onToggleDrawer()
                    }) {
                        Icon(Icons.Default.Menu, contentDescription = "菜单")
                    }
                },
                actions = {
                    IconButton(onClick = {
                        dismissInput()
                        onNewSession()
                    }) {
                        Icon(Icons.Default.Add, contentDescription = "新建对话")
                    }
                },
            )
        },
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .imePadding(),
        ) {
            LazyColumn(
                state = listState,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentPadding = PaddingValues(
                    start = 12.dp,
                    top = 8.dp,
                    end = 12.dp,
                    bottom = bottomContentPadding,
                ),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(
                    items = uiState.messages,
                    key = { it.id },
                ) { entry ->
                    val isLastAssistant = entry is ChatEntry.AssistantMessage
                        && !entry.isStreaming
                        && uiState.messages.lastOrNull()
                            ?.let { it is ChatEntry.AssistantMessage || it is ChatEntry.ToolResult || it is ChatEntry.ToolCall || it is ChatEntry.Thinking }
                            ?: false
                        && uiState.messages.indexOf(entry) == uiState.messages.indexOfLast { it is ChatEntry.AssistantMessage }
                    MessageBubble(
                        entry = entry,
                        onRegenerate = if (isLastAssistant) ({ viewModel.regenerateLastResponse() }) else null,
                    )
                }
                if (uiState.thinkingContent.isNotEmpty()) {
                    item(key = "__thinking__") {
                        MessageBubble(
                            entry = ChatEntry.Thinking(
                                id = "__thinking__",
                                timestamp = System.currentTimeMillis(),
                                content = uiState.thinkingContent,
                            )
                        )
                    }
                }
                if (uiState.streamingContent.isNotEmpty()) {
                    item(key = "__streaming__") {
                        MessageBubble(
                            entry = ChatEntry.AssistantMessage(
                                id = "__streaming__",
                                timestamp = System.currentTimeMillis(),
                                content = uiState.streamingContent,
                                isStreaming = true,
                            )
                        )
                    }
                }
            }

            // Error banner
            if (uiState.error != null) {
                Snackbar(
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    action = {
                        TextButton(onClick = { viewModel.switchToSession(sessionId) }) {
                            Text("重试")
                        }
                    },
                    dismissAction = {
                        TextButton(onClick = { viewModel.clearError() }) {
                            Text("关闭")
                        }
                    },
                ) {
                    Text(uiState.error!!, maxLines = 2)
                }
            }

            ChatBottomBar(
                input = input,
                onInputChange = { input = it },
                onSend = {
                    viewModel.sendMessage(input)
                    input = ""
                    dismissInput()
                },
                onStop = if (isStreaming) ({ viewModel.abortGeneration() }) else null,
                canSend = uiState.connState == ConnectionState.CONNECTED,
                modifier = Modifier.onSizeChanged { bottomBarHeightPx = it.height },
            )
        }
    }
}
