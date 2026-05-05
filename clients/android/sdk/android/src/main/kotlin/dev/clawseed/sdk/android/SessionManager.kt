package dev.clawseed.sdk.android

import android.app.Application
import android.content.Context
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import dev.clawseed.sdk.core.ClawSeedConfig
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.ClawSeed
import dev.clawseed.sdk.core.model.ConnectionState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.util.Collections
import java.util.WeakHashMap

/**
 * Application-scoped owner for the active [ClawSeedSession].
 */
class SessionManager internal constructor(
    private val config: ClawSeedConfig,
    private val sessionFactory: (ClawSeedConfig) -> ClawSeedSession = ClawSeed::createSession,
    private val appContextProvider: () -> Context? = { runCatching { ClawSeedAndroid.context }.getOrNull() },
    private val processLifecycleProvider: () -> Lifecycle = { ProcessLifecycleOwner.get().lifecycle },
) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private val observedLifecycles = Collections.newSetFromMap(WeakHashMap<Lifecycle, Boolean>())
    @Volatile private var processLifecycleObserver: DefaultLifecycleObserver? = null
    @Volatile private var disconnectOnBackground = false

    private val _activeSession = MutableStateFlow<ClawSeedSession?>(null)
    /** Currently active session, if one has been created. */
    val activeSession: StateFlow<ClawSeedSession?> = _activeSession.asStateFlow()

    /**
     * Connects to an existing session or creates a new one when [sessionId] is `null`.
     */
    suspend fun connect(sessionId: String? = null): ClawSeedSession {
        val current = _activeSession.value
        if (
            current != null &&
            sessionId == null &&
            current.connectionState.value == ConnectionState.DISCONNECTED &&
            current.sessionInfo.value?.sessionId != null
        ) {
            current.connect()
            return current
        }
        // Disconnect old session if it's for a different sessionId
        if (current != null) {
            val currentSid = current.sessionInfo.value?.sessionId
            if (currentSid != null && currentSid == sessionId && current.connectionState.value == ConnectionState.CONNECTED) {
                return current
            }
            current.disconnect()
            _activeSession.value = null
        }
        val session = sessionFactory(config)
        session.connect(sessionId)
        // Bridge CETP external tools into this session's registry
        runCatching { ClawSeedAndroid.externalToolBridge().attachToRegistry(session.tools) }
        _activeSession.value = session
        return session
    }

    /** Disconnects and clears the active session, if present. */
    suspend fun disconnect() {
        runCatching { ClawSeedAndroid.externalToolBridge().detachFromRegistry() }
        _activeSession.value?.disconnect()
        _activeSession.value = null
    }

    /** Registers lightweight lifecycle cleanup for a UI-owned [lifecycle]. */
    fun observeLifecycle(lifecycle: Lifecycle) {
        if (!observedLifecycles.add(lifecycle)) {
            return
        }
        val observer = object : DefaultLifecycleObserver {
            override fun onDestroy(owner: LifecycleOwner) {
                observedLifecycles.remove(lifecycle)
                lifecycle.removeObserver(this)
            }
        }
        lifecycle.addObserver(observer)
    }

    /** Binds reconnect and disconnect behavior to the process lifecycle. */
    fun bindToProcessLifecycle(disconnectOnBackground: Boolean = false) {
        val appContext = appContextProvider()
        if (appContext !is Application) return

        this.disconnectOnBackground = disconnectOnBackground
        if (processLifecycleObserver != null) {
            return
        }

        val observer = object : DefaultLifecycleObserver {
            override fun onStop(owner: LifecycleOwner) {
                if (this@SessionManager.disconnectOnBackground) {
                    _activeSession.value?.let { session ->
                        if (session.connectionState.value == ConnectionState.CONNECTED) {
                            scope.launch { session.disconnect() }
                        }
                    }
                }
            }

            override fun onStart(owner: LifecycleOwner) {
                if (this@SessionManager.disconnectOnBackground) {
                    _activeSession.value?.let { session ->
                        if (session.connectionState.value == ConnectionState.DISCONNECTED) {
                            scope.launch { session.connect() }
                        }
                    }
                }
            }
        }

        processLifecycleProvider().addObserver(observer)
        processLifecycleObserver = observer
    }
}
