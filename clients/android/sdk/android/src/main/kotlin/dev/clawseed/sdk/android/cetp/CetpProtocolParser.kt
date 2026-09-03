package dev.clawseed.sdk.android.cetp

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

internal class CetpProtocolParser(
    private val json: Json = Json { ignoreUnknownKeys = true },
) {
    fun advertisedVersions(legacyVersion: Int, versionsValue: String?): Set<Int> {
        if (versionsValue.isNullOrBlank()) return setOf(legacyVersion)
        return versionsValue
            .split(',')
            .mapNotNull { it.trim().toIntOrNull() }
            .filter { it > 0 }
            .toSortedSet()
            .ifEmpty { setOf(legacyVersion) }
    }

    fun negotiation(dataJson: String): CetpSession? = runCatching {
        val root = json.parseToJsonElement(dataJson).jsonObject
        val selectedVersion = root.requiredInt("selected_version")
        val providerId = root.requiredString("provider_id")
        val providerName = root.requiredString("provider_name")
        val sessionToken = root.requiredString("session_token")
        val capabilities = root["capabilities"]?.jsonArray
            ?.map { it.jsonPrimitive.content }
            ?.toSet()
            ?: emptySet()
        val limitsObject = root["limits"]?.jsonObject ?: JsonObject(emptyMap())
        CetpSession(
            protocolVersion = selectedVersion,
            providerId = providerId,
            providerName = providerName,
            sessionToken = sessionToken,
            expiresAt = root["expires_at"]?.jsonPrimitive?.contentOrNull,
            capabilities = capabilities,
            limits = CetpLimits(
                maxRequestBytes = limitsObject["max_request_bytes"]?.jsonPrimitive?.intOrNull
                    ?: CetpConstants.DEFAULT_MAX_REQUEST_BYTES,
                maxInlineResultBytes = limitsObject["max_inline_result_bytes"]?.jsonPrimitive?.intOrNull
                    ?: CetpConstants.DEFAULT_MAX_INLINE_RESULT_BYTES,
                maxConcurrentRequests = limitsObject["max_concurrent_requests"]?.jsonPrimitive?.intOrNull ?: 1,
                idempotencyWindowSeconds = limitsObject["idempotency_window_seconds"]
                    ?.jsonPrimitive?.longOrNull ?: CetpConstants.MIN_IDEMPOTENCY_WINDOW_SECONDS,
            ),
        )
    }.getOrNull()

    fun providerInfo(
        dataJson: String,
        fallbackProviderId: String = "",
        strictSecurityEnums: Boolean = false,
    ): ProviderInfo? = runCatching {
        val root = json.parseToJsonElement(dataJson).jsonObject
        val scopes = root["scopes"]?.jsonArray?.mapNotNull { element ->
            val scope = element.jsonObject
            val name = scope["name"]?.jsonPrimitive?.contentOrNull ?: return@mapNotNull null
            val access = enumByWire<ScopeAccess>(scope["access"]?.jsonPrimitive?.contentOrNull)
            val sensitivity = enumByWire<ScopeSensitivity>(scope["sensitivity"]?.jsonPrimitive?.contentOrNull)
            if (strictSecurityEnums && (access == null || sensitivity == null)) return null
            ProviderScope(
                name = name,
                description = scope["description"]?.jsonPrimitive?.contentOrNull ?: "",
                title = scope["title"]?.jsonPrimitive?.contentOrNull ?: name,
                access = access ?: ScopeAccess.READ,
                sensitivity = sensitivity ?: ScopeSensitivity.OTHER,
            )
        } ?: emptyList()
        ProviderInfo(
            providerName = root["provider_name"]?.jsonPrimitive?.contentOrNull ?: "",
            description = root["description"]?.jsonPrimitive?.contentOrNull ?: "",
            scopes = scopes,
            providerId = root["provider_id"]?.jsonPrimitive?.contentOrNull ?: fallbackProviderId,
            schemaDialect = root["schema_dialect"]?.jsonPrimitive?.contentOrNull
                ?: CetpConstants.DEFAULT_SCHEMA_DIALECT,
        )
    }.getOrNull()

    fun tools(dataJson: String, namespace: String, protocolVersion: Int): List<DiscoveredTool> = runCatching {
        val root = json.parseToJsonElement(dataJson).jsonObject
        root["tools"]?.jsonArray?.mapNotNull { element ->
            val tool = element.jsonObject
            val name = tool["name"]?.jsonPrimitive?.contentOrNull ?: return@mapNotNull null
            if (name.isBlank() ||
                (protocolVersion >= CetpConstants.PROTOCOL_VERSION_V2 && !TOOL_NAME.matches(name))
            ) {
                return@mapNotNull null
            }
            val description = tool["description"]?.jsonPrimitive?.contentOrNull ?: ""
            val inputSchema = if (protocolVersion >= CetpConstants.PROTOCOL_VERSION_V2) {
                tool["input_schema"]
            } else {
                tool["parameters"]
            } ?: return@mapNotNull null
            val annotations = if (protocolVersion >= CetpConstants.PROTOCOL_VERSION_V2) {
                parseAnnotations(tool["annotations"]?.jsonObject ?: return@mapNotNull null)
                    ?: return@mapNotNull null
            } else {
                ToolAnnotations()
            }
            DiscoveredTool(
                name = name,
                namespacedName = "$namespace${CetpConstants.NAMESPACE_SEPARATOR}$name",
                description = description,
                parametersJson = inputSchema.toString(),
                title = tool["title"]?.jsonPrimitive?.contentOrNull ?: description,
                outputSchemaJson = tool["output_schema"]?.toString(),
                scopes = tool["scopes"]?.jsonArray?.map { it.jsonPrimitive.content } ?: emptyList(),
                annotations = annotations,
            )
        } ?: emptyList()
    }.getOrDefault(emptyList())

    fun operation(dataJson: String): CetpOperation? = runCatching {
        val root = json.parseToJsonElement(dataJson).jsonObject
        val state = enumByWire<OperationState>(root.requiredString("state")) ?: return null
        val errorObject = root["error"]?.jsonObject
        CetpOperation(
            operationId = root.requiredString("operation_id"),
            state = state,
            pollAfterMillis = root["poll_after_ms"]?.jsonPrimitive?.longOrNull
                ?: CetpConstants.DEFAULT_OPERATION_POLL_MS,
            resultJson = root["result"]?.toString(),
            error = errorObject?.let {
                CetpOperationError(
                    code = it.requiredString("error_code"),
                    message = it.requiredString("error_message"),
                    retryable = it["retryable"]?.jsonPrimitive?.booleanOrNull ?: false,
                    retryAfterMillis = it["retry_after_ms"]?.jsonPrimitive?.longOrNull,
                )
            },
        )
    }.getOrNull()

    fun inlineResult(dataJson: String): String? = runCatching {
        val root = json.parseToJsonElement(dataJson).jsonObject
        if (root.requiredString("kind") != CetpConstants.RESULT_KIND_INLINE) return null
        root["value"]?.toString() ?: "null"
    }.getOrNull()

    fun operationId(dataJson: String): String? = runCatching {
        json.parseToJsonElement(dataJson).jsonObject.requiredString("operation_id")
    }.getOrNull()

    private fun parseAnnotations(value: JsonObject): ToolAnnotations? {
        val effect = enumByWire<ToolEffect>(value["effect"]?.jsonPrimitive?.contentOrNull) ?: return null
        val risk = enumByWire<ToolRisk>(value["risk"]?.jsonPrimitive?.contentOrNull) ?: return null
        val confirmation = enumByWire<ConfirmationPolicy>(
            value["confirmation"]?.jsonPrimitive?.contentOrNull,
        ) ?: return null
        val execution = enumByWire<ExecutionMode>(value["execution"]?.jsonPrimitive?.contentOrNull) ?: return null
        val annotations = ToolAnnotations(
            effect = effect,
            risk = risk,
            destructive = value["destructive"]?.jsonPrimitive?.booleanOrNull ?: return null,
            idempotent = value["idempotent"]?.jsonPrimitive?.booleanOrNull ?: return null,
            openWorld = value["open_world"]?.jsonPrimitive?.booleanOrNull ?: return null,
            confirmation = confirmation,
            execution = execution,
        )
        if ((effect == ToolEffect.DELETE || effect == ToolEffect.TRANSACTION || annotations.destructive ||
                risk == ToolRisk.HIGH) && confirmation != ConfirmationPolicy.ALWAYS
        ) {
            return null
        }
        return annotations
    }

    private fun JsonObject.requiredString(key: String): String =
        this[key]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }
            ?: error("Missing $key")

    private fun JsonObject.requiredInt(key: String): Int =
        this[key]?.jsonPrimitive?.intOrNull ?: error("Missing $key")

    private inline fun <reified T> enumByWire(value: String?): T? where T : Enum<T> =
        enumValues<T>().firstOrNull {
            when (it) {
                is ToolEffect -> it.wireValue == value
                is ToolRisk -> it.wireValue == value
                is ConfirmationPolicy -> it.wireValue == value
                is ExecutionMode -> it.wireValue == value
                is ScopeAccess -> it.wireValue == value
                is ScopeSensitivity -> it.wireValue == value
                is OperationState -> it.wireValue == value
                else -> false
            }
        }

    companion object {
        private val TOOL_NAME = Regex("[a-z][a-z0-9_]{0,63}")
    }
}
