package dev.clawseed.sdk.android.cetp

import android.app.PendingIntent
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.database.Cursor
import android.net.Uri
import android.os.Binder
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.util.Log
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

data class CetpProviderDescriptor(
    val providerId: String,
    val providerName: String,
    val description: String,
    val scopes: List<ProviderScope>,
    val schemaDialect: String = CetpConstants.DEFAULT_SCHEMA_DIALECT,
    val capabilities: Set<String> = emptySet(),
    val limits: CetpLimits = CetpLimits(),
)

data class CetpProviderTool(
    val name: String,
    val title: String,
    val description: String,
    val inputSchema: JsonObject,
    val outputSchema: JsonObject,
    val scopes: List<String> = emptyList(),
    val annotations: ToolAnnotations = ToolAnnotations(),
)

data class CetpCallerIdentity(
    val uid: Int,
    val packages: List<CetpCallerPackage>,
) {
    val stableKey: String = buildString {
        append(uid)
        packages.sortedBy { it.packageName }.forEach { callerPackage ->
            append('|').append(callerPackage.packageName)
            callerPackage.signingCertificateSha256.sorted().forEach { append(':').append(it) }
        }
    }

    companion object {
        @Suppress("DEPRECATION")
        fun resolve(context: Context, uid: Int): CetpCallerIdentity {
            val packageManager = context.packageManager
            val packages = packageManager.getPackagesForUid(uid).orEmpty().map { packageName ->
                val packageInfo = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNING_CERTIFICATES)
                } else {
                    packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNATURES)
                }
                val signatures = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    val signingInfo = packageInfo.signingInfo
                    if (signingInfo?.hasMultipleSigners() == true) {
                        signingInfo.apkContentsSigners.orEmpty().asList()
                    } else {
                        signingInfo?.signingCertificateHistory.orEmpty().asList()
                    }
                } else {
                    packageInfo.signatures.orEmpty().asList()
                }
                CetpCallerPackage(
                    packageName = packageName,
                    signingCertificateSha256 = signatures.map { signature ->
                        MessageDigest.getInstance("SHA-256")
                            .digest(signature.toByteArray())
                            .joinToString("") { "%02x".format(it) }
                    }.toSet(),
                )
            }
            return CetpCallerIdentity(uid, packages)
        }
    }
}

data class CetpCallerPackage(
    val packageName: String,
    val signingCertificateSha256: Set<String>,
)

data class CetpProviderRequest(
    val caller: CetpCallerIdentity,
    val requestId: String,
    val deadlineAtMillis: Long,
    val tool: CetpProviderTool,
    val args: JsonObject,
    val rawArgsJson: String,
    val resumeToken: String?,
)

sealed class CetpAccessDecision {
    data class Granted(val providerConfirmed: Boolean = false) : CetpAccessDecision()

    data class ActionRequired(
        val resolution: PendingIntent,
        val resumeToken: String,
        val message: String,
        val hint: String? = null,
        val errorCode: String = CetpConstants.ERROR_USER_ACTION_REQUIRED,
    ) : CetpAccessDecision()

    data class Denied(
        val message: String = "Caller is not authorized",
        val errorCode: String = CetpConstants.ERROR_PERMISSION_DENIED,
    ) : CetpAccessDecision()
}

sealed class CetpProviderResult {
    data class Inline(val value: JsonElement) : CetpProviderResult()

    data class File(
        val descriptor: ParcelFileDescriptor,
        val mediaType: String,
        val sizeBytes: Long,
        val sha256: String,
    ) : CetpProviderResult()

    data class Accepted(
        val operationId: String,
        val pollAfterMillis: Long = CetpConstants.DEFAULT_OPERATION_POLL_MS,
    ) : CetpProviderResult()

    data class Failure(
        val errorCode: String,
        val message: String,
        val retryable: Boolean = false,
        val retryAfterMillis: Long? = null,
        val details: JsonObject? = null,
    ) : CetpProviderResult()
}

data class CetpProviderOperation(
    val operationId: String,
    val state: OperationState,
    val pollAfterMillis: Long = CetpConstants.DEFAULT_OPERATION_POLL_MS,
    val progressCurrent: Long? = null,
    val progressTotal: Long? = null,
    val progressMessage: String? = null,
    val result: CetpProviderResult? = null,
    val error: CetpProviderResult.Failure? = null,
)

