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
import kotlinx.coroutines.withTimeoutOrNull
import java.util.Collections
import java.util.WeakHashMap
import java.util.concurrent.ConcurrentHashMap

/**
 * Application-scoped owner for active [ClawSeedSession]s.
 *
 * Maintains a pool of live sessions so that switching between conversations
 * does **not** disconnect the old session.  The gateway continues the agent
 * turn as long as the WebSocket is alive; when the user switches back, the
 * completed (or still-streaming) response is still available.
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

    /** Pool of live sessions keyed by sessionId. */
    private val sessions = ConcurrentHashMap<String, ClawSeedSession>()

    /** Tracks the last-access timestamp for LRU eviction. */
    private val lastAccessTime = ConcurrentHashMap<String, Long>()

    private val _activeSessionId = MutableStateFlow<String?>(null)
    /** SessionId of the session currently displayed in the UI. */
    val activeSessionId: StateFlow<String?> = _activeSessionId.asStateFlow()

    /** Maximum number of concurrent live sessions in the pool. */
    var maxPoolSize: Int = DEFAULT_MAX_POOL_SIZE

    companion object {
        /** Default maximum pool size. */
        const val DEFAULT_MAX_POOL_SIZE = 5
        /** Timeout in milliseconds to wait for a new session to receive its sessionId. */
        private const val SESSION_ID_TIMEOUT_MS = 10_000L
    }

    /**
     * Returns the currently active session, if one has been created.
     */
    val activeSession: StateFlow<ClawSeedSession?> = kotlinx.coroutines.flow.MutableStateFlow<ClawSeedSession?>(null).also { flow ->
        // Derive activeSession from activeSessionId + pool
        scope.launch {
            _activeSessionId.collect { sid ->
                flow.value = sid?.let { sessions[it] }
            }
        }
    }.asStateFlow()

    /**
     * Connects to an existing session or creates a new one when [sessionId] is `null`.
     *
     * Unlike the previous one-at-a-time model, this does **not** disconnect
     * the old session on switch.  Old sessions remain alive in the pool so
     * the gateway continues any ongoing agent turn.
     */
    suspend fun connect(sessionId: String? = null): ClawSeedSession {
        // ── Reuse existing session ──────────────────────────
        if (sessionId != null) {
            val existing = sessions[sessionId]
            if (existing != null) {
                val state = existing.connectionState.value
                if (state == ConnectionState.CONNECTED) {
                    touch(sessionId)
                    _activeSessionId.value = sessionId
                    return existing
                }
                if (state == ConnectionState.DISCONNECTED) {
                    existing.connect(sessionId)
                    touch(sessionId)
                    _activeSessionId.value = sessionId
                    return existing
                }
                // CONNECTING / RECONNECTING — await
                withTimeoutOrNull(SESSION_ID_TIMEOUT_MS) {
                    while (existing.connectionState.value != ConnectionState.CONNECTED &&
                        existing.connectionState.value != ConnectionState.DISCONNECTED) {
                        kotlinx.coroutines.delay(100)
                    }
                }
                if (existing.connectionState.value == ConnectionState.CONNECTED) {
                    touch(sessionId)
                    _activeSessionId.value = sessionId
                    return existing
                }
                // Failed to reconnect — remove stale entry and fall through to create
                sessions.remove(sessionId)
                lastAccessTime.remove(sessionId)
                runCatching { existing.disconnect() }
            }
        }

        // ── Evict LRU idle session if pool is full ──────────
        evictIfNeeded()

        // ── Create new session ──────────────────────────────
        val session = sessionFactory(config)
        session.connect(sessionId)

        // Bridge CETP external tools into this session's registry
        runCatching { ClawSeedAndroid.externalToolBridge().attachToRegistry(session.tools) }

        // Wait for SessionStarted event to get the real sessionId
        val realSessionId = waitForSessionId(session, sessionId)

        if (realSessionId != null) {
            sessions[realSessionId] = session
            touch(realSessionId)
            _activeSessionId.value = realSessionId
        } else {
            // Session failed to get an ID (e.g. connect error). Don't pool it.
            // Still set as active so the caller can use it.
            _activeSessionId.value = null
        }

        return session
    }

    /** Disconnects a specific session and removes it from the pool. */
    suspend fun disconnect(sessionId: String) {
        val session = sessions.remove(sessionId) ?: return
        lastAccessTime.remove(sessionId)
        runCatching { ClawSeedAndroid.externalToolBridge().detachFromRegistry() }
        session.disconnect()
        if (_activeSessionId.value == sessionId) {
            _activeSessionId.value = null
        }
    }

    /** Disconnects and clears all sessions from the pool. */
    suspend fun disconnectAll() {
        val allSessionIds = sessions.keys.toList()
        for (sid in allSessionIds) {
            val session = sessions.remove(sid)
            lastAccessTime.remove(sid)
            runCatching { session?.disconnect() }
        }
        runCatching { ClawSeedAndroid.externalToolBridge().detachFromRegistry() }
        _activeSessionId.value = null
    }

    /** Returns the pool entry for a given sessionId, or null. */
    fun getSession(sessionId: String): ClawSeedSession? = sessions[sessionId]

    /** Returns all sessionIds currently in the pool. */
    fun poolSessionIds(): Set<String> = sessions.keys.toSet()

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
                    scope.launch { disconnectAll() }
                }
            }

            override fun onStart(owner: LifecycleOwner) {
                if (this@SessionManager.disconnectOnBackground) {
                    // Reconnect the active session
                    val sid = _activeSessionId.value
                    if (sid != null) {
                        scope.launch {
                            connect(sid)
                        }
                    }
                }
            }
        }
        processLifecycleProvider().addObserver(observer)
        processLifecycleObserver = observer
    }

    // ── Internal helpers ────────────────────────────────────

    /** Test-only: add a session to the pool directly. */
    internal fun addSessionToPoolForTest(sessionId: String, session: ClawSeedSession) {
        sessions[sessionId] = session
        touch(sessionId)
    }

    /** Update last-access timestamp for LRU eviction. */
    private fun touch(sessionId: String) {
        lastAccessTime[sessionId] = System.currentTimeMillis()
    }

    /** Evict the least-recently-used idle session if the pool exceeds [maxPoolSize]. */
    private fun evictIfNeeded() {
        while (sessions.size >= maxPoolSize) {
            val activeId = _activeSessionId.value
            // Find the LRU session that is NOT the active one
            val lruEntry = lastAccessTime.entries
                .filter { it.key != activeId }
                .minByOrNull { it.value }

            if (lruEntry == null) {
                // All sessions are active; can't evict without disrupting the user.
                // Remove the oldest active session as last resort.
                val oldest = lastAccessTime.entries.minByOrNull { it.value }
                if (oldest != null) {
                    scope.launch { disconnect(oldest.key) }
                }
                break
            }

            scope.launch { disconnect(lruEntry.key) }
            // Remove synchronously from maps so the size check re-evaluates.
            // The async disconnect will handle the actual WS close.
            sessions.remove(lruEntry.key)
            lastAccessTime.remove(lruEntry.key)
        }
    }

    /** Wait for the session to receive a sessionId via SessionStarted event. */
    private suspend fun waitForSessionId(session: ClawSeedSession, fallbackId: String?): String? {
        // If we already have a sessionId (reconnect case), return immediately
        val currentId = session.sessionInfo.value?.sessionId
        if (currentId != null) return currentId

        // If we have a fallback (explicit sessionId), use it
        if (fallbackId != null) return fallbackId

        // Wait for SessionStarted event
        withTimeoutOrNull(SESSION_ID_TIMEOUT_MS) {
            while (session.sessionInfo.value?.sessionId == null) {
                kotlinx.coroutines.delay(200)
            }
        }
        return session.sessionInfo.value?.sessionId
    }
}
