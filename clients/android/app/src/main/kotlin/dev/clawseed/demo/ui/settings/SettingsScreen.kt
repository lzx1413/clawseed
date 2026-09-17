package dev.clawseed.demo.ui.settings

import androidx.compose.animation.AnimatedVisibility
import androidx.activity.compose.BackHandler
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.foundation.text.ClickableText
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.BuildConfig
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.theme.success
import dev.clawseed.demo.data.LocalStore
import kotlinx.coroutines.launch

@Composable
private fun ExpandableSection(
    title: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    subtitle: String? = null,
    content: @Composable () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 4.dp)
                .clickable(onClick = onToggle),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                )
                if (subtitle != null) {
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Icon(
                if (expanded) Icons.Default.KeyboardArrowDown
                else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = if (expanded) stringResource(R.string.common_collapse) else stringResource(R.string.common_expand),
            )
        }
        AnimatedVisibility(
            visible = expanded,
            enter = expandVertically(),
            exit = shrinkVertically(),
        ) {
            content()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onBack: () -> Unit,
    onManageUserProfile: () -> Unit = {},
    localStore: LocalStore? = null,
) {
    val viewModel: SettingsViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val snackbarHostState = remember { SnackbarHostState() }
    var llmExpanded by rememberSaveable { mutableStateOf(false) }
    val settingsListState = rememberLazyListState()
    val providerScrollState = rememberScrollState()
    var userModelExpanded by remember { mutableStateOf(false) }
    var searchEngineExpanded by remember { mutableStateOf(false) }
    var developerExpanded by remember { mutableStateOf(false) }
    var appearanceExpanded by remember { mutableStateOf(false) }
    var sessionExpanded by remember { mutableStateOf(false) }
    var dataTransferExpanded by remember { mutableStateOf(false) }

    LaunchedEffect(uiState.error) {
        uiState.error?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.clearError()
        }
    }

    LaunchedEffect(uiState.successMessage) {
        uiState.successMessage?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.clearSuccess()
        }
    }

    BackHandler(enabled = llmExpanded) { llmExpanded = false }
    if (llmExpanded) {
        LaunchedEffect(uiState.baseUrl, uiState.apiKey, uiState.isLoading) {
            viewModel.clearBalance()
            if (!uiState.isLoading) {
                kotlinx.coroutines.delay(600)
                while (true) {
                    viewModel.refreshBalance()
                    kotlinx.coroutines.delay(60_000)
                }
            }
        }
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(stringResource(R.string.settings_llm_config)) },
                    navigationIcon = {
                        IconButton(onClick = { llmExpanded = false }) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                        }
                    },
                )
            },
            bottomBar = {
                Column(Modifier.navigationBarsPadding().imePadding()) {
                    HorizontalDivider()
                    Button(
                        onClick = viewModel::saveConfig,
                        enabled = !uiState.isSaving && !uiState.isLoading && uiState.selectedModel.isNotBlank(),
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        if (uiState.isSaving) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                        else Icon(Icons.Default.Check, contentDescription = null)
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(if (uiState.isSaving) R.string.common_saving else R.string.settings_save_config))
                    }
                }
            },
            snackbarHost = { SnackbarHost(snackbarHostState) },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding).verticalScroll(providerScrollState).padding(16.dp)) {
                ProviderFormEditor(
                    state = uiState,
                    onSelectProvider = viewModel::selectProvider,
                    onBaseUrlChange = viewModel::updateBaseUrl,
                    onApiKeyChange = viewModel::updateApiKey,
                    onFetchModels = viewModel::fetchModels,
                    onSelectModel = viewModel::selectModel,
                    onToggleThinking = viewModel::toggleThinking,
                    onVisionChange = viewModel::updateVision,
                    onUpdateMaxTokens = viewModel::updateMaxTokens,
                    onToggleAutoContinue = viewModel::toggleAutoContinueOnTruncation,
                )
            }
        }
        return
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.settings_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                    }
                },
                actions = {
                    IconButton(onClick = { viewModel.loadAll() }) {
                        Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.common_refresh))
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { innerPadding ->
        if (uiState.isLoading && uiState.status == null) {
            Box(
                modifier = Modifier.fillMaxSize().padding(innerPadding),
                contentAlignment = Alignment.Center,
            ) {
                CircularProgressIndicator()
            }
        } else {
            LazyColumn(
                state = settingsListState,
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding)
                    .padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                item { Spacer(modifier = Modifier.height(4.dp)) }

                item { StatusCard(uiState.status, uiState.downloadProgress) }

                if (localStore != null) {
                    item {
                        val themeMode by localStore.themeMode.collectAsState(initial = "system")
                        ExpandableSection(
                            title = stringResource(R.string.settings_appearance),
                            expanded = appearanceExpanded,
                            onToggle = { appearanceExpanded = !appearanceExpanded },
                            subtitle = if (!appearanceExpanded) when (themeMode) {
                                "light" -> stringResource(R.string.settings_theme_light)
                                "dark" -> stringResource(R.string.settings_theme_dark)
                                else -> stringResource(R.string.settings_theme_system)
                            } else null,
                        ) {
                            AppearanceCard(localStore)
                        }
                    }
                }

                // Session settings section
                item {
                    ExpandableSection(
                        title = stringResource(R.string.settings_session_title),
                        expanded = sessionExpanded,
                        onToggle = { sessionExpanded = !sessionExpanded },
                        subtitle = if (!sessionExpanded) {
                            val d = ttlHoursToDays(uiState.sessionTtlHours)
                            val ttlStr = if (d.isEmpty() || d == "0") stringResource(R.string.settings_session_ttl_never_delete) else stringResource(R.string.settings_session_ttl_days, d.toInt())
                            val councilStr = if (uiState.councilEnabled) stringResource(R.string.settings_council_reviewers, uiState.councilReviewers.size) else stringResource(R.string.settings_council_off)
                            stringResource(R.string.settings_session_subtitle, ttlStr, councilStr)
                        } else null,
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            SessionSettingsCard(
                                ttlHours = uiState.sessionTtlHours,
                                onTtlChange = { viewModel.updateSessionTtlHours(it) },
                            )
                            CouncilCard(
                                enabled = uiState.councilEnabled,
                                reviewers = uiState.councilReviewers,
                                onToggleEnabled = viewModel::toggleCouncilEnabled,
                                onAddReviewer = viewModel::addCouncilReviewer,
                                onRemoveReviewer = viewModel::removeCouncilReviewer,
                                onUpdateReviewer = viewModel::updateCouncilReviewer,
                            )
                            Button(
                                onClick = { viewModel.saveConfig() },
                                enabled = !uiState.isSaving,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                if (uiState.isSaving) {
                                    CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                                    Spacer(modifier = Modifier.width(8.dp))
                                }
                                Text(if (uiState.isSaving) stringResource(R.string.common_saving) else stringResource(R.string.settings_save_session_config))
                            }
                        }
                    }
                }

                // LLM Config section
                item {
                    ExpandableSection(
                        title = stringResource(R.string.settings_llm_config),
                        expanded = false,
                        onToggle = { llmExpanded = true },
                        subtitle = if (!llmExpanded && uiState.selectedModel.isNotBlank())
                            "${stringResource(PROVIDER_PRESETS[uiState.selectedPresetIndex].displayNameRes)} / ${uiState.selectedModel}"
                        else if (!llmExpanded) stringResource(R.string.settings_not_configured) else null,
                    ) {}
                }

                // Search Engine section
                item {
                    val searchSubtitle = when (uiState.searchEngine) {
                        "tavily" -> "Tavily"
                        "bing" -> "Bing"
                        else -> null
                    }
                    ExpandableSection(
                        title = stringResource(R.string.settings_search_engine),
                        expanded = searchEngineExpanded,
                        onToggle = { searchEngineExpanded = !searchEngineExpanded },
                        subtitle = if (!searchEngineExpanded) searchSubtitle else null,
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            SearchEngineCard(
                                searchEngine = uiState.searchEngine,
                                tavilyApiKey = uiState.tavilyApiKey,
                                tavilyApiKeyVisible = uiState.tavilyApiKeyVisible,
                                onSearchEngineChange = viewModel::updateSearchEngine,
                                onTavilyApiKeyChange = viewModel::updateTavilyApiKey,
                                onToggleTavilyApiKeyVisibility = viewModel::toggleTavilyApiKeyVisibility,
                            )
                            Button(
                                onClick = { viewModel.saveConfig() },
                                enabled = !uiState.isSaving,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                if (uiState.isSaving) {
                                    CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                                    Spacer(modifier = Modifier.width(8.dp))
                                }
                                Text(if (uiState.isSaving) stringResource(R.string.common_saving) else stringResource(R.string.settings_save_search_config))
                            }
                        }
                    }
                }

                // User modeling section
                item {
                    ExpandableSection(
                        title = stringResource(R.string.settings_user_model),
                        expanded = userModelExpanded,
                        onToggle = { userModelExpanded = !userModelExpanded },
                        subtitle = if (!userModelExpanded) {
                            if (uiState.userModelAutoInfer) {
                                stringResource(R.string.settings_user_model_enabled)
                            } else {
                                stringResource(R.string.settings_user_model_disabled)
                            }
                        } else null,
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            UserModelCard(
                                enabled = uiState.userModelAutoInfer,
                                controlsEnabled = !uiState.isSaving && uiState.configToml.isNotBlank(),
                                onToggle = viewModel::toggleUserModelAutoInfer,
                            )
                            OutlinedButton(
                                onClick = onManageUserProfile,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(stringResource(R.string.settings_manage_user_profile))
                                Spacer(modifier = Modifier.weight(1f))
                                Icon(
                                    Icons.AutoMirrored.Filled.KeyboardArrowRight,
                                    contentDescription = null,
                                )
                            }
                            Button(
                                onClick = viewModel::saveUserModelConfig,
                                enabled = !uiState.isSaving && uiState.configToml.isNotBlank(),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                if (uiState.isSaving) {
                                    CircularProgressIndicator(
                                        modifier = Modifier.size(18.dp),
                                        color = MaterialTheme.colorScheme.onPrimary,
                                    )
                                    Spacer(modifier = Modifier.width(8.dp))
                                }
                                Text(
                                    if (uiState.isSaving) {
                                        stringResource(R.string.common_saving)
                                    } else {
                                        stringResource(R.string.settings_save_user_model)
                                    },
                                )
                            }
                        }
                    }
                }

                // Developer Options section
                if (localStore != null) {
                    item {
                        ExpandableSection(
                            title = stringResource(R.string.settings_developer_options),
                            expanded = developerExpanded,
                            onToggle = { developerExpanded = !developerExpanded },
                        ) {
                            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                DeveloperOptionsCard(localStore)
                                Card(
                                    modifier = Modifier.fillMaxWidth(),
                                    colors = CardDefaults.cardColors(
                                        containerColor = MaterialTheme.colorScheme.surfaceVariant,
                                    ),
                                ) {
                                    Column(
                                        modifier = Modifier.padding(16.dp),
                                        verticalArrangement = Arrangement.spacedBy(12.dp),
                                    ) {
                                        Text(
                                            text = stringResource(R.string.settings_global_config),
                                            style = MaterialTheme.typography.titleSmall,
                                        )
                                        Text(
                                            text = stringResource(R.string.settings_global_config_desc),
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                        TomlEditor(
                                            toml = uiState.configToml,
                                            onTomlChange = { viewModel.updateConfigToml(it) },
                                        )
                                        Button(
                                            onClick = { viewModel.saveConfigToml() },
                                            enabled = !uiState.isSaving,
                                            modifier = Modifier.fillMaxWidth(),
                                        ) {
                                            if (uiState.isSaving) {
                                                CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                                                Spacer(modifier = Modifier.width(8.dp))
                                            }
                                            Text(if (uiState.isSaving) stringResource(R.string.common_saving) else stringResource(R.string.settings_save_global_config))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Data Transfer section
                item {
                    ExpandableSection(
                        title = stringResource(R.string.settings_data_management),
                        expanded = dataTransferExpanded,
                        onToggle = { dataTransferExpanded = !dataTransferExpanded },
                        subtitle = if (!dataTransferExpanded) stringResource(R.string.settings_data_management_subtitle) else null,
                    ) {
                        DataTransferSection()
                    }
                }

                item { Spacer(modifier = Modifier.height(16.dp)) }
            }
        }
    }
}

@Composable
private fun UserModelCard(
    enabled: Boolean,
    controlsEnabled: Boolean,
    onToggle: (Boolean) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.settings_user_model_auto_infer),
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    text = stringResource(R.string.settings_user_model_auto_infer_desc),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(modifier = Modifier.width(12.dp))
            Switch(
                checked = enabled,
                onCheckedChange = onToggle,
                enabled = controlsEnabled,
            )
        }
    }
}

