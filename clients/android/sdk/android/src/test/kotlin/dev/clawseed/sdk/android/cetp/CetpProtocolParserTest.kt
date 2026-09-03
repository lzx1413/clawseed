package dev.clawseed.sdk.android.cetp

import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class CetpProtocolParserTest {
    private val parser = CetpProtocolParser()

    @Test
    fun advertisedVersionsPreferExplicitVersionSet() {
        assertEquals(setOf(1, 2), parser.advertisedVersions(1, "2, 1,invalid"))
        assertEquals(setOf(1), parser.advertisedVersions(1, null))
    }

    @Test
    fun negotiationParsesCapabilitiesAndLimits() {
        val session = parser.negotiation(
            """
            {
              "selected_version": 2,
              "provider_id": "com.example.finance",
              "provider_name": "Finance",
              "session_token": "token",
              "capabilities": ["async_operations", "large_results"],
              "limits": {
                "max_request_bytes": 65536,
                "max_inline_result_bytes": 262144,
                "max_concurrent_requests": 2,
                "idempotency_window_seconds": 86400
              }
            }
            """.trimIndent(),
        )

        assertNotNull(session)
        assertEquals(2, session.protocolVersion)
        assertEquals("com.example.finance", session.providerId)
        assertEquals(setOf("async_operations", "large_results"), session.capabilities)
        assertEquals(2, session.limits.maxConcurrentRequests)
    }

    @Test
    fun v2ToolParsesSecurityAnnotations() {
        val tools = parser.tools(v2ToolsJson(confirmation = "provider_policy"), "finance_abcd", 2)

        assertEquals(1, tools.size)
        val tool = tools.single()
        assertEquals("finance_abcd__create_alert", tool.namespacedName)
        assertEquals(listOf("alerts.write"), tool.scopes)
        assertEquals(ToolEffect.CREATE, tool.annotations.effect)
        assertTrue(tool.annotations.hasSideEffects)
        assertFalse(tool.annotations.destructive)
    }

    @Test
    fun v2RejectsHighRiskToolWithoutAlwaysConfirmation() {
        val json = v2ToolsJson(confirmation = "provider_policy", risk = "high")

        assertTrue(parser.tools(json, "finance", 2).isEmpty())
    }

    @Test
    fun v2RejectsUnknownSecurityEnum() {
        val json = v2ToolsJson(confirmation = "sometimes")

        assertTrue(parser.tools(json, "finance", 2).isEmpty())
    }

    @Test
    fun v1KeepsLegacyToolNamesAndParametersField() {
        val tools = parser.tools(
            """{"tools":[{"name":"Legacy.Tool","description":"Read","parameters":{"type":"object"}}]}""",
            "legacy",
            1,
        )

        assertEquals(1, tools.size)
        assertEquals("legacy__Legacy.Tool", tools.single().namespacedName)
        assertEquals(ToolEffect.READ, tools.single().annotations.effect)
    }

    @Test
    fun providerInfoParsesScopeMetadata() {
        val info = parser.providerInfo(
            """
            {
              "provider_id":"com.example.finance",
              "provider_name":"Finance",
              "description":"Portfolio data",
              "schema_dialect":"custom",
              "scopes":[{
                "name":"portfolio.read",
                "title":"Portfolio",
                "description":"Read holdings",
                "access":"read",
                "sensitivity":"financial"
              }]
            }
            """.trimIndent(),
        )

        assertNotNull(info)
        assertEquals("com.example.finance", info.providerId)
        assertEquals(ScopeSensitivity.FINANCIAL, info.scopes.single().sensitivity)
    }

    @Test
    fun strictProviderInfoRejectsUnknownSecurityEnum() {
        val info = parser.providerInfo(
            """
            {
              "provider_id":"com.example.finance",
              "provider_name":"Finance",
              "scopes":[{
                "name":"portfolio.read",
                "access":"sometimes",
                "sensitivity":"financial"
              }]
            }
            """.trimIndent(),
            strictSecurityEnums = true,
        )

        assertNull(info)
    }

    @Test
    fun operationParsesTerminalError() {
        val operation = parser.operation(
            """
            {
              "operation_id":"op_1",
              "state":"failed",
              "error":{
                "error_code":"CONFLICT",
                "error_message":"Already changed",
                "retryable":false
              }
            }
            """.trimIndent(),
        )

        assertNotNull(operation)
        assertEquals(OperationState.FAILED, operation.state)
        assertEquals("CONFLICT", operation.error?.code)
    }

    @Test
    fun malformedNegotiationIsRejected() {
        assertNull(parser.negotiation("{\"selected_version\":2}"))
    }

    private fun v2ToolsJson(
        confirmation: String,
        risk: String = "moderate",
    ): String =
        """
        {
          "revision":"1",
          "tools":[{
            "name":"create_alert",
            "description":"Create an alert",
            "input_schema":{"type":"object"},
            "output_schema":{"type":"object"},
            "scopes":["alerts.write"],
            "annotations":{
              "effect":"create",
              "risk":"$risk",
              "destructive":false,
              "idempotent":false,
              "open_world":false,
              "confirmation":"$confirmation",
              "execution":"sync"
            }
          }]
        }
        """.trimIndent()
}