data class CetpMutationContext(
    val callerKey: String,
    val providerId: String,
    val requestId: String,
    val fingerprintSha256: String,
    val retainUntilMillis: Long,
)

/**
 * Executes a mutation at most once and returns a stored response for duplicate requests.
 * Implementations must persist the reservation and outcome atomically with the business mutation.
 */
fun interface CetpMutationExecutor {
    fun execute(context: CetpMutationContext, mutation: () -> CetpProviderResult): CetpProviderResult
}

/**
 * Base ContentProvider implementing the CETP v2 wire contract.
 *
 * Authorization, Provider confirmation UI, business execution, operation persistence, and durable
 * mutation deduplication remain explicit hooks owned by the Provider application.
 */
abstract class CetpV2ContentProvider : ContentProvider() {
    protected abstract val provider: CetpProviderDescriptor
    protected abstract val providerTools: List<CetpProviderTool>

    protected open val mutationExecutor: CetpMutationExecutor? = null
    protected open val sessionTtlMillis: Long = DEFAULT_SESSION_TTL_MS

    private val json = Json { ignoreUnknownKeys = true }
    private val sessions = ConcurrentHashMap<String, ProviderSession>()
    private val secureRandom = SecureRandom()

    final override fun onCreate(): Boolean {
        val appContext = context ?: return false
        require(provider.providerId == appContext.packageName || provider.providerId.startsWith("${appContext.packageName}.")) {
            "CETP provider_id must be the package name or use its namespace"
        }
        require(provider.limits.isProtocolCompliant()) { "CETP provider limits are below v2 minimums" }
        require(CetpConstants.CAPABILITY_CANCELLATION !in provider.capabilities ||
            CetpConstants.CAPABILITY_ASYNC_OPERATIONS in provider.capabilities
        ) { "CETP cancellation capability requires async_operations" }
        require(providerTools.map { it.name }.distinct().size == providerTools.size) {
            "CETP tool names must be unique"
        }
        providerTools.forEach { validateTool(it) }
        return onCetpCreate()
    }

    protected open fun onCetpCreate(): Boolean = true

    final override fun call(method: String, arg: String?, extras: Bundle?): Bundle {
        val appContext = context ?: return v2Error(
            requestId = extras?.getString(CetpConstants.EXTRA_REQUEST_ID),
            code = CetpConstants.ERROR_INTERNAL_ERROR,
            message = "Provider context is unavailable",
        )
        val caller = runCatching { CetpCallerIdentity.resolve(appContext, Binder.getCallingUid()) }
            .getOrElse {
                return v2Error(
                    requestId = extras?.getString(CetpConstants.EXTRA_REQUEST_ID),
                    code = CetpConstants.ERROR_PERMISSION_DENIED,
                    message = "Unable to verify caller identity",
                )
            }
        if (method == CetpConstants.METHOD_NEGOTIATE) {
            return runCatching { negotiate(caller, extras) }.getOrElse { error ->
                Log.e(TAG, "CETP v2 negotiation failed", error)
                v2Error(null, CetpConstants.ERROR_INTERNAL_ERROR, "Provider internal error")
            }
        }

        val protocolVersion = extras?.getInt(
            CetpConstants.EXTRA_PROTOCOL_VERSION,
            CetpConstants.PROTOCOL_VERSION_V1,
        ) ?: CetpConstants.PROTOCOL_VERSION_V1
        if (protocolVersion != CetpConstants.PROTOCOL_VERSION_V2) {
            return handleV1Call(method, arg, extras, caller)
        }
        return runCatching { handleV2Call(method, extras, caller) }.getOrElse { error ->
            Log.e(TAG, "CETP v2 provider call failed", error)
            v2Error(
                requestId = extras?.getString(CetpConstants.EXTRA_REQUEST_ID),
                code = CetpConstants.ERROR_INTERNAL_ERROR,
                message = "Provider internal error",
            )
        }
    }

    protected open fun handleV1Call(
        method: String,
        arg: String?,
        extras: Bundle?,
        caller: CetpCallerIdentity,
    ): Bundle = Bundle().apply {
        putString(CetpConstants.BUNDLE_STATUS, CetpConstants.STATUS_ERROR)
        putString(CetpConstants.BUNDLE_ERROR_CODE, CetpConstants.ERROR_METHOD_NOT_FOUND)
        putString(CetpConstants.BUNDLE_ERROR_MESSAGE, "CETP v1 is not supported")
    }

