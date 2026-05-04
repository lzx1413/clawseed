package dev.clawseed.sdk.android

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.model.ConnectionState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Convenience base [AndroidViewModel] that wires a session and accumulator to `viewModelScope`.
 */
open class ClawSeedViewModel(application: Application) : AndroidViewModel(application) {
    private val sessionManager by lazy { ClawSeedAndroid.sessionManager() }
    private val _session = MutableStateFlow<ClawSeedSession?>(null)
    private val _connectionState = MutableStateFlow(ConnectionState.DISCONNECTED)
    private var _accumulator: ChatAccumulator? = null

    /** Active session once initialization has completed. */
    protected val session: ClawSeedSession
        get() = _session.value ?: error("Session not yet available")

    /** Accumulator bound to the active session. */
    protected val accumulator: ChatAccumulator
        get() = _accumulator ?: error("Session not yet available")

    init {
        viewModelScope.launch {
            val activeSession = sessionManager.activeSession.value ?: sessionManager.connect()
            _session.value = activeSession
            _accumulator = ChatAccumulator(activeSession).also { it.startIn(viewModelScope) }
            activeSession.connectionState.collect { state ->
                _connectionState.value = state
            }
        }
    }

    /** Connection state exposed for UI consumption. */
    val connectionState: StateFlow<ConnectionState>
        get() = _connectionState.asStateFlow()

    /** Adds a user message locally and forwards it to the active session. */
    fun sendMessage(content: String) {
        val activeSession = _session.value ?: return
        _accumulator?.addUserMessage(content)
        activeSession.sendMessage(content)
    }

    /** Requests abortion of the current active turn. */
    fun abort() {
        viewModelScope.launch { _session.value?.abort() }
    }
}
