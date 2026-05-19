package dev.clawseed.demo.ui.settings

import androidx.compose.animation.AnimatedVisibility
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
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
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
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
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Icon(
                if (expanded) Icons.Default.KeyboardArrowDown
                else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = if (expanded) "收起" else "展开",
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
fun SettingsScreen(onBack: () -> Unit, localStore: LocalStore? = null) {
    val viewModel: SettingsViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val snackbarHostState = remember { SnackbarHostState() }
    var llmExpanded by remember { mutableStateOf(false) }
    var searchEngineExpanded by remember { mutableStateOf(false) }
    var soulExpanded by remember { mutableStateOf(false) }
    var toolsExpanded by remember { mutableStateOf(false) }
    var skillsExpanded by remember { mutableStateOf(false) }
    var developerExpanded by remember { mutableStateOf(false) }
    var appearanceExpanded by remember { mutableStateOf(false) }

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

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("设置") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "返回")
                    }
                },
                actions = {
                    IconButton(onClick = { viewModel.loadAll() }) {
                        Icon(Icons.Default.Refresh, contentDescription = "刷新")
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
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding)
                    .padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                item { Spacer(modifier = Modifier.height(4.dp)) }

                item { StatusCard(uiState.status) }

                if (localStore != null) {
                    item {
                        val themeMode by localStore.themeMode.collectAsState(initial = "system")
                        ExpandableSection(
                            title = "外观",
                            expanded = appearanceExpanded,
                            onToggle = { appearanceExpanded = !appearanceExpanded },
                            subtitle = if (!appearanceExpanded) when (themeMode) {
                                "light" -> "浅色"
                                "dark" -> "深色"
                                else -> "跟随系统"
                            } else null,
                        ) {
                            AppearanceCard(localStore)
                        }
                    }
                }

                // LLM Config section
                item {
                    ExpandableSection(
                        title = "LLM 配置",
                        expanded = llmExpanded,
                        onToggle = { llmExpanded = !llmExpanded },
                        subtitle = if (!llmExpanded && uiState.selectedModel.isNotBlank())
                            "${PROVIDER_PRESETS[uiState.selectedPresetIndex].displayName} / ${uiState.selectedModel}"
                        else if (!llmExpanded) "未配置" else null,
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                FilterChip(
                                    selected = uiState.editMode == EditMode.FORM,
                                    onClick = { viewModel.setEditMode(EditMode.FORM) },
                                    label = { Text("表单") },
                                )
                                FilterChip(
                                    selected = uiState.editMode == EditMode.TOML,
                                    onClick = { viewModel.setEditMode(EditMode.TOML) },
                                    label = { Text("TOML") },
                                )
                            }

                            when (uiState.editMode) {
                                EditMode.FORM -> ProviderFormEditor(
                                    state = uiState,
                                    onSelectProvider = viewModel::selectProvider,
                                    onBaseUrlChange = viewModel::updateBaseUrl,
                                    onApiKeyChange = viewModel::updateApiKey,
                                    onFetchModels = viewModel::fetchModels,
                                    onSelectModel = viewModel::selectModel,
                                    onToggleThinking = viewModel::toggleThinking,
                                    onUpdateMaxTokens = viewModel::updateMaxTokens,
                                )
                                EditMode.TOML -> TomlEditor(
                                    toml = uiState.configToml,
                                    onTomlChange = { viewModel.updateConfigToml(it) },
                                )
                            }

                            Button(
                                onClick = { viewModel.saveConfig() },
                                enabled = !uiState.isSaving && uiState.selectedModel.isNotBlank(),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                if (uiState.isSaving) {
                                    CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                                    Spacer(modifier = Modifier.width(8.dp))
                                }
                                Text(if (uiState.isSaving) "保存中..." else "保存配置")
                            }
                        }
                    }
                }

                // Search Engine section
                if (uiState.editMode == EditMode.FORM) {
                    item {
                        val searchSubtitle = when (uiState.searchEngine) {
                            "tavily" -> "Tavily"
                            "bing" -> "Bing"
                            else -> null
                        }
                        ExpandableSection(
                            title = "搜索引擎",
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
                                    Text(if (uiState.isSaving) "保存中..." else "保存搜索配置")
                                }
                            }
                        }
                    }
                }

                // Soul section
                item {
                    val soulLoaded = uiState.soulContent != null
                    val soulPreview = uiState.soulContent?.lineSequence()
                        ?.firstOrNull()?.removePrefix("# ")?.trim()
                    ExpandableSection(
                        title = "Soul",
                        expanded = soulExpanded,
                        onToggle = { soulExpanded = !soulExpanded },
                        subtitle = if (!soulExpanded && !soulLoaded) "加载失败"
                        else if (!soulExpanded && soulPreview.isNullOrBlank()) "未自定义"
                        else if (!soulExpanded) soulPreview
                        else null,
                    ) {
                        if (soulLoaded) {
                            SoulEditor(
                                content = uiState.soulContent!!,
                                onContentChange = viewModel::updateSoulContent,
                                isSaving = uiState.isSavingSoul,
                                onSave = viewModel::saveSoul,
                            )
                        } else {
                            Text(
                                text = "Soul 内容加载失败，请刷新重试",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                }

                // Tools section
                item {
                    ExpandableSection(
                        title = "已注册工具 (${uiState.tools.size})",
                        expanded = toolsExpanded,
                        onToggle = { toolsExpanded = !toolsExpanded },
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            uiState.tools.forEach { tool ->
                                ToolCard(
                                    tool = tool,
                                    onToggle = { viewModel.toggleTool(tool.name, it) },
                                )
                            }
                        }
                    }
                }

                // Skills section
                item {
                    ExpandableSection(
                        title = "可用技能 (${uiState.skills.size})",
                        expanded = skillsExpanded,
                        onToggle = { skillsExpanded = !skillsExpanded },
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            OutlinedButton(
                                onClick = { viewModel.refreshSkills() },
                                enabled = !uiState.isRefreshingSkills,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                if (uiState.isRefreshingSkills) {
                                    CircularProgressIndicator(modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(6.dp))
                                }
                                Icon(
                                    Icons.Default.Refresh,
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                )
                                Spacer(modifier = Modifier.width(6.dp))
                                Text(if (uiState.isRefreshingSkills) "刷新中..." else "刷新技能")
                            }
                            uiState.skills.forEach { skill ->
                                SkillCard(
                                    skill = skill,
                                    onToggle = { viewModel.toggleSkill(skill.name, it) },
                                )
                            }
                        }
                    }
                }

                // Developer Options section
                if (localStore != null) {
                    item {
                        ExpandableSection(
                            title = "开发者选项",
                            expanded = developerExpanded,
                            onToggle = { developerExpanded = !developerExpanded },
                        ) {
                            DeveloperOptionsCard(localStore)
                        }
                    }
                }

                item { Spacer(modifier = Modifier.height(16.dp)) }
            }
        }
    }
}