    protected open fun canDiscoverTool(caller: CetpCallerIdentity, tool: CetpProviderTool): Boolean = true

    protected open fun authorize(request: CetpProviderRequest): CetpAccessDecision =
        CetpAccessDecision.Denied()

    protected abstract fun executeTool(request: CetpProviderRequest): CetpProviderResult

    protected open fun getOperation(
        caller: CetpCallerIdentity,
        requestId: String,
        operationId: String,
    ): CetpProviderOperation? = null

    protected open fun cancelOperation(
        caller: CetpCallerIdentity,
        requestId: String,
        operationId: String,
    ): CetpProviderOperation? = null

    private fun negotiate(caller: CetpCallerIdentity, extras: Bundle?): Bundle {
        val versions = extras?.getIntArray(CetpConstants.EXTRA_CONSUMER_VERSIONS) ?: intArrayOf()
        if (CetpConstants.PROTOCOL_VERSION_V2 !in versions) {
            return v2Error(null, CetpConstants.ERROR_UNSUPPORTED_VERSION, "CETP v2 is not supported by Consumer")
        }
        val requestedCapabilities = runCatching {
            json.parseToJsonElement(
                extras?.getString(CetpConstants.EXTRA_CONSUMER_CAPABILITIES) ?: "[]",
            ).jsonArray.map { it.jsonPrimitive.content }.toSet()
        }.getOrElse {
            return v2Error(null, CetpConstants.ERROR_INVALID_REQUEST, "Invalid consumer_capabilities")
        }
        val selectedCapabilities = provider.capabilities.intersect(requestedCapabilities)
        val token = newSessionToken()
        val expiresAtMillis = System.currentTimeMillis() + sessionTtlMillis
        sessions[token] = ProviderSession(caller.uid, selectedCapabilities, expiresAtMillis)
        pruneSessions()
        val data = buildJsonObject {
            put("selected_version", CetpConstants.PROTOCOL_VERSION_V2)
            put("provider_id", provider.providerId)
            put("provider_name", provider.providerName)
            put("session_token", token)
            put("expires_at", java.time.Instant.ofEpochMilli(expiresAtMillis).toString())
            put("capabilities", buildJsonArray { selectedCapabilities.sorted().forEach(::add) })
            put("limits", provider.limits.toJson())
        }
        return success(data.toString(), requestId = null)
    }

    private fun handleV2Call(method: String, extras: Bundle?, caller: CetpCallerIdentity): Bundle {
        val requestId = extras?.getString(CetpConstants.EXTRA_REQUEST_ID)
        if (requestId == null || runCatching { UUID.fromString(requestId) }.isFailure) {
            return v2Error(requestId, CetpConstants.ERROR_INVALID_REQUEST, "Missing or invalid request_id")
        }
        val deadline = if (extras.containsKey(CetpConstants.EXTRA_DEADLINE_AT_MS)) {
            extras.getLong(CetpConstants.EXTRA_DEADLINE_AT_MS)
        } else {
            0L
        }
        if (deadline <= System.currentTimeMillis()) {
            return v2Error(requestId, CetpConstants.ERROR_DEADLINE_EXCEEDED, "Request deadline has elapsed")
        }
        val session = validateSession(extras.getString(CetpConstants.EXTRA_SESSION_TOKEN), caller)
            ?: return v2Error(
                requestId,
                CetpConstants.ERROR_NEGOTIATION_REQUIRED,
                "CETP negotiation is required",
                retryable = true,
            )

        return when (method) {
            CetpConstants.METHOD_GET_PROVIDER_INFO -> providerInfo(requestId)
            CetpConstants.METHOD_LIST_TOOLS -> listTools(caller, requestId)
            CetpConstants.METHOD_EXECUTE_TOOL -> execute(extras, caller, session, requestId, deadline)
            CetpConstants.METHOD_GET_OPERATION -> operation(extras, caller, session, requestId, cancel = false)
            CetpConstants.METHOD_CANCEL_OPERATION -> operation(extras, caller, session, requestId, cancel = true)
            else -> v2Error(requestId, CetpConstants.ERROR_METHOD_NOT_FOUND, "Unknown CETP method")
        }
    }

