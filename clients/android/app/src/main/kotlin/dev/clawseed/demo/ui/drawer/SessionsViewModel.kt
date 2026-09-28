package dev.clawseed.demo.ui.drawer

import android.app.Application
import dev.clawseed.demo.R
import dev.clawseed.demo.data.LocalStore
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.SessionSummary
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull

data class SessionsUiState(
    val sessions: List<SessionSummary> = emptyList(),
    val personaVisuals: Map<String, PersonaInfo> = emptyMap(),
    val pinnedSessionIds: Set<String> = emptySet(),
    val collapsedPersonaKeys: Set<String> = emptySet(),
    val deletingSessionIds: Set<String> = emptySet(),
    val isLoading: Boolean = true,
    val error: String? = null,
    val deletionError: String? = null,
)

class SessionsViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(SessionsUiState())
    val uiState: StateFlow<SessionsUiState> = _uiState.asStateFlow()
    private var loadJob: Job? = null
    private val localStore = LocalStore(application)

    init {
        viewModelScope.launch {
            localStore.pinnedSessionIds.collect { ids ->
                _uiState.value = _uiState.value.copy(pinnedSessionIds = ids)
            }
        }
        viewModelScope.launch {
            localStore.collapsedHistoryPersonas.collect { keys ->
                _uiState.value = _uiState.value.copy(collapsedPersonaKeys = keys)
            }
        }
    }

    private fun gatewayClient(): dev.clawseed.sdk.core.client.GatewayClient {
        return ClawSeedAndroid.gatewayClient()
    }

    fun loadSessions() {
        if (_uiState.value.deletingSessionIds.isNotEmpty()) return
        loadJob?.cancel()
        loadJob = viewModelScope.launch {
            val showLoading = _uiState.value.sessions.isEmpty()
            _uiState.value = _uiState.value.copy(isLoading = showLoading, error = null)
            val ready = withTimeoutOrNull(15_000) { ClawSeedAndroid.awaitInit(); true } ?: false
            if (!ready) {
                _uiState.value = _uiState.value.copy(
                    isLoading = false,
                    error = getApplication<Application>().getString(R.string.drawer_gateway_wait_failed),
                )
                return@launch
            }
            val sessionsResult = gatewayClient().sessions()
            val personasResult = gatewayClient().personas()
            if (sessionsResult.isSuccess) {
                val visuals = personasResult.getOrElse { emptyList() }
                    .filter { it.isPersona }
                    .associateBy { it.name }
                _uiState.value = _uiState.value.copy(
                    sessions = sessionsResult.getOrThrow(),
                    personaVisuals = visuals,
                    isLoading = false,
                )
            } else {
                _uiState.value = _uiState.value.copy(
                    isLoading = false,
                    error = sessionsResult.exceptionOrNull()?.message
                        ?: getApplication<Application>().getString(R.string.drawer_gateway_wait_failed),
                )
            }
        }
    }

    fun deleteSession(sessionId: String, onSuccess: (() -> Unit)? = null) {
        deleteSessions(listOf(sessionId)) { onSuccess?.invoke() }
    }

    fun deleteSessions(sessionIds: List<String>, onDeleted: (String) -> Unit) {
        if (sessionIds.isEmpty() || _uiState.value.deletingSessionIds.isNotEmpty()) return
        loadJob?.cancel()
        val ids = sessionIds.distinct()
        _uiState.value = _uiState.value.copy(
            deletingSessionIds = ids.toSet(), isLoading = false, error = null, deletionError = null,
        )
        viewModelScope.launch {
            val deletedIds = mutableSetOf<String>()
            try {
                val result = deleteHistorySessions(
                    ids,
                    delete = { id ->
                        // Release pooled connections so deleted conversations cannot be reused.
                        ClawSeedAndroid.sessionManager().disconnect(id)
                        gatewayClient().deleteSession(id)
                    },
                    onDeleted = { id ->
                        deletedIds += id
                        _uiState.value = _uiState.value.copy(
                            sessions = _uiState.value.sessions.filterNot { it.id == id },
                        )
                        onDeleted(id)
                    },
                )
                if (result.failed.isNotEmpty()) {
                    _uiState.value = _uiState.value.copy(
                        deletionError = getApplication<Application>().getString(
                            R.string.drawer_delete_partial_failure, result.deleted.size, result.failed.size,
                        ),
                    )
                }
            } finally {
                _uiState.value = _uiState.value.copy(deletingSessionIds = emptySet())
                if (deletedIds.isNotEmpty()) {
                    saveHistoryPreference { localStore.removePinnedSessions(deletedIds) }
                }
            }
        }
    }

    fun dismissDeletionError() {
        _uiState.value = _uiState.value.copy(deletionError = null)
    }

    fun renameSession(sessionId: String, name: String) {
        viewModelScope.launch {
            gatewayClient().renameSession(sessionId, name)
                .onSuccess { loadSessions() }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(error = e.message)
                }
        }
    }

    fun togglePinned(sessionId: String) {
        saveHistoryPreference { localStore.toggleSessionPinned(sessionId) }
    }

    fun togglePersonaCollapsed(key: String) {
        saveHistoryPreference { localStore.toggleHistoryPersonaCollapsed(key) }
    }

    private fun saveHistoryPreference(save: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                save()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _uiState.value = _uiState.value.copy(
                    error = getApplication<Application>().getString(R.string.drawer_save_failed),
                )
            }
        }
    }
}
