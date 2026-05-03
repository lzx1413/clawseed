package dev.clawseed.demo.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.data.GatewayApi
import dev.clawseed.demo.data.StatusInfo
import dev.clawseed.demo.data.ToolInfo
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
    val status: StatusInfo? = null,
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
)

enum class EditMode { FORM, TOML }

class SettingsViewModel : ViewModel() {

    private val api = GatewayApi()
    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()
    private var preservedApiKey: String? = null

    init {
        loadAll()
    }

    fun loadAll() {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)

            val statusResult = api.getStatus()
            val toolsResult = api.getTools()
            val configResult = api.getConfig()

            val toml = configResult.getOrElse { "" }
            val status = statusResult.getOrNull()

            // Extract current provider settings from TOML
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
        val rawApiKey = saved?.first ?: ""
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
            selectedModel = saved?.second ?: "",
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

    fun fetchModels() {
        val state = _uiState.value
        if (state.baseUrl.isBlank()) return

        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isFetchingModels = true, connectionOk = null, error = null)
            api.fetchModels(state.baseUrl, state.apiKey)
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
            api.putConfig(toml)
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

        // Rename provider ID everywhere (section headers, fallback, sub-tables)
        if (oldFallback.isNotBlank() && oldFallback != newProviderId) {
            toml = toml.replace("\"$oldFallback\"", "\"$newProviderId\"")
        } else if (oldFallback.isBlank()) {
            toml = replaceOrAppendTomlValue(toml, "fallback", newProviderId)
        }

        // Update values in the provider section
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
            }
            val agentIdx = toml.indexOf("\n[agent]")
            toml = if (agentIdx >= 0) {
                toml.substring(0, agentIdx) + section + toml.substring(agentIdx)
            } else {
                toml + section
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
            // Extract from custom:url format
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

        private fun extractProviderModel(toml: String, status: StatusInfo?): String {
            val fallback = extractTomlValue(toml, "fallback") ?: return ""
            val section = findSection(toml, "[providers.models.\"$fallback\"]")
            return extractTomlValueInBlock(section, "model") ?: ""
        }

        private fun findSavedProviderSettings(toml: String, baseUrl: String): Pair<String, String>? {
            if (baseUrl.isBlank()) return null
            val trimmedUrl = baseUrl.trimEnd('/')
            val sectionHeader = "[providers.models.\"custom:$trimmedUrl\"]"
            val section = findSection(toml, sectionHeader)
            if (section.isEmpty()) return null
            val apiKey = extractTomlValueInBlock(section, "api_key") ?: ""
            val model = extractTomlValueInBlock(section, "model") ?: ""
            return Pair(apiKey, model)
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
    }
}
