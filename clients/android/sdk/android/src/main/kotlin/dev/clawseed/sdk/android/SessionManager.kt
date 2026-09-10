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
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.Collections
import java.util.WeakHashMap
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

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
    private val scope: CoroutineScope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
) {
    private val poolMutex = Mutex()
    private val accessCounter = AtomicLong()
    private val generatingSessions = ConcurrentHashMap.newKeySet<String>()
    private val _pooledSessions = MutableStateFlow<Map<String, ClawSeedSession>>(emptyMap())
    /** Snapshot used by UI caches to release state when a session leaves the pool. */
    val pooledSessions = _pooledSessions.asStateFlow()
    private val observedLifecycles = Collections.newSetFromMap(WeakHashMap<Lifecycle, Boolean>())
    @Volatile private var processLifecycleObserver: DefaultLifecycleObserver? = null
    @Volatile private var disconnectOnBackground = false

    /** Pool of live sessions keyed by sessionId. */
    private val sessions = ConcurrentHashMap<String, ClawSeedSession>()

    /** Monotonic access order for LRU eviction, including rapid switches. */
    private val lastAccessTime = ConcurrentHashMap<String, Long>()

    private val _activeSessionId = MutableStateFlow<String?>(null)
    /** SessionId of the session currently displayed in the UI. */
    val activeSessionId: StateFlow<String?> = _activeSessionId.asStateFlow()

    /** Target pool size; unfinished turns may temporarily exceed this limit. */
    var maxPoolSize: Int = DEFAULT_MAX_POOL_SIZE
        set(value) { require(value > 0); field = value }

    companion object {
        /** Default maximum pool size. */
        const val DEFAULT_MAX_POOL_SIZE = 5
        /** Timeout in milliseconds to wait for a new session to receive its sessionId. */
        private const val SESSION_ID_TIMEOUT_MS = 10_000L
    }

    /**
     * Returns the currently active session, if one has been created.
     */
    private val _activeSession = MutableStateFlow<ClawSeedSession?>(null)
    val activeSession = _activeSession.asStateFlow()

    private fun activate(sessionId: String) {
        touch(sessionId)
        _activeSessionId.value = sessionId
        _activeSession.value = sessions[sessionId]
    }

    /** Generating sessions can temporarily exceed the idle-cache limit. */
    fun setSessionGenerating(sessionId: String, generating: Boolean) {
        if (generating) generatingSessions.add(sessionId)
        else if (generatingSessions.remove(sessionId)) {
            scope.launch { poolMutex.withLock { evictIfNeeded() } }
        }
    }

    /**
     * Connects to an existing session or creates a new one when [sessionId] is `null`.
     *
     * [persona] selects a named persona for a **new** session (sent as
     * `?persona=`). On resume the gateway ignores it and uses the stored
     * binding — persona is write-once per session.
     *
     * Unlike the previous one-at-a-time model, this does **not** disconnect
     * the old session on switch.  Old sessions remain alive in the pool so
     * the gateway continues any ongoing agent turn.
     */
    suspend fun connect(sessionId: String? = null, persona: String? = null): ClawSeedSession =
        poolMutex.withLock { connectLocked(sessionId, persona) }

    private suspend fun connectLocked(sessionId: String?, persona: String?): ClawSeedSession {
        // ── Reuse existing session ──────────────────────────
        if (sessionId != null) {
            val existing = sessions[sessionId]
            if (existing != null) {
                val state = existing.connectionState.value
                if (state == ConnectionState.CONNECTED) {
                    activate(sessionId)
                    return existing
                }
                if (state == ConnectionState.DISCONNECTED) {
                    existing.connect(sessionId, persona)
                    activate(sessionId)
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
                    activate(sessionId)
                    return existing
                }
                // Failed to reconnect — remove stale entry and fall through to create
                disconnectLocked(sessionId)
            }
        }

        // ── Create new session ──────────────────────────────
        val session = sessionFactory(config)
        try {
            session.connect(sessionId, persona)
            val realSessionId = waitForSessionId(session, sessionId)
                ?: error("Session did not receive an ID")
            sessions[realSessionId] = session
            activate(realSessionId)
            _pooledSessions.value = sessions.toMap()
            runCatching { ClawSeedAndroid.externalToolBridge().attachToRegistry(session.tools) }
            evictIfNeeded()
            return session
        } catch (error: Throwable) {
            // A cancelled connection must not leave an unowned WebSocket behind.
            val key = sessions.entries.firstOrNull { it.value === session }?.key
            if (key != null) disconnectLocked(key)
            else withContext(NonCancellable) {
                try { session.disconnect() } finally { session.close() }
            }
            throw error
        }
    }

    /** Disconnects a specific session and removes it from the pool. */
    suspend fun disconnect(sessionId: String) = poolMutex.withLock {
        disconnectLocked(sessionId)
    }

    private suspend fun disconnectLocked(sessionId: String) {
        val session = sessions.remove(sessionId) ?: return
        lastAccessTime.remove(sessionId)
        generatingSessions.remove(sessionId)
        _pooledSessions.value = sessions.toMap()
        if (_activeSessionId.value == sessionId) {
            _activeSessionId.value = null
            _activeSession.value = null
            runCatching { ClawSeedAndroid.externalToolBridge().detachFromRegistry() }
        }
        // Keep the removed object until close finishes, even if the caller is cancelled.
        withContext(NonCancellable) {
            try { session.disconnect() } finally { session.close() }
        }
    }

    /** Disconnects and clears all sessions from the pool. */
    suspend fun disconnectAll() = poolMutex.withLock {
        for (sid in sessions.keys.toList()) disconnectLocked(sid)
        runCatching { ClawSeedAndroid.externalToolBridge().detachFromRegistry() }
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
        _pooledSessions.value = sessions.toMap()
        touch(sessionId)
    }

    /** Update access order for LRU eviction. */
    private fun touch(sessionId: String) {
        lastAccessTime[sessionId] = accessCounter.incrementAndGet()
    }

    /** Retain the visible session and any unfinished turns; bound idle history. */
    private suspend fun evictIfNeeded() {
        while (sessions.size > maxPoolSize) {
            val oldestIdle = lastAccessTime.entries
                .filter { it.key != _activeSessionId.value && it.key !in generatingSessions }
                .minByOrNull { it.value } ?: break
            disconnectLocked(oldestIdle.key)
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
