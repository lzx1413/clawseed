package dev.clawseed.sdk.android

import android.content.Context
import dev.clawseed.sdk.core.ClawSeedConfig
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.EmbeddingDownloadProgress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

import dev.clawseed.sdk.android.cetp.ExternalToolBridge

/**
 * Android-specific SDK entry point that stores application-scoped configuration
 * and exposes singleton helpers.
 */
object ClawSeedAndroid {
    private var _context: Context? = null
    private var _config: ClawSeedConfig? = null
    private var _sessionManager: SessionManager? = null
    private var _gatewayClient: GatewayClient? = null
    private var _externalToolBridge: ExternalToolBridge? = null
    private var _downloadProgressFlow: StateFlow<EmbeddingDownloadProgress?> = MutableStateFlow(null).asStateFlow()
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
            _externalToolBridge = ExternalToolBridge(context.applicationContext).also {
                it.startWatching()
            }
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

    /** Returns the singleton [ExternalToolBridge] for discovering and bridging CETP tools. */
    fun externalToolBridge(): ExternalToolBridge {
        return _externalToolBridge ?: error("ClawSeedAndroid not initialized. Call init() first.")
    }

    /** Registers the download progress flow from the embedded gateway. Called by the hosting service. */
    fun setDownloadProgress(flow: StateFlow<EmbeddingDownloadProgress?>) {
        _downloadProgressFlow = flow
    }

    /** Returns the current embedding download progress, or null if no download is active. */
    fun downloadProgress(): StateFlow<EmbeddingDownloadProgress?> = _downloadProgressFlow

    private var _gatewayRestarter: (suspend () -> Unit)? = null

    /** Registers a callback that restarts the embedded gateway process. Called by the hosting service. */
    fun setGatewayRestarter(restarter: suspend () -> Unit) {
        _gatewayRestarter = restarter
    }

    /** Restarts the embedded gateway. Throws if no restarter has been registered. */
    suspend fun restartGateway() {
        _gatewayRestarter?.invoke() ?: error("Gateway restarter not registered")
    }

    internal val context: Context get() = _context ?: error("ClawSeedAndroid not initialized.")

    /** Suspends until [init] has completed. */
    suspend fun awaitInit() {
        while (!_initialized) {
            kotlinx.coroutines.delay(200)
        }
    }
}