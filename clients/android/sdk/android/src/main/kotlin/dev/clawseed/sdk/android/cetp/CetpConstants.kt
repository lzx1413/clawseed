package dev.clawseed.sdk.android.cetp

object CetpConstants {
    const val ACTION_TOOL_PROVIDER = "com.clawseed.action.TOOL_PROVIDER"
    const val META_AUTHORITY = "com.clawseed.tools.authority"
    const val META_VERSION = "com.clawseed.tools.version"
    const val PERMISSION_ACCESS_TOOLS = "com.clawseed.permission.ACCESS_TOOLS"

    const val METHOD_LIST_TOOLS = "list_tools"
    const val METHOD_EXECUTE_TOOL = "execute_tool"
    const val METHOD_GET_PROVIDER_INFO = "get_provider_info"

    const val BUNDLE_STATUS = "status"
    const val BUNDLE_DATA = "data"
    const val BUNDLE_ERROR_CODE = "error_code"
    const val BUNDLE_ERROR_MESSAGE = "error_message"
    const val BUNDLE_RESOLUTION_HINT = "resolution_hint"
    const val BUNDLE_AUTHORIZE_INTENT = "authorize_intent"

    const val EXTRA_TOOL_NAME = "tool_name"
    const val EXTRA_ARGS = "args"
    const val EXTRA_REQUEST_ID = "request_id"

    const val STATUS_SUCCESS = "success"
    const val STATUS_ERROR = "error"

    const val ERROR_AUTH_REQUIRED = "AUTH_REQUIRED"
    const val ERROR_PERMISSION_DENIED = "PERMISSION_DENIED"
    const val ERROR_TOOL_NOT_FOUND = "TOOL_NOT_FOUND"
    const val ERROR_INVALID_ARGS = "INVALID_ARGS"
    const val ERROR_RATE_LIMITED = "RATE_LIMITED"
    const val ERROR_INTERNAL_ERROR = "INTERNAL_ERROR"

    const val NAMESPACE_SEPARATOR = "__"
    const val SUPPORTED_PROTOCOL_VERSION = 1
}
