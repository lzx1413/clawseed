package dev.clawseed.sdk.android.cetp

import android.app.PendingIntent
import android.content.ContentResolver
import android.content.Context
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.util.Log
import androidx.core.net.toUri
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray

sealed class CetpResult<out T> {
    data class Success<T>(
        val data: T,
        val protocolVersion: Int = CetpConstants.PROTOCOL_VERSION_V1,
        val requestId: String? = null,
        val fileDescriptor: ParcelFileDescriptor? = null,
    ) : CetpResult<T>()

    data class Accepted<T>(
        val data: T,
        val protocolVersion: Int,
        val requestId: String,
    ) : CetpResult<T>()

    data class Error(
        val errorCode: String,
        val errorMessage: String,
        val resolutionHint: String? = null,
        val authorizeIntent: String? = null,
        val retryable: Boolean = false,
        val retryAfterMillis: Long? = null,
        val errorDetailsJson: String? = null,
        val resolution: PendingIntent? = null,
        val resumeToken: String? = null,
        val protocolVersion: Int = CetpConstants.PROTOCOL_VERSION_V1,
        val requestId: String? = null,
    ) : CetpResult<Nothing>()
}

class CetpClient(private val context: Context) {
    private val parser = CetpProtocolParser()

    private val contentResolver: ContentResolver
        get() = context.contentResolver

    fun negotiate(
        authority: String,
        consumerVersions: IntArray = intArrayOf(
            CetpConstants.PROTOCOL_VERSION_V2,
            CetpConstants.PROTOCOL_VERSION_V1,
        ),
        consumerCapabilities: Set<String> = setOf(
            CetpConstants.CAPABILITY_ASYNC_OPERATIONS,
            CetpConstants.CAPABILITY_CANCELLATION,
            CetpConstants.CAPABILITY_LARGE_RESULTS,
        ),
    ): CetpResult<CetpSession> {
        val extras = Bundle().apply {
            putIntArray(CetpConstants.EXTRA_CONSUMER_VERSIONS, consumerVersions)
            putString(
                CetpConstants.EXTRA_CONSUMER_CAPABILITIES,
                buildJsonArray { consumerCapabilities.sorted().forEach { add(it) } }.toString(),
            )
        }
        return callProvider(authority, CetpConstants.METHOD_NEGOTIATE, extras).mapSuccess { data ->
            parser.negotiation(data)?.takeIf { session ->
                session.protocolVersion in consumerVersions &&
                    session.capabilities.all { it in consumerCapabilities } &&
                    session.sessionToken.toByteArray(Charsets.UTF_8).size <= MAX_SESSION_TOKEN_BYTES &&
                    session.limits.isValid()
            } ?: error("Invalid CETP negotiation response")
        }
    }

    fun listTools(
        authority: String,
        session: CetpSession? = null,
        requestId: String = newRequestId(),
        deadlineAtMillis: Long = defaultDeadline(),
    ): CetpResult<String> {
        val extras = session?.requestExtras(requestId, deadlineAtMillis)
        return callProvider(authority, CetpConstants.METHOD_LIST_TOOLS, extras)
    }

    fun executeTool(
        authority: String,
        toolName: String,
        argsJson: String,
        requestId: String? = null,
        session: CetpSession? = null,
        deadlineAtMillis: Long = defaultDeadline(),
        resumeToken: String? = null,
    ): CetpResult<String> {
        val effectiveRequestId = requestId ?: newRequestId()
        if (session != null && argsJson.toByteArray(Charsets.UTF_8).size > session.limits.maxRequestBytes) {
            return CetpResult.Error(
                errorCode = CetpConstants.ERROR_INVALID_ARGS,
                errorMessage = "CETP request exceeds negotiated max_request_bytes",
                protocolVersion = session.protocolVersion,
                requestId = effectiveRequestId,
            )
        }
        val extras = (session?.requestExtras(effectiveRequestId, deadlineAtMillis) ?: Bundle()).apply {
            putString(CetpConstants.EXTRA_TOOL_NAME, toolName)
            putString(CetpConstants.EXTRA_ARGS, argsJson)
            putString(CetpConstants.EXTRA_REQUEST_ID, effectiveRequestId)
            resumeToken?.let { putString(CetpConstants.EXTRA_RESUME_TOKEN, it) }
        }
        return callProvider(authority, CetpConstants.METHOD_EXECUTE_TOOL, extras)
    }

    fun getProviderInfo(
        authority: String,
        session: CetpSession? = null,
        requestId: String = newRequestId(),
        deadlineAtMillis: Long = defaultDeadline(),
    ): CetpResult<String>? {
        val extras = session?.requestExtras(requestId, deadlineAtMillis)
        return callProvider(authority, CetpConstants.METHOD_GET_PROVIDER_INFO, extras)
    }

