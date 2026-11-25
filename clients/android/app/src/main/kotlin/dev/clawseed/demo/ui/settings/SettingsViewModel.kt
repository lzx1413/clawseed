package dev.clawseed.demo.ui.settings

import android.app.Application
import android.content.Intent
import androidx.annotation.StringRes
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.R
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.updater.ApkDownloader
import dev.clawseed.demo.updater.ApkInstaller
import dev.clawseed.demo.updater.AppUpdateChecker
import dev.clawseed.demo.updater.UpdateInfo
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.GatewayStatus
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.ToolInfo
import dev.clawseed.sdk.core.model.PluginInfo
import dev.clawseed.sdk.embedded.GatewayConfigManager
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.launch

data class ProviderPreset(@StringRes val displayNameRes: Int, val id: String, val baseUrl: String)

val PROVIDER_PRESETS = listOf(
    ProviderPreset(R.string.provider_deepseek, "deepseek", "https://api.deepseek.com/v1"),
    ProviderPreset(R.string.provider_qwen, "qwen", "https://dashscope.aliyuncs.com/compatible-mode/v1"),
    ProviderPreset(R.string.provider_moonshot, "moonshot", "https://api.moonshot.cn/v1"),
    ProviderPreset(R.string.provider_glm, "glm-cn", "https://open.bigmodel.cn/api/paas/v4"),
    ProviderPreset(R.string.provider_doubao, "doubao", "https://ark.cn-beijing.volces.com/api/v3"),
    ProviderPreset(R.string.provider_qianfan, "qianfan", "https://qianfan.baidubce.com/v2"),
    ProviderPreset(R.string.provider_mimo, "mimo", "https://api.xiaomimimo.com/v1"),
    ProviderPreset(R.string.provider_openai, "openai", "https://api.openai.com/v1"),
    ProviderPreset(R.string.provider_anthropic, "anthropic", "https://api.anthropic.com/v1"),
    ProviderPreset(R.string.provider_openrouter, "openrouter", "https://openrouter.ai/api/v1"),
    ProviderPreset(R.string.provider_ollama, "ollama", "http://localhost:11434/v1"),
    ProviderPreset(R.string.provider_custom, "custom", ""),
)

data class SettingsUiState(
    val providerBalance: dev.clawseed.sdk.core.model.ProviderBalance? = null,
    val isFetchingBalance: Boolean = false,
    val status: GatewayStatus? = null,
    val tools: List<ToolInfo> = emptyList(),
    val plugins: List<PluginInfo> = emptyList(),
    val pluginsEnabled: Boolean = false,
    val pluginAllowed: Set<String> = emptySet(),
    val pluginDenied: Set<String> = emptySet(),
    val skills: List<SkillInfo> = emptyList(),
    val configToml: String = "",
    val configDraft: String? = null,
    val saveErrorSection: Int? = null,
    val saveErrorMessage: String? = null,
    val isLoading: Boolean = true,
    val isSaving: Boolean = false,
    val error: String? = null,
    val successMessage: String? = null,
    val selectedPresetIndex: Int = PROVIDER_PRESETS.size - 1,
    val baseUrl: String = "",
    val apiKey: String = "",
    val hasServerApiKey: Boolean = false,
    val selectedModel: String = "",
    val availableModels: List<String> = emptyList(),
    val isFetchingModels: Boolean = false,
    val connectionOk: Boolean? = null,
    val thinkingEnabled: Boolean = false,
    val vision: String = "auto",
    val maxTokens: String = "262144",
    val contextWindowTokens: String = "512000",
    val contextCompactionEnabled: Boolean = true,
    val contextCompactionKeepRecentTurns: String = "1",
    val contextCompactionThresholdPercent: String = "80",
    val userModelAutoInfer: Boolean = false,
    val sessionTtlHours: String = "0",
    // Keep the app default aligned with the selector: Bing supplies image
    // result metadata, whereas the backend's legacy empty-provider fallback
    // is DuckDuckGo and only returns ordinary web links.
    val searchEngine: String = "bing",
    val tavilyApiKey: String = "",
    val tavilyApiKeyVisible: Boolean = false,
    val embeddingProvider: String = "",  // "" | "local" | "openai" | "openrouter" | "custom:URL"
    val embeddingModel: String = "",
    val embeddingDims: String = "",
    val embeddingApiKey: String = "",
    val embeddingApiKeyVisible: Boolean = false,
    val downloadProgress: dev.clawseed.sdk.core.model.EmbeddingDownloadProgress? = null,
    val soulContent: String? = null,
    val isRefreshingSkills: Boolean = false,
    val isSavingSoul: Boolean = false,
    val editingSkill: SkillInfo? = null,
    val skillContent: String? = null,
    val isLoadingSkill: Boolean = false,
    val isSavingSkill: Boolean = false,
    val councilEnabled: Boolean = false,
    val councilReviewers: List<CouncilReviewerDraft> = emptyList(),
    // App update state
    val updateInfo: UpdateInfo? = null,
    val isCheckingUpdate: Boolean = false,
    val isDownloadingUpdate: Boolean = false,
    val updateDownloadProgress: dev.clawseed.demo.updater.ApkDownloadProgress? = null,
    val updateApkReady: Boolean = false,
    val updateInstallError: String? = null,
    val updateCheckResult: UpdateCheckResult? = null,
) {
    val nonPluginTools: List<ToolInfo> get() = tools.filterNot { it.sourceType == "plugin" }
}

/** Result of an update check — used to show "up to date" or error messages. */
sealed class UpdateCheckResult {
    data object UpToDate : UpdateCheckResult()
    data class Error(val message: String) : UpdateCheckResult()
}

private data class ProviderDraft(
    val apiKey: String,
    val selectedModel: String,
    val availableModels: List<String>,
    val thinkingEnabled: Boolean,
    val vision: String,
    val maxTokens: String,
)

data class CouncilReviewerDraft(
    val role: String = "",
    val focusPrompt: String = "",
    val model: String = "",
)

class SettingsViewModel(application: Application) : AndroidViewModel(application) {

    private fun client(): dev.clawseed.sdk.core.client.GatewayClient {
        return ClawSeedAndroid.gatewayClient()
    }

    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()
    private val providerDrafts = mutableMapOf<String, ProviderDraft>()

    private fun saveCurrentDraft() {
        val state = _uiState.value
        if (state.baseUrl.isBlank()) return
        providerDrafts[state.baseUrl.trimEnd('/')] = ProviderDraft(
            apiKey = state.apiKey,
            selectedModel = state.selectedModel,
            availableModels = state.availableModels,
            thinkingEnabled = state.thinkingEnabled,
            vision = state.vision,
            maxTokens = state.maxTokens,
        )
    }

    private val localStore = LocalStore(getApplication())
    private val storedApiKeys = mutableMapOf<String, String>()

    init {
        loadAll()
        observeDownloadProgress()
    }

    private fun observeDownloadProgress() {
        viewModelScope.launch {
            ClawSeedAndroid.downloadProgress().collect { progress ->
                _uiState.value = _uiState.value.copy(downloadProgress = progress)
            }
        }
    }

    fun loadAll() {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)

            // Read persisted API keys from local storage
            storedApiKeys.clear()
            storedApiKeys.putAll(localStore.providerApiKeys.first())

            if (!ClawSeedAndroid.isInitialized) {
                loadFromLocalConfig()
                return@launch
            }

            val statusResult = client().status()
            val toolsResult = client().tools()
            val pluginsResult = client().pluginCatalog()
            val pluginCatalog = pluginsResult.getOrElse { dev.clawseed.sdk.core.model.PluginCatalog() }
            val skillsResult = client().skills()
            val configResult = client().config()
            val personalityResult = client().personality()

            val toml = configResult.getOrElse { "" }
            val status = statusResult.getOrNull()