@Composable
private fun AppearanceCard(localStore: LocalStore) {
    val themeMode by localStore.themeMode.collectAsState(initial = "system")
    val oledMode by localStore.oledMode.collectAsState(initial = false)
    val languageMode by localStore.languageMode.collectAsState(initial = "system")
    val speechOutput by localStore.speechOutputEnabled.collectAsState(initial = false)
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val useDarkTheme = when (themeMode) {
        "light" -> false
        "dark" -> true
        else -> isSystemInDarkTheme()
    }

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = themeMode == "light",
                    onClick = { scope.launch { localStore.setThemeMode("light") } },
                    label = { Text(stringResource(R.string.settings_theme_light)) },
                )
                FilterChip(
                    selected = themeMode == "dark",
                    onClick = { scope.launch { localStore.setThemeMode("dark") } },
                    label = { Text(stringResource(R.string.settings_theme_dark)) },
                )
                FilterChip(
                    selected = themeMode == "system",
                    onClick = { scope.launch { localStore.setThemeMode("system") } },
                    label = { Text(stringResource(R.string.settings_theme_system)) },
                )
            }
            Spacer(modifier = Modifier.height(12.dp))
            Text(stringResource(R.string.settings_language), style = MaterialTheme.typography.bodyMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = languageMode == "system",
                    onClick = { scope.launch { localStore.setLanguageMode("system"); (context as? android.app.Activity)?.recreate() } },
                    label = { Text(stringResource(R.string.settings_language_system)) },
                )
                FilterChip(
                    selected = languageMode == "en",
                    onClick = { scope.launch { localStore.setLanguageMode("en"); (context as? android.app.Activity)?.recreate() } },
                    label = { Text(stringResource(R.string.settings_language_english)) },
                )
                FilterChip(
                    selected = languageMode == "zh",
                    onClick = { scope.launch { localStore.setLanguageMode("zh"); (context as? android.app.Activity)?.recreate() } },
                    label = { Text(stringResource(R.string.settings_language_chinese)) },
                )
            }
            Spacer(modifier = Modifier.height(12.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.settings_speech_output), style = MaterialTheme.typography.bodyMedium)
                    Text(
                        stringResource(R.string.settings_speech_output_desc),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = speechOutput,
                    onCheckedChange = { scope.launch { localStore.setSpeechOutputEnabled(it) } },
                )
            }
            if (useDarkTheme) {
                Spacer(modifier = Modifier.height(12.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(stringResource(R.string.settings_oled_mode), style = MaterialTheme.typography.bodyMedium)
                        Text(
                            stringResource(R.string.settings_oled_mode_desc),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(
                        checked = oledMode,
                        onCheckedChange = { scope.launch { localStore.setOledMode(it) } },
                    )
                }
            }
        }
    }
}

