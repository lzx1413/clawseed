package dev.clawseed.sdk.android

import android.content.Context
import dev.clawseed.sdk.core.ClawSeedConfig
import dev.clawseed.sdk.core.client.GatewayClient

/**
 * Android-specific SDK entry point that stores application-scoped configuration
 * and exposes singleton helpers.
 */
object ClawSeedAndroid {
    private var _context: Context? = null
    private var _config: ClawSeedConfig? = null
    private var _sessionManager: SessionManager? = null
    private var _gatewayClient: GatewayClient? = null
    @Volatile
    private var _initialized = false
    private val initLock = Object()

    /** Returns whether [init] has completed successfully. */
    val isInitialized: Boolean get() = _initialized

    /** Initializes the Android SDK singleton state. Call once from `Application.onCreate()`. */
    fun init(context: Context, config: ClawSeedConfig) {
        synchronized(initLock) {
            _context = context.applicationContext
            _config = config
            _gatewayClient = GatewayClient(
                baseUrl = config.gatewayUrl,
                authTokenProvider = config.authTokenProvider,
            )
            _initialized = true
            initLock.notifyAll()
        }
    }

    /** Returns the singleton [SessionManager] for the current process. */
    fun sessionManager(): SessionManager {
        val config = _config ?: error("ClawSeedAndroid not initialized. Call init() first.")
        if (_sessionManager == null) {
            _sessionManager = SessionManager(config)
        }
        return _sessionManager!!
    }

    /** Returns a [GatewayClient] configured from the latest [init] call. */
    fun gatewayClient(): GatewayClient {
        return _gatewayClient ?: error("ClawSeedAndroid not initialized. Call init() first.")
    }

    internal val context: Context get() = _context ?: error("ClawSeedAndroid not initialized.")

    /** Suspends until [init] has completed. */
    suspend fun awaitInit() {
        while (!_initialized) {
            kotlinx.coroutines.delay(200)
        }
    }
}
