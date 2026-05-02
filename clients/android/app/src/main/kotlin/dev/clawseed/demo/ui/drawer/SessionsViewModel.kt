package dev.clawseed.demo.ui.drawer

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.data.ChatSession
import dev.clawseed.demo.data.GatewayApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class SessionsUiState(
    val sessions: List<ChatSession> = emptyList(),
    val isLoading: Boolean = false,
    val error: String? = null,
)

class SessionsViewModel(application: Application) : AndroidViewModel(application) {

    private val api = GatewayApi()

    private val _uiState = MutableStateFlow(SessionsUiState())
    val uiState: StateFlow<SessionsUiState> = _uiState.asStateFlow()

    fun loadSessions() {
        viewModelScope.launch {
            val showLoading = _uiState.value.sessions.isEmpty()
            _uiState.value = _uiState.value.copy(isLoading = showLoading, error = null)
            api.getSessions()
                .onSuccess { sessions ->
                    _uiState.value = SessionsUiState(sessions = sessions, isLoading = false)
                }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(isLoading = false, error = e.message)
                }
        }
    }

    fun deleteSession(sessionId: String) {
        viewModelScope.launch {
            api.deleteSession(sessionId)
                .onSuccess { loadSessions() }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(error = e.message)
                }
        }
    }

    fun renameSession(sessionId: String, name: String) {
        viewModelScope.launch {
            api.renameSession(sessionId, name)
                .onSuccess { loadSessions() }
                .onFailure { e ->
                    _uiState.value = _uiState.value.copy(error = e.message)
                }
        }
    }
}