    fun getOperation(
        authority: String,
        session: CetpSession,
        operationId: String,
        requestId: String,
        deadlineAtMillis: Long,
    ): CetpResult<CetpOperation> {
        if (CetpConstants.CAPABILITY_ASYNC_OPERATIONS !in session.capabilities) {
            return unsupportedCapability(session, requestId, CetpConstants.CAPABILITY_ASYNC_OPERATIONS)
        }
        val extras = session.requestExtras(requestId, deadlineAtMillis).apply {
            putString(CetpConstants.EXTRA_OPERATION_ID, operationId)
        }
        return callProvider(authority, CetpConstants.METHOD_GET_OPERATION, extras).mapSuccess { data ->
            parser.operation(data) ?: error("Invalid CETP operation response")
        }
    }

    fun cancelOperation(
        authority: String,
        session: CetpSession,
        operationId: String,
        requestId: String,
        deadlineAtMillis: Long = defaultDeadline(),
    ): CetpResult<CetpOperation> {
        if (CetpConstants.CAPABILITY_CANCELLATION !in session.capabilities) {
            return unsupportedCapability(session, requestId, CetpConstants.CAPABILITY_CANCELLATION)
        }
        val extras = session.requestExtras(requestId, deadlineAtMillis).apply {
            putString(CetpConstants.EXTRA_OPERATION_ID, operationId)
        }
        return callProvider(authority, CetpConstants.METHOD_CANCEL_OPERATION, extras).mapSuccess { data ->
            parser.operation(data) ?: error("Invalid CETP cancellation response")
        }
    }

    private fun callProvider(authority: String, method: String, extras: Bundle?): CetpResult<String> {
        val uri = "content://$authority".toUri()
        return try {
            val response = contentResolver.call(uri, method, null, extras)
                ?: return unavailable(authority, extras)
            parseResponse(response)
        } catch (e: IllegalArgumentException) {
            Log.w(TAG, "Authority not found, preparing provider process for $authority")
            try {
                contentResolver.acquireContentProviderClient(authority)?.close()
            } catch (_: Exception) {
            }
            try {
                Thread.sleep(PROVIDER_RETRY_DELAY_MS)
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            }
            try {
                val response = contentResolver.call(uri, method, null, extras)
                    ?: return unavailable(authority, extras)
                parseResponse(response)
            } catch (retryError: Exception) {
                Log.w(TAG, "Retry failed for $authority/$method: ${retryError.message}")
                unavailable(authority, extras)
            }
        } catch (e: SecurityException) {
            Log.w(TAG, "SecurityException calling $authority/$method: ${e.message}")
            CetpResult.Error(
                errorCode = CetpConstants.ERROR_PERMISSION_DENIED,
                errorMessage = "Provider denied access",
                protocolVersion = extras.protocolVersion(),
                requestId = extras?.getString(CetpConstants.EXTRA_REQUEST_ID),
            )
        } catch (e: Exception) {
            Log.w(TAG, "Exception calling $authority/$method: ${e.message}")
            unavailable(authority, extras)
        }
    }

    @Suppress("DEPRECATION")
    private fun parseResponse(bundle: Bundle): CetpResult<String> {
        val status = bundle.getString(CetpConstants.BUNDLE_STATUS, "")
        val protocolVersion = bundle.getInt(
            CetpConstants.BUNDLE_PROTOCOL_VERSION,
            CetpConstants.PROTOCOL_VERSION_V1,
        )
        val requestId = bundle.getString(CetpConstants.BUNDLE_REQUEST_ID)
        return when (status) {
            CetpConstants.STATUS_SUCCESS -> CetpResult.Success(
                data = bundle.getString(CetpConstants.BUNDLE_DATA, ""),
                protocolVersion = protocolVersion,
                requestId = requestId,
                fileDescriptor = bundle.getParcelable(CetpConstants.BUNDLE_RESULT_FD) as? ParcelFileDescriptor,
            )

            CetpConstants.STATUS_ACCEPTED -> CetpResult.Accepted(
                data = bundle.getString(CetpConstants.BUNDLE_DATA, ""),
                protocolVersion = protocolVersion,
                requestId = requestId ?: "",
            )

            CetpConstants.STATUS_ERROR -> CetpResult.Error(
                errorCode = bundle.getString(
                    CetpConstants.BUNDLE_ERROR_CODE,
                    CetpConstants.ERROR_INTERNAL_ERROR,
                ),
                errorMessage = bundle.getString(CetpConstants.BUNDLE_ERROR_MESSAGE, "Unknown error"),
                resolutionHint = bundle.getString(CetpConstants.BUNDLE_RESOLUTION_HINT),
                authorizeIntent = bundle.getString(CetpConstants.BUNDLE_AUTHORIZE_INTENT),
                retryable = bundle.getBoolean(CetpConstants.BUNDLE_RETRYABLE, false),
                retryAfterMillis = bundle.getLongOrNull(CetpConstants.BUNDLE_RETRY_AFTER_MS),
                errorDetailsJson = bundle.getString(CetpConstants.BUNDLE_ERROR_DETAILS),
                resolution = bundle.getParcelable(CetpConstants.BUNDLE_RESOLUTION) as? PendingIntent,
                resumeToken = bundle.getString(CetpConstants.BUNDLE_RESUME_TOKEN),
                protocolVersion = protocolVersion,
                requestId = requestId,
            )

            else -> CetpResult.Error(
                errorCode = CetpConstants.ERROR_INTERNAL_ERROR,
                errorMessage = "Unexpected CETP status: $status",
                protocolVersion = protocolVersion,
                requestId = requestId,
            )
        }
    }