            val currentBaseUrl = extractProviderBaseUrl(toml)
            val rawApiKey = extractProviderApiKey(toml)
            val serverHasKey = rawApiKey.contains("***") || rawApiKey.isNotBlank()
            val currentDraft = providerDrafts[currentBaseUrl]
            val currentApiKey = when {
                rawApiKey.contains("***") && currentDraft != null && currentDraft.apiKey != MASKED_KEY_PLACEHOLDER && !currentDraft.apiKey.contains("***") -> currentDraft.apiKey
                rawApiKey.contains("***") && storedApiKeys.containsKey(currentBaseUrl) -> storedApiKeys[currentBaseUrl]!!
                rawApiKey.contains("***") -> MASKED_KEY_PLACEHOLDER
                else -> rawApiKey
            }
            val currentModel = extractProviderModel(toml, status)
            val thinking = extractProviderThinking(toml)
            val maxTokens = extractProviderMaxTokens(toml)
            val contextWindowTokens = extractContextWindowTokens(toml)
            val contextCompactionEnabled = extractContextCompactionEnabled(toml)
            val contextCompactionKeepRecentTurns = extractContextCompactionKeepRecentTurns(toml)
            val contextCompactionThresholdPercent = extractContextCompactionThresholdPercent(toml)
            // Update draft with resolved data (preserving real key over masked)
            providerDrafts[currentBaseUrl] = ProviderDraft(
                apiKey = currentApiKey,
                selectedModel = currentModel,
                availableModels = extractProviderModels(toml),
                thinkingEnabled = thinking,
                vision = extractProviderVision(toml),
                maxTokens = maxTokens,
            )
            val userModelAutoInfer = UserModelConfigToml.extractAutoInfer(toml)
            val searchEngine = extractSearchEngine(toml)
            val tavilyKey = extractTavilyApiKey(toml)
            val embeddingProv = extractEmbeddingProvider(toml)
            val embeddingMod = extractEmbeddingModel(toml)
            val embeddingDim = extractEmbeddingDims(toml)
            val sessionTtl = extractSessionTtlHours(toml)

            // Provider identity is authoritative. A provider may use a custom
            // base URL (notably Mimo), so never infer it from the URL alone.
            val activeProviderId = activeProviderKey(toml).orEmpty()
            val presetIdx = PROVIDER_PRESETS.indexOfFirst { it.id == activeProviderId }
                .let { found ->
                    if (found >= 0) found else PROVIDER_PRESETS.indexOfFirst {
                        it.baseUrl.isNotBlank() && currentBaseUrl.trimEnd('/') == it.baseUrl.trimEnd('/')
                    }.let { byUrl -> if (byUrl >= 0) byUrl else PROVIDER_PRESETS.size - 1 }
                }

