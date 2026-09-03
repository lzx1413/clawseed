package dev.clawseed.sdk.android.cetp

object CetpConstants {
    const val ACTION_TOOL_PROVIDER = "com.clawseed.action.TOOL_PROVIDER"
    const val META_AUTHORITY = "com.clawseed.tools.authority"
    const val META_VERSION = "com.clawseed.tools.version"
    const val META_VERSIONS = "com.clawseed.tools.versions"
    const val PERMISSION_ACCESS_TOOLS = "com.clawseed.permission.ACCESS_TOOLS"

    const val METHOD_NEGOTIATE = "negotiate"
    const val METHOD_LIST_TOOLS = "list_tools"
    const val METHOD_EXECUTE_TOOL = "execute_tool"
    const val METHOD_GET_PROVIDER_INFO = "get_provider_info"
    const val METHOD_GET_OPERATION = "get_operation"
    const val METHOD_CANCEL_OPERATION = "cancel_operation"

    const val BUNDLE_STATUS = "status"
    const val BUNDLE_DATA = "data"
    const val BUNDLE_PROTOCOL_VERSION = "protocol_version"
    const val BUNDLE_REQUEST_ID = "request_id"
    const val BUNDLE_ERROR_CODE = "error_code"
    const val BUNDLE_ERROR_MESSAGE = "error_message"
    const val BUNDLE_ERROR_DETAILS = "error_details"
    const val BUNDLE_RETRYABLE = "retryable"
    const val BUNDLE_RETRY_AFTER_MS = "retry_after_ms"
    const val BUNDLE_RESOLUTION_HINT = "resolution_hint"
    const val BUNDLE_AUTHORIZE_INTENT = "authorize_intent"
    const val BUNDLE_RESOLUTION = "resolution"
    const val BUNDLE_RESUME_TOKEN = "resume_token"
    const val BUNDLE_RESULT_FD = "result_fd"

    const val EXTRA_TOOL_NAME = "tool_name"
    const val EXTRA_ARGS = "args"
    const val EXTRA_REQUEST_ID = "request_id"
    const val EXTRA_PROTOCOL_VERSION = "protocol_version"
    const val EXTRA_SESSION_TOKEN = "session_token"
    const val EXTRA_DEADLINE_AT_MS = "deadline_at_ms"
    const val EXTRA_RESUME_TOKEN = "resume_token"
    const val EXTRA_OPERATION_ID = "operation_id"
    const val EXTRA_CONSUMER_VERSIONS = "consumer_versions"
    const val EXTRA_CONSUMER_CAPABILITIES = "consumer_capabilities"

    const val STATUS_SUCCESS = "success"
    const val STATUS_ACCEPTED = "accepted"
    const val STATUS_ERROR = "error"

    const val ERROR_UNSUPPORTED_VERSION = "UNSUPPORTED_VERSION"
    const val ERROR_UNSUPPORTED_CAPABILITY = "UNSUPPORTED_CAPABILITY"
    const val ERROR_NEGOTIATION_REQUIRED = "NEGOTIATION_REQUIRED"
    const val ERROR_AUTH_REQUIRED = "AUTH_REQUIRED"
    const val ERROR_USER_ACTION_REQUIRED = "USER_ACTION_REQUIRED"
    const val ERROR_USER_CANCELLED = "USER_CANCELLED"
    const val ERROR_PERMISSION_DENIED = "PERMISSION_DENIED"
    const val ERROR_TOOL_NOT_FOUND = "TOOL_NOT_FOUND"
    const val ERROR_METHOD_NOT_FOUND = "METHOD_NOT_FOUND"
    const val ERROR_INVALID_ARGS = "INVALID_ARGS"
    const val ERROR_INVALID_REQUEST = "INVALID_REQUEST"
    const val ERROR_CONFLICT = "CONFLICT"
    const val ERROR_RATE_LIMITED = "RATE_LIMITED"
    const val ERROR_DEADLINE_EXCEEDED = "DEADLINE_EXCEEDED"
    const val ERROR_CANCELLED = "CANCELLED"
    const val ERROR_RESULT_TOO_LARGE = "RESULT_TOO_LARGE"
    const val ERROR_RESULT_EXPIRED = "RESULT_EXPIRED"
    const val ERROR_UNAVAILABLE = "UNAVAILABLE"
    const val ERROR_INTERNAL_ERROR = "INTERNAL_ERROR"

    const val CAPABILITY_ASYNC_OPERATIONS = "async_operations"
    const val CAPABILITY_CANCELLATION = "cancellation"
    const val CAPABILITY_LARGE_RESULTS = "large_results"

    const val RESULT_KIND_INLINE = "inline"
    const val RESULT_KIND_FILE = "file"

    const val PROTOCOL_VERSION_V1 = 1
    const val PROTOCOL_VERSION_V2 = 2
    const val NAMESPACE_SEPARATOR = "__"
    const val SUPPORTED_PROTOCOL_VERSION = PROTOCOL_VERSION_V2

    const val DEFAULT_REQUEST_TIMEOUT_MS = 30_000L
    const val DEFAULT_OPERATION_TIMEOUT_MS = 5 * 60_000L
    const val DEFAULT_OPERATION_POLL_MS = 1_000L
    const val MIN_OPERATION_POLL_MS = 100L
    const val MAX_OPERATION_POLL_MS = 10_000L
    const val DEFAULT_MAX_REQUEST_BYTES = 65_536
    const val DEFAULT_MAX_INLINE_RESULT_BYTES = 262_144
    const val DEFAULT_MAX_FILE_RESULT_BYTES = 16 * 1024 * 1024
    const val MIN_IDEMPOTENCY_WINDOW_SECONDS = 86_400L

    const val DEFAULT_SCHEMA_DIALECT = "https://json-schema.org/draft/2020-12/schema"
}