@Composable
private fun DeveloperOptionsCard(localStore: LocalStore) {
    val debugEnabled by localStore.showDebugInfo.collectAsState(initial = false)
    val scope = rememberCoroutineScope()

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = stringResource(R.string.settings_debug_query),
                    style = MaterialTheme.typography.bodyMedium,
                )
                Switch(
                    checked = debugEnabled,
                    onCheckedChange = { scope.launch { localStore.setShowDebugInfo(it) } },
                )
            }
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = stringResource(R.string.settings_debug_desc),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun SoulEditor(
    content: String,
    onContentChange: (String) -> Unit,
    isSaving: Boolean,
    onSave: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OutlinedTextField(
                value = content,
                onValueChange = onContentChange,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(300.dp),
                textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                label = { Text("SOUL.md") },
                placeholder = { Text(stringResource(R.string.settings_soul_placeholder)) },
            )

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                OutlinedButton(
                    onClick = onSave,
                    enabled = !isSaving,
                ) {
                    if (isSaving) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp))
                        Spacer(modifier = Modifier.width(6.dp))
                    }
                    Text(if (isSaving) stringResource(R.string.common_saving) else stringResource(R.string.settings_save_soul))
                }
            }

            Text(
                text = stringResource(R.string.settings_soul_desc),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun StatusCard(
    status: dev.clawseed.sdk.core.model.GatewayStatus?,
    downloadProgress: dev.clawseed.sdk.core.model.EmbeddingDownloadProgress?,
) {
    var expanded by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth()) {
        ExpandableSection(
            title = stringResource(R.string.settings_gateway_status),
            expanded = expanded,
            onToggle = { expanded = !expanded },
            subtitle = if (status != null) stringResource(R.string.settings_gateway_ready, status.model) else null,
        ) {
            Column(modifier = Modifier.padding(vertical = 8.dp)) {
                if (status != null) {
                    StatusRow("Provider", status.provider ?: stringResource(R.string.settings_gateway_unknown))
                    StatusRow("Model", status.model)
                    val mem = status.memory
                    if (mem != null) {
                        StatusRow("Memory", mem.backend)
                        if (mem.embeddingProvider != "none") {
                            StatusRow("Embedding", "${mem.embeddingProvider}/${mem.embeddingModel}")
                            StatusRow("Dimensions", mem.embeddingDims.toString())
                            StatusRow("Search", mem.searchMode)
                        }
                        StatusRow("Memories", mem.count.toString())
                    } else {
                        StatusRow("Memory", status.memoryBackend ?: "none")
                    }
                }
            }
        }
        if (status == null) {
            if (downloadProgress != null && !downloadProgress.isComplete) {
                Text(
                    stringResource(R.string.settings_gateway_starting_download),
                    color = MaterialTheme.colorScheme.onSurface,
                    style = MaterialTheme.typography.bodyMedium,
                )
                Spacer(modifier = Modifier.height(8.dp))
                DownloadProgressIndicator(downloadProgress)
            } else {
                Text(stringResource(R.string.settings_gateway_unreachable), color = MaterialTheme.colorScheme.error)
            }
        }
        HorizontalDivider(Modifier.padding(top = 8.dp))
    }
}