            _uiState.value = _uiState.value.copy(
                status = status,
                tools = toolsResult.getOrElse { emptyList() },
                plugins = pluginCatalog.plugins,
                pluginsEnabled = pluginCatalog.enabled,
                pluginAllowed = pluginCatalog.allowed.toSet(),
                pluginDenied = pluginCatalog.denied.toSet(),
                skills = skillsResult.getOrElse { emptyList() },
                configToml = toml,
                isLoading = false,
                error = statusResult.exceptionOrNull()?.message
                    ?: configResult.exceptionOrNull()?.message,
                selectedPresetIndex = presetIdx,
                baseUrl = currentBaseUrl,
                apiKey = currentApiKey,
                hasServerApiKey = serverHasKey,
                selectedModel = currentModel,
                availableModels = extractProviderModels(toml),
                thinkingEnabled = thinking,
                vision = extractProviderVision(toml),
                maxTokens = maxTokens,
                contextWindowTokens = contextWindowTokens,
                contextCompactionEnabled = contextCompactionEnabled,
                contextCompactionKeepRecentTurns = contextCompactionKeepRecentTurns,
                contextCompactionThresholdPercent = contextCompactionThresholdPercent,
                userModelAutoInfer = userModelAutoInfer,
                searchEngine = searchEngine,
                tavilyApiKey = tavilyKey,
                embeddingProvider = embeddingProv,
                embeddingModel = embeddingMod,
                embeddingDims = embeddingDim,
                sessionTtlHours = sessionTtl,
                soulContent = personalityResult.getOrElse { null }?.get("SOUL.md"),
            )
        }
    }

    fun updateConfigToml(toml: String) {
        _uiState.value = _uiState.value.copy(configDraft = toml, successMessage = null)
    }

    fun selectProvider(index: Int) {
        // Save current provider's form state to drafts before switching
        saveCurrentDraft()

        val preset = PROVIDER_PRESETS[index]
        val toml = _uiState.value.configToml
        val providerKey = if (preset.id == "custom") {
            "custom:${preset.baseUrl.trimEnd('/')}"
        } else {
            preset.id
        }
        val saved = findSavedProviderSettings(toml, providerKey)
        val draft = providerDrafts[preset.baseUrl.trimEnd('/')]
        val storedKey = storedApiKeys[preset.baseUrl.trimEnd('/')]

        // Determine displayed API key: prefer draft > stored > masked TOML > draft masked > TOML > empty
        val displayApiKey = when {
            draft != null && draft.apiKey != MASKED_KEY_PLACEHOLDER && !draft.apiKey.contains("***") -> draft.apiKey
            storedKey != null -> storedKey
            saved != null && saved.apiKey.contains("***") -> MASKED_KEY_PLACEHOLDER
            draft != null -> draft.apiKey
            saved != null -> saved.apiKey
            else -> ""
        }

        val hasServerKey = saved != null && saved.apiKey.isNotBlank()

        val form = _uiState.value.copy(
            selectedPresetIndex = index,
            baseUrl = preset.baseUrl,
            apiKey = displayApiKey,
            providerBalance = null,
            isFetchingBalance = true,
            hasServerApiKey = hasServerKey,
            selectedModel = draft?.selectedModel ?: saved?.model ?: "",
            availableModels = draft?.availableModels ?: saved?.models.orEmpty(),
            thinkingEnabled = draft?.thinkingEnabled ?: saved?.thinking ?: false,
            vision = draft?.vision ?: saved?.vision ?: "auto",
            maxTokens = draft?.maxTokens ?: saved?.maxTokens ?: "262144",
            contextWindowTokens = extractContextWindowTokens(toml),
            contextCompactionEnabled = extractContextCompactionEnabled(toml),
            contextCompactionKeepRecentTurns = extractContextCompactionKeepRecentTurns(toml),
            contextCompactionThresholdPercent = extractContextCompactionThresholdPercent(toml),
            connectionOk = null,
            successMessage = null,
        )
        if (saved != null && saved.model.isNotBlank() && activeProviderKey(toml) != providerKey) {
            saveSetting(R.string.settings_llm_config,
                change = { setValue(it, "[providers]", "fallback", tomlString(providerKey)) },
                applied = { current -> current.copy(
                    selectedPresetIndex = index, baseUrl = form.baseUrl, apiKey = displayApiKey,
                    hasServerApiKey = hasServerKey, selectedModel = form.selectedModel,
                    availableModels = form.availableModels, thinkingEnabled = form.thinkingEnabled,
                    vision = form.vision, maxTokens = form.maxTokens, providerBalance = null, connectionOk = null,
                ) },
            )
        } else {
            _uiState.value = form
        }
    }

    fun updateBaseUrl(url: String) {
        _uiState.value = _uiState.value.copy(
            baseUrl = url,
            providerBalance = null,
            isFetchingBalance = true,
            vision = if (url.trimEnd('/') == _uiState.value.baseUrl.trimEnd('/')) _uiState.value.vision else "auto",
            availableModels = emptyList(),
            connectionOk = null,
            successMessage = null,
        )
    }

    fun updateApiKey(key: String) {
        _uiState.value = _uiState.value.copy(apiKey = key, successMessage = null, providerBalance = null, isFetchingBalance = true)
    }

    fun selectModel(model: String) {
        _uiState.value = _uiState.value.copy(selectedModel = model, vision = if (model == _uiState.value.selectedModel) _uiState.value.vision else "auto", successMessage = null)
    }

    private fun changeProviderOption(
        change: (String, String) -> String,
        apply: (SettingsUiState) -> SettingsUiState,
    ) {
        val state = _uiState.value
        val preset = PROVIDER_PRESETS[state.selectedPresetIndex]
        val key = if (preset.id == "custom") "custom:${state.baseUrl.trimEnd('/')}" else preset.id
        val header = providerSectionHeader(state.configToml, key)
        if (findSection(state.configToml, header).isEmpty()) {
            _uiState.value = apply(state)
            return
        }
        saveSetting(R.string.settings_llm_config, change = { change(it, header) }, applied = {
            if (it.selectedPresetIndex == state.selectedPresetIndex && it.baseUrl == state.baseUrl) apply(it) else it
        })
    }

    fun chooseModel(model: String) = changeProviderOption(
        change = { toml, header -> replaceInSection(replaceInSection(toml, header, "model", model), header, "vision", "auto") },
        apply = { it.copy(selectedModel = model, vision = "auto") },
    )

    fun updateVision(value: String) = changeProviderOption(
        change = { toml, header -> replaceInSection(toml, header, "vision", value) },
        apply = { it.copy(vision = value) },
    )

    fun toggleThinking(enabled: Boolean) = changeProviderOption(
        change = { toml, header -> setProviderExtraInSection(toml, header, enabled) },
        apply = { it.copy(thinkingEnabled = enabled) },
    )

    fun updateSessionTtlHours(value: String) {
        _uiState.value = _uiState.value.copy(sessionTtlHours = value, successMessage = null)
    }

    fun updateMaxTokens(value: String) {
        _uiState.value = _uiState.value.copy(maxTokens = value, successMessage = null)
    }

    fun updateContextWindowTokens(value: String) {
        _uiState.value = _uiState.value.copy(contextWindowTokens = value, successMessage = null)
    }

    fun toggleContextCompaction(enabled: Boolean) {
        saveSetting(R.string.settings_context_compaction_title,
            change = { setValue(it, "[agent]", "context_compaction_enabled", enabled.toString()) },
            applied = { it.copy(contextCompactionEnabled = enabled) },
        )
    }

    fun updateContextCompactionKeepRecentTurns(value: String) {
        saveSetting(R.string.settings_context_compaction_title,
            change = { removeFromSection(setValue(it, "[agent]", "context_compaction_keep_recent_turns", value), "[agent]", "context_compaction_keep_recent_messages") },
            applied = { it.copy(contextCompactionKeepRecentTurns = value) },
        )
    }

    fun updateContextCompactionThresholdPercent(value: String) {
        saveSetting(R.string.settings_context_compaction_title,
            change = { removeFromSection(setValue(it, "[agent]", "context_compaction_threshold_percent", value), "[agent]", "context_compaction_trigger_tokens") },
            applied = { it.copy(contextCompactionThresholdPercent = value) },
        )
    }

    fun toggleUserModelAutoInfer(enabled: Boolean) {
        saveSetting(R.string.settings_user_model,
            change = { UserModelConfigToml.updateAutoInfer(it, enabled) },
            applied = { it.copy(userModelAutoInfer = enabled) },
        )
    }

    fun updateSearchEngine(engine: String) {
        if (engine == "tavily" && extractTavilyApiKey(_uiState.value.configToml).isBlank()) {
            _uiState.value = _uiState.value.copy(searchEngine = engine)
            return
        }
        saveSetting(R.string.settings_search_engine,
            change = { setValue(it, "[web_search]", "provider", tomlString(engine)) },
            applied = { it.copy(searchEngine = engine) },
        )
    }

    fun updateTavilyApiKey(key: String) {
        _uiState.value = _uiState.value.copy(tavilyApiKey = key, successMessage = null)
    }

    fun toggleTavilyApiKeyVisibility() {
        _uiState.value = _uiState.value.copy(tavilyApiKeyVisible = !_uiState.value.tavilyApiKeyVisible)
    }

    fun updateEmbeddingProvider(provider: String) {
        val defaults = when (provider) {
            "local" -> "gte-multilingual-base" to "768"
            "openai" -> "text-embedding-3-small" to "1536"
            "openrouter" -> "openai/text-embedding-3-small" to "1536"
            else -> "" to ""
        }
        _uiState.value = _uiState.value.copy(
            embeddingProvider = provider,
            embeddingModel = if (_uiState.value.embeddingModel.isBlank()) defaults.first else _uiState.value.embeddingModel,
            embeddingDims = if (_uiState.value.embeddingDims.isBlank()) defaults.second else _uiState.value.embeddingDims,
            successMessage = null,
        )
    }

    fun updateEmbeddingModel(model: String) {
        _uiState.value = _uiState.value.copy(embeddingModel = model, successMessage = null)
    }

    fun updateEmbeddingDims(dims: String) {
        _uiState.value = _uiState.value.copy(embeddingDims = dims, successMessage = null)
    }

    fun updateEmbeddingApiKey(key: String) {
        _uiState.value = _uiState.value.copy(embeddingApiKey = key, successMessage = null)
    }

    fun toggleEmbeddingApiKeyVisibility() {
        _uiState.value = _uiState.value.copy(embeddingApiKeyVisible = !_uiState.value.embeddingApiKeyVisible)
    }

    fun toggleCouncilEnabled(enabled: Boolean) {
        _uiState.value = _uiState.value.copy(councilEnabled = enabled, successMessage = null)
    }

    fun addCouncilReviewer() {
        val current = _uiState.value.councilReviewers
        _uiState.value = _uiState.value.copy(councilReviewers = current + CouncilReviewerDraft(), successMessage = null)
    }

    fun removeCouncilReviewer(index: Int) {
        val current = _uiState.value.councilReviewers
        if (index in current.indices) {
            _uiState.value = _uiState.value.copy(councilReviewers = current.toMutableList().apply { removeAt(index) }, successMessage = null)
        }
    }

    fun updateCouncilReviewer(index: Int, draft: CouncilReviewerDraft) {
        val current = _uiState.value.councilReviewers
        if (index in current.indices) {
            _uiState.value = _uiState.value.copy(councilReviewers = current.toMutableList().apply { set(index, draft) }, successMessage = null)
        }
    }

    fun clearBalance() {
        _uiState.value = _uiState.value.copy(providerBalance = null, isFetchingBalance = true)
    }

    suspend fun refreshBalance() {
        val state = _uiState.value
        val key = state.apiKey.takeUnless { it == MASKED_KEY_PLACEHOLDER || it.contains("***") }
        val result = if (ClawSeedAndroid.isInitialized) {
            client().providerBalance(state.baseUrl, key).getOrNull()
        } else null
        kotlinx.coroutines.currentCoroutineContext().ensureActive()
        // A delayed response must never overwrite a different provider/account.
        if (_uiState.value.baseUrl == state.baseUrl && _uiState.value.apiKey == state.apiKey) {
            _uiState.value = _uiState.value.copy(
                isFetchingBalance = false,
                providerBalance = result ?: dev.clawseed.sdk.core.model.ProviderBalance("unavailable"),
            )
        }
    }

    fun fetchModels() {
        val state = _uiState.value
        if (state.baseUrl.isBlank()) return

        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isFetchingModels = true, connectionOk = null, error = null)
            val useProxy = state.apiKey == MASKED_KEY_PLACEHOLDER || state.apiKey.contains("***")
            val preset = PROVIDER_PRESETS.getOrNull(state.selectedPresetIndex)
            val providerId = if (preset != null && preset.id != "custom") {
                preset.id
            } else {
                "custom:${state.baseUrl.trimEnd('/')}"
            }
            val result = if (useProxy) {
                client().models(providerId)
            } else {
                client().fetchProviderModels(state.baseUrl, state.apiKey)
            }
            result
                .onSuccess { models ->
                    val normalized = models.map(String::trim).filter(String::isNotBlank).distinct()
                    _uiState.value = _uiState.value.copy(
                        isFetchingModels = false,
                        availableModels = normalized,
                        connectionOk = true,
                    )
                    saveFetchedModels(normalized)
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(
                        isFetchingModels = false,
                        connectionOk = false,
                        error = getApplication<Application>().getString(R.string.settings_fetch_models_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    /** Persist a discovered model catalog immediately so persona editors can
     * use it after the settings screen is closed. */
    private fun saveFetchedModels(models: List<String>) {
        if (models.isEmpty()) return
        val state = _uiState.value
        val preset = PROVIDER_PRESETS.getOrNull(state.selectedPresetIndex)
        val providerKey = if (preset != null && preset.id != "custom") {
            preset.id
        } else {
            "custom:${state.baseUrl.trimEnd('/')}"
        }
        val header = "[providers.models.${tomlKey(providerKey)}]"
        saveSetting(R.string.settings_llm_config, change = { updateTomlArray(it, header, "models", models) }, restart = false)
    }

    private fun loadFromLocalConfig() {
        val configManager = GatewayConfigManager(getApplication())
        val file = configManager.ensureConfig()
        val toml = file.readText()

        val currentBaseUrl = extractProviderBaseUrl(toml)
        val rawApiKey = extractProviderApiKey(toml)
        val serverHasKey = rawApiKey.contains("***") || rawApiKey.isNotBlank()
        val currentDraft = providerDrafts[currentBaseUrl]
        val currentApiKey = when {
            rawApiKey.contains("***") && currentDraft != null && currentDraft.apiKey != MASKED_KEY_PLACEHOLDER && !currentDraft.apiKey.contains("***") -> currentDraft.apiKey
            rawApiKey.contains("***") && storedApiKeys.containsKey(currentBaseUrl) -> storedApiKeys[currentBaseUrl]!!
            rawApiKey.contains("***") -> MASKED_KEY_PLACEHOLDER
            else -> rawApiKey
        }
        val currentModel = extractProviderModel(toml, null)
        val thinking = extractProviderThinking(toml)
        val maxTokens = extractProviderMaxTokens(toml)
        // Update draft with resolved data
        providerDrafts[currentBaseUrl] = ProviderDraft(
            apiKey = currentApiKey,
            selectedModel = currentModel,
            availableModels = extractProviderModels(toml),
            thinkingEnabled = thinking,
            vision = extractProviderVision(toml),
            maxTokens = maxTokens,
        )
        val userModelAutoInfer = UserModelConfigToml.extractAutoInfer(toml)
        val searchEngine = extractSearchEngine(toml)
        val tavilyKey = extractTavilyApiKey(toml)
        val embeddingProv = extractEmbeddingProvider(toml)
        val embeddingMod = extractEmbeddingModel(toml)
        val embeddingDim = extractEmbeddingDims(toml)
        val sessionTtl = extractSessionTtlHours(toml)
        val contextWindowTokens = extractContextWindowTokens(toml)
        val contextCompactionEnabled = extractContextCompactionEnabled(toml)
        val contextCompactionKeepRecentTurns = extractContextCompactionKeepRecentTurns(toml)
        val contextCompactionThresholdPercent = extractContextCompactionThresholdPercent(toml)

        val presetIdx = PROVIDER_PRESETS.indexOfFirst { it.baseUrl.isNotBlank() && currentBaseUrl.contains(it.baseUrl.removeSuffix("/v1").removeSuffix("/")) }
            .let { if (it == -1) PROVIDER_PRESETS.size - 1 else it }

        _uiState.value = _uiState.value.copy(
            status = null,
            tools = emptyList(),
            plugins = emptyList(),
            pluginsEnabled = false,
            pluginAllowed = emptySet(),
            pluginDenied = emptySet(),
            skills = emptyList(),
            configToml = toml,
            isLoading = false,
            selectedPresetIndex = presetIdx,
            baseUrl = currentBaseUrl,
            apiKey = currentApiKey,
            hasServerApiKey = serverHasKey,
            selectedModel = currentModel,
            availableModels = extractProviderModels(toml),
            thinkingEnabled = thinking,
            vision = extractProviderVision(toml),
            maxTokens = maxTokens,
            contextWindowTokens = contextWindowTokens,
            contextCompactionEnabled = contextCompactionEnabled,
            contextCompactionKeepRecentTurns = contextCompactionKeepRecentTurns,
            contextCompactionThresholdPercent = contextCompactionThresholdPercent,
            userModelAutoInfer = userModelAutoInfer,
            searchEngine = searchEngine,
            tavilyApiKey = tavilyKey,
            embeddingProvider = embeddingProv,
            embeddingModel = embeddingMod,
            embeddingDims = embeddingDim,
            sessionTtlHours = sessionTtl,
        )
    }

    private suspend fun saveToLocalConfig(toml: String) {
        kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
            ConfigPersistence.save(toml, null) { GatewayConfigManager(getApplication()).ensureConfig() }
        }
    }

    private val settingsWrites = Mutex()

    private fun saveSetting(
        section: Int,
        change: (String) -> String,
        applied: (SettingsUiState) -> SettingsUiState = { it },
        restart: Boolean = true,
    ) {
        viewModelScope.launch {
            settingsWrites.withLock {
                _uiState.value = _uiState.value.copy(isSaving = true, error = null, saveErrorSection = null, saveErrorMessage = null)
                try {
                    val online = ClawSeedAndroid.isInitialized
                    val current = if (online) client().config().getOrThrow() else
                        kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                            GatewayConfigManager(getApplication()).ensureConfig().readText()
                        }
                    val updated = change(current)
                    if (online) saveThroughGateway(updated).getOrThrow() else saveToLocalConfig(updated)
                    _uiState.value = applied(_uiState.value).copy(configToml = updated)
                    if (online && restart) ClawSeedAndroid.restartGateway()
                } catch (error: Exception) {
                    if (error is kotlinx.coroutines.CancellationException) throw error
                    _uiState.value = _uiState.value.copy(saveErrorSection = section,
                        saveErrorMessage = getApplication<Application>().getString(R.string.settings_save_failed, error.message.orEmpty()))
                } finally {
                    _uiState.value = _uiState.value.copy(isSaving = false)
                }
            }
        }
    }

    fun saveConfig() {
        val form = _uiState.value
        saveSetting(R.string.settings_llm_config, change = { buildProviderConfig(form.copy(configToml = it)) }, applied = { state ->
            val key = form.apiKey
            if (key.isNotBlank() && !key.contains("***") && key != MASKED_KEY_PLACEHOLDER) {
                val url = form.baseUrl.trimEnd('/')
                storedApiKeys[url] = key
                viewModelScope.launch { localStore.setProviderApiKey(url, key) }
            }
            state.copy(successMessage = getApplication<Application>().getString(
                if (ClawSeedAndroid.isInitialized) R.string.settings_save_config_gateway else R.string.settings_save_config_local
            ))
        })
    }

    fun saveSessionTtl() {
        val ttl = _uiState.value.sessionTtlHours
        saveSetting(R.string.settings_session_title, change = { setValue(it, "[gateway]", "session_ttl_hours", ttl) })
    }

    fun saveSearchCredentials() {
        val key = _uiState.value.tavilyApiKey
        saveSetting(R.string.settings_search_engine, change = {
            val config = setValue(it, "[web_search]", "provider", tomlString("tavily"))
            setValue(config, "[web_search]", "tavily_api_key", tomlString(key))
        })
    }

    private fun setValue(toml: String, section: String, key: String, raw: String): String {
        val base = if (section in toml) toml else toml.trimEnd() + "\n\n$section\n"
        return replaceInSectionRaw(base, section, key, " $raw")
    }

    fun saveConfigToml() {
        viewModelScope.launch {
            val state = _uiState.value
            _uiState.value = state.copy(isSaving = true, error = null, successMessage = null)

            val toml = state.configDraft ?: state.configToml

            if (!ClawSeedAndroid.isInitialized) {
                try {
                    saveToLocalConfig(toml)
                } catch (e: Exception) {
                    if (e is kotlinx.coroutines.CancellationException) throw e
                    showConfigSaveError(e)
                    return@launch
                }
                _uiState.value = _uiState.value.copy(
                    isSaving = false,
                    successMessage = getApplication<Application>().getString(R.string.settings_global_config_saved_local),
                    configToml = toml, configDraft = null,
                )
                return@launch
            }

            saveThroughGateway(toml)
                .onSuccess {
                    _uiState.value = _uiState.value.copy(isSaving = false, configDraft = null, successMessage = getApplication<Application>().getString(R.string.settings_global_config_saved_gateway))
                    viewModelScope.launch {
                        ClawSeedAndroid.restartGateway()
                        loadAll()
                    }
                }
                .onFailure { e ->
                    showConfigSaveError(e)
                }
        }
    }

    private suspend fun saveThroughGateway(toml: String): Result<Unit> = runCatching {
        ConfigPersistence.save(toml, client()::updateConfig) { GatewayConfigManager(getApplication()).configFile() }
    }

    private fun showConfigSaveError(error: Throwable) {
        if (error is kotlinx.coroutines.CancellationException) throw error
        _uiState.value = _uiState.value.copy(
            isSaving = false,
            successMessage = null,
            error = getApplication<Application>().getString(R.string.settings_save_failed, error.message?.take(100) ?: ""),
        )
    }

    private fun buildProviderConfig(state: SettingsUiState): String {
        val baseUrl = state.baseUrl.trimEnd('/')
        val selectedPreset = PROVIDER_PRESETS.getOrNull(state.selectedPresetIndex)
        val newProviderId = if (selectedPreset != null && selectedPreset.id != "custom") {
            selectedPreset.id
        } else {
            "custom:$baseUrl"
        }

        var toml = state.configToml
        val oldFallback = extractTomlValue(toml, "fallback") ?: ""

        if (oldFallback.isNotBlank() && oldFallback != newProviderId) {
            // Change only the active fallback assignment. Renaming every
            // occurrence can turn an existing provider table (e.g. deepseek)
            // into a duplicate table when the target provider already exists.
            toml = toml.replaceFirst(
                Regex("(?m)^\\s*fallback\\s*=\\s*\\\"[^\\\"]*\\\""),
                "fallback = ${tomlString(newProviderId)}",
            )
        } else if (oldFallback.isBlank()) {
            toml = replaceOrAppendTomlValue(toml, "fallback", newProviderId)
        }

        val sectionHeader = providerSectionHeader(toml, newProviderId)
        if (toml.contains(sectionHeader)) {
            toml = replaceInSection(toml, sectionHeader, "base_url", baseUrl)
            toml = replaceInSection(toml, sectionHeader, "model", state.selectedModel)
            toml = updateTomlArray(toml, sectionHeader, "models", state.availableModels)
            toml = replaceInSection(toml, sectionHeader, "vision", state.vision)
            val isRealKey = state.apiKey.isNotBlank()
                    && state.apiKey != MASKED_KEY_PLACEHOLDER
                    && !state.apiKey.contains("***")
            if (isRealKey) {
                toml = replaceInSection(toml, sectionHeader, "api_key", state.apiKey)
            }
            toml = replaceInIntSection(toml, sectionHeader, "max_tokens", state.maxTokens)
            toml = setProviderExtraInSection(toml, sectionHeader, state.thinkingEnabled)
        } else {
            val section = buildString {
                appendLine()
                appendLine(sectionHeader)
                appendLine("base_url = ${tomlString(baseUrl)}")
                appendLine("model = ${tomlString(state.selectedModel)}")
                if (state.availableModels.isNotEmpty()) {
                    appendLine("models = ${state.availableModels.joinToString(", ", "[", "]") { tomlString(it) }}")
                }
                val isRealKey = state.apiKey.isNotBlank()
                        && state.apiKey != MASKED_KEY_PLACEHOLDER
                        && !state.apiKey.contains("***")
                if (isRealKey) {
                    appendLine("api_key = ${tomlString(state.apiKey)}")
                }
                appendLine("vision = ${tomlString(state.vision)}")
                appendLine("max_tokens = ${state.maxTokens}")
                if (state.thinkingEnabled) {
                    appendLine(THINKING_ENABLED_LINE)
                } else {
                    appendLine(THINKING_DISABLED_LINE)
                }
            }
            val agentIdx = toml.indexOf("\n[agent]")
            toml = if (agentIdx >= 0) {
                toml.substring(0, agentIdx) + section + toml.substring(agentIdx)
            } else {
                toml + section
            }
        }

        val window = state.contextWindowTokens.toLongOrNull()?.coerceIn(1_000L, 10_000_000L) ?: 512_000L
        toml = setValue(toml, "[agent]", "context_window_tokens", window.toString())
        return toml
    }

    fun clearError() {
        _uiState.value = _uiState.value.copy(error = null)
    }

    fun clearSuccess() {
        _uiState.value = _uiState.value.copy(successMessage = null)
    }

    fun updateSoulContent(content: String) {
        _uiState.value = _uiState.value.copy(soulContent = content, successMessage = null)
    }

    fun saveSoul() {
        viewModelScope.launch {
            val content = _uiState.value.soulContent ?: return@launch
            _uiState.value = _uiState.value.copy(isSavingSoul = true, error = null, successMessage = null)

            client().updatePersonality(mapOf("SOUL.md" to content))
                .onSuccess {
                    _uiState.value = _uiState.value.copy(isSavingSoul = false, successMessage = getApplication<Application>().getString(R.string.settings_soul_saved))
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(
                        isSavingSoul = false,
                        error = getApplication<Application>().getString(R.string.settings_save_soul_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    fun toggleTool(name: String, enabled: Boolean) {
        // Optimistically update local tool state
        val state = _uiState.value
        _uiState.value = state.copy(
            tools = state.tools.map { if (it.name == name) it.copy(enabled = enabled) else it }
        )

        viewModelScope.launch {
            val currentToml = _uiState.value.configToml
            val currentDisabled = _uiState.value.tools.filter { !it.enabled }.map { it.name }
            var toml = updateTomlArray(currentToml, "[agent]", "denied_tools", currentDisabled)
            client().updateConfig(toml)
                .onSuccess {
                    _uiState.value = _uiState.value.copy(configToml = toml)
                }
                .onFailure { e ->
                    // Revert on failure
                    _uiState.value = state.copy(
                        tools = state.tools,
                        error = getApplication<Application>().getString(R.string.settings_save_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    fun togglePlugin(pluginId: String, enabled: Boolean) {
        val previous = _uiState.value
        val allowed = previous.pluginAllowed.toMutableSet()
        val denied = previous.pluginDenied.toMutableSet()
        if (enabled) {
            denied.remove(pluginId)
            if (allowed.isNotEmpty() || !previous.pluginsEnabled) allowed.add(pluginId)
        } else if (allowed.isNotEmpty()) {
            allowed.remove(pluginId)
        } else {
            denied.add(pluginId)
        }
        _uiState.value = previous.copy(
            plugins = previous.plugins.map {
                if (it.pluginId == pluginId) it.copy(enabled = enabled, status = if (enabled) "enabled" else "disabled") else it
            },
            pluginsEnabled = if (enabled) true else previous.pluginsEnabled,
            pluginAllowed = allowed,
            pluginDenied = denied,
        )
        viewModelScope.launch {
            val result = client().updatePluginConfig(
                dev.clawseed.sdk.core.model.PluginConfigPatch(
                    enabled = if (enabled) true else if (previous.plugins.all { !it.enabled }) false else true,
                    allowed = allowed.toList().sorted(),
                    denied = denied.toList().sorted(),
                ),
            )
            result.onSuccess {
                runCatching { ClawSeedAndroid.restartGateway() }
                    .onFailure { error ->
                        _uiState.value = _uiState.value.copy(error = error.message)
                    }
                loadAll()
            }.onFailure { error ->
                _uiState.value = previous.copy(error = getApplication<Application>().getString(R.string.settings_save_failed, error.message?.take(100) ?: ""))
            }
        }
    }

    fun deletePlugin(pluginId: String) {
        val previous = _uiState.value
        viewModelScope.launch {
            client().deletePlugin(pluginId)
                .onSuccess {
                    runCatching { ClawSeedAndroid.restartGateway() }
                        .onFailure { error ->
                            _uiState.value = _uiState.value.copy(error = error.message)
                        }
                    loadAll()
                }
                .onFailure { error ->
                    _uiState.value = previous.copy(
                        error = getApplication<Application>().getString(
                            R.string.settings_save_failed,
                            error.message?.take(100) ?: "",
                        ),
                    )
                }
        }
    }

    fun toggleSkill(name: String, enabled: Boolean) {
        val state = _uiState.value
        _uiState.value = state.copy(
            skills = state.skills.map { if (it.name == name) it.copy(enabled = enabled) else it }
        )

        viewModelScope.launch {
            val currentToml = _uiState.value.configToml
            val currentExcluded = _uiState.value.skills.filter { !it.enabled }.map { it.name }
            var toml = updateTomlArray(currentToml, "[skills]", "excluded", currentExcluded)
            client().updateConfig(toml)
                .onSuccess {
                    _uiState.value = _uiState.value.copy(configToml = toml)
                }
                .onFailure { e ->
                    _uiState.value = state.copy(
                        skills = state.skills,
                        error = getApplication<Application>().getString(R.string.settings_save_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    fun refreshSkills() {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isRefreshingSkills = true, error = null)
            client().reloadSkills()
                .onSuccess {
                    val skillsResult = client().skills()
                    _uiState.value = _uiState.value.copy(
                        isRefreshingSkills = false,
                        skills = skillsResult.getOrElse { emptyList() },
                        successMessage = getApplication<Application>().getString(R.string.settings_skills_refreshed),
                    )
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(
                        isRefreshingSkills = false,
                        error = getApplication<Application>().getString(R.string.settings_refresh_skills_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    // ── Skill Editor ────────────────────────────────────────────────────

    fun editSkill(skill: SkillInfo) {
        _uiState.value = _uiState.value.copy(
            editingSkill = skill,
            skillContent = null,
            isLoadingSkill = true,
            error = null,
        )
        viewModelScope.launch {
            client().skill(skill.name)
                .onSuccess { detail ->
                    _uiState.value = _uiState.value.copy(
                        skillContent = detail.content,
                        isLoadingSkill = false,
                    )
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(
                        skillContent = "",
                        isLoadingSkill = false,
                        error = getApplication<Application>().getString(R.string.settings_load_skill_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    fun updateSkillContent(content: String) {
        _uiState.value = _uiState.value.copy(skillContent = content, successMessage = null)
    }

    fun saveSkill() {
        val skill = _uiState.value.editingSkill ?: return
        val content = _uiState.value.skillContent ?: return
        _uiState.value = _uiState.value.copy(isSavingSkill = true, error = null, successMessage = null)
        viewModelScope.launch {
            client().updateSkill(skill.name, content)
                .onSuccess {
                    // Refresh skill index to reflect changes
                    val skillsResult = client().skills()
                    _uiState.value = _uiState.value.copy(
                        isSavingSkill = false,
                        skills = skillsResult.getOrElse { emptyList() },
                        successMessage = getApplication<Application>().getString(R.string.settings_skill_saved),
                    )
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(
                        isSavingSkill = false,
                        error = getApplication<Application>().getString(R.string.settings_save_skill_failed, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    fun closeSkillEditor() {
        _uiState.value = _uiState.value.copy(
            editingSkill = null,
            skillContent = null,
            isLoadingSkill = false,
            isSavingSkill = false,
        )
    }

    // ── App Update ──────────────────────────────────────────────────

    private val updateChecker = AppUpdateChecker(getApplication())
    private val apkDownloader = ApkDownloader(getApplication())

    fun checkForUpdate() {
        if (_uiState.value.isDownloadingUpdate || _uiState.value.isCheckingUpdate) return
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(
                isCheckingUpdate = true,
                updateInfo = null,
                updateCheckResult = null,
                updateApkReady = false,
                updateInstallError = null,
                error = null,
            )
            try {
                val info = updateChecker.checkUpdate()
                if (info != null) {
                    val verifiedApk = apkDownloader.getVerifiedApk(info)
                    _uiState.value = _uiState.value.copy(
                        isCheckingUpdate = false,
                        updateInfo = info,
                        updateApkReady = verifiedApk != null,
                    )
                } else {
                    _uiState.value = _uiState.value.copy(
                        isCheckingUpdate = false,
                        updateCheckResult = UpdateCheckResult.UpToDate,
                    )
                }
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                _uiState.value = _uiState.value.copy(
                    isCheckingUpdate = false,
                    updateCheckResult = UpdateCheckResult.Error(e.message ?: "Unknown error"),
                )
            }
        }
    }

    fun downloadUpdate() {
        val info = _uiState.value.updateInfo ?: return
        if (_uiState.value.isDownloadingUpdate || _uiState.value.isCheckingUpdate) return
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(
                isDownloadingUpdate = true,
                updateDownloadProgress = null,
                updateApkReady = false,
                updateInstallError = null,
                error = null,
            )
            try {
                apkDownloader.download(info.downloadUrl, info.downloadSize).collect { progress ->
                    _uiState.value = _uiState.value.copy(updateDownloadProgress = progress)
                }
                val verifiedApk = apkDownloader.getVerifiedApk(info)
                _uiState.value = _uiState.value.copy(
                    isDownloadingUpdate = false,
                    updateApkReady = verifiedApk != null,
                    updateInstallError = if (verifiedApk == null) getApplication<Application>().getString(R.string.update_invalid_apk) else null,
                )
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                _uiState.value = _uiState.value.copy(
                    isDownloadingUpdate = false,
                    updateInstallError = getApplication<Application>().getString(R.string.update_download_failed, e.message?.take(100) ?: ""),
                )
            }
        }
    }

    fun installUpdate() {
        val info = _uiState.value.updateInfo ?: return
        if (_uiState.value.isDownloadingUpdate || _uiState.value.isCheckingUpdate) return
        viewModelScope.launch {
            val apkFile = apkDownloader.getVerifiedApk(info)
            if (apkFile == null) {
                _uiState.value = _uiState.value.copy(updateApkReady = false, updateInstallError = getApplication<Application>().getString(R.string.update_invalid_apk))
                return@launch
            }
            val context = getApplication<Application>()
            if (!ApkInstaller.canInstallPackages(context)) {
                _uiState.value = _uiState.value.copy(updateInstallError = context.getString(R.string.update_install_permission_desc))
                return@launch
            }
            try {
                ApkInstaller.install(context, apkFile)
                _uiState.value = _uiState.value.copy(updateInstallError = null)
            } catch (e: Exception) {
                _uiState.value = _uiState.value.copy(updateInstallError = context.getString(R.string.update_install_failed))
            }
        }
    }

    fun getInstallPermissionIntent(): Intent = ApkInstaller.installPermissionIntent()

    companion object {
        /** Encode a TOML basic string so user-entered URLs, keys and model IDs
         * cannot break the document when they contain quotes or backslashes. */
        private fun tomlString(value: String): String = buildString {
            append('"')
            for (ch in value) {
                when (ch) {
                    '\\' -> append("\\\\")
                    '"' -> append("\\\"")
                    '\n' -> append("\\n")
                    '\r' -> append("\\r")
                    '\t' -> append("\\t")
                    else -> append(ch)
                }
            }
            append('"')
        }

        /** Use the same canonical table-key form as Rust's TOML serializer:
         * bare keys for ordinary provider IDs, quoted keys for URLs/custom IDs. */
        private fun tomlKey(value: String): String =
            if (value.matches(Regex("[A-Za-z0-9_-]+"))) value else tomlString(value)

        private fun activeProviderKey(toml: String): String? {
            extractTomlValue(toml, "fallback")?.takeIf { it.isNotBlank() }?.let { return it }
            val match = Regex("(?m)^\\[providers\\.models\\.(\\\"(?:\\\\.|[^\\\"])*\\\"|[A-Za-z0-9_-]+)\\]\\s*$")
                .find(toml) ?: return null
            return match.groupValues[1].removeSurrounding("\"")
        }

        private fun extractProviderBaseUrl(toml: String): String {
            val fallback = activeProviderKey(toml) ?: return ""
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            if (section.isNotEmpty()) {
                val url = extractTomlValueInBlock(section, "base_url")
                if (url != null) return url
            }
            if (fallback.startsWith("custom:")) {
                return fallback.removePrefix("custom:")
            }
            return ""
        }

        private fun extractProviderApiKey(toml: String): String {
            val fallback = activeProviderKey(toml) ?: return ""
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            if (section.isNotEmpty()) {
                return extractTomlValueInBlock(section, "api_key") ?: ""
            }
            return extractTomlValue(toml, "default_api_key") ?: ""
        }

        private fun extractProviderModel(toml: String, status: GatewayStatus?): String {
            val fallback = activeProviderKey(toml) ?: return ""
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            return extractTomlValueInBlock(section, "model") ?: ""
        }

        private fun extractProviderModels(toml: String): List<String> {
            val fallback = activeProviderKey(toml) ?: return emptyList()
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            val saved = parseTomlArray(section, "", "models")
            if (saved.isNotEmpty()) return saved
            return listOfNotNull(extractTomlValueInBlock(section, "model")?.takeIf(String::isNotBlank))
        }

        private fun extractProviderThinking(toml: String): Boolean {
            val fallback = activeProviderKey(toml) ?: return false
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            if (sectionHasThinkingEnabled(section)) return true
            val subTableHeader = "[providers.models.${tomlKey(fallback)}.provider_extra.thinking]"
            val subSection = findSection(toml, subTableHeader)
            if (subSection.isNotEmpty()) {
                val typeVal = extractTomlValueInBlock(subSection, "type")
                return typeVal == "enabled"
            }
            return false
        }

        private fun extractProviderMaxTokens(toml: String): String {
            val fallback = activeProviderKey(toml) ?: return "262144"
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            return extractTomlValueInBlock(section, "max_tokens") ?: "262144"
        }

        private fun extractContextWindowTokens(toml: String): String {
            val section = findSection(toml, "[agent]")
            return extractTomlValueInBlock(section, "context_window_tokens") ?: "512000"
        }

        private fun extractContextCompactionEnabled(toml: String): Boolean {
            val section = findSection(toml, "[agent]")
            return extractTomlValueInBlock(section, "context_compaction_enabled") != "false"
        }

        private fun extractContextCompactionKeepRecentTurns(toml: String): String {
            val section = findSection(toml, "[agent]")
            extractTomlValueInBlock(section, "context_compaction_keep_recent_turns")
                ?.toIntOrNull()
                ?.coerceIn(1, 100)
                ?.let { return it.toString() }
            // Legacy configs counted messages. Convert their value to a
            // conservative number of user/assistant turns for the UI.
            return extractTomlValueInBlock(section, "context_compaction_keep_recent_messages")
                ?.toIntOrNull()
                ?.coerceAtLeast(1)
                ?.plus(1)
                ?.div(2)
                ?.coerceIn(1, 100)
                ?.toString()
                ?: "1"
        }

        private fun extractContextCompactionThresholdPercent(toml: String): String {
            val section = findSection(toml, "[agent]")
            return extractTomlValueInBlock(section, "context_compaction_threshold_percent")
                ?.toIntOrNull()
                ?.coerceIn(5, 95)
                ?.toString()
                ?: "80"
        }

        internal fun extractProviderVision(toml: String): String {
            val fallback = activeProviderKey(toml) ?: return "auto"
            val section = findSection(toml, "[providers.models.${tomlKey(fallback)}]")
            return extractTomlValueInBlock(section, "vision") ?: "auto"
        }

        private data class SavedProviderSettings(
            val apiKey: String,
            val model: String,
            val models: List<String>,
            val thinking: Boolean,
            val vision: String,
            val maxTokens: String,
        )

        private fun findSavedProviderSettings(toml: String, providerKey: String): SavedProviderSettings? {
            if (providerKey.isBlank()) return null
            val sectionHeader = "[providers.models.${tomlKey(providerKey)}]"
            val section = findSection(toml, sectionHeader)
            if (section.isEmpty()) return null
            val apiKey = extractTomlValueInBlock(section, "api_key") ?: ""
            val model = extractTomlValueInBlock(section, "model") ?: ""
            val models = parseTomlArray(section, "", "models")
            val maxTokens = extractTomlValueInBlock(section, "max_tokens") ?: "262144"
            var thinking = sectionHasThinkingEnabled(section)
            if (!thinking) {
                val subSection = findSection(toml, "[providers.models.${tomlKey(providerKey)}.provider_extra.thinking]")
                if (subSection.isNotEmpty()) {
                    thinking = extractTomlValueInBlock(subSection, "type") == "enabled"
                }
            }
            return SavedProviderSettings(apiKey, model, models, thinking, extractTomlValueInBlock(section, "vision") ?: "auto", maxTokens)
        }

        private fun sectionHasThinkingEnabled(section: String): Boolean {
            for (line in section.lines()) {
                val trimmed = line.trim()
                if (trimmed.startsWith("provider_extra")) {
                    return trimmed.contains("enabled")
                }
            }
            return false
        }

        private fun setProviderExtraInSection(toml: String, sectionHeader: String, thinkingEnabled: Boolean): String {
            val subTablePrefix = sectionHeader.removeSuffix("]") + ".provider_extra"
            var result = removeSubTableSections(toml, subTablePrefix)

            val idx = result.indexOf(sectionHeader)
            if (idx == -1) return result
            val afterHeader = idx + sectionHeader.length
            val nextSection = result.indexOf("\n[", afterHeader).let { if (it == -1) result.length else it }
            val before = result.substring(0, afterHeader)
            val section = result.substring(afterHeader, nextSection)
            val after = result.substring(nextSection)

            val lines = section.lines().toMutableList()
            val existingIdx = lines.indexOfFirst { it.trim().startsWith("provider_extra") }
            val targetLine = if (thinkingEnabled) THINKING_ENABLED_LINE else THINKING_DISABLED_LINE
            if (existingIdx >= 0) {
                lines[existingIdx] = targetLine
            } else {
                lines.add(targetLine)
            }
            return before + lines.joinToString("\n") + after
        }

        private fun removeSubTableSections(toml: String, prefix: String): String {
            val lines = toml.lines().toMutableList()
            var inSubTable = false
            val result = mutableListOf<String>()
            for (line in lines) {
                val trimmed = line.trim()
                if (trimmed.startsWith("[") && !trimmed.startsWith("[[")) {
                    inSubTable = trimmed.startsWith(prefix)
                }
                if (!inSubTable) {
                    result.add(line)
                }
            }
            return result.joinToString("\n")
        }

        private fun findSection(toml: String, header: String): String {
            val headers = buildList {
                add(header)
                if (header.startsWith("[providers.models.")) {
                    val prefix = "[providers.models."
                    val rest = header.removePrefix(prefix)
                    val keyEnd = rest.indexOfAny(charArrayOf('.', ']'))
                    if (keyEnd >= 0) {
                        val keyToken = rest.substring(0, keyEnd)
                        val tail = rest.substring(keyEnd)
                        val unquoted = keyToken.removeSurrounding("\"")
                        val alternate = if (keyToken.startsWith('"')) {
                            "$prefix$unquoted$tail"
                        } else {
                            "$prefix\"$unquoted\"$tail"
                        }
                        if (alternate != header) add(alternate)
                    }
                }
            }
            for (candidate in headers) {
                val idx = toml.indexOf(candidate)
                if (idx == -1) continue
                val afterHeader = idx + candidate.length
                val nextSection = toml.indexOf("\n[", afterHeader).let { if (it == -1) toml.length else it }
                return toml.substring(afterHeader, nextSection)
            }
            return ""
        }

        private fun providerSectionHeader(toml: String, providerKey: String): String {
            val canonical = "[providers.models.${tomlKey(providerKey)}]"
            if (toml.contains(canonical)) return canonical
            val quoted = "[providers.models.${tomlString(providerKey)}]"
            return if (toml.contains(quoted)) quoted else canonical
        }

        private fun extractTomlValueInBlock(block: String, key: String): String? {
            for (line in block.lines()) {
                val trimmed = line.trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val eqIdx = trimmed.indexOf('=')
                    if (eqIdx >= 0) {
                        return trimmed.substring(eqIdx + 1).trim().removeSurrounding("\"")
                    }
                }
            }
            return null
        }

        fun extractTomlValue(toml: String, key: String): String? {
            for (line in toml.lines()) {
                val trimmed = line.trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val eqIdx = trimmed.indexOf('=')
                    if (eqIdx >= 0) {
                        return trimmed.substring(eqIdx + 1).trim().removeSurrounding("\"")
                    }
                }
            }
            return null
        }

        private fun replaceInSection(toml: String, sectionHeader: String, key: String, value: String): String {
            return replaceInSectionRaw(toml, sectionHeader, key, " ${tomlString(value)}")
        }

        private fun replaceInIntSection(toml: String, sectionHeader: String, key: String, value: String): String {
            return replaceInSectionRaw(toml, sectionHeader, key, " $value")
        }

        private fun replaceInSectionRaw(toml: String, sectionHeader: String, key: String, rawValue: String): String {
            val idx = toml.indexOf(sectionHeader)
            if (idx == -1) return toml
            val afterHeader = idx + sectionHeader.length
            val nextSection = toml.indexOf("\n[", afterHeader).let { if (it == -1) toml.length else it }
            val before = toml.substring(0, afterHeader)
            val section = toml.substring(afterHeader, nextSection)
            val after = toml.substring(nextSection)

            val lines = section.lines().toMutableList()
            var found = false
            for (i in lines.indices) {
                val trimmed = lines[i].trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val eqIdx = lines[i].indexOf('=')
                    if (eqIdx >= 0) {
                        lines[i] = lines[i].substring(0, eqIdx + 1) + rawValue
                        found = true
                        break
                    }
                }
            }
            if (!found) {
                lines.add("$key =$rawValue")
            }
            return before + lines.joinToString("\n") + after
        }

        private fun removeFromSection(toml: String, sectionHeader: String, key: String): String {
            val idx = toml.indexOf(sectionHeader)
            if (idx == -1) return toml
            val afterHeader = idx + sectionHeader.length
            val nextSection = toml.indexOf("\n[", afterHeader).let { if (it == -1) toml.length else it }
            val before = toml.substring(0, afterHeader)
            val section = toml.substring(afterHeader, nextSection)
            val after = toml.substring(nextSection)
            val filtered = section.lines().filterNot { line ->
                val trimmed = line.trim()
                trimmed.startsWith("$key ") || trimmed.startsWith("$key=")
            }
            return before + filtered.joinToString("\n") + after
        }

        private fun replaceOrAppendTomlValue(toml: String, key: String, value: String): String {
            val lines = toml.lines().toMutableList()
            for (i in lines.indices) {
                val trimmed = lines[i].trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val eqIdx = lines[i].indexOf('=')
                    if (eqIdx >= 0) {
                        lines[i] = lines[i].substring(0, eqIdx + 1) + " ${tomlString(value)}"
                        return lines.joinToString("\n")
                    }
                }
            }
            val providersIdx = lines.indexOfFirst { it.trim() == "[providers]" }
            if (providersIdx >= 0) {
                lines.add(providersIdx + 1, "$key = ${tomlString(value)}")
            }
            return lines.joinToString("\n")
        }

        const val MASKED_KEY_PLACEHOLDER = "••••••••"

        private fun extractEmbeddingProvider(toml: String): String {
            val section = findSection(toml, "[memory]")
            return extractTomlValueInBlock(section, "embedding_provider") ?: ""
        }

        private fun extractEmbeddingModel(toml: String): String {
            val section = findSection(toml, "[memory]")
            return extractTomlValueInBlock(section, "embedding_model") ?: ""
        }

        private fun extractEmbeddingDims(toml: String): String {
            val section = findSection(toml, "[memory]")
            return extractTomlValueInBlock(section, "embedding_dims") ?: ""
        }

        private fun extractSearchEngine(toml: String): String {
            val section = findSection(toml, "[web_search]")
            return extractTomlValueInBlock(section, "provider") ?: "bing"
        }

        private fun extractTavilyApiKey(toml: String): String {
            val section = findSection(toml, "[web_search]")
            return extractTomlValueInBlock(section, "tavily_api_key") ?: ""
        }

        private fun extractSessionTtlHours(toml: String): String {
            val section = findSection(toml, "[gateway]")
            return extractTomlValueInBlock(section, "session_ttl_hours") ?: "0"
        }

        private const val THINKING_ENABLED_LINE = "provider_extra = { thinking = { type = \"enabled\" } }"
        private const val THINKING_DISABLED_LINE = "provider_extra = { thinking = { type = \"disabled\" } }"

        /** Parse a TOML array value like `denied_tools = ["shell", "http_request"]` from a section. */
        fun parseTomlArray(toml: String, sectionHeader: String, key: String): List<String> {
            val section = if (sectionHeader.isBlank()) toml else findSection(toml, sectionHeader)
            if (section.isEmpty()) return emptyList()
            val lines = section.lines()
            for (i in lines.indices) {
                val trimmed = lines[i].trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val endIndex = tomlArrayEndLine(lines, i)
                    val value = lines.subList(i, endIndex + 1)
                        .joinToString(" ")
                        .substringAfter('=')
                        .trim()
                    if (!value.startsWith("[") || !value.endsWith("]")) return emptyList()
                    return splitTomlArrayItems(value.substring(1, value.length - 1))
                }
            }
            return emptyList()
        }

        private fun splitTomlArrayItems(inner: String): List<String> {
            val items = mutableListOf<String>()
            val current = StringBuilder()
            var basicString = false
            var literalString = false
            var escaped = false
            for (ch in inner) {
                if (basicString) {
                    current.append(ch)
                    if (escaped) {
                        escaped = false
                    } else if (ch == '\\') {
                        escaped = true
                    } else if (ch == '"') {
                        basicString = false
                    }
                    continue
                }
                if (literalString) {
                    current.append(ch)
                    if (ch == '\'') literalString = false
                    continue
                }
                when (ch) {
                    '"' -> {
                        basicString = true
                        current.append(ch)
                    }
                    '\'' -> {
                        literalString = true
                        current.append(ch)
                    }
                    ',' -> {
                        val item = current.toString().trim()
                        if (item.isNotBlank()) items.add(item)
                        current.clear()
                    }
                    else -> current.append(ch)
                }
            }
            val last = current.toString().trim()
            if (last.isNotBlank()) items.add(last)
            return items.map { item ->
                item.removeSurrounding("\"")
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\")
            }
        }

        /** Update or add a TOML array value in a section. Creates the section if missing. */
        fun updateTomlArray(toml: String, sectionHeader: String, key: String, values: List<String>): String {
            val arrayStr = if (values.isEmpty()) {
                "[]"
            } else {
                values.joinToString(", ", "[", "]") { tomlString(it) }
            }
            val newLine = "$key = $arrayStr"

            if (!toml.contains(sectionHeader)) {
                // Section doesn't exist — append it
                return toml.trimEnd() + "\n\n$sectionHeader\n$newLine\n"
            }

            val idx = toml.indexOf(sectionHeader)
            val afterHeader = idx + sectionHeader.length
            val nextSection = toml.indexOf("\n[", afterHeader).let { if (it == -1) toml.length else it }
            val before = toml.substring(0, afterHeader)
            val section = toml.substring(afterHeader, nextSection)
            val after = toml.substring(nextSection)

            val lines = section.lines().toMutableList()
            var found = false
            for (i in lines.indices) {
                val trimmed = lines[i].trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    // TOML arrays may be formatted over several lines. Replace
                    // the whole value, otherwise the old continuation lines
                    // remain after the new one and make the document invalid.
                    val endIndex = tomlArrayEndLine(lines, i)
                    lines.subList(i, endIndex + 1).clear()
                    lines.add(i, newLine)
                    found = true
                    break
                }
            }
            if (!found) {
                lines.add(newLine)
            }
            return before + lines.joinToString("\n") + after
        }

        /** Return the last line belonging to an array value starting at [start]. */
        private fun tomlArrayEndLine(lines: List<String>, start: Int): Int {
            var depth = 0
            var basicString = false
            var literalString = false
            var escaped = false
            var sawArray = false

            for (lineIndex in start until lines.size) {
                val line = lines[lineIndex]
                val charStart = if (lineIndex == start) line.indexOf('=').let { if (it >= 0) it + 1 else 0 } else 0
                for (charIndex in charStart until line.length) {
                    val ch = line[charIndex]
                    if (basicString) {
                        if (escaped) {
                            escaped = false
                        } else if (ch == '\\') {
                            escaped = true
                        } else if (ch == '"') {
                            basicString = false
                        }
                        continue
                    }
                    if (literalString) {
                        if (ch == '\'') literalString = false
                        continue
                    }
                    when (ch) {
                        '"' -> basicString = true
                        '\'' -> literalString = true
                        '[' -> {
                            depth++
                            sawArray = true
                        }
                        ']' -> {
                            depth--
                            if (sawArray && depth <= 0) return lineIndex
                        }
                    }
                }
            }
            return start
        }
    }
}