    private fun providerInfo(requestId: String): Bundle {
        val data = buildJsonObject {
            put("provider_id", provider.providerId)
            put("provider_name", provider.providerName)
            put("description", provider.description)
            put("schema_dialect", provider.schemaDialect)
            put("scopes", buildJsonArray {
                provider.scopes.forEach { scope ->
                    add(buildJsonObject {
                        put("name", scope.name)
                        put("title", scope.title)
                        put("description", scope.description)
                        put("access", scope.access.wireValue)
                        put("sensitivity", scope.sensitivity.wireValue)
                    })
                }
            })
        }
        return success(data.toString(), requestId)
    }

    private fun listTools(caller: CetpCallerIdentity, requestId: String): Bundle {
        val visibleTools = providerTools.filter { canDiscoverTool(caller, it) }
        val data = buildJsonObject {
            put("revision", toolRevision(visibleTools))
            put("tools", buildJsonArray {
                visibleTools.forEach { tool -> add(tool.toJson()) }
            })
        }
        return success(data.toString(), requestId)
    }

    private fun execute(
        extras: Bundle,
        caller: CetpCallerIdentity,
        session: ProviderSession,
        requestId: String,
        deadline: Long,
    ): Bundle {
        val toolName = extras.getString(CetpConstants.EXTRA_TOOL_NAME)
            ?: return v2Error(requestId, CetpConstants.ERROR_INVALID_REQUEST, "Missing tool_name")
        val tool = providerTools.firstOrNull { it.name == toolName && canDiscoverTool(caller, it) }
            ?: return v2Error(requestId, CetpConstants.ERROR_TOOL_NOT_FOUND, "Tool not found")
        if (tool.annotations.execution == ExecutionMode.SYNC_OR_ASYNC &&
            CetpConstants.CAPABILITY_ASYNC_OPERATIONS !in session.capabilities
        ) {
            return v2Error(
                requestId,
                CetpConstants.ERROR_UNSUPPORTED_CAPABILITY,
                "async_operations was not negotiated",
            )
        }
        val rawArgs = extras.getString(CetpConstants.EXTRA_ARGS) ?: "{}"
        if (rawArgs.toByteArray(Charsets.UTF_8).size > provider.limits.maxRequestBytes) {
            return v2Error(requestId, CetpConstants.ERROR_INVALID_ARGS, "Arguments exceed max_request_bytes")
        }
        val args = runCatching { json.parseToJsonElement(rawArgs).jsonObject }.getOrElse {
            return v2Error(requestId, CetpConstants.ERROR_INVALID_ARGS, "Arguments must be a JSON object")
        }
        val request = CetpProviderRequest(
            caller = caller,
            requestId = requestId,
            deadlineAtMillis = deadline,
            tool = tool,
            args = args,
            rawArgsJson = rawArgs,
            resumeToken = extras.getString(CetpConstants.EXTRA_RESUME_TOKEN),
        )
        when (val access = authorize(request)) {
            is CetpAccessDecision.Denied -> return v2Error(requestId, access.errorCode, access.message)
            is CetpAccessDecision.ActionRequired -> return actionRequired(requestId, access)
            is CetpAccessDecision.Granted -> if (tool.annotations.confirmation == ConfirmationPolicy.ALWAYS &&
                !access.providerConfirmed
            ) {
                return v2Error(
                    requestId,
                    CetpConstants.ERROR_INTERNAL_ERROR,
                    "Provider confirmation policy was not satisfied",
                )
            }
        }

        val result = if (tool.annotations.hasSideEffects) {
            val executor = mutationExecutor ?: return v2Error(
                requestId,
                CetpConstants.ERROR_INTERNAL_ERROR,
                "Provider has no durable mutation executor",
            )
            val now = System.currentTimeMillis()
            executor.execute(
                CetpMutationContext(
                    callerKey = caller.stableKey,
                    providerId = provider.providerId,
                    requestId = requestId,
                    fingerprintSha256 = requestFingerprint(toolName, rawArgs),
                    retainUntilMillis = now + provider.limits.idempotencyWindowSeconds * 1_000L,
                ),
            ) { executeTool(request) }
        } else {
            executeTool(request)
        }
        return result.toBundle(requestId, session)
    }