    private fun unavailable(authority: String, extras: Bundle?): CetpResult.Error = CetpResult.Error(
        errorCode = CetpConstants.ERROR_UNAVAILABLE,
        errorMessage = "Provider call failed for $authority",
        retryable = true,
        protocolVersion = extras.protocolVersion(),
        requestId = extras?.getString(CetpConstants.EXTRA_REQUEST_ID),
    )

    private fun unsupportedCapability(
        session: CetpSession,
        requestId: String,
        capability: String,
    ): CetpResult.Error = CetpResult.Error(
        errorCode = CetpConstants.ERROR_UNSUPPORTED_CAPABILITY,
        errorMessage = "CETP capability was not negotiated: $capability",
        protocolVersion = session.protocolVersion,
        requestId = requestId,
    )

    private fun CetpSession.requestExtras(requestId: String, deadlineAtMillis: Long): Bundle = Bundle().apply {
        putInt(CetpConstants.EXTRA_PROTOCOL_VERSION, protocolVersion)
        putString(CetpConstants.EXTRA_SESSION_TOKEN, sessionToken)
        putString(CetpConstants.EXTRA_REQUEST_ID, requestId)
        putLong(CetpConstants.EXTRA_DEADLINE_AT_MS, deadlineAtMillis)
    }

    private fun CetpLimits.isValid(): Boolean =
        maxRequestBytes >= CetpConstants.DEFAULT_MAX_REQUEST_BYTES &&
            maxInlineResultBytes >= CetpConstants.DEFAULT_MAX_INLINE_RESULT_BYTES &&
            maxConcurrentRequests >= 1 &&
            idempotencyWindowSeconds >= CetpConstants.MIN_IDEMPOTENCY_WINDOW_SECONDS

    private fun Bundle?.protocolVersion(): Int = this?.getInt(
        CetpConstants.EXTRA_PROTOCOL_VERSION,
        CetpConstants.PROTOCOL_VERSION_V1,
    ) ?: CetpConstants.PROTOCOL_VERSION_V1

    private fun Bundle.getLongOrNull(key: String): Long? = if (containsKey(key)) getLong(key) else null

    private inline fun <T, R> CetpResult<T>.mapSuccess(transform: (T) -> R): CetpResult<R> = when (this) {
        is CetpResult.Success -> runCatching { transform(data) }.fold(
            onSuccess = {
                CetpResult.Success(
                    data = it,
                    protocolVersion = protocolVersion,
                    requestId = requestId,
                    fileDescriptor = fileDescriptor,
                )
            },
            onFailure = {
                fileDescriptor?.close()
                CetpResult.Error(
                    errorCode = CetpConstants.ERROR_INTERNAL_ERROR,
                    errorMessage = it.message ?: "Invalid CETP response",
                    protocolVersion = protocolVersion,
                    requestId = requestId,
                )
            },
        )

        is CetpResult.Accepted -> CetpResult.Error(
            errorCode = CetpConstants.ERROR_INTERNAL_ERROR,
            errorMessage = "Unexpected accepted response",
            protocolVersion = protocolVersion,
            requestId = requestId,
        )

        is CetpResult.Error -> this
    }

    companion object {
        private const val TAG = "CetpClient"
        private const val PROVIDER_RETRY_DELAY_MS = 300L
        private const val MAX_SESSION_TOKEN_BYTES = 256

        internal fun newRequestId(): String = java.util.UUID.randomUUID().toString()

        internal fun defaultDeadline(timeoutMillis: Long = CetpConstants.DEFAULT_REQUEST_TIMEOUT_MS): Long =
            System.currentTimeMillis() + timeoutMillis
    }
}
