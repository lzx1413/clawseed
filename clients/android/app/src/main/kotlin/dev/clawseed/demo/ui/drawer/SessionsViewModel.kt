package dev.clawseed.demo.ui.drawer

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.SessionSummary
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class SessionsUiState(
    val sessions: List<SessionSummary> = emptyList(),
    val isLoading: Boolean = false,
    val error: String? = null,
)

class SessionsViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(SessionsUiState())
    val uiState: StateFlow<SessionsUiState> = _uiState.asStateFlow()

    private fun gatewayClient(): dev.clawseed.sdk.core.client.GatewayClient {
        return ClawSeedAndroid.gatewayClient()
    }

    fun loadSessions() {
        viewModelScope.launch {
            if (!ClawSeedAndroid.isInitialized) return@launch
            val showLoading = _uiState.value.sessions.isEmpty()
            _uiState.value = _uiState.value.copy(isLoading = showLoading, error = null)
            gatewayClient().sessions()
                .onSuccess { sessions ->
                    _uiState.value = SessionsUiState(sessions = sessions, isLoading = false)
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(isLoading = false, error = e.message)
                }
        }
    }

    fun deleteSession(sessionId: String, onSuccess: (() -> Unit)? = null) {
        viewModelScope.launch {
            gatewayClient().deleteSession(sessionId)
                .onSuccess {
                    onSuccess?.invoke()
                    loadSessions()
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(error = e.message)
                }
        }
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
}
