package dev.clawseed.sdk.android.cetp

import android.content.ContentResolver
import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.util.Log

sealed class CetpResult<out T> {
    data class Success<T>(val data: T) : CetpResult<T>()
    data class Error(
        val errorCode: String,
        val errorMessage: String,
        val resolutionHint: String? = null,
        val authorizeIntent: String? = null,
    ) : CetpResult<Nothing>()
}

class CetpClient(private val context: Context) {

    private val contentResolver: ContentResolver
        get() = context.contentResolver

    fun listTools(authority: String): CetpResult<String> {
        val bundle = callProvider(authority, CetpConstants.METHOD_LIST_TOOLS, null)
            ?: return CetpResult.Error(
                CetpConstants.ERROR_INTERNAL_ERROR,
                "Provider call failed for $authority",
            )
        return parseResponse(bundle)
    }

    fun executeTool(
        authority: String,
        toolName: String,
        argsJson: String,
        requestId: String? = null,
    ): CetpResult<String> {
        val extras = Bundle().apply {
            putString(CetpConstants.EXTRA_TOOL_NAME, toolName)
            putString(CetpConstants.EXTRA_ARGS, argsJson)
            if (requestId != null) {
                putString(CetpConstants.EXTRA_REQUEST_ID, requestId)
            }
        }
        val bundle = callProvider(authority, CetpConstants.METHOD_EXECUTE_TOOL, extras)
            ?: return CetpResult.Error(
                CetpConstants.ERROR_INTERNAL_ERROR,
                "Provider call failed for $authority",
            )
        return parseResponse(bundle)
    }

    fun getProviderInfo(authority: String): CetpResult<String>? {
        val bundle = callProvider(authority, CetpConstants.METHOD_GET_PROVIDER_INFO, null)
            ?: return null
        return parseResponse(bundle)
    }

    private fun callProvider(authority: String, method: String, extras: Bundle?): Bundle? {
        val uri = Uri.parse("content://$authority")
        return try {
            contentResolver.call(uri, method, null, extras)
        } catch (e: IllegalArgumentException) {
            // "Unknown authority" — the provider process hasn't been started yet.
            // ContentResolver.call() does NOT auto-start the provider process.
            // Use acquireContentProviderClient() which triggers process creation
            // on most Android versions, then retry with a short delay.
            Log.w(TAG, "Authority not found, poking provider process for $authority")
            try {
                val client = contentResolver.acquireContentProviderClient(authority)
                client?.close()
            } catch (_: Exception) {}
            try {
                Thread.sleep(300)
            } catch (_: InterruptedException) {}
            try {
                contentResolver.call(uri, method, null, extras)
            } catch (e2: Exception) {
                Log.w(TAG, "Retry failed for $authority/$method: ${e2.message}")
                null
            }
        } catch (e: SecurityException) {
            Log.w(TAG, "SecurityException calling $authority/$method: ${e.message}")
            null
        } catch (e: Exception) {
            Log.w(TAG, "Exception calling $authority/$method: ${e.message}")
            null
        }
    }

    companion object {
        private const val TAG = "CetpClient"
    }

    private fun parseResponse(bundle: Bundle): CetpResult<String> {
        val status = bundle.getString(CetpConstants.BUNDLE_STATUS, "")
        return when (status) {
            CetpConstants.STATUS_SUCCESS -> {
                val data = bundle.getString(CetpConstants.BUNDLE_DATA, "")
                CetpResult.Success(data)
            }
            CetpConstants.STATUS_ERROR -> {
                CetpResult.Error(
                    errorCode = bundle.getString(CetpConstants.BUNDLE_ERROR_CODE, CetpConstants.ERROR_INTERNAL_ERROR),
                    errorMessage = bundle.getString(CetpConstants.BUNDLE_ERROR_MESSAGE, "Unknown error"),
                    resolutionHint = bundle.getString(CetpConstants.BUNDLE_RESOLUTION_HINT),
                    authorizeIntent = bundle.getString(CetpConstants.BUNDLE_AUTHORIZE_INTENT),
                )
            }
            else -> CetpResult.Error(
                CetpConstants.ERROR_INTERNAL_ERROR,
                "Unexpected status: $status",
            )
        }
    }
}
