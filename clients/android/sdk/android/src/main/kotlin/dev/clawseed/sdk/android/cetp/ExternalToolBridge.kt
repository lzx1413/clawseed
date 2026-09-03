package dev.clawseed.sdk.android.cetp

import android.content.Context
import android.content.pm.PackageManager
import android.os.ParcelFileDescriptor
import android.util.Log
import androidx.core.content.ContextCompat
import dev.clawseed.sdk.core.tool.ClawSeedTool
import dev.clawseed.sdk.core.tool.ToolRegistry
import dev.clawseed.sdk.core.tool.ToolResult
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

class ExternalToolBridge(private val context: Context) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private val cetpClient = CetpClient(context)
    private val parser = CetpProtocolParser()
    private val json = Json { ignoreUnknownKeys = true }
    private val scanMutex = Mutex()

    private val _providers = MutableStateFlow<List<DiscoveredProvider>>(emptyList())
    val providers: StateFlow<List<DiscoveredProvider>> = _providers.asStateFlow()

    private val _authEvents = MutableSharedFlow<AuthRequiredEvent>(extraBufferCapacity = 8)
    val authEvents: SharedFlow<AuthRequiredEvent> = _authEvents.asSharedFlow()

    private var currentRegistry: ToolRegistry? = null
    private var packageChangeReceiver: PackageChangeReceiver? = null
    private val registeredToolNames = mutableSetOf<String>()
    private val pendingResumes = ConcurrentHashMap<String, PendingResume>()
    private val resumeSignals = ConcurrentHashMap<String, CompletableDeferred<Boolean>>()
    private val activeOperations = ConcurrentHashMap<String, ActiveOperation>()

    fun attachToRegistry(registry: ToolRegistry) {
        if (currentRegistry == registry) return
        detachFromRegistry()
        currentRegistry = registry
        scope.launch { rescan() }
    }

    fun detachFromRegistry() {
        val registry = currentRegistry ?: return
        registeredToolNames.forEach(registry::unregister)
        registeredToolNames.clear()
        currentRegistry = null
    }

    suspend fun scan() = scanMutex.withLock { scanLocked() }

    private suspend fun scanLocked() {
        val discovered = mutableListOf<DiscoveredProvider>()
        val intent = android.content.Intent(CetpConstants.ACTION_TOOL_PROVIDER)
        val resolveInfos = context.packageManager.queryIntentServices(intent, PackageManager.GET_META_DATA)
        val usedV1Labels = mutableSetOf<String>()

        Log.d(TAG, "scan: found ${resolveInfos.size} provider service(s)")

        for (info in resolveInfos) {
            val serviceInfo = info.serviceInfo ?: continue
            val metadata = serviceInfo.metaData ?: continue
            val authority = metadata.getString(CetpConstants.META_AUTHORITY) ?: continue
            val packageName = serviceInfo.packageName
            if (!authorityBelongsToPackage(authority, packageName)) {
                Log.w(TAG, "Ignoring CETP authority $authority not owned by $packageName")
                continue
            }

            val legacyVersion = metadata.getInt(CetpConstants.META_VERSION, CetpConstants.PROTOCOL_VERSION_V1)
            val versions = parser.advertisedVersions(
                legacyVersion = legacyVersion,
                versionsValue = metadata.getString(CetpConstants.META_VERSIONS),
            )
            val provider = when {
                CetpConstants.PROTOCOL_VERSION_V2 in versions -> discoverV2Provider(
                    packageName = packageName,
                    authority = authority,
                    supportedVersions = versions,
                )

                CetpConstants.PROTOCOL_VERSION_V1 in versions -> {
                    val label = deriveV1ProviderLabel(packageName, usedV1Labels)
                    usedV1Labels += label
                    discoverV1Provider(packageName, authority, versions, label)
                }

                else -> null
            }
            if (provider != null) discovered += provider
        }

        _providers.value = discovered
        Log.d(TAG, "scan: ${discovered.size} provider(s), ${discovered.sumOf { it.tools.size }} tool(s)")
        currentRegistry?.let(::registerAllTools)
    }

    suspend fun rescan() = scanMutex.withLock {
        val registry = currentRegistry
        if (registry != null) {
            registeredToolNames.forEach(registry::unregister)
            registeredToolNames.clear()
        }
        scanLocked()
    }

    fun startWatching() {
        if (packageChangeReceiver != null) return
        val receiver = PackageChangeReceiver { _, packageName ->
            scope.launch {
                if (isPotentialProvider(packageName)) rescan()
            }
        }
        ContextCompat.registerReceiver(
            context,
            receiver,
            PackageChangeReceiver.createFilter(),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        packageChangeReceiver = receiver
    }

    fun stopWatching() {
        val receiver = packageChangeReceiver ?: return
        runCatching { context.unregisterReceiver(receiver) }
        packageChangeReceiver = null
    }

    fun cancelActiveOperations() {
        activeOperations.values.toList().forEach { operation ->
            scope.launch { cancelOperation(operation) }
        }
    }

    fun resumePendingAction(requestId: String) {
        resumeSignals[requestId]?.complete(true)
    }

    fun cancelPendingAction(requestId: String) {
        resumeSignals[requestId]?.complete(false)
    }

    private suspend fun discoverV2Provider(
        packageName: String,
        authority: String,
        supportedVersions: Set<Int>,
    ): DiscoveredProvider? {
        val negotiation = callWithProviderRecovery(packageName, authority) {
            cetpClient.negotiate(authority)
        }
        val session = (negotiation as? CetpResult.Success)?.data ?: run {
            Log.w(TAG, "CETP v2 negotiation failed for $authority: $negotiation")
            return null
        }
        if (session.protocolVersion != CetpConstants.PROTOCOL_VERSION_V2 ||
            !providerIdBelongsToPackage(session.providerId, packageName)
        ) {
            Log.w(TAG, "CETP v2 provider identity mismatch for $authority")
            return null
        }

        val namespace = stableNamespace(session.providerId)
        val toolsResult = callWithProviderRecovery(packageName, authority) {
            cetpClient.listTools(authority, session)
        }
        val tools = (toolsResult as? CetpResult.Success)?.let {
            parser.tools(it.data, namespace, CetpConstants.PROTOCOL_VERSION_V2)
        } ?: emptyList()
        if (tools.isEmpty()) return null

        val providerInfo = (cetpClient.getProviderInfo(authority, session) as? CetpResult.Success)?.let {
            parser.providerInfo(it.data, session.providerId, strictSecurityEnums = true)
        }
        if (providerInfo != null && providerInfo.providerId != session.providerId) {
            Log.w(TAG, "CETP v2 provider_info identity mismatch for $authority")
            return null
        }
        return DiscoveredProvider(
            packageName = packageName,
            authority = authority,
            version = session.protocolVersion,
            providerLabel = namespace,
            tools = tools,
            providerInfo = providerInfo,
            supportedVersions = supportedVersions,
            session = session,
        )
    }

    private suspend fun discoverV1Provider(
        packageName: String,
        authority: String,
        supportedVersions: Set<Int>,
        label: String,
    ): DiscoveredProvider? {
        val toolsResult = callWithProviderRecovery(packageName, authority) {
            cetpClient.listTools(authority)
        }
        val tools = (toolsResult as? CetpResult.Success)?.let {
            parser.tools(it.data, label, CetpConstants.PROTOCOL_VERSION_V1)
        } ?: emptyList()
        if (tools.isEmpty()) return null
        val providerInfo = (cetpClient.getProviderInfo(authority) as? CetpResult.Success)?.let {
            parser.providerInfo(it.data, packageName)
        }
        return DiscoveredProvider(
            packageName = packageName,
            authority = authority,
            version = CetpConstants.PROTOCOL_VERSION_V1,
            providerLabel = label,
            tools = tools,
            providerInfo = providerInfo,
            supportedVersions = supportedVersions,
        )
    }

    private suspend fun <T> callWithProviderRecovery(
        packageName: String,
        authority: String,
        call: () -> CetpResult<T>,
    ): CetpResult<T> {
        val first = call()
        if (!first.isProviderUnavailable()) return first
        if (!prepareProviderSilently(authority)) wakeProviderForDiscovery(packageName)
        return call()
    }

    private fun registerAllTools(registry: ToolRegistry) {
        for (provider in _providers.value) {
            for (tool in provider.tools) {
                val schema = runCatching { json.parseToJsonElement(tool.parametersJson).jsonObject }.getOrNull()
                    ?: continue
                registry.register(
                    CetpProxyTool(
                        name = tool.namespacedName,
                        description = tool.description,
                        parametersSchema = schema,
                        authority = provider.authority,
                        localToolName = tool.name,
                        providerPackageName = provider.packageName,
                        providerLabel = provider.providerLabel,
                        initialSession = provider.session,
                        annotations = tool.annotations,
                    ),
                )
                registeredToolNames += tool.namespacedName
            }
        }
    }

    private fun authorityBelongsToPackage(authority: String, packageName: String): Boolean {
        val providerInfo = context.packageManager.resolveContentProvider(authority, PackageManager.GET_META_DATA)
            ?: return false
        return providerInfo.packageName == packageName
    }

    private fun providerIdBelongsToPackage(providerId: String, packageName: String): Boolean =
        providerId == packageName || providerId.startsWith("$packageName.")

    private fun deriveV1ProviderLabel(packageName: String, usedLabels: Set<String>): String {
        val base = packageName.substringAfterLast('.')
            .replace(Regex("[^a-zA-Z0-9]"), "_")
            .lowercase()
            .ifBlank { "provider" }
        if (base !in usedLabels) return base
        var suffix = 2
        while ("${base}_$suffix" in usedLabels) suffix++
        return "${base}_$suffix"
    }

    private fun stableNamespace(providerId: String): String {
        val leaf = providerId.substringAfterLast('.')
            .lowercase()
            .replace(Regex("[^a-z0-9_]"), "_")
            .take(MAX_NAMESPACE_LEAF_LENGTH)
            .ifBlank { "provider" }
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(providerId.toByteArray(Charsets.UTF_8))
            .take(NAMESPACE_DIGEST_BYTES)
            .joinToString("") { "%02x".format(it) }
        return "${leaf}_$digest"
    }

    private fun isPotentialProvider(packageName: String): Boolean {
        if (_providers.value.any { it.packageName == packageName }) return true
        val intent = android.content.Intent(CetpConstants.ACTION_TOOL_PROVIDER).apply { setPackage(packageName) }
        return context.packageManager.queryIntentServices(intent, 0).isNotEmpty()
    }

    private suspend fun prepareProviderSilently(authority: String): Boolean = try {
        val client = context.contentResolver.acquireContentProviderClient(authority) ?: return false
        client.close()
        delay(PROVIDER_PREPARE_DELAY_MS)
        true
    } catch (e: Exception) {
        Log.w(TAG, "Failed to prepare CETP provider $authority: ${e.message}")
        false
    }

    private suspend fun wakeProviderForDiscovery(packageName: String): Boolean = try {
        val launchIntent = context.packageManager.getLaunchIntentForPackage(packageName) ?: return false
        launchIntent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(launchIntent)
        delay(PROVIDER_PREPARE_DELAY_MS)
        true
    } catch (e: Exception) {
        Log.w(TAG, "Failed to wake CETP provider $packageName: ${e.message}")
        false
    }

    private fun CetpResult<*>.isProviderUnavailable(): Boolean =
        this is CetpResult.Error && errorCode == CetpConstants.ERROR_UNAVAILABLE

    private fun pendingKey(authority: String, toolName: String, argsJson: String): String {
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(argsJson.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
        return "$authority\u0000$toolName\u0000$digest"
    }

    private suspend fun cancelOperation(operation: ActiveOperation) {
        val result = cetpClient.cancelOperation(
            authority = operation.authority,
            session = operation.session,
            operationId = operation.operationId,
            requestId = operation.requestId,
        )
        if (result is CetpResult.Error) {
            Log.w(TAG, "Failed to cancel CETP operation ${operation.operationId}: ${result.errorCode}")
        }
    }

    private inner class CetpProxyTool(
        override val name: String,
        override val description: String,
        override val parametersSchema: JsonObject,
        private val authority: String,
        private val localToolName: String,
        private val providerPackageName: String,
        private val providerLabel: String,
        initialSession: CetpSession?,
        private val annotations: ToolAnnotations,
    ) : ClawSeedTool {
        @Volatile
        private var session: CetpSession? = initialSession

        override suspend fun execute(args: JsonObject): ToolResult {
            val argsJson = args.toString()
            val key = pendingKey(authority, localToolName, argsJson)
            val cachedPending = pendingResumes[key]
            val pending = if (cachedPending != null && cachedPending.isExpired()) {
                pendingResumes.remove(key, cachedPending)
                null
            } else {
                cachedPending
            }
            val requestId = pending?.requestId ?: CetpClient.newRequestId()
            val deadline = CetpClient.defaultDeadline(
                if (annotations.execution == ExecutionMode.SYNC_OR_ASYNC ||
                    annotations.hasSideEffects || annotations.confirmation != ConfirmationPolicy.NEVER
                ) {
                    CetpConstants.DEFAULT_OPERATION_TIMEOUT_MS
                } else {
                    CetpConstants.DEFAULT_REQUEST_TIMEOUT_MS
                },
            )

            var activeSession = session
            var result = cetpClient.executeTool(
                authority = authority,
                toolName = localToolName,
                argsJson = argsJson,
                requestId = requestId,
                session = activeSession,
                deadlineAtMillis = deadline,
                resumeToken = pending?.resumeToken,
            )
            if (activeSession != null && result is CetpResult.Error &&
                result.errorCode == CetpConstants.ERROR_NEGOTIATION_REQUIRED
            ) {
                val renegotiated = cetpClient.negotiate(authority)
                val replacement = (renegotiated as? CetpResult.Success)?.data
                if (replacement != null && replacement.providerId == activeSession.providerId) {
                    session = replacement
                    activeSession = replacement
                    result = cetpClient.executeTool(
                        authority = authority,
                        toolName = localToolName,
                        argsJson = argsJson,
                        requestId = requestId,
                        session = replacement,
                        deadlineAtMillis = deadline,
                        resumeToken = pending?.resumeToken,
                    )
                }
            }

            return if (activeSession == null) {
                handleV1Result(result)
            } else {
                handleV2Result(result, activeSession, requestId, key, argsJson, deadline)
            }
        }

        private fun handleV1Result(result: CetpResult<String>): ToolResult = when (result) {
            is CetpResult.Success -> ToolResult.Success(result.data)
            is CetpResult.Accepted -> ToolResult.Failure("Unexpected asynchronous response from CETP v1 provider")
            is CetpResult.Error -> {
                emitActionRequired(result, result.requestId)
                ToolResult.Failure(result.displayMessage())
            }
        }

        private suspend fun handleV2Result(
            initial: CetpResult<String>,
            activeSession: CetpSession,
            requestId: String,
            pendingKey: String,
            argsJson: String,
            deadlineAtMillis: Long,
        ): ToolResult {
            return when (initial) {
                is CetpResult.Success -> {
                    pendingResumes.remove(pendingKey)
                    decodeV2Result(initial.data, initial.fileDescriptor, activeSession)
                }

                is CetpResult.Error -> {
                    if (initial.isActionRequired()) {
                        val resumeToken = initial.resumeToken
                        if (resumeToken != null && initial.resolution?.creatorPackage == providerPackageName) {
                            pendingResumes[pendingKey] = PendingResume(
                                requestId,
                                resumeToken,
                                System.currentTimeMillis(),
                            )
                            emitActionRequired(initial, requestId)
                            return awaitProviderAction(
                                activeSession = activeSession,
                                requestId = requestId,
                                pendingKey = pendingKey,
                                argsJson = argsJson,
                                resumeToken = resumeToken,
                                deadlineAtMillis = deadlineAtMillis,
                            )
                        } else {
                            return ToolResult.Failure("Invalid CETP v2 action resolution")
                        }
                    } else if (initial.errorCode == CetpConstants.ERROR_USER_CANCELLED) {
                        pendingResumes.remove(pendingKey)
                    }
                    ToolResult.Failure(initial.displayMessage())
                }

                is CetpResult.Accepted -> {
                    val operationId = parser.operationId(initial.data)
                        ?: return ToolResult.Failure("Invalid CETP accepted response")
                    awaitOperation(activeSession, operationId, requestId, pendingKey, deadlineAtMillis)
                }
            }
        }

        private suspend fun awaitProviderAction(
            activeSession: CetpSession,
            requestId: String,
            pendingKey: String,
            argsJson: String,
            resumeToken: String,
            deadlineAtMillis: Long,
        ): ToolResult {
            val signal = CompletableDeferred<Boolean>()
            resumeSignals.put(requestId, signal)?.complete(false)
            return try {
                val remainingMillis = (deadlineAtMillis - System.currentTimeMillis()).coerceAtLeast(1L)
                val shouldResume = withTimeoutOrNull(remainingMillis) { signal.await() }
                if (shouldResume != true) {
                    pendingResumes.remove(pendingKey)
                    return ToolResult.Failure(
                        if (shouldResume == false) {
                            "[USER_CANCELLED] Provider action cancelled"
                        } else {
                            "[DEADLINE_EXCEEDED] Provider action was not completed before deadline"
                        },
                    )
                }
                val result = cetpClient.executeTool(
                    authority = authority,
                    toolName = localToolName,
                    argsJson = argsJson,
                    requestId = requestId,
                    session = activeSession,
                    deadlineAtMillis = deadlineAtMillis,
                    resumeToken = resumeToken,
                )
                handleV2Result(
                    initial = result,
                    activeSession = activeSession,
                    requestId = requestId,
                    pendingKey = pendingKey,
                    argsJson = argsJson,
                    deadlineAtMillis = deadlineAtMillis,
                )
            } finally {
                resumeSignals.remove(requestId, signal)
            }
        }

        private suspend fun awaitOperation(
            activeSession: CetpSession,
            operationId: String,
            requestId: String,
            pendingKey: String,
            deadlineAtMillis: Long,
        ): ToolResult {
            val active = ActiveOperation(authority, activeSession, operationId, requestId)
            activeOperations[requestId] = active
            var pollDelay = CetpConstants.DEFAULT_OPERATION_POLL_MS
            try {
                while (System.currentTimeMillis() < deadlineAtMillis) {
                    delay(pollDelay.coerceIn(CetpConstants.MIN_OPERATION_POLL_MS, CetpConstants.MAX_OPERATION_POLL_MS))
                    when (val result = cetpClient.getOperation(
                        authority,
                        activeSession,
                        operationId,
                        requestId,
                        deadlineAtMillis,
                    )) {
                        is CetpResult.Success -> {
                            val operation = result.data
                            pollDelay = operation.pollAfterMillis
                            when (operation.state) {
                                OperationState.QUEUED,
                                OperationState.RUNNING,
                                OperationState.CANCELLATION_REQUESTED,
                                -> result.fileDescriptor?.close()

                                OperationState.SUCCEEDED -> {
                                    pendingResumes.remove(pendingKey)
                                    return decodeV2Result(
                                        operation.resultJson ?: return ToolResult.Failure("Missing CETP result"),
                                        result.fileDescriptor,
                                        activeSession,
                                    )
                                }

                                OperationState.FAILED -> {
                                    result.fileDescriptor?.close()
                                    val error = operation.error
                                    return ToolResult.Failure(
                                        if (error == null) "CETP operation failed" else "[${error.code}] ${error.message}",
                                    )
                                }

                                OperationState.CANCELLED -> {
                                    result.fileDescriptor?.close()
                                    return ToolResult.Failure("[CANCELLED] CETP operation cancelled")
                                }
                            }
                        }

                        is CetpResult.Error -> {
                            if (!result.retryable) return ToolResult.Failure(result.displayMessage())
                            pollDelay = result.retryAfterMillis ?: pollDelay
                        }

                        is CetpResult.Accepted -> return ToolResult.Failure("Invalid CETP operation response")
                    }
                }
                withContext(NonCancellable) { cancelOperation(active) }
                return ToolResult.Failure("[DEADLINE_EXCEEDED] CETP operation did not complete before deadline")
            } catch (cancelled: CancellationException) {
                withContext(NonCancellable) { cancelOperation(active) }
                throw cancelled
            } finally {
                activeOperations.remove(requestId, active)
            }
        }

        private fun decodeV2Result(
            dataJson: String,
            fileDescriptor: ParcelFileDescriptor?,
            activeSession: CetpSession,
        ): ToolResult {
            val root = runCatching { json.parseToJsonElement(dataJson).jsonObject }.getOrNull()
                ?: return ToolResult.Failure("Invalid CETP result envelope")
            return when (root["kind"]?.jsonPrimitive?.contentOrNull) {
                CetpConstants.RESULT_KIND_INLINE -> {
                    fileDescriptor?.close()
                    val value = root["value"] ?: return ToolResult.Failure("Missing CETP inline result")
                    val encoded = value.toString()
                    if (encoded.toByteArray(Charsets.UTF_8).size > activeSession.limits.maxInlineResultBytes) {
                        ToolResult.Failure("CETP inline result exceeds negotiated limit")
                    } else {
                        ToolResult.Success(encoded)
                    }
                }

                CetpConstants.RESULT_KIND_FILE -> decodeFileResult(root, fileDescriptor, activeSession)
                else -> {
                    fileDescriptor?.close()
                    ToolResult.Failure("Unknown CETP result kind")
                }
            }
        }

        private fun decodeFileResult(
            envelope: JsonObject,
            fileDescriptor: ParcelFileDescriptor?,
            activeSession: CetpSession,
        ): ToolResult {
            if (CetpConstants.CAPABILITY_LARGE_RESULTS !in activeSession.capabilities) {
                fileDescriptor?.close()
                return ToolResult.Failure("CETP large result capability was not negotiated")
            }
            val descriptor = fileDescriptor ?: return ToolResult.Failure("Missing CETP result file descriptor")
            val mediaType = envelope["media_type"]?.jsonPrimitive?.contentOrNull
            if (mediaType != JSON_MEDIA_TYPE) {
                descriptor.close()
                return ToolResult.Failure("Unsupported CETP result media type")
            }
            val declaredSize = envelope["size_bytes"]?.jsonPrimitive?.longOrNull
                ?: run {
                    descriptor.close()
                    return ToolResult.Failure("Missing CETP result size")
                }
            val expectedDigest = envelope["sha256"]?.jsonPrimitive?.contentOrNull
                ?.takeIf { SHA256_HEX.matches(it) }
                ?: run {
                    descriptor.close()
                    return ToolResult.Failure("Invalid CETP result digest")
                }
            if (declaredSize < 0 || declaredSize > CetpConstants.DEFAULT_MAX_FILE_RESULT_BYTES) {
                descriptor.close()
                return ToolResult.Failure("CETP file result exceeds local limit")
            }

            return runCatching {
                val digest = MessageDigest.getInstance("SHA-256")
                val output = java.io.ByteArrayOutputStream(declaredSize.toInt())
                ParcelFileDescriptor.AutoCloseInputStream(descriptor).use { input ->
                    val buffer = ByteArray(FILE_BUFFER_BYTES)
                    var total = 0L
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        total += read
                        if (total > declaredSize || total > CetpConstants.DEFAULT_MAX_FILE_RESULT_BYTES) {
                            error("CETP file result exceeded declared size")
                        }
                        digest.update(buffer, 0, read)
                        output.write(buffer, 0, read)
                    }
                    check(total == declaredSize) { "CETP file result size mismatch" }
                }
                val actualDigest = digest.digest().joinToString("") { "%02x".format(it) }
                check(MessageDigest.isEqual(
                    actualDigest.toByteArray(Charsets.US_ASCII),
                    expectedDigest.toByteArray(Charsets.US_ASCII),
                )) { "CETP file result digest mismatch" }
                val content = output.toString(Charsets.UTF_8.name())
                json.parseToJsonElement(content)
                ToolResult.Success(content)
            }.getOrElse { ToolResult.Failure(it.message ?: "Failed to read CETP file result") }
        }

        private fun emitActionRequired(error: CetpResult.Error, requestId: String?) {
            if (!error.isActionRequired()) return
            scope.launch {
                _authEvents.emit(
                    AuthRequiredEvent(
                        providerPackageName = providerPackageName,
                        providerLabel = providerLabel,
                        toolName = name,
                        resolutionHint = error.resolutionHint,
                        authorizeIntent = error.authorizeIntent,
                        resolution = error.resolution,
                        resumeToken = error.resumeToken,
                        requestId = requestId,
                        errorCode = error.errorCode,
                    ),
                )
            }
        }
    }

    private data class PendingResume(
        val requestId: String,
        val resumeToken: String,
        val createdAtMillis: Long,
    ) {
        fun isExpired(): Boolean = System.currentTimeMillis() - createdAtMillis > PENDING_RESUME_TTL_MS
    }

    private data class ActiveOperation(
        val authority: String,
        val session: CetpSession,
        val operationId: String,
        val requestId: String,
    )

    private fun CetpResult.Error.isActionRequired(): Boolean =
        errorCode == CetpConstants.ERROR_AUTH_REQUIRED || errorCode == CetpConstants.ERROR_USER_ACTION_REQUIRED

    private fun CetpResult.Error.displayMessage(): String = buildString {
        append("[$errorCode] $errorMessage")
        resolutionHint?.let { append(" - $it") }
    }

    companion object {
        private const val TAG = "ExternalToolBridge"
        private const val PROVIDER_PREPARE_DELAY_MS = 500L
        private const val MAX_NAMESPACE_LEAF_LENGTH = 24
        private const val NAMESPACE_DIGEST_BYTES = 4
        private const val FILE_BUFFER_BYTES = 8 * 1024
        private const val PENDING_RESUME_TTL_MS = 10 * 60_000L
        private const val JSON_MEDIA_TYPE = "application/json"
        private val SHA256_HEX = Regex("[0-9a-f]{64}")
    }
}
