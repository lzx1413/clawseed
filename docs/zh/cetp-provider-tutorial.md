# CETP Provider 接入教程：理财 App 样例

以一个理财 App 为例，演示如何通过 CETP v1 协议向 ClawSeed 暴露只读数据工具。

## 1. ContentProvider 实现

只需实现一个 ContentProvider，约 100 行代码：

```kotlin
class FinanceToolProvider : ContentProvider() {

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle {
        // 可选：验证调用方身份（包名识别仅用于 Provider 自主授权策略）
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
              "description": "获取当前持仓信息，包含股票代码、数量、成本价和现价",
              "parameters": {
                "type": "object",
                "properties": {
                  "account_type": {
                    "type": "string",
                    "enum": ["stock", "fund", "all"],
                    "description": "按账户类型筛选"
                  }
                },
                "required": []
              }
            },
            {
              "name": "get_transactions",
              "description": "获取交易记录",
              "parameters": {
                "type": "object",
                "properties": {
                  "start_date": {
                    "type": "string",
                    "description": "起始日期，ISO 8601 格式 (YYYY-MM-DD)"
                  },
                  "end_date": {
                    "type": "string",
                    "description": "结束日期，ISO 8601 格式 (YYYY-MM-DD)"
                  },
                  "limit": {
                    "type": "integer",
                    "default": 50,
                    "description": "最大返回条数"
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
          "description": "个人理财数据",
          "scopes": [
            {"name": "holdings", "description": "持仓信息"},
            {"name": "transactions", "description": "交易记录"}
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
        // Provider 自行实现授权策略
        return true
    }

    // ContentProvider 必须方法，CETP 不使用
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

### AUTH_REQUIRED 处理

当用户未在 Provider App 中完成授权时，返回 `AUTH_REQUIRED` 错误：

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

// 使用示例
private fun executeGetHoldings(argsJson: String): Bundle {
    if (!userHasAuthorized()) {
        return errorBundle(
            "AUTH_REQUIRED",
            "用户尚未授权该数据范围",
            resolutionHint = "请打开 App 完成授权",
            authorizeIntent = "com.example.finance.ACTION_AUTHORIZE_CLAWSEED"
        )
    }
    // ...正常返回数据
}
```

## 2. AndroidManifest 声明

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.finance">

    <!-- 自定义权限（仅用于协议入口标识和减少误调用，不构成强安全边界） -->
    <permission
        android:name="com.clawseed.permission.ACCESS_TOOLS"
        android:protectionLevel="normal"
        android:label="ClawSeed Tool Access" />

    <application ...>

        <!-- 发现入口 -->
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

        <!-- 数据交换 -->
        <provider
            android:name=".clawseed.FinanceToolProvider"
            android:authorities="com.example.finance.clawseed.tools"
            android:exported="true"
            android:permission="com.clawseed.permission.ACCESS_TOOLS"
            android:initOrder="100" />

        <!-- 进程保活（推荐） -->
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

## 3. 进程保活（推荐）

部分厂商 ROM（MIUI、ColorOS）的后台管理可能阻止 ContentProvider 进程自动启动，导致 `ContentResolver.call()` 返回 `Unknown authority`。

### BootReceiver

```kotlin
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        // 无需操作——仅触发进程启动即可
        // ContentProvider.onCreate() 在进程启动时自动调用
    }
}
```

### initOrder

`<provider>` 设置 `android:initOrder="100"` 确保 Provider 优先初始化（部分厂商 ROM 需要）。

## 4. 验证

安装 Provider App 后，在 ClawSeed 中：
1. 新建会话或重新连接
2. 检查注册工具列表，应出现 `finance__get_portfolio_holdings` 等工具
3. 在聊天中让 Agent 调用工具，验证数据返回正确
4. 测试 AUTH_REQUIRED 流程：在 Provider App 中撤销授权，再次调用应返回授权提示
