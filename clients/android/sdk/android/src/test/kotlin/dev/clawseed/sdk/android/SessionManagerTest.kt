package dev.clawseed.sdk.android

import android.app.Application
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleObserver
import dev.clawseed.sdk.core.ClawSeedConfig
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.runTest
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertSame
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class SessionManagerTest {

    @Test
    fun connectReusesPooledSessionWhenSessionIdIsKnown() = runTest {
        val session = FakeSession(
            initialConnectionState = ConnectionState.DISCONNECTED,
            initialSessionInfo = SessionInfo(
                sessionId = "session-1",
                name = "Session 1",
                resumed = true,
                messageCount = 3,
            ),
        )
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = { error("sessionFactory should not be called when reusing a pooled session") },
            appContextProvider = { null },
            processLifecycleProvider = { CountingLifecycle() },
        )

        // Add the session to the pool directly
        manager.addSessionToPoolForTest("session-1", session)

        val connectedSession = manager.connect("session-1")

        assertSame(session, connectedSession)
        assertEquals(1, session.connectCalls)
        assertEquals("session-1", session.lastConnectSessionId)
    }

    @Test
    fun connectReturnsExistingConnectedSessionWithoutReconnect() = runTest {
        val session = FakeSession(
            initialConnectionState = ConnectionState.CONNECTED,
            initialSessionInfo = SessionInfo(
                sessionId = "session-2",
                name = "Session 2",
                resumed = true,
                messageCount = 0,
            ),
        )
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = { error("sessionFactory should not be called") },
            appContextProvider = { null },
            processLifecycleProvider = { CountingLifecycle() },
        )

        manager.addSessionToPoolForTest("session-2", session)

        val connectedSession = manager.connect("session-2")

        assertSame(session, connectedSession)
        // No extra connect call — session was already CONNECTED
        assertEquals(0, session.connectCalls)
    }

    @Test
    fun connectCreatesNewSessionWhenNotInPool() = runTest {
        var factoryCalled = false
        val newSession = FakeSession(
            initialConnectionState = ConnectionState.CONNECTED,
            initialSessionInfo = SessionInfo(
                sessionId = "new-session",
                name = "New Session",
                resumed = false,
                messageCount = 0,
            ),
        )
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = {
                factoryCalled = true
                newSession
            },
            appContextProvider = { null },
            processLifecycleProvider = { CountingLifecycle() },
        )

        val connectedSession = manager.connect("new-session")

        assertTrue(factoryCalled)
        assertSame(newSession, connectedSession)
    }

    @Test
    fun disconnectRemovesSessionFromPool() = runTest {
        val session = FakeSession(
            initialConnectionState = ConnectionState.CONNECTED,
            initialSessionInfo = SessionInfo(
                sessionId = "session-1",
                name = "Session 1",
                resumed = true,
                messageCount = 3,
            ),
        )
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = { FakeSession() },
            appContextProvider = { null },
            processLifecycleProvider = { CountingLifecycle() },
        )

        manager.addSessionToPoolForTest("session-1", session)
        manager.disconnect("session-1")

        assertEquals(ConnectionState.DISCONNECTED, session.connectionState.value)
        assertEquals(null, manager.getSession("session-1"))
    }

    @Test
    fun disconnectAllRemovesAllSessionsFromPool() = runTest {
        val session1 = FakeSession(
            initialConnectionState = ConnectionState.CONNECTED,
            initialSessionInfo = SessionInfo(
                sessionId = "session-1",
                name = "Session 1",
                resumed = true,
                messageCount = 3,
            ),
        )
        val session2 = FakeSession(
            initialConnectionState = ConnectionState.CONNECTED,
            initialSessionInfo = SessionInfo(
                sessionId = "session-2",
                name = "Session 2",
                resumed = true,
                messageCount = 1,
            ),
        )
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = { FakeSession() },
            appContextProvider = { null },
            processLifecycleProvider = { CountingLifecycle() },
        )

        manager.addSessionToPoolForTest("session-1", session1)
        manager.addSessionToPoolForTest("session-2", session2)
        manager.disconnectAll()

        assertEquals(ConnectionState.DISCONNECTED, session1.connectionState.value)
        assertEquals(ConnectionState.DISCONNECTED, session2.connectionState.value)
        assertEquals(emptySet<String>(), manager.poolSessionIds())
    }

    @Test
    fun observeLifecycleDoesNotRegisterDuplicateObserversForSameLifecycle() {
        val lifecycle = CountingLifecycle()
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = { FakeSession() },
            appContextProvider = { null },
            processLifecycleProvider = { lifecycle },
        )

        manager.observeLifecycle(lifecycle)
        manager.observeLifecycle(lifecycle)

        assertEquals(1, lifecycle.addedObservers.size)
    }

    @Test
    fun bindToProcessLifecycleRegistersObserverOnlyOnce() {
        val processLifecycle = CountingLifecycle()
        val manager = SessionManager(
            config = ClawSeedConfig("http://localhost:3000"),
            sessionFactory = { FakeSession() },
            appContextProvider = { Application() },
            processLifecycleProvider = { processLifecycle },
        )

        manager.bindToProcessLifecycle(disconnectOnBackground = true)
        manager.bindToProcessLifecycle(disconnectOnBackground = true)

        assertEquals(1, processLifecycle.addedObservers.size)
    }

    private class CountingLifecycle : Lifecycle() {
        val addedObservers = mutableListOf<LifecycleObserver>()
        private val observers = linkedSetOf<LifecycleObserver>()

        override fun addObserver(observer: LifecycleObserver) {
            addedObservers += observer
            observers += observer
        }

        override fun removeObserver(observer: LifecycleObserver) {
            observers -= observer
        }

        override val currentState: State
            get() = State.CREATED
    }

    private class FakeSession(
        initialConnectionState: ConnectionState = ConnectionState.CONNECTED,
        initialSessionInfo: SessionInfo? = null,
    ) : ClawSeedSession {
        private val mutableConnectionState = MutableStateFlow(initialConnectionState)
        private val mutableSessionInfo = MutableStateFlow(initialSessionInfo)
        private val mutableEvents = MutableSharedFlow<ChatEvent>(extraBufferCapacity = 16)

        var connectCalls: Int = 0
            private set
        var lastConnectSessionId: String? = "__unset__"
            private set

        override val connectionState: StateFlow<ConnectionState> = mutableConnectionState
        override val sessionInfo: StateFlow<SessionInfo?> = mutableSessionInfo
        override val events: SharedFlow<ChatEvent> = mutableEvents
        override val tools: ToolRegistry = ToolRegistry()
        override val gateway: GatewayClient = GatewayClient("http://localhost")

        override suspend fun connect(sessionId: String?) {
            connectCalls += 1
            lastConnectSessionId = sessionId
            mutableConnectionState.value = ConnectionState.CONNECTED
        }

        override suspend fun disconnect() {
            mutableConnectionState.value = ConnectionState.DISCONNECTED
        }

        override fun sendMessage(content: String, debug: Boolean) = Unit

        override fun regenerate(debug: Boolean) = Unit

        override suspend fun abort() = Unit
    }
}
