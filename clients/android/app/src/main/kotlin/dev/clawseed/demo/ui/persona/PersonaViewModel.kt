package dev.clawseed.demo.ui.persona

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.R
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.PersonaDetail
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.PersonaUpsert
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.ToolInfo
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class PersonaDraft(
    val originalName: String? = null,
    val name: String = "",
    val systemPrompt: String = "",
    val model: String = "",
    val thinkingEnabled: Boolean? = null,
    val avatar: String = "",
    val color: String = "",
    val memoryMode: String = "shared",
    val allowedTools: Set<String> = emptySet(),
    val deniedSkills: Set<String> = emptySet(),
)

data class PersonaUiState(
    val personas: List<PersonaInfo> = emptyList(),
    val tools: List<ToolInfo> = emptyList(),
    val skills: List<SkillInfo> = emptyList(),
    val availableModels: List<String> = emptyList(),
    val isLoading: Boolean = false,
    val isSaving: Boolean = false,
    val editing: PersonaDraft? = null,
    val viewing: PersonaDetail? = null,
    val error: String? = null,
)

class PersonaViewModel(application: Application) : AndroidViewModel(application) {
    private val _uiState = MutableStateFlow(PersonaUiState())
    val uiState: StateFlow<PersonaUiState> = _uiState.asStateFlow()

    fun load() {
        viewModelScope.launch {
            if (!ClawSeedAndroid.isInitialized) return@launch
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)
            val personasResult = ClawSeedAndroid.gatewayClient().personas()
            val toolsResult = ClawSeedAndroid.gatewayClient().tools()
            val skillsResult = ClawSeedAndroid.gatewayClient().skills()
            val modelsResult = ClawSeedAndroid.gatewayClient().models()
            _uiState.value = _uiState.value.copy(
                personas = personasResult.getOrElse { emptyList() }.filter { it.isPersona },
                tools = toolsResult.getOrElse { emptyList() },
                skills = skillsResult.getOrElse { emptyList() },
                availableModels = modelsResult.getOrElse { emptyList() },
                isLoading = false,
                error = personasResult.exceptionOrNull()?.message
                    ?: toolsResult.exceptionOrNull()?.message
                    ?: skillsResult.exceptionOrNull()?.message,
            )
        }
    }

    fun newPersona() {
        _uiState.value = _uiState.value.copy(
            editing = PersonaDraft(
                allowedTools = defaultAllowedTools(_uiState.value.tools),
            ),
            viewing = null,
            error = null,
        )
    }

    fun edit(name: String) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)
            ClawSeedAndroid.gatewayClient().persona(name)
                .onSuccess { detail ->
                    _uiState.value = _uiState.value.copy(
                        isLoading = false,
                        viewing = null,
                        editing = detail.toDraft(_uiState.value.tools),
                    )
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(isLoading = false, error = e.message)
                }
        }
    }

    fun view(name: String) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)
            ClawSeedAndroid.gatewayClient().persona(name)
                .onSuccess { detail ->
                    _uiState.value = _uiState.value.copy(isLoading = false, viewing = detail, editing = null)
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(isLoading = false, error = e.message)
                }
        }
    }

    fun updateDraft(transform: (PersonaDraft) -> PersonaDraft) {
        val draft = _uiState.value.editing ?: return
        _uiState.value = _uiState.value.copy(editing = transform(draft), error = null)
    }

    fun closeEditor() {
        _uiState.value = _uiState.value.copy(editing = null, viewing = null, error = null)
    }

    fun save(onSaved: ((String) -> Unit)? = null) {
        val draft = _uiState.value.editing ?: return
        val name = draft.name.trim()
        if (name.isEmpty()) {
            _uiState.value = _uiState.value.copy(error = getApplication<Application>().getString(R.string.persona_name_required))
            return
        }
        if (!draft.hasPersonaOverrides()) {
            _uiState.value = _uiState.value.copy(error = getApplication<Application>().getString(R.string.persona_override_required))
            return
        }

        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isSaving = true, error = null)
            val localAvatar = PersonaAvatarStorage.ensureLocalAvatar(getApplication(), name, draft.avatar)
            val upsert = draft.copy(avatar = localAvatar).toUpsert()
            ClawSeedAndroid.gatewayClient().upsertPersona(name, upsert)
                .onSuccess {
                    val oldName = draft.originalName
                    if (oldName != null && oldName != name) {
                        ClawSeedAndroid.gatewayClient().deletePersona(oldName)
                    }
                    _uiState.value = _uiState.value.copy(isSaving = false, editing = null)
                    load()
                    onSaved?.invoke(name)
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(isSaving = false, error = e.message)
                }
        }
    }

    fun duplicate(info: PersonaInfo) {
        viewModelScope.launch {
            ClawSeedAndroid.gatewayClient().persona(info.name)
                .onSuccess { detail ->
                    _uiState.value = _uiState.value.copy(
                        editing = detail.toDraft(_uiState.value.tools).copy(
                            originalName = null,
                            name = "${detail.name}-copy",
                        ),
                        viewing = null,
                    )
                }
                .onFailure { e -> _uiState.value = _uiState.value.copy(error = e.message) }
        }
    }

    fun delete(name: String) {
        viewModelScope.launch {
            ClawSeedAndroid.gatewayClient().deletePersona(name)
                .onSuccess { load() }
                .onFailure { e -> _uiState.value = _uiState.value.copy(error = e.message) }
        }
    }

    private fun PersonaDetail.toDraft(tools: List<ToolInfo>): PersonaDraft {
        val ns = memoryNamespace.orEmpty()
        return PersonaDraft(
            originalName = name,
            name = name,
            systemPrompt = systemPrompt ?: identity?.toString().orEmpty(),
            model = model.orEmpty(),
            thinkingEnabled = thinkingEnabled,
            avatar = avatar.orEmpty(),
            color = color.orEmpty(),
            memoryMode = if (ns.isBlank()) "shared" else "isolated",
            allowedTools = if (allowedTools.isEmpty()) defaultAllowedTools(tools) else allowedTools.toSet(),
            deniedSkills = deniedSkills.toSet(),
        )
    }

    private fun PersonaDraft.toUpsert(): PersonaUpsert {
        return PersonaUpsert(
            identity = null,
            systemPrompt = systemPrompt.trim().ifEmpty { null },
            memoryNamespace = if (memoryMode == "isolated") {
                name.trim()
            } else {
                null
            },
            allowedTools = allowedTools.sorted(),
            deniedTools = emptyList(),
            deniedSkills = deniedSkills.sorted(),
            model = model.trim().ifEmpty { null },
            thinkingEnabled = thinkingEnabled,
            avatar = avatar.trim().ifEmpty { null },
            color = color.trim().ifEmpty { null },
        )
    }

    private fun PersonaDraft.hasPersonaOverrides(): Boolean =
        systemPrompt.isNotBlank()
            || memoryMode == "isolated"
            || allowedTools.isNotEmpty()
            || deniedSkills.isNotEmpty()
            || model.isNotBlank()
            || thinkingEnabled != null
            || avatar.isNotBlank()
            || color.isNotBlank()

    private fun defaultAllowedTools(tools: List<ToolInfo>): Set<String> =
        tools.filter { it.sourceType == "builtin" }.map { it.name }.toSet()
}