@Composable
private fun StatusRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false).padding(start = 12.dp),
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProviderFormEditor(
    state: SettingsUiState,
    onSelectProvider: (Int) -> Unit,
    onBaseUrlChange: (String) -> Unit,
    onApiKeyChange: (String) -> Unit,
    onFetchModels: () -> Unit,
    onSelectModel: (String) -> Unit,
    onToggleThinking: (Boolean) -> Unit,
    onVisionChange: (String) -> Unit,
    onUpdateMaxTokens: (String) -> Unit,
    onToggleAutoContinue: (Boolean) -> Unit,
) {
    var providerExpanded by remember { mutableStateOf(false) }
    var modelExpanded by remember { mutableStateOf(false) }
    var showApiKey by remember { mutableStateOf(false) }

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Provider dropdown
            ExposedDropdownMenuBox(
                expanded = providerExpanded,
                onExpandedChange = { providerExpanded = it },
            ) {
                OutlinedTextField(
                    value = stringResource(PROVIDER_PRESETS[state.selectedPresetIndex].displayNameRes),
                    onValueChange = {},
                    readOnly = true,
                    label = { Text("Provider") },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = providerExpanded) },
                    modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
                )
                ExposedDropdownMenu(
                    expanded = providerExpanded,
                    onDismissRequest = { providerExpanded = false },
                ) {
                    PROVIDER_PRESETS.forEachIndexed { index, preset ->
                        DropdownMenuItem(
                            text = { Text(stringResource(preset.displayNameRes)) },
                            onClick = {
                                onSelectProvider(index)
                                providerExpanded = false
                            },
                        )
                    }
                }
            }

            // Base URL
            OutlinedTextField(
                value = state.baseUrl,
                onValueChange = onBaseUrlChange,
                label = { Text("Base URL") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                placeholder = { Text("https://api.example.com/v1") },
            )

            // API Key
            val isPlaceholderKey = state.apiKey == SettingsViewModel.MASKED_KEY_PLACEHOLDER
            OutlinedTextField(
                value = state.apiKey,
                onValueChange = { newValue ->
                    if (isPlaceholderKey && newValue != state.apiKey) {
                        onApiKeyChange(newValue.removePrefix(SettingsViewModel.MASKED_KEY_PLACEHOLDER))
                    } else {
                        onApiKeyChange(newValue)
                    }
                },
                label = { Text("API Key") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                visualTransformation = if (!isPlaceholderKey && state.apiKey.isNotEmpty() && !showApiKey)
                    PasswordVisualTransformation() else VisualTransformation.None,
                trailingIcon = if (state.apiKey.isNotEmpty() && !isPlaceholderKey) {
                    {
                        IconButton(onClick = { showApiKey = !showApiKey }) {
                            Text(
                                if (showApiKey) stringResource(R.string.common_hide) else stringResource(R.string.common_show),
                                style = MaterialTheme.typography.labelSmall,
                            )
                        }
                    }
                } else null,
                supportingText = if (isPlaceholderKey && state.hasServerApiKey) {
                    { Text(stringResource(R.string.settings_server_has_key), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                } else null,
            )

            val balanceText = when {
                state.isFetchingBalance -> stringResource(R.string.balance_loading)
                state.providerBalance?.status == "available" -> state.providerBalance.balances.joinToString(" · ") {
                    "${it.currency} ${it.available.toBigDecimalOrNull()?.stripTrailingZeros()?.toPlainString() ?: it.available}"
                }
                else -> stringResource(when (state.providerBalance?.status) {
                    "unsupported" -> R.string.balance_unsupported
                    "missing_key" -> R.string.balance_missing_key
                    "authentication_failed" -> R.string.balance_auth_failed
                    "permission_denied" -> R.string.balance_permission_denied
                    else -> R.string.balance_unavailable
                })
            }
            Text(
                text = stringResource(R.string.settings_provider_balance, balanceText),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            // Fetch models button + status
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(
                    onClick = onFetchModels,
                    enabled = !state.isFetchingModels && state.baseUrl.isNotBlank(),
                ) {
                    if (state.isFetchingModels) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp))
                        Spacer(modifier = Modifier.width(6.dp))
                    }
                    Text(stringResource(R.string.settings_fetch_models))
                }

                when (state.connectionOk) {
                    true -> {
                        Icon(
                            Icons.Default.Check,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.success,
                            modifier = Modifier.size(20.dp),
                        )
                        Text(
                            stringResource(R.string.settings_model_count, state.availableModels.size),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.success,
                        )
                    }
                    false -> {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                            modifier = Modifier.size(20.dp),
                        )
                        Text(
                            stringResource(R.string.settings_connection_failed),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                    null -> {}
                }
            }

            // Model dropdown
            if (state.availableModels.isNotEmpty()) {
                ExposedDropdownMenuBox(
                    expanded = modelExpanded,
                    onExpandedChange = { modelExpanded = it },
                ) {
                    OutlinedTextField(
                        value = state.selectedModel,
                        onValueChange = { onSelectModel(it) },
                        label = { Text("Model") },
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = modelExpanded) },
                        modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryEditable),
                        singleLine = true,
                    )
                    ExposedDropdownMenu(
                        expanded = modelExpanded,
                        onDismissRequest = { modelExpanded = false },
                    ) {
                        state.availableModels.forEach { model ->
                            DropdownMenuItem(
                                text = { Text(model, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                                onClick = {
                                    onSelectModel(model)
                                    modelExpanded = false
                                },
                            )
                        }
                    }
                }
            } else {
                OutlinedTextField(
                    value = state.selectedModel,
                    onValueChange = { onSelectModel(it) },
                    label = { Text("Model") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = { Text(stringResource(R.string.settings_model_placeholder)) },
                )
            }

            dev.clawseed.demo.ui.components.VisionModeSelector(
                value = state.vision,
                onChange = onVisionChange,
            )

            if (state.vision == "auto") {
                val status = state.status
                val support = if (status?.model == state.selectedModel &&
                    status.provider == "custom:${state.baseUrl.trimEnd('/')}"
                ) status.imageAttachments.modelSupport else "unknown"
                Text(
                    stringResource(when (support) {
                        "supported" -> R.string.settings_vision_supported
                        "unsupported" -> R.string.settings_vision_unsupported
                        else -> R.string.settings_vision_unknown
                    }),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.settings_thinking_mode), style = MaterialTheme.typography.bodyMedium)
                    Text(
                        stringResource(R.string.settings_thinking_desc),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = state.thinkingEnabled,
                    onCheckedChange = onToggleThinking,
                )
            }

            OutlinedTextField(
                value = state.maxTokens,
                onValueChange = { newValue ->
                    if (newValue.all { it.isDigit() }) {
                        onUpdateMaxTokens(newValue)
                    }
                },
                label = { Text("Max Tokens") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                placeholder = { Text("262144") },
                supportingText = {
                    Text(
                        stringResource(R.string.settings_max_tokens_desc),
                        style = MaterialTheme.typography.bodySmall,
                    )
                },
            )

            Spacer(modifier = Modifier.height(8.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.settings_auto_continue), style = MaterialTheme.typography.bodyMedium)
                    Text(
                        stringResource(R.string.settings_auto_continue_desc),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = state.autoContinueOnTruncation,
                    onCheckedChange = onToggleAutoContinue,
                )
            }
        }
    }
}