    private fun operation(
        extras: Bundle,
        caller: CetpCallerIdentity,
        session: ProviderSession,
        requestId: String,
        cancel: Boolean,
    ): Bundle {
        val capability = if (cancel) {
            CetpConstants.CAPABILITY_CANCELLATION
        } else {
            CetpConstants.CAPABILITY_ASYNC_OPERATIONS
        }
        if (capability !in session.capabilities) {
            return v2Error(
                requestId,
                CetpConstants.ERROR_UNSUPPORTED_CAPABILITY,
                "$capability was not negotiated",
            )
        }
        val operationId = extras.getString(CetpConstants.EXTRA_OPERATION_ID)
            ?: return v2Error(requestId, CetpConstants.ERROR_INVALID_REQUEST, "Missing operation_id")
        val operation = if (cancel) {
            cancelOperation(caller, requestId, operationId)
        } else {
            getOperation(caller, requestId, operationId)
        } ?: return v2Error(requestId, CetpConstants.ERROR_RESULT_EXPIRED, "Operation not found")
        if (operation.operationId != operationId) {
            return v2Error(requestId, CetpConstants.ERROR_INTERNAL_ERROR, "Operation identity mismatch")
        }
        return operation.toBundle(requestId, session)
    }

    private fun CetpProviderResult.toBundle(requestId: String, session: ProviderSession): Bundle = when (this) {
        is CetpProviderResult.Inline -> {
            val envelope = buildJsonObject {
                put("kind", CetpConstants.RESULT_KIND_INLINE)
                put("value", value)
            }.toString()
            if (envelope.toByteArray(Charsets.UTF_8).size > provider.limits.maxInlineResultBytes) {
                v2Error(requestId, CetpConstants.ERROR_RESULT_TOO_LARGE, "Result exceeds inline limit")
            } else {
                success(envelope, requestId)
            }
        }

        is CetpProviderResult.File -> {
            if (CetpConstants.CAPABILITY_LARGE_RESULTS !in session.capabilities) {
                descriptor.close()
                v2Error(
                    requestId,
                    CetpConstants.ERROR_RESULT_TOO_LARGE,
                    "large_results was not negotiated",
                )
            } else {
                require(sizeBytes >= 0) { "CETP file result size must be non-negative" }
                require(SHA256_HEX.matches(sha256)) { "CETP file result sha256 is invalid" }
                success(
                    data = buildJsonObject {
                        put("kind", CetpConstants.RESULT_KIND_FILE)
                        put("media_type", mediaType)
                        put("size_bytes", sizeBytes)
                        put("sha256", sha256)
                    }.toString(),
                    requestId = requestId,
                ).apply { putParcelable(CetpConstants.BUNDLE_RESULT_FD, descriptor) }
            }
        }

        is CetpProviderResult.Accepted -> {
            if (CetpConstants.CAPABILITY_ASYNC_OPERATIONS !in session.capabilities) {
                v2Error(
                    requestId,
                    CetpConstants.ERROR_UNSUPPORTED_CAPABILITY,
                    "async_operations was not negotiated",
                )
            } else {
                Bundle().apply {
                    putString(CetpConstants.BUNDLE_STATUS, CetpConstants.STATUS_ACCEPTED)
                    putInt(CetpConstants.BUNDLE_PROTOCOL_VERSION, CetpConstants.PROTOCOL_VERSION_V2)
                    putString(CetpConstants.BUNDLE_REQUEST_ID, requestId)
                    putString(
                        CetpConstants.BUNDLE_DATA,
                        buildJsonObject {
                            put("operation_id", operationId)
                            put("poll_after_ms", pollAfterMillis.coerceIn(
                                CetpConstants.MIN_OPERATION_POLL_MS,
                                CetpConstants.MAX_OPERATION_POLL_MS,
                            ))
                        }.toString(),
                    )
                }
            }
        }

        is CetpProviderResult.Failure -> v2Error(
            requestId = requestId,
            code = errorCode,
            message = message,
            retryable = retryable,
            retryAfterMillis = retryAfterMillis,
            details = details,
        )
    }

