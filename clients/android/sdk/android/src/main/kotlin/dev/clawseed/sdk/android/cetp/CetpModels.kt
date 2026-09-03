package dev.clawseed.sdk.android.cetp

import android.app.PendingIntent

data class DiscoveredProvider(
    val packageName: String,
    val authority: String,
    val version: Int,
    val providerLabel: String,
    val tools: List<DiscoveredTool>,
    val providerInfo: ProviderInfo?,
    val supportedVersions: Set<Int> = setOf(version),
    val session: CetpSession? = null,
)

data class DiscoveredTool(
    val name: String,
    val namespacedName: String,
    val description: String,
    val parametersJson: String,
    val title: String = description,
    val outputSchemaJson: String? = null,
    val scopes: List<String> = emptyList(),
    val annotations: ToolAnnotations = ToolAnnotations(),
)

data class ProviderInfo(
    val providerName: String,
    val description: String,
    val scopes: List<ProviderScope>,
    val providerId: String = "",
    val schemaDialect: String = CetpConstants.DEFAULT_SCHEMA_DIALECT,
)

data class ProviderScope(
    val name: String,
    val description: String,
    val title: String = name,
    val access: ScopeAccess = ScopeAccess.READ,
    val sensitivity: ScopeSensitivity = ScopeSensitivity.OTHER,
)

data class AuthRequiredEvent(
    val providerPackageName: String,
    val providerLabel: String,
    val toolName: String,
    val resolutionHint: String?,
    val authorizeIntent: String?,
    val resolution: PendingIntent? = null,
    val resumeToken: String? = null,
    val requestId: String? = null,
    val errorCode: String = CetpConstants.ERROR_AUTH_REQUIRED,
)

data class CetpSession(
    val protocolVersion: Int,
    val providerId: String,
    val providerName: String,
    val sessionToken: String,
    val expiresAt: String?,
    val capabilities: Set<String>,
    val limits: CetpLimits,
)

data class CetpLimits(
    val maxRequestBytes: Int = CetpConstants.DEFAULT_MAX_REQUEST_BYTES,
    val maxInlineResultBytes: Int = CetpConstants.DEFAULT_MAX_INLINE_RESULT_BYTES,
    val maxConcurrentRequests: Int = 1,
    val idempotencyWindowSeconds: Long = CetpConstants.MIN_IDEMPOTENCY_WINDOW_SECONDS,
)

enum class ToolEffect(val wireValue: String) {
    READ("read"),
    CREATE("create"),
    UPDATE("update"),
    DELETE("delete"),
    TRANSACTION("transaction"),
}

enum class ToolRisk(val wireValue: String) {
    LOW("low"),
    MODERATE("moderate"),
    HIGH("high"),
}

enum class ConfirmationPolicy(val wireValue: String) {
    NEVER("never"),
    PROVIDER_POLICY("provider_policy"),
    ALWAYS("always"),
}

enum class ExecutionMode(val wireValue: String) {
    SYNC("sync"),
    SYNC_OR_ASYNC("sync_or_async"),
}

data class ToolAnnotations(
    val effect: ToolEffect = ToolEffect.READ,
    val risk: ToolRisk = ToolRisk.LOW,
    val destructive: Boolean = false,
    val idempotent: Boolean = true,
    val openWorld: Boolean = false,
    val confirmation: ConfirmationPolicy = ConfirmationPolicy.NEVER,
    val execution: ExecutionMode = ExecutionMode.SYNC,
) {
    val hasSideEffects: Boolean get() = effect != ToolEffect.READ
}

enum class ScopeAccess(val wireValue: String) {
    READ("read"),
    WRITE("write"),
}

enum class ScopeSensitivity(val wireValue: String) {
    PUBLIC("public"),
    PERSONAL("personal"),
    FINANCIAL("financial"),
    HEALTH("health"),
    DEVICE("device"),
    OTHER("other"),
}

enum class OperationState(val wireValue: String) {
    QUEUED("queued"),
    RUNNING("running"),
    SUCCEEDED("succeeded"),
    FAILED("failed"),
    CANCELLED("cancelled"),
    CANCELLATION_REQUESTED("cancellation_requested"),
}

data class CetpOperation(
    val operationId: String,
    val state: OperationState,
    val pollAfterMillis: Long = CetpConstants.DEFAULT_OPERATION_POLL_MS,
    val resultJson: String? = null,
    val error: CetpOperationError? = null,
)

data class CetpOperationError(
    val code: String,
    val message: String,
    val retryable: Boolean = false,
    val retryAfterMillis: Long? = null,
)
