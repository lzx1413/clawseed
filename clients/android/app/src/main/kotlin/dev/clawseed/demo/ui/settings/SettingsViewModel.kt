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
import dev.clawseed.sdk.embedded.GatewayConfigManager
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
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
    val status: GatewayStatus? = null,
    val tools: List<ToolInfo> = emptyList(),
    val skills: List<SkillInfo> = emptyList(),
    val configToml: String = "",
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
    val maxTokens: String = "262144",
    val autoContinueOnTruncation: Boolean = true,
    val sessionTtlHours: String = "0",
    val searchEngine: String = "",
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
    val councilEnabled: Boolean = false,
    val councilReviewers: List<CouncilReviewerDraft> = emptyList(),
    // App update state
    val updateInfo: UpdateInfo? = null,
    val isCheckingUpdate: Boolean = false,
    val isDownloadingUpdate: Boolean = false,
    val updateDownloadProgress: dev.clawseed.demo.updater.ApkDownloadProgress? = null,
    val updateApkReady: Boolean = false,
    val updateCheckResult: UpdateCheckResult? = null,
)

/** Result of an update check — used to show "up to date" or error messages. */
sealed class UpdateCheckResult {
    data object UpToDate : UpdateCheckResult()
    data class Error(val message: String) : UpdateCheckResult()
}