    private fun CetpProviderOperation.toBundle(requestId: String, session: ProviderSession): Bundle {
        var resultFd: ParcelFileDescriptor? = null
        val data = buildJsonObject {
            put("operation_id", operationId)
            put("state", state.wireValue)
            put("poll_after_ms", pollAfterMillis.coerceIn(
                CetpConstants.MIN_OPERATION_POLL_MS,
                CetpConstants.MAX_OPERATION_POLL_MS,
            ))
            if (progressCurrent != null || progressTotal != null || progressMessage != null) {
                put("progress", buildJsonObject {
                    progressCurrent?.let { put("current", it) }
                    progressTotal?.let { put("total", it) }
                    progressMessage?.let { put("message", it) }
                })
            }
            result?.let { providerResult ->
                when (providerResult) {
                    is CetpProviderResult.Inline -> put("result", buildJsonObject {
                        put("kind", CetpConstants.RESULT_KIND_INLINE)
                        put("value", providerResult.value)
                    })
                    is CetpProviderResult.File -> {
                        check(CetpConstants.CAPABILITY_LARGE_RESULTS in session.capabilities)
                        resultFd = providerResult.descriptor
                        put("result", buildJsonObject {
                            put("kind", CetpConstants.RESULT_KIND_FILE)
                            put("media_type", providerResult.mediaType)
                            put("size_bytes", providerResult.sizeBytes)
                            put("sha256", providerResult.sha256)
                        })
                    }
                    else -> error("Operation terminal result must be inline or file")
                }
            }
            error?.let { failure ->
                put("error", buildJsonObject {
                    put("error_code", failure.errorCode)
                    put("error_message", failure.message)
                    put("retryable", failure.retryable)
                    failure.retryAfterMillis?.let { put("retry_after_ms", it) }
                    failure.details?.let { put("error_details", it) }
                })
            }
        }
        return success(data.toString(), requestId).apply {
            resultFd?.let { putParcelable(CetpConstants.BUNDLE_RESULT_FD, it) }
        }
    }

    private fun actionRequired(requestId: String, decision: CetpAccessDecision.ActionRequired): Bundle =
        if (decision.resumeToken.isBlank() ||
            decision.resumeToken.toByteArray(Charsets.UTF_8).size > MAX_RESUME_TOKEN_BYTES ||
            decision.resolution.creatorPackage != context?.packageName
        ) {
            v2Error(
                requestId = requestId,
                code = CetpConstants.ERROR_INTERNAL_ERROR,
                message = "Provider returned an invalid action resolution",
            )
        } else v2Error(
            requestId = requestId,
            code = decision.errorCode,
            message = decision.message,
        ).apply {
            putString(CetpConstants.BUNDLE_RESOLUTION_HINT, decision.hint)
            putParcelable(CetpConstants.BUNDLE_RESOLUTION, decision.resolution)
            putString(CetpConstants.BUNDLE_RESUME_TOKEN, decision.resumeToken)
        }

    private fun success(data: String, requestId: String?): Bundle = Bundle().apply {
        putString(CetpConstants.BUNDLE_STATUS, CetpConstants.STATUS_SUCCESS)
        putInt(CetpConstants.BUNDLE_PROTOCOL_VERSION, CetpConstants.PROTOCOL_VERSION_V2)
        requestId?.let { putString(CetpConstants.BUNDLE_REQUEST_ID, it) }
        putString(CetpConstants.BUNDLE_DATA, data)
    }

    private fun v2Error(
        requestId: String?,
        code: String,
        message: String,
        retryable: Boolean = false,
        retryAfterMillis: Long? = null,
        details: JsonObject? = null,
    ): Bundle = Bundle().apply {
        putString(CetpConstants.BUNDLE_STATUS, CetpConstants.STATUS_ERROR)
        putInt(CetpConstants.BUNDLE_PROTOCOL_VERSION, CetpConstants.PROTOCOL_VERSION_V2)
        requestId?.let { putString(CetpConstants.BUNDLE_REQUEST_ID, it) }
        putString(CetpConstants.BUNDLE_ERROR_CODE, code)
        putString(CetpConstants.BUNDLE_ERROR_MESSAGE, message)
        putBoolean(CetpConstants.BUNDLE_RETRYABLE, retryable)
        retryAfterMillis?.let { putLong(CetpConstants.BUNDLE_RETRY_AFTER_MS, it) }
        details?.let { putString(CetpConstants.BUNDLE_ERROR_DETAILS, it.toString()) }
    }