@Composable
private fun AppearanceCard(localStore: LocalStore) {
    val themeMode by localStore.themeMode.collectAsState(initial = "system")
    val oledMode by localStore.oledMode.collectAsState(initial = false)
    val scope = rememberCoroutineScope()
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
                    label = { Text("浅色") },
                )
                FilterChip(
                    selected = themeMode == "dark",
                    onClick = { scope.launch { localStore.setThemeMode("dark") } },
                    label = { Text("深色") },
                )
                FilterChip(
                    selected = themeMode == "system",
                    onClick = { scope.launch { localStore.setThemeMode("system") } },
                    label = { Text("跟随系统") },
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
                        Text("OLED 纯黑背景", style = MaterialTheme.typography.bodyMedium)
                        Text(
                            "使用纯黑背景以节省 OLED 屏幕电量",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
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
                    text = "Debug Query Message",
                    style = MaterialTheme.typography.bodyMedium,
                )
                Switch(
                    checked = debugEnabled,
                    onCheckedChange = { scope.launch { localStore.setShowDebugInfo(it) } },
                )
            }
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = "开启后，每次发送消息时会在聊天界面显示实际发送给 LLM 的完整内容，并估计 Token 数量",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
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
                placeholder = { Text("定义 AI 助手的人格和行为准则...") },
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
                    Text(if (isSaving) "保存中..." else "保存 Soul")
                }
            }

            Text(
                text = "修改后需要新会话才能生效。Soul 定义了 AI 助手的核心行为和人格。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
            )
        }
    }
}

@Composable
private fun StatusCard(status: dev.clawseed.sdk.core.model.GatewayStatus?) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text("Gateway 状态", style = MaterialTheme.typography.titleSmall)
            Spacer(modifier = Modifier.height(8.dp))
            if (status != null) {
                StatusRow("Provider", status.provider ?: "未知")
                StatusRow("Model", status.model)
                StatusRow("Memory", status.memoryBackend ?: "none")
            } else {
                Text("无法获取状态", color = MaterialTheme.colorScheme.error)
            }
        }
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
            color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.7f),
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onPrimaryContainer,
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
    onUpdateMaxTokens: (String) -> Unit,
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
                    value = PROVIDER_PRESETS[state.selectedPresetIndex].displayName,
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
                            text = { Text(preset.displayName) },
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
                                if (showApiKey) "隐藏" else "显示",
                                style = MaterialTheme.typography.labelSmall,
                            )
                        }
                    }
                } else null,
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
                    Text("获取模型列表")
                }

                when (state.connectionOk) {
                    true -> {
                        Icon(
                            Icons.Default.Check,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(20.dp),
                        )
                        Text(
                            "${state.availableModels.size} 个模型",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.primary,
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
                            "连接失败",
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
                    placeholder = { Text("点击上方按钮获取可用模型") },
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Thinking Mode", style = MaterialTheme.typography.bodyMedium)
                    Text(
                        "启用后模型会先推理思考再回答，适用于复杂任务",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
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
                        "最大输出 Token 数（思考+回复共享），建议 262144 (256K)。过小可能导致长回复被截断",
                        style = MaterialTheme.typography.bodySmall,
                    )
                },
            )
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
                    color = if (tool.enabled) MaterialTheme.colorScheme.onSurfaceVariant
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f),
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
                color = if (tool.enabled) MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
                    else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f),
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
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
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
                    text = skill.name,
                    style = MaterialTheme.typography.labelLarge,
                    color = if (skill.enabled) MaterialTheme.colorScheme.onSurfaceVariant
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f),
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
                    color = if (skill.enabled) MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f),
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
            Text("搜索引擎", style = MaterialTheme.typography.titleSmall)

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
                                    if (tavilyApiKeyVisible) "隐藏" else "显示",
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            }
                        }
                    } else null,
                )

                ClickableText(
                    text = buildAnnotatedString {
                        append("免费获取 API Key: ")
                        withStyle(SpanStyle(
                            color = MaterialTheme.colorScheme.primary,
                            textDecoration = TextDecoration.Underline,
                        )) {
                            append("tavily.com")
                        }
                    },
                    style = MaterialTheme.typography.bodySmall.copy(
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
                    ),
                ) { offset ->
                    val urlStart = "免费获取 API Key: ".length
                    if (offset >= urlStart) {
                        uriHandler.openUri("https://tavily.com")
                    }
                }
            }
        }
    }
}
