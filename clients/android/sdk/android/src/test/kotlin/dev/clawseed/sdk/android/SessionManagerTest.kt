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

@OptIn(ExperimentalCoroutinesApi::class)
class SessionManagerTest {

    @Test
    fun connectReusesDisconnectedSessionWhenSessionIdIsKnown() = runTest {
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
            sessionFactory = { error("sessionFactory should not be called when reusing an existing session") },
            appContextProvider = { null },
            processLifecycleProvider = { CountingLifecycle() },
        )

        manager.setActiveSessionForTest(session)

        val connectedSession = manager.connect()

        assertSame(session, connectedSession)
        assertEquals(1, session.connectCalls)
        assertEquals(null, session.lastConnectSessionId)
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

    private fun SessionManager.setActiveSessionForTest(session: ClawSeedSession) {
        val field = SessionManager::class.java.getDeclaredField("_activeSession")
        field.isAccessible = true
        @Suppress("UNCHECKED_CAST")
        val stateFlow = field.get(this) as MutableStateFlow<ClawSeedSession?>
        stateFlow.value = session
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

        override suspend fun abort() = Unit
    }
}