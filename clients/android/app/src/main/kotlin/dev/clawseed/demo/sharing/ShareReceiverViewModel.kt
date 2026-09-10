package dev.clawseed.demo.sharing

import android.app.Application
import android.content.Intent
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal data class ShareReceiveState(val receiving: Boolean = false, val openSession: String? = null, val error: String? = null)

internal class ShareReceiverViewModel(application: Application) : AndroidViewModel(application) {
    private val inbox = SharedInbox(application)
    private val requests = mutableSetOf<String>()
    private val mutex = Mutex()
    private val _state = MutableStateFlow(ShareReceiveState())
    val state = _state.asStateFlow()

    fun receive(id: String, intent: Intent) {
        if (!requests.add(id)) return
        viewModelScope.launch {
            mutex.withLock {
                _state.value = ShareReceiveState(receiving = true)
                try {
                    val bundle = inbox.receive(id, intent)
                    _state.value = ShareReceiveState(openSession = bundle.id)
                } catch (error: Exception) {
                    if (error is kotlinx.coroutines.CancellationException) throw error
                    _state.value = ShareReceiveState(error = error.message ?: "无法接收分享，请重新分享")
                }
            }
        }
    }

    fun recover() {
        viewModelScope.launch {
            mutex.withLock {
                if (_state.value.openSession == null && requests.isEmpty()) {
                    inbox.pending()?.let { _state.value = ShareReceiveState(openSession = it.id) }
                }
            }
        }
    }

    fun navigationHandled(id: String) {
        if (_state.value.openSession == id) _state.value = _state.value.copy(openSession = null)
    }

    fun dismissError() { _state.value = _state.value.copy(error = null) }
}
