package dev.clawseed.demo.ui.chat

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.activity.ComponentActivity
import androidx.activity.compose.LocalActivity
import androidx.compose.ui.res.stringResource
import dev.clawseed.demo.R
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.ui.chat.components.ChatBottomBar
import dev.clawseed.demo.ui.chat.components.MessageBubble
import dev.clawseed.demo.ui.chat.components.SpeakerOffIcon
import dev.clawseed.demo.ui.chat.components.SpeakerStopIcon
import dev.clawseed.demo.ui.chat.components.SpeakerPlayIcon
import dev.clawseed.demo.ui.persona.PersonaDot
import dev.clawseed.demo.ui.persona.personaContainerColor
import dev.clawseed.demo.ui.persona.personaContentColor

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(
    onToggleDrawer: () -> Unit,
    onNewSession: (String?) -> Unit = {},
    sessionId: String? = null,
    onSessionIdChanged: (String?) -> Unit = {},
    onSessionEstablished: () -> Unit = {},
    sessionVersion: Int = 0,
    newSessionPersona: String? = null,
    hasNewSessionPersona: Boolean = false,
    onNewSessionPersonaConsumed: () -> Unit = {},
    onManagePersonas: () -> Unit = {},
    onOpenPersona: (String) -> Unit = {},
    autoSendMessage: String? = null,
    onAutoMessageSent: () -> Unit = {},
) {
    val activity = checkNotNull(LocalActivity.current as? ComponentActivity)
    val viewModel: ChatViewModel = viewModel(activity)
    val uiState by viewModel.uiState.collectAsState()
    val lifecycleOwner = LocalLifecycleOwner.current
    var input by remember { mutableStateOf("") }
    val density = LocalDensity.current
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    var bottomBarHeightPx by remember { mutableStateOf(0) }
    var showPersonaSheet by remember { mutableStateOf(false) }

    fun dismissInput() {
        focusManager.clearFocus(force = true)
        keyboardController?.hide()
    }

    val locationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { /* granted or denied — tool handler checks at call time */ }

    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) {
                viewModel.refreshPersonaVisuals()
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
        }
    }

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
    val displayedItemCount = uiState.messages.size +
        (if (uiState.thinkingContent.isNotEmpty()) 1 else 0) +
        (if (uiState.streamingContent.isNotEmpty()) 1 else 0)
    val bottomAnchorIndex = displayedItemCount
    // Track whether user just sent a message — isStreaming arrives asynchronously
    // (one composition frame late), causing a blank gap between send→stop button.
    // This local flag bridges that gap and clears itself once isStreaming arrives.
    var justSent by remember { mutableStateOf(false) }
    if (isStreaming) justSent = false
    val isLoading = isStreaming || justSent
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

    // Auto-send pending message from scheduled task "Run Now"
    LaunchedEffect(autoSendMessage) {
        if (autoSendMessage != null) {
            // Wait for connection before sending
            while (uiState.connState != ConnectionState.CONNECTED) {
                kotlinx.coroutines.delay(200)
            }
            viewModel.sendMessage(autoSendMessage)
            onAutoMessageSent()
        }
    }

    // Switch session only on explicit user action (version bump).
    // When starting a new session, App may carry a one-shot persona request.
    // Resume from the drawer leaves hasNewSessionPersona=false so the gateway's
    // stored binding is authoritative.
    var scrollToLatestAfterSessionSwitch by remember { mutableStateOf(false) }
    LaunchedEffect(sessionVersion) {
        scrollToLatestAfterSessionSwitch = true
        val persona = if (hasNewSessionPersona) newSessionPersona else null
        viewModel.switchToSession(sessionId, persona)
        if (hasNewSessionPersona) onNewSessionPersonaConsumed()
    }

    // Propagate session ID changes
    LaunchedEffect(uiState.currentSessionId) {
        if (uiState.currentSessionId != null && uiState.currentSessionId != sessionId) {
            onSessionIdChanged(uiState.currentSessionId)
            onSessionEstablished()
        }
    }

    // Opening a stored session should land on the latest reply even though the
    // list state starts at the top before history messages are measured.
    val isSessionContentReadyForInitialScroll = if (sessionId == null) {
        uiState.currentSessionId == null
    } else {
        uiState.currentSessionId == sessionId
    }
    LaunchedEffect(
        sessionVersion,
        displayedItemCount,
        isSessionContentReadyForInitialScroll,
    ) {
        if (
            scrollToLatestAfterSessionSwitch &&
            isSessionContentReadyForInitialScroll &&
            displayedItemCount > 0
        ) {
            listState.scrollToItem(bottomAnchorIndex)
            scrollToLatestAfterSessionSwitch = false
        }
    }

    // Auto-scroll to bottom on new messages (only when user is near bottom)
    LaunchedEffect(uiState.messages.size, uiState.streamingContent, uiState.thinkingContent) {
        if (isNearBottom && (uiState.messages.isNotEmpty() || uiState.streamingContent.isNotEmpty() || uiState.thinkingContent.isNotEmpty())) {
            if (displayedItemCount > 0) {
                listState.animateScrollToItem(bottomAnchorIndex)
            }
        }
    }

    LaunchedEffect(imeBottom) {
        if (!isNearBottom || imeBottom <= 0) {
            return@LaunchedEffect
        }

        if (displayedItemCount > 0) {
            listState.animateScrollToItem(bottomAnchorIndex)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (!uiState.currentPersona.isNullOrEmpty()) {
                            val personaName = uiState.currentPersona!!
                            val personaVisual = uiState.personaVisuals[personaName]
                            AssistChip(
                                onClick = { onOpenPersona(personaName) },
                                label = { Text(personaName) },
                                leadingIcon = {
                                    PersonaDot(
                                        personaName,
                                        Modifier.size(20.dp),
                                        showInitial = true,
                                        avatar = personaVisual?.avatar,
                                        color = personaVisual?.color,
                                    )
                                },
                                colors = AssistChipDefaults.assistChipColors(
                                    containerColor = personaContainerColor(personaName, personaVisual?.color),
                                    labelColor = personaContentColor(personaName, personaVisual?.color),
                                ),
                                modifier = Modifier.padding(end = 8.dp),
                            )
                        }
                        Text(
                            text = uiState.sessionName ?: stringResource(R.string.chat_new_conversation),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = {
                        dismissInput()
                        onToggleDrawer()
                    }) {
                        Icon(Icons.Default.Menu, contentDescription = stringResource(R.string.chat_menu))
                    }
                },
                actions = {
                    IconButton(onClick = {
                        if (uiState.speechOutputEnabled && uiState.isSpeaking) {
                            viewModel.stopSpeech()
                        } else {
                            viewModel.toggleSpeechOutput()
                        }
                    }) {
                        Icon(
                            imageVector = when {
                                uiState.speechOutputEnabled && uiState.isSpeaking -> SpeakerStopIcon
                                uiState.speechOutputEnabled -> SpeakerPlayIcon
                                else -> SpeakerOffIcon
                            },
                            contentDescription = stringResource(R.string.chat_speech_output),
                        )
                    }
                    IconButton(onClick = {
                        dismissInput()
                        showPersonaSheet = true
                    }) {
                        Icon(Icons.Default.Add, contentDescription = stringResource(R.string.chat_new_session))
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
                    top = 8.dp,
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
                            ?.let { it is ChatEntry.AssistantMessage || it is ChatEntry.ToolInvocations || it is ChatEntry.Thinking }
                            ?: false
                        && uiState.messages.indexOf(entry) == uiState.messages.indexOfLast { it is ChatEntry.AssistantMessage }
                    val canSpeak = entry is ChatEntry.AssistantMessage && !entry.isStreaming
                    MessageBubble(
                        entry = entry,
                        onRegenerate = if (isLastAssistant) ({ viewModel.regenerateLastResponse() }) else null,
                        onSpeak = if (canSpeak) ({ viewModel.speakMessage(entry.content, entry.id) }) else null,
                        onStop = if (canSpeak) ({ viewModel.stopSpeech() }) else null,
                        isSpeakingThis = canSpeak && uiState.speakingMessageId == entry.id,
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
                item(key = "__bottom_anchor__") {
                    Spacer(Modifier.size(1.dp))
                }
            }

            // Error banner
            if (uiState.error != null) {
                Snackbar(
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    action = {
                        TextButton(onClick = { viewModel.switchToSession(sessionId) }) {
                            Text(stringResource(R.string.common_retry))
                        }
                    },
                    dismissAction = {
                        TextButton(onClick = { viewModel.clearError() }) {
                            Text(stringResource(R.string.common_close))
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
                    val text = input
                    dismissInput()
                    justSent = true
                    input = ""
                    viewModel.sendMessage(text)
                },
                onStop = { viewModel.abortGeneration() },
                isLoading = isLoading,
                canSend = uiState.connState == ConnectionState.CONNECTED,
                modifier = Modifier.onSizeChanged { bottomBarHeightPx = it.height },
            )
        }
    }

    // Persona picker — shown when the user taps "+" to start a new chat.
    if (showPersonaSheet) {
        PersonaPickerSheet(
            onDismiss = { showPersonaSheet = false },
            onStart = { persona ->
                showPersonaSheet = false
                viewModel.startNewSession(persona)
            },
            onManage = {
                showPersonaSheet = false
                onManagePersonas()
            },
        )
    }
}