private data class ProviderDraft(
    val apiKey: String,
    val selectedModel: String,
    val thinkingEnabled: Boolean,
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
            thinkingEnabled = state.thinkingEnabled,
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
            // Update draft with resolved data (preserving real key over masked)
            providerDrafts[currentBaseUrl] = ProviderDraft(
                apiKey = currentApiKey,
                selectedModel = currentModel,
                thinkingEnabled = thinking,
                maxTokens = maxTokens,
            )
            val autoContinue = extractAutoContinueOnTruncation(toml)
            val searchEngine = extractSearchEngine(toml)
            val tavilyKey = extractTavilyApiKey(toml)
            val embeddingProv = extractEmbeddingProvider(toml)
            val embeddingMod = extractEmbeddingModel(toml)
            val embeddingDim = extractEmbeddingDims(toml)
            val sessionTtl = extractSessionTtlHours(toml)

            val presetIdx = PROVIDER_PRESETS.indexOfFirst { it.baseUrl.isNotBlank() && currentBaseUrl.contains(it.baseUrl.removeSuffix("/v1").removeSuffix("/")) }
                .let { if (it == -1) PROVIDER_PRESETS.size - 1 else it }

            _uiState.value = _uiState.value.copy(
                status = status,
                tools = toolsResult.getOrElse { emptyList() },
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
                thinkingEnabled = thinking,
                maxTokens = maxTokens,
                autoContinueOnTruncation = autoContinue,
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
        _uiState.value = _uiState.value.copy(configToml = toml, successMessage = null)
    }

    fun selectProvider(index: Int) {
        // Save current provider's form state to drafts before switching
        saveCurrentDraft()

        val preset = PROVIDER_PRESETS[index]
        val toml = _uiState.value.configToml
        val saved = findSavedProviderSettings(toml, preset.baseUrl)
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

        _uiState.value = _uiState.value.copy(
            selectedPresetIndex = index,
            baseUrl = preset.baseUrl,
            apiKey = displayApiKey,
            hasServerApiKey = hasServerKey,
            selectedModel = draft?.selectedModel ?: saved?.model ?: "",
            thinkingEnabled = draft?.thinkingEnabled ?: saved?.thinking ?: false,
            maxTokens = draft?.maxTokens ?: saved?.maxTokens ?: "262144",
            autoContinueOnTruncation = true,
            availableModels = emptyList(),
            connectionOk = null,
            successMessage = null,
        )
    }

    fun updateBaseUrl(url: String) {
        _uiState.value = _uiState.value.copy(
            baseUrl = url,
            availableModels = emptyList(),
            connectionOk = null,
            successMessage = null,
        )
    }

    fun updateApiKey(key: String) {
        _uiState.value = _uiState.value.copy(apiKey = key, successMessage = null)
    }

    fun selectModel(model: String) {
        _uiState.value = _uiState.value.copy(selectedModel = model, successMessage = null)
    }

    fun toggleThinking(enabled: Boolean) {
        _uiState.value = _uiState.value.copy(thinkingEnabled = enabled, successMessage = null)
    }

    fun updateSessionTtlHours(value: String) {
        _uiState.value = _uiState.value.copy(sessionTtlHours = value, successMessage = null)
    }

    fun updateMaxTokens(value: String) {
        _uiState.value = _uiState.value.copy(maxTokens = value, successMessage = null)
    }

    fun toggleAutoContinueOnTruncation(enabled: Boolean) {
        _uiState.value = _uiState.value.copy(autoContinueOnTruncation = enabled, successMessage = null)
    }

    fun updateSearchEngine(engine: String) {
        _uiState.value = _uiState.value.copy(searchEngine = engine, successMessage = null)
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

    fun fetchModels() {
        val state = _uiState.value
        if (state.baseUrl.isBlank()) return

        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isFetchingModels = true, connectionOk = null, error = null)
            val useProxy = state.apiKey == MASKED_KEY_PLACEHOLDER || state.apiKey.contains("***")
            val result = if (useProxy) {
                client().models()
            } else {
                client().fetchProviderModels(state.baseUrl, state.apiKey)
            }
            result
                .onSuccess { models ->
                    _uiState.value = _uiState.value.copy(
                        isFetchingModels = false,
                        availableModels = models,
                        connectionOk = true,
                    )
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
            thinkingEnabled = thinking,
            maxTokens = maxTokens,
        )
        val autoContinue = extractAutoContinueOnTruncation(toml)
        val searchEngine = extractSearchEngine(toml)
        val tavilyKey = extractTavilyApiKey(toml)
        val embeddingProv = extractEmbeddingProvider(toml)
        val embeddingMod = extractEmbeddingModel(toml)
        val embeddingDim = extractEmbeddingDims(toml)
        val sessionTtl = extractSessionTtlHours(toml)

        val presetIdx = PROVIDER_PRESETS.indexOfFirst { it.baseUrl.isNotBlank() && currentBaseUrl.contains(it.baseUrl.removeSuffix("/v1").removeSuffix("/")) }
            .let { if (it == -1) PROVIDER_PRESETS.size - 1 else it }

        _uiState.value = _uiState.value.copy(
            status = null,
            tools = emptyList(),
            skills = emptyList(),
            configToml = toml,
            isLoading = false,
            selectedPresetIndex = presetIdx,
            baseUrl = currentBaseUrl,
            apiKey = currentApiKey,
            hasServerApiKey = serverHasKey,
            selectedModel = currentModel,
            thinkingEnabled = thinking,
            maxTokens = maxTokens,
            autoContinueOnTruncation = autoContinue,
            searchEngine = searchEngine,
            tavilyApiKey = tavilyKey,
            embeddingProvider = embeddingProv,
            embeddingModel = embeddingMod,
            embeddingDims = embeddingDim,
            sessionTtlHours = sessionTtl,
        )
    }

    private fun saveToLocalConfig(toml: String) {
        val configManager = GatewayConfigManager(getApplication())
        val file = configManager.ensureConfig()
        file.writeText(toml)
    }

    fun saveConfig() {
        viewModelScope.launch {
            val state = _uiState.value
            _uiState.value = state.copy(isSaving = true, error = null, successMessage = null)

            val toml = buildConfigToml(state)

            if (!ClawSeedAndroid.isInitialized) {
                // Gateway not running — save directly to local config file
                saveToLocalConfig(toml)
                _uiState.value = _uiState.value.copy(
                    isSaving = false,
                    successMessage = getApplication<Application>().getString(R.string.settings_save_config_local),
                )
                _uiState.value = _uiState.value.copy(configToml = toml)
                // Persist API key locally
                val savedBaseUrl = state.baseUrl.trimEnd('/')
                val isRealKey = state.apiKey.isNotBlank() && state.apiKey != MASKED_KEY_PLACEHOLDER && !state.apiKey.contains("***")
                if (isRealKey) {
                    storedApiKeys[savedBaseUrl] = state.apiKey
                    localStore.setProviderApiKey(savedBaseUrl, state.apiKey)
                }
                return@launch
            }

            client().updateConfig(toml)
                .onSuccess {
                    _uiState.value = _uiState.value.copy(isSaving = false, successMessage = getApplication<Application>().getString(R.string.settings_save_config_gateway))
                    saveCurrentDraft()
                    // Persist API key to local storage for recovery after gateway masking
                    val savedBaseUrl = state.baseUrl.trimEnd('/')
                    val isRealKey = state.apiKey.isNotBlank() && state.apiKey != MASKED_KEY_PLACEHOLDER && !state.apiKey.contains("***")
                    if (isRealKey) {
                        storedApiKeys[savedBaseUrl] = state.apiKey
                        viewModelScope.launch { localStore.setProviderApiKey(savedBaseUrl, state.apiKey) }
                    }
                    // Restart gateway in background — don't block the save coroutine
                    viewModelScope.launch {
                        ClawSeedAndroid.restartGateway()
                        loadAll()
                    }
                }
                .onFailure { e ->
                    // API failed (gateway might have crashed during save) — save to local file
                    saveToLocalConfig(toml)
                    _uiState.value = _uiState.value.copy(
                        isSaving = false,
                        successMessage = getApplication<Application>().getString(R.string.settings_save_config_local),
                        configToml = toml,
                    )
                }
        }
    }

    fun saveConfigToml() {
        viewModelScope.launch {
            val state = _uiState.value
            _uiState.value = state.copy(isSaving = true, error = null, successMessage = null)

            val toml = state.configToml

            if (!ClawSeedAndroid.isInitialized) {
                saveToLocalConfig(toml)
                _uiState.value = _uiState.value.copy(
                    isSaving = false,
                    successMessage = getApplication<Application>().getString(R.string.settings_global_config_saved_local),
                    configToml = toml,
                )
                return@launch
            }

            client().updateConfig(toml)
                .onSuccess {
                    _uiState.value = _uiState.value.copy(isSaving = false, successMessage = getApplication<Application>().getString(R.string.settings_global_config_saved_gateway))
                    viewModelScope.launch {
                        ClawSeedAndroid.restartGateway()
                        loadAll()
                    }
                }
                .onFailure { e ->
                    saveToLocalConfig(toml)
                    _uiState.value = _uiState.value.copy(
                        isSaving = false,
                        successMessage = getApplication<Application>().getString(R.string.settings_global_config_saved_local),
                        configToml = toml,
                        error = getApplication<Application>().getString(R.string.settings_save_failed_gateway, e.message?.take(100) ?: ""),
                    )
                }
        }
    }

    private fun buildConfigToml(state: SettingsUiState): String {
        val baseUrl = state.baseUrl.trimEnd('/')
        val newProviderId = "custom:$baseUrl"

        var toml = state.configToml
        val oldFallback = extractTomlValue(toml, "fallback") ?: ""

        if (oldFallback.isNotBlank() && oldFallback != newProviderId) {
            toml = toml.replace("\"$oldFallback\"", "\"$newProviderId\"")
        } else if (oldFallback.isBlank()) {
            toml = replaceOrAppendTomlValue(toml, "fallback", newProviderId)
        }

        val sectionHeader = "[providers.models.\"$newProviderId\"]"
        if (toml.contains(sectionHeader)) {
            toml = replaceInSection(toml, sectionHeader, "base_url", baseUrl)
            toml = replaceInSection(toml, sectionHeader, "model", state.selectedModel)
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
                appendLine("base_url = \"$baseUrl\"")
                appendLine("model = \"${state.selectedModel}\"")
                val isRealKey = state.apiKey.isNotBlank()
                        && state.apiKey != MASKED_KEY_PLACEHOLDER
                        && !state.apiKey.contains("***")
                if (isRealKey) {
                    appendLine("api_key = \"${state.apiKey}\"")
                }
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

        // Update [web_search] section
        val webSearchHeader = "[web_search]"
        if (toml.contains(webSearchHeader)) {
            toml = replaceInSection(toml, webSearchHeader, "provider", state.searchEngine)
            if (state.searchEngine == "tavily" && state.tavilyApiKey.isNotBlank()) {
                toml = replaceInSection(toml, webSearchHeader, "tavily_api_key", state.tavilyApiKey)
            }
        }

        // Update [agent] auto_continue_on_truncation
        val agentHeader = "[agent]"
        if (toml.contains(agentHeader)) {
            toml = replaceInSectionRaw(toml, agentHeader, "auto_continue_on_truncation", if (state.autoContinueOnTruncation) " true" else " false")
        } else {
            toml = toml.trimEnd() + "\n\n[agent]\nauto_continue_on_truncation = ${state.autoContinueOnTruncation}\n"
        }

        // Update [gateway] session_ttl_hours
        val gatewayHeader = "[gateway]"
        if (toml.contains(gatewayHeader)) {
            toml = replaceInIntSection(toml, gatewayHeader, "session_ttl_hours", state.sessionTtlHours)
        } else {
            toml = toml.trimEnd() + "\n\n[gateway]\nsession_ttl_hours = ${state.sessionTtlHours}\n"
        }

        // Update [memory] section for embedding
        val memoryHeader = "[memory]"
        if (state.embeddingProvider.isBlank()) {
            // Embedding disabled — remove [memory] section if it exists
            if (toml.contains(memoryHeader)) {
                val idx = toml.indexOf(memoryHeader)
                val nextSection = toml.indexOf("\n[", idx + 1).let { if (it == -1) toml.length else it }
                toml = toml.substring(0, idx).trimEnd() + "\n" + toml.substring(nextSection)
            }
        } else {
            val embeddingModel = if (state.embeddingProvider == "local") {
                state.embeddingModel.ifBlank { "gte-multilingual-base" }
            } else {
                state.embeddingModel
            }
            val isLocal = state.embeddingProvider == "local"
            val embeddingDims = state.embeddingDims.ifBlank {
                if (isLocal) "768" else ""
            }
            val isRealEmbeddingApiKey = state.embeddingApiKey.isNotBlank()
                    && state.embeddingApiKey != MASKED_KEY_PLACEHOLDER
                    && !state.embeddingApiKey.contains("***")
            val memorySection = buildString {
                append(memoryHeader)
                appendLine()
                append("embedding_provider = \"${state.embeddingProvider}\"")
                appendLine()
                append("embedding_model = \"$embeddingModel\"")
                appendLine()
                if (embeddingDims.isNotBlank()) {
                    append("embedding_dims = $embeddingDims")
                    appendLine()
                }
                if (!isLocal && isRealEmbeddingApiKey) {
                    append("embedding_api_key = \"${state.embeddingApiKey}\"")
                    appendLine()
                }
                // Memory system upgrade fields (Phase A-E defaults)
                append("merge_strategy = \"rrf\"")
                appendLine()
                append("defer_embedding = true")
                appendLine()
                append("stable_memory_in_system_prompt = true")
                appendLine()
                append("conflict_mode = \"combined\"")
                appendLine()
                append("min_retention_floor = 30")
                appendLine()
                append("backfill_on_startup = true")
                appendLine()
            }
            if (toml.contains(memoryHeader)) {
                val idx = toml.indexOf(memoryHeader)
                val nextSection = toml.indexOf("\n[", idx + 1).let { if (it == -1) toml.length else it }
                toml = toml.substring(0, idx) + memorySection + toml.substring(nextSection)
            } else {
                toml = toml.trimEnd() + "\n\n" + memorySection
            }
        }

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

    // ── App Update ──────────────────────────────────────────────────

    private val updateChecker = AppUpdateChecker(getApplication())
    private val apkDownloader = ApkDownloader(getApplication())

    fun checkForUpdate() {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(
                isCheckingUpdate = true,
                updateInfo = null,
                updateCheckResult = null,
                error = null,
            )
            try {
                val info = updateChecker.checkUpdate()
                if (info != null) {
                    _uiState.value = _uiState.value.copy(
                        isCheckingUpdate = false,
                        updateInfo = info,
                        updateApkReady = apkDownloader.getDownloadedApk() != null,
                    )
                } else {
                    _uiState.value = _uiState.value.copy(
                        isCheckingUpdate = false,
                        updateCheckResult = UpdateCheckResult.UpToDate,
                    )
                }
            } catch (e: Exception) {
                _uiState.value = _uiState.value.copy(
                    isCheckingUpdate = false,
                    updateCheckResult = UpdateCheckResult.Error(e.message ?: "Unknown error"),
                )
            }
        }
    }

    fun downloadUpdate() {
        val info = _uiState.value.updateInfo ?: return
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(
                isDownloadingUpdate = true,
                updateDownloadProgress = null,
                error = null,
            )
            try {
                apkDownloader.download(info.downloadUrl, info.downloadSize).collect { progress ->
                    _uiState.value = _uiState.value.copy(updateDownloadProgress = progress)
                }
                _uiState.value = _uiState.value.copy(
                    isDownloadingUpdate = false,
                    updateApkReady = apkDownloader.getDownloadedApk() != null,
                )
            } catch (e: Exception) {
                _uiState.value = _uiState.value.copy(
                    isDownloadingUpdate = false,
                    error = getApplication<Application>().getString(R.string.update_download_failed, e.message?.take(100) ?: ""),
                )
            }
        }
    }

    fun installUpdate() {
        val apkFile = apkDownloader.getDownloadedApk() ?: return
        val context = getApplication<Application>()
        if (!ApkInstaller.canInstallPackages(context)) {
            // Request permission — the UI layer handles showing the settings intent
            _uiState.value = _uiState.value.copy(
                error = getApplication<Application>().getString(R.string.update_install_permission_desc),
            )
            return
        }
        ApkInstaller.install(context, apkFile)
    }

    fun getInstallPermissionIntent(): Intent = ApkInstaller.installPermissionIntent()

    companion object {
        private fun extractProviderBaseUrl(toml: String): String {
            val fallback = extractTomlValue(toml, "fallback") ?: return ""
            val section = findSection(toml, "[providers.models.\"$fallback\"]")
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
            val fallback = extractTomlValue(toml, "fallback") ?: return ""
            val section = findSection(toml, "[providers.models.\"$fallback\"]")
            if (section.isNotEmpty()) {
                return extractTomlValueInBlock(section, "api_key") ?: ""
            }
            return extractTomlValue(toml, "default_api_key") ?: ""
        }

        private fun extractProviderModel(toml: String, status: GatewayStatus?): String {
            val fallback = extractTomlValue(toml, "fallback") ?: return ""
            val section = findSection(toml, "[providers.models.\"$fallback\"]")
            return extractTomlValueInBlock(section, "model") ?: ""
        }

        private fun extractProviderThinking(toml: String): Boolean {
            val fallback = extractTomlValue(toml, "fallback") ?: return false
            val section = findSection(toml, "[providers.models.\"$fallback\"]")
            if (sectionHasThinkingEnabled(section)) return true
            val subTableHeader = "[providers.models.\"$fallback\".provider_extra.thinking]"
            val subSection = findSection(toml, subTableHeader)
            if (subSection.isNotEmpty()) {
                val typeVal = extractTomlValueInBlock(subSection, "type")
                return typeVal == "enabled"
            }
            return false
        }

        private fun extractProviderMaxTokens(toml: String): String {
            val fallback = extractTomlValue(toml, "fallback") ?: return "262144"
            val section = findSection(toml, "[providers.models.\"$fallback\"]")
            return extractTomlValueInBlock(section, "max_tokens") ?: "262144"
        }

        private fun extractAutoContinueOnTruncation(toml: String): Boolean {
            val section = findSection(toml, "[agent]")
            val value = extractTomlValueInBlock(section, "auto_continue_on_truncation")
            return value != "false"
        }

        private data class SavedProviderSettings(
            val apiKey: String,
            val model: String,
            val thinking: Boolean,
            val maxTokens: String,
        )

        private fun findSavedProviderSettings(toml: String, baseUrl: String): SavedProviderSettings? {
            if (baseUrl.isBlank()) return null
            val trimmedUrl = baseUrl.trimEnd('/')
            val providerKey = "custom:$trimmedUrl"
            val sectionHeader = "[providers.models.\"$providerKey\"]"
            val section = findSection(toml, sectionHeader)
            if (section.isEmpty()) return null
            val apiKey = extractTomlValueInBlock(section, "api_key") ?: ""
            val model = extractTomlValueInBlock(section, "model") ?: ""
            val maxTokens = extractTomlValueInBlock(section, "max_tokens") ?: "262144"
            var thinking = sectionHasThinkingEnabled(section)
            if (!thinking) {
                val subSection = findSection(toml, "[providers.models.\"$providerKey\".provider_extra.thinking]")
                if (subSection.isNotEmpty()) {
                    thinking = extractTomlValueInBlock(subSection, "type") == "enabled"
                }
            }
            return SavedProviderSettings(apiKey, model, thinking, maxTokens)
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
            val idx = toml.indexOf(header)
            if (idx == -1) return ""
            val afterHeader = idx + header.length
            val nextSection = toml.indexOf("\n[", afterHeader).let { if (it == -1) toml.length else it }
            return toml.substring(afterHeader, nextSection)
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
            return replaceInSectionRaw(toml, sectionHeader, key, " \"$value\"")
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

        private fun replaceOrAppendTomlValue(toml: String, key: String, value: String): String {
            val lines = toml.lines().toMutableList()
            for (i in lines.indices) {
                val trimmed = lines[i].trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val eqIdx = lines[i].indexOf('=')
                    if (eqIdx >= 0) {
                        lines[i] = lines[i].substring(0, eqIdx + 1) + " \"$value\""
                        return lines.joinToString("\n")
                    }
                }
            }
            val providersIdx = lines.indexOfFirst { it.trim() == "[providers]" }
            if (providersIdx >= 0) {
                lines.add(providersIdx + 1, "$key = \"$value\"")
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
            return extractTomlValueInBlock(section, "provider") ?: ""
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
            val section = findSection(toml, sectionHeader)
            if (section.isEmpty()) return emptyList()
            for (line in section.lines()) {
                val trimmed = line.trim()
                if (trimmed.startsWith("$key ") || trimmed.startsWith("$key=")) {
                    val eqIdx = trimmed.indexOf('=')
                    if (eqIdx >= 0) {
                        val value = trimmed.substring(eqIdx + 1).trim()
                        if (value.startsWith("[") && value.endsWith("]")) {
                            val inner = value.substring(1, value.length - 1)
                            return inner.split(",")
                                .map { it.trim().removeSurrounding("\"") }
                                .filter { it.isNotBlank() }
                        }
                    }
                }
            }
            return emptyList()
        }

        /** Update or add a TOML array value in a section. Creates the section if missing. */
        fun updateTomlArray(toml: String, sectionHeader: String, key: String, values: List<String>): String {
            val arrayStr = if (values.isEmpty()) {
                "[]"
            } else {
                values.joinToString(", ", "[", "]") { "\"$it\"" }
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
                    lines[i] = newLine
                    found = true
                    break
                }
            }
            if (!found) {
                lines.add(newLine)
            }
            return before + lines.joinToString("\n") + after
        }
    }
}
