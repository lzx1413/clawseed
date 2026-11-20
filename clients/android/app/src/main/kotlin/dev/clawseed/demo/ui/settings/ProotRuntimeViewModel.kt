package dev.clawseed.demo.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.BuildConfig
import dev.clawseed.sdk.embedded.ProotRuntimeConfig
import dev.clawseed.sdk.embedded.ProotRuntimeManager
import dev.clawseed.sdk.embedded.ProotRuntimeProgress
import dev.clawseed.sdk.embedded.ProotRuntimeState
import dev.clawseed.sdk.embedded.ProotRuntimeStatus
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class ProotRuntimeUiState(
    val status: ProotRuntimeStatus = ProotRuntimeStatus(ProotRuntimeState.NOT_INSTALLED),
    val progress: ProotRuntimeProgress? = null,
    val isBusy: Boolean = false,
    val manifestConfigured: Boolean = false,
)

class ProotRuntimeViewModel(application: Application) : AndroidViewModel(application) {
    private val manager = ProotRuntimeManager(
        application,
        ProotRuntimeConfig(
            manifestUrl = BuildConfig.PROOT_RUNTIME_MANIFEST_URL.takeIf { it.isNotBlank() },
            publisherKeys = BuildConfig.PROOT_RUNTIME_PUBLISHER_KEY
                .takeIf { it.isNotBlank() }
                ?.let { mapOf(BuildConfig.PROOT_RUNTIME_PUBLISHER_KEY_ID to it) }
                ?: emptyMap(),
            allowUnsignedManifest = BuildConfig.PROOT_RUNTIME_ALLOW_UNSIGNED,
        ),
    )
    private val _uiState = MutableStateFlow(
        ProotRuntimeUiState(
            manifestConfigured = BuildConfig.PROOT_RUNTIME_MANIFEST_URL.isNotBlank(),
        ),
    )
    val uiState: StateFlow<ProotRuntimeUiState> = _uiState.asStateFlow()

    init {
        refresh()
    }

    fun refresh() {
        if (_uiState.value.isBusy) return
        viewModelScope.launch {
            val status = withContext(Dispatchers.IO) { manager.status() }
            _uiState.update { it.copy(status = status) }
        }
    }

    fun installOrRetry() {
        if (_uiState.value.isBusy || !_uiState.value.manifestConfigured) return
        viewModelScope.launch {
            _uiState.update { it.copy(isBusy = true, progress = ProotRuntimeProgress("manifest")) }
            val status = manager.ensureReady { progress ->
                _uiState.update { it.copy(progress = progress) }
            }
            _uiState.update { it.copy(status = status, progress = null, isBusy = false) }
        }
    }

    fun clearInstalled() {
        if (_uiState.value.isBusy) return
        viewModelScope.launch {
            _uiState.update { it.copy(isBusy = true) }
            withContext(Dispatchers.IO) { manager.clearInstalled() }
            val status = withContext(Dispatchers.IO) { manager.status() }
            _uiState.update { it.copy(status = status, isBusy = false) }
        }
    }
}