    private fun validateSession(token: String?, caller: CetpCallerIdentity): ProviderSession? {
        if (token == null) return null
        val session = sessions[token] ?: return null
        if (session.callerUid != caller.uid || session.expiresAtMillis <= System.currentTimeMillis()) {
            sessions.remove(token, session)
            return null
        }
        return session
    }

    private fun newSessionToken(): String {
        val bytes = ByteArray(32)
        secureRandom.nextBytes(bytes)
        return Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    }

    private fun pruneSessions() {
        val now = System.currentTimeMillis()
        sessions.entries.removeIf { it.value.expiresAtMillis <= now }
    }

    private fun validateTool(tool: CetpProviderTool) {
        require(TOOL_NAME.matches(tool.name)) { "Invalid CETP tool name: ${tool.name}" }
        val declaredScopes = provider.scopes.map { it.name }.toSet()
        require(tool.scopes.all { it in declaredScopes }) { "Tool ${tool.name} uses an undeclared scope" }
        val annotations = tool.annotations
        if (annotations.effect == ToolEffect.DELETE || annotations.effect == ToolEffect.TRANSACTION ||
            annotations.destructive || annotations.risk == ToolRisk.HIGH
        ) {
            require(annotations.confirmation == ConfirmationPolicy.ALWAYS) {
                "High-risk CETP tool ${tool.name} must always require Provider confirmation"
            }
        }
        if (annotations.execution == ExecutionMode.SYNC_OR_ASYNC) {
            require(CetpConstants.CAPABILITY_ASYNC_OPERATIONS in provider.capabilities) {
                "Async CETP tool ${tool.name} requires async_operations capability"
            }
        }
    }

    private fun toolRevision(tools: List<CetpProviderTool>): String {
        val canonical = buildJsonArray { tools.sortedBy { it.name }.forEach { add(it.toJson()) } }.toString()
        return MessageDigest.getInstance("SHA-256")
            .digest(canonical.toByteArray(Charsets.UTF_8))
            .take(8)
            .joinToString("") { "%02x".format(it) }
    }

    private fun CetpProviderTool.toJson(): JsonObject = buildJsonObject {
        put("name", name)
        put("title", title)
        put("description", description)
        put("input_schema", inputSchema)
        put("output_schema", outputSchema)
        put("scopes", buildJsonArray { scopes.forEach(::add) })
        put("annotations", buildJsonObject {
            put("effect", annotations.effect.wireValue)
            put("risk", annotations.risk.wireValue)
            put("destructive", annotations.destructive)
            put("idempotent", annotations.idempotent)
            put("open_world", annotations.openWorld)
            put("confirmation", annotations.confirmation.wireValue)
            put("execution", annotations.execution.wireValue)
        })
    }

    private fun CetpLimits.toJson(): JsonObject = buildJsonObject {
        put("max_request_bytes", maxRequestBytes)
        put("max_inline_result_bytes", maxInlineResultBytes)
        put("max_concurrent_requests", maxConcurrentRequests)
        put("idempotency_window_seconds", idempotencyWindowSeconds)
    }

    private fun CetpLimits.isProtocolCompliant(): Boolean =
        maxRequestBytes >= CetpConstants.DEFAULT_MAX_REQUEST_BYTES &&
            maxInlineResultBytes >= CetpConstants.DEFAULT_MAX_INLINE_RESULT_BYTES &&
            maxConcurrentRequests >= 1 &&
            idempotencyWindowSeconds >= CetpConstants.MIN_IDEMPOTENCY_WINDOW_SECONDS

    private fun requestFingerprint(toolName: String, rawArgs: String): String =
        MessageDigest.getInstance("SHA-256")
            .digest("$toolName\u0000$rawArgs".toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }

    final override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    final override fun getType(uri: Uri): String? = null

    final override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    final override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    final override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    private data class ProviderSession(
        val callerUid: Int,
        val capabilities: Set<String>,
        val expiresAtMillis: Long,
    )

    companion object {
        private const val TAG = "CetpV2ContentProvider"
        private const val DEFAULT_SESSION_TTL_MS = 60 * 60_000L
        private const val MAX_RESUME_TOKEN_BYTES = 256
        private val TOOL_NAME = Regex("[a-z][a-z0-9_]{0,63}")
        private val SHA256_HEX = Regex("[0-9a-f]{64}")
    }
}
