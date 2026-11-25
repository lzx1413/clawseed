package dev.clawseed.demo.ui.persona

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.ui.settings.ToolSourceGroup
import dev.clawseed.demo.ui.settings.sourceGroup
import dev.clawseed.demo.R
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.PersonaDetail
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.PersonaUpsert
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.ToolInfo
import dev.clawseed.sdk.core.model.ProviderInfo
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.launch

data class PersonaDraft(
    val originalName: String? = null,
    val name: String = "",
    val systemPrompt: String = "",
    val model: String = "",
    val provider: String = "",
    val thinkingEnabled: Boolean = false,
    val vision: String = "auto",
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
    val availableProviders: List<ProviderInfo> = emptyList(),
    val isLoading: Boolean = false,
    val toolsLoaded: Boolean = false,
    val isFetchingModels: Boolean = false,
    val isSaving: Boolean = false,
    val editing: PersonaDraft? = null,
    val savedDraft: PersonaDraft? = null,
    val viewing: PersonaDetail? = null,
    val error: String? = null,
)

class PersonaViewModel(application: Application) : AndroidViewModel(application) {
    private val localStore = LocalStore(application)
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
            val providersResult = ClawSeedAndroid.gatewayClient().providers()
            _uiState.value = _uiState.value.copy(
                personas = personasResult.getOrElse { emptyList() }.filter { it.isPersona },
                tools = toolsResult.getOrElse { _uiState.value.tools },
                toolsLoaded = toolsResult.isSuccess || _uiState.value.toolsLoaded,
                skills = skillsResult.getOrElse { emptyList() },
                availableModels = modelsResult.getOrElse { emptyList() },
                availableProviders = providersResult.getOrElse { emptyList() },
                isLoading = false,
                error = personasResult.exceptionOrNull()?.message
                    ?: toolsResult.exceptionOrNull()?.message
                    ?: skillsResult.exceptionOrNull()?.message,
            )
        }
    }

    fun newPersona() {
        if (!_uiState.value.toolsLoaded) {
            viewModelScope.launch {
                if (!ClawSeedAndroid.isInitialized) return@launch
                ClawSeedAndroid.gatewayClient().tools()
                    .onSuccess { tools ->
                        _uiState.value = _uiState.value.copy(tools = tools, toolsLoaded = true)
                        startNewPersona()
                    }
                    .onFailure { error -> _uiState.value = _uiState.value.copy(error = error.message) }
            }
            return
        }
        startNewPersona()
    }

    private fun startNewPersona() {
        val provider = _uiState.value.availableProviders.firstOrNull { it.active }
            ?: _uiState.value.availableProviders.firstOrNull()
        _uiState.value = _uiState.value.copy(
            editing = PersonaDraft(
                provider = provider?.id.orEmpty(),
                model = provider?.models?.firstOrNull() ?: provider?.model.orEmpty(),
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
                    val draft = detail.toDraft(_uiState.value.tools, _uiState.value.availableProviders)
                    _uiState.value = _uiState.value.copy(
                        isLoading = false,
                        viewing = null,
                        editing = draft, savedDraft = draft,
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

    private val choiceWrites = Mutex()

    fun applyChoice(change: (PersonaDraft) -> PersonaDraft) {
        val name = _uiState.value.editing?.originalName
        if (name == null) {
            updateDraft(change)
            return
        }
        viewModelScope.launch {
            choiceWrites.withLock {
                val state = _uiState.value
                if (state.editing?.originalName != name) return@withLock
                _uiState.value = state.copy(isSaving = true, error = null)
                try {
                    val client = ClawSeedAndroid.gatewayClient()
                    val detail = client.persona(name).getOrThrow()
                    val saved = detail.toDraft(state.tools, state.availableProviders)
                    val next = change(saved)
                    client.upsertPersona(name, detail.withChanges(saved, next)).getOrThrow()
                    val current = _uiState.value.editing
                    if (current?.originalName == name) {
                        _uiState.value = _uiState.value.copy(editing = change(current), savedDraft = next)
                    }
                    load()
                } catch (error: Exception) {
                    if (error is kotlinx.coroutines.CancellationException) throw error
                    _uiState.value = _uiState.value.copy(error = error.message)
                } finally {
                    _uiState.value = _uiState.value.copy(isSaving = false)
                }
            }
        }
    }

    fun fetchModelsForEditingProvider() {
        val providerId = _uiState.value.editing?.provider?.takeIf(String::isNotBlank) ?: return
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isFetchingModels = true, error = null)
            ClawSeedAndroid.gatewayClient().models(providerId)
                .onSuccess { models ->
                    val normalized = models.map(String::trim).filter(String::isNotBlank).distinct()
                    val state = _uiState.value
                    val draft = state.editing
                    _uiState.value = state.copy(
                        isFetchingModels = false,
                        availableProviders = state.availableProviders.map { provider ->
                            if (provider.id == providerId) provider.copy(models = normalized) else provider
                        },
                        editing = draft?.takeIf { it.provider == providerId }?.let {
                            if (it.model.isBlank()) it.copy(model = normalized.firstOrNull().orEmpty()) else it
                        } ?: draft,
                    )
                }
                .onFailure { error ->
                    _uiState.value = _uiState.value.copy(
                        isFetchingModels = false,
                        error = error.message,
                    )
                }
        }
    }

    fun closeEditor() {
        _uiState.value = _uiState.value.copy(editing = null, viewing = null, error = null)
    }

    fun save() {
        val draft = _uiState.value.editing ?: return
        val name = draft.name.trim()
        if (name.isEmpty()) {
            _uiState.value = _uiState.value.copy(error = getApplication<Application>().getString(R.string.persona_name_required))
            return
        }
        if (draft.provider.isBlank() || draft.model.isBlank()) {
            _uiState.value = _uiState.value.copy(error = getApplication<Application>().getString(R.string.persona_llm_required))
            return
        }
        if (!draft.hasPersonaOverrides()) {
            _uiState.value = _uiState.value.copy(error = getApplication<Application>().getString(R.string.persona_override_required))
            return
        }

        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isSaving = true, error = null)
            val localAvatar = PersonaAvatarStorage.ensureLocalAvatar(getApplication(), name, draft.avatar)
            val edited = draft.copy(avatar = localAvatar)
            val gateway = ClawSeedAndroid.gatewayClient()
            val result = runCatching {
                val payload = draft.originalName?.let { original ->
                    val detail = gateway.persona(original).getOrThrow()
                    detail.withChanges(detail.toDraft(_uiState.value.tools, _uiState.value.availableProviders), edited)
                } ?: edited.toUpsert()
                gateway.upsertPersona(name, payload).getOrThrow()
            }
            result
                .onSuccess {
                    val oldName = draft.originalName
                    if (oldName != null && oldName != name) {
                        val deleted = ClawSeedAndroid.gatewayClient().deletePersona(oldName)
                        if (deleted.isSuccess) {
                            dev.clawseed.demo.data.LocalStore(getApplication()).moveSuggestionCards(oldName, name)
                            if (localStore.defaultPersonaName.first() == oldName) {
                                localStore.setDefaultPersonaName(name)
                            }
                        }
                    }
                    _uiState.value = _uiState.value.copy(isSaving = false, editing = null)
                    load()
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
                        editing = detail.toDraft(_uiState.value.tools, _uiState.value.availableProviders).copy(
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
                .onSuccess {
                    dev.clawseed.demo.data.LocalStore(getApplication()).setSuggestionCards(name, null)
                    if (localStore.defaultPersonaName.first() == name) {
                        localStore.setDefaultPersonaName("default")
                    }
                    load()
                }
                .onFailure { e -> _uiState.value = _uiState.value.copy(error = e.message) }
        }
    }

    private fun PersonaDetail.toDraft(tools: List<ToolInfo>, providers: List<ProviderInfo>): PersonaDraft {
        val ns = memoryNamespace.orEmpty()
        val selectedProvider = providers.firstOrNull { it.id == provider }
            ?: providers.firstOrNull { it.active }
            ?: providers.firstOrNull()
        return PersonaDraft(
            originalName = name,
            name = name,
            systemPrompt = systemPrompt ?: identity?.takeIf { hasIdentity }?.toString().orEmpty(),
            model = model?.takeIf(String::isNotBlank)
                ?: selectedProvider?.models?.firstOrNull()
                ?: selectedProvider?.model.orEmpty(),
            provider = provider?.takeIf(String::isNotBlank) ?: selectedProvider?.id.orEmpty(),
            thinkingEnabled = thinkingEnabled ?: false,
            vision = vision ?: "auto",
            avatar = avatar.orEmpty(),
            color = color.orEmpty(),
            memoryMode = if (ns.isBlank()) "shared" else "isolated",
            allowedTools = if ("*" in allowedTools) {
                tools.map { it.name }.toSet()
            } else {
                allowedTools.toSet()
            },
            deniedSkills = deniedSkills.toSet(),
        )
    }

    private fun PersonaDraft.hasPersonaOverrides(): Boolean =
        systemPrompt.isNotBlank()
            || memoryMode == "isolated"
            || allowedTools.isNotEmpty()
            || deniedSkills.isNotEmpty()
            || provider.isNotBlank()
            || model.isNotBlank()
            || avatar.isNotBlank()
            || color.isNotBlank()

}

internal fun defaultAllowedTools(tools: List<ToolInfo>): Set<String> =
    tools.filter { it.sourceGroup() == ToolSourceGroup.BUILTIN }.map { it.name }.toSet()

internal fun PersonaDraft.toUpsert(): PersonaUpsert {
    return PersonaUpsert(
        provider = provider.trim(),
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
        model = model.trim(),
        thinkingEnabled = thinkingEnabled,
        vision = vision,
        avatar = avatar.trim().ifEmpty { null },
        color = color.trim().ifEmpty { null },
    )
}

// A PUT replaces the persona. Keep inherited and legacy fields exactly as
// stored when the user changes a different setting.
internal fun PersonaDetail.withChanges(before: PersonaDraft, after: PersonaDraft): PersonaUpsert {
    val changed = after.toUpsert()
    return PersonaUpsert(
        provider = if (before.provider != after.provider) changed.provider else provider,
        model = if (before.model != after.model || before.provider != after.provider) changed.model else model,
        vision = if (before.vision != after.vision) changed.vision else vision,
        thinkingEnabled = if (before.thinkingEnabled != after.thinkingEnabled) changed.thinkingEnabled else thinkingEnabled,
        avatar = if (before.avatar != after.avatar) changed.avatar else avatar,
        color = if (before.color != after.color) changed.color else color,
        memoryNamespace = if (before.memoryMode != after.memoryMode || (before.name != after.name && memoryNamespace == before.name)) changed.memoryNamespace else memoryNamespace,
        allowedTools = if (before.allowedTools != after.allowedTools) changed.allowedTools else allowedTools,
        deniedTools = deniedTools,
        deniedSkills = if (before.deniedSkills != after.deniedSkills) changed.deniedSkills else deniedSkills,
        identity = if (before.systemPrompt != after.systemPrompt) null else identity,
        systemPrompt = if (before.systemPrompt != after.systemPrompt) changed.systemPrompt else systemPrompt,
    )
}