@Composable
private fun TomlEditor(toml: String, onTomlChange: (String) -> Unit) {
    OutlinedTextField(
        value = toml,
        onValueChange = onTomlChange,
        modifier = Modifier
            .fillMaxWidth()
            .height(400.dp),
        textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
        label = { Text("clawseed.toml") },
    )
}

@Composable
private fun ToolCard(
    tool: dev.clawseed.sdk.core.model.ToolInfo,
    onToggle: (Boolean) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = when (tool.sourceType) {
                "remote" -> MaterialTheme.colorScheme.tertiaryContainer
                "mcp" -> MaterialTheme.colorScheme.secondaryContainer
                else -> MaterialTheme.colorScheme.surfaceVariant
            },
        ),
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = tool.name,
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(
                        selected = false,
                        onClick = {},
                        label = {
                            Text(
                                text = when (tool.sourceType) {
                                    "remote" -> "Remote"
                                    "mcp" -> "MCP"
                                    else -> "Built-in"
                                },
                                style = MaterialTheme.typography.labelSmall,
                            )
                        },
                        modifier = Modifier.height(24.dp),
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Switch(
                        checked = tool.enabled,
                        onCheckedChange = onToggle,
                        modifier = Modifier.height(24.dp),
                    )
                }
            }
            Text(
                text = tool.description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun SkillCard(
    skill: dev.clawseed.sdk.core.model.SkillInfo,
    onToggle: (Boolean) -> Unit,
    onClick: () -> Unit = {},
) {
    Card(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (skill.version.isNotBlank()) "${skill.name}  v${skill.version}" else skill.name,
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Switch(
                    checked = skill.enabled,
                    onCheckedChange = onToggle,
                    modifier = Modifier.height(24.dp),
                )
            }
            if (skill.description.isNotBlank()) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = skill.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (skill.triggers.isNotEmpty()) {
                Spacer(modifier = Modifier.height(6.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    skill.triggers.forEach { trigger ->
                        FilterChip(
                            selected = false,
                            onClick = {},
                            label = {
                                Text(
                                    text = trigger,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                            modifier = Modifier.height(24.dp),
                        )
                    }
                }
            }
            if (skill.permissions.isNotEmpty()) {
                Spacer(modifier = Modifier.height(4.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    skill.permissions.forEach { perm ->
                        FilterChip(
                            selected = false,
                            onClick = {},
                            label = {
                                Text(
                                    text = perm,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                            modifier = Modifier.height(24.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun SkillEditor(
    skillName: String,
    content: String?,
    isLoading: Boolean,
    isSaving: Boolean,
    onContentChange: (String) -> Unit,
    onSave: () -> Unit,
    onClose: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = skillName,
                    style = MaterialTheme.typography.titleSmall,
                )
                IconButton(onClick = onClose) {
                    Icon(
                        Icons.Default.Close,
                        contentDescription = stringResource(R.string.settings_close_skill_editor),
                        modifier = Modifier.size(20.dp),
                    )
                }
            }

            if (isLoading) {
                Box(
                    modifier = Modifier.fillMaxWidth().height(200.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(24.dp))
                }
            } else if (content != null) {
                OutlinedTextField(
                    value = content,
                    onValueChange = onContentChange,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(300.dp),
                    textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                    label = { Text("SKILL.md") },
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    OutlinedButton(
                        onClick = onSave,
                        enabled = !isSaving,
                    ) {
                        if (isSaving) {
                            CircularProgressIndicator(modifier = Modifier.size(16.dp))
                            Spacer(modifier = Modifier.width(6.dp))
                        }
                        Text(if (isSaving) stringResource(R.string.common_saving) else stringResource(R.string.settings_save_skill))
                    }
                }
            }

            Text(
                text = stringResource(R.string.settings_skill_editor_desc),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun DownloadProgressIndicator(progress: dev.clawseed.sdk.core.model.EmbeddingDownloadProgress) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = stringResource(R.string.settings_downloading, progress.filename),
                style = MaterialTheme.typography.titleSmall,
            )

            if (progress.percent != null) {
                val pct = progress.percent ?: 0
                LinearProgressIndicator(
                    progress = { pct.toFloat() / 100f },
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = "${pct}%",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        text = formatBytes(progress.downloadedBytes) + " / " + formatBytes(progress.totalBytes ?: 0L),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            } else {
                LinearProgressIndicator(
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(
                    text = stringResource(R.string.settings_downloaded, formatBytes(progress.downloadedBytes)),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1_000_000_000 -> "${bytes / 1_000_000_000} GB"
    bytes >= 1_000_000 -> "%.1f MB".format(bytes / 1_000_000.0)
    bytes >= 1_000 -> "${bytes / 1_000} KB"
    else -> "$bytes B"
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EmbeddingCard(
    embeddingProvider: String,
    embeddingModel: String,
    embeddingDims: String,
    embeddingApiKey: String,
    embeddingApiKeyVisible: Boolean,
    onProviderChange: (String) -> Unit,
    onModelChange: (String) -> Unit,
    onDimsChange: (String) -> Unit,
    onApiKeyChange: (String) -> Unit,
    onToggleApiKeyVisibility: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(stringResource(R.string.settings_memory_embedding), style = MaterialTheme.typography.titleSmall)

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = embeddingProvider.isBlank(),
                    onClick = { onProviderChange("") },
                    label = { Text(stringResource(R.string.settings_memory_off)) },
                )
                FilterChip(
                    selected = embeddingProvider == "local",
                    onClick = { onProviderChange("local") },
                    label = { Text(stringResource(R.string.settings_memory_local_model)) },
                )
                FilterChip(
                    selected = embeddingProvider == "openai",
                    onClick = { onProviderChange("openai") },
                    label = { Text(stringResource(R.string.settings_memory_openai)) },
                )
                FilterChip(
                    selected = embeddingProvider == "openrouter",
                    onClick = { onProviderChange("openrouter") },
                    label = { Text(stringResource(R.string.settings_memory_openrouter)) },
                )
            }

            if (embeddingProvider.isNotBlank()) {
                val isLocal = embeddingProvider == "local"

                if (!isLocal) {
                    OutlinedTextField(
                        value = if (embeddingProvider.startsWith("custom:")) embeddingProvider.removePrefix("custom:") else "",
                        onValueChange = { onProviderChange("custom:$it") },
                        label = { Text("Base URL") },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        placeholder = { Text("https://api.example.com/v1") },
                        enabled = !embeddingProvider.startsWith("openai") && !embeddingProvider.startsWith("openrouter"),
                    )
                }

                OutlinedTextField(
                    value = embeddingModel,
                    onValueChange = onModelChange,
                    label = { Text("Model") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = { Text(if (isLocal) "gte-multilingual-base" else "text-embedding-3-small") },
                )

                OutlinedTextField(
                    value = embeddingDims,
                    onValueChange = { if (it.all { c -> c.isDigit() }) onDimsChange(it) },
                    label = { Text("Dimensions") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = { Text(if (isLocal) "768" else "1536") },
                )

                if (isLocal) {
                    Text(
                        text = stringResource(R.string.settings_embedding_local_hint),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    OutlinedTextField(
                        value = embeddingApiKey,
                        onValueChange = onApiKeyChange,
                        label = { Text("API Key") },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        visualTransformation = if (embeddingApiKey.isNotEmpty() && !embeddingApiKeyVisible)
                            PasswordVisualTransformation() else VisualTransformation.None,
                        trailingIcon = if (embeddingApiKey.isNotEmpty()) {
                            {
                                IconButton(onClick = onToggleApiKeyVisibility) {
                                    Text(
                                        if (embeddingApiKeyVisible) stringResource(R.string.common_hide) else stringResource(R.string.common_show),
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                }
                            }
                        } else null,
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SearchEngineCard(
    searchEngine: String,
    tavilyApiKey: String,
    tavilyApiKeyVisible: Boolean,
    onSearchEngineChange: (String) -> Unit,
    onTavilyApiKeyChange: (String) -> Unit,
    onToggleTavilyApiKeyVisibility: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val uriHandler = LocalUriHandler.current

    val searchEngines = listOf("bing" to "Bing", "tavily" to "Tavily")
    val selectedDisplayName = searchEngines.find { it.first == searchEngine }?.second ?: "Bing"

    val freeApiKeyPrefix = stringResource(R.string.settings_search_free_api_key)

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(stringResource(R.string.settings_search_engine), style = MaterialTheme.typography.titleSmall)

            ExposedDropdownMenuBox(
                expanded = expanded,
                onExpandedChange = { expanded = it },
            ) {
                OutlinedTextField(
                    value = selectedDisplayName,
                    onValueChange = {},
                    readOnly = true,
                    label = { Text("Provider") },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = expanded) },
                    modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
                )
                ExposedDropdownMenu(
                    expanded = expanded,
                    onDismissRequest = { expanded = false },
                ) {
                    searchEngines.forEach { (id, name) ->
                        DropdownMenuItem(
                            text = { Text(name) },
                            onClick = {
                                onSearchEngineChange(id)
                                expanded = false
                            },
                        )
                    }
                }
            }

            if (searchEngine == "tavily") {
                OutlinedTextField(
                    value = tavilyApiKey,
                    onValueChange = onTavilyApiKeyChange,
                    label = { Text("Tavily API Key") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    visualTransformation = if (tavilyApiKey.isNotEmpty() && !tavilyApiKeyVisible)
                        PasswordVisualTransformation() else VisualTransformation.None,
                    trailingIcon = if (tavilyApiKey.isNotEmpty()) {
                        {
                            IconButton(onClick = onToggleTavilyApiKeyVisibility) {
                                Text(
                                    if (tavilyApiKeyVisible) stringResource(R.string.common_hide) else stringResource(R.string.common_show),
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            }
                        }
                    } else null,
                )

                ClickableText(
                    text = buildAnnotatedString {
                        append(freeApiKeyPrefix)
                        withStyle(SpanStyle(
                            color = MaterialTheme.colorScheme.primary,
                            textDecoration = TextDecoration.Underline,
                        )) {
                            append("tavily.com")
                        }
                    },
                    style = MaterialTheme.typography.bodySmall.copy(
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    ),
                ) { offset ->
                    val urlStart = freeApiKeyPrefix.length
                    if (offset >= urlStart) {
                        uriHandler.openUri("https://tavily.com")
                    }
                }
            }
        }
    }
}

// ── Session TTL helpers ──────────────────────────────────────────

private fun ttlHoursToDays(hours: String): String {
    val h = hours.toIntOrNull() ?: 0
    return if (h == 0) "" else (h / 24).toString()
}

private fun ttlDaysToHours(days: String): String {
    val d = days.toIntOrNull() ?: 0
    return if (d == 0) "0" else (d * 24).toString()
}

@Composable
private fun SessionSettingsCard(
    ttlHours: String,
    onTtlChange: (String) -> Unit,
) {
    var daysValue by remember(ttlHours) {
        mutableStateOf(ttlHoursToDays(ttlHours))
    }

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text(
                text = stringResource(R.string.settings_session_auto_cleanup),
                style = MaterialTheme.typography.titleSmall,
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = stringResource(R.string.settings_session_cleanup_desc),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(modifier = Modifier.height(12.dp))
            OutlinedTextField(
                value = daysValue,
                onValueChange = { v ->
                    val filtered = v.filter { it.isDigit() }
                    daysValue = filtered
                    onTtlChange(ttlDaysToHours(filtered))
                },
                label = { Text(stringResource(R.string.settings_session_ttl_days_label)) },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            )
        }
    }
}

@Composable
fun CouncilCard(
    enabled: Boolean,
    reviewers: List<CouncilReviewerDraft>,
    onToggleEnabled: (Boolean) -> Unit,
    onAddReviewer: () -> Unit,
    onRemoveReviewer: (Int) -> Unit,
    onUpdateReviewer: (Int, CouncilReviewerDraft) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.settings_council_enable), style = MaterialTheme.typography.bodyLarge)
            Switch(checked = enabled, onCheckedChange = onToggleEnabled)
        }
        if (enabled) {
            Text(
                stringResource(R.string.settings_council_desc),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            for ((index, reviewer) in reviewers.withIndex()) {
                ReviewerEditor(
                    index = index,
                    draft = reviewer,
                    onUpdate = { onUpdateReviewer(index, it) },
                    onRemove = { onRemoveReviewer(index) },
                )
            }
            OutlinedButton(onClick = onAddReviewer, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.settings_add_reviewer))
            }
        }
    }
}

@Composable
fun ReviewerEditor(
    index: Int,
    draft: CouncilReviewerDraft,
    onUpdate: (CouncilReviewerDraft) -> Unit,
    onRemove: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(R.string.settings_reviewer_label, index + 1), style = MaterialTheme.typography.labelLarge)
                IconButton(onClick = onRemove, modifier = Modifier.size(24.dp)) {
                    Icon(Icons.Default.Close, contentDescription = stringResource(R.string.common_delete), modifier = Modifier.size(16.dp))
                }
            }
            OutlinedTextField(
                value = draft.role,
                onValueChange = { onUpdate(draft.copy(role = it)) },
                label = { Text(stringResource(R.string.settings_reviewer_role)) },
                placeholder = { Text(stringResource(R.string.settings_reviewer_role_placeholder)) },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
            )
            OutlinedTextField(
                value = draft.focusPrompt,
                onValueChange = { onUpdate(draft.copy(focusPrompt = it)) },
                label = { Text(stringResource(R.string.settings_reviewer_focus)) },
                placeholder = { Text(stringResource(R.string.settings_reviewer_focus_placeholder)) },
                modifier = Modifier.fillMaxWidth(),
                minLines = 2,
                maxLines = 4,
            )
            OutlinedTextField(
                value = draft.model,
                onValueChange = { onUpdate(draft.copy(model = it)) },
                label = { Text(stringResource(R.string.settings_reviewer_model)) },
                placeholder = { Text(stringResource(R.string.settings_reviewer_model_placeholder)) },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
            )
        }
    }
}
