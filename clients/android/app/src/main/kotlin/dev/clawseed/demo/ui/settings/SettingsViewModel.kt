package dev.clawseed.demo.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.GatewayStatus
import dev.clawseed.sdk.core.model.ToolInfo
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class ProviderPreset(val displayName: String, val id: String, val baseUrl: String)

val PROVIDER_PRESETS = listOf(
    ProviderPreset("DeepSeek", "deepseek", "https://api.deepseek.com/v1"),
    ProviderPreset("Qwen (阿里通义)", "qwen", "https://dashscope.aliyuncs.com/compatible-mode/v1"),
    ProviderPreset("Moonshot (Kimi)", "moonshot", "https://api.moonshot.cn/v1"),
    ProviderPreset("GLM (智谱)", "glm-cn", "https://open.bigmodel.cn/api/paas/v4"),
    ProviderPreset("Doubao (豆包)", "doubao", "https://ark.cn-beijing.volces.com/api/v3"),
    ProviderPreset("Baidu (千帆)", "qianfan", "https://qianfan.baidubce.com/v2"),
    ProviderPreset("OpenAI", "openai", "https://api.openai.com/v1"),
    ProviderPreset("Anthropic", "anthropic", "https://api.anthropic.com/v1"),
    ProviderPreset("OpenRouter", "openrouter", "https://openrouter.ai/api/v1"),
    ProviderPreset("Ollama (本地)", "ollama", "http://localhost:11434/v1"),
    ProviderPreset("自定义", "custom", ""),
)

data class SettingsUiState(
    val status: GatewayStatus? = null,
    val tools: List<ToolInfo> = emptyList(),
    val configToml: String = "",
    val isLoading: Boolean = true,
    val isSaving: Boolean = false,
    val error: String? = null,
    val saveSuccess: Boolean = false,
    val editMode: EditMode = EditMode.FORM,
    val selectedPresetIndex: Int = PROVIDER_PRESETS.size - 1,
    val baseUrl: String = "",
    val apiKey: String = "",
    val hasServerApiKey: Boolean = false,
    val selectedModel: String = "",
    val availableModels: List<String> = emptyList(),
    val isFetchingModels: Boolean = false,
    val connectionOk: Boolean? = null,
    val thinkingEnabled: Boolean = false,
    val searchEngine: String = "",
    val tavilyApiKey: String = "",
    val tavilyApiKeyVisible: Boolean = false,
)

enum class EditMode { FORM, TOML }

class SettingsViewModel : ViewModel() {

    private fun client(): dev.clawseed.sdk.core.client.GatewayClient {
        return ClawSeedAndroid.gatewayClient()
    }

    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()
    private var preservedApiKey: String? = null

    init {
        loadAll()
    }

