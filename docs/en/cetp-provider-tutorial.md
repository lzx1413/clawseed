# CETP Provider Integration Tutorial: Finance App Example

Walks through implementing a CETP v1 Provider in a finance app, exposing read-only portfolio data to ClawSeed.

## 1. ContentProvider Implementation

A single ContentProvider, ~100 lines of code:

```kotlin
class FinanceToolProvider : ContentProvider() {

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle {
        // Optional: verify caller identity (package name identification is only for
        // Provider's own authorization policy, not system-level trusted authentication)
        val callingUid = Binder.getCallingUid()
        val callerPackage = context!!.packageManager
            .getPackagesForUid(callingUid)?.firstOrNull()
        if (!isAuthorized(callerPackage)) {
            return errorBundle("PERMISSION_DENIED", "Unauthorized caller")
        }

        return when (method) {
            "list_tools" -> handleListTools()
            "execute_tool" -> handleExecuteTool(extras)
            "get_provider_info" -> handleGetProviderInfo()
            else -> errorBundle("TOOL_NOT_FOUND", "Unknown method: $method")
        }
    }

    private fun handleListTools(): Bundle {
        val data = """
        {
          "tools": [
            {
              "name": "get_portfolio_holdings",
              "description": "Get current portfolio holdings with symbol, quantity, cost basis, and current price",
              "parameters": {
                "type": "object",
                "properties": {
                  "account_type": {
                    "type": "string",
                    "enum": ["stock", "fund", "all"],
                    "description": "Filter by account type"
                  }
                },
                "required": []
              }
            },
            {
              "name": "get_transactions",
              "description": "Get transaction history",
              "parameters": {
                "type": "object",
                "properties": {
                  "start_date": {
                    "type": "string",
                    "description": "Start date in ISO 8601 format (YYYY-MM-DD)"
                  },
                  "end_date": {
                    "type": "string",
                    "description": "End date in ISO 8601 format (YYYY-MM-DD)"
                  },
                  "limit": {
                    "type": "integer",
                    "default": 50,
                    "description": "Maximum number of results"
                  }
                },
                "required": ["start_date"]
              }
            }
          ]
        }
        """.trimIndent()
        return successBundle(data)
    }

    private fun handleExecuteTool(extras: Bundle?): Bundle {
        val toolName = extras?.getString("tool_name")
            ?: return errorBundle("INVALID_ARGS", "missing tool_name")
        val args = extras.getString("args") ?: "{}"

        return when (toolName) {
            "get_portfolio_holdings" -> executeGetHoldings(args)
            "get_transactions" -> executeGetTransactions(args)
            else -> errorBundle("TOOL_NOT_FOUND", "Unknown tool: $toolName")
        }
    }

    private fun executeGetHoldings(argsJson: String): Bundle {
        val holdings = portfolioRepository.getHoldings()
        return successBundle(holdings.toJsonString())
    }

    private fun executeGetTransactions(argsJson: String): Bundle {
        val transactions = transactionRepository.getTransactions()
        return successBundle(transactions.toJsonString())
    }

    private fun handleGetProviderInfo(): Bundle {
        val data = """
        {
          "provider_name": "MyFinance",
          "description": "Personal finance data",
          "scopes": [
            {"name": "holdings", "description": "Portfolio holdings"},
            {"name": "transactions", "description": "Transaction history"}
          ]
        }
        """.trimIndent()
        return successBundle(data)
    }

    private fun successBundle(data: String): Bundle {
        return Bundle().apply {
            putString("status", "success")
            putString("data", data)
        }
    }

    private fun errorBundle(code: String, message: String): Bundle {
        return Bundle().apply {
            putString("status", "error")
            putString("error_code", code)
            putString("error_message", message)
        }
    }

    private fun isAuthorized(packageName: String?): Boolean {
        // Provider implements its own authorization strategy
        return true
    }

    // Required ContentProvider methods not used by CETP
    override fun onCreate() = true
    override fun query(u: Uri, p: Array<String>?, s: String?,
                       sa: Array<String>?, so: String?) = null
    override fun getType(uri: Uri) = null
    override fun insert(uri: Uri, values: ContentValues?) = null
    override fun delete(uri: Uri, s: String?, sa: Array<String>?) = 0
    override fun update(uri: Uri, v: ContentValues?,
                        s: String?, sa: Array<String>?) = 0
}
```

### AUTH_REQUIRED Handling

When the user has not yet authorized access in the Provider app, return `AUTH_REQUIRED`:

```kotlin
private fun errorBundle(code: String, message: String,
                        resolutionHint: String? = null,
                        authorizeIntent: String? = null): Bundle {
    return Bundle().apply {
        putString("status", "error")
        putString("error_code", code)
        putString("error_message", message)
        resolutionHint?.let { putString("resolution_hint", it) }
        authorizeIntent?.let { putString("authorize_intent", it) }
    }
}

// Usage example
private fun executeGetHoldings(argsJson: String): Bundle {
    if (!userHasAuthorized()) {
        return errorBundle(
            "AUTH_REQUIRED",
            "User has not authorized this data scope",
            resolutionHint = "Please open the app to complete authorization",
            authorizeIntent = "com.example.finance.ACTION_AUTHORIZE_CLAWSEED"
        )
    }
    // ...normal data return
}
```

## 2. AndroidManifest Declaration

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.finance">

    <!-- Custom permission (for protocol entry identification and accidental call prevention only) -->
    <permission
        android:name="com.clawseed.permission.ACCESS_TOOLS"
        android:protectionLevel="normal"
        android:label="ClawSeed Tool Access"
        android:description="@string/permission_desc" />

    <application ...>

        <!-- Discovery entry point -->
        <service
            android:name=".clawseed.ToolProviderService"
            android:exported="true">
            <intent-filter>
                <action android:name="com.clawseed.action.TOOL_PROVIDER" />
            </intent-filter>
            <meta-data
                android:name="com.clawseed.tools.authority"
                android:value="com.example.finance.clawseed.tools" />
            <meta-data
                android:name="com.clawseed.tools.version"
                android:value="1" />
        </service>

        <!-- Data exchange -->
        <provider
            android:name=".clawseed.FinanceToolProvider"
            android:authorities="com.example.finance.clawseed.tools"
            android:exported="true"
            android:permission="com.clawseed.permission.ACCESS_TOOLS"
            android:initOrder="100" />

        <!-- Process keep-alive (recommended) -->
        <receiver
            android:name=".cetp.BootReceiver"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.BOOT_COMPLETED" />
            </intent-filter>
        </receiver>

    </application>
</manifest>
```

## 3. Process Keep-Alive (Recommended)

Some OEM ROMs (MIUI, ColorOS) may prevent ContentProvider processes from auto-starting, causing `ContentResolver.call()` to return `Unknown authority`.

### BootReceiver

```kotlin
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        // No action needed — simply triggering process start is sufficient.
        // ContentProvider.onCreate() is called automatically when the process starts.
    }
}
```

### initOrder

Set `android:initOrder="100"` on the `<provider>` element to ensure early initialization (needed on some OEM ROMs).

## 4. Verification

After installing the Provider App, in ClawSeed:
1. Create a new session or reconnect
2. Check the registered tools list — `finance__get_portfolio_holdings` and similar tools should appear
3. Ask the Agent to invoke a tool in chat and verify the data is returned correctly
4. Test AUTH_REQUIRED flow: revoke authorization in the Provider App, invoke again and confirm the authorization prompt appears
