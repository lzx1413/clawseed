package dev.clawseed.demo.ui.chat

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
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
import androidx.compose.material3.AlertDialog
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
    val drafts by viewModel.drafts.collectAsState()
    val lifecycleOwner = LocalLifecycleOwner.current
    val draftKey = uiState.currentSessionId ?: sessionId ?: "__new__"
    val input = drafts[draftKey].orEmpty()
    val density = LocalDensity.current
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    var showPersonaSheet by remember { mutableStateOf(false) }

    fun dismissInput() {
        focusManager.clearFocus(force = true)
        keyboardController?.hide()
    }

    val locationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { /* granted or denied — tool handler checks at call time */ }
    val providerActionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartIntentSenderForResult(),
    ) {
        viewModel.completeProviderAction()
    }

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
    val displayedItemCount = uiState.messages.size +
        (if (uiState.thinkingContent.isNotEmpty()) 1 else 0) +
        (if (uiState.streamingContent.isNotEmpty()) 1 else 0)
    val bottomAnchorIndex = displayedItemCount
    val isLoading = uiState.isGenerating
    val isImeVisible = WindowInsets.ime.getBottom(density) > 0

    // Only auto-scroll if user is near the bottom
    val isNearBottom by remember {
        derivedStateOf {
            val lastVisible = listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
            val totalItems = listState.layoutInfo.totalItemsCount
            totalItems == 0 || lastVisible >= totalItems - 2
        }
    }

    // Switch session only on explicit user action (version bump).
    // When starting a new session, App may carry a one-shot persona request.
    // Resume from the drawer leaves hasNewSessionPersona=false so the gateway's
    // stored binding is authoritative.
    var scrollToLatestAfterSessionSwitch by remember { mutableStateOf(false) }
    var switchedVersion by remember { mutableStateOf<Int?>(null) }
    val sessionSwitchReady = switchedVersion == sessionVersion &&
        uiState.connState == ConnectionState.CONNECTED && uiState.currentSessionId != null &&
        (sessionId == null || uiState.currentSessionId == sessionId)
    LaunchedEffect(sessionVersion) {
        switchedVersion = null
        scrollToLatestAfterSessionSwitch = true
        val persona = if (hasNewSessionPersona) newSessionPersona else null
        viewModel.switchToSession(sessionId, persona, sessionVersion)
        switchedVersion = sessionVersion
        if (hasNewSessionPersona) onNewSessionPersonaConsumed()
        val ready = kotlinx.coroutines.withTimeoutOrNull(20_000) {
            viewModel.awaitSessionReady(sessionId)
            true
        } ?: false
        if (!ready) {
            viewModel.reportConnectionTimeout()
        }
    }

    LaunchedEffect(sessionSwitchReady, autoSendMessage, uiState.isGenerating, uiState.currentSessionId) {
        if (sessionSwitchReady && autoSendMessage != null && !uiState.isGenerating && viewModel.sendMessage(autoSendMessage, uiState.currentSessionId)) {
            onAutoMessageSent()
        }
    }

    // Propagate session ID changes
    LaunchedEffect(uiState.currentSessionId, sessionSwitchReady) {
        if (sessionSwitchReady && uiState.currentSessionId != null && uiState.currentSessionId != sessionId) {
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

    LaunchedEffect(isImeVisible) {
        if (!isNearBottom || !isImeVisible) {
            return@LaunchedEffect
        }

        if (displayedItemCount > 0) {
            listState.scrollToItem(bottomAnchorIndex)
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
                .padding(innerPadding),
        ) {
            LazyColumn(
                state = listState,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentPadding = PaddingValues(
                    top = 8.dp,
                    bottom = 8.dp,
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
                        onRegenerate = if (isLastAssistant && !isLoading) ({ viewModel.regenerateLastResponse() }) else null,
                        onSpeak = if (canSpeak) ({ viewModel.speakMessage(entry.content, entry.id) }) else null,
                        onStop = if (canSpeak) ({ viewModel.stopSpeech() }) else null,
                        onPresentationAction = if (!isLoading) ({ command ->
                            viewModel.sendMessage(command, uiState.currentSessionId)
                        }) else null,
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
                        TextButton(onClick = { viewModel.retryLastRequest() }) {
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
                onInputChange = { viewModel.updateDraft(draftKey, it) },
                onSend = {
                    val text = input
                    dismissInput()
                    if (viewModel.sendMessage(text, uiState.currentSessionId)) viewModel.updateDraft(draftKey, "")
                },
                onStop = { viewModel.abortGeneration() },
                isLoading = isLoading,
                canSend = sessionSwitchReady,
                modifier = Modifier.imePadding(),
            )
        }
    }

    // Persona picker — shown when the user taps "+" to start a new chat.
    if (showPersonaSheet) {
        PersonaPickerSheet(
            onDismiss = { showPersonaSheet = false },
            onStart = { persona ->
                showPersonaSheet = false
                onNewSession(persona)
            },
            onManage = {
                showPersonaSheet = false
                onManagePersonas()
            },
        )
    }

    uiState.authPrompt?.let { prompt ->
        AlertDialog(
            onDismissRequest = viewModel::dismissAuthPrompt,
            title = { Text(stringResource(R.string.chat_provider_action_title)) },
            text = { Text(prompt.hint) },
            confirmButton = {
                TextButton(onClick = {
                    val resolution = prompt.resolution
                    val requestId = prompt.requestId
                    if (resolution != null && requestId != null) {
                        val launched = runCatching {
                            viewModel.markProviderActionStarted(requestId)
                            providerActionLauncher.launch(
                                IntentSenderRequest.Builder(resolution.intentSender).build(),
                            )
                        }.isSuccess
                        if (!launched) {
                            viewModel.failProviderAction(requestId)
                        }
                    } else {
                        viewModel.handleAuthAction()
                    }
                }) {
                    Text(stringResource(R.string.chat_provider_action_open))
                }
            },
            dismissButton = {
                TextButton(onClick = viewModel::dismissAuthPrompt) {
                    Text(stringResource(R.string.common_cancel))
                }
            },
        )
    }
}