    fun loadAll() {
        viewModelScope.launch {
            if (!ClawSeedAndroid.isInitialized) return@launch
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)

            val statusResult = client().status()
            val toolsResult = client().tools()
            val configResult = client().config()

            val toml = configResult.getOrElse { "" }
            val status = statusResult.getOrNull()

            val currentBaseUrl = extractProviderBaseUrl(toml)
            val rawApiKey = extractProviderApiKey(toml)
            val serverHasKey = rawApiKey.contains("***") || rawApiKey.isNotBlank()
            val currentApiKey = when {
                rawApiKey.contains("***") && preservedApiKey != null -> preservedApiKey!!
                rawApiKey.contains("***") -> MASKED_KEY_PLACEHOLDER
                else -> rawApiKey
            }
            preservedApiKey = null
            val currentModel = extractProviderModel(toml, status)
            val thinking = extractProviderThinking(toml)
            val searchEngine = extractSearchEngine(toml)
            val tavilyKey = extractTavilyApiKey(toml)

            val presetIdx = PROVIDER_PRESETS.indexOfFirst { it.baseUrl.isNotBlank() && currentBaseUrl.contains(it.baseUrl.removeSuffix("/v1").removeSuffix("/")) }
                .let { if (it == -1) PROVIDER_PRESETS.size - 1 else it }

            _uiState.value = _uiState.value.copy(
                status = status,
                tools = toolsResult.getOrElse { emptyList() },
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
                searchEngine = searchEngine,
                tavilyApiKey = tavilyKey,
            )
        }
    }

    fun setEditMode(mode: EditMode) {
        _uiState.value = _uiState.value.copy(editMode = mode, saveSuccess = false)
    }

    fun updateConfigToml(toml: String) {
        _uiState.value = _uiState.value.copy(configToml = toml, saveSuccess = false)
    }

    fun selectProvider(index: Int) {
        val preset = PROVIDER_PRESETS[index]
        val toml = _uiState.value.configToml
        val saved = findSavedProviderSettings(toml, preset.baseUrl)
        val rawApiKey = saved?.apiKey ?: ""
        val serverHasKey = rawApiKey.contains("***") || rawApiKey.isNotBlank()
        val displayApiKey = when {
            rawApiKey.contains("***") -> MASKED_KEY_PLACEHOLDER
            else -> rawApiKey
        }
        _uiState.value = _uiState.value.copy(
            selectedPresetIndex = index,
            baseUrl = preset.baseUrl,
            apiKey = displayApiKey,
            hasServerApiKey = serverHasKey,
            selectedModel = saved?.model ?: "",
            thinkingEnabled = saved?.thinking ?: false,
            availableModels = emptyList(),
            connectionOk = null,
            saveSuccess = false,
        )
    }

    fun updateBaseUrl(url: String) {
        _uiState.value = _uiState.value.copy(
            baseUrl = url,
            availableModels = emptyList(),
            connectionOk = null,
            saveSuccess = false,
        )
    }

    fun updateApiKey(key: String) {
        _uiState.value = _uiState.value.copy(apiKey = key, saveSuccess = false)
    }

    fun selectModel(model: String) {
        _uiState.value = _uiState.value.copy(selectedModel = model, saveSuccess = false)
    }

    fun toggleThinking(enabled: Boolean) {
        _uiState.value = _uiState.value.copy(thinkingEnabled = enabled, saveSuccess = false)
    }

    fun updateSearchEngine(engine: String) {
        _uiState.value = _uiState.value.copy(searchEngine = engine, saveSuccess = false)
    }

    fun updateTavilyApiKey(key: String) {
        _uiState.value = _uiState.value.copy(tavilyApiKey = key, saveSuccess = false)
    }

    fun toggleTavilyApiKeyVisibility() {
        _uiState.value = _uiState.value.copy(tavilyApiKeyVisible = !_uiState.value.tavilyApiKeyVisible)
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
                        error = "获取模型失败: ${e.message?.take(100)}",
                    )
                }
        }
    }

    fun saveConfig() {
        viewModelScope.launch {
            val state = _uiState.value
            _uiState.value = state.copy(isSaving = true, error = null, saveSuccess = false)

            val toml = buildConfigToml(state)
            client().updateConfig(toml)
                .onSuccess {
                    _uiState.value = _uiState.value.copy(isSaving = false, saveSuccess = true)
                    preservedApiKey = state.apiKey.ifBlank { null }
                    loadAll()
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(
                        isSaving = false,
                        error = e.message ?: "保存失败",
                    )
                }
        }
    }

    private fun buildConfigToml(state: SettingsUiState): String {
        if (state.editMode == EditMode.TOML) return state.configToml

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

        return toml
    }

    fun clearError() {
        _uiState.value = _uiState.value.copy(error = null)
    }

    fun clearSaveSuccess() {
        _uiState.value = _uiState.value.copy(saveSuccess = false)
    }

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

        private data class SavedProviderSettings(
            val apiKey: String,
            val model: String,
            val thinking: Boolean,
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
            var thinking = sectionHasThinkingEnabled(section)
            if (!thinking) {
                val subSection = findSection(toml, "[providers.models.\"$providerKey\".provider_extra.thinking]")
                if (subSection.isNotEmpty()) {
                    thinking = extractTomlValueInBlock(subSection, "type") == "enabled"
                }
            }
            return SavedProviderSettings(apiKey, model, thinking)
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
                        lines[i] = lines[i].substring(0, eqIdx + 1) + " \"$value\""
                        found = true
                        break
                    }
                }
            }
            if (!found) {
                lines.add("$key = \"$value\"")
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

        private fun extractSearchEngine(toml: String): String {
            val section = findSection(toml, "[web_search]")
            return extractTomlValueInBlock(section, "provider") ?: ""
        }

        private fun extractTavilyApiKey(toml: String): String {
            val section = findSection(toml, "[web_search]")
            return extractTomlValueInBlock(section, "tavily_api_key") ?: ""
        }
        private const val THINKING_ENABLED_LINE = "provider_extra = { thinking = { type = \"enabled\" } }"
        private const val THINKING_DISABLED_LINE = "provider_extra = { thinking = { type = \"disabled\" } }"
    }
}
