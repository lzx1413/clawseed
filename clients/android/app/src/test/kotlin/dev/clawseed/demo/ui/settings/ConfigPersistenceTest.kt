package dev.clawseed.demo.ui.settings

import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ConfigPersistenceTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun gatewayRejectionDoesNotOverwriteLocalConfig() = runTest {
        val file = temporary.newFile("clawseed.toml").apply { writeText("model = \"working\"") }
        var localAccessed = false
        val failure = runCatching {
            ConfigPersistence.save("invalid = [", { Result.failure(IllegalArgumentException("Invalid TOML")) }) {
                localAccessed = true
                file
            }
        }
        assertTrue(failure.isFailure)
        assertFalse(localAccessed)
        assertEquals("model = \"working\"", file.readText())
    }

    @Test fun offlineInvalidTomlPreservesPreviousConfig() = runTest {
        val file = temporary.newFile().apply { writeText("model = \"working\"") }
        val result = runCatching { ConfigPersistence.save("invalid = [", null) { file } }
        assertTrue(result.isFailure)
        assertEquals("model = \"working\"", file.readText())
    }

    @Test fun offlineSaveReplacesFileOnlyAfterParsing() = runTest {
        val file = temporary.newFile().apply { writeText("model = \"old\"") }
        val toml = "[providers.models.\"custom:https://example.com/v1\"]\nmodel = \"new\"\n"
        ConfigPersistence.save(toml, null) { file }
        assertEquals(toml, file.readText())
        assertEquals(listOf(file.name), temporary.root.list()!!.toList())
    }

    @Test fun successfulGatewaySaveNeverWritesMaskedConfigLocally() = runTest {
        var received: String? = null
        ConfigPersistence.save("api_key = \"***\"", { received = it; Result.success(Unit) }) {
            error("Gateway owns persistence and credential hydration")
        }
        assertEquals("api_key = \"***\"", received)
    }
}
